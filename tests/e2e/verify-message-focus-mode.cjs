// Formal WASM and an isolated node. No prompts or real project writes.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');

(async () => {
  assert(process.env.WORK_UI_URL, 'Run with tests/support/run-static-ui.cjs');
  fs.mkdirSync('artifacts/focus-mode', { recursive: true });
  const fixture = await startFixture();
  fixture.setMessages([
    { entryId: 'focus-user', role: 'user', content: '核对专注阅读的布局与退出。' },
    { entryId: 'focus-answer', role: 'assistant', content: Array.from({ length: 45 }, (_, index) => `第 ${index + 1} 段。保留完整的消息和滚动区域，进入专注阅读后可继续阅读。`).join('\n\n') },
  ]);
  const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
  const page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
  page.setDefaultTimeout(12000);
  const errors = [], results = [];
  const button = name => page.getByRole('button', { name, exact: true });
  const shell = page.locator('.app-shell');
  const draft = page.locator('.composer textarea');
  const text = '专注阅读退出后保留这份草稿。';
  page.on('pageerror', error => errors.push(error.message));
  try {
    await page.addInitScript(address => {
      localStorage.setItem('rpi-work-nodes-v1', JSON.stringify({ version: 1, recent: [address], selected: address }));
    }, fixture.url);
    await page.route('https://api.github.com/**', route => route.fulfill({ status: 404, body: '{}' }));
    await page.goto(process.env.WORK_UI_URL);
    await page.locator('[data-session-id=dark] .session-item').first().click();
    await page.locator('[data-history-id=focus-answer]').waitFor();
    await draft.fill(text);
    for (const [width, height, collapsed, inspector] of [
      [1440, 1000, false, false],
      [1440, 1000, true, false],
      [1440, 1000, false, true],
      [1024, 768, true, true],
      [390, 844, false, false],
      [320, 420, false, false],
    ]) {
      await page.setViewportSize({ width, height });
      await page.waitForFunction(w => document.querySelector('.app-shell').dataset.deviceMode === (w < 768 ? 'mobile' : 'desktop'), width);
      if (width >= 768) {
        const isCollapsed = await shell.evaluate(element => element.classList.contains('sidebar-collapsed'));
        if (isCollapsed !== collapsed) await page.locator('.app-header > .mobile-menu').click();
      }
      const context = page.locator('.app-shell > .inspector');
      if ((await context.isVisible()) !== inspector) {
        if (inspector) await button('查看任务上下文').click();
        else await context.getByRole('button', { name: '关闭任务上下文', exact: true }).click();
      }
      if (inspector) await context.getByRole('button', { name: '专注阅读', exact: true }).click();
      else {
        await page.locator('.workspace-view-menu > summary').click();
        await button('专注阅读').click();
      }
      const exit = button('退出专注阅读');
      await exit.waitFor();
      await page.waitForFunction(() => document.activeElement?.id === 'conversation-focus-exit');
      await page.screenshot({ path: `artifacts/focus-mode/${width}-${collapsed}-${inspector}.png`, animations: 'disabled' });
      const geometry = await page.evaluate(() => {
        const box = selector => document.querySelector(selector).getBoundingClientRect().toJSON();
        return { main: box('.main-column'), transcript: box('.transcript'), width: innerWidth, height: innerHeight, scroll: document.documentElement.scrollWidth };
      });
      assert(Math.abs(geometry.main.x) <= 1 && Math.abs(geometry.main.width - width) <= 1, JSON.stringify(geometry));
      assert(geometry.transcript.height >= height - 1, 'reading occupies the available height');
      assert(geometry.scroll <= width + 1);
      assert.equal(await page.locator('.app-header').isVisible(), false);
      assert.equal(await page.locator('.app-shell > .sidebar').isVisible(), false);
      assert.equal(await page.locator('.sidebar-resize-handle').isVisible(), false);
      assert.equal(await context.isVisible(), false);
      assert.equal(await page.locator('.composer-main').isVisible(), false);
      assert.equal(await exit.evaluate(element => document.activeElement === element), true, 'focus moves to the visible exit control');
      assert(await exit.evaluate(element => {
        const rect = element.getBoundingClientRect();
        return rect.width >= 44 && rect.height >= 44 && rect.top >= 0 && rect.right <= innerWidth
          && element.contains(document.elementFromPoint(rect.x + rect.width / 2, rect.y + rect.height / 2));
      }), 'exit has a visible, unobstructed touch target');
      await page.locator('.transcript').evaluate(element => { element.scrollTop = 250; });
      assert(await page.locator('.transcript').evaluate(element => element.scrollTop > 0));
      if (collapsed) await exit.click();
      else await page.keyboard.press('Escape');
      await exit.waitFor({ state: 'hidden' });
      const returnSelector = inspector ? '.inspector-focus-toggle' : '.workspace-view-menu > summary';
      await page.waitForFunction(selector => document.activeElement === document.querySelector(selector), returnSelector);
      assert.equal(await draft.inputValue(), text);
      assert.equal(await page.locator(returnSelector).evaluate(element => document.activeElement === element), true);
      assert.equal(await context.isVisible(), inspector);
      if (width >= 768) assert.equal(await shell.evaluate(element => element.classList.contains('sidebar-collapsed')), collapsed);
      results.push(`${width}×${height}, collapsed=${collapsed}, inspector=${inspector}`);
      console.log('PASS full-width focus, exit, restored workspace and draft', results.at(-1));
    }
    assert.deepEqual(errors, []);
    fs.writeFileSync('artifacts/focus-mode/validation.json', JSON.stringify({ results, errors }, null, 2));
  } finally {
    await browser.close();
    await fixture.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
