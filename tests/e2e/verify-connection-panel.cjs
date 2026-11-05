const fs = require('node:fs');
const path = require('node:path');
const http = require('node:http');
const assert = require('node:assert/strict');
const root = path.resolve(__dirname, '../..');
const bundle = path.join(root, 'target/parity-connection-panel-web');
const artifacts = path.join(root, 'target/connection-panel-review');

function sourceContracts() {
  for (const file of ['apps/work-ui/src/app/mobile_entry.rs', 'apps/work-ui/src/app/entry.rs', 'apps/work-ui/src/features/settings/devices.rs']) {
    const source = fs.readFileSync(path.join(root, file), 'utf8');
    assert(/ConnectionScreen\s*[{]/.test(source), `${file} must mount the complete shared screen`);
    assert(!/(ConnectionTabs|PasswordConnectionForm|ConnectionScanner)\s*[{]/.test(source), `${file} must not reassemble a separate screen`);
    assert(!/id:\s*"phone-(manual-connection|node-address|node-password)"/.test(source));
  }
}

async function serve() {
  const types = {'.html':'text/html; charset=utf-8', '.js':'text/javascript', '.wasm':'application/wasm'};
  const server = http.createServer((req, res) => {
    const url = new URL(req.url, 'http://localhost');
    const name = url.pathname === '/' ? 'index.html' : url.pathname.slice(1);
    // Only generated harness files; never proxy production APIs or fall back to an app server.
    const generatedSnippet = /^snippets\/[a-zA-Z0-9_-]+\/(?:[a-zA-Z0-9_-]+\/)*[a-zA-Z0-9_-]+\.js$/.test(name);
    if (!['index.html', 'work-ui.js', 'work-ui_bg.wasm'].includes(name) && !generatedSnippet) { res.writeHead(404).end(); return; }
    fs.readFile(path.join(bundle, name), (error, data) => {
      if (error) { res.writeHead(404).end(); return; }
      res.writeHead(200, {'content-type':types[path.extname(name)], 'cache-control':'no-store'}).end(data);
    });
  });
  await new Promise((resolve, reject) => { server.once('error', reject); server.listen(0, '127.0.0.1', resolve); });
  return {url:`http://127.0.0.1:${server.address().port}/`, close:()=>new Promise(resolve=>server.close(resolve))};
}

(async () => {
  sourceContracts();
  for (const name of ['index.html', 'work-ui.js', 'work-ui_bg.wasm']) {
    assert(fs.existsSync(path.join(bundle, name)), `Missing harness bundle ${name}; run powershell.exe -NoProfile -File tools/review/build-connection-panel.ps1`);
  }
  const {chromium} = require('../support/browser.cjs');
  fs.mkdirSync(artifacts, {recursive:true});
  const server = await serve();
  let browser;
  const errors = [], requests = [], results = [], diagnostics = [];
  let page;
  let failure = null;
  try {
    browser = await chromium.launch({headless:true, executablePath:process.env.CHROME_PATH});
    page = await browser.newPage();
    await page.route('**/*', route=>route.request().url().startsWith(server.url) ? route.continue() : route.abort());
    page.on('pageerror', error=>errors.push(error.message));
    page.on('requestfailed', request=>diagnostics.push({url:request.url(), failure:request.failure()}));
    page.on('console', message=>{ if (message.type() === 'error') diagnostics.push({console:message.text()}); });
    page.on('request', request=>requests.push(request.url()));
    const button = name=>page.getByRole('button', {name, exact:true});
    const panel = page.locator('#review-panel');
    const state = page.locator('#review-state');
    const assertLayout = async () => {
      assert(await page.evaluate(()=>document.documentElement.scrollWidth <= innerWidth), 'document overflow');
      const failures = await panel.locator('button,input:not([type=file]),label.button').evaluateAll(elements=>elements.filter(el=>el.getClientRects().length).flatMap(el=>{
        const box=el.getBoundingClientRect();
        return box.width < 44 || box.height < 44 || box.left < -1 || box.right > innerWidth+1 ? [`${el.id || el.textContent}: ${box.width}x${box.height}`] : [];
      }));
      assert.deepEqual(failures, [], 'visible controls must fit and have 44px targets');
    };
    const screenshot = async name => {
      // Never capture even a synthetic password. Do not silently clear it to hide a failure.
      for (const input of await page.locator('input[type=password]').all()) assert.equal(await input.inputValue(), '');
      await page.screenshot({path:path.join(artifacts, `${name}.png`), animations:'disabled'});
    };
    const backgrounds = new Map();
    const nativeScreens = new Map();
    const compareScreen = async key => {
      await page.waitForTimeout(250); // Compare settled theme transitions, not intermediate colors.
      // Hidden file inputs are adapter plumbing. Compare all visible UI and geometry.
      const value = await panel.evaluate(root => [...root.querySelectorAll('h1,p,strong,code,button,input:not([type=file])')]
        .filter(el => el.getClientRects().length).map(el => {
          const box=el.getBoundingClientRect(), css=getComputedStyle(el);
          return {tag:el.tagName,text:el.textContent,value:el.value,disabled:el.disabled,readonly:el.readOnly,
            rect:[box.x,box.y,box.width,box.height].map(v=>Math.round(v*10)/10),font:css.font,color:css.color,background:css.backgroundColor};
        }));
      assert(value.every(item => item.tag !== 'INPUT' || item.value === '' || item.value.startsWith('http')), 'never record passwords');
      if ((await state.getAttribute('data-adapter')) === 'native') nativeScreens.set(key,value);
      else assert.deepEqual(value,nativeScreens.get(key),`native/Web visible screen differs: ${key}`);
    };
    for (const adapter of ['native','web']) for (const width of [320,390,768]) for (const mode of ['light','dark']) {
      await page.setViewportSize({width,height:900});
      await page.goto(server.url + `?adapter=${adapter}`);
      await page.locator(`#review-state[data-adapter="${adapter}"]`).waitFor();
      await page.locator('#phone-manual-connection').waitFor();
      await page.evaluate(mode=>{document.documentElement.dataset.theme='paper';document.documentElement.dataset.mode=mode;}, mode);
      await page.getByLabel('电脑节点地址', {exact:true}).fill('https://computer.example:13030');
      assert.equal(await page.getByLabel('电脑节点密码', {exact:true}).getAttribute('type'), 'password');
      assert(await button('连接电脑').isEnabled());
      await assertLayout();
      const background = await panel.evaluate(el=>getComputedStyle(el.querySelector('input')).backgroundColor);
      assert(background && background !== 'rgba(0, 0, 0, 0)', 'shared theme must style inputs');
      if (mode === 'light') backgrounds.set(`${adapter}-${width}`, background);
      else assert.notEqual(background, backgrounds.get(`${adapter}-${width}`), 'dark theme must change shared input styling');
      await compareScreen(`manual-${width}-${mode}`);
      await screenshot(`${adapter}-manual-${width}-${mode}`);
      await button('扫码连接').click();
      await button('打开相机').waitFor();
      assert.equal(await panel.locator('#phone-manual-connection:visible').count(), 0);
      await assertLayout();
      await compareScreen(`scan-${width}-${mode}`);
      await screenshot(`${adapter}-scan-${width}-${mode}`);
      await button('手动输入地址').click();
      assert.equal(await page.locator('#phone-node-address').inputValue(), 'https://computer.example:13030');
      await button('Review hosted').click();
      await page.getByLabel('节点密码', {exact:true}).waitFor();
      assert.equal(await button('扫码连接').count(), 0, 'fixed hosted mode must not offer arbitrary remote targets');
      const fixedAddress = panel.locator('#phone-node-address');
      if (await fixedAddress.count()) {
        assert(await fixedAddress.isHidden() || await fixedAddress.isDisabled() || await fixedAddress.getAttribute('readonly') !== null);
        assert.equal(await fixedAddress.inputValue(), new URL(server.url).origin);
      } else {
        assert((await panel.textContent()).includes(new URL(server.url).origin), 'show fixed same-origin target');
      }
      await assertLayout();
      await compareScreen(`hosted-${width}-${mode}`);
      await screenshot(`${adapter}-hosted-${width}-${mode}`);
      await button('Review hosted').click();
      await button('Review connected').click();
      await panel.locator('.phone-current-node').waitFor();
      assert.equal(await panel.locator('form').count(),0);
      await compareScreen(`connected-${width}-${mode}`);
      await assertLayout();
      await screenshot(`${adapter}-connected-${width}-${mode}`);
      await button('分享当前连接').click();
      assert((await state.textContent()).includes('share:1'));
      await button('连接其他电脑').click();
      await page.locator('#phone-node-address').waitFor();
      assert((await state.textContent()).includes('switch:1'));
      await button('Review remembered').click();
      await button('恢复上次连接').click();
      assert((await state.textContent()).includes('resume:1'));
      await compareScreen(`remembered-${width}-${mode}`);
      await button('忘记已保存的电脑').click();
      assert.equal(await button('恢复上次连接').count(),0);
      await button('Review connected').click();
      await button('退出并清除登录').click();
      assert((await state.textContent()).includes('forget:2'));
      await page.locator('#phone-node-address').waitFor();
      results.push(`${adapter}/${width}/${mode}: identical visible screen; manual/scan/fixed-host/connected/restore/switch/exit, 44px, no overflow`);
    }
    for (const adapter of ['native','web']) {
    await page.goto(server.url + `?adapter=${adapter}`);
    await page.locator(`#review-state[data-adapter="${adapter}"]`).waitFor();
    await page.getByLabel('电脑节点地址', {exact:true}).fill('https://computer.example:13030');
    await button('连接电脑').click();
    await page.waitForFunction(()=>document.querySelector('#review-state').textContent.includes('submit:1'));
    await button('Review busy').click();
    assert(await panel.locator('button[type=submit]').isDisabled());
    await button('取消连接').click();
    assert(await panel.locator('button[type=submit]').isEnabled());
    await page.locator('#phone-node-password').fill('synthetic-test-only');
    await page.locator('#phone-node-password').press('Enter');
    await page.waitForFunction(()=>document.querySelector('#review-state').textContent.includes('submit:2'));
    await page.locator('#phone-node-password').fill('');
    await button('Review error').click();
    await panel.getByRole('alert').filter({hasText:'测试连接错误'}).waitFor();
    await button('Review error').click();
    await button('扫码连接').click();
    await button('打开相机').click();
    await button('关闭相机').waitFor();
    await button('关闭相机').click();
    await button('打开相机').waitFor();
    if (adapter === 'native') await button('相册扫码').click();
    else {
      const picker = page.waitForEvent('filechooser');
      await button('相册扫码').click();
      await (await picker).setFiles({name:'synthetic.png', mimeType:'image/png', buffer:Buffer.from('synthetic callback only')});
    }
    await page.waitForFunction(()=>document.querySelector('#review-state').textContent.includes('album:1'));
    await button('Review scanned target').click();
    await button('连接这台电脑').click();
    await page.waitForFunction(()=>document.querySelector('#review-state').textContent.includes('confirm:1'));
    await button('Review busy').click();
    assert(await button('连接这台电脑').isDisabled());
    await button('Review busy').click();
    const cancelledBefore = Number((await state.textContent()).match(/cancel:(\d+)/)[1]);
    await panel.getByRole('button', {name:/取消|返回/}).click();
    await page.waitForFunction(expected=>document.querySelector('#review-state').textContent.includes(`cancel:${expected}`), cancelledBefore+1);
    assert.equal(await panel.getByRole('button', {name:'连接这台电脑',exact:true}).count(), 0);
    await button('手动输入地址').click();
    await button('Review hosted').click();
    await page.getByLabel('节点密码', {exact:true}).waitFor();
    const address = page.locator('#phone-node-address');
    if (await address.count()) assert(await address.isHidden() || await address.isDisabled() || await address.getAttribute('readonly') !== null, 'hosted address must not be editable');
    assert(await button('登录节点').isEnabled());
    await button('登录节点').click();
    await page.waitForFunction(()=>document.querySelector('#review-state').textContent.includes('submit:3'));
    await assertLayout();
    await screenshot(`${adapter}-fixed-hosted`);
    results.push(`${adapter}: submit/Enter, busy/error, camera toggle, album adapter, confirm/cancel, fixed-host callbacks`);
    }
    assert.deepEqual(errors, []);
    assert(requests.every(url=>url.startsWith(server.url) || url.startsWith('data:')), 'harness must not make external or node API requests');
    results.push('synthetic submit/Enter, busy, error, camera toggle, confirm, cancel and fixed-hosted callbacks');
    console.log('PASS shared connection panel browser harness', results);
  } catch (error) {
    failure = {name:error.name, message:error.message};
    if (page) diagnostics.push(await page.evaluate(()=>({url:location.href, forms:document.querySelectorAll('form').length, main:document.querySelector('#main')?.childElementCount})));
    console.error('Harness diagnostics:', diagnostics);
    throw error;
  } finally {
    fs.writeFileSync(path.join(artifacts,'validation.json'), JSON.stringify({passed:failure === null, failure, scope:'Real shared Dioxus components; synthetic callbacks only; no native network, camera, authentication or physical-device validation',results,errors,diagnostics},null,2));
    if (browser) await browser.close();
    await server.close();
  }
})().catch(error=>{console.error(error);process.exitCode=1;});
