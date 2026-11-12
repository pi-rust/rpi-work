// Startup selection: real WASM + isolated HTTP/SSE node, no prompts or creation.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');
const UI = process.env.WORK_UI_URL || 'http://127.0.0.1:14180/';
const wait = ms => new Promise(resolve => setTimeout(resolve, ms));
(async () => {
  fs.mkdirSync('artifacts', { recursive: true });
  const fixture = await startFixture(0);
  // Non-loopback node identity exercises phone address policy; every API call
  // is intercepted below and routed to the isolated loopback fixture, never DNS.
  const nodeAddress = `http://fixture.test:${new URL(fixture.url).port}`;
  const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
  const errors = [];
  async function scenario({ query = '', empty = false, hidden = false, manual = false, phone = false } = {}) {
    const context = await browser.newContext({ viewport: { width: phone ? 390 : 1440, height: 900 } });
    const page = await context.newPage();
    page.setDefaultTimeout(15000);
    page.on('pageerror', error => errors.push(error.message));
    await page.addInitScript(({ address, hidden, manual }) => {
      localStorage.setItem('rpi-work-appearance-v1', JSON.stringify({ theme: 'codex', mode: 'light', density: 'standard' }));
      if (!manual) localStorage.setItem('rpi-work-nodes-v1', JSON.stringify({ version: 1, recent: [address], selected: address }));
      if (hidden) localStorage.setItem('rpi-work-archived-projects-v1', JSON.stringify([`${address}::project::rpi-work`]));
    }, { address: nodeAddress, hidden, manual });
    await page.route('https://api.github.com/**', route => route.fulfill({ status: 404, json: {} }));
    await page.route('**/api/**', async route => {
      const request = route.request(), url = new URL(request.url());
      if (url.pathname === '/api/v1/sessions' && request.method() === 'GET') {
        const response = await route.fetch({ url: new URL(url.pathname + url.search, fixture.url).href });
        const body = await response.json();
        if (empty) body.sessions = [];
        else {
          for (const session of body.sessions) {
            session.updatedAt = session.sessionId === 'mobile' ? 300 : session.sessionId === 'rpc' ? 200 : 100;
            session.pinned = session.sessionId === 'dark';
          }
          // The newest entry is archived and must not win automatic selection.
          body.sessions.find(session => session.sessionId === 'dark').archived = true;
        }
        return route.fulfill({ response, json: body });
      }
      if (url.origin !== new URL(fixture.url).origin) {
        return route.continue({ url: new URL(url.pathname + url.search, fixture.url).href });
      }
      return route.continue();
    });
    await page.goto(UI + query);
    return { page, context };
  }
  const loaded = async (page, id) => {
    try { await page.locator(`[data-history-id="user-${id}"]`).waitFor(); }
    catch (error) {
      await page.screenshot({ path: 'artifacts/default-session-failure.png', animations: 'disabled' });
      console.error((await page.locator('body').innerText()).slice(-2500), fixture.requests.slice(-12));
      throw error;
    }
    await page.locator('.composer textarea:not(:disabled)').waitFor();
    assert(await page.locator('.transcript').getAttribute('data-session-scope'));
  };
  try {
    let test = await scenario({ phone: true });
    await loaded(test.page, 'mobile');
    assert.equal(await test.page.locator('.composer textarea').evaluate(el => el === document.activeElement), false, 'automatic opening does not raise the mobile keyboard');
    await test.page.screenshot({ path: 'artifacts/default-session-mobile-390.png', animations: 'disabled' });
    await test.page.getByRole('button', { name: '切换会话', exact: true }).click();
    await test.page.locator('#session-switcher [data-session-id="architecture"]').click();
    await loaded(test.page, 'architecture');
    await wait(3500); // Background catalog refresh must never reopen the default.
    assert.equal(await test.page.locator('[data-history-id="user-architecture"]').count(), 1);
    await test.page.reload();
    await loaded(test.page, 'mobile');
    await test.context.close();
    console.log('PASS remembered-node startup opens latest unarchived session without focus; manual switching persists through refresh; reload opens normally');

    test = await scenario({ query: '?session=architecture' });
    await loaded(test.page, 'architecture');
    assert.equal(await test.page.locator('[data-history-id="user-mobile"]').count(), 0, 'explicit session link outranks recency');
    await test.context.close();
    console.log('PASS explicit session links stay authoritative over default selection');

    test = await scenario({ query: '?session=missing-session' });
    await test.page.getByText('节点已连接，但会话链接不存在或已被删除。', { exact: true }).first().waitFor();
    assert.equal(await test.page.locator('.transcript [data-history-id]').count(), 0, 'missing explicit link must not silently open a different conversation');
    await test.context.close();
    console.log('PASS missing explicit links report failure rather than silently opening another session');

    test = await scenario({ hidden: true });
    await loaded(test.page, 'rpc');
    await test.context.close();
    console.log('PASS archived projects are skipped; selection remains within the connected node');

    test = await scenario({ empty: true });
    await test.page.locator('[data-project-id="rpi-work"] .project-toggle').waitFor();
    await wait(400);
    assert.equal(await test.page.locator('.transcript [data-history-id]').count(), 0);
    await test.context.close();
    console.log('PASS empty nodes retain an empty workspace without creating or executing a session');

    test = await scenario({ manual: true });
    await test.page.getByRole('button', { name: '打开设置', exact: true }).click();
    await test.page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '工作协同', exact: true }).click();
    await test.page.getByRole('button', { name: '连接其他节点', exact: true }).click();
    await test.page.getByLabel('节点地址', { exact: true }).fill(nodeAddress);
    await test.page.getByRole('button', { name: '连接节点', exact: true }).click();
    await test.page.getByRole('status').filter({ hasText: '在线' }).first().waitFor();
    await wait(300);
    await test.page.getByRole('button', { name: '返回会话', exact: true }).click();
    await loaded(test.page, 'mobile');
    await test.context.close();
    console.log('PASS connecting in settings selects a default without taking over the current settings page');
    assert.equal(fixture.requests.filter(request => request.method === 'POST' && request.path === '/api/v1/sessions').length, 0, 'no automatic session creation');
    assert.equal(fixture.requests.filter(request => request.path.endsWith('/commands') && !request.body.type?.startsWith('get_')).length, 0, 'no model/prompt/queue commands');
    assert.deepEqual(errors, []);
  } finally { await browser.close(); await fixture.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
