use std::process::Command;

#[test]
fn case_003_manifest_and_actual_mcp_usage() {
    let script = r#"
import importlib.util, json, tempfile
from pathlib import Path
spec = importlib.util.spec_from_file_location('matrix', 'scripts/agent-eval/run-matrix-case.py')
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)
cases = []
for key in sorted(m.KEYS):
    c = {f: 'value' for f in m.FIELDS}
    c.update(key=key, scale=key.rsplit('-',1)[0], symbols=['start'], expected_path=['start'])
    cases.append(c)
assert m.validate_manifest(cases) == cases
for broken in (cases[:-1], [dict(c, baseline_ref='') for c in cases], [dict(c, control_query='') for c in cases]):
    try: m.validate_manifest(broken)
    except ValueError: pass
    else: raise AssertionError('invalid manifest accepted')
with tempfile.TemporaryDirectory() as d:
    p = Path(d)/'run.jsonl'
    event = {'type':'assistant', 'message':{'usage':{'input_tokens':3}, 'model':'sonnet', 'content':[{'type':'tool_use','name':'mcp__rustcodegraph__explore'}]}}
    p.write_text(json.dumps(event)+'\n'+json.dumps(event)+'\n')
    result = m.summarize(p, True)
    assert result['mcp_calls'] == 2 and len(result['turn_usage']) == 2
    event['message']['content'] = [{'type':'tool_use','name':'Read'}]
    p.write_text(json.dumps(event)+'\n')
    try: m.summarize(p, True)
    except ValueError: pass
    else: raise AssertionError('zero MCP accepted')
"#;
    let result = Command::new("python3")
        .args(["-c", script])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[cfg(unix)]
#[test]
fn case_003_real_graph_and_agent_boundary() {
    let script = r#"
import json, os, subprocess, tempfile
from pathlib import Path
with tempfile.TemporaryDirectory() as d:
    root = Path(d)
    repo = root/'source'
    repo.mkdir()
    def git(*args): return subprocess.check_output(['git','-C',str(repo),*args],text=True).strip()
    git('init','-q')
    (repo/'main.py').write_text('def finish():\n    return 1\n\ndef start():\n    return finish()\n')
    git('add','.')
    git('-c','user.name=Test','-c','user.email=test@example.com','commit','-qm','fixture')
    agent = root/'claude'
    agent.write_text('''#!/usr/bin/env python3
import json, sys
cfg = sys.argv[sys.argv.index('--mcp-config')+1]
name = 'mcp__rustcodegraph__rustcodegraph_explore' if 'rustcodegraph' in cfg else 'Read'
print(json.dumps({'type':'assistant','message':{'model':'sonnet','usage':{'input_tokens':5,'output_tokens':2},'content':[{'type':'tool_use','id':'1','name':name,'input':{}}]}}))
''')
    agent.chmod(0o755)
    cases = []
    for scale in ('small','medium','large'):
        for i in (1,2,3):
            cases.append(dict(key=f'{scale}-{i}',repo_url='https://example.com/fixture',commit=git('rev-parse','HEAD'),local_path=str(repo),scale=scale,language_framework='Python',question='How does start reach finish?',symbols=['start','finish'],expected_path=['start','finish'],baseline_ref='HEAD',control_repo=str(repo),control_query='start finish'))
    manifest = root/'manifest.json'
    manifest.write_text(json.dumps(cases))
    out = root/'out'
    env = dict(os.environ, PATH=str(root)+os.pathsep+os.environ['PATH'])
    result = subprocess.run(['python3','scripts/agent-eval/run-matrix-case.py','--manifest',str(manifest),'--case','small-1','--binary',os.environ['MATRIX_BINARY'],'--out',str(out)],env=env,capture_output=True,text=True)
    assert result.returncode == 0, result.stderr + (out/'report.json').read_text()
    report = json.loads((out/'report.json').read_text())
    assert len(report['runs']) == 4
    assert all(r['mcp_calls'] > 0 for r in report['runs'] if r['arm']=='with')
    assert json.loads((out/'graph-before.json').read_text())['nodes'] > 0
    assert json.loads((out/'graph-before.json').read_text())['nodes'] == json.loads((out/'graph-after.json').read_text())['nodes']
"#;
    let result = Command::new("python3")
        .args(["-c", script])
        .env("MATRIX_BINARY", env!("CARGO_BIN_EXE_rustcodegraph"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}
