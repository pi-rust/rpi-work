// Formal Web UI + isolated HTTP/SSE fixture. No model, shell or user-node calls.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');
(async () => {
  fs.mkdirSync('artifacts', { recursive: true });
  const fixture = await startFixture(0);
  const commands = {
    fixture_multiline: Array.from({ length: 9 }, (_, i) => `echo 'line ${i + 1} 中文 <script>window.__commandXss=1</script>'`).join('\n') + '\n',
    fixture_wrapped: `echo '${'long-command-中文-'.repeat(100)}'`,
    fixture_short: 'echo short',
    fixture_responsive: `echo '${'w'.repeat(160)}'`,
  };
  fixture.setMessages(Object.entries(commands).flatMap(([name, command]) => [
    { entryId: `${name}-call`, role: 'assistant', content: [{ type: 'toolCall', id: name, name, arguments: { command, timeout: 120 } }] },
    { entryId: `${name}-result`, role: 'toolResult', toolCallId: name, content: [{ type: 'text', text: `Complete output of ${name}` }] },
  ]));
  fixture.setQueue([{ entryId: 'preview-queue', queue: 'followUp', text: '保留队列功能', status: 'queued' }]);
  const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
  const context = await browser.newContext({ viewport: { width: 1440, height: 1000 }, permissions: ['clipboard-read', 'clipboard-write'] });
  const page = await context.newPage(), results = [], errors = [];
  page.setDefaultTimeout(15000);
  page.on('pageerror', error => errors.push(error.message));
  const pass = text => { results.push(text); console.log('PASS', text); };
  const card = name => page.locator('.inline-call').filter({ has: page.locator('.call-name > strong').filter({ hasText: new RegExp(`^${name}$`) }) });
  const metrics = name => card(name).locator('.command-text').evaluate(e => ({ height: e.clientHeight, total: e.scrollHeight, line: parseFloat(getComputedStyle(e).lineHeight), text: e.textContent }));
  const collapsed = async name => {
    await card(name).getByRole('button', { name: '展开完整命令', exact: true }).waitFor();
    const m = await metrics(name);
    assert(m.total > m.height, `${name} must overflow`);
    assert(Math.abs(m.height - m.line * 4) <= 1, `${name}: exactly four rendered lines, ${JSON.stringify(m)}`);
    assert.equal(m.text, commands[name]);
  };
  const copy = async name => {
    await card(name).getByRole('button', { name: '复制完整命令', exact: true }).click();
    await card(name).locator('.command-preview').getByText('已复制', { exact: true }).waitFor();
    await page.waitForFunction(async expected => (await navigator.clipboard.readText()).replace(/\r\n/g, '\n') === expected, commands[name]);
    assert.equal((await page.evaluate(() => navigator.clipboard.readText())).replace(/\r\n/g, '\n'), commands[name]);
  };
  await context.route('https://api.github.com/**', route => route.fulfill({ status: 404, body: '{}' }));
  await context.route(url => url.origin === new URL(fixture.url).origin && url.pathname === '/api/v1/sessions', async route => {
    const response = await route.fetch(), data = await response.json();
    for (const session of data.sessions) {
      if (session.sessionId === 'architecture') { session.pendingResume = true; session.status = 'interrupted'; }
      if (session.sessionId === 'mobile') session.status = 'awaiting_approval';
      if (session.sessionId === 'rpc') session.status = 'offline';
    }
    await route.fulfill({ response, json: data });
  });
  try {
    await page.goto(process.env.WORK_UI_URL || 'http://127.0.0.1:14180/');
    await page.getByRole('button', { name: '打开设置', exact: true }).click();
    await page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '执行节点', exact: true }).click();
    await page.getByRole('button', { name: '连接其他节点', exact: true }).click();
    await page.getByLabel('节点地址', { exact: true }).fill(fixture.url);
    await page.getByRole('button', { name: '连接节点', exact: true }).click();
    await page.getByRole('status').filter({ hasText: '在线' }).first().waitFor();
    await page.getByRole('button', { name: '返回会话', exact: true }).click();
    // Expand without project auto-resume selecting the synthetic waiting mobile
    // session: its completed snapshot would legitimately supersede list status.
    const toggle = page.locator('[data-project-id="rpi-work"] .project-toggle');
    if (await toggle.getAttribute('aria-expanded') !== 'true') await toggle.click();
    await page.locator('[data-session-id="dark"] .session-item').click();
    await page.locator('[data-session-id="architecture"] .session-dot[title="待恢复"]').waitFor();
    await page.locator('[data-session-id="mobile"] .session-dot[title="等待处理"]').waitFor();
    assert.equal(await page.locator('[data-session-id="dark"] .session-dot').count(), 0);
    const activeSession = await page.locator('.session-entry.selected').getAttribute('data-session-id');
    assert.equal(fixture.requests.filter(r => {
      const match = r.path.match(/sessions\/(architecture|mobile|rpc)\/(snapshot|history)$/);
      return match && match[1] !== activeSession;
    }).length, 0, 'status badges must not scan unselected history');
    pass('explicit pendingResume is distinct from approval wait; idle is not treated as interrupted and no per-session history scan occurs');

    await page.locator('.sidebar-sessions').getByRole('button', { name: '为工作台添加深色模式', exact: true }).click();
    await page.locator('.composer textarea').fill('预览和复制不得发送命令或清空草稿');
    for (const name of Object.keys(commands)) await card(name).locator(':scope > summary').click();
    await collapsed('fixture_multiline'); await collapsed('fixture_wrapped');
    assert.equal(await card('fixture_short').locator('.command-toggle').count(), 0);
    assert.equal((await metrics('fixture_short')).text, commands.fixture_short);
    assert.equal(await card('fixture_responsive').locator('.command-toggle').count(), 0);
    await copy('fixture_multiline'); await copy('fixture_wrapped');
    assert.equal(await page.evaluate(() => window.__commandXss), undefined);
    assert.equal(await card('fixture_multiline').locator('script').count(), 0);
    pass('multiline and one-line soft-wrapped commands preview four real lines; copying retains every byte and command text never executes');

    await card('fixture_multiline').getByRole('button', { name: '展开完整命令' }).click();
    const full = await metrics('fixture_multiline');
    assert(full.height > full.line * 4);
    assert(Math.abs(full.height - full.total) <= 1);
    await collapsed('fixture_wrapped');
    assert.equal(await card('fixture_multiline').locator('.call-output').innerText(), 'Complete output of fixture_multiline');
    await card('fixture_multiline').locator('.command-raw-arguments > summary').click();
    assert.deepEqual(JSON.parse(await card('fixture_multiline').locator('.call-arguments').innerText()), { command: commands.fixture_multiline, timeout: 120 });
    await card('fixture_multiline').locator(':scope > summary').click();
    await card('fixture_multiline').locator(':scope > summary').click();
    await card('fixture_multiline').getByRole('button', { name: '收起命令' }).waitFor();
    await card('fixture_multiline').getByRole('button', { name: '收起命令' }).click();
    await collapsed('fixture_multiline');
    pass('command expansion is independent per card and preserves raw arguments, full output and outer details state');

    const shelf = page.getByRole('region', { name: '消息队列摘要', exact: true });
    await shelf.waitFor();
    assert.equal(await shelf.locator('.queue-shelf-title').count(), 0);
    assert.match(await shelf.locator('.queue-shelf-summary').innerText(), /1 条待执行/);
    await shelf.getByRole('button', { name: '打开消息队列', exact: true }).click();
    const queue = page.getByRole('dialog', { name: '消息队列', exact: true });
    await queue.waitFor(); await queue.getByText('保留队列功能', { exact: true }).waitFor();
    await page.keyboard.press('Escape'); await queue.waitFor({ state: 'hidden' });
    pass('queue shelf removes only the visible heading, retaining accessible entry, counts, items and dialog');

    for (const width of [390, 320]) {
      await page.setViewportSize({ width, height: 844 });
      await collapsed('fixture_multiline'); await collapsed('fixture_wrapped'); await collapsed('fixture_responsive');
      assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
      const button = card('fixture_wrapped').getByRole('button', { name: '展开完整命令', exact: true });
      assert((await button.boundingBox()).height >= 44);
      await page.screenshot({ path: `artifacts/conversation-polish-${width}.png`, animations: 'disabled' });
    }
    assert.equal(await page.locator('.composer textarea').inputValue(), '预览和复制不得发送命令或清空草稿');
    await page.setViewportSize({ width: 1440, height: 1000 });
    await card('fixture_responsive').locator('.command-toggle').waitFor({ state: 'detached' });
    commands.fixture_live = 'echo before-update';
    fixture.emit({ type: 'tool_execution_start', toolCallId: 'fixture_live', toolName: 'fixture_live', args: { command: commands.fixture_live } });
    await card('fixture_live').locator(':scope > summary').click();
    await copy('fixture_live');
    commands.fixture_live = 'echo updated';
    fixture.emit({ type: 'tool_execution_start', toolCallId: 'fixture_live', toolName: 'fixture_live', args: { command: commands.fixture_live } });
    await card('fixture_live').locator('.command-text').filter({ hasText: /^echo updated$/ }).waitFor();
    await copy('fixture_live');
    await page.locator('[data-session-id="architecture"] .session-item').click();
    await page.waitForFunction(() => window.__rpiCommandPreviews?.size === document.querySelectorAll('.command-preview').length);
    pass('width-only changes add/remove the overflow control, message updates copy the latest command and session switching cleans observers');
    assert.equal(fixture.requests.filter(r => ['prompt', 'bash', 'exec', 'queue_message', 'resume_pending'].includes(r.body.type)).length, 0);
    assert.deepEqual(errors, []);
    pass('390/320px preserve four rendered lines, touch targets and drafts with zero execution calls or JS errors');
  } catch (error) {
    await page.screenshot({ path: 'artifacts/conversation-polish-failure.png', animations: 'disabled' });
    throw error;
  } finally {
    fs.writeFileSync('artifacts/conversation-polish-validation.json', JSON.stringify({ results, errors }, null, 2));
    await browser.close(); await fixture.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
