// Formal WASM + isolated fixture only; one fixture-only side prompt, no real model.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');
(async () => {
  const fixture = await startFixture();
  const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
  const page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
  const results = [], errors = [], blocked = [];
  const web = process.env.WORK_UI_URL || 'http://127.0.0.1:14180/';
  page.setDefaultTimeout(12000);
  page.on('pageerror', error => errors.push(error.message));
  const pass = name => { results.push(name); console.log('PASS', name); };
  await page.route('**/*', async route => {
    const request = route.request(), url = new URL(request.url());
    if (url.origin === new URL(fixture.url).origin) return route.continue();
    if (url.origin === new URL(web).origin) {
      if (url.pathname.startsWith('/api/v1/')) return route.fulfill({ response: await route.fetch({ url: new URL(url.pathname + url.search, fixture.url).href }) });
      return route.continue();
    }
    if (url.hostname === 'api.github.com') return route.fulfill({ status: 404, body: '{}' });
    blocked.push(request.url()); return route.abort('blockedbyclient');
  });
  try {
    await page.goto(web);
    await page.getByRole('button', { name: '打开设置', exact: true }).click();
    await page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '执行节点', exact: true }).click();
    await page.getByRole('button', { name: '连接其他节点', exact: true }).click();
    await page.getByLabel('节点地址', { exact: true }).fill(fixture.url);
    await page.getByRole('button', { name: '连接节点', exact: true }).click();
    await page.locator('.server-details [role=status]').filter({ hasText: '在线' }).waitFor();
    await page.getByRole('button', { name: '返回会话', exact: true }).click();
    await page.locator('.project-row').filter({ hasText: 'rpi-work' }).click();
    await page.locator('[data-session-id="dark"] .session-item').click();
    await page.locator('.composer-main textarea').fill('/btw');
    await page.locator('.composer-main textarea').press('Enter');
    const panel = page.getByRole('complementary', { name: 'BTW 独立旁问会话', exact: true });
    await panel.waitFor();
    await page.locator('.composer-main textarea').fill('主对话草稿必须保留，不发送');
    await panel.getByLabel('旁问消息', { exact: true }).fill('fixture-only identity question');
    const accepted = page.waitForResponse(response => response.request().method() === 'POST' && response.url().endsWith('/commands') && response.request().postDataJSON()?.type === 'prompt');
    await panel.getByRole('button', { name: '发送旁问', exact: true }).click();
    const response = await accepted; await response.finished();
    const session = new URL(response.url()).pathname.split('/')[4];
    await panel.locator('.btw-session-id').filter({ hasText: session }).waitFor();
    await panel.locator('.btw-boundary-summary').filter({ hasText: '执行节点工具为空（已确认）' }).waitFor();
    const answer = '旁问的流式行和完成历史只能有一条。';
    fixture.emit({ type: 'agent_start', runId: 'btw-identity' }, session);
    fixture.emit({ type: 'message_start', message: { role: 'assistant', timestamp: 61000, content: [] } }, session);
    fixture.emit({ type: 'message_update', message: { role: 'assistant', timestamp: 61000 }, eventType: 'text_delta', delta: answer }, session);
    await panel.locator('.btw-answer').getByText(answer, { exact: true }).waitFor();
    fixture.emit({ type: 'message_end', message: { entryId: 'btw-formal', role: 'assistant', timestamp: 61001, content: answer } }, session);
    // A second end barrier ensures the first end publication was consumed.
    fixture.emit({ type: 'message_end', message: { entryId: 'btw-barrier', role: 'assistant', content: '旁问完成屏障' } }, session);
    await panel.locator('.btw-answer').getByText('旁问完成屏障', { exact: true }).waitFor();
    assert.equal(await panel.locator('.btw-answer').getByText(answer, { exact: true }).count(), 1);
    pass('BTW projection completion replaces its cached row once, including timestamp-revised canonical promotion');
    const settled = page.waitForResponse(r => r.request().method() === 'GET' && new URL(r.url()).pathname === `/api/v1/sessions/${session}/snapshot`);
    fixture.emit({ type: 'agent_settled', runId: 'btw-identity', outcome: 'completed' }, session);
    await (await settled).finished();
    await panel.locator('.btw-status').filter({ hasText: '节点已结算' }).waitFor();
    assert.equal(await panel.locator('.btw-answer').getByText(answer, { exact: true }).count(), 1);
    pass('BTW anonymous completed owner retires on formal settlement without losing adjacent answers');
    await panel.getByRole('button', { name: '关闭旁问面板', exact: true }).click();
    assert.equal(await page.locator('.composer-main textarea').inputValue(), '主对话草稿必须保留，不发送');
    await page.locator('.composer-main textarea').fill('/btw');
    await page.locator('.composer-main textarea').press('Enter');
    await panel.waitFor();
    await page.locator('.composer-main textarea').fill('主对话草稿必须保留，不发送');
    await panel.locator('.btw-boundary-summary').filter({ hasText: '执行节点工具为空（已确认）' }).waitFor();
    assert.equal(await panel.locator('.btw-answer').getByText(answer, { exact: true }).count(), 1);
    assert.equal(await panel.locator('.btw-answer').getByText('旁问完成屏障', { exact: true }).count(), 1);
    assert.equal(await page.locator('.composer-main textarea').inputValue(), '主对话草稿必须保留，不发送');
    const sends = fixture.requests.filter(request => ['prompt', 'queue_message', 'abort'].includes(request.body?.type));
    assert.equal(sends.length, 1); assert.equal(sends[0].path, `/api/v1/sessions/${session}/commands`);
    assert.equal(sends[0].body.message, 'fixture-only identity question');
    assert.deepEqual(errors, []); assert.deepEqual(blocked, []);
    pass('BTW close/reopen keeps one answer, adjacent history and main draft; only one isolated side prompt and no real model calls');
  } catch (error) {
    results.push({ failure: error.message }); throw error;
  } finally {
    fs.writeFileSync('artifacts/btw-message-identity-validation.json', JSON.stringify({ results, errors, blocked, scope: 'isolated fixture only; not native desktop or Android acceptance' }, null, 2));
    await browser.close(); await fixture.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
