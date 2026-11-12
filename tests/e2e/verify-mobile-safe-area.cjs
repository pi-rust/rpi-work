// Formal WASM. Safe-area values are simulated; no physical display-cutout claim.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');
(async () => {
  fs.mkdirSync('artifacts', { recursive: true });
  const fixture = await startFixture();
  const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
  const results = [], geometry = [], errors = [];
  const pass = text => { results.push(text); console.log('PASS', text); };
  try {
    const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
    page.setDefaultTimeout(10000);
    page.on('pageerror', error => errors.push(error.message));
    await page.route('https://api.github.com/**', route => route.fulfill({ status: 404, body: '{}' }));
    await page.goto(process.env.WORK_UI_URL || 'http://127.0.0.1:14180/');
    await page.getByRole('button', { name: '打开设置', exact: true }).click();
    await page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '工作协同', exact: true }).click();
    await page.getByRole('button', { name: '连接其他节点', exact: true }).click();
    await page.getByLabel('节点地址', { exact: true }).fill(fixture.url);
    await page.getByRole('button', { name: '连接节点', exact: true }).click();
    await page.locator('.server-details [role=status]').filter({ hasText: '在线' }).waitFor();
    await page.getByRole('button', { name: '返回会话', exact: true }).click();
    await page.locator('.project-row').filter({ hasText: 'rpi-work' }).click();
    await page.locator('.sidebar-sessions').getByRole('button', { name: '为工作台添加深色模式', exact: true }).click();
    const draft = page.getByRole('textbox', { name: '发送给 agent 的消息', exact: true });
    await draft.fill('全面屏布局检查，保留草稿');
    const scenarios = [
      { name: 'phone-fullscreen', width: 390, height: 844, top: 0, bottom: 0, left: 0, right: 0 },
      { name: 'phone-cutout', width: 390, height: 844, top: 47, bottom: 34, left: 0, right: 0 },
      { name: 'small-phone-cutout', width: 320, height: 740, top: 24, bottom: 24, left: 0, right: 0 },
      { name: 'keyboard-resized', width: 390, height: 430, top: 24, bottom: 0, left: 0, right: 0 },
      { name: 'landscape-cutout', width: 844, height: 390, top: 0, bottom: 21, left: 44, right: 44 },
      { name: 'desktop', width: 1280, height: 900, top: 0, bottom: 0, left: 0, right: 0 },
    ];
    for (const scenario of scenarios) {
      const { name, width, height, top, bottom, left, right } = scenario;
      await page.setViewportSize({ width, height });
      await page.evaluate(({ top, bottom, left, right }) => {
        for (const [side, value] of Object.entries({ top, bottom, left, right })) document.documentElement.style.setProperty('--app-safe-' + side, value + 'px');
      }, scenario);
      const shell = await page.locator('.app-shell').boundingBox();
      assert(Math.abs(shell.height - height) < 1, `${name}: shell must fill the viewport without a legacy titlebar deduction`);
      const check = async selector => {
        const box = await page.locator(selector).first().boundingBox();
        assert(box && box.y >= top - 1 && box.y + box.height <= height - bottom + 1, `${name}/${selector}: outside safe vertical region ${JSON.stringify(box)}`);
        assert(box.x >= left - 1 && box.x + box.width <= width - right + 1, `${name}/${selector}: outside safe horizontal region`);
        return box;
      };
      await check('.app-header');
      const composer = await check('.composer-main');
      await check('.composer-actions .send-button');
      assert(Math.abs(composer.y + composer.height - (height - bottom)) < 1, `${name}: composer must reach safe-area bottom, not leave a fixed blank strip`);
      assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
      if (width <= 700) {
        await page.getByRole('button', { name: '打开项目导航', exact: true }).click();
        await page.waitForFunction(() => document.querySelector('.sidebar').getBoundingClientRect().left >= -0.5);
        await check('.sidebar');
        await page.keyboard.press('Escape');
        await page.getByRole('button', { name: '关闭项目导航', exact: true }).waitFor({ state: 'hidden' });
      }
      if (width <= 1100) {
        await page.getByRole('button', { name: '查看任务上下文', exact: true }).click();
        await page.locator('.inspector').waitFor({ state: 'visible' });
        await check('.inspector');
        await page.locator('.inspector-close').click();
      }
      await page.getByRole('button', { name: '选择供应商与模型', exact: true }).click();
      const dialog = page.getByRole('dialog', { name: '选择供应商与模型', exact: true });
      await dialog.waitFor();
      // Composer pickers are native popovers with dialog semantics, not <dialog>.
      await check('[role="dialog"].composer-model-dialog:popover-open');
      await page.keyboard.press('Escape'); await dialog.waitFor({ state: 'hidden' });
      assert.equal(await draft.inputValue(), '全面屏布局检查，保留草稿');
      geometry.push({ scenario, shell, composer });
      await page.screenshot({ path: `artifacts/mobile-safe-area-${name}.png`, animations: 'disabled' });
      pass(`${name}: header, composer, overlays and dialogs respect available height and insets`);
    }
    assert.match(await page.locator('meta[name=viewport]').getAttribute('content'), /viewport-fit=cover/);
    assert.deepEqual(errors, []);
    pass('viewport enables cutout reporting; no application errors');
  } finally {
    fs.writeFileSync('artifacts/mobile-safe-area-validation.json', JSON.stringify({ scope: 'Formal WASM, simulated safe-area values and resized viewports; not an Android native inset test', results, geometry, errors }, null, 2));
    await browser.close(); await fixture.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
