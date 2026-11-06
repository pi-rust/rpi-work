// Formal WASM UI -> actual isolated headless owner -> packaged component UI.
// No production ports, user registry or model calls are used.
const {chromium}=require('../../tools/node_modules/playwright');
const {spawn}=require('node:child_process');
const readline=require('node:readline');
const path=require('node:path');
const fs=require('node:fs');
const os=require('node:os');
const crypto=require('node:crypto');
const assert=require('node:assert/strict');
const timeout=(p,ms=30000)=>Promise.race([p,new Promise((_,reject)=>{const t=setTimeout(()=>reject(new Error('Fixture timeout')),ms);t.unref()})]);
(async()=>{
    const data=fs.mkdtempSync(path.join(os.tmpdir(),'work-components-ui-'));
    const env={...process.env,RPI_WORK_DATA_DIR:data,RPI_CODING_AGENT_DIR:path.join(data,'agent'),RPI_WORK_RPI_BIN:path.resolve('target/debug/work-server-rpc-fixture.exe'),RPI_WORK_HTTP_BIND:'127.0.0.1',RPI_WORK_HTTP_PORT:'0',RPI_WORK_HTTP_PASSWORD:'component-ui-fixture',RPI_WORK_HTTP_ASSETS:path.resolve(process.env.WORK_UI_BUNDLE||'target/component-review/parity-web'),RPI_WORK_HTTP_WEB_PORT:''};
    delete env.RPI_WORK_HTTP_PUBLIC_URL;fs.mkdirSync(env.RPI_CODING_AGENT_DIR,{recursive:true});
    const child=spawn(path.resolve(process.env.WORK_CLI_BIN||'target/debug/work-cli.exe'),['serve','--shutdown-stdin'],{env,windowsHide:true,stdio:['pipe','pipe','pipe']});
    let stderr='';child.stderr.on('data',b=>stderr+=b);
    const exited=new Promise(r=>child.once('exit',r));
    const ready=new Promise((resolve,reject)=>{child.once('error',reject);child.once('exit',code=>reject(new Error(`Owner exited ${code}: ${stderr}`)));readline.createInterface({input:child.stdout}).on('line',line=>{try{const value=JSON.parse(line);if(value.ready)resolve(value)}catch{}})});
    let browser;
    try{
        const node=await timeout(ready);browser=await chromium.launch({headless:true});
        const context=await browser.newContext({viewport:{width:1440,height:1000}});
        const login=await context.request.post(node.address+'api/v1/auth',{headers:{'x-rpi-web':'1'},data:{password:'component-ui-fixture'}});assert.equal(login.status(),200);
        const page=await context.newPage();const errors=[];page.on('pageerror',e=>errors.push(e.message));
        await page.goto(node.address,{waitUntil:'domcontentloaded',timeout:90000});
        await page.getByRole('button',{name:'打开设置',exact:true}).click({timeout:90000});
        await page.getByRole('navigation',{name:'设置分类'}).getByRole('button',{name:'节点组件',exact:true}).click();
        await page.getByRole('button',{name:'打开远程访问',exact:true}).waitFor();
        assert.equal(await page.locator('.component-frame').count(),0);
        await page.getByRole('button',{name:'打开远程访问',exact:true}).click();
        const remote=page.frameLocator('.component-frame');
        await remote.getByText('正在共享',{exact:true}).waitFor();assert.equal(await remote.getByRole('switch').isDisabled(),true);
        await page.getByRole('button',{name:'返回组件目录'}).click();
        await page.getByRole('button',{name:'安装组件',exact:true}).click();
        await page.getByText('粘贴组件包 JSON',{exact:true}).click();
        const manifest={schemaVersion:1,id:'custom-notes',version:'1.0.0',title:'自定义记录面板',description:'独立布局，不修改 Work UI',panels:[{id:'notes',title:'记录面板',slot:'settings',entry:'index.html',protocol:1}],actions:{}};
        const files={'index.html':'<!doctype html><html lang="zh-CN"><body style="font:18px system-ui;padding:32px;background:#eff5ee"><h1>我的独立界面</h1><p>由安装包提供完整布局。</p></body></html>'};
        const bundle={manifest,files,sha256:crypto.createHash('sha256').update(JSON.stringify([manifest,files])).digest('hex')};
        let downloads=0;
        await page.route('https://component-fixture.invalid/component.json',async route=>{
            downloads++;
            const headers=route.request().headers();assert.equal(headers.cookie,undefined);assert.equal(headers.authorization,undefined);
            await route.fulfill({status:200,contentType:'application/json',headers:{'access-control-allow-origin':'*'},body:JSON.stringify(bundle)});
        });
        await page.getByLabel('组件包下载地址',{exact:true}).fill('https://component-fixture.invalid/component.json');
        await page.getByRole('button',{name:'下载并检查',exact:true}).click();
        await page.getByRole('button',{name:'安装到当前节点',exact:true}).waitFor();
        assert.equal(downloads,1);
        await page.getByLabel('组件包 JSON',{exact:true}).fill(JSON.stringify(bundle));
        await page.getByRole('button',{name:'检查组件包',exact:true}).click();
        await page.getByRole('button',{name:'安装到当前节点',exact:true}).click();
        await page.getByRole('button',{name:'打开记录面板',exact:true}).click();
        await page.frameLocator('.component-frame').getByRole('heading',{name:'我的独立界面'}).waitFor();
        fs.mkdirSync('artifacts/component-panels',{recursive:true});
        await page.screenshot({path:'artifacts/component-panels/workspace.png'});
        await page.getByRole('button',{name:'返回组件目录'}).click();
        await page.locator('.component-card').filter({hasText:'自定义记录面板'}).getByRole('button',{name:'卸载',exact:true}).click();
        await page.locator('.component-card').filter({hasText:'自定义记录面板'}).waitFor({state:'detached'});
        assert.deepEqual(errors,[]);
        fs.writeFileSync('artifacts/component-panels/workspace.json',JSON.stringify({passed:true,scope:'formal WASM UI and actual isolated work-host; HTTPS package download fulfilled by isolated browser fixture; no model invocation',checks:['lazy load','official remote-access panel','remote policy read-only','credential-free HTTPS download','custom UI package install','custom layout mount','uninstall'],errors},null,2));
        console.log('PASS formal Work UI: official panel, custom package install/mount/uninstall, actual owner');
    }finally{
        if(browser)await browser.close();child.stdin.end();const code=await timeout(exited);assert.equal(code,0,stderr);
    }
})().catch(e=>{console.error(e);process.exitCode=1;});
