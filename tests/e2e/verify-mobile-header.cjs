// Actual WASM + isolated fixture. Header status presentation never calls a model.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { chromium } = require('../support/browser.cjs');
const { startFixture, fixtureCommand } = require('../fixtures/ui-fixture.cjs');
(async () => {
  fs.mkdirSync('artifacts', { recursive: true });
  const fixture = await startFixture(0);
  const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
  const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
  const results = [], errors = [], geometry = [];
  let blockEvents = false;
  page.setDefaultTimeout(10000); page.on('pageerror', error => errors.push(error.message));
  page.on('websocket', socket => { if (blockEvents && socket.url().includes('/api/v1/realtime')) socket.close(); });
  const pass = text => { results.push(text); console.log('PASS', text); };
  const task = () => page.locator('.top-actions > .status-badge');
  const node = () => page.locator('.top-actions > .connection-badge');
  const check = async (badge, label, icon, width) => {
    await page.waitForFunction(({ selector, label, icon }) => {
      const badge = document.querySelector(selector);
      return badge?.getAttribute('aria-label') === label && badge.querySelector('svg')?.dataset.icon === icon;
    }, { selector: badge === 'task' ? '.top-actions > .status-badge' : '.top-actions > .connection-badge', label, icon });
    const element = badge === 'task' ? task() : node();
    assert.equal(await element.getAttribute('title'), label);
    assert.equal(await element.getAttribute('role'), 'img');
    const proof = await element.evaluate(e => {
      const rect = e.getBoundingClientRect(), svg = e.querySelector('svg').getBoundingClientRect();
      return { width: rect.width, height: rect.height, icon: [svg.width, svg.height], clip: getComputedStyle(e.querySelector('.header-status-label')).clipPath, background: getComputedStyle(e).backgroundColor, color: getComputedStyle(e).color };
    });
    if (width <= 700) {
      assert.deepEqual([proof.width, proof.height], [28, 28]);
      assert.deepEqual(proof.icon, [14, 14]);
      assert.equal(proof.clip, 'inset(50%)', 'label must not consume mobile header width');
      assert.equal(proof.background, 'rgba(0, 0, 0, 0)');
      assert.equal(await element.evaluate(e => e.querySelector('i') ? getComputedStyle(e.querySelector('i')).display : getComputedStyle(e, '::before').display), 'none');
    } else {
      assert.equal(proof.clip, 'none');
      assert.deepEqual(proof.icon, [0, 0]);
      assert.equal(await element.locator('.header-status-label').innerText(), label);
    }
    geometry.push({ width, badge, label, icon, ...proof });
  };
  const checkHeader = async width => {
    const bounds = await page.locator('.app-header').evaluate(header => {
      const rect = e => { const r = e.getBoundingClientRect(); return { left: r.left, right: r.right, width: r.width }; };
      const quick = header.querySelector('.quick-session');
      return { header: rect(header), title: rect(rect(quick).width ? quick : header.querySelector('.breadcrumb')), actions: rect(header.querySelector('.top-actions')) };
    });
    assert(bounds.title.width >= 24 && bounds.title.right <= bounds.actions.left + 1, JSON.stringify(bounds));
    assert(bounds.actions.right <= width + 1, JSON.stringify(bounds));
    assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
  };
  try {
    await page.route('https://api.github.com/**', route => route.fulfill({ status: 404, body: '{}' }));
    await page.goto(process.env.WORK_UI_URL || 'http://127.0.0.1:14180/');
    await page.setViewportSize({ width: 320, height: 844 });
    await check('task', '任务状态未知', 'info', 320);
    await check('node', '节点未连接', 'close', 320);
    await page.setViewportSize({ width: 1280, height: 900 });
    await page.getByRole('button', { name: '打开设置', exact: true }).click();
    await page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '工作协同', exact: true }).click();
    await page.getByRole('button', { name: '连接其他节点', exact: true }).click();
    await page.getByLabel('节点地址', { exact: true }).fill(fixture.url);
    await page.getByRole('button', { name: '连接节点', exact: true }).click();
    await page.locator('.server-details [role=status]').filter({ hasText: '在线' }).waitFor();
    await page.getByRole('button', { name: '返回会话', exact: true }).click();
    await page.locator('.project-row').filter({ hasText: 'rpi-work' }).click();
    await page.locator('[data-session-id="dark"] .session-item').click();
    await page.locator('.composer textarea').fill('顶部只显示图标，保留草稿');
    for (const mode of ['light', 'dark']) {
      await page.getByRole('button', { name: '外观与主题', exact: true }).click();
      const appearance = page.getByRole('dialog', { name: '外观与主题', exact: true });
      await appearance.getByLabel('明暗模式', { exact: true }).selectOption(mode);
      await appearance.getByRole('button', { name: '保存', exact: true }).click();
      await appearance.waitFor({ state: 'hidden' });
      for (const width of [1280, 390, 320]) {
        await page.setViewportSize({ width, height: 844 });
        await check('task', '已完成', 'check', width);
        await check('node', '节点在线', 'server', width);
        await checkHeader(width);
        fixture.emit({ type: 'agent_start', runId: `header-${mode}-${width}` });
        await check('task', '已接收', 'clock', width);
        fixture.emit({ type: 'message_update', message: { entryId: `header-answer-${mode}-${width}`, role: 'assistant' }, eventType: 'text_delta', delta: '正文正在生成。' });
        await check('task', '生成文本', 'edit', width);
        await page.locator('.app-header').screenshot({ path: `artifacts/mobile-header-${mode}-${width}-text.png`, animations: 'disabled' });
        fixture.emit({ type: 'tool_execution_start', toolCallId: `header-read-${mode}-${width}`, toolName: 'read', args: { path: 'README.md' } });
        await check('task', '执行工具', 'tool', width);
        fixture.emit({ type: 'tool_execution_end', toolCallId: `header-read-${mode}-${width}`, result: { content: [{ type: 'text', text: 'fixture output' }] } });
        fixture.emit({ type: 'ui_dialog_request', requestId: `header-ask-${mode}-${width}`, ui: { purpose: 'question', kind: 'input', question: '只显示等待状态，不自动回答' } });
        await check('task', '等待确认', 'ask', width);
        fixture.emit({ type: 'ui_dialog_closed', requestId: `header-ask-${mode}-${width}` });
        fixture.emit({ type: 'agent_settled', outcome: 'failed' });
        await check('task', '执行失败', 'info', width);
        await page.locator('.app-header').screenshot({ path: `artifacts/mobile-header-${mode}-${width}-failed.png`, animations: 'disabled' });
        fixture.emit({ type: 'agent_settled', outcome: 'interrupted' });
        await check('task', '已中断', 'close', width);
        fixture.emit({ type: 'agent_settled', outcome: 'completed' });
        await check('task', '已完成', 'check', width);
        assert.equal(await page.locator('.composer textarea').inputValue(), '顶部只显示图标，保留草稿');
        pass(`${mode}/${width}: task/node status stays named and truthful; mobile uses 14px icons and desktop keeps labels`);
      }
      await page.setViewportSize({ width: 1280, height: 900 });
    }
    await page.setViewportSize({ width: 320, height: 844 });
    blockEvents = true; fixture.disconnect('dark', true);
    await check('node', '节点重连中', 'refresh', 320);
    await check('task', '任务状态未知', 'info', 320);
    await page.locator('.app-header').screenshot({ path: 'artifacts/mobile-header-reconnecting-320.png', animations: 'disabled' });
    blockEvents = false;
    await check('node', '节点在线', 'server', 320);
    await checkHeader(320);
    assert.equal(fixture.requests.filter(r => ['prompt', 'queue_message', 'ui_dialog_response'].includes(fixtureCommand(r))).length, 0);
    assert.deepEqual(errors, []);
    pass('disconnect/reconnect switches icon and accessible name; no model calls, implicit replies or application errors');
  } catch (error) {
    await page.screenshot({ path: 'artifacts/mobile-header-failure.png', animations: 'disabled' }); throw error;
  } finally {
    fs.writeFileSync('artifacts/mobile-header-validation.json', JSON.stringify({ results, geometry, errors }, null, 2));
    await browser.close(); await fixture.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
