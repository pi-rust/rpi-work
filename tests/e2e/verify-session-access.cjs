// Formal prebuilt WASM + isolated HTTP/WebSocket fixture. No build, live agent or user profile.
// Run: node tests/support/run-static-ui.cjs tests/e2e/verify-session-access.cjs
const assert = require('node:assert/strict');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');
const UI = process.env.WORK_UI_URL || 'http://127.0.0.1:14180/';
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
const results = [];
let browser;
async function until(check, label) {
  const deadline = Date.now() + 10000;
  while (Date.now() < deadline) { if (await check()) return; await delay(40); }
  throw new Error(`Timed out: ${label}`);
}
async function test(name, configure, verify) {
  let fixture, context, page;
  const requests = [], responses = [], external = [], errors = [];
  const proof = { name, status: 'FAIL' };
  const started = Date.now();
  try {
    fixture = await startFixture(0);
    configure(fixture.controls);
    context = await browser.newContext({ viewport: { width: 1280, height: 950 }, serviceWorkers: 'block' });
    page = await context.newPage(); page.setDefaultTimeout(10000);
    page.on('pageerror', error => errors.push(error.message));
    const uiOrigin = new URL(UI).origin, fixtureOrigin = new URL(fixture.url).origin;
    page.on('response', response => {
      if (new URL(response.url()).origin === fixtureOrigin) responses.push({ path: new URL(response.url()).pathname, status: response.status() });
    });
    const observations = [];
    await context.route('**/*', async route => {
      const request = route.request(), url = new URL(request.url());
      if (!['http:', 'https:'].includes(url.protocol)) return route.continue();
      if (url.origin === 'https://api.github.com') return route.fulfill({ status: 404, json: {}, headers: { 'access-control-allow-origin': '*' } });
      if (url.origin === fixtureOrigin) {
        requests.push({ method: request.method(), path: url.pathname, body: request.method() === 'POST' ? request.postDataJSON() : null });
        if (url.pathname === '/api/v1/capabilities' || url.pathname.endsWith('/access')) {
          const response = await route.fetch();
          const data = await response.json();
          observations.push({ path: url.pathname, status: response.status(), data });
          return route.fulfill({ response, json: data });
        }
        return route.continue();
      }
      if (url.origin === uiOrigin && !url.pathname.startsWith('/api/') && ['GET', 'HEAD'].includes(request.method())) return route.continue();
      external.push(request.url()); return route.abort('blockedbyclient');
    });
    const h = {
      page, fixture, requests, responses, observations, proof,
      session: () => page.locator('[data-session-id="dark"] .session-item'),
      field: () => page.getByRole('textbox', { name: '发送给 agent 的消息', exact: true }),
      send: () => page.getByRole('button', { name: '发送消息', exact: true }),
      access: () => responses.filter(item => item.path === '/api/v1/sessions/dark/access'),
      async connect() {
        await page.getByRole('button', { name: '打开设置', exact: true }).click();
        await page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '执行节点', exact: true }).click();
        if (!await page.getByLabel('节点地址', { exact: true }).isVisible()) await page.getByRole('button', { name: '连接其他节点', exact: true }).click();
        await page.getByLabel('节点地址', { exact: true }).fill(fixture.url);
        await page.getByRole('button', { name: '连接节点', exact: true }).click();
        await until(async () => await page.locator('.server-details [role=status]').filter({ hasText: '在线' }).isVisible()
          || (!await page.getByRole('navigation', { name: '设置分类' }).isVisible() && await h.field().isVisible()), 'connected settings or auto-returned conversation');
        const back = page.getByRole('button', { name: '返回会话', exact: true });
        if (await back.isVisible()) await back.click();
        if (!await h.session().isVisible()) await page.locator('.project-row').filter({ hasText: 'rpi-work' }).click();
      },
      async select() { await h.session().click(); },
      async ready() {
        await h.field().waitFor();
        await page.waitForFunction(() => {
          const field = document.querySelector('.composer-main textarea');
          return field && !field.disabled && field.dataset.imeReady === 'true';
        });
      },
      writes() {
        return requests.filter(item => item.method === 'POST' && (['prompt', 'queue_message', 'cancel_queued', 'clear_queue', 'set_model', 'set_thinking_level', 'set_active_tools', 'set_api_key', 'ui_dialog_response'].includes(item.body?.type) || /\/(configuration|dialogs)$/.test(item.path)));
      },
      async noWrites() { await delay(250); assert.deepEqual(h.writes(), [], 'no prompt/queue/config/dialog/credential writes'); },
    };
    await page.goto(UI); await h.connect(); await verify(h);
    assert.deepEqual(external, [], 'only UI assets and isolated fixture allowed');
    assert.deepEqual(errors, [], 'no browser runtime errors');
    proof.status = 'PASS';
  } catch (error) {
    proof.error = error.stack || String(error);
    if (page) proof.ui = (await page.locator('body').innerText().catch(() => '')).slice(-5000);
  } finally {
    if (context) await context.close().catch(() => {});
    if (fixture) await fixture.close();
    proof.durationMs = Date.now() - started;
    proof.requests = requests; proof.responses = responses; proof.external = external; proof.pageErrors = errors;
    results.push(proof);
    console.log(JSON.stringify(proof, null, 2));
  }
}
(async () => {
  const watchdog = setTimeout(() => { console.error('FAIL: bounded suite deadline (150s)'); process.exit(1); }, 150000);
  try {
    browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
    for (const status of [404, 503]) {
      await test(`access ${status}: initial unselected, fail closed, reselect recovers`, controls => { controls.accessStatus = status; }, async h => {
        await h.select();
        await until(() => h.access().some(item => item.status === status), `access ${status}`);
        await h.page.getByText(/读取会话能力失败/).first().waitFor();
        // Opening fails before selection. Do not force a nonexistent composer or pretend it was open.
        const fields = await h.field().count();
        h.proof.failedOpening = { fields, text: await h.page.locator('body').innerText() };
        if (fields) assert(await h.field().isDisabled(), 'failed initial selection cannot expose an enabled composer');
        assert.equal(h.requests.filter(item => item.path === '/api/v1/realtime' || /\/dark\/(agent-binding|configuration|commands)$/.test(item.path)).length, 0, 'no global-capability fallback / session reads after access failure');
        await h.noWrites();
        h.fixture.controls.accessStatus = 0;
        const reads = h.access().length;
        await h.select(); await h.ready();
        await until(() => h.access().length > reads && h.access().at(-1).status === 200, 'fresh successful access after explicit reselect');
        await h.field().fill('fixture recovery draft');
        await until(() => h.send().isEnabled(), 'recovered prompt UI enabled');
        assert(h.requests.some(item => item.path.endsWith('/dark/agent-binding')));
        await h.noWrites();
      });
    }
    await test('session lacks prompt despite global prompt; same-address reconnect refreshes access', controls => {
      controls.sessionAccessById = { dark: { supportedCommands: ['get_state', 'get_queue', 'get_available_models', 'set_model', 'get_available_thinking_levels', 'set_thinking_level', 'get_session_branch', 'ui_dialog_response'] } };
    }, async h => {
      await h.select(); await h.ready();
      assert(h.observations.some(item => item.path === '/api/v1/capabilities' && item.data.supportedCommands.includes('prompt')), 'global capabilities really include prompt');
      assert(h.observations.some(item => item.path === '/api/v1/sessions/dark/access' && !item.data.supportedCommands.includes('prompt')), 'session access really excludes prompt');
      await h.field().fill('must not dispatch');
      await until(() => h.send().isDisabled(), 'missing session prompt disables send');
      await h.field().press('Enter'); await h.send().evaluate(button => button.click()); await h.noWrites();
      const reads = h.access().length;
      h.fixture.controls.sessionAccessById = {};
      await h.connect(); await h.ready();
      await until(() => h.access().length > reads, 'same-address reconnect fresh access observed');
      await h.field().fill('refreshed capability draft');
      await until(() => h.send().isEnabled(), 'same-address refreshed capability enables prompt');
      h.proof.reconnect = { beforeAccessReads: reads, afterAccessReads: h.access().length, sendEnabled: await h.send().isEnabled() };
      await h.noWrites();
    });
    await test('dialogs=false: typed dialog submit and cancel cannot bypass session access', controls => {
      controls.sessionAccessById = { dark: { capabilities: { dialogs: false } } };
    }, async h => {
      await h.select(); await h.ready();
      for (const action of ['submit', 'cancel']) {
        const id = `access-${action}`;
        h.fixture.emit({ type: 'ui_dialog_request', requestId: id, toolCallId: `tool-${id}`, ui: { purpose: 'question', kind: 'input', question: 'Fixture-only typed question' } }, 'dark');
        const card = h.page.locator(`[data-request-id="${id}"]`); await card.waitFor();
        await card.locator('textarea').fill('fixture answer');
        const button = action === 'submit' ? card.locator('button[type="submit"]') : card.getByRole('button', { name: `${id} · 取消回答`, exact: true });
        await button.click();
        await h.page.getByText('当前会话不支持确认回复，或能力尚未确认。', { exact: true }).first().waitFor();
        await h.noWrites();
        assert(await card.isVisible(), 'unsupported dialog remains unresolved');
        h.proof.dialogUI ??= [];
        h.proof.dialogUI.push({ action, text: await card.innerText() });
      }
    });
  } finally { if (browser) await browser.close(); clearTimeout(watchdog); }
  console.log('SESSION_ACCESS_RESULT', JSON.stringify(results.map(({ name, status, error, durationMs }) => ({ name, status, error, durationMs }))));
  if (results.length !== 4 || results.some(item => item.status !== 'PASS')) process.exitCode = 1;
})().catch(error => { console.error(error); process.exitCode = 1; });
