// Browser qualification of the actual packaged UI and shared bridge. The owner
// status is an isolated fixture; real owner/HTTP semantics are tested in Rust.
const {chromium}=require('node:module').createRequire(require.resolve('../../tools/package.json'))('playwright');
const http=require('node:http');
const fs=require('node:fs');
const path=require('node:path');
const assert=require('node:assert/strict');
const source=path.resolve(process.env.RPI_COMPONENTS_SOURCE||'vendor');
const ui=fs.readFileSync(path.join(source,'rpi-http-component/ui/index.html'),'utf8');
const bridge=fs.readFileSync(path.join(source,'rpi-components/ui-host/bridge.js'),'utf8');
const literal=x=>JSON.stringify(x).replace(/</g,'\\u003c');
let enabled=true,reads=0,writes=0;
let configuration={bind:'0.0.0.0',port:13031,publicUrl:''},configurationWrites=0;
const server=http.createServer((req,res)=>{
    if(req.url==='/call'){
        let body='';req.on('data',c=>body+=c);req.on('end',()=>{
            const request=JSON.parse(body);
            if(request.action==='set-enabled'){writes++;enabled=request.arguments.enabled;}
            else if(request.action==='configure'){configurationWrites++;configuration=request.arguments.configuration;assert.equal(request.arguments.newPassword,'isolated-panel-password');}
            else reads++;
            res.setHeader('Content-Type','application/json');
            res.end(JSON.stringify({id:request.id,value:{listening:true,enabled,status:enabled?'HTTP 服务运行中':'远程访问已关闭',warning:'',publicUrl:'http://192.0.2.10:13031/',canConfigure:true,configuration,restartRequired:configurationWrites>0}}));
        });return;
    }
    res.setHeader('Content-Type','text/html; charset=utf-8');
    res.end(`<!doctype html><html style="--canvas:#faf9f6;--card:#fff;--text:#262722;--muted:#777970;--line:#e6e7df;--accent:#54794d"><head><link rel="icon" href="data:,"></head><body style="margin:24px"><button id="open">打开组件</button><main></main><script>
        const replies=[],waiters=[];window.dioxus={send:async data=>{const r=await fetch('/call',{method:'POST',body:JSON.stringify(data)});const value=await r.json();if(waiters.length)waiters.shift()(value);else replies.push(value);},recv:()=>replies.length?Promise.resolve(replies.shift()):new Promise(r=>waiters.push(r))};
        ${bridge}
        document.getElementById('open').onclick=()=>{document.querySelector('main').innerHTML='<iframe id="component" title="扩展面板" sandbox="allow-scripts" style="width:100%;height:650px;border:0"></iframe>';mountComponentPanel('component',${literal(ui)},['status','set-enabled','configure']);};
        </script></body></html>`);
});
(async()=>{
    await new Promise(r=>server.listen(0,'127.0.0.1',r));
    const browser=await chromium.launch({headless:true});
    try{
        const page=await browser.newPage({viewport:{width:1100,height:850}});
        const errors=[];page.on('pageerror',e=>errors.push(e.message));
        const consoleErrors=[];page.on('console',m=>{if(m.type()==='error')consoleErrors.push(m.text())});
        await page.goto(`http://127.0.0.1:${server.address().port}`);
        assert.equal(reads,0);assert.equal(await page.locator('iframe').count(),0);
        await page.getByRole('button',{name:'打开组件'}).click();
        const frame=page.frameLocator('#component');
        await frame.getByText('正在共享',{exact:true}).waitFor();
        assert.equal(await frame.getByLabel('访问地址').inputValue(),'http://192.0.2.10:13031/');
        assert.equal(await page.locator('#component').getAttribute('sandbox'),'allow-scripts');
        await frame.getByRole('switch').click();
        await frame.getByText('已关闭',{exact:true}).waitFor();
        assert.equal(enabled,false);assert.equal(writes,1);
        await frame.getByRole('switch').click();await frame.getByText('正在共享',{exact:true}).waitFor();
        await frame.locator('summary').filter({hasText:'连接设置'}).click();
        await frame.getByLabel('监听 IP',{exact:true}).fill('127.0.0.1');
        await frame.getByLabel('监听端口',{exact:true}).fill('14321');
        await frame.getByLabel('分享地址',{exact:true}).fill('https://node.example.com/');
        await frame.getByLabel('节点新密码',{exact:true}).fill('isolated-panel-password');
        await frame.getByRole('button',{name:'保存，下次启动生效',exact:true}).click();
        await frame.getByText('设置已保存，下次启动生效。当前连接和任务继续运行。',{exact:true}).waitFor({timeout:5000}).catch(error=>{throw new Error(`Configuration UI did not save (${configurationWrites} calls): ${consoleErrors.join('; ')}; ${error.message}`)});
        assert.equal(configurationWrites,1);assert.equal(configuration.port,14321);assert.equal(configuration.bind,'127.0.0.1');
        assert.equal(await frame.getByLabel('节点新密码',{exact:true}).inputValue(),'');
        fs.mkdirSync('artifacts/component-panels',{recursive:true});
        await page.locator('#component').evaluate(el=>el.style.height='1000px');
        await page.screenshot({path:'artifacts/component-panels/configuration-panel.png',fullPage:true});
        await frame.locator('summary').filter({hasText:'连接设置'}).click();await page.locator('#component').evaluate(el=>el.style.height='650px');
        await page.screenshot({path:'artifacts/component-panels/desktop.png'});
        await page.setViewportSize({width:390,height:850});
        await page.screenshot({path:'artifacts/component-panels/mobile.png'});
        await frame.locator('summary').filter({hasText:'连接设置'}).click();
        await frame.getByLabel('节点新密码',{exact:true}).waitFor();
        await page.screenshot({path:'artifacts/component-panels/configuration-mobile.png'});
        const overflow=await frame.locator('html').evaluate(el=>el.scrollWidth>el.clientWidth);
        assert.equal(overflow,false);
        // The sandbox must not grant parent DOM or credential access.
        const isolated=await frame.locator('html').evaluate(()=>{try{return parent.document===document}catch{return true}});
        assert.equal(isolated,true);
        assert.deepEqual(errors,[]);
        assert.deepEqual(consoleErrors,[]);
        fs.writeFileSync('artifacts/component-panels/browser.json',JSON.stringify({passed:true,scope:'packaged custom HTML + shared MessageChannel bridge; isolated owner status fixture',reads,writes,configurationWrites,checks:['listener form','new password cleared after save','pending restart notice'],errors},null,2));
        console.log('PASS lazy load, custom panel, action bridge, sandbox, desktop/mobile layout');
    }finally{await browser.close();await new Promise(r=>server.close(r));}
})().catch(e=>{console.error(e);process.exitCode=1;server.close();});
