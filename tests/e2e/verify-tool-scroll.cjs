// Real WASM + isolated HTTP/SSE fixture; commands and paths are inert test data.
// Checks native wheel/touch scrolling rather than only inspecting CSS properties.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');
const UI = process.env.WORK_UI_URL || 'http://127.0.0.1:14180/';
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
(async () => {
  fs.mkdirSync('artifacts', { recursive: true });
  const fixture = await startFixture(0);
  const output = Array.from({ length: 140 }, (_, i) => `line ${i}: 工具输出只作为测试数据，不执行命令。`).join('\n');
  fixture.setMessages([
    { entryId: 'scroll-user', role: 'user', content: '检查工具和外层会话的滚动。' },
    { entryId: 'scroll-reply', role: 'assistant', content: [
      { type: 'text', text: '工具前的输出消息，不应再有独立复制栏。' },
      { type: 'toolCall', id: 'scroll-read', name: 'read', arguments: { path: 'fixture/long-output.txt' } },
    ] },
    { entryId: 'scroll-result', role: 'toolResult', toolCallId: 'scroll-read', toolName: 'read', content: [{ type: 'text', text: output }] },
    ...Array.from({ length: 14 }, (_, i) => ({ entryId: `scroll-after-${i}`, role: 'assistant', content: `工具后的数据段落 ${i}。${'用于保留充足的外层滚动范围。'.repeat(36)}` })),
  ]);
  const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
  const errors = [];
  try {
    const context = await browser.newContext({ viewport: { width: 1440, height: 1000 }, hasTouch: true, permissions: ['clipboard-read', 'clipboard-write'] });
    const page = await context.newPage();
    page.setDefaultTimeout(12000);
    page.on('pageerror', error => errors.push(error.message));
    await page.route('https://api.github.com/**', route => route.fulfill({ status: 404, json: {} }));
    await page.addInitScript(() => localStorage.setItem('rpi-work-appearance-v1', JSON.stringify({ theme: 'codex', mode: 'light', density: 'standard', toolRendering: 'simple' })));
    await page.goto(UI);
    await page.getByRole('button', { name: '打开设置', exact: true }).click();
    await page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '工作协同', exact: true }).click();
    await page.getByRole('button', { name: '连接其他节点', exact: true }).click();
    await page.getByLabel('节点地址', { exact: true }).fill(fixture.url);
    await page.getByRole('button', { name: '连接节点', exact: true }).click();
    await page.getByRole('status').filter({ hasText: '在线' }).first().waitFor();
    await page.getByRole('button', { name: '返回会话', exact: true }).click();
    await page.locator('[data-project-id="rpi-work"] .project-toggle').click();
    await page.locator('[data-session-id="dark"] .session-item').click();
    const field = page.getByRole('textbox', { name: '发送给 agent 的消息', exact: true });
    await field.waitFor();
    await field.fill('滚动时必须保留的本地草稿');
    const card = page.locator('[data-tool-id="scroll-read"]');
    await card.waitFor();
    await card.locator('summary').first().click();
    const inner = card.locator('pre.call-output');
    const outer = page.locator('.transcript');
    assert.equal(await page.locator('.transcript .message-actions, .transcript .message-copy-markdown').count(), 0);
    await card.getByRole('button', { name: '复制工具输出', exact: true }).click();
    await card.getByRole('button', { name: '复制工具输出', exact: true }).locator('[data-icon="check"]').waitFor();
    assert.equal((await page.evaluate(() => navigator.clipboard.readText())).replace(/\r\n/g, '\n'), output);
    console.log('PASS message/tool boundary has no copy row; explicit tool output copy remains lossless');
    const metrics = () => page.evaluate(() => ({
      outer: document.querySelector('.transcript').scrollTop,
      inner: document.querySelector('[data-tool-id="scroll-read"] pre.call-output').scrollTop,
    }));
    const position = async () => {
      await outer.evaluate(el => { el.scrollTop = 0; el.dispatchEvent(new Event('scroll')); });
      await sleep(150);
      // Keep ample transcript room below the tool, regardless of responsive chrome.
      await outer.evaluate(el => {
        const output = el.querySelector('[data-tool-id="scroll-read"] pre.call-output');
        el.scrollTop += output.getBoundingClientRect().top - el.getBoundingClientRect().top - 50;
        el.dispatchEvent(new Event('scroll'));
      });
      await sleep(150);
      const box = await inner.boundingBox();
      const bounds = await outer.boundingBox();
      return { x: box.x + box.width / 2, y: Math.max(box.y + 20, bounds.y + 40) };
    };
    for (const rendering of ['simple', 'detailed']) {
      await page.evaluate(value => document.documentElement.dataset.toolRendering = value, rendering);
      await page.setViewportSize({ width: 1440, height: 1000 });
      const point = await position();
      const styles = await inner.evaluate(el => ({ x: getComputedStyle(el).overscrollBehaviorX, y: getComputedStyle(el).overscrollBehaviorY, touch: getComputedStyle(el).touchAction, range: el.scrollHeight - el.clientHeight }));
      assert.equal(styles.y, 'auto'); assert.equal(styles.x, 'contain'); assert.equal(styles.touch, 'auto'); assert(styles.range > 1000);
      await inner.evaluate(el => { el.scrollTop = 100; });
      await page.mouse.move(point.x, point.y);
      const before = await metrics();
      await page.mouse.wheel(0, 100); await sleep(250);
      const inside = await metrics();
      assert(inside.inner > before.inner + 20, `${rendering}: wheel scrolls tool while it has room`);
      assert(Math.abs(inside.outer - before.outer) < 3, `${rendering}: inner wheel must not move the outer conversation`);
      await inner.evaluate(el => { el.scrollTop = el.scrollHeight; });
      await sleep(500); // A fresh wheel gesture, after Chromium releases scroll latching.
      const edge = await metrics();
      // Chromium may consume the first gesture normalizing a fractional-pixel
      // boundary after programmatic positioning; continued gestures must chain.
      await page.mouse.wheel(0, 170); await sleep(300);
      await page.mouse.wheel(0, 170); await sleep(300);
      const chained = await metrics();
      assert(chained.outer > edge.outer + 40, `${rendering}: wheel continues to transcript at tool boundary: ${JSON.stringify({ edge, chained })}`);
      console.log(`PASS ${rendering}: wheel stays local while output has room and continues into conversation at boundary`);
    }
    const cdp = await context.newCDPSession(page);
    const swipe = async (x, y, dy) => {
      await cdp.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [{ x, y }] });
      for (let i = 1; i <= 8; i++) {
        await cdp.send('Input.dispatchTouchEvent', { type: 'touchMove', touchPoints: [{ x, y: y + dy * i / 8 }] });
        await sleep(25);
      }
      await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
      await sleep(350);
    };
    for (const width of [390, 320]) {
      await page.setViewportSize({ width, height: 844 });
      const point = await position();
      await inner.evaluate(el => { el.scrollTop = 100; });
      const start = await metrics();
      await swipe(point.x, point.y + 100, -80);
      const inside = await metrics();
      assert(inside.inner > start.inner + 20, `${width}: native touch pans tool output`);
      assert(Math.abs(inside.outer - start.outer) < 3, `${width}: local touch leaves transcript steady`);
      await inner.evaluate(el => { el.scrollTop = el.scrollHeight; });
      const edge = await metrics();
      await swipe(point.x, point.y + 100, -100);
      await swipe(point.x, point.y + 100, -100);
      assert((await metrics()).outer > edge.outer + 30, `${width}: boundary touch continues into transcript instead of trapping finger: ${JSON.stringify({ edge, after: await metrics() })}`);
      const top = (await metrics()).outer;
      fixture.emit({ type: 'message_update', message: { entryId: `scroll-new-${width}`, role: 'assistant' }, eventType: 'text_delta', delta: `滚动阅读时新到的消息 ${width}` });
      await page.getByRole('button', { name: '回到最新消息', exact: true }).waitFor();
      await sleep(200);
      assert(Math.abs((await metrics()).outer - top) < 4, 'streaming output must not steal older reading position');
      assert.equal(await field.inputValue(), '滚动时必须保留的本地草稿');
      assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1));
      await page.screenshot({ path: `artifacts/tool-scroll-touch-${width}.png`, animations: 'disabled' });
      console.log(`PASS ${width}: native touch works locally and at boundary; streaming preserves reading position and draft`);
    }
    await cdp.detach();
    assert.deepEqual(errors, []);
  } finally { await browser.close(); await fixture.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
