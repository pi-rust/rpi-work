// Isolated node only: directory browsing requires a ready connection and recovers from 401.
const fs = require('node:fs');
const assert = require('node:assert/strict');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');
(async () => {
  const node = await startFixture(0, { authRequired: true });
  const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
  const page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
  const results = [], errors = [];
  page.setDefaultTimeout(8000);
  page.on('pageerror', error => errors.push(error.message));
  const pass = name => { results.push(name); console.log('PASS', name); };
  const add = () => page.getByRole('button', { name: '添加项目', exact: true });
  const picker = () => page.getByRole('dialog', { name: '添加项目', exact: true });
  const folders = () => page.getByRole('dialog', { name: '选择项目文件夹', exact: true });
  const online = () => page.locator('.server-details [role=status]').filter({ hasText: '在线' }).waitFor();
  const login = async () => {
    await page.getByLabel('节点密码', { exact: true }).fill('test-only');
    await page.getByRole('button', { name: '登录', exact: true }).click();
    await online();
  };
  const openPicker = async () => {
    await page.getByRole('button', { name: '返回会话', exact: true }).click();
    await add().click();
    await picker().waitFor();
    await picker().getByRole('button', { name: /^浏览/ }).click();
    await folders().waitFor();
  };
  try {
    await page.goto(process.env.WORK_UI_URL || 'http://127.0.0.1:4180/');
    await add().click();
    await page.locator('.devices-page').waitFor();
    assert.equal(await picker().count(), 0);
    assert.equal(node.requests.filter(request => request.path === '/api/v1/project-folders').length, 0);
    pass('adding a project before connecting opens connection settings without a directory request');

    await page.getByRole('button', { name: '连接其他节点', exact: true }).click();
    await page.getByLabel('节点地址', { exact: true }).fill(node.url);
    await page.getByRole('button', { name: '连接节点', exact: true }).click();
    await page.getByLabel('节点密码', { exact: true }).waitFor();
    await add().click();
    assert.equal(await picker().count(), 0);
    assert.equal(node.requests.filter(request => request.path === '/api/v1/project-folders').length, 0);
    pass('a password challenge cannot open the project directory picker');

    await login();
    await openPicker();
    await folders().locator('.project-folder-list').waitFor();
    assert(node.requests.some(request => request.path === '/api/v1/project-folders' && request.authenticated));
    assert.equal(await folders().locator('.project-folder-toolbar code').innerText(), 'D:/Projects');
    assert(await folders().getByRole('button', { name: '上级目录', exact: true }).isDisabled());
    const projectRow = folders().locator('.project-folder-row').filter({ hasText: 'D:/Projects/rpi-work' });
    await projectRow.getByRole('button', { name: '进入此目录', exact: true }).click();
    await folders().locator('.project-folder-toolbar code').filter({ hasText: 'D:/Projects/rpi-work' }).waitFor();
    assert.equal(new URLSearchParams(node.requests.filter(request => request.path === '/api/v1/project-folders').at(-1).query).get('path'), 'rpi-work');
    await folders().getByRole('button', { name: '上级目录', exact: true }).click();
    await folders().locator('.project-folder-toolbar code').filter({ hasText: /^D:\/Projects$/ }).waitFor();
    await folders().locator('.project-folder-row').filter({ hasText: 'D:/Projects/rpi-work' }).locator('.project-folder-select').click();
    await folders().getByRole('button', { name: '使用此目录', exact: true }).click();
    assert.equal(await picker().getByLabel('已有项目目录', { exact: true }).inputValue(), 'D:/Projects/rpi-work');
    for (const request of node.requests.filter(request => request.path === '/api/v1/project-folders')) {
      const path = new URLSearchParams(request.query).get('path') || '';
      assert(!path.startsWith('/') && !/[\\:]/.test(path));
    }
    await picker().getByRole('button', { name: '取消', exact: true }).click();
    pass('authenticated browsing enters and leaves folders with relative API paths and returns an absolute selection');

    const pattern = node.url + 'api/v1/project-folders**';
    await page.route(pattern, async route => {
      const response = await route.fetch();
      await route.fulfill({ response, status: 401, json: { error: 'unauthorized' } });
    });
    await add().click();
    await picker().getByRole('button', { name: /^浏览/ }).click();
    await folders().getByRole('alert').filter({ hasText: '节点认证未完成或已失效' }).waitFor();
    assert.doesNotMatch(await folders().innerText(), /HTTP 401|unauthorized/);
    await folders().getByRole('button', { name: '连接或登录节点', exact: true }).click();
    await page.getByLabel('节点密码', { exact: true }).waitFor();
    assert.equal(await picker().count(), 0);
    assert.equal(await folders().count(), 0);
    pass('expired directory authorization shows a sign-in action and returns to authentication');

    await page.unroute(pattern);
    await login();
    await openPicker();
    await folders().locator('.project-folder-list').waitFor();
    await page.screenshot({ path: 'artifacts/project-auth-recovery.png', animations: 'disabled' });
    assert.equal(node.requests.filter(request => request.method === 'POST' && request.path === '/api/v1/projects').length, 0);
    assert.deepEqual(errors, []);
    pass('signing in again restores browsing without creating projects or calling a model');
  } finally {
    fs.writeFileSync('artifacts/project-auth-validation.json', JSON.stringify({ results, errors }, null, 2));
    await browser.close();
    await node.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
