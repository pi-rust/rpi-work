// Formal WASM + isolated HTTP/SSE only. Explicit response gates, no model calls.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');
function deferred() { let resolve; const promise = new Promise(r => { resolve = r; }); return { promise, resolve }; }
async function bounded(promise, label) {
  let timer;
  try { return await Promise.race([promise, new Promise((_, reject) => { timer = setTimeout(() => reject(new Error(`Timed out: ${label}`)), 12000); })]); }
  finally { clearTimeout(timer); }
}
(async () => {
  fs.mkdirSync('artifacts', { recursive: true });
  const a = await startFixture(), b = await startFixture();
  const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
  const context = await browser.newContext({ viewport: { width: 1440, height: 1000 } });
  const page = await context.newPage(); page.setDefaultTimeout(12000);
  const results = [], errors = [], gates = [];
  page.on('pageerror', error => errors.push(error.message));
  await context.route('https://api.github.com/**', route => route.fulfill({ status: 404, body: '{}' }));
  const pass = text => { results.push(text); console.log('PASS', text); };
  const creates = node => node.requests.filter(r => r.method === 'POST' && r.path === '/api/v1/sessions');
  const selected = () => page.locator('.session-entry.selected');
  const field = () => page.locator('.composer-main textarea');
  const session = id => page.locator(`[data-session-id="${id}"] .session-item`);
  const plus = () => page.locator('[data-project-id="rpi-work"] .project-new-session');
  const settings = () => page.getByRole('navigation', { name: '设置分类' });
  async function openSettings() { await page.getByRole('button', { name: '打开设置', exact: true }).click(); await settings().waitFor(); }
  async function returnWorkspace() { await page.getByRole('button', { name: '返回会话', exact: true }).click(); await field().waitFor(); }
  async function connect(node) {
    if (!await settings().isVisible()) await openSettings();
    await settings().getByRole('button', { name: '工作协同', exact: true }).click();
    await page.getByRole('button', { name: '连接其他节点', exact: true }).click();
    await page.getByLabel('节点地址', { exact: true }).fill(node.url);
    await page.getByRole('button', { name: '连接节点', exact: true }).click();
    await page.locator('.server-details [role=status]').filter({ hasText: '在线' }).waitFor();
    await returnWorkspace();
    await page.locator('[data-project-id="rpi-work"] .project-row').click();
    await session('dark').click();
    await page.locator('.transcript').getByText('主题切换已经准备好，接下来运行组件测试。', { exact: true }).waitFor();
    await page.waitForFunction(() => !document.querySelector('.composer-main textarea').disabled);
  }
  async function gate(node, { failure = false, detail = false } = {}) {
    const arrived = deferred(), release = deferred(), done = deferred();
    let used = false;
    const pattern = detail ? `${node.url}api/v1/sessions/new-*` : `${node.url}api/v1/sessions`;
    const handler = async route => {
      const request = route.request();
      if (used || (detail ? request.method() !== 'GET' : request.method() !== 'POST')) return route.continue();
      used = true;
      const response = await route.fetch();
      const body = await response.json();
      arrived.resolve(body);
      await release.promise;
      try {
        if (failure) await route.fulfill({ status: 500, json: { error: 'Isolated creation failure' } });
        else await route.fulfill({ response });
      } catch (error) {
        // Node changes deliberately cancel a fetch whose server may have already
        // committed. That is a tested outcome, not a transport success assertion.
        if (!/closed|disposed|invalid interception|not found|canceled|cancelled|finished/i.test(error.message)) throw error;
      } finally { done.resolve(); }
    };
    await context.route(pattern, handler);
    const item = { arrived: arrived.promise, finish: async () => {
      release.resolve(); await bounded(done.promise, 'gate response delivery'); await context.unroute(pattern, handler);
      // Let the delivered response and reactive effects run before checking
      // negative stale-result assertions; response release alone is not a UI ack.
      await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
    }, release };
    gates.push(item); return item;
  }
  async function start(g) { await plus().click(); return bounded(g.arrived, 'creation response arrival'); }
  async function creationFinished() { await page.waitForFunction(() => !document.querySelector('.new-task')?.disabled); }
  async function selection(id) { await page.waitForFunction(id => document.querySelector('.session-entry.selected')?.dataset.sessionId === id, id); }
  async function list(node) { const response = await context.request.get(`${node.url}api/v1/sessions`); return (await response.json()).sessions; }
  try {
    await page.goto(process.env.WORK_UI_URL || 'http://127.0.0.1:4185/');
    await connect(a);
    await field().fill('node A original draft');
    await page.getByLabel('选择本地附件，可多选', { exact: true }).setInputFiles({ name: 'scope-a.txt', mimeType: 'text/plain', buffer: Buffer.from('local only') });
    await page.getByRole('button', { name: '预览 scope-a.txt', exact: true }).waitFor();

    // Repro: late success must not navigate away from settings.
    const navigation = await gate(a);
    const first = await start(navigation);
    await openSettings();
    await navigation.finish(); await creationFinished();
    assert(await settings().isVisible(), 'late creation must not force settings back to workspace');
    assert.equal(await selected().count(), 0);
    await returnWorkspace();
    await session(first.sessionId).waitFor();
    assert.equal(await selected().count(), 0);
    pass('late success remains discoverable but cannot leave settings or select the created session');

    // Leaving and returning does not revive an abandoned automatic-open intent.
    const returned = await gate(a);
    await start(returned); await openSettings(); await returnWorkspace();
    await returned.finish(); await creationFinished();
    assert.equal(await selected().count(), 0);
    pass('returning to the conversation before completion does not revive abandoned creation focus');

    const tab = await gate(a);
    await start(tab);
    await page.getByRole('tab', { name: /变更/ }).click();
    await tab.finish(); await creationFinished();
    assert.equal(await page.getByRole('tab', { name: /变更/ }).getAttribute('aria-selected'), 'true');
    assert.equal(await selected().count(), 0);
    await page.getByRole('tab', { name: '对话', exact: true }).click();
    pass('late success cannot switch the changes tab back to conversation');

    const failure = await gate(a, { failure: true });
    await start(failure); await openSettings();
    await failure.finish(); await creationFinished();
    assert(await settings().isVisible());
    assert.equal(await page.getByRole('status').filter({ hasText: 'Isolated creation failure' }).count(), 0);
    await returnWorkspace();
    assert.equal(await page.locator('.empty-state [role=alert]').count(), 0);
    pass('an abandoned creation failure does not overwrite another page status or show stale retry errors');

    const switched = await gate(a, { detail: true });
    await start(switched); await session('dark').click(); await selection('dark');
    await field().fill('node A newer draft');
    await switched.finish(); await creationFinished();
    await selection('dark'); assert.equal(await field().inputValue(), 'node A newer draft');
    assert(await page.getByRole('button', { name: '预览 scope-a.txt', exact: true }).isVisible());
    pass('switching sessions during detail GET preserves explicit selection, draft and local attachments');

    // A committed POST is held before the client learns its ID. B has the same
    // seed session IDs and can start its own independent creation immediately.
    const oldA = await gate(a);
    const old = await start(oldA);
    await connect(b); await field().fill('node B draft');
    const newB = await gate(b);
    const fresh = await start(newB);
    await oldA.finish();
    assert(await page.locator('.new-task').isDisabled());
    assert.equal(await session(old.sessionId).count(), 0);
    await newB.finish(); await creationFinished(); await selection(fresh.sessionId);
    await session('dark').click(); await selection('dark');
    assert.equal(await field().inputValue(), 'node B draft');
    assert.equal(await page.getByRole('button', { name: '预览 scope-a.txt', exact: true }).count(), 0);
    pass('old-node success cannot insert/select on B or clear B creation busy; new-node creation and scope draft remain valid');

    const oldFailure = await gate(b, { failure: true });
    await start(oldFailure); await connect(a);
    await oldFailure.finish(); await selection('dark');
    assert.equal(await field().inputValue(), 'node A newer draft');
    assert.equal(await page.getByRole('status').filter({ hasText: 'Isolated creation failure' }).count(), 0);
    const more = page.locator('[data-project-id="rpi-work"]').getByRole('button', { name: /^显示更多/ });
    if (await more.isVisible()) await more.click();
    await session(old.sessionId).waitFor();
    pass('late old-node failure cannot replace new-node status; a committed old session is rediscovered from its real node list');

    // Same-origin reconnect increments epoch without canceling submissions.
    // Releasing the old result while a new POST is gated must not clear busy.
    const reconnect = await gate(a, { detail: true });
    const oldCount = creates(a).length;
    const summary = await start(reconnect);
    await connect(a);
    const current = await gate(a);
    const newest = await start(current);
    await reconnect.finish();
    assert(await page.locator('.new-task').isDisabled());
    assert.equal(await selected().count(), 0);
    await current.finish(); await creationFinished(); await selection(newest.sessionId);
    assert.equal(creates(a).length, oldCount + 2);
    pass('same-address reconnect rejects stale epoch detail response without clearing new creation busy or duplicating POSTs');

    const staleError = await gate(a, { failure: true });
    await start(staleError); await connect(a);
    const afterError = await gate(a);
    const retry = await start(afterError);
    await staleError.finish();
    assert(await page.locator('.new-task').isDisabled());
    assert.equal(await page.getByRole('status').filter({ hasText: 'Isolated creation failure' }).count(), 0);
    await afterError.finish(); await creationFinished(); await selection(retry.sessionId);
    pass('same-origin stale POST failure cannot clear a newer creation busy flag or expose a stale retry error');

    const roundTrip = await gate(a, { detail: true });
    await start(roundTrip); await connect(b); await connect(a);
    await roundTrip.finish(); await selection('dark');
    assert.equal(await field().inputValue(), 'node A newer draft');
    assert(await page.getByRole('button', { name: '预览 scope-a.txt', exact: true }).isVisible());
    assert(summary.sessionId);
    pass('A-to-B-to-A cannot revive old creation intent even when origin and session IDs match again');

    const before = creates(a).length;
    const normal = await gate(a);
    await plus().dblclick(); await bounded(normal.arrived, 'double-click creation arrival');
    assert.equal(creates(a).length, before + 1);
    await normal.finish(); await creationFinished();
    await page.waitForFunction(() => document.querySelector('.app-header h1')?.textContent === '新会话');
    await page.waitForFunction(() => document.activeElement?.matches('.composer-main textarea'));
    assert.equal(creates(a).length, before + 1);
    assert.equal(await field().inputValue(), '');
    pass('ordinary creation still opens and focuses once with double-click deduplication after all stale-result cases');

    assert.equal([...a.requests, ...b.requests].filter(r => ['prompt', 'queue_message'].includes(r.body.type)).length, 0);
    assert.deepEqual(errors, []);
    pass('zero prompt/queue/model calls and no application JavaScript errors');
    fs.writeFileSync('artifacts/session-creation-isolation-validation.json', JSON.stringify({ results, errors, counts: { a: creates(a).length, b: creates(b).length }, sessions: { a: (await list(a)).length, b: (await list(b)).length } }, null, 2));
  } catch (error) {
    await page.screenshot({ path: 'artifacts/session-creation-isolation-failure.png' });
    fs.writeFileSync('artifacts/session-creation-isolation-validation.json', JSON.stringify({ results, errors, fatal: error.message }, null, 2));
    throw error;
  } finally {
    for (const g of gates) g.release.resolve();
    await browser.close(); await a.close(); await b.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
