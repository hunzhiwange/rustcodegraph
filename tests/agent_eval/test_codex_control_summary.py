import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('summary', ROOT / 'scripts/agent-eval/summarize-codex-control.py')
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)


def call(id, tool='rustcodegraph_node', arguments=None, text=''):
    return {'type': 'item.completed', 'item': {'id': id, 'type': 'mcp_tool_call', 'tool': tool, 'arguments': arguments or {}, 'result': {'content': [{'type': 'text', 'text': text}]}}}


class SummaryTests(unittest.TestCase):
    def run_events(self, events, meta=None):
        with tempfile.TemporaryDirectory() as tmp:
            p = Path(tmp) / 'ab-wired-base-1'
            p.mkdir()
            (p / 'meta.json').write_text(json.dumps(meta or {'model': 'test', 'engine': 'base', 'exit_code': 0}))
            (p / 'run.jsonl').write_text('\n'.join(json.dumps(e) for e in events))
            return m.summarize(p.parent)['runs'][0]

    def test_unique_completed(self):
        e = call('a')
        r = self.run_events([dict(e, type='item.started'), e, e])
        self.assertEqual(r['counts']['total_tools'], 1)
        self.assertIn('duplicate_event', r['exclusion_reasons'])

    def test_modes(self):
        r = self.run_events([call('a', arguments={'file': 'a.ts'}), call('b', arguments={'file': 'a.ts', 'name': 'f'}), call('c', arguments={'symbol': 'g', 'file': 'a.ts'})])
        self.assertEqual(r['counts']['mcp_file_view_calls'], 1)
        self.assertEqual(r['counts']['symbol_source_calls'], 2)

    def test_overlap(self):
        r = self.run_events([call('a', 'rustcodegraph_explore', text='#### a.ts · full\n1\tx\n2\ty'), call('b', arguments={'file': 'a.ts'}, text='2\ty\n3\tz')])
        self.assertEqual(r['source']['line_occurrences'], 4)
        self.assertEqual(r['source']['repeated_lines'], 1)

    def test_content_and_path(self):
        r = self.run_events([call(str(i), arguments={'file': p}, text=t) for i, (p, t) in enumerate([('a.ts', '1\tx'), ('b.ts', '1\tx'), ('a.ts', '1\ty')])])
        self.assertEqual(r['source']['repeated_lines'], 0)

    def test_unknown(self):
        r = self.run_events([call('a', text='```ts\n1\tx\n```'), {'type': 'item.completed', 'item': {'id': 'b', 'type': 'command_execution', 'command': 'python3 -c "print(1)"'}}])
        self.assertGreater(r['source']['unknown_blocks'], 0)
        self.assertEqual(r['commands'][0]['category'], 'manual_audit')

    def test_failed_missing_usage(self):
        r = self.run_events([{'type': 'turn.failed', 'error': {'message': 'failed'}}])
        self.assertEqual(r['usage_status'], 'unavailable')
        self.assertIn('no_mcp', r['exclusion_reasons'])
        self.assertIn('turn_failed', r['exclusion_reasons'])

    def test_mixed_models(self):
        with tempfile.TemporaryDirectory() as tmp:
            for arm, model in [('base', 'a'), ('new', 'b')]:
                p = Path(tmp) / ('ab-wired-' + arm + '-1'); p.mkdir()
                (p / 'meta.json').write_text(json.dumps({'model': model, 'engine': arm, 'exit_code': 0}))
                (p / 'run.jsonl').write_text(json.dumps(call('a')) + '\n' + json.dumps({'type': 'turn.completed', 'usage': {'input_tokens': 10, 'cached_input_tokens': 8}}))
            r = m.summarize(tmp)
            self.assertEqual(r['valid_ab_summary'], {})
            self.assertTrue(all('model_mismatch' in x['exclusion_reasons'] for x in r['runs']))

    def test_symbol_location_and_usage(self):
        usage = {'input_tokens': 10, 'cached_input_tokens': 8, 'output_tokens': 2}
        r = self.run_events([call('a', arguments={'name': 'f'}, text='## f (function)\n**Location:** a.ts:1\n### Source\n1\tx'), {'type': 'turn.completed', 'usage': usage}])
        self.assertEqual(r['source']['line_occurrences'], 1)
        self.assertEqual(r['source']['unknown_blocks'], 0)
        self.assertEqual(r['turn_usage'], [usage])
        self.assertEqual(r['exclusion_reasons'], [])

    def test_damaged_and_tool_error(self):
        e = call('a')
        e['item']['result']['isError'] = True
        r = self.run_events([e, None])
        self.assertIn('damaged_event', r['exclusion_reasons'])
        self.assertIn('tool_failure', r['exclusion_reasons'])

    def test_frozen_event(self):
        events = [json.loads(l) for l in (Path(__file__).parent / 'fixtures/codex_flow_source.jsonl').read_text().splitlines()]
        r = self.run_events(events)
        self.assertEqual(r['counts']['explore'], 1)
        self.assertGreater(r['source']['line_occurrences'], 0)


if __name__ == '__main__':
    unittest.main()
