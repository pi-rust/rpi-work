// Real installed rpi loads the built cdylib through its existing plugin ABI.
// Isolated agent/project/data directories; the node's execution binary is a
// protocol fixture. No user registry, current node or external model is used.
const {spawn}=require('node:child_process');
const fs=require('node:fs');const path=require('node:path');const os=require('node:os');
const http=require('node:http');
const readline=require('node:readline');const assert=require('node:assert/strict');
const {randomBytes}=require('node:crypto');
function bounded(p,ms=25000){return Promise.race([p,new Promise((_,reject)=>{const t=setTimeout(()=>reject(new Error('Native extension timeout')),ms);t.unref()})]);}
(async()=>{
    const dll=path.resolve(process.env.RPI_HTTP_EXTENSION||'../rpi-package-work-components/target/debug/rpi_http_extension.dll');
    assert(fs.existsSync(dll),'Build rpi-http-extension first');
    const root=fs.mkdtempSync(path.join(os.tmpdir(),'rpi-native-http-'));
    const agent=path.join(root,'agent'),project=path.join(root,'project'),data=path.join(root,'data');
    fs.mkdirSync(agent);fs.mkdirSync(project);
    let modelCalls=0;const model=http.createServer((req,res)=>{modelCalls++;res.writeHead(500);res.end();});
    await new Promise(resolve=>model.listen(0,'127.0.0.1',resolve));
    const modelFree=process.env.RPI_NATIVE_MODEL_FREE==='1';
    const capability=randomBytes(32).toString('hex');
    const env={...process.env,RPI_CODING_AGENT_DIR:agent,RPI_WORK_RPI_BIN:path.resolve('target/debug/work-server-rpc-fixture.exe'),RPI_HTTP_PASSWORD:'native-fixture-only'};
    delete env.RPI_NODE_CONTROL_TOKEN;
    delete env.RPI_NODE_CONTROL_STDIN;
    // The service bootstrap check must not inherit credentials from the shell.
    if(modelFree){for(const key of Object.keys(env)){if(/API_KEY|AUTH_TOKEN|ACCESS_TOKEN|BEARER_TOKEN/i.test(key))delete env[key];}}
    else env.OPENAI_API_KEY='fixture-bootstrap-only';
    for(const key of ['RPI_WORK_HTTP_PASSWORD','RPI_WORK_HTTP_PUBLIC_URL','RPI_WORK_HTTP_ASSETS','RPI_WORK_HTTP_WEB_PORT','RPI_WORK_DATA_DIR'])delete env[key];
    const args=['--extension',dll,'--http','--node-control-stdin','--bind','127.0.0.1','--port','0','--node-data',data,'--http-shutdown-stdin','--no-session','--no-skills','--no-prompt-templates','--no-context-files'];
    if(!modelFree)args.push('--provider','openai','--model','gpt-6-astra','--base-url',`http://127.0.0.1:${model.address().port}/v1`);
    const child=spawn(process.env.RPI_NATIVE_BIN||'rpi',args,{env,cwd:project,windowsHide:true,stdio:['pipe','pipe','pipe']});
    child.stdin.write(JSON.stringify({capability})+'\n');
    let stdout='',stderr='';child.stdout.on('data',b=>stdout+=b);child.stderr.on('data',b=>stderr+=b);
    const exited=new Promise(resolve=>child.once('exit',resolve));
    const ready=new Promise((resolve,reject)=>{
        child.once('error',reject);child.once('exit',code=>reject(new Error(`rpi exited ${code}: ${stderr}\n${stdout}`)));
        readline.createInterface({input:child.stdout}).on('line',line=>{try{const value=JSON.parse(line);if(value.ready===true)resolve(value)}catch{}});
    });
    try{
        const node=await bounded(ready);
        const health=await (await fetch(node.address+'api/v1/health')).json();assert.equal(health.ok,true);assert.equal(health.nodeId,node.nodeId);
        assert.equal((await fetch(node.address+'api/v1/components')).status,401);
        const login=await fetch(node.address+'api/v1/auth',{method:'POST',headers:{'content-type':'application/json','x-rpi-web':'1'},body:JSON.stringify({password:'native-fixture-only'})});
        assert.equal(login.status,200);const cookie=login.headers.get('set-cookie').split(';')[0];
        const catalog=await (await fetch(node.address+'api/v1/components',{headers:{cookie}})).json();assert(catalog.some(c=>c.manifest.id==='rpi-http'));
        const status=await (await fetch(node.address+'api/v1/components/rpi-http/call',{method:'POST',headers:{cookie,'content-type':'application/json','x-rpi-web':'1'},body:JSON.stringify({action:'status',arguments:{}})})).json();
        assert.equal(status.listening,true);assert.equal(status.canConfigure,false);assert(!('password'in status));
        // The private loopback plane is independently authenticated and keeps
        // the same owner alive when public sharing is revoked.
        assert(node.controlAddress?.startsWith('http://127.0.0.1:'));
        assert.equal((await fetch(node.controlAddress+'api/v1/health')).status,401);
        const privateHeaders={'x-rpi-node-control':capability,'content-type':'application/json','x-rpi-web':'1'};
        const privateCall=async(action,args={})=>{
            const response=await fetch(node.controlAddress+'api/v1/components/rpi-http/call',{method:'POST',headers:privateHeaders,body:JSON.stringify({action,arguments:args})});
            assert.equal(response.status,200);return response.json();
        };
        assert.equal((await privateCall('status')).canConfigure,true);
        assert.equal((await fetch(node.address+'api/v1/components',{headers:{'x-rpi-node-control':capability}})).status,401);
        assert.equal((await privateCall('set-enabled',{enabled:false})).enabled,false);
        assert.equal((await fetch(node.address+'api/v1/components',{headers:{cookie}})).status,403);
        assert.equal((await (await fetch(node.controlAddress+'api/v1/health',{headers:privateHeaders})).json()).nodeId,node.nodeId);
        assert.equal((await fetch(node.controlAddress+'api/v1/projects',{headers:privateHeaders})).status,200);
        assert.equal((await privateCall('set-enabled',{enabled:true})).enabled,true);
        assert.equal((await fetch(node.address+'api/v1/components',{headers:{cookie}})).status,401,'Old public session must stay revoked');
        const id=fs.readFileSync(path.join(data,'node-id'),'utf8').trim();assert.equal(id,node.nodeId);
        child.stdin.end();assert.equal(await bounded(exited),0,stderr);
        assert.equal(await fetch(node.address+'api/v1/health').then(()=>true,()=>false),false);
        assert.equal(await fetch(node.controlAddress+'api/v1/health',{headers:privateHeaders}).then(()=>true,()=>false),false);
        assert(!stdout.includes(capability)&&!stderr.includes(capability),'Private capability must not enter output');
        fs.mkdirSync('artifacts/component-panels',{recursive:true});
        assert(!fs.existsSync(path.join(agent,'sessions'))||fs.readdirSync(path.join(agent,'sessions')).length===0,'Service bootstrap must not create Agent sessions');
        fs.writeFileSync('artifacts/component-panels/native-extension.json',JSON.stringify({passed:true,modelFreeBootstrap:modelFree,scope:'real rpi plugin ABI; isolated data; zero model requests',checks:['native cdylib load','--http startup','authenticated node API','component discovery','credential-free status','supervised EOF shutdown','no unrelated Agent sessions','private capability isolation','same-owner local access after public revocation']},null,2));
        assert.equal(modelCalls,0);await new Promise(resolve=>model.close(resolve));
        console.log('PASS actual rpi ABI load, HTTP node, component catalog and graceful shutdown');
    }catch(error){child.stdin.end();await bounded(exited,5000).catch(()=>child.kill());await new Promise(resolve=>model.close(resolve));throw error;}
})().catch(e=>{console.error(e);process.exitCode=1;});
