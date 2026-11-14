const assert = require('node:assert/strict');
const path = require('node:path');
const fs = require('node:fs');
const { pathToFileURL } = require('node:url');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');
(async () => {
  fs.mkdirSync(path.resolve('artifacts'), {recursive:true});
  const fixture = await startFixture();
  const browser = await chromium.launch({ headless: true });
  const context = await browser.newContext({ viewport: {width:1440,height:900} });
  const page = await context.newPage(), reference = await browser.newPage();
  const errors = [];
  page.on('pageerror', e => errors.push(e.message));
  page.setDefaultTimeout(8000);
  await context.route('https://api.github.com/**', r => r.fulfill({ status:404, body:'{}' }));
  await context.route(fixture.url+'api/v1/providers', r => r.fulfill({ json:{
    revision:'fixture',
    providers:[{id:'deepseek',configured:true,configKeyPresent:true,storedCredential:'apiKey',
      definition:{id:'deepseek',name:'DeepSeek',api:'openai-completions',baseUrl:'https://example.invalid',models:[
        {id:'deepseek-v4-pro',name:'DeepSeek V4 Pro',reasoning:true,contextWindow:null,maxTokens:null},
      ]}}],
  }}));
  await context.route('**/api/v1/projects/*/native-resource-config', r => r.fulfill({status:501,json:{error:'Unsupported by fixture'}}));
  const button = (name, scope = page) => scope.getByRole('button',{name,exact:true});
  const home = page.locator('.home-content');
  const input = home.locator('textarea');
  const submit = button('发送消息',home);
  const creates = () => fixture.requests.filter(r => r.method==='POST' && r.path==='/api/v1/sessions');
  const sent = [];
  page.on('websocket', ws => ws.on('framesent', ({payload}) => sent.push(JSON.parse(payload))));
  const prompts = () => sent.filter(frame => frame.type==='execute' && frame.payload.type==='prompt').map(frame => ({body:frame.payload}));
  const waitUntil = async (condition, label) => {
    for (let i=0;i<200;i++) {
      if (await condition()) return;
      await new Promise(resolve => setTimeout(resolve,25));
    }
    assert.fail(label);
  };
  try {
    await page.goto(process.env.WORK_UI_URL);
    await home.waitFor();
    const pairs = [
      ['.home-content','.conversation-card'],['.home-heading','.welcome-heading'],['.home-heading h1','.welcome-heading h1'],
      ['.home-composer','.composer'],['.home-composer textarea','.composer textarea'],['.home-tools','.composer-tools'],
      ['.home-suggestions','.welcome-suggestions'],['.home-footnote','.welcome-footnote'],
    ];
    for (const [width,height] of [[1920,1080],[1440,900],[1280,900],[1024,768],[768,768],[390,844],[320,568],[390,520],[1024,500]]) {
      await page.setViewportSize({width,height});
      await page.waitForFunction(w => document.querySelector('.app-shell').dataset.deviceMode===(w<768?'mobile':'desktop'),width);
      await reference.setViewportSize({width,height});
      await reference.goto(pathToFileURL(path.resolve('design/clarity-workbench/index.html')).href+'#home');
      await reference.locator('.welcome-heading').waitFor();
      for (const [actual,expected] of pairs) {
        const [a,b] = await Promise.all([page.locator(actual).boundingBox(),reference.locator(expected).boundingBox()]);
        for (const axis of ['x','y','width','height']) assert(Math.abs(a[axis]-b[axis])<=1, `${width}x${height} ${actual} ${axis}: ${a[axis]} vs ${b[axis]}`);
      }
      assert(await page.evaluate(() => document.documentElement.scrollWidth<=innerWidth));
      console.log('PASS geometry',width,height);
    }
    const styles = [
      ['.home-heading h1', '.welcome-heading h1', ['fontSize', 'fontWeight', 'lineHeight', 'letterSpacing', 'color']],
      // The user supplied a new gear logo. Retain the prototype's mark geometry.
      ['.home-mark', '.welcome-mark .brand-symbol', ['width', 'height']],
      ['.home-identity strong', '.welcome-identity strong', ['fontSize', 'fontWeight', 'lineHeight', 'letterSpacing', 'color']],
      ['.home-header', '.topbar', ['fontSize', 'lineHeight', 'padding']],
      ['.home-tools .home-more', '.composer .attach', ['width', 'height', 'borderRadius']],
      ['.home-tools .composer-model-picker', '.composer .model-button', ['fontSize', 'fontWeight', 'padding', 'gap', 'color', 'borderRadius']],
      ['.home-tools .composer-thinking-select', '.composer .thinking-button', ['fontSize', 'padding', 'gap', 'borderRadius']],
      ['.home-suggestions button', '.welcome-suggestions button', ['fontSize', 'height', 'padding', 'gap', 'borderColor', 'borderRadius', 'color']],
    ];
    for (const width of [1440, 1024, 390, 320]) {
      await page.setViewportSize({width, height: 900});
      await reference.setViewportSize({width, height: 900});
      await page.mouse.move(0, 0);
      for (const [actualSelector, expectedSelector, keys] of styles) {
        const read = (element, properties) => {
          const style = getComputedStyle(element);
          return Object.fromEntries(properties.map(key => [key, style[key]]));
        };
        const [actual, expected] = await Promise.all([
          page.locator(actualSelector).first().evaluate(read, keys),
          reference.locator(expectedSelector).first().evaluate(read, keys),
        ]);
        assert.deepEqual(actual, expected, `${width}: ${actualSelector} style differs from prototype`);
      }
      await input.focus();
      await reference.locator('.composer textarea').focus();
      assert.equal(
        await home.locator('.home-composer').evaluate(element => getComputedStyle(element).borderColor),
        await reference.locator('.composer').evaluate(element => getComputedStyle(element).borderColor),
        'home editor focus border matches the prototype',
      );
      await home.locator('h1').click();
      await reference.locator('.welcome-heading h1').click();
      await page.screenshot({path: `artifacts/home-clarity-${width}.png`, animations: 'disabled'});
      await reference.screenshot({path: `artifacts/home-reference-${width}.png`, animations: 'disabled'});
      console.log('PASS home header, controls, mark, suggestions and focus styles', width);
    }
    await page.setViewportSize({width:1440,height:900});
    await page.waitForFunction(() => document.querySelector('.app-shell').dataset.deviceMode==='desktop');
    await button('进入全屏').click();
    await page.waitForFunction(() => Boolean(document.fullscreenElement));
    await button('退出全屏').click();
    await page.waitForFunction(() => !document.fullscreenElement);
    await button('收起项目导航').click();
    assert.equal(await page.locator('.sidebar').isVisible(),false);
    await button('打开项目导航').click();
    assert(await page.locator('.sidebar').isVisible());
    assert(await submit.isDisabled());
    await input.fill('离线草稿');
    assert(await submit.isDisabled());
    await button('打开设置').click();
    await button('工作协同', page.getByRole('navigation',{name:'设置分类'})).click();
    await button('连接设备',page.locator('.collaboration-hub .connect-card')).click();
    await page.getByLabel('节点地址',{exact:true}).fill(fixture.url);
    await button('连接节点').click();
    await page.locator('.collaboration-page-heading [role=status]').filter({hasText:'已连接'}).waitFor();
    await button('返回会话').click();
    await home.waitFor();
    assert.equal(await input.inputValue(),'');
    assert.equal(creates().length,0);
    await button('选择新对话的项目',home).click();
    await page.getByRole('dialog',{name:'在哪个项目开始？'}).getByRole('button',{name:/pi-rust/}).click();
    await button('编写代码',home).click();
    await waitUntil(async () => (await input.inputValue()).includes('帮我实现一个功能'), 'suggestion did not fill draft');
    assert.equal(creates().length,0);
    await button('选择供应商与模型',home).click();
    await page.locator('#composer-model-picker .composer-model-option').filter({hasText:'DeepSeek V4 Pro'}).click();
    await button('思考等级',home).click();
    await page.locator('#composer-thinking-picker').getByRole('button',{name:'中',exact:true}).click();
    await button('附件与更多',home).click();
    await page.getByLabel('选择本地附件，可多选').setInputFiles({name:'home-note.txt',mimeType:'text/plain',buffer:Buffer.from('hello from attachment')});
    await page.getByRole('dialog',{name:'附件与更多'}).waitFor();
    await waitUntil(async () => await home.getByText('home-note.txt',{exact:true}).isVisible(), 'attachment missing');
    await page.keyboard.press('Escape');
    const old = page.locator('[data-session-id=dark] .session-item').first();
    await old.click();
    await page.locator('.composer-main textarea').fill('历史会话草稿');
    await page.locator('.home-nav').click();
    await home.waitFor();
    assert.match(await input.inputValue(),/帮我实现一个功能/);
    assert.match(await home.locator('.composer-model-picker').textContent(),/DeepSeek V4 Pro/);
    assert.match(await home.locator('.composer-thinking-select').textContent(),/中/);
    fixture.controls.failSession=true;
    await submit.click();
    await home.getByRole('alert').filter({hasText:'创建会话失败'}).waitFor();
    assert.match(await input.inputValue(),/帮我实现一个功能/);
    assert.equal(prompts().length,0);
    fixture.controls.failSession=false;
    await input.press('Enter');
    await waitUntil(() => prompts().length===1,'first prompt not sent');
    assert.equal(creates().length,2);
    assert.equal(creates().at(-1).body.projectId,'pi-rust');
    assert.match(JSON.stringify(prompts()[0].body),/hello from attachment/);
    assert(sent.some(frame => frame.type==='execute' && frame.payload.type==='set_thinking_level' && frame.payload.level==='medium'));
    await old.click();
    await waitUntil(async () => (await page.locator('.composer-main textarea').inputValue())==='历史会话草稿','old draft lost');
    await page.locator('.composer-main textarea').press('Control+j');
    await home.waitFor();
    assert.equal(await input.inputValue(),'');
    await page.goto(new URL('#workspace', process.env.WORK_UI_URL).href);
    await page.reload();
    await page.locator('.composer-main textarea').waitFor();
    assert.equal(await home.count(), 0, '#workspace opens the conversation');
    await page.goto(new URL('#home', process.env.WORK_UI_URL).href);
    await page.reload();
    await home.waitFor();
    await page.goto(process.env.WORK_UI_URL);
    await page.reload();
    await home.waitFor();
    assert.deepEqual(errors,[]);
    console.log('PASS browser fullscreen, sidebar, real project/model/thinking, attachments, failed creation, first send, scoped draft recovery and entry hashes');
  } catch (error) {
    console.log(sent.filter(f=>['execute','reserve'].includes(f.type)).map(f=>[f.type,f.sessionId,f.payload,f.operation?.kind]));
    console.log(fixture.requests.filter(r => /new-/.test(r.path)).map(r => [r.method,r.path,r.body]));
    await page.screenshot({path:'artifacts/home-clarity-failure.png'});
    throw error;
  } finally { await browser.close(); await fixture.close(); }
})().catch(e=>{console.error(e);process.exitCode=1;});
