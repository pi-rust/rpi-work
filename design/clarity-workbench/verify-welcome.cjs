// Browser acceptance for the standalone entry screen. No model or node calls.
const { chromium } = require('../../tests/support/browser.cjs');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { pathToFileURL } = require('node:url');
const output = path.resolve(__dirname,'../../target/clarity-welcome');
fs.mkdirSync(output,{recursive:true});
(async () => {
  const browser = await chromium.launch({headless:true});
  const context = await browser.newContext({viewport:{width:1440,height:1000},reducedMotion:'reduce'});
  const page = await context.newPage();page.setDefaultTimeout(6000);
  const errors = [], network = [], checks = [];
  page.on('pageerror',error => errors.push(error.message));
  await context.route(/^https?:/,route => {network.push(route.request().url());route.abort();});
  const check = async (name,body) => {await body();checks.push(name);console.log('PASS',name);};
  const noOverflow = async () => assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth && document.querySelector('.workspace-page').scrollWidth <= document.querySelector('.workspace-page').clientWidth + 1));
  const home = async () => {await page.keyboard.press('Control+j');await page.locator('.welcome-heading').waitFor();};
  try {
    await page.goto(pathToFileURL(path.join(__dirname,'index.html')).href);
    await check('首次进入欢迎页；空白不发送、不选中历史会话',async () => {
      assert.equal(new URL(page.url()).hash,'#home');assert(await page.locator('.welcome-heading').isVisible());
      assert(await page.locator('#chat-panel').isHidden());assert(await page.locator('.send-button').isDisabled());
      assert.equal(await page.locator('#sidebar-sessions .session.active').count(),0);
      await page.locator('#prompt').fill('   ');await page.keyboard.press('Enter');assert.equal(new URL(page.url()).hash,'#home');
      await page.locator('#prompt').fill('');await page.locator('#prompt').blur();
      await page.screenshot({path:path.join(output,'welcome-desktop.png')});
    });
    await check('快捷任务只填入草稿；项目/模型选择保留输入，选择器可 Escape 关闭',async () => {
      await page.getByRole('button',{name:'编写代码',exact:true}).click();
      assert.equal(await page.locator('#prompt').inputValue(),'帮我实现一个功能：');
      await page.locator('#prompt').fill('帮我实现一个功能：<img src=x onerror=alert(1)>');
      await page.getByRole('button',{name:'选择新对话的项目',exact:true}).click();
      await page.locator('[data-home-node="studio"][data-home-project="pi-rust"]').click();
      assert.equal(await page.locator('#welcome-project-name').innerText(),'pi-rust');
      assert.equal(await page.locator('#welcome-project-node').innerText(),'开发机');
      await page.getByRole('button',{name:'切换模型',exact:true}).click();await page.locator('[data-model="GPT-5"]').click();
      await page.getByRole('button',{name:'切换思考等级',exact:true}).click();await page.locator('[data-thinking="中"]').click();
      await page.getByRole('button',{name:'选择新对话的项目',exact:true}).click();await page.keyboard.press('Escape');
      assert(await page.getByRole('button',{name:'选择新对话的项目',exact:true}).evaluate(el=>el===document.activeElement));
      assert.equal(await page.locator('#prompt').inputValue(),'帮我实现一个功能：<img src=x onerror=alert(1)>');
      await page.getByRole('button',{name:'附件与更多',exact:true}).click();assert.equal(await page.locator('#configuration-origin').innerText(),'开发机 · pi-rust');await page.keyboard.press('Escape');
    });
    await check('发送进入所选项目的本地对话；不显示固定历史，输入作为纯文本呈现',async () => {
      await page.locator('#prompt').focus();await page.keyboard.press('Enter');
      assert.equal(new URL(page.url()).hash,'#workspace');assert(await page.locator('.welcome-heading').isHidden());
      assert(await page.locator('#fixture-transcript').isHidden());assert(await page.locator('#preview-transcript').isVisible());
      assert.equal(await page.locator('#session-origin').innerText(),'开发机 · pi-rust');
      assert.equal(await page.locator('.model-name').innerText(),'GPT-5');assert.match(await page.locator('.thinking-name').innerText(),/中/);
      assert.equal(await page.locator('#preview-transcript img').count(),0);
      assert.equal(await page.locator('#preview-transcript .user-bubble').innerText(),'帮我实现一个功能：<img src=x onerror=alert(1)>');
      assert(await page.locator('.send-button').isDisabled());
      await page.locator('#prompt').fill('继续这个想法');await page.keyboard.press('Shift+Enter');
      assert.equal(await page.locator('#preview-transcript .user-message').count(),1);
      await page.keyboard.press('Enter');assert.equal(await page.locator('#preview-transcript .user-message').count(),2);
    });
    await check('欢迎页草稿与原会话草稿隔离；返回已有会话恢复原历史',async () => {
      await page.locator('#prompt').fill('新会话里的草稿');await home();
      assert.equal(await page.locator('#prompt').inputValue(),'');await page.locator('#prompt').fill('欢迎页里的草稿');
      await page.locator('#sidebar-sessions .sidebar-recent [data-session-id^="preview-"]').first().click();
      assert.equal(await page.locator('#prompt').inputValue(),'新会话里的草稿');
      await home();assert.equal(await page.locator('#prompt').inputValue(),'欢迎页里的草稿');
      await page.keyboard.press('Control+k');await page.locator('#session-search').fill('重新设计工作台界面');
      await page.locator('#switcher-sessions [data-node="local"][data-session-id="design"]').click();
      assert(await page.locator('#fixture-transcript').isVisible());assert(await page.locator('#preview-transcript').isHidden());
      await home();assert.equal(await page.locator('#prompt').inputValue(),'欢迎页里的草稿');await page.locator('#prompt').fill('');
    });
    await check('六档宽度及短视口无横向溢出，常用控件可见且触控高度至少44px',async () => {
      for (const [width,height] of [[1440,1000],[1280,800],[1024,768],[768,900],[390,844],[320,640],[390,480],[1024,480]]) {
        await page.setViewportSize({width,height});await noOverflow();
        for (const selector of ['.composer .attach','.composer .model-button','.composer .thinking-button','.composer .send-button','.welcome-suggestions button']) {
          for (const button of await page.locator(selector).all()) {
            const box=await button.boundingBox();assert(box && box.height>=44,`${selector} at ${width}x${height}`);
            assert(box.x>=0 && box.x+box.width<=width+1,`${selector} in viewport`);
          }
        }
        if (width===390 && height===844 || width===320 && height===640) await page.screenshot({path:path.join(output,`welcome-${width}.png`)});
      }
    });
    await check('手机导航、弹层与新建对话可用；切换项目不清空输入',async () => {
      await page.setViewportSize({width:390,height:844});
      await page.getByRole('button',{name:'打开导航',exact:true}).click();await page.getByRole('button',{name:'新对话',exact:true}).click();
      assert(await page.locator('.sidebar').isHidden());
      await page.locator('#prompt').fill('手机上的想法');await page.getByRole('button',{name:'选择新对话的项目',exact:true}).click();
      await page.locator('[data-home-node="local"][data-home-project="rpi-work"]').click();assert.equal(await page.locator('#prompt').inputValue(),'手机上的想法');
      await page.getByRole('button',{name:'发送消息',exact:true}).click();assert.equal(await page.locator('#session-origin').innerText(),'本机 · rpi-work');await noOverflow();
    });
    await check('无外部请求或脚本异常',async () => {assert.deepEqual(errors,[]);assert.deepEqual(network,[]);});
    fs.writeFileSync(path.join(output,'verification.json'),JSON.stringify({checks,errors,networkRequests:network},null,2)+'\n');
  } finally {await browser.close();}
})().catch(error => {console.error(error);process.exitCode=1;});
