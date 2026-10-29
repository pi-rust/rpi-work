// Isolated browser/node tests for actual WASM connection and authentication.
const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');

(async () => {
  const fixture = await startFixture();
  const protectedNode = await startFixture(0, { authRequired: true });
  const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
  const context = await browser.newContext({ viewport: { width: 1280, height: 1000 } });
  const results = [], errors = [];
  const pass = name => { results.push(name); console.log('PASS', name); };
  const app = process.env.WORK_UI_URL || 'http://127.0.0.1:4180/';
  await context.route('https://api.github.com/**', route => route.fulfill({ status: 404, body: '{}', headers: { 'access-control-allow-origin': '*' } }));
  const page = await context.newPage();
  page.on('pageerror', error => errors.push(error.message));
  page.setDefaultTimeout(8000);
  const status = () => page.locator('.server-details [role=status]');
  const openNodes = async () => {
    await page.getByRole('button', { name: '打开设置', exact: true }).click();
    await page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '执行节点', exact: true }).click();
  };
  const connect = async url => {
    if (!await page.locator('#node-url').isVisible()) await page.getByRole('button', { name: '连接其他节点', exact: true }).click();
    await page.getByLabel('节点地址', { exact: true }).fill(url);
    await page.getByRole('button', { name: '连接节点', exact: true }).click();
  };
  const saved = () => page.evaluate(() => JSON.parse(localStorage.getItem('rpi-work-nodes-v1') || '{}').recent || []);
  const waitSaved = url => page.waitForFunction(address => JSON.parse(localStorage.getItem('rpi-work-nodes-v1') || '{}').recent?.[0] === address.replace(/\/$/, ''), url);
  try {
    await page.goto(app);
    await openNodes();
    assert.equal(await status().innerText(), '未连接');
    assert.doesNotMatch(await page.locator('.devices-page').innerText(), /服务就绪|短时邀请|填写设备名|已授权的手机/);
    pass('an unprobed desktop node has no fabricated online or pairing state');

    await connect(protectedNode.url);
    await status().filter({ hasText: '需要节点密码' }).waitFor();
    assert.deepEqual(await saved(), []);
    await page.getByLabel('节点密码', { exact: true }).fill('incorrect-test-only');
    await page.getByRole('button', { name: '登录', exact: true }).click();
    await status().filter({ hasText: '登录失败' }).waitFor();
    assert.equal(await page.getByLabel('节点密码', { exact: true }).inputValue(), '');
    assert.deepEqual(await saved(), []);
    pass('a password challenge or rejected password is never saved as a successful node');

    await page.getByLabel('节点密码', { exact: true }).fill('test-only');
    await page.getByRole('button', { name: '登录', exact: true }).click();
    await status().filter({ hasText: '在线' }).waitFor();
    await waitSaved(protectedNode.url);
    assert(protectedNode.requests.some(request => request.path === '/api/v1/projects' && request.authenticated));
    assert.equal(await page.getByLabel('节点密码', { exact: true }).count(), 0);
    pass('real cookie authentication loads projects and sessions immediately without a second Connect');

    await page.reload();
    await openNodes();
    await status().filter({ hasText: '在线' }).waitFor();
    await page.getByRole('region', { name: '最近连接的节点' }).getByText(protectedNode.url.replace(/\/$/, ''), { exact: true }).waitFor();
    pass('a reload reconnects the saved node using the existing browser session');

    await context.clearCookies();
    await page.reload();
    await openNodes();
    await status().filter({ hasText: '需要节点密码' }).waitFor();
    await page.getByLabel('节点密码', { exact: true }).waitFor();
    pass('an expired login reconnects to a password challenge instead of pretending to be online');

    await connect(fixture.url);
    await status().filter({ hasText: '在线' }).waitFor();
    await waitSaved(fixture.url);
    assert.equal((await saved()).length, 2);
    const actualAddress = await page.locator('.node-address code').innerText();
    await page.getByLabel('节点地址', { exact: true }).fill('https://user:secret@example.test');
    await page.getByRole('button', { name: '连接节点', exact: true }).click();
    await page.getByRole('alert').filter({ hasText: '不包含' }).waitFor();
    assert.equal(await page.locator('.node-address code').innerText(), actualAddress);
    assert.match(await status().innerText(), /在线/);
    assert.equal((await saved()).length, 2);
    pass('an invalid address does not clear the working node or leak into recent history');

    let delayedRoute;
    await page.route(protectedNode.url + 'api/v1/health', route => { delayedRoute = route; });
    await connect(protectedNode.url);
    await page.waitForFunction(() => document.querySelector('.server-details [role=status]')?.textContent.includes('正在连接'));
    for (let index = 0; index < 100 && !delayedRoute; index++) await new Promise(resolve => setTimeout(resolve, 10));
    assert(delayedRoute);
    await connect(fixture.url);
    await status().filter({ hasText: '在线' }).waitFor();
    await delayedRoute.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ ok: false }) }).catch(() => {});
    await page.unroute(protectedNode.url + 'api/v1/health');
    await page.waitForTimeout(150);
    assert.match(await status().innerText(), /在线/);
    assert.equal(await page.locator('.node-address code').innerText(), fixture.url);
    await waitSaved(fixture.url);
    pass('a late failed response from a previous node cannot replace the current successful connection');

    await page.route(fixture.url + 'api/v1/projects', route => route.fulfill({ status: 503, contentType: 'application/json', body: '{"error":"test-only project failure"}' }));
    await page.getByRole('button', { name: '清除连接记录', exact: true }).click();
    await connect(fixture.url);
    await status().filter({ hasText: '项目加载失败' }).waitFor();
    assert.deepEqual(await saved(), []);
    await page.unroute(fixture.url + 'api/v1/projects');
    pass('a project loading failure is reported as failure and is not recorded as ready');

    let stalled;
    await page.route(fixture.url + 'api/v1/health', route => { stalled = route; });
    await connect(fixture.url);
    await status().filter({ hasText: '连接超时' }).waitFor({ timeout: 18000 });
    await stalled?.abort().catch(() => {});
    await page.unroute(fixture.url + 'api/v1/health');
    await connect(fixture.url);
    await status().filter({ hasText: '在线' }).waitFor();
    await waitSaved(fixture.url);
    pass('an unresponsive node times out and a subsequent retry succeeds');

    await page.getByRole('button', { name: '清除连接记录', exact: true }).click();
    await page.waitForFunction(() => JSON.parse(localStorage.getItem('rpi-work-nodes-v1') || '{}').recent?.length === 0);
    assert.match(await status().innerText(), /在线/);
    const countBeforeReload = fixture.requests.filter(request => request.path === '/api/v1/health').length;
    await page.reload(); await openNodes();
    assert.equal(await status().innerText(), '未连接');
    assert.equal(fixture.requests.filter(request => request.path === '/api/v1/health').length, countBeforeReload);
    pass('clearing history keeps the live connection and prevents automatic reconnect after reopening');

    await connect(fixture.url);
    await status().filter({ hasText: '在线' }).waitFor();
    await page.screenshot({ path: 'artifacts/node-lifecycle-desktop.png', fullPage: true, animations: 'disabled' });
    await page.setViewportSize({ width: 320, height: 844 });
    await page.locator('.devices-page').getByRole('heading', { name: '连接我的电脑', exact: true }).waitFor();
    assert.equal(await page.locator('#node-url,.node-history').count(), 0);
    assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
    assert.equal(await page.getByRole('button', { name: '手动输入地址', exact: true }).getAttribute('aria-expanded'), 'false');
    pass('narrow-screen connection UI retains scanning with a collapsed manual address entry');

    const blocked = await context.newPage();
    blocked.on('pageerror', error => errors.push(error.message));
    await blocked.addInitScript(() => {
      const save = Storage.prototype.setItem;
      Storage.prototype.setItem = function (key, value) {
        if (key === 'rpi-work-nodes-v1') throw new Error('test-only storage rejection');
        return save.call(this, key, value);
      };
    });
    await blocked.goto(app);
    await blocked.getByRole('button', { name: '打开设置', exact: true }).click();
    await blocked.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '执行节点', exact: true }).click();
    await blocked.locator('.server-details [role=status]').filter({ hasText: '在线' }).waitFor();
    await blocked.getByText('无法保存连接地址；本次连接仍可使用。', { exact: true }).waitFor();
    await blocked.close();
    pass('a storage failure warns without preventing a successful connection');

    assert.deepEqual(errors, []);
    pass('no browser application errors');
  } catch (error) {
    await page.screenshot({ path: 'artifacts/node-lifecycle-failure.png', fullPage: true }).catch(() => {});
    throw error;
  } finally {
    fs.writeFileSync('artifacts/node-lifecycle-validation.json', JSON.stringify({ scope: 'Actual Dioxus WASM, isolated HTTP fixtures, real browser cookie authentication; no physical camera test', results, errors }, null, 2));
    await browser.close(); await fixture.close(); await protectedNode.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
