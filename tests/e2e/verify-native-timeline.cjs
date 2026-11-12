// Formal, already-built Web UI + isolated HTTP/WebSocket fixture. No model/credentials.
// Run only after the main agent's build: node tests/e2e/verify-native-timeline.cjs
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { setTimeout: delay } = require('node:timers/promises');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');
const WEB = process.env.WORK_UI_URL || 'http://127.0.0.1:14180/';
const results = [], runs = [];
const DRAFT = 'native timeline 本地草稿，历史阅读不得覆盖。';
const denseHistory = (prefix = 'native') => Array.from({ length: 20 }, (_, group) => [
  { entryId: `${prefix}-user-${group}`, role: 'user', content: `问题 ${group}：${prefix} 稠密工具历史。` },
  ...Array.from({ length: 20 }, (_, item) => item % 2 === 0
    ? { entryId: `${prefix}-${group}-${item}`, role: 'assistant', content: [{ type: 'text', text: `fixture 答复 ${group}/${item}` }, { type: 'toolCall', id: `${prefix}-call-${group}-${item}`, name: 'read', arguments: { path: `fixture-${group}-${item}.txt` } }] }
    : { entryId: `${prefix}-${group}-${item}`, role: 'toolResult', toolCallId: `${prefix}-call-${group}-${item - 1}`, toolName: 'read', content: [{ type: 'text', text: `fixture 工具结果 ${group}/${item}` }] }),
]).flat();
async function poll(check, label, timeout = 12000) {
  const end = Date.now() + timeout;
  while (Date.now() < end) { if (await check()) return; await delay(25); }
  throw new Error(`Timed out: ${label}`);
}
const pass = name => { results.push(name); console.log('PASS', name); };
async function setup(browser, prepare = () => {}) {
  const fixture = await startFixture(0, { pagedHistory: true, nativeQueries: true });
  fixture.setMessages(denseHistory());
  fixture.setMessages(denseHistory('mobile'), 'mobile');
  prepare(fixture);
  const context = await browser.newContext({ viewport: { width: 1440, height: 1000 } });
  const page = await context.newPage(); page.setDefaultTimeout(12000);
  const errors = [], blocked = [], wire = [];
  page.on('pageerror', error => errors.push(error.message));
  await context.route('**/*', async route => {
    const request = route.request(), url = new URL(request.url());
    if (url.hostname === 'api.github.com') return route.fulfill({ status: 404, body: '{}' });
    const isNode = url.origin === new URL(fixture.url).origin;
    const isApp = url.origin === new URL(WEB).origin;
    if (!isNode && !isApp) { blocked.push(request.url()); return route.abort('blockedbyclient'); }
    if (!url.pathname.startsWith('/api/v1/')) return route.continue();
    if (!['GET', 'OPTIONS'].includes(request.method())) {
      const body = request.postDataJSON();
      if (!(url.pathname.endsWith('/open') || (url.pathname.endsWith('/commands') && /^get_/.test(body?.type || '')))) {
        blocked.push(`${request.method()} ${url.pathname} ${body?.type || ''}`);
        return route.abort('blockedbyclient');
      }
    }
    const response = await route.fetch({ url: new URL(url.pathname + url.search, fixture.url).href });
    if (/\/(snapshot|message-summaries|message-window)$/.test(url.pathname)) {
      const body = await response.json();
      // Seed projection events are unrelated to this bounded-history contract.
      if (url.pathname.endsWith('/snapshot')) { body.events = []; delete body.latestSettlement; }
      wire.push({ path: url.pathname, query: url.search, status: response.status(), body });
      return route.fulfill({ response, json: body });
    }
    return route.fulfill({ response });
  });
  const s = { fixture, context, page, errors, blocked, wire,
    nodes: page.locator('.conversation-timeline .timeline-node'),
    panel: page.locator('[aria-label="历史消息窗口"]'),
    composer: page.locator('.composer textarea'),
    requests(action, id = 'dark') { return fixture.requests.filter(request => request.method === 'GET' && request.path === `/api/v1/sessions/${id}/${action}`); },
    responses(action, id = 'dark') { return wire.filter(response => response.path === `/api/v1/sessions/${id}/${action}`); },
    async select(id) {
      const entry = page.locator(`[data-session-id="${id}"] .session-item`);
      if (!await entry.isVisible()) await page.locator('.project-row').filter({ hasText: 'rpi-work' }).click();
      await entry.click();
      await page.waitForFunction(id => document.querySelector('.transcript[data-session-scope]')?.dataset.sessionScope.endsWith(`::session::${id}`), id);
    },
    liveIds() { return page.locator('.transcript [data-history-id]').evaluateAll(rows => rows.map(row => row.dataset.historyId)); },
    tick(id) { return page.locator(`.conversation-timeline .timeline-node[data-timeline-target="entry:${id}"]`); },
    async close() {
      runs.push({ requests: fixture.requests, wire, errors, blocked });
      await context.close(); await fixture.close();
    },
  };
  try {
    await page.goto(WEB);
    await page.getByRole('button', { name: '打开设置', exact: true }).click();
    await page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '工作协同', exact: true }).click();
    await page.getByRole('button', { name: '连接其他节点', exact: true }).click();
    await page.getByLabel('节点地址', { exact: true }).fill(fixture.url);
    await page.getByRole('button', { name: '连接节点', exact: true }).click();
    await page.getByRole('status').filter({ hasText: '在线' }).first().waitFor();
    await page.getByRole('button', { name: '返回会话', exact: true }).click();
    await s.select('dark');
    await poll(() => s.requests('snapshot').length > 0, 'bounded snapshot');
    await page.waitForFunction(() => !document.querySelector('.composer textarea')?.disabled);
    await page.getByRole('button', { name: '显示时间线', exact: true }).click();
    await page.waitForFunction(() => window.__rpiUserTimelines?.size === 1);
    return s;
  } catch (error) { await s.close(); throw error; }
}
function noWrites(s) {
  assert.deepEqual(s.errors, [], 'no browser application errors');
  assert.deepEqual(s.blocked, [], 'no external services or write requests');
  assert.equal(s.fixture.requests.filter(request => ['prompt', 'queue_message', 'abort', 'compact', 'set_api_key', 'set_model'].includes(request.body.type)).length, 0);
}
function params(request) { return Object.fromEntries(new URLSearchParams(request.query)); }
async function windowAction(s, buttonName, expectedCursor, leafId) {
  const count = s.requests('message-window').length;
  const completed = s.responses('message-window').length;
  await s.panel.getByRole('button', { name: buttonName, exact: true }).click();
  await poll(() => s.responses('message-window').length === completed + 1, buttonName);
  await poll(async () => !await s.panel.getByRole('button', { name: buttonName, exact: true }).isDisabled(), 'window controls settled');
  assert.equal(s.requests('message-window').length, count + 1, 'one window request per action');
  const query = params(s.requests('message-window').at(-1));
  assert.equal(query.limit, '48'); assert.equal(query.leafId, leafId);
  assert.equal(query[expectedCursor.key], expectedCursor.value);
  return s.responses('message-window').at(-1);
}
(async () => {
  fs.mkdirSync('artifacts', { recursive: true });
  const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
  let active;
  try {
    const s = active = await setup(browser);
    await poll(async () => await s.nodes.count() >= 10, 'first screen contains ten user summaries');
    const snapshot = s.responses('snapshot').at(-1).body;
    const leafId = snapshot.leafId;
    assert.equal(leafId, 'native-19-19');
    const summaries = s.requests('message-summaries');
    assert.equal(summaries.length, 1, 'no eager summary pagination');
    assert.equal(params(summaries[0]).limit, '10');
    assert.equal(params(summaries[0]).role, 'user');
    assert.equal(params(summaries[0]).leafId, leafId);
    assert.equal(s.fixture.requests.filter(request => request.body.type === 'get_messages').length, 0);
    const initialSummary = s.responses('message-summaries')[0].body;
    assert.equal(initialSummary.messages.length, 10);
    assert(initialSummary.messages.every(message => message.role === 'user' && [...message.preview].length <= 140));
    pass('dense 20 × (user + 20 assistant/tool) fixture renders >=10 native nodes; first limit10; zero get_messages');

    await s.composer.fill(DRAFT);
    const live = await s.liveIds();
    assert(!live.includes('native-user-14'), 'chosen old node is outside the live 48-message suffix');
    const completed = s.responses('message-window').length;
    await s.page.setViewportSize({ width: 390, height: 844 });
    await s.page.locator('.timeline-list').evaluate(list => { list.scrollTop = 0; });
    const cdp = await s.context.newCDPSession(s.page);
    try {
      await cdp.send('Emulation.setTouchEmulationEnabled', { enabled: true, maxTouchPoints: 1 });
      const box = await s.tick('native-user-14').boundingBox();
      const point = { x: box.x + box.width / 2, y: box.y + box.height / 2, id: 1 };
      await cdp.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [point] });
      const tooltip = s.page.locator('.conversation-timeline').getByRole('tooltip');
      await tooltip.waitFor();
      assert.equal(await tooltip.locator('.timeline-label').textContent(), '问题 14：native 稠密工具历史。');
      await cdp.send('Input.dispatchTouchEvent', { type: 'touchMove', touchPoints: [{ ...point, y: point.y + 5 }] });
      await poll(async () => await s.page.locator('.timeline-list').getAttribute('data-scrubbing') === 'true', 'touch scrubbing native summaries');
      assert.equal(s.requests('message-window').length, 0, 'holding/dragging summary-only ticks previews without reading historical windows');
      await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
      await tooltip.waitFor({ state: 'hidden' });
    } finally {
      await cdp.detach();
      await s.page.setViewportSize({ width: 1440, height: 1000 });
    }
    await s.panel.waitFor();
    await poll(() => s.responses('message-window').length === completed + 1, 'anchor response');
    await s.panel.getByText('问题 14：native 稠密工具历史。', { exact: true }).waitFor();
    assert.equal(s.requests('message-window').length, 1);
    const anchorQuery = params(s.requests('message-window')[0]);
    assert.equal(anchorQuery.limit, '48'); assert.equal(anchorQuery.anchorId, 'native-user-14'); assert.equal(anchorQuery.leafId, leafId);
    assert.deepEqual(await s.liveIds(), live, 'history window must not prepend live transcript rows');
    assert.equal(await s.composer.inputValue(), DRAFT);
    pass('unloaded summary opens separately labelled history panel with exactly one pinned anchor window; live rows/draft unchanged');

    const anchored = s.responses('message-window').at(-1).body;
    const earlier = await windowAction(s, '更早的历史消息', { key: 'beforeId', value: anchored.nextBeforeId }, leafId);
    assert.equal(earlier.status, 200);
    const later = await windowAction(s, '更晚的历史消息', { key: 'afterId', value: earlier.body.nextAfterId }, leafId);
    assert.equal(later.status, 200);
    assert.deepEqual(await s.liveIds(), live);
    pass('earlier/later window buttons use host cursors, limit48 and the same leaf without growing live transcript');

    // Recoverable HTTP failures, including selected-leaf conflicts, must retain
    // the retryable request rather than replacing the live conversation.
    for (const status of [501, 409]) {
      s.fixture.controls.messageWindowStatus = status;
      s.fixture.controls.messageWindowFailCount = 1;
      await s.panel.getByRole('button', { name: '更早的历史消息', exact: true }).click();
      await s.panel.getByRole('button', { name: '重试历史消息', exact: true }).waitFor();
      const failed = params(s.requests('message-window').at(-1));
      const responseCount = s.responses('message-window').length;
      await s.panel.getByRole('button', { name: '重试历史消息', exact: true }).click();
      await poll(() => s.responses('message-window').length === responseCount + 1, 'retry response');
      await poll(async () => await s.panel.getByRole('button', { name: '重试历史消息', exact: true }).count() === 0, 'retry clears error');
      assert.deepEqual(params(s.requests('message-window').at(-1)), failed, 'retry preserves exact cursor/leaf/limit');
      assert.equal(s.responses('message-window').at(-1).status, 200);
      assert.deepEqual(await s.liveIds(), live);
    }
    s.fixture.controls.messageWindowStatus = undefined;
    pass('501 and 409 history errors expose retry and recover the exact pinned request');

    await s.panel.getByRole('button', { name: '返回实时消息', exact: true }).click();
    await s.panel.waitFor({ state: 'hidden' });
    assert.equal(await s.composer.inputValue(), DRAFT);
    const summaryCount = s.requests('message-summaries').length;
    await s.page.locator('.timeline-load-earlier').click();
    await poll(() => s.requests('message-summaries').length === summaryCount + 1, 'explicit earlier summary page');
    await s.tick('native-user-0').waitFor();
    const olderQuery = params(s.requests('message-summaries').at(-1));
    assert.equal(olderQuery.limit, '10'); assert.equal(olderQuery.beforeId, initialSummary.nextBeforeId); assert.equal(olderQuery.leafId, leafId);
    assert.deepEqual(await s.liveIds(), live);
    pass('timeline-load-earlier loads user metadata only using the fixed leaf and host before cursor');

    // Delay at the fixture, not a fabricated client response: the old request
    // really executes, but must not publish into the newly selected session.
    s.fixture.controls.messageWindowDelayMs = 700;
    const lateWindows = s.responses('message-window').length;
    const oldRequests = s.requests('message-window').length;
    await s.tick('native-user-0').click();
    await poll(() => s.requests('message-window').length === oldRequests + 1, 'pending old-session window');
    await s.select('architecture');
    await s.page.locator('.transcript').getByText('这是 梳理 Agent 架构 的独立历史。', { exact: true }).waitFor();
    await poll(() => s.responses('message-window').length === lateWindows + 1, 'late old-session response');
    await delay(150);
    assert.equal(await s.panel.count(), 0, 'late window does not reopen in another session');
    assert.equal(await s.page.locator('.transcript [data-history-id="native-user-0"]').count(), 0);
    s.fixture.controls.messageWindowDelayMs = 0;
    await s.select('dark'); await s.tick('native-user-19').waitFor();
    assert.equal(await s.composer.inputValue(), DRAFT, 'switching back preserves the dark-session draft');
    pass('late window completion is session-isolated; returning restores the local draft');

    await s.select('mobile'); await s.tick('mobile-user-10').waitFor();
    s.fixture.controls.messageSummariesDelayMs = 700;
    const mobileResponses = s.responses('message-summaries', 'mobile').length;
    const mobileRequests = s.requests('message-summaries', 'mobile').length;
    await s.page.locator('.timeline-load-earlier').click();
    await poll(() => s.requests('message-summaries', 'mobile').length === mobileRequests + 1, 'pending mobile summary');
    await s.select('architecture');
    await s.page.locator('.transcript').getByText('这是 梳理 Agent 架构 的独立历史。', { exact: true }).waitFor();
    await poll(() => s.responses('message-summaries', 'mobile').length === mobileResponses + 1, 'late mobile summary');
    await delay(150);
    assert.equal(await s.page.locator('.timeline-node[data-timeline-target^="entry:mobile-"]').count(), 0);
    assert.equal(await s.panel.count(), 0);
    assert.equal(s.fixture.requests.filter(request => request.body.type === 'get_messages').length, 0);
    noWrites(s);
    pass('late earlier-summary response cannot contaminate another session; native interactions never call get_messages');
    await s.page.screenshot({ path: 'artifacts/native-timeline.png', animations: 'disabled' });
    await s.close(); active = null;

    // Capability advertisement can race server deployment: unsupported metadata
    // routes must fall back to the already-loaded suffix, never scan all history.
    for (const status of [404, 501]) {
      const fallback = active = await setup(browser, fixture => { fixture.controls.messageSummariesStatus = status; });
      await poll(() => fallback.responses('message-summaries').some(response => response.status === status), 'unsupported summary route');
      await fallback.tick('native-user-19').waitFor();
      assert(await fallback.nodes.count() < 10, 'fallback is bounded to loaded user messages, not an eager history scan');
      assert.equal(fallback.fixture.requests.filter(request => request.body.type === 'get_messages').length, 0);
      assert.equal(fallback.requests('message-window').length, 0);
      assert.equal(await fallback.panel.count(), 0);
      noWrites(fallback);
      pass(`${status} summary endpoint falls back to loaded timeline without history scanning/model calls`);
      await fallback.close(); active = null;
    }
  } catch (error) {
    results.push({ failure: error.message });
    if (active) await active.page.screenshot({ path: 'artifacts/native-timeline-failure.png', animations: 'disabled' }).catch(() => {});
    throw error;
  } finally {
    if (active) await active.close();
    await browser.close();
    fs.writeFileSync('artifacts/native-timeline-validation.json', JSON.stringify({ results, runs, scope: 'already-built formal Web + isolated fixture; not native desktop acceptance' }, null, 2));
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
