// Isolated protected nodes verify password-only connection and credential scope.
const fs = require('node:fs'), assert = require('node:assert/strict');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');
(async () => {
  const first = await startFixture(0, { authRequired: true });
  const second = await startFixture(0, { authRequired: true });
  const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
  const page = await browser.newPage({ viewport: { width: 1440, height: 1000 } }), results = [];
  page.setDefaultTimeout(10000);
  const pass = name => { results.push(name); console.log('PASS', name); };
  const posts = node => node.requests.filter(r => r.method === 'POST' && r.path === '/api/v1/auth');
  try {
    await page.goto(process.env.WORK_UI_URL || 'http://127.0.0.1:14180/');
    await page.getByRole('button', { name: '打开设置', exact: true }).click();
    await page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '执行节点', exact: true }).click();
    await page.getByRole('button', { name: '连接其他节点', exact: true }).click();
    const address = page.getByLabel('节点地址', { exact: true });
    const password = page.getByLabel('节点密码', { exact: true });
    const connect = page.getByRole('button', { name: '连接节点', exact: true });
    const login = page.getByRole('button', { name: '登录', exact: true });
    const status = text => page.locator('.server-details [role=status]').filter({ hasText: text }).waitFor();
    assert.equal(await page.locator('#node-token,#phone-node-token,#scanned-node-token').count(), 0);
    assert.equal(await password.count(), 1);
    assert.equal(await password.getAttribute('autocomplete'), 'current-password');
    pass('the former Token position contains one password field before connecting');

    await address.fill(first.url);
    await connect.click(); await status('需要节点密码');
    assert.equal(await password.count(), 1);
    await password.fill('test-only'); await address.fill(second.url);
    assert.equal(await password.inputValue(), '');
    assert(await login.isDisabled());
    assert.equal(posts(first).length, 0); assert.equal(posts(second).length, 0);
    pass('editing an address clears the password without sending it to either node');

    await connect.click(); await status('需要节点密码');
    await password.fill('test-only'); await login.click(); await status('在线');
    assert.equal(posts(first).length, 0); assert.equal(posts(second).length, 1);
    assert(second.requests.some(r => r.method === 'GET' && r.path === '/api/v1/auth' && r.authenticated));
    assert.equal(await password.inputValue(), '');
    pass('a password challenge reuses the same field and authoritative cookie authentication');

    await connect.click(); await status('在线');
    assert.equal(posts(second).length, 1);
    assert(second.requests.some(r => r.path === '/api/v1/projects' && r.authenticated));
    pass('reconnecting retains the selected node cookie without resending a password');

    await address.fill(first.url); await password.fill('incorrect-test-only');
    await connect.click(); await status('登录失败');
    assert.equal(posts(first).length, 1); assert.equal(posts(second).length, 1);
    assert.equal(await password.inputValue(), '');
    pass('an incorrect password entered before connection is cleared and can be retried');

    await password.fill('test-only'); await connect.click(); await status('在线');
    assert.equal(posts(first).length, 2); assert.equal(posts(second).length, 1);
    assert.equal(posts(first)[1].body.password, 'test-only');
    assert.equal(await password.inputValue(), '');
    assert(first.requests.some(r => r.path === '/api/v1/projects' && r.authenticated));
    assert(first.requests.some(r => r.path === '/api/v1/sessions' && r.authenticated));
    const saved = await page.evaluate(() => localStorage.getItem('rpi-work-nodes-v1'));
    assert(!saved.includes('test-only'));
    await page.screenshot({ path: 'artifacts/password-node-connection.png', animations: 'disabled' });
    pass('entering a password then Connect authenticates and loads the workspace in one step without storing the password');
  } catch (error) {
    await page.screenshot({ path: 'artifacts/ui-auth-scope-failure.png' }); throw error;
  } finally {
    fs.writeFileSync('artifacts/ui-auth-scope-validation.json', JSON.stringify({ results }, null, 2));
    await browser.close(); await first.close(); await second.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
