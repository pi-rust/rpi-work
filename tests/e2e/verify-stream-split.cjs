// Formal WASM + isolated HTTP/SSE. No user session or model requests.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');
(async () => {
  const fixture = await startFixture(0, { pagedHistory: true });
  const seed = { entryId: 'split-user', role: 'user', content: 'Synthetic stream identity regression' };
  const id = 'split-assistant';
  const prefix = 'STREAM SLOT: first half';
  const secret = 'PRIVATE THINKING MUST STAY HIDDEN';
  const structure = text => [{ type: 'thinking', thinking: secret }, { type: 'text', text }];
  const emit = event => fixture.emit(event);
  const delta = text => emit({ type: 'message_update', messageId: id, eventType: 'text_delta', contentIndex: 1, delta: text });
  fixture.setMessages([seed]);
  emit({ type: 'agent_start', runId: 'split-run' });
  emit({ type: 'message_start', messageId: id, message: { kind: 'assistant', content: [] } });
  delta(prefix);
  emit({ type: 'message_update', messageId: id, contentIndex: 1, message: { kind: 'assistant', content: structure(prefix) } });
  const results = [], errors = [];
  const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
  const page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
  page.setDefaultTimeout(12000);
  page.on('pageerror', error => errors.push(error.message));
  await page.route('https://api.github.com/**', route => route.fulfill({ status: 404, body: '{}' }));
  const pass = text => { results.push(text); console.log('PASS', text); };
  const exact = value => page.locator('.transcript').getByText(value, { exact: true });
  const one = async value => {
    await exact(value).first().waitFor();
    assert.equal(await exact(value).count(), 1, 'same public text must have one owner');
    assert(!(await page.locator('.transcript').innerText()).includes(secret));
    assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
  };
  try {
    await page.goto(process.env.WORK_UI_URL || 'http://127.0.0.1:14180/');
    await page.getByRole('button', { name: '打开设置', exact: true }).click();
    await page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '工作协同', exact: true }).click();
    await page.getByRole('button', { name: '连接其他节点', exact: true }).click();
    await page.getByLabel('节点地址', { exact: true }).fill(fixture.url);
    await page.getByRole('button', { name: '连接节点', exact: true }).click();
    await page.getByRole('status').filter({ hasText: '在线' }).first().waitFor();
    await page.getByRole('button', { name: '返回会话', exact: true }).click();
    const session = page.locator('[data-session-id="dark"] .session-item');
    if (!await session.isVisible()) await page.locator('[data-project-id="rpi-work"] .project-row').click();
    await session.click();
    await exact(prefix).first().waitFor();
    // Connect/select the synthetic loopback node on desktop, then exercise the
    // real phone layout without weakening its LAN address policy.
    await page.setViewportSize({ width: 390, height: 844 });
    await one(prefix);
    pass('event-only restoration with thinking slot 0 and public text slot 1 has no frozen copy');
    const firstRow = await exact(prefix).evaluateHandle(el => el.closest('[data-message-row]'));
    delta(' continues');
    await one(prefix + ' continues');
    assert.equal(await exact(prefix).count(), 0, 'old partial must not remain above continuing text');
    assert(await exact(prefix + ' continues').evaluate((el, first) => el.closest('[data-message-row]') === first, firstRow));
    pass('live compact delta updates the original display row and leaves no stopped upper partial');
    const restored = page.waitForResponse(r => r.request().method() === 'GET' && new URL(r.url()).pathname.endsWith('/dark/snapshot'));
    fixture.disconnect();
    await (await restored).finished();
    await one(prefix + ' continues');
    delta(' after reconnect');
    const final = prefix + ' continues after reconnect';
    await one(final);
    assert.equal(await exact(prefix).count(), 0);
    assert.equal(await exact(prefix + ' continues').count(), 0);
    pass('reconnection replays structured snapshot and resumes the correct text slot without duplicates');
    emit({ type: 'message_end', messageId: id, message: { entryId: id, role: 'assistant', content: structure(final) } });
    emit({ type: 'agent_settled', runId: 'split-run', outcome: 'completed' });
    await one(final);
    await page.locator(`[data-history-id="${id}"]`).waitFor();
    for (const other of ['same-a', 'same-b']) {
      emit({ type: 'agent_start', runId: other });
      emit({ type: 'message_start', messageId: other, message: { kind: 'assistant', content: [] } });
      emit({ type: 'message_update', messageId: other, eventType: 'text_delta', contentIndex: 1, delta: 'LEGITIMATE SAME CONTENT' });
      emit({ type: 'message_end', messageId: other, message: { entryId: other, role: 'assistant', content: structure('LEGITIMATE SAME CONTENT') } });
      emit({ type: 'agent_settled', runId: other, outcome: 'completed' });
      await page.locator(`[data-history-id="${other}"]`).waitFor();
    }
    assert.equal(await exact('LEGITIMATE SAME CONTENT').count(), 2);
    await one(final);
    pass('different durable message IDs keep both legitimate identical replies');
    await page.screenshot({ path: 'artifacts/stream-split-mobile-fixed.png', animations: 'disabled' });
    assert.equal(fixture.requests.filter(r => ['prompt', 'queue_message'].includes(r.body?.type)).length, 0);
    assert.deepEqual(errors, []);
    pass('no private thinking leak, horizontal overflow, prompt/queue calls or application errors');
  } catch (error) {
    await page.screenshot({ path: 'artifacts/stream-split-mobile-failure.png', animations: 'disabled' });
    throw error;
  } finally {
    fs.writeFileSync('artifacts/stream-split-mobile-validation.json', JSON.stringify({ scope: 'Formal WASM mobile UI; isolated HTTP/SSE and structured thinking/text snapshots; no real model or Android hardware', results, errors }, null, 2));
    await browser.close(); await fixture.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
