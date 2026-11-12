// Formal Web UI + isolated HTTP/SSE fixture; no model calls or prompt commands.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');
const KEY = 'rpi-work-sidebar-width-v1';
const URL = process.env.WORK_UI_URL || 'http://127.0.0.1:14180/';

(async () => {
  fs.mkdirSync('artifacts', { recursive: true });
  const fixture = await startFixture();
  const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
  const context = await browser.newContext({ viewport: { width: 1440, height: 1000 } });
  await context.route('https://api.github.com/**', route => route.fulfill({ status: 404, body: '{}' }));
  const page = await context.newPage();
  page.setDefaultTimeout(10000);
  const results = [], errors = [];
  const pass = text => { results.push(text); console.log('PASS', text); };
  page.on('pageerror', error => errors.push(error.message));
  const handle = page.getByRole('separator', { name: '调整侧栏宽度', exact: true });
  const width = () => page.locator('.sidebar').evaluate(el => el.getBoundingClientRect().width);
  const stored = () => page.evaluate(key => localStorage.getItem(key), KEY);
  const expectWidth = value => page.waitForFunction(expected => Math.abs(document.querySelector('.sidebar').getBoundingClientRect().width - expected) < 1, value);
  const idle = () => page.waitForFunction(() => !document.documentElement.classList.contains('sidebar-resizing'));
  const begin = async () => {
    const box = await handle.boundingBox();
    const before = await width(), x = box.x + box.width / 2, y = box.y + box.height / 2;
    await handle.evaluate(el => el.addEventListener('pointerdown', e => { el.dataset.testPointer = e.pointerId; }, { once: true }));
    await page.mouse.move(x, y);
    await page.mouse.down();
    assert(await handle.evaluate(el => el.hasPointerCapture(Number(el.dataset.testPointer))), 'drag must capture the pointer');
    return { before, x, y };
  };
  const move = async (start, target) => page.mouse.move(start.x + target - start.before, start.y, { steps: 8 });
  const drag = async target => {
    const start = await begin();
    await move(start, target);
    await page.mouse.up();
    await idle();
  };
  try {
    await page.goto(URL);
    await handle.waitFor();
    await expectWidth(272);
    assert.equal(await handle.evaluate(el => getComputedStyle(el).cursor), 'col-resize');
    assert(await handle.evaluate(el => {
      const sidebar = el.closest('.app-shell').querySelector('.sidebar').getBoundingClientRect();
      const y = sidebar.y + sidebar.height / 2;
      return [-2, 0, 2].every(offset => document.elementFromPoint(sidebar.right + offset, y) === el);
    }), 'the visible border and both sides of it must hit the drag handle');
    await page.getByRole('button', { name: '打开设置', exact: true }).click();
    await page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '工作协同', exact: true }).click();
    await page.getByRole('button', { name: '连接其他节点', exact: true }).click();
    await page.getByLabel('节点地址', { exact: true }).fill(fixture.url);
    await page.getByRole('button', { name: '连接节点', exact: true }).click();
    await page.getByRole('status').filter({ hasText: '在线' }).first().waitFor();
    await page.getByRole('button', { name: '返回会话', exact: true }).click();
    await page.locator('.project-row').filter({ hasText: 'rpi-work' }).click();
    await page.locator('[data-session-id="dark"] .session-item').click();
    const composer = page.getByRole('textbox', { name: '发送给 agent 的消息', exact: true });
    await composer.fill('拖动侧栏时保留这份草稿');
    const input = await composer.evaluateHandle(el => el);
    await drag(360);
    await expectWidth(360);
    assert.equal(await stored(), '360');
    assert.equal(await composer.inputValue(), '拖动侧栏时保留这份草稿');
    assert(await composer.evaluate((el, original) => el === original, input), 'drag must not remount the input');
    await input.dispose();
    await page.mouse.move(900, 300);
    await expectWidth(360);
    assert.equal(await handle.getAttribute('aria-valuenow'), '360');
    assert.equal(await handle.getAttribute('aria-orientation'), 'vertical');
    pass('real border drag captures the pointer, resizes the grid, saves width and preserves the same input/draft');

    await drag(650);
    await expectWidth(480);
    await drag(60);
    await expectWidth(200);
    await handle.focus();
    await page.keyboard.press('ArrowRight');
    await expectWidth(210);
    await page.keyboard.press('Shift+ArrowRight');
    await expectWidth(250);
    await page.keyboard.press('End');
    await expectWidth(480);
    await page.keyboard.press('Home');
    await expectWidth(200);
    await drag(360);
    pass('minimum/maximum widths and separator keyboard controls work');

    for (const cancel of ['Escape', 'pointercancel', 'lostpointercapture', 'blur']) {
      const start = await begin();
      await move(start, 420);
      await expectWidth(420);
      if (cancel === 'Escape') await page.keyboard.press('Escape');
      else if (cancel === 'blur') await page.evaluate(() => window.dispatchEvent(new Event('blur')));
      else if (cancel === 'lostpointercapture') {
        await handle.evaluate(el => el.releasePointerCapture(Number(el.dataset.testPointer)));
        await page.mouse.move(start.x + 61, start.y);
      } else await handle.dispatchEvent('pointercancel', { pointerId: Number(await handle.getAttribute('data-test-pointer')) });
      await idle();
      await page.mouse.up();
      await expectWidth(360);
      assert.equal(await stored(), '360', `${cancel}: cancellation must not overwrite the committed width`);
    }
    pass('Escape, pointer cancellation, lost capture and focus loss clear drag state and roll back uncommitted width');

    await page.getByRole('button', { name: '打开设置', exact: true }).click();
    await expectWidth(360);
    await page.getByRole('button', { name: '返回会话', exact: true }).click();
    await expectWidth(360);
    assert.equal(await composer.inputValue(), '拖动侧栏时保留这份草稿');
    assert.equal(await page.evaluate(() => window.__rpiSidebarResize?.size), 1, 'rerenders must not duplicate controllers');
    await page.reload();
    await handle.waitFor();
    await expectWidth(360);
    pass('page changes preserve width and draft; reload restores the saved preference with one controller');

    await page.setViewportSize({ width: 800, height: 950 });
    await expectWidth(320);
    assert.equal(await stored(), '360', 'viewport clamping must not overwrite the desktop preference');
    await page.setViewportSize({ width: 390, height: 844 });
    assert.equal(await handle.isVisible(), false);
    await page.getByRole('button', { name: '打开项目导航', exact: true }).click();
    await page.waitForFunction(() => document.querySelector('.sidebar').getBoundingClientRect().left >= -0.5);
    await expectWidth(280);
    assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
    await page.keyboard.press('Escape');
    await page.setViewportSize({ width: 1440, height: 1000 });
    await expectWidth(360);
    await handle.dblclick();
    await expectWidth(272);
    assert.equal(await stored(), null);
    await page.setViewportSize({ width: 800, height: 950 });
    await expectWidth(210);
    await page.setViewportSize({ width: 1440, height: 1000 });
    pass('responsive limits preserve desktop preference; phone drawer is unchanged; double-click restores desktop/tablet defaults');

    const start = await begin();
    await move(start, 400);
    await page.setViewportSize({ width: 390, height: 844 });
    await idle();
    await page.mouse.up();
    await page.setViewportSize({ width: 1440, height: 1000 });
    await expectWidth(272);
    assert.equal(await stored(), null);
    pass('changing viewport during a drag safely cancels without saving a phone width');
    await page.screenshot({ path: 'artifacts/sidebar-resize.png', animations: 'disabled' });

    const isolated = await browser.newContext({ viewport: { width: 1440, height: 1000 } });
    try {
      await isolated.route('https://api.github.com/**', route => route.fulfill({ status: 404, body: '{}' }));
      await isolated.addInitScript(key => {
        localStorage.setItem(key, 'invalid width');
        const original = Storage.prototype.setItem;
        Storage.prototype.setItem = function (name, value) {
          if (name === key) throw new DOMException('test-only storage restriction', 'QuotaExceededError');
          return original.call(this, name, value);
        };
      }, KEY);
      const probe = await isolated.newPage();
      probe.on('pageerror', error => errors.push(error.message));
      await probe.goto(URL);
      const divider = probe.getByRole('separator', { name: '调整侧栏宽度', exact: true });
      await divider.waitFor();
      await probe.waitForFunction(() => window.__rpiSidebarResize?.size === 1);
      await divider.focus();
      await probe.keyboard.press('ArrowRight');
      await probe.waitForFunction(() => Math.abs(document.querySelector('.sidebar').getBoundingClientRect().width - 282) < 1);
      await divider.dblclick();
      await probe.waitForFunction(() => Math.abs(document.querySelector('.sidebar').getBoundingClientRect().width - 272) < 1);
    } finally { await isolated.close(); }
    pass('invalid saved values and blocked storage cannot break resizing');
    assert.deepEqual(errors, []);
    assert(!fixture.requests.some(request => ['prompt', 'queue_message', 'abort'].includes(request.body.type)));
    fs.writeFileSync('artifacts/sidebar-resize-validation.json', JSON.stringify({ passed: true, results, errors }, null, 2));
  } catch (error) {
    await page.screenshot({ path: 'artifacts/sidebar-resize-failure.png', animations: 'disabled' }).catch(() => {});
    throw error;
  } finally {
    await browser.close();
    await fixture.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
