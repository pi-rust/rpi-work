// Formal WASM UI and isolated read-only HTTP/SSE fixtures. Never invokes a model.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');
const seed = () => Array.from({ length: 150 }, (_, i) => ({ entryId: `history-${String(i).padStart(3, '0')}`, role: 'user', content: `历史消息 ${i}\n这是保留阅读位置的原始内容，包含中文与可换行文本。\n第 ${i} 条不会被运行事件覆盖。` }));
(async () => {
  fs.mkdirSync('artifacts', { recursive: true });
  const nodes = [], results = [], errors = [];
  const fixture = await startFixture(0, { pagedHistory: true }); nodes.push(fixture); fixture.setMessages(seed());
  const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
  const context = await browser.newContext({ viewport: { width: 1440, height: 1000 } });
  const page = await context.newPage(); page.setDefaultTimeout(15000);
  page.on('pageerror', error => errors.push(error.message));
  await context.route('https://api.github.com/**', route => route.fulfill({ status: 404, body: '{}' }));
  const pass = text => { results.push(text); console.log('PASS', text); };
  const panel = page.locator('#conversation-panel');
  const rows = () => panel.locator('.message-turn[data-history-id]');
  const requests = node => node.requests.filter(r => r.body.type === 'get_messages');
  const until = async predicate => { for (let i = 0; i < 600; i++) { if (predicate()) return; await page.waitForTimeout(25); } throw new Error('Timed out waiting for fixture request'); };
  const count = expected => page.waitForFunction(n => document.querySelectorAll('#conversation-panel .message-turn[data-history-id]').length === n, expected);
  const connect = async node => {
    await page.getByRole('button', { name: '打开设置', exact: true }).click();
    await page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '工作协同', exact: true }).click();
    await page.getByRole('button', { name: '连接其他节点', exact: true }).click();
    await page.getByLabel('节点地址', { exact: true }).fill(node.url);
    await page.getByRole('button', { name: '连接节点', exact: true }).click();
    await page.getByRole('status').filter({ hasText: '在线' }).first().waitFor();
    await page.getByRole('button', { name: '返回会话', exact: true }).click();
    const toggle = page.locator('[data-project-id="rpi-work"] .project-toggle');
    if (await toggle.getAttribute('aria-expanded') !== 'true') await toggle.click();
    await page.locator('[data-session-id="dark"] .session-item').click();
    await page.waitForFunction(origin => document.querySelector('#conversation-panel')?.dataset.sessionScope.startsWith(origin), node.url.replace(/\/$/, ''));
    await count(48);
  };
  const up = async () => { await panel.hover(); await page.mouse.wheel(0, -100000); };
  const capture = () => panel.evaluate(container => {
    const bounds = container.getBoundingClientRect();
    const row = [...container.querySelectorAll('[data-history-id]')].find(e => e.getBoundingClientRect().bottom > bounds.top && e.getBoundingClientRect().top < bounds.bottom);
    return { id: row.dataset.historyId, offset: row.getBoundingClientRect().top - bounds.top };
  });
  const anchored = anchor => page.waitForFunction(({ id, offset }) => {
    const container = document.querySelector('#conversation-panel');
    const row = [...container.querySelectorAll('[data-history-id]')].find(e => e.dataset.historyId === id);
    return row && Math.abs(row.getBoundingClientRect().top - container.getBoundingClientRect().top - offset) <= 2;
  }, anchor);
  try {
    await page.goto(process.env.WORK_UI_URL || 'http://127.0.0.1:14180/');
    await connect(fixture);
    assert.equal(await rows().first().getAttribute('data-history-id'), 'history-102');
    assert(fixture.requests.some(r => r.path.endsWith('/dark/snapshot') && new URLSearchParams(r.query).get('limit') === '48'));
    assert.equal(requests(fixture).length, 0);
    await page.locator('.composer textarea').fill('分页期间保留草稿');
    pass('initial connection requests 48 transcript entries and subscribes without eagerly downloading older pages');

    fixture.controls.historyDelayMs = 650;
    await up(); await until(() => requests(fixture).length === 1);
    await page.waitForFunction(() => document.querySelector('#conversation-panel').scrollTop <= 96);
    const anchor = await capture();
    for (let i = 0; i < 5; i++) await page.mouse.wheel(0, -20);
    await count(96); await anchored(anchor);
    assert.equal(requests(fixture).length, 1);
    assert.equal(requests(fixture)[0].body.beforeId, 'history-102');
    assert.equal(requests(fixture)[0].body.limit, 48);
    pass('upward scrolling loads one page; repeated input while pending cannot duplicate the request or move the reading anchor');

    await panel.locator('[data-history-id="history-054"]').evaluate(row => {
      const media = document.createElement('div'); media.style.height = '210px'; media.setAttribute('data-test-media-size', 'late'); row.append(media);
    });
    await anchored(anchor);
    fixture.emit({ type: 'message_end', message: { entryId: 'history-150', role: 'user', content: '分页后的实时消息' } });
    await count(97); await anchored(anchor);
    assert.equal(await rows().first().getAttribute('data-history-id'), 'history-054');
    assert.equal(await rows().last().getAttribute('data-history-id'), 'history-150');
    pass('late media-size changes above the reader preserve its screen offset; live messages retain already-loaded history');
    fixture.setMessages([...seed(), { entryId: 'history-150', role: 'user', content: '结算后的权威消息' }]);
    fixture.emit({ type: 'agent_settled', runId: 'history-settlement', outcome: 'completed' });
    await rows().last().getByText('结算后的权威消息', { exact: true }).waitFor();
    assert.deepEqual(await rows().evaluateAll(es => es.map(e => e.dataset.historyId)), Array.from({ length: 97 }, (_, i) => `history-${String(i + 54).padStart(3, '0')}`));
    pass('a bounded settlement snapshot corrects the live suffix without moving retained historical entries to the end');

    fixture.controls.failHistoryCount = 1;
    await up(); await until(() => requests(fixture).length === 2);
    await page.getByRole('button', { name: '重试加载更早消息', exact: true }).waitFor();
    for (let i = 0; i < 4; i++) await page.mouse.wheel(0, -20);
    assert.equal(requests(fixture).length, 2);
    await page.getByRole('button', { name: '重试加载更早消息', exact: true }).click();
    // Hold a real pointer gesture while the delayed page arrives. Its active
    // interaction state must win over persistent reading-anchor correction.
    await panel.hover(); await page.mouse.down();
    await count(145);
    const draggedTop = await panel.evaluate(e => { e.scrollTop = 220; return e.scrollTop; });
    await page.waitForTimeout(150);
    assert(Math.abs(await panel.evaluate(e => e.scrollTop) - draggedTop) <= 1, 'a held pointer gesture must not snap back to the pre-page anchor');
    await page.mouse.up();
    assert.equal(requests(fixture).length, 3);
    assert.equal(requests(fixture)[2].body.beforeId, requests(fixture)[1].body.beforeId);
    assert.equal(await rows().first().getAttribute('data-history-id'), 'history-006');
    pass('a failed page keeps its cursor and loaded messages, avoids automatic retry storms and supports explicit retry');

    fixture.controls.historyDelayMs = 1000;
    await up(); await until(() => requests(fixture).length === 4);
    await page.locator('[data-session-id="architecture"] .session-item').click();
    await page.waitForFunction(() => document.querySelector('#conversation-panel')?.dataset.sessionScope.endsWith('::session::architecture'));
    await page.waitForTimeout(1200);
    assert.equal(await panel.locator('[data-history-id^="history-"]').count(), 0);
    fixture.controls.historyDelayMs = 0;
    await page.locator('[data-session-id="dark"] .session-item').click(); await count(48);
    await up(); await count(96);
    fixture.disconnect();
    await count(48);
    assert.equal(await rows().first().getAttribute('data-history-id'), 'history-103');
    pass('switching sessions rejects late responses; reconnect resets loaded pages to a fresh bounded snapshot instead of mixing stale branches');

    fixture.controls.historyDelayMs = 1000;
    const beforePending = requests(fixture).length;
    const snapshots = () => fixture.requests.filter(r => r.path.endsWith('/dark/snapshot')).length;
    const beforeReconnect = snapshots();
    await up(); await until(() => requests(fixture).length === beforePending + 1);
    fixture.disconnect(); await until(() => snapshots() > beforeReconnect);
    await page.waitForTimeout(1200); await count(48);
    assert.equal(await page.getByRole('button', { name: '重试加载更早消息', exact: true }).count(), 0);
    pass('a page response from before reconnect cannot mutate the new snapshot even when session and cursor are unchanged');

    fixture.controls.historyDelayMs = 600;
    const beforeTabSwitch = requests(fixture).length;
    await up(); await until(() => requests(fixture).length === beforeTabSwitch + 1);
    await page.locator('#changes-tab').click();
    await page.waitForTimeout(750);
    await page.locator('#conversation-tab').click();
    await page.getByRole('button', { name: '重试加载更早消息', exact: true }).waitFor();
    fixture.controls.historyDelayMs = 0;
    await page.getByRole('button', { name: '重试加载更早消息', exact: true }).click(); await count(96);
    await up(); await count(144);
    await up(); await count(151);
    await page.getByText('已加载到会话开头', { exact: true }).waitFor();
    const completed = requests(fixture).length;
    for (let i = 0; i < 4; i++) await page.mouse.wheel(0, -20);
    assert.equal(requests(fixture).length, completed);
    assert.equal(await rows().first().getAttribute('data-history-id'), 'history-000');
    assert.equal(new Set(await rows().evaluateAll(es => es.map(e => e.dataset.historyId))).size, 151);
    pass('leaving the conversation cancels loading without a permanent busy state; retry reaches the oldest entry exactly once');

    for (const width of [390, 320]) {
      await page.setViewportSize({ width, height: 844 });
      assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
      await page.screenshot({ path: `artifacts/history-pagination-${width}.png`, animations: 'disabled' });
    }
    await page.setViewportSize({ width: 1440, height: 1000 });
    assert.equal(await page.locator('.composer textarea').inputValue(), '分页期间保留草稿');

    const unsupported = await startFixture(0, { pagedHistory: true, noMessagePaging: true }); nodes.push(unsupported); unsupported.setMessages(seed());
    await connect(unsupported); await up(); await page.waitForTimeout(150);
    assert.equal(requests(unsupported).length, 0);
    assert.equal(await page.getByRole('button', { name: '加载更早消息', exact: true }).count(), 0);
    await page.getByText('当前节点未提供可用的历史分页能力或真实消息游标。', { exact: true }).waitFor();
    pass('missing get_messages capability shows a truthful boundary without issuing an unsupported request');

    const legacy = await startFixture(0, { pagedHistory: true }); nodes.push(legacy); legacy.setMessages(seed());
    await connect(legacy); await up(); await count(96);
    legacy.controls.ignoreHistoryCursor = true;
    await up();
    await page.getByRole('status').filter({ hasText: '读取更早消息失败' }).waitFor();
    await count(96);
    for (let i = 0; i < 4; i++) await page.mouse.wheel(0, -20);
    assert.equal(requests(legacy).length, 2);
    assert.equal(await page.getByRole('button', { name: '重试加载更早消息', exact: true }).count(), 0);
    assert.equal(await page.getByText('已加载到会话开头', { exact: true }).count(), 0, 'unsupported is not a successful end of history');
    assert.equal(new Set(await rows().evaluateAll(es => es.map(e => e.dataset.historyId))).size, 96);
    assert.equal(await page.evaluate(() => window.__rpiHistoryPagers.size), 0, 'disabled pagination removes its observer and all obsolete session observers');
    pass('a legacy node that ignores beforeId is rejected without duplicating the tail, looping or falsely reporting the beginning');
    assert.deepEqual(errors, []);
    for (const node of nodes) assert.equal(node.requests.filter(r => ['prompt', 'resume_pending', 'append_entry'].includes(r.body.type)).length, 0);
    pass('390/320px layouts, draft isolation, zero model calls and application JS errors remain safe');
  } catch (error) {
    await page.screenshot({ path: 'artifacts/history-pagination-failure.png', animations: 'disabled' });
    results.push({ failure: error.message, pages: nodes.map(node => requests(node).map(r => r.body)) });
    throw error;
  } finally {
    fs.writeFileSync('artifacts/history-pagination-validation.json', JSON.stringify({ results, errors }, null, 2));
    await browser.close(); for (const node of nodes) await node.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
