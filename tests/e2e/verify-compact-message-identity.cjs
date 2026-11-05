// Real rpc/events.rs envelope IDs + compact deltas through formal WASM/fixture.
// No model calls. Equal text in distinct host entries must remain distinct.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');
(async () => {
  fs.mkdirSync('artifacts', { recursive: true });
  const fixture = await startFixture(0, { pagedHistory: true });
  const seed = { entryId: 'compact-old-user', role: 'user', content: 'Existing compact fixture history' };
  const persisted = [seed]; fixture.setMessages(persisted);
  const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
  const context = await browser.newContext({ viewport: { width: 1440, height: 1000 } });
  const page = await context.newPage(); page.setDefaultTimeout(15000);
  const errors = [], results = [];
  page.on('pageerror', error => errors.push(error.message));
  await context.route('https://api.github.com/**', route => route.fulfill({ status: 404, body: '{}' }));
  const pass = text => { results.push(text); console.log('PASS', text); };
  const history = id => page.locator(`[data-history-id="${id}"]`);
  const text = value => page.locator('.transcript').getByText(value, { exact: true });
  const emit = data => fixture.emit(data);
  async function one(value, count = 1) {
    await page.waitForFunction(({ value, count }) => [...document.querySelectorAll('.transcript .message-content p')].filter(el => el.textContent === value).length === count, { value, count });
    assert.equal(await text(value).count(), count, `render count: ${value}`);
  }
  const start = (id, timestamp = 42) => emit({ type: 'message_start', ...(id ? { messageId: id } : {}), message: { kind: 'assistant', timestamp, content: [] } });
  const delta = (id, value) => emit({ type: 'message_update', ...(id ? { messageId: id } : {}), eventType: 'text_delta', delta: value, contentIndex: 0, assistantMessageEvent: { type: 'text_delta', contentIndex: 0, delta: value } });
  function end(id, value, timestamp = 42) {
    const message = { kind: 'assistant', timestamp, content: [{ type: 'text', text: value }] };
    emit({ type: 'message_end', messageId: id, message });
    persisted.push({ ...message, entryId: id });
    // The generic fixture persists event.message verbatim. A real host adds the
    // session entry ID separately; install that actual canonical history here.
    fixture.setMessages(persisted);
  }
  async function settle(run) {
    const updated = page.waitForResponse(response => response.request().method() === 'GET' && new URL(response.url()).pathname.endsWith('/dark/snapshot'));
    emit({ type: 'agent_settled', runId: run, outcome: 'completed' });
    await (await updated).finished();
    for (const message of persisted) await history(message.entryId).waitFor();
    await page.getByRole('button', { name: '发送消息', exact: true }).waitFor();
  }
  try {
    await page.goto(process.env.WORK_UI_URL || 'http://127.0.0.1:14180/');
    if (!await page.getByRole('navigation', { name: '设置分类' }).isVisible()) await page.getByRole('button', { name: '打开设置', exact: true }).click();
    await page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '执行节点', exact: true }).click();
    await page.getByRole('button', { name: '连接其他节点', exact: true }).click();
    await page.getByLabel('节点地址', { exact: true }).fill(fixture.url);
    await page.getByRole('button', { name: '连接节点', exact: true }).click();
    await page.getByRole('status').filter({ hasText: '在线' }).first().waitFor();
    await page.getByRole('button', { name: '返回会话', exact: true }).click();
    if (!await page.locator('[data-session-id="dark"] .session-item').isVisible()) await page.locator('[data-project-id="rpi-work"] .project-row').click();
    await page.locator('[data-session-id="dark"] .session-item').click();
    await history(seed.entryId).waitFor();
    await page.waitForFunction(() => !document.querySelector('.composer textarea')?.disabled);
    await page.locator('.composer textarea').fill('Never sent; preserve this draft');

    const answer = 'COMPACT: real envelope identity';
    emit({ type: 'agent_start', runId: 'compact-first' });
    start('compact-first'); delta('compact-first', answer);
    await one(answer);
    delta('compact-first', ' with tail'); await one(`${answer} with tail`);
    end('compact-first', `${answer} with tail`);
    await one(`${answer} with tail`);
    assert.equal(await history('compact-first').count(), 1, 'full end owns the streamed row immediately');
    await settle('compact-first'); await one(`${answer} with tail`);
    await page.screenshot({ path: 'artifacts/compact-message-identity-after-fix.png' });
    pass('real envelope IDs and bodyless deltas share one row while streaming, at end and after canonical settlement');

    const promoted = 'COMPACT: late durable identity';
    emit({ type: 'agent_start', runId: 'compact-promoted' });
    start(null); delta(null, promoted); await one(promoted);
    delta('compact-promoted', ' promoted'); await one(`${promoted} promoted`);
    end('compact-promoted', `${promoted} promoted`, 43);
    await one(`${promoted} promoted`); await settle('compact-promoted'); await one(`${promoted} promoted`);
    pass('anonymous compact stream promotes to envelope identity without a second row even when final timestamp changes');

    const equal = 'COMPACT: legitimately repeated answer';
    for (const [index, id] of ['compact-equal-1', 'compact-equal-2'].entries()) {
      emit({ type: 'agent_start', runId: id }); start(id); delta(id, equal); end(id, equal);
      await one(equal, index + 1); await settle(id); await one(equal, index + 1);
    }
    pass('different authoritative IDs preserve both legitimate identical replies, including identical timestamps');

    // Reopen from persisted messages plus overlapping original envelope events.
    await page.locator('[data-session-id="architecture"] .session-item').click();
    await page.locator('[data-session-id="dark"] .session-item').click();
    await history('compact-first').waitFor();
    await one(`${answer} with tail`); await one(`${promoted} promoted`); await one(equal, 2);
    for (const message of persisted) assert.equal(await history(message.entryId).count(), 1);
    assert.equal(await page.locator('.composer textarea').inputValue(), 'Never sent; preserve this draft');
    pass('snapshot replay/reopen does not resurrect anonymous copies or compact delta fragments; draft is preserved');

    const reconnected = page.waitForResponse(response => response.request().method() === 'GET' && new URL(response.url()).pathname.endsWith('/dark/snapshot'));
    fixture.disconnect();
    await (await reconnected).finished();
    await page.waitForFunction(() => !document.querySelector('.composer textarea')?.disabled);
    await one(`${answer} with tail`); await one(`${promoted} promoted`); await one(equal, 2);
    assert.equal(fixture.requests.filter(request => ['prompt', 'queue_message'].includes(request.body.type)).length, 0);
    assert.deepEqual(errors, []);
    pass('reconnect retains unique identities with zero prompt/queue/model calls or application JavaScript errors');
    fs.writeFileSync('artifacts/compact-message-identity-validation.json', JSON.stringify({ results, errors }, null, 2));
  } catch (error) {
    await page.screenshot({ path: 'artifacts/compact-message-identity-failure.png' });
    fs.writeFileSync('artifacts/compact-message-identity-validation.json', JSON.stringify({ results, errors, fatal: error.message }, null, 2));
    throw error;
  } finally { await browser.close(); await fixture.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
