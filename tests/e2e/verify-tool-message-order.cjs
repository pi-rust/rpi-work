// Formal Web UI + isolated HTTP/WebSocket. No real node or model calls.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');

(async () => {
  fs.mkdirSync('artifacts', { recursive: true });
  const fixture = await startFixture(0, { pagedHistory: true, includeHistoryAssistant: true });
  const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
  const page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
  page.setDefaultTimeout(15000);
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  await page.route('https://api.github.com/**', route => route.fulfill({ status: 404, body: '{}' }));
  const history = id => page.locator(`[data-history-id="${id}"]`);
  const calls = () => page.locator('.transcript .inline-call').filter({
    has: page.locator('.call-name > strong').filter({ hasText: /^(subagent|order_read_)/ }),
  });
  const names = () => calls().locator('.call-name > strong').allTextContents();
  const assertCompactTools = async () => {
    const gaps = await calls().evaluateAll(cards => {
      const boxes = cards.map(card => card.getBoundingClientRect());
      return boxes.slice(1).map((box, index) => box.top - boxes[index].bottom);
    });
    for (const gap of gaps) assert(Math.abs(gap) <= 1, `live/history message boundaries must not split consecutive tool rows: ${gaps}`);
  };
  const expected = ['subagent'];
  function message(value) { fixture.emit({ type: 'message_end', message: value }); }
  function startCall(index, name) {
    const id = `order-tool-${index}`;
    const call = { type: 'toolCall', id, name, arguments: { path: `order-file-${index}.rs` } };
    const snapshot = { entryId: `order-call-${index}`, kind: 'assistant', timestamp: 1000 + index, content: [call] };
    fixture.emit({ type: 'message_start', message: snapshot });
    fixture.emit({ type: 'message_update', message: snapshot, eventType: 'tool_call_delta', delta: '' });
    fixture.emit({ type: 'message_end', message: snapshot });
    fixture.emit({ type: 'tool_execution_start', toolCallId: id, toolName: name, args: call.arguments });
  }
  function finishCall(index, name) {
    const result = [{ type: 'text', text: `order-result-${index}` }];
    fixture.emit({ type: 'tool_execution_end', toolCallId: `order-tool-${index}`, toolName: name, result: { content: result } });
    message({ entryId: `order-result-${index}`, role: 'toolResult', toolCallId: `order-tool-${index}`, toolName: name, content: result });
  }
  try {
    await page.goto(process.env.WORK_UI_URL || 'http://127.0.0.1:14180/');
    if (!await page.getByRole('navigation', { name: '设置分类' }).isVisible()) {
      await page.getByRole('button', { name: '打开设置', exact: true }).click();
    }
    await page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '工作协同', exact: true }).click();
    await page.getByRole('button', { name: '连接其他节点', exact: true }).click();
    await page.getByLabel('节点地址', { exact: true }).fill(fixture.url);
    await page.getByRole('button', { name: '连接节点', exact: true }).click();
    await page.getByRole('status').filter({ hasText: '在线' }).first().waitFor();
    await page.getByRole('button', { name: '返回会话', exact: true }).click();
    await page.locator('.project-row').filter({ hasText: 'rpi-work' }).click();
    await page.locator('[data-session-id="dark"] .session-item').click();
    await page.locator('.transcript').getByText('主题切换已经准备好，接下来运行组件测试。', { exact: true }).waitFor();
    await page.waitForFunction(() => !document.querySelector('.composer-main textarea')?.disabled);
    assert(fixture.requests.some(request => request.path === '/api/v1/realtime' && request.sessionId === 'dark'));

    message({ entryId: 'order-user', role: 'user', content: '工具调用消息顺序回归。' });
    fixture.emit({ type: 'agent_start', runId: 'order-run' });
    startCall(0, 'subagent');
    await history('order-call-0').locator('.inline-call').waitFor();
    assert.equal(await calls().count(), 1, 'call-only snapshot must own the running card');
    await history('order-call-0').locator('.inline-call').evaluate(card => { card.open = true; });
    assert((await history('order-call-0').locator('.call-arguments').innerText()).includes('order-file-0.rs'));
    finishCall(0, 'subagent');
    await history('order-call-0').locator('.call-output').filter({ hasText: 'order-result-0' }).waitFor();
    assert.equal(await calls().count(), 1, 'result must update its original call, not add an orphan card');
    console.log('PASS call-only child-agent message retains its anchor, parameters and one result card');

    // A long parent turn exceeds the 48-message canonical suffix. Complete one
    // pair in reverse order to ensure results never determine call ordering.
    for (let index = 1; index <= 30; index++) {
      const name = `order_read_${String(index).padStart(2, '0')}`;
      expected.push(name);
      startCall(index, name);
      if (index === 1) continue;
      finishCall(index, name);
      if (index === 2) finishCall(1, 'order_read_01');
    }
    await history('order-call-30').locator('.call-name > strong').waitFor();
    assert.deepEqual(await names(), expected);
    await assertCompactTools();
    message({ entryId: 'order-answer', role: 'assistant', content: '本轮工具全部完成。' });
    await history('order-answer').waitFor();
    fixture.emit({ type: 'agent_end' });
    fixture.emit({ type: 'agent_settled', runId: 'order-run', outcome: 'completed' });
    await page.getByRole('button', { name: '发送消息', exact: true }).waitFor();
    assert.deepEqual(await names(), expected, 'bounded settlement must preserve older call rows and order');
    assert.equal(await history('order-call-0').count(), 1);
    await assertCompactTools();
    console.log('PASS long-turn settlement and reverse result completion preserve all 31 call anchors and compact spacing');

    message({ entryId: 'order-next-user', role: 'user', content: '下一轮保持消息位置。' });
    fixture.emit({ type: 'agent_start', runId: 'order-next-run' });
    message({ entryId: 'order-next-answer', role: 'assistant', content: '下一轮回复。' });
    fixture.emit({ type: 'agent_end' });
    fixture.emit({ type: 'agent_settled', runId: 'order-next-run', outcome: 'completed' });
    await history('order-next-answer').waitFor();
    await page.getByRole('button', { name: '发送消息', exact: true }).waitFor();
    assert.deepEqual(await names(), expected);
    assert.deepEqual(await page.locator('[data-history-id^="order-"]').evaluateAll(rows => rows
      .filter(row => row.querySelector('.timeline-message'))
      .map(row => row.dataset.historyId)), [
      'order-user', ...Array.from({ length: 31 }, (_, index) => `order-call-${index}`),
      'order-answer', 'order-next-user', 'order-next-answer',
    ]);
    assert.deepEqual(errors, []);
    assert(!fixture.requests.some(request => ['prompt', 'queue_message'].includes(request.body.type)));
    await page.screenshot({ path: 'artifacts/tool-message-order.png', animations: 'disabled' });
    fs.writeFileSync('artifacts/tool-message-order-validation.json', JSON.stringify({ passed: true, calls: expected.length, errors }, null, 2));
    console.log('PASS next turn leaves previous calls in chronological positions without duplicates or model requests');
  } catch (error) {
    await page.screenshot({ path: 'artifacts/tool-message-order-failure.png', animations: 'disabled' }).catch(() => {});
    throw error;
  } finally {
    await browser.close();
    await fixture.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
