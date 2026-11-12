#!/usr/bin/env python3
"""Real native Package import/RPC execution, loopback model only, no shared owner."""
import argparse
import hashlib
import http.server
import json
import os
import pathlib
import queue
import subprocess
import threading
import time
import uuid


def dump(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2)+'\n', encoding='utf8')


class RPC:
    def __init__(self, args, cwd, env, log):
        self.process = subprocess.Popen(args, cwd=cwd, env=env, stdin=subprocess.PIPE,
            stdout=subprocess.PIPE, stderr=log, text=True, encoding='utf8')
        self.messages = queue.Queue()
        self.events = []
        def read():
            for line in self.process.stdout:
                try: self.messages.put(json.loads(line))
                except json.JSONDecodeError: self.messages.put({'unparsed':line})
            self.messages.put({'eof':self.process.poll()})
        self.reader = threading.Thread(target=read, daemon=True)
        self.reader.start()

    def command(self, kind, timeout=100, **fields):
        identity = str(uuid.uuid4())
        self.process.stdin.write(json.dumps({'type':kind,'id':identity,**fields})+'\n')
        self.process.stdin.flush()
        deadline = time.monotonic()+timeout
        while True:
            msg = self.messages.get(timeout=max(.001,deadline-time.monotonic()))
            self.events.append(msg)
            if 'eof' in msg:
                raise RuntimeError(f'native host exited: {msg}')
            if msg.get('type')=='response' and msg.get('id')==identity:
                assert msg.get('success'),msg
                return msg.get('data')

    def close(self):
        if self.process.poll() is None:
            self.process.stdin.close()
            try: self.process.wait(timeout=12)
            except subprocess.TimeoutExpired:
                self.process.terminate()
                try: self.process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    self.process.kill(); self.process.wait(timeout=5)
        self.reader.join(timeout=2)


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--native',type=pathlib.Path,required=True)
    ap.add_argument('--native-sha256',required=True,help='Explicit trusted digest; checked before any native process starts')
    ap.add_argument('--export',type=pathlib.Path,required=True)
    ap.add_argument('--output',type=pathlib.Path,required=True,help='New isolated evidence directory')
    args = ap.parse_args()
    native, export, root = args.native.resolve(),args.export.resolve(),args.output.resolve()
    assert native.is_file() and not root.exists()
    native_digest = hashlib.sha256(native.read_bytes()).hexdigest()
    if native_digest != args.native_sha256.lower():
        raise ValueError(f'native executable digest mismatch: expected {args.native_sha256}, got {native_digest}')
    root.mkdir(parents=True)
    project, agent, sessions = [root/n for n in ('project','agent','sessions')]
    for p in (project,agent,sessions):p.mkdir()
    (project/'src').mkdir()
    (project/'Cargo.toml').write_text('[package]\nname="rust-package-smoke"\nversion="0.1.0"\nedition="2024"\n\n[workspace]\n',encoding='utf8')
    (project/'src/lib.rs').write_text('pub fn answer() -> u32 { 42 }\n',encoding='utf8')
    source = export/'rust-development'
    installed = project/'.rpi/packages/rust-development'
    requests = []
    fixture_errors = []
    steps = [
        ('read',{'path':str(installed/'skills/rust-development/SKILL.md')}),
        ('code_lens',{'check':'cargo','cwd':str(project)}),
        ('codemode',{'action':'list','limit':128}),
    ]
    class Model(http.server.BaseHTTPRequestHandler):
        def log_message(self,*a): pass
        def do_POST(self):
            try:
                req = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
                requests.append(req)
                done = sum(m.get('role')=='tool' for m in req['messages'])
                if done < len(steps):
                    name, params = steps[done]
                    delta = {'tool_calls':[{'index':0,'id':f'rust-dev-{done}','type':'function','function':{'name':name,'arguments':json.dumps(params)}}]}
                    reason = 'tool_calls'
                else: delta = {'content':'RUST_DEVELOPMENT_PACKAGE_EXECUTION_OK'}; reason='stop'
                frame = {'id':'isolated-rust-dev','object':'chat.completion.chunk','choices':[{'index':0,'delta':delta,'finish_reason':reason}]}
                body = ('data: '+json.dumps(frame)+'\n\ndata: [DONE]\n\n').encode()
                self.send_response(200);self.send_header('Content-Type','text/event-stream');self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
            except Exception as exc:
                fixture_errors.append(str(exc));self.send_error(500)
    server = http.server.ThreadingHTTPServer(('127.0.0.1',0),Model)
    threading.Thread(target=server.serve_forever,daemon=True).start()
    dump(agent/'models.json',{'providers':{'fixture':{'api':'openai-completions','apiKey':'loopback-only','baseUrl':f'http://127.0.0.1:{server.server_port}/v1','models':[{'id':'test','contextWindow':131072,'maxTokens':1024}]}}})
    dump(agent/'settings.json',{'defaultProvider':'fixture','defaultModel':'test','retry':{'enabled':False}})
    env = os.environ.copy()
    for key in list(env):
        if key.startswith(('RPI_','ANTHROPIC_','OPENAI_')) or key.upper() in ('HTTP_PROXY','HTTPS_PROXY','ALL_PROXY'):
            env.pop(key)
    env.update(RPI_CODING_AGENT_DIR=str(agent), RPI_AGENT_DIR=str(agent), RPI_OFFLINE='1', NO_PROXY='127.0.0.1,localhost')
    checks = []
    def passed(label): checks.append(label);print('PASS '+label,flush=True)
    def manage(*cmd):
        r = subprocess.run([str(native),'resource',*cmd],cwd=project,env=env,capture_output=True,text=True,encoding='utf8',timeout=60)
        assert r.returncode==0,(r.stdout,r.stderr)
        data = json.loads(r.stdout);assert data['success'],data
        return data
    log = (root/'native.log').open('w',encoding='utf8')
    rpc = None
    success = False
    try:
        for record in json.loads((export/'checksums.json').read_text(encoding='utf8')):
            assert hashlib.sha256((source/record['path']).read_bytes()).hexdigest()==record['sha256'],record['path']
        receipt = manage('install','package','rust-development','--scope','project','--path',str(source),'--version','1.0.0')
        dump(root/'install.json',receipt)
        assert receipt['data']['outcome']=='installed'
        for record in json.loads((export/'checksums.json').read_text(encoding='utf8')):
            assert hashlib.sha256((installed/record['path']).read_bytes()).hexdigest()==record['sha256'],record['path']
        inspection = manage('inspect','package','rust-development','--scope','project')
        dump(root/'inspect.json',inspection)
        passed('canonical directory imported by native resource manager; copied inventory verified')
        selection = json.loads((export/'selection.json').read_text(encoding='utf8'))
        dump(root/'selection.json',selection)
        session_id = str(uuid.uuid4())
        argv = [str(native),'--mode','rpc','--offline','--resource-config',str(root/'selection.json'),
            '--session-dir',str(sessions),'--session-id',session_id,'--provider','fixture','--model','test','--approve']
        rpc = RPC(argv,project,env,log)
        state = rpc.command('get_resource_state',version=1)
        dump(root/'loaded.json',state)
        expected = {p.name for p in (source/'extensions').glob('*.dll')}
        actual = {pathlib.PureWindowsPath(x['path']).name for x in state['loadedExtensions']}
        assert actual==expected,(actual,expected)
        assert state['package']['name']=='rust-development',state['package']
        assert not state['skillDiagnostics'],state['skillDiagnostics']
        loaded_skills = {s['name'] for s in state['loadedSkills']}
        inventory = json.loads((source/'inventory.json').read_text(encoding='utf8'))
        assert loaded_skills=={s['name'] for s in inventory['skills']},loaded_skills
        assert {'rust-router','rust-development','dioxus-knowledge-patch'} <= loaded_skills,loaded_skills
        effective = state['modelEffectiveTools']
        names = {x if isinstance(x,str) else x['name'] for x in effective}
        required = {'read','write','edit','bash','code_lens','codegraph_status','codemode','delegate_task','subagent','plan','dispatch','goal','todo','plan_mode_start','ask_user','permissions','mcp_list','webfetch','websearch'}
        assert required<=names,required-names
        passed(f'exact session loads all {len(actual)} native DLLs and {len(loaded_skills)} Skills; {len(names)} model-effective tools observed')
        rpc.command('prompt',message='Use the isolated Rust development fixture tools, then report completion.')
        deadline = time.monotonic()+120
        while time.monotonic()<deadline:
            state_run = rpc.command('get_state')
            if len(requests)>=4 and not state_run.get('isStreaming',False) and not state_run.get('isRunning',False):break
            time.sleep(.2)
        assert not fixture_errors,fixture_errors
        assert len(requests)==4,len(requests)
        tool_messages = [m for m in requests[-1]['messages'] if m.get('role')=='tool']
        assert len(tool_messages)==3,tool_messages
        results = [json.dumps(m,ensure_ascii=False) for m in tool_messages]
        assert 'Rust development Package' in results[0],results[0]
        assert 'check=cargo exit=0' in results[1],results[1]
        catalog = json.loads(tool_messages[2]['content'])
        assert catalog['protocolVersion']==1 and catalog['capabilities']['resultEnvelope']==1
        assert 'code_lens' in {t['name'] for t in catalog['tools']}
        assert {t['name'] for t in catalog['tools']}==names-{'codemode'}
        model_text = json.dumps(requests[0],ensure_ascii=False)
        assert 'RUST_DEVELOPMENT_PACKAGE_V1' in model_text
        model_names = {t['function']['name'] for t in requests[0]['tools']}
        assert names==model_names,(names-model_names,model_names-names)
        messages = rpc.command('get_messages')
        dump(root/'messages.json',messages)
        assert 'RUST_DEVELOPMENT_PACKAGE_EXECUTION_OK' in json.dumps(messages),messages
        passed('real loopback model sees package instructions/skills and exact effective tool schemas')
        passed('real native read, code_lens cargo check and codemode catalog execute successfully')
        first_host = state['hostId']
        rpc.close();dump(root/'rpc-events.json',rpc.events);rpc=None
        rpc = RPC(argv,project,env,log)
        reopened = rpc.command('get_resource_state',version=1)
        dump(root/'reopened.json',reopened)
        assert reopened['sessionId']==session_id and reopened['hostId']!=first_host
        assert reopened['modelEffectiveTools']==state['modelEffectiveTools']
        assert reopened['loadedExtensions']==state['loadedExtensions']
        passed('same native session reopens with unchanged package binding/resources after process exit')
        success = True
    finally:
        if rpc:
            rpc.close()
            dump(root/'last-rpc-events.json',rpc.events)
        log.close();server.shutdown();server.server_close()
        dump(root/'model-requests.json',requests)
        dump(root/'result.json',{'success':success,'checks':checks,'native':str(native),'nativeSha256':hashlib.sha256(native.read_bytes()).hexdigest(),'evidence':str(root),'limitations':['Loopback deterministic model, not a remote provider test','No shared owner or existing user session changed','All extensions loaded; only read/code_lens/codemode behavior executed','External SDKs, codegraph CLI, MCP servers and platform adapters are not bundled/tested']})
        print('Evidence: '+str(root),flush=True)


if __name__=='__main__': main()
