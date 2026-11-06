// Actual native ABI -> private local configuration -> restart -> authenticated
// public API. Isolated node/agent directories; no external model requests.
const {spawn}=require('node:child_process');
const {randomBytes}=require('node:crypto');
const fs=require('node:fs'),os=require('node:os'),path=require('node:path'),net=require('node:net');
const readline=require('node:readline'),assert=require('node:assert/strict');
const bounded=(p,ms=25000)=>Promise.race([p,new Promise((_,reject)=>{const t=setTimeout(()=>reject(new Error('Native configuration timeout')),ms);t.unref()})]);
(async()=>{
    const rpi=process.env.RPI_NATIVE_BIN;assert(rpi,'Set RPI_NATIVE_BIN to a model-free service host');
    const dll=path.resolve(process.env.RPI_HTTP_EXTENSION||'target/debug/rpi_http_extension.dll');assert(fs.existsSync(dll));
    const root=fs.mkdtempSync(path.join(os.tmpdir(),'rpi-http-config-'));
    const data=path.join(root,'node'),agent=path.join(root,'agent');fs.mkdirSync(agent);
    const base={...process.env,RPI_CODING_AGENT_DIR:agent,RPI_WORK_DATA_DIR:data,RPI_WORK_RPI_BIN:path.resolve('target/debug/work-server-rpc-fixture.exe')};
    for(const key of Object.keys(base))if(/API_KEY|AUTH_TOKEN|ACCESS_TOKEN|BEARER_TOKEN/i.test(key))delete base[key];
    for(const key of ['RPI_WORK_HTTP_BIND','RPI_WORK_HTTP_PORT','RPI_WORK_HTTP_PASSWORD','RPI_HTTP_PASSWORD','RPI_WORK_HTTP_PUBLIC_URL','RPI_WORK_HTTP_ALLOWED_HOSTS','RPI_WORK_HTTP_ASSETS','RPI_WORK_HTTP_WEB_PORT','RPI_NODE_CONTROL_TOKEN','RPI_NODE_CONTROL_STDIN'])delete base[key];
    async function start(initial=false){
        const capability=randomBytes(32).toString('hex');
        const args=['--local-only','--no-session','--no-skills','--no-prompt-templates','--no-context-files','--extension',dll,'--http','--node-control-stdin'];
        if(initial)args.push('--bind','127.0.0.1','--port','0');
        const child=spawn(rpi,args,{env:base,cwd:root,windowsHide:true,stdio:['pipe','pipe','pipe']});
        child.stdin.write(JSON.stringify({capability})+'\n');let stderr='';child.stderr.on('data',b=>stderr+=b);
        const exited=new Promise(resolve=>child.once('exit',resolve));
        const ready=await bounded(new Promise((resolve,reject)=>{
            child.once('error',reject);child.once('exit',code=>reject(new Error(`Native host exited ${code}: ${stderr}`)));
            readline.createInterface({input:child.stdout}).on('line',line=>{try{const value=JSON.parse(line);if(value.ready)resolve(value)}catch{}});
        })).catch(error=>{child.stdin.end();throw error});
        const call=async(action,arguments={})=>{
            const response=await fetch(ready.controlAddress+'api/v1/components/rpi-http/call',{method:'POST',headers:{'content-type':'application/json','x-rpi-web':'1','x-rpi-node-control':capability},body:JSON.stringify({action,arguments})});
            return {status:response.status,value:await response.json()};
        };
        return {ready,call,stop:async()=>{child.stdin.end();assert.equal(await bounded(exited),0,stderr);}};
    }
    const portServer=net.createServer();await new Promise(r=>portServer.listen(0,'127.0.0.1',r));const port=portServer.address().port;await new Promise(r=>portServer.close(r));
    let host=await start(true);
    try{
        const firstId=host.ready.nodeId,firstAddress=host.ready.address;
        const configuration={bind:'127.0.0.1',port,publicUrl:''},password='isolated-new-node-password';
        const saved=await host.call('configure',{configuration,newPassword:password});assert.equal(saved.status,200);assert(saved.value.restartRequired);
        assert(!JSON.stringify(saved.value).includes(password)&&!JSON.stringify(saved.value).includes('passwordDigest'));
        const file=path.join(data,'http-component.json'),bytes=fs.readFileSync(file,'utf8');assert(!bytes.includes(password));
        const invalid=await host.call('configure',{configuration:{...configuration,bind:'invalid IP'},newPassword:'must-not-echo-this'});assert.notEqual(invalid.status,200);assert(!JSON.stringify(invalid).includes('must-not-echo-this'));assert.equal(fs.readFileSync(file,'utf8'),bytes);
        assert.equal((await fetch(firstAddress+'api/v1/health')).status,200,'Saving must not stop current listener');
        await host.stop();host=await start();
        assert.equal(host.ready.nodeId,firstId);assert.equal(new URL(host.ready.address).port,String(port));
        const status=await host.call('status');assert.equal(status.value.restartRequired,false);assert.equal(status.value.configuration.port,port);
        const login=await fetch(host.ready.address+'api/v1/auth',{method:'POST',headers:{'content-type':'application/json','x-rpi-web':'1'},body:JSON.stringify({password})});assert.equal(login.status,200);
        const cookie=login.headers.get('set-cookie').split(';')[0];
        const denied=await fetch(host.ready.address+'api/v1/components/rpi-http/call',{method:'POST',headers:{cookie,'content-type':'application/json','x-rpi-web':'1'},body:JSON.stringify({action:'configure',arguments:{configuration:{...configuration,port:0}}})});assert.notEqual(denied.status,200);
        const sharedConfiguration={bind:'0.0.0.0',port:0,publicUrl:'https://node.example.com/'};
        assert.equal((await host.call('configure',{configuration:sharedConfiguration})).status,200);
        await host.stop();host=await start();
        const advertised=await host.call('status');assert.equal(advertised.value.publicUrl,sharedConfiguration.publicUrl);assert.equal(advertised.value.configuration.bind,'0.0.0.0');
        const loopback=new URL(host.ready.address);loopback.hostname='127.0.0.1';
        const reauth=await fetch(loopback+'api/v1/auth',{method:'POST',headers:{'content-type':'application/json','x-rpi-web':'1'},body:JSON.stringify({password})});assert.equal(reauth.status,200,'OS-selected public port and preserved password must remain usable');
        await host.stop();
        const corrupt=JSON.parse(fs.readFileSync(file,'utf8'));corrupt.configuration.bind='bad-address';fs.writeFileSync(file,JSON.stringify(corrupt));
        host=await start();assert.equal(host.ready.address,'');
        const unavailable=await host.call('status');assert.equal(unavailable.value.listening,false);assert.equal(unavailable.value.canConfigure,true);assert.equal(host.ready.nodeId,firstId);
        assert.equal((await host.call('configure',{configuration:{...configuration,port:0}})).status,200);
        const repaired=JSON.parse(fs.readFileSync(file,'utf8'));assert.deepEqual(repaired.passwordDigest,corrupt.passwordDigest,'Repairing listener settings must retain the password verifier');
        fs.mkdirSync('artifacts/component-panels',{recursive:true});fs.writeFileSync('artifacts/component-panels/configuration.json',JSON.stringify({passed:true,checks:['local-only configuration','current listener preserved','same node identity after restart','saved listener port applied','saved password verifier authenticates','no password/status echo','private local repair after public configuration failure']},null,2));
        console.log('PASS actual ABI listener configuration, restart, password auth and local repair');
    }finally{await host.stop();}
})().catch(error=>{console.error(error);process.exitCode=1});
