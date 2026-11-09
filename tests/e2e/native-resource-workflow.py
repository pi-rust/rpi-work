"""Opt-in REAL native + Work typed HTTP acceptance. No PATH runtime/global data.
Build: cargo build --locked -p work-node-service --example resource_workflow
Run with RPI_RESOURCE_NATIVE_BIN, WORK_CDE_HOST and optional WORK_CDE_ASSETS.
All evidence/provider/session/resource/browser data lives under WORK_CDE_BUILD.
"""
import hashlib, http.server, json, os, pathlib, shutil, subprocess, threading, time, urllib.request, urllib.error, http.cookiejar
BUILD = pathlib.Path(os.environ['WORK_CDE_BUILD']).resolve()
assert 'rpi-cde-build' in str(BUILD) and BUILD.is_absolute()
ROOT = BUILD / ('acceptance-' + str(time.time_ns())); ROOT.mkdir(parents=True)
BIN = pathlib.Path(os.environ['RPI_RESOURCE_NATIVE_BIN']); HOST = pathlib.Path(os.environ['WORK_CDE_HOST'])
assert BIN.is_absolute() and HOST.is_absolute()
assert hashlib.sha256(BIN.read_bytes()).hexdigest() == '77a9f89d61aadab02f9799ac6cd69fb194258f7369f195d553e8733f29ded4dc'
requests, checks = [], []
succeeded, failure = False, None
busy_entered, busy_release = threading.Event(), threading.Event()
class Model(http.server.BaseHTTPRequestHandler):
    def log_message(self,*a): pass
    def do_POST(self):
        req = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        requests.append(req)
        text = json.dumps(req,ensure_ascii=False)
        if 'CDE_BUSY' in text: busy_entered.set(); busy_release.wait(30)
        last_user = next((m.get('content','') for m in reversed(req['messages']) if m['role']=='user'),'')
        if isinstance(last_user,list): last_user = '\n'.join(c.get('text','') for c in last_user)
        # A real native write tool edits the SAME managed draft source.
        marker = 'MODEL_EDIT_DRAFT:'
        tool_done = any(m['role']=='tool' for m in req['messages'])
        if marker in last_user and not tool_done:
            path = last_user.split(marker,1)[1].strip()
            delta = {'tool_calls':[{'index':0,'id':'cde-write-draft','type':'function','function':{'name':'write','arguments':json.dumps({'path':path,'content':'# CDE_MODEL_EDIT\nActual developer native tool changed these source bytes.\n'})}}]}
            reason='tool_calls'
        else: delta={'content':'CDE successful real loopback reply'}; reason='stop'
        frame={'id':'cde-fixture','object':'chat.completion.chunk','choices':[{'index':0,'delta':delta,'finish_reason':reason}]}
        body=('data: '+json.dumps(frame)+'\n\ndata: [DONE]\n\n').encode()
        self.send_response(200); self.send_header('Content-Type','text/event-stream'); self.send_header('Content-Length',str(len(body))); self.end_headers(); self.wfile.write(body)
provider=http.server.ThreadingHTTPServer(('127.0.0.1',0),Model)
threading.Thread(target=provider.serve_forever,daemon=True).start()
agent=ROOT/'agent'; agent.mkdir()
(agent/'models.json').write_text(json.dumps({'providers':{'fixture':{'api':'openai-completions','apiKey':'loopback-only','baseUrl':f'http://127.0.0.1:{provider.server_port}/v1','models':[{'id':'test','contextWindow':131072,'maxTokens':256}]}}}),encoding='utf8')
(agent/'settings.json').write_text(json.dumps({'defaultProvider':'fixture','defaultModel':'test','retry':{'enabled':False}}),encoding='utf8')
env=os.environ.copy()
for k in ['ANTHROPIC_API_KEY','ANTHROPIC_AUTH_TOKEN','ANTHROPIC_BASE_URL','OPENAI_API_KEY','HTTP_PROXY','HTTPS_PROXY','ALL_PROXY']: env.pop(k,None)
env.update(WORK_CDE_TEST_ROOT=str(ROOT),RPI_CODING_AGENT_DIR=str(agent),NO_PROXY='127.0.0.1,localhost',TEMP=str(ROOT),TMP=str(ROOT))
log=(ROOT/'host.log').open('w',encoding='utf8')
p=subprocess.Popen([str(HOST)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=log,text=True,encoding='utf8',env=env)
info=json.loads(p.stdout.readline()); base=info['url']; project=info['projectId']
jar=http.cookiejar.CookieJar(); opener=urllib.request.build_opener(urllib.request.HTTPCookieProcessor(jar),urllib.request.ProxyHandler({}))
def api(path,body=None,expected=200,headers=None,anonymous=False):
    req=urllib.request.Request(base+'/api/v1/'+path,data=None if body is None else json.dumps(body).encode(),headers={'Content-Type':'application/json','x-rpi-web':'1',**(headers or {})})
    try:
        with (urllib.request.build_opener(urllib.request.ProxyHandler({})) if anonymous else opener).open(req,timeout=140) as r: code=r.status; value=json.load(r)
    except urllib.error.HTTPError as e: code=e.code; value=json.load(e)
    assert code==expected,(path,code,value)
    return value
def passed(label): checks.append(label); print('PASS',label,flush=True)
def author(body): return api(f'projects/{project}/authoring',body)
def state(id):return api(f'sessions/{id}/resources')
def release():
    report=api(f'projects/{project}/release-idle-resource-hosts',{'scope':'project'}); assert not report['blocked'],report
    return report
def wait_idle(id):
    for _ in range(150):
        s=api(f'sessions/{id}/commands',{'type':'get_state'})['data']
        if not s.get('isStreaming',False) and not s.get('isRunning',False):
            try: author({'action':'read'});return
            except AssertionError: pass
        time.sleep(.1)
    raise AssertionError('idle deadline')
def selection_from(st):
    d=st['configuration']['desired']
    return {'mainPackage':d['mainPackage']['reference'],'packageInstructions':d['packageInstructions'],'skills':[r['reference'] for r in d['skills']],'extensions':[r['reference'] for r in d['extensions']],'tools':d['tools'],'supplementaryInstructions':d['supplementaryInstructions']}
try:
    api('auth',{'password':'loopback-cde-only'})
    api(f'projects/{project}/authoring',{'action':'read'},expected=401,anonymous=True)
    api(f'projects/{project}/authoring',{'action':'read'},expected=403,headers={'Origin':'https://foreign.invalid'})
    baseid=api('sessions',{'projectId':project})['sessionId']; st=state(baseid)
    assert st['readiness']=='verified' and st['executable'] is True,st
    assert st['configuration']['revision']==1 and st['actual']['loadedSkills']
    api(f'sessions/{baseid}/commands',{'type':'prompt','message':'CDE_BASE_FIRST_TURN'})
    wait_idle(baseid); assert any('RPI Work base' in json.dumps(r) for r in requests)
    passed('real shipped base selected/loaded/verified BEFORE first loopback request; authorization preserved')
    release()
    devid=api(f'projects/{project}/resource-sessions',{'developer':True,'selection':None})['sessionId']
    assert 'rpi-work-developer' in json.dumps(state(devid)['configuration'])
    draft=author({'action':'create','id':'cde-package','kind':'package'})['drafts'][0]
    sources=draft['sources'].copy(); sources['prompts/system.md']='# CDE_PUBLISHED_INSTRUCTION\nAlways include CDE_PUBLISHED_BEHAVIOR when describing your purpose.\n'
    draft=author({'action':'save','id':draft['id'],'revision':draft['revision'],'digest':draft['digest'],'sources':sources})['drafts'][0]
    assert author({'action':'read'})['drafts'][0]['sources']==sources
    stale=draft.copy()
    api(f'sessions/{devid}/commands',{'type':'prompt','message':'MODEL_EDIT_DRAFT:'+str(pathlib.Path(draft['path'])/'prompts/system.md')})
    wait_idle(devid)
    changed=author({'action':'read'})['drafts'][0]
    assert 'CDE_MODEL_EDIT' in changed['sources']['prompts/system.md'] and changed['digest']!=stale['digest'],changed
    api(f'projects/{project}/authoring',{'action':'save','id':draft['id'],'revision':stale['revision'],'digest':stale['digest'],'sources':stale['sources']},expected=409)
    assert author({'action':'read'})['drafts'][0]['digest']==changed['digest']
    passed('ordinary developer Package native write edits managed files; readback invalidates stale facts; stale save does not erase model edit')
    draft=author({'action':'save','id':changed['id'],'revision':changed['revision'],'digest':changed['digest'],'sources':sources})['drafts'][0]
    draft=author({'action':'validate','id':draft['id'],'revision':draft['revision'],'digest':draft['digest']})['drafts'][0]
    assert draft['validation']['valid']
    # In-use native leases are authoritative; no invisible auto-stop.
    result=api(f'projects/{project}/native-resources',{'operation':'install','scope':'project','kind':'package','name':'in-use-proof','source':draft['path'],'version':'1.0.0','trusted':True})
    assert result['success'] is False,result
    passed('in-use native install refuses until explicit owned idle release')
    release()
    draft=author({'action':'test','id':draft['id'],'revision':draft['revision'],'digest':draft['digest']})['drafts'][0]; test=draft['tests'][-1]
    assert test['readback']['loadedSkills'] and test['evidence'] is None
    api(f'projects/{project}/authoring',{'action':'publish','id':draft['id'],'revision':draft['revision'],'digest':draft['digest'],'test_id':test['id'],'version':'1.0.0'},expected=409)
    draft=author({'action':'executeTest','id':draft['id'],'test_id':test['id'],'prompt':'CDE_TEST_EXECUTION'})['drafts'][0]
    test=draft['tests'][-1]; assert test['evidence']['outcome']=='completed'
    draft=author({'action':'publish','id':draft['id'],'revision':draft['revision'],'digest':draft['digest'],'test_id':test['id'],'version':'1.0.0'})['drafts'][0]
    release_record=draft['releases'][-1]
    assert pathlib.Path(release_record['source']).is_dir()
    passed('immutable SOURCE test creates real exact session; publication rejects schema-only evidence then succeeds after native Completed settlement')
    # Later SOURCE edits cannot mutate old test or local release.
    snapshot_before={str(q.relative_to(test['snapshot'])):q.read_bytes() for q in pathlib.Path(test['snapshot']).rglob('*') if q.is_file()}
    pathlib.Path(draft['path'],'prompts/system.md').write_text('# LATER_DRAFT_ONLY\n',encoding='utf8')
    later=author({'action':'read'})['drafts'][0];assert later['digest']!=test['digest'] and later['validation'] is None
    assert snapshot_before=={str(q.relative_to(test['snapshot'])):q.read_bytes() for q in pathlib.Path(test['snapshot']).rglob('*') if q.is_file()}
    release()
    install=api(f'projects/{project}/native-resources',{'operation':'install','scope':'project','kind':'package','name':'cde-published-reuse','source':release_record['source'],'version':'1.0.0','trusted':True});assert install['success'],install
    catalog=api(f'projects/{project}/native-resources'); ref=next(r['reference'] for r in catalog['resources'] if r['name']=='cde-published-reuse')
    selected={'mainPackage':ref,'packageInstructions':['prompts/system.md'],'skills':[],'extensions':[],'tools':{'mode':'none'},'supplementaryInstructions':'CDE_SUPPLEMENT_IMMUTABLE'}
    reused=api(f'projects/{project}/resource-sessions',{'developer':False,'selection':selected})['sessionId']
    before=len(requests);api(f'sessions/{reused}/commands',{'type':'prompt','message':'CDE_REUSE_REQUEST'});wait_idle(reused)
    req=requests[before]; assert 'CDE_PUBLISHED_INSTRUCTION' in json.dumps(req) and 'LATER_DRAFT_ONLY' not in json.dumps(req) and not req.get('tools')
    assert 'CDE_SUPPLEMENT_IMMUTABLE' in json.dumps(req)
    passed('published immutable Package reused by ordinary exact session; actual model input changes and tools none sends zero schemas')
    op={'expectedRevision':1,'operationId':'work-cas-two-clients','target':{**selected,'tools':{'mode':'only','names':['read']},'supplementaryInstructions':'CDE_SECOND_IMMUTABLE'},'supersedesOperationId':None}
    result=api(f'sessions/{reused}/resources',op);assert result['status']=='applied' and result['resultRevision']==2,result
    assert api(f'sessions/{reused}/resources',op)['resultRevision']==2
    api(f'sessions/{reused}/resources',{**op,'operationId':'stale-other-client'},expected=409)
    assert state(baseid)['configuration']['revision']==1
    passed('same-project sessions independent; idle same-ID CAS apply/idempotent retry/stale two-client rejection')
    api(f'sessions/{reused}/commands',{'type':'prompt','message':'CDE_BUSY'})
    assert busy_entered.wait(15),'provider did not receive busy prompt'
    api(f'sessions/{reused}/resources',{**op,'expectedRevision':2,'operationId':'must-reject-active-run'},expected=409)
    report=api(f'projects/{project}/release-idle-resource-hosts',{'scope':'global'})
    assert report['blocked'],report
    busy_release.set(); wait_idle(reused)
    assert state(reused)['configuration']['revision']==2
    passed('actual in-flight loopback run refuses resource apply; global idle release reports blocked owner without stopping active work')
    # Standalone Skill SOURCE is a DIRECTORY; native selectable.skills is a file.
    skill=next(d for d in author({'action':'create','id':'cde-skill','kind':'skill'})['drafts'] if d['id']=='cde-skill')
    skill_sources=skill['sources'].copy();skill_sources['SKILL.md']='---\nname: cde-skill\ndescription: CDE_SKILL_VISIBLE_BEHAVIOR for authored skill acceptance.\n---\nReport observed project facts honestly.\n'
    skill=next(d for d in author({'action':'save','id':skill['id'],'revision':skill['revision'],'digest':skill['digest'],'sources':skill_sources})['drafts'] if d['id']=='cde-skill')
    skill=next(d for d in author({'action':'validate','id':skill['id'],'revision':skill['revision'],'digest':skill['digest']})['drafts'] if d['id']=='cde-skill');assert skill['validation']['valid']
    release()
    skill=next(d for d in author({'action':'test','id':skill['id'],'revision':skill['revision'],'digest':skill['digest']})['drafts'] if d['id']=='cde-skill');skilltest=skill['tests'][-1]
    assert any('cde-skill' in s['path'] or 'work-' in s['path'] for s in skilltest['readback']['loadedSkills']),skilltest
    skill=next(d for d in author({'action':'executeTest','id':skill['id'],'test_id':skilltest['id'],'prompt':'CDE_SKILL_EXECUTION'})['drafts'] if d['id']=='cde-skill')
    assert skill['tests'][-1]['evidence']['outcome']=='completed'
    skill=next(d for d in author({'action':'publish','id':skill['id'],'revision':skill['revision'],'digest':skill['digest'],'test_id':skilltest['id'],'version':'1.0.0'})['drafts'] if d['id']=='cde-skill')
    release()
    catalog=api(f'projects/{project}/native-resources');tested_skill=next(r['reference'] for r in catalog['resources'] if r['name']=='cde-skill')
    removed=api(f'projects/{project}/native-resources',{'operation':'remove','reference':tested_skill});assert removed['success'],removed
    receipt=api(f'projects/{project}/native-resources',{'operation':'install','scope':'project','kind':'skill','name':'cde-skill','source':skill['releases'][-1]['source'],'version':'1.0.0','trusted':True});assert receipt['success'],receipt
    catalog=api(f'projects/{project}/native-resources');skillref=next(r['reference'] for r in catalog['resources'] if r['name']=='cde-skill')
    skill_session=api(f'projects/{project}/resource-sessions',{'developer':False,'selection':{**selected,'skills':[skillref],'tools':{'mode':'only','names':['read']}}})['sessionId']
    skillstate=state(skill_session); assert skillstate['configuration']['desired']['skills'][0]['reference']==skillref
    before=len(requests);api(f'sessions/{skill_session}/commands',{'type':'prompt','message':'CDE_SKILL_REUSE_REQUEST'});wait_idle(skill_session)
    assert 'CDE_SKILL_VISIBLE_BEHAVIOR' in json.dumps(requests[before]),requests[before]
    passed('authored standalone Skill DIRECTORY installs, tests with real Completed, publishes and reuses selectable SKILL.md with actual model-input evidence')
    # Persisted authoring survives a complete owner restart with same ID/history.
    (ROOT/'before-restart.json').write_text(json.dumps({'base':baseid,'developer':devid,'reused':reused,'draft':later},indent=2),encoding='utf8')
    p.stdin.write('stop\n');p.stdin.flush();p.wait(timeout=30)
    p=subprocess.Popen([str(HOST)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=log,text=True,encoding='utf8',env=env)
    info=json.loads(p.stdout.readline());base=info['url'];api('auth',{'password':'loopback-cde-only'})
    api(f'sessions/{reused}/open',{})
    restored=state(reused);assert restored['configuration']['revision']==2 and restored['readiness']=='verified',restored
    snapshot=api(f'sessions/{reused}/snapshot');assert 'CDE_REUSE_REQUEST' in json.dumps(snapshot)
    assert author({'action':'read'})['drafts'][0]['digest']==later['digest']
    passed('same-ID node restart preserves exact selection, native history and durable project drafts')
    (ROOT/'browser-context.json').write_text(json.dumps({'url':base,'projectId':project,'sessionId':reused,'developerId':devid,'draftId':draft['id']}),encoding='utf8')
    if os.environ.get('WORK_CDE_BROWSER_SCRIPT'):
        runenv=env.copy();runenv['WORK_CDE_CONTEXT']=str(ROOT/'browser-context.json');runenv['WORK_CDE_EVIDENCE']=str(ROOT)
        subprocess.run(['node',os.environ['WORK_CDE_BROWSER_SCRIPT']],env=runenv,check=True)
    shutil.rmtree(later['path'])
    api(f'projects/{project}/authoring',{'action':'read'},expected=500)
    assert state(reused)['readiness']=='verified'
    assert snapshot_before=={str(q.relative_to(test['snapshot'])):q.read_bytes() for q in pathlib.Path(test['snapshot']).rglob('*') if q.is_file()}
    passed('deleting original draft source never resets store or mutates old test snapshot/installed ordinary session')
    succeeded=True
except BaseException as error:
    failure=repr(error)
    raise
finally:
    busy_release.set()
    if p.poll() is None:
        p.stdin.write('stop\n');p.stdin.flush()
        try:p.wait(timeout=30)
        except subprocess.TimeoutExpired:p.kill();p.wait()
    provider.shutdown();log.close()
    (ROOT/'model-requests.json').write_text(json.dumps(requests,indent=2,ensure_ascii=False),encoding='utf8')
    (ROOT/'results.json').write_text(json.dumps({'status':'passed' if succeeded else 'failed','failure':failure,'checks':checks,'root':str(ROOT)},indent=2),encoding='utf8')
    print('EVIDENCE',ROOT,flush=True)
