#!/usr/bin/env python3
"""Offline audit of Codex exec JSONL. Never executes recorded commands.

Usage is the raw whole-user-turn usage, not per-assistant usage or dollar cost.
Unknown source attribution and shell semantics require manual review.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import shlex


def shell_category(command):
    try:
        words = shlex.split(command)
        if len(words) == 3 and Path(words[0]).name in ('zsh', 'bash', 'sh') and words[1] in ('-lc', '-c'):
            words = shlex.split(words[2])
        if not words or re.search(r'[;&|<>`\n]|\$\(', ' '.join(words)):
            return 'manual_audit'
        name = Path(words[0]).name
        if name == 'cat' and len(words) == 2 and words[1].endswith('/SKILL.md'):
            return 'skill_instructions'
        if name == 'rustcodegraph' and words[1:] == ['status']:
            return 'status'
        if name in ('rg', 'grep', 'ag'):
            return 'source_search'
        if name in ('cat', 'sed', 'head', 'tail', 'less', 'more'):
            return 'source_read'
    except ValueError:
        pass
    return 'manual_audit'


def source_rows(text, fallback):
    # Only explicit file headings/locations establish attribution; never infer
    # it from Flow prose (which can mention several files on a single line).
    path = fallback
    unknown = 0
    in_unknown = False
    rows = []
    for line in text.splitlines():
        heading = re.match(r'^(?:##|####) (.+?)(?: · .*|$)', line)
        location = re.match(r'^\*\*Location:\*\* `?([^`\s]+):\d+', line)
        if heading:
            candidate = heading[1].strip('`')
            path = candidate if re.fullmatch(r'[^\s:]+\.[A-Za-z0-9]+', candidate) else None
        if location:
            path = location[1]
        numbered = re.match(r'^\s*(\d+)(?:\t| \| ?)(.*)$', line)
        if numbered:
            if path:
                rows.append((path, int(numbered[1]), hashlib.sha256(numbered[2].encode()).hexdigest()))
            elif not in_unknown:
                unknown += 1
            in_unknown = not bool(path)
        else:
            in_unknown = False
    # Unnumbered code fences cannot be reconstructed into source identities.
    unknown += len(re.findall(r'^```[^\n]*\n(?!\s*\d+(?:\t| \|))[^`]+?^```', text, re.M))
    return rows, unknown


def summarize_run(directory):
    r = {'run': directory.name, 'counts': dict.fromkeys(('explore', 'symbol_source_calls', 'mcp_file_view_calls', 'other_mcp', 'shell', 'total_tools'), 0),
         'commands': [], 'unknown_items': [], 'turn_usage': [], 'exclusion_reasons': []}
    reasons = r['exclusion_reasons']
    try:
        meta = json.loads((directory / 'meta.json').read_text())
        if not isinstance(meta, dict):
            raise ValueError('meta must be an object')
    except (OSError, ValueError):
        meta = {}
        reasons.append('invalid_meta')
    r['meta'] = meta
    if meta.get('exit_code') != 0:
        reasons.append('unsuccessful_exit')
    if not meta.get('model'):
        reasons.append('missing_model')
    seen = set()
    rows = []
    unknown = 0
    completed_turn = False
    try:
        lines = (directory / 'run.jsonl').read_text().splitlines()
    except OSError:
        lines = []
        reasons.append('missing_log')
    for number, line in enumerate(lines, 1):
        try:
            event = json.loads(line)
            if not isinstance(event, dict):
                raise ValueError('event must be object')
            kind = event.get('type')
            if kind == 'turn.failed' or kind == 'error':
                reasons.append('turn_failed')
            if kind == 'turn.completed':
                completed_turn = True
                if isinstance(event.get('usage'), dict):
                    r['turn_usage'].append(event['usage'])
            if kind != 'item.completed':
                continue
            item = event['item']
            identity = item.get('id')
            if not identity:
                reasons.append('missing_item_id')
                continue
            if identity in seen:
                reasons.append('duplicate_event')
                continue
            seen.add(identity)
            typ = item.get('type')
            counts = r['counts']
            if typ == 'command_execution':
                counts['shell'] += 1
                counts['total_tools'] += 1
                command = item.get('command', '')
                r['commands'].append({'command': command, 'category': shell_category(command)})
                if item.get('exit_code', 0) != 0:
                    reasons.append('tool_failure')
            elif typ == 'mcp_tool_call':
                counts['total_tools'] += 1
                tool = item.get('tool', '')
                args = item.get('arguments', {})
                if isinstance(args, str):
                    args = json.loads(args)
                symbol = 'name' in args or 'symbol' in args
                file_mode = 'file' in args and not symbol
                category = ('explore' if tool == 'rustcodegraph_explore' else
                            'symbol_source_calls' if tool == 'rustcodegraph_node' and symbol else
                            'mcp_file_view_calls' if tool == 'rustcodegraph_node' and file_mode else 'other_mcp')
                counts[category] += 1
                result = item.get('result') or {}
                if item.get('error') or result.get('isError') or item.get('status') == 'failed':
                    reasons.append('tool_failure')
                for block in result.get('content', []):
                    if block.get('type') == 'text':
                        found, missing = source_rows(block.get('text', ''), args.get('file') if file_mode else None)
                        rows.extend(found)
                        unknown += missing
            elif typ not in ('agent_message', 'reasoning', 'todo_list'):
                r['unknown_items'].append(item)
        except (ValueError, TypeError, KeyError, AttributeError):
            reasons.append('damaged_event')
            r['unknown_items'].append({'line': number, 'raw': line})
    if not completed_turn:
        reasons.append('incomplete_turn')
    if not sum(r['counts'][k] for k in ('explore', 'symbol_source_calls', 'mcp_file_view_calls', 'other_mcp')):
        reasons.append('no_mcp')
    r['usage_status'] = 'available' if r['turn_usage'] else 'unavailable'
    if not r['turn_usage']:
        reasons.append('usage_unavailable')
    r['source'] = {'line_occurrences': len(rows), 'unique_lines': len(set(rows)),
                   'repeated_lines': len(rows) - len(set(rows)), 'unknown_blocks': unknown,
                   'identities': [list(row) for row in sorted(set(rows))]}
    r['exclusion_reasons'] = sorted(set(reasons))
    return r


def summarize(root):
    runs = [summarize_run(p) for p in sorted(Path(root).glob('ab-wired-*')) if p.is_dir()]
    models = {r['meta'].get('model') for r in runs if r['meta'].get('model')}
    if len(models) > 1:
        for r in runs:
            r['exclusion_reasons'].append('model_mismatch')
    valid = [r for r in runs if not r['exclusion_reasons']]
    arms = {r['meta'].get('engine') for r in valid}
    summary = {}
    if {'base', 'new'} <= arms:
        for arm in ('base', 'new'):
            samples = [r for r in valid if r['meta'].get('engine') == arm]
            summary[arm] = {'samples': len(samples), 'tool_totals': {k: sum(r['counts'][k] for r in samples) for k in samples[0]['counts']}}
    return {'runs': runs, 'valid_ab_summary': summary,
            'limitations': ['Offline classification only; no sufficiency verdict.',
                            'Usage is raw whole-user-turn usage; cached input is a subset, never added again.',
                            'Source counts cover explicitly attributed numbered MCP text only; unknown blocks and shell commands require review.']}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('root', type=Path)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    args.output.write_text(json.dumps(summarize(args.root), indent=2) + '\n')


if __name__ == '__main__':
    main()
