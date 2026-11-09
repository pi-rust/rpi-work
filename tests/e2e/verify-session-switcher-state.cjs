// Published WASM regression; fixture HTTP/WebSocket only, no real-node/model traffic.
// Run: node tests/e2e/verify-session-switcher-state.cjs
const assert = require('node:assert/strict');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');

(async () => {
  const fixture = await startFixture(0);
  const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
  const page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
  page.setDefaultTimeout(8000);
  const failures = [], errors = [], catalog = new Map(), summaries = new Map();
  let catalogReads = 0;
  page.on('pageerror', error => errors.push(error.message));
  await page.route('https://api.github.com/**', route => route.fulfill({ status: 404, body: '{}' }));
  // Change metadata only: never forge lastSeq, snapshot events, or WebSocket cursors.
  await page.route(`${fixture.url}api/v1/sessions`, async route => {
    if (route.request().method() !== 'GET') return route.continue();
    const response = await route.fetch(), body = await response.json();
    for (const session of body.sessions) Object.assign(session, catalog.get(session.sessionId));
    const base = body.sessions[0];
    body.sessions.push(
      { ...base, sessionId: 'actual-idle', title: 'Never started', status: 'idle', error: null, pendingResume: false, latestSettlement: null, lastSeq: 0 },
      { ...base, sessionId: 'actual-unknown', title: 'No observed outcome', status: 'unknown', error: null, pendingResume: false, latestSettlement: null, lastSeq: 0 }
    );
    await route.fulfill({ response, json: body });
    catalogReads++;
  });
  await page.route(new RegExp(`${fixture.url.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}api/v1/sessions/[^/]+$`), async route => {
    if (route.request().method() !== 'GET') return route.continue();
    const response = await route.fetch(), body = await response.json();
    Object.assign(body, summaries.get(body.sessionId));
    await route.fulfill({ response, json: body });
  });
  const modal = page.locator('#session-switcher');
  const search = modal.getByLabel('搜索会话或项目', { exact: true });
  const row = id => modal.locator(`.session-switcher-row[data-session-id="${id}"]`);
  const label = id => row(id).locator('.switcher-session-state');
  const wait = async (predicate, message, timeout = 6500) => {
    const end = Date.now() + timeout;
    while (Date.now() < end) {
      if (await predicate()) return;
      await new Promise(resolve => setTimeout(resolve, 80));
    }
    assert.fail(message);
  };
  const expectState = async (id, expected) => {
    await wait(async () => (await label(id).textContent().catch(() => ''))?.trim() === expected,
      `${id}: expected picker state ${JSON.stringify(expected)}, observed ${JSON.stringify((await label(id).textContent().catch(() => '<missing>'))?.trim())}`);
  };
  const open = async () => {
    if (!await modal.isVisible()) await page.getByRole('button', { name: '切换会话', exact: true }).click();
    await search.waitFor();
  };
  const select = async id => {
    await open();
    await search.fill(id);
    await row(id).click();
    await modal.waitFor({ state: 'hidden' });
    await page.locator(`[data-history-id="user-${id}"]`).waitFor();
    await page.locator('.composer textarea:not(:disabled)').waitFor();
    await wait(() => fixture.requests.some(request => request.path === '/api/v1/realtime' && request.sessionId === id), `${id}: WebSocket subscription not established`);
  };
  const check = async (name, test) => {
    try { await test(); console.log('PASS', name); }
    catch (error) { failures.push(`${name}: ${error.message}`); console.error('FAIL', failures.at(-1)); }
  };
  const stale = { status: 'failed', error: 'Historical fixture failure', pendingResume: true, latestSettlement: { outcome: 'failed', pendingResume: true } };
  const draft = 'Do not lose this unsent session-switcher draft';
  try {
    await page.goto(process.env.WORK_UI_URL || 'http://127.0.0.1:14180/');
    await page.getByRole('button', { name: '打开设置', exact: true }).click();
    await page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '执行节点', exact: true }).click();
    await page.getByRole('button', { name: '连接其他节点', exact: true }).click();
    await page.getByLabel('节点地址', { exact: true }).fill(fixture.url);
    await page.getByRole('button', { name: '连接节点', exact: true }).click();
    await page.getByRole('status').filter({ hasText: '在线' }).first().waitFor();
    await page.getByRole('button', { name: '返回会话', exact: true }).click();
    await page.locator('[data-project-id="rpi-work"] .project-toggle').click();
    await page.locator('[data-session-id="dark"] .session-item').click();
    await page.locator('.composer textarea:not(:disabled)').waitFor();
    await page.locator('[data-history-id="user-dark"]').waitFor();
    await page.locator('.composer textarea').fill(draft);
    await page.setViewportSize({ width: 390, height: 844 });
    await open();
    await check('actual idle and unknown have no invented success', async () => {
      await search.fill('');
      await expectState('actual-idle', '空闲');
      await expectState('actual-unknown', '状态未知');
    });

    // Fail a real fixture run, then load a snapshot containing its successful next run.
    await search.fill('dark');
    fixture.emit({ type: 'agent_start' });
    await expectState('dark', '运行中');
    fixture.emit({ type: 'agent_settled', outcome: 'failed', pendingResume: true });
    await check('live failed settlement is not still running', () => expectState('dark', '异常'));
    await select('architecture');
    fixture.emit({ type: 'agent_start' });
    fixture.emit({ type: 'agent_settled', outcome: 'completed', pendingResume: false });
    catalog.set('dark', stale); summaries.set('dark', stale);
    await select('dark');
    await open(); await search.fill('dark');
    await check('newer completed snapshot beats historical error/pendingResume/catalog failure', () => expectState('dark', '完成'));
    await check('switch away/back retains successful next-run outcome and draft', async () => {
      await select('architecture'); await select('dark');
      assert.equal(await page.locator('.composer textarea').inputValue(), draft);
      await open(); await search.fill('dark');
      await expectState('dark', '完成');
    });

    await select('architecture');
    fixture.emit({ type: 'agent_start' });
    await select('dark'); await open(); await search.fill('dark');
    await check('newer running snapshot beats stale terminal fields', () => expectState('dark', '运行中'));
    // The terminal snapshot, not an event delivered to the selected UI, must win.
    await select('architecture');
    fixture.emit({ type: 'agent_settled', outcome: 'interrupted', pendingResume: false });
    catalog.set('dark', { ...stale, status: 'completed', latestSettlement: { outcome: 'completed' } });
    summaries.set('dark', catalog.get('dark'));
    await select('dark'); await open(); await search.fill('dark');
    await check('interrupted snapshot is stopped, not stale completed/error/pendingResume', () => expectState('dark', '已停止'));
    await check('stopped outcome survives switch away/back', async () => {
      await select('architecture'); await select('dark');
      assert.equal(await page.locator('.composer textarea').inputValue(), draft);
      await open(); await search.fill('dark'); await expectState('dark', '已停止');
    });

    await select('architecture');
    fixture.emit({ type: 'agent_start' });
    fixture.emit({ type: 'agent_settled', outcome: 'unknown', pendingResume: false });
    const staleSuccess = { status: 'completed', error: null, pendingResume: false, latestSettlement: { outcome: 'completed' } };
    catalog.set('dark', staleSuccess); summaries.set('dark', staleSuccess);
    await select('dark'); await open(); await search.fill('dark');
    await check('unknown terminal snapshot never inherits historical success', () => expectState('dark', '状态未知'));

    await check('unselected session changes via catalog polling without resetting picker search/focus', async () => {
      await search.fill('mobile'); await search.focus();
      const before = fixture.requests.filter(request => request.path === '/api/v1/sessions/mobile/snapshot').length;
      catalog.set('mobile', { status: 'running', error: null, pendingResume: false });
      let reads = catalogReads;
      await wait(() => catalogReads > reads, 'no catalog poll while picker open', 12000);
      await expectState('mobile', '运行中');
      assert.equal(await search.inputValue(), 'mobile');
      assert(await search.evaluate(el => el === document.activeElement), 'catalog poll must not remount search');
      catalog.set('mobile', { status: 'idle', error: null, pendingResume: false, latestSettlement: { outcome: 'completed' } });
      reads = catalogReads;
      await wait(() => catalogReads > reads, 'no second catalog poll while picker open', 12000);
      await expectState('mobile', '完成');
      assert.equal(await search.inputValue(), 'mobile');
      assert(await search.evaluate(el => el === document.activeElement));
      assert.equal(fixture.requests.filter(request => request.path === '/api/v1/sessions/mobile/snapshot').length, before, 'unselected update must come from catalog, not opening a snapshot');
      await page.keyboard.press('Escape'); await modal.waitFor({ state: 'hidden' });
      assert.equal(await page.locator('.composer textarea').inputValue(), draft);
    });
    await check('accepted prompt settled offscreen does not remain waiting for a reply', async () => {
      catalog.delete('dark'); summaries.delete('dark');
      await page.locator('.composer textarea').fill('fixture-only session status regression');
      await page.locator('.composer textarea').press('Enter');
      await wait(() => fixture.requests.some(request => request.body.type === 'prompt'), 'fixture prompt was not sent');
      await wait(async () => await page.locator('.composer textarea').inputValue() === '', 'fixture prompt was not acknowledged');
      await open(); await search.fill('dark');
      await expectState('dark', '等待回答'); // Old settlement cannot settle a new accepted send.
      await select('architecture');
      fixture.emit({ type: 'agent_start' });
      fixture.emit({ type: 'agent_settled', outcome: 'completed', pendingResume: false });
      await open(); await search.fill('dark');
      await expectState('dark', '完成');
      await select('dark'); await open(); await search.fill('dark');
      await expectState('dark', '完成'); // A fresh snapshot also retires stale waiting feedback.
    });
    assert.deepEqual(errors, [], 'browser runtime errors');
    const prompts = fixture.requests.filter(request => request.body.type === 'prompt');
    assert.equal(prompts.length, 1, 'only the explicit fixture-only status prompt is allowed');
    assert.equal(prompts[0].body.message, 'fixture-only session status regression');
    assert.equal(fixture.requests.filter(request => request.body.type === 'resume_pending').length, 0);
    assert.deepEqual(failures, [], 'session-switcher state regressions');
  } finally { await browser.close(); await fixture.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
