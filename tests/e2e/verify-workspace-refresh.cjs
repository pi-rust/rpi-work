// Formal WASM UI + isolated real fixture catalogs. GET snapshots can be gated
// or failed to test manual refresh; no real user node or model is accessed.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');
(async () => {
  const a = await startFixture(), b = await startFixture();
  const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
  const context = await browser.newContext({ viewport: { width: 1440, height: 1000 } });
  const page = await context.newPage(); page.setDefaultTimeout(15000);
  const results = [], errors = [], active = new Map(), maximum = new Map(), gates = [];
  let fail = false, nextGate = null;
  const pass = text => { results.push(text); console.log('PASS', text); };
  const refresh = () => page.getByRole('button', { name: '刷新会话', exact: true });
  const feedback = text => page.locator('.sidebar-catalog-status').filter({ hasText: text }).waitFor();
  const session = id => page.locator(`.session-entry[data-session-id="${id}"]`);
  const composer = () => page.getByRole('textbox', { name: '发送给 agent 的消息', exact: true });
  function gateNext() {
    let reached, release;
    const ready = new Promise(resolve => { reached = resolve; });
    const timeout = new Promise((_, reject) => { const timer = setTimeout(() => reject(new Error('Timed out waiting for a refresh snapshot')), 15000); timer.unref(); });
    const gate = { ready: Promise.race([ready, timeout]), wait: new Promise(resolve => { release = resolve; }), reached: () => reached(), release: () => release() };
    gates.push(gate); nextGate = gate; return gate;
  }
  await context.route(url => [a.url, b.url].some(origin => url.origin === new URL(origin).origin) && /^\/api\/v1\/(sessions|projects)$/.test(url.pathname), async route => {
    const req = route.request(), url = new URL(req.url());
    if (req.method() !== 'GET') { await route.continue(); return; }
    const key = `${url.origin}${url.pathname}`;
    active.set(key, (active.get(key) || 0) + 1);
    maximum.set(key, Math.max(maximum.get(key) || 0, active.get(key)));
    const gate = url.origin === new URL(a.url).origin && url.pathname.endsWith('/sessions') ? nextGate : null;
    if (gate) nextGate = null;
    try {
      if (fail && url.origin === new URL(a.url).origin) {
        await route.fulfill({ status: 503, contentType: 'application/json', body: '{"error":"isolated refresh failure"}' });
      } else {
        const response = await route.fetch();
        if (gate) { gate.reached(); await gate.wait; }
        await route.fulfill({ response });
      }
    } catch (error) {
      // Navigation can cancel an old connection's held browser response.
      if (!/closed|cancel|abort|Invalid Interception/i.test(error.message)) throw error;
    } finally { active.set(key, active.get(key) - 1); }
  });
  page.on('pageerror', error => errors.push(error.message));
  async function connect(node) {
    await page.getByRole('button', { name: '打开设置', exact: true }).click();
    await page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '工作协同', exact: true }).click();
    if (!await page.locator('#node-url').isVisible()) await page.getByRole('button', { name: '连接其他节点', exact: true }).click();
    await page.getByLabel('节点地址', { exact: true }).fill(node.url);
    await page.getByRole('button', { name: '连接节点', exact: true }).click();
    await page.locator('.server-details [role=status]').filter({ hasText: '在线' }).waitFor();
    await page.getByRole('button', { name: '返回会话', exact: true }).click();
    await page.locator('.project-entry[data-project-id="rpi-work"]').waitFor();
  }
  async function chooseSession() {
    const toggle = page.locator('.project-entry[data-project-id="rpi-work"] .project-toggle');
    if (await toggle.getAttribute('aria-expanded') !== 'true') await toggle.click();
    await session('dark').locator('.session-item').click();
    await composer().waitFor(); await composer().fill('手动刷新不应丢失的草稿');
  }
  async function retained() {
    assert.equal(await composer().inputValue(), '手动刷新不应丢失的草稿');
    assert.equal(await session('dark').locator('.session-item').getAttribute('aria-current'), 'true');
    assert.equal(await page.evaluate(() => window.__manualRefreshDocument), 'retained');
  }
  async function post(node, endpoint, data) {
    const response = await context.request.post(new URL(`api/v1/${endpoint}`, node.url).href, { data });
    assert(response.ok()); return response.json();
  }
  try {
    await page.goto(process.env.WORK_UI_URL || 'http://127.0.0.1:14180/');
    assert(await refresh().isDisabled());
    pass('manual refresh is unavailable until the selected node is online');
    await connect(a); await chooseSession();
    await page.evaluate(() => { window.__manualRefreshDocument = 'retained'; });
    const created = await post(a, 'sessions', { projectId: 'rpi-work' });
    await post(a, `sessions/${created.sessionId}/rename`, { title: '另一端新增的会话' });
    await refresh().click(); await feedback('已拉取最新会话');
    await session(created.sessionId).getByText('另一端新增的会话', { exact: true }).waitFor();
    await retained();
    pass('Refresh pulls an externally created session without reloading, changing selection or losing the draft');

    // Hold an automatic snapshot, then mutate and request a fresh manual one.
    const automatic = gateNext(); await automatic.ready;
    await post(a, `sessions/${created.sessionId}/rename`, { title: '在途快照之后的最新名称' });
    await refresh().click(); await feedback('正在拉取最新会话');
    assert(await refresh().isDisabled());
    await refresh().evaluate(button => { button.click(); button.click(); });
    automatic.release();
    await feedback('已拉取最新会话');
    await session(created.sessionId).getByText('在途快照之后的最新名称', { exact: true }).waitFor();
    await retained();
    assert([...maximum.values()].every(count => count <= 1));
    pass('a click during automatic polling queues a newer snapshot, deduplicates clicks and never overlaps requests');

    fail = true;
    await refresh().click(); await feedback('刷新会话失败');
    await session(created.sessionId).getByText('在途快照之后的最新名称', { exact: true }).waitFor();
    await retained(); assert(await refresh().isEnabled());
    fail = false;
    await refresh().click(); await feedback('已拉取最新会话');
    pass('failed refresh keeps the last good list and permits an immediate retry');

    const stale = gateNext(); await refresh().click(); await stale.ready;
    await connect(b);
    assert(await refresh().isEnabled());
    assert.equal(await page.locator('.sidebar-catalog-status').count(), 0);
    stale.release(); await page.waitForTimeout(150);
    assert.equal(await session(created.sessionId).count(), 0);
    assert.equal(await page.locator('.sidebar-catalog-status').count(), 0);
    pass('switching nodes discards old refresh results and resets busy/error feedback');

    await chooseSession();
    for (const width of [390, 320]) {
      await page.setViewportSize({ width, height: 844 });
      await page.getByRole('button', { name: '打开项目导航', exact: true }).click();
      await refresh().click(); await feedback('已拉取最新会话');
      const box = await refresh().boundingBox();
      assert(box.height >= 44 && box.width >= 44 && box.x >= 0 && box.x + box.width <= width);
      assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
      await page.screenshot({ path: `artifacts/manual-refresh-${width}.png`, animations: 'disabled' });
      await page.getByRole('button', { name: '关闭项目导航', exact: true }).click({ position: { x: width - 8, y: 400 } });
      await retained();
    }
    pass('390/320 mobile navigation has a 44px refresh control and preserves the selected conversation draft');
    assert(![...a.requests, ...b.requests].some(r => r.path.endsWith('/commands') && r.body?.type === 'prompt'));
    assert.deepEqual(errors, []);
  } catch (error) {
    await page.screenshot({ path: 'artifacts/manual-refresh-failure.png' }).catch(() => {}); throw error;
  } finally {
    gates.forEach(gate => gate.release());
    fs.writeFileSync('artifacts/manual-refresh-validation.json', JSON.stringify({ results, errors }, null, 2));
    await browser.close(); await a.close(); await b.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
