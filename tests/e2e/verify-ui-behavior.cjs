const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const {chromium}=require('../support/browser.cjs');
const {startFixture}=require('../fixtures/ui-fixture.cjs');
const results=[];
async function check(name,fn){try{await fn();results.push({name,passed:true});console.log('PASS',name)}catch(e){results.push({name,passed:false,error:e.message});console.log('FAIL',name,e.message.split('\n')[0])}}
async function connect(page,fixture,login=false){
  await page.goto(process.env.WORK_UI_URL||'http://127.0.0.1:4180/');
  await page.getByRole('button',{name:'打开设置',exact:true}).click();
  await page.getByRole('navigation',{name:'设置分类'}).getByRole('button',{name:'执行节点',exact:true}).click();
  await page.getByLabel('节点地址',{exact:true}).fill(fixture.url);
  await page.getByRole('button',{name:'连接节点',exact:true}).click();
  if(login){
    await page.getByLabel('节点密码',{exact:true}).fill('test-only');
    const loginResponse=page.waitForResponse(r=>r.url().endsWith('/api/v1/auth')&&r.request().method()==='POST');
    await page.getByRole('button',{name:'登录',exact:true}).click();
    await loginResponse;
    // An explicit connect after login must preserve the authenticated client too.
    await page.getByRole('button',{name:'连接节点',exact:true}).click();
  }
  await page.getByRole('status').filter({hasText:'在线'}).waitFor();
  await page.getByRole('button',{name:'返回会话',exact:true}).click();
  await page.locator('.project-row').filter({hasText:'rpi-work'}).click();
  await page.locator('.sidebar-sessions').getByRole('button',{name:'为工作台添加深色模式',exact:true}).click();
  await page.locator('.transcript').getByText('主题切换已经准备好，接下来运行组件测试。',{exact:true}).waitFor();
}
(async()=>{
  const fixture=await startFixture(),protectedFixture=await startFixture(0,{authRequired:true});
  const browser=await chromium.launch({headless:true,executablePath:process.env.CHROME_PATH});
  const page=await browser.newPage({viewport:{width:1440,height:1000}});
  page.setDefaultTimeout(5000);
  try{
    await connect(page,fixture);
    const prompt=page.locator('.composer-main textarea');
    await check('empty queue does not consume conversation height',async()=>{
      const queue=page.locator('.queue').first();
      if(await queue.count()&&await queue.isVisible())assert((await queue.boundingBox()).height<=48,'empty queue is a full panel');
    });
    await check('footer displays known token/cache/context metrics',async()=>{
      const text=await page.locator('.composer-main').innerText();
      assert.match(text,/缓存/);assert.match(text,/75\s*%/);assert.match(text,/上下文/);
      assert.match(text,/(?:输入|↑)\s*(?:32(?:\.0)?\s*[kK]|32[,.]?000)/);
      assert.match(text,/(?:输出|↓)\s*(?:4\.8\s*[kK]|4[,.]?800)/);
    });
    await check('model, thinking, and tool settings use real node commands',async()=>{
      await page.locator('.composer-options .composer-option').first().click();
      await page.getByLabel('模型',{exact:true}).selectOption('openai/gpt-6-astra');
      await page.getByLabel('思考级别',{exact:true}).selectOption('low');
      await page.getByRole('checkbox',{name:'write',exact:true}).uncheck();
      await page.waitForFunction(()=>document.querySelector('#session-thinking')?.value==='low');
      assert(fixture.requests.some(r=>r.body.type==='set_model'&&r.body.modelId==='openai/gpt-6-astra'));
      assert(fixture.requests.some(r=>r.body.type==='set_thinking_level'&&r.body.level==='low'));
      assert(fixture.requests.some(r=>r.body.type==='set_active_tools'&&!r.body.tools.includes('write')));
      await page.keyboard.press('Escape');
    });
    if(await page.getByRole('dialog').isVisible())await page.keyboard.press('Escape');
    await check('session drafts are independent and restored',async()=>{
      await prompt.fill('会话 A 的草稿');
      await page.locator('.sidebar-sessions').getByRole('button',{name:'梳理 Agent 架构',exact:true}).click();
      assert.equal(await prompt.inputValue(),'');
      await prompt.fill('会话 B 的草稿');
      await page.locator('.sidebar-sessions').getByRole('button',{name:'为工作台添加深色模式',exact:true}).click();
      assert.equal(await prompt.inputValue(),'会话 A 的草稿');
    });
    // Restore a known selection after a failed assertion as well.
    await page.locator('.sidebar-sessions').getByRole('button',{name:'为工作台添加深色模式',exact:true}).click();
    await check('failed send retains submitted text',async()=>{
      fixture.controls.failPrompt=true;
      await prompt.fill('发送失败必须保留这条消息');
      const response=page.waitForResponse(r=>r.url().endsWith('/dark/commands')&&r.request().postDataJSON().type==='prompt');
      await page.getByRole('button',{name:'发送消息',exact:true}).click();
      await response;
      await page.waitForFunction(()=>document.querySelector('.composer-main').textContent.includes('未接受')||document.querySelector('.composer-main').textContent.includes('失败')||document.querySelector('.composer-main textarea').value==='发送失败必须保留这条消息');
      assert.equal(await prompt.inputValue(),'发送失败必须保留这条消息');
      fixture.controls.failPrompt=false;
    });
    fixture.controls.failPrompt=false;
    await check('switching project never keeps another project session selected',async()=>{
      await page.locator('.project-row').filter({hasText:'pi-rust'}).click();
      assert.doesNotMatch(await page.locator('.app-header').innerText(),/为工作台添加深色模式/);
    });
    await page.locator('.project-row').filter({hasText:'rpi-work'}).click();
    await page.locator('.sidebar-sessions').getByRole('button',{name:'为工作台添加深色模式',exact:true}).click();
    await check('large change list actually filters by file name',async()=>{
      await page.getByRole('tab',{name:/^变更/}).click();
      await page.getByRole('textbox',{name:'搜索变更文件',exact:true}).fill('module-119');
      await page.waitForFunction(()=>document.querySelectorAll('.change-file').length<=1);
      assert.equal(await page.locator('.change-file').count(),1);
      assert.match(await page.locator('.change-file').innerText(),/module-119/);
    });
    const authPage=await browser.newPage({viewport:{width:1440,height:1000}});authPage.setDefaultTimeout(5000);
    await check('protected node login persists across connect and session requests',async()=>{
      await connect(authPage,protectedFixture,true);
      const sessionRequests=protectedFixture.requests.filter(r=>r.path.includes('/sessions'));
      assert(sessionRequests.length>0);
      assert(sessionRequests.every(r=>r.authenticated));
    });
    const buildPath='target/parity-web/build.json';
    const build=fs.existsSync(buildPath)?JSON.parse(fs.readFileSync(buildPath,'utf8').replace(/^\uFEFF/,'')):null;
    fs.writeFileSync('artifacts/ui-behavior-validation.json',JSON.stringify({fixture:'test-only; no model requests',build,results,requests:fixture.requests,protectedRequests:protectedFixture.requests.map(({body,...rest})=>rest)},null,2));
    if(results.some(r=>!r.passed))process.exitCode=1;
  }finally{await browser.close();await fixture.close();await protectedFixture.close();}
})().catch(e=>{console.error(e);process.exitCode=1});
