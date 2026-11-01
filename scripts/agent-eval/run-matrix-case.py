#!/usr/bin/env python3
"""Run one frozen matrix case; preserve raw evidence and fail closed."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import time

FIELDS = 'key repo_url commit local_path scale language_framework question symbols expected_path baseline_ref control_repo control_query'.split()
KEYS = {f'{scale}-{i}' for scale in ('small', 'medium', 'large') for i in range(1, 4)}


def validate_manifest(data):
    if not isinstance(data, list) or len(data) != 9:
        raise ValueError('manifest must contain nine cases')
    if {c.get('key') for c in data} != KEYS:
        raise ValueError('case keys must be small-1 through large-3')
    for case in data:
        for field in FIELDS:
            if not case.get(field):
                raise ValueError(f'{case.get("key")}: missing {field}')
        for field in ('symbols', 'expected_path'):
            if not isinstance(case[field], list) or not all(isinstance(s, str) and s.strip() for s in case[field]):
                raise ValueError(f'{field} must be a nonempty string list')
        if case['scale'] != case['key'].rsplit('-', 1)[0]:
            raise ValueError('scale does not match key')
    return data


def summarize(path, require_mcp):
    calls, usage, models, durations = [], [], [], []
    for line in Path(path).read_text().splitlines():
        event = json.loads(line)
        if event.get('type') == 'result' and 'duration_ms' in event:
            durations.append(event['duration_ms'])
        if event.get('type') != 'assistant':
            continue
        message = event.get('message', {})
        if message.get('usage'):
            usage.append(message['usage'])
        if message.get('model'):
            models.append(message['model'])
        calls.extend(c['name'] for c in message.get('content', []) if c.get('type') == 'tool_use')
    mcp = sum('rustcodegraph' in c for c in calls)
    if require_mcp and not mcp:
        raise ValueError('with branch has zero actual rustcodegraph MCP calls')
    if not usage:
        raise ValueError('no assistant turn usage')
    return dict(total_calls=len(calls), Read=calls.count('Read'), Grep=calls.count('Grep'),
                mcp_calls=mcp, turn_usage=usage, models=models, duration_ms=durations)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('manifest', 'case', 'binary', 'out'):
        parser.add_argument('--' + name, required=True)
    args = parser.parse_args()
    cases = validate_manifest(json.loads(Path(args.manifest).read_text()))
    case = next(c for c in cases if c['key'] == args.case)
    binary, out = Path(args.binary), Path(args.out)
    if not binary.is_absolute() or not out.is_absolute():
        raise ValueError('binary and out must be absolute paths')
    # Never mutate a user checkout: copy the pinned source to an owned directory.
    out.mkdir(parents=True, exist_ok=False)
    report = dict(case=case, model='sonnet', effort='high', commands=[], runs=[])

    def run(command, label, env=None):
        start = time.monotonic()
        with (out / (label + '.stdout')).open('w') as stdout, (out / (label + '.stderr')).open('w') as stderr:
            result = subprocess.run(list(map(str, command)), stdout=stdout, stderr=stderr, env=env)
        report['commands'].append(dict(command=list(map(str, command)), seconds=time.monotonic()-start, exit_code=result.returncode))
        if result.returncode:
            raise RuntimeError(f'{label} failed: {result.returncode}')
        return (out / (label + '.stdout')).read_text()

    try:
        actual = subprocess.check_output(['git', '-C', case['local_path'], 'rev-parse', 'HEAD'], text=True).strip()
        expected = subprocess.check_output(['git', '-C', case['local_path'], 'rev-parse', case['commit'] + '^{commit}'], text=True).strip()
        if actual != expected:
            raise ValueError('local checkout does not match pinned commit')
        repo = out / 'repo'
        run(['git', 'clone', '--no-hardlinks', case['local_path'], repo], 'clone')
        run(['git', '-C', repo, 'checkout', '--detach', expected], 'checkout')
        # Indexing is explicitly authorized only for the owned acceptance copy.
        run([binary, 'init', '-i', repo], 'index-before')
        import sqlite3

        def snapshot(label):
            with sqlite3.connect(repo / '.rustcodegraph/rustcodegraph.db') as db:
                count = db.execute('select count(*) from nodes').fetchone()[0]
                db.row_factory = sqlite3.Row
                samples = [dict(row) for row in db.execute("select * from edges where provenance='heuristic' limit 20")]
            (out / (label + '.json')).write_text(json.dumps(dict(nodes=count, heuristic_samples=samples), indent=2))
            return count

        before = snapshot('graph-before')
        for i, symbol in enumerate(case['symbols']):
            run([binary, 'agent-eval', 'probe-node', repo, symbol, 'code'], f'node-{i}')
        flow = run([binary, 'agent-eval', 'probe-explore', repo, *case['symbols']], 'explore')
        # Preserve ordered path evidence separately; missing endpoints cannot pass.
        section = flow.split('## Flow ', 1)[-1].split('\n## ', 1)[0]
        def matches(line):
            cursor = 0
            for symbol in case['expected_path']:
                cursor = line.find(symbol + ' (', cursor)
                if cursor < 0:
                    return False
                cursor += len(symbol)
            return '-->' in line
        if not any(matches(line) for line in section.splitlines()):
            raise ValueError('expected connected path missing from Flow section')
        run([binary, 'index', repo], 'index-after')
        if snapshot('graph-after') != before:
            raise ValueError('node count changed after rebuild')
        env = dict(os.environ, RUSTCODEGRAPH_BIN=str(binary), MODEL='sonnet', EFFORT='high',
                   RUSTCODEGRAPH_DAEMON_INTERNAL='1', RUSTCODEGRAPH_DAEMON_IDLE_TIMEOUT_MS='1800000')
        with (out / 'daemon.log').open('w') as log:
            daemon = subprocess.Popen([str(binary), 'serve', '--mcp', '--path', str(repo)], stdin=subprocess.DEVNULL, stdout=log, stderr=log, env=env)
            try:
                deadline = time.monotonic() + 30
                def ready():
                    try:
                        info = json.loads((repo / '.rustcodegraph/daemon.pid').read_text())
                        return info['pid'] == daemon.pid and (os.name == 'nt' or Path(info['socketPath']).exists())
                    except (OSError, ValueError, KeyError):
                        return False
                while not ready():
                    if daemon.poll() is not None or time.monotonic() > deadline:
                        raise RuntimeError('owned daemon failed to become ready')
                    time.sleep(.1)
                for iteration in (1, 2):
                    folder = out / f'round-{iteration}'
                    run(['bash', Path(__file__).with_name('run-all.sh'), repo, case['question']],
                        f'round-{iteration}', dict(env, AGENT_EVAL_OUT=str(folder)))
                    for arm in ('with', 'without'):
                        report['runs'].append(dict(round=iteration, arm=arm, **summarize(folder / f'run-headless-{arm}.jsonl', arm == 'with')))
            finally:
                if daemon.poll() is None:
                    daemon.terminate()
                    try:
                        daemon.wait(timeout=10)
                    except subprocess.TimeoutExpired:
                        daemon.kill()
                        daemon.wait()
        report['status'] = 'pending-review'
        report['remaining'] = 'Review heuristic samples and baseline/control regression evidence before acceptance'
    except Exception as error:
        report['status'] = 'failed'
        report['error'] = str(error)
        raise
    finally:
        (out / 'report.json').write_text(json.dumps(report, indent=2))


if __name__ == '__main__':
    main()
