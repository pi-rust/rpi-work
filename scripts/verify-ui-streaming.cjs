const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const {chromium}=require(require.resolve('playwright',{paths:[process.cwd(),path.resolve('../rpi-package/packages/rpi-web/frontend')]}));
const {startFixture}=require('./ui-fixture.cjs');
const results=[];
async function check(name,fn){try{await fn();results.push({name,passed:true});console.log('PASS',name)}catch(e){results.push({name,passed:false,error:e.message});console.log('FAIL',name,e.message.split('\n')[0])}}
async function until(fn,timeout=6000){const end=Date.now()+timeout;while(Date.now()<end){if(fn())return;await new Promise(resolve=>setTimeout(resolve,50))}throw new Error('Fixture observation timed out')}
(async()=>{
  const fixture=await startFixture(0,{includeHistoryAssistant:true});
  const browser=await chromium.launch({headless:true,executablePath:'C:/Users/bigfish/scoop/apps/googlechrome/current/chrome.exe'});
  const page=await browser.newPage({viewport:{width:1440,height:1000}});page.setDefaultTimeout(5000);
  const errors=[];page.on('pageerror',e=>errors.push(e.message));
  try{
    await page.goto(process.env.WORK_UI_URL||'http://127.0.0.1:4173/target/parity-web/');
    await page.getByRole('button',{name:'打开设置',exact:true}).click();
    await page.getByRole('navigation',{name:'设置分类'}).getByRole('button',{name:'执行节点',exact:true}).click();
    await page.getByLabel('节点地址',{exact:true}).fill(fixture.url);
    await page.getByRole('button',{name:'连接节点',exact:true}).click();
    await page.getByRole('status').filter({hasText:'在线'}).waitFor();
    await page.getByRole('button',{name:'返回会话',exact:true}).click();
    await page.locator('.project-row').filter({hasText:'rpi-work'}).click();
    await page.locator('.sidebar-sessions').getByRole('button',{name:'为工作台添加深色模式',exact:true}).click();
    await page.locator('.transcript').getByText('主题切换已经准备好，接下来运行组件测试。',{exact:true}).first().waitFor();
    await check('snapshot messages and event projection render assistant once',async()=>{
      assert.equal(await page.locator('.transcript').getByText('主题切换已经准备好，接下来运行组件测试。',{exact:true}).count(),1);
    });
    fixture.emit({type:'agent_start',runId:'fixture-run-2'});
    fixture.emit({type:'message_start',message:{entryId:'answer-live',role:'assistant',content:[]}});
    fixture.emit({type:'message_update',message:{entryId:'answer-live',role:'assistant'},eventType:'text_delta',delta:'流式片段已保留'});
    await check('streamed text appears before settlement',async()=>{
      await page.locator('.transcript').getByText('流式片段已保留',{exact:true}).waitFor();
    });
    fixture.emit({type:'message_end',message:{entryId:'answer-live',role:'assistant',content:[{type:'text',text:'流式片段已保留'}]}});
    fixture.emit({type:'agent_end'});
    await check('agent_end waits for settlement',async()=>{
      await page.waitForFunction(()=>document.querySelector('.status-badge')?.textContent.includes('结算'));
    });
    const settledSeq=fixture.emit({type:'agent_settled',runId:'fixture-run-2',outcome:'failed',error:'Fixture failure'});
    await check('failed settlement retains partial text',async()=>{
      await page.waitForFunction(()=>document.querySelector('.status-badge')?.textContent.includes('失败'));
      assert.equal(await page.locator('.transcript').getByText('流式片段已保留',{exact:true}).count(),1);
    });
    fixture.disconnect();
    await check('EOF reconnects using the latest snapshot cursor without sending prompt again',async()=>{
      await until(()=>fixture.requests.filter(r=>r.path.endsWith('/events')&&r.query===`?after=${settledSeq}`).length>0);
      assert.equal(fixture.requests.filter(r=>r.body.type==='prompt').length,0);
      assert.equal(await page.locator('.transcript').getByText('流式片段已保留',{exact:true}).count(),1);
    });
    fixture.disconnect('dark',true);
    await check('broken SSE also reconnects with bounded retries',async()=>{
      await until(()=>fixture.requests.filter(r=>r.path.endsWith('/events')&&r.query===`?after=${settledSeq}`).length>=2);
      assert(fixture.requests.filter(r=>r.path.endsWith('/events')).length<10);
    });
    fixture.emit({type:'message_end',message:{entryId:'user-live',role:'user',content:[{type:'text',text:'用户补充消息不能变成助手'}]}});
    await check('live user messages retain their role and appear in conversation',async()=>{
      await page.locator('.transcript .user-message').getByText('用户补充消息不能变成助手',{exact:true}).waitFor();
      assert.equal(await page.locator('.agent-message').getByText('用户补充消息不能变成助手',{exact:true}).count(),0);
    });
    await check('no application JavaScript errors',async()=>assert.deepEqual(errors,[]));
    await page.screenshot({path:'artifacts/ui-streaming-review.png'});
    const buildPath='target/parity-web/build.json';
    fs.writeFileSync('artifacts/ui-streaming-validation.json',JSON.stringify({build:fs.existsSync(buildPath)?JSON.parse(fs.readFileSync(buildPath,'utf8').replace(/^\uFEFF/,'')):null,results,errors,requests:fixture.requests},null,2));
    if(results.some(r=>!r.passed))process.exitCode=1;
  }finally{await browser.close();await fixture.close();}
})().catch(e=>{console.error(e);process.exitCode=1});
