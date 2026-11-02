#!/usr/bin/env python3
"""Matched Codex/MCP control runs on disposable copies of an indexed checkout."""
import os, subprocess, time, json, pathlib, shutil
import argparse
parser = argparse.ArgumentParser(description='Run two matched Codex/MCP retrieval control arms twice.')
parser.add_argument('root', type=pathlib.Path, help='Contains excalidraw (indexed checkout), engine-base and engine-new; receives ab-wired-* logs')
parser.add_argument('--iterations', type=int, nargs='+', default=[1, 2])
parser.add_argument('--arms', choices=['base', 'new'], nargs='+', default=['base', 'new'])
parser.add_argument('--baseline-repo', type=pathlib.Path, help='Indexed baseline checkout; defaults to root/excalidraw')
parser.add_argument('--new-repo', type=pathlib.Path, help='Indexed new checkout; defaults to root/excalidraw')
args = parser.parse_args()
root = args.root.resolve()
env = os.environ.copy()
q = 'Explain how updating an element causes the Excalidraw canvas to render on screen. Trace mutateElement through triggerUpdate, triggerRender, React render, StaticCanvas and renderStaticScene using this checkout. Cite code locations, identify any gaps honestly, and do not edit files.'
q += ' Begin by calling the rustcodegraph MCP rustcodegraph_explore tool with query "mutateElement triggerUpdate triggerRender render StaticCanvas renderStaticScene". Use the returned source and follow up with rustcodegraph tools before falling back to source reads. If MCP tools are unavailable, report that explicitly.'
failed = False
for iteration in args.iterations:
    for label in args.arms:
        run = root / f'ab-wired-{label}-{iteration}'
        if (run / 'run.jsonl').exists():
            raise FileExistsError(f'Refusing to overwrite run evidence: {run}')
        run.mkdir(exist_ok=True)
        indexed_repo = (args.baseline_repo if label == 'base' else args.new_repo) or root / 'excalidraw'
        indexed_repo = indexed_repo.resolve()
        target = run / 'repo'
        target.mkdir(exist_ok=True)
        subprocess.run(['rsync', '-a', '--exclude=.git', '--exclude=.rustcodegraph', str(indexed_repo) + '/', str(target) + '/'], check=True)
        shutil.copytree(indexed_repo / '.rustcodegraph', target / '.rustcodegraph', dirs_exist_ok=True)
        binary = str(root / f'engine-{label}')
        denv = env | {'RUSTCODEGRAPH_DAEMON_INTERNAL': '1', 'RUSTCODEGRAPH_DAEMON_IDLE_TIMEOUT_MS': '1800000'}
        with open(run / 'daemon.log', 'w') as df:
            daemon = subprocess.Popen([binary, 'serve', '--mcp', '--path', str(target)], stdin=subprocess.DEVNULL, stdout=df, stderr=df, env=denv)
            try:
                for _ in range(200):
                    if (target / '.rustcodegraph/daemon.sock').exists():
                        break
                    if daemon.poll() is not None:
                        raise RuntimeError('daemon exited')
                    time.sleep(0.1)
                else:
                    raise RuntimeError('daemon not ready')
                config = {'command': binary, 'args': ['serve', '--mcp', '--path', str(target)], 'required': True, 'startup_timeout_sec': 60}
                inline = '{command=' + json.dumps(binary) + ',args=' + json.dumps(config['args']) + ',required=true,startup_timeout_sec=60}'
                cmd = ['codex', 'exec', '--ephemeral', '--ignore-user-config', '--skip-git-repo-check', '--json', '--dangerously-bypass-approvals-and-sandbox', '-C', str(target), '--model', 'gpt-6.1-sol', '-c', 'model_reasoning_effort="high"', '-c', 'mcp_servers.rustcodegraph=' + inline, q]
                started = time.monotonic()
                with open(run / 'run.jsonl', 'w') as out, open(run / 'run.err', 'w') as err:
                    result = subprocess.run(cmd, stdin=subprocess.DEVNULL, stdout=out, stderr=err, env=env, timeout=600)
                (run / 'meta.json').write_text(json.dumps({'seconds': time.monotonic() - started, 'exit_code': result.returncode, 'model': 'gpt-6.1-sol', 'effort': 'high', 'engine': label, 'iteration': iteration}, indent=2))
                print(label, iteration, result.returncode, flush=True)
                failed = failed or result.returncode != 0
            finally:
                daemon.terminate()
                try:
                    daemon.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    daemon.kill()
                    daemon.wait()

raise SystemExit(1 if failed else 0)
