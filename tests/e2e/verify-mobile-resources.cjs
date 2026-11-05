// Actual WASM + local fixture only. Details and design references share one settings dialog.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');
(async () => {
  fs.mkdirSync('artifacts', { recursive: true });
  const fixture = await startFixture(0);
  const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
  const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
  const results = [], errors = [], geometry = [];
  page.setDefaultTimeout(10000);
  page.on('pageerror', error => errors.push(error.message));
  const pass = text => { results.push(text); console.log('PASS', text); };
  const entry = page.locator('.section-tabs').getByRole('button', { name: '打开会话资源配置', exact: true });
  const config = entry;
  const modal = page.getByRole('dialog', { name: '会话设置', exact: true });
  const draftText = '收起会话能力入口，不丢失草稿';
  const measure = () => page.locator('.section-tabs').evaluate(root => {
    const box = e => { const r = e.getBoundingClientRect(); return { x: r.x, y: r.y, width: r.width, height: r.height }; };
    return { row: box(root), scroll: root.scrollWidth, client: root.clientWidth, entry: box(root.querySelector('[aria-label="打开会话资源配置"]')), visible: [...root.querySelectorAll('[aria-label="打开会话资源配置"]')].filter(e => e.getBoundingClientRect().width > 0).map(e => ({ name: e.getAttribute('aria-label'), ...box(e) })) };
  });
  try {
    await page.route('https://api.github.com/**', route => route.fulfill({ status: 404, body: '{}' }));
    await page.goto(process.env.WORK_UI_URL || 'http://127.0.0.1:14180/');
    await page.getByRole('button', { name: '打开设置', exact: true }).click();
    await page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '执行节点', exact: true }).click();
    await page.getByRole('button', { name: '连接其他节点', exact: true }).click();
    await page.getByLabel('节点地址', { exact: true }).fill(fixture.url);
    await page.getByRole('button', { name: '连接节点', exact: true }).click();
    await page.locator('.server-details [role=status]').filter({ hasText: '在线' }).waitFor();
    await page.getByRole('button', { name: '返回会话', exact: true }).click();
    await page.locator('.project-row').filter({ hasText: 'rpi-work' }).click();
    await page.locator('.sidebar-sessions').getByRole('button', { name: '为工作台添加深色模式', exact: true }).click();
    const draft = page.locator('.composer textarea');
    await draft.fill(draftText);
    const desktop = await measure();
    assert.equal(desktop.visible.length, 1, 'desktop and mobile share a single design settings entry');
    for (const mode of ['light', 'dark']) {
      await page.locator('.app-header .header-more').click();
      await page.getByRole('menuitem', { name: '外观与主题', exact: true }).click();
      const appearance = page.getByRole('dialog', { name: '外观与主题', exact: true });
      await appearance.getByLabel('明暗模式', { exact: true }).selectOption(mode);
      await appearance.getByRole('button', { name: '保存', exact: true }).click();
      await appearance.waitFor({ state: 'hidden' });
      for (const width of [390, 320]) {
        await page.setViewportSize({ width, height: 844 });
        const sizes = await measure(); geometry.push({ mode, width, sizes });
        assert.equal(sizes.visible.length, 1, 'mobile shows only one resource entry');
        assert(sizes.scroll <= sizes.client + 1, `no hidden horizontal resource strip: ${JSON.stringify(sizes)}`);
        assert(sizes.visible[0].width >= 44 && sizes.visible[0].height >= 44);
        assert.equal(await config.getAttribute('aria-label'), '打开会话资源配置');
        assert.equal(await config.getAttribute('aria-haspopup'), 'dialog');
        assert(await page.locator('.section-tabs').getByRole('button', { name: '打开项目文件', exact: true }).isVisible());
        await page.locator('.section-tabs').screenshot({ path: `artifacts/mobile-resources-${mode}-${width}-collapsed.png`, animations: 'disabled' });
        await config.click(); await modal.waitFor();
        assert.equal(await page.locator('dialog:modal').count(), 1, 'no separate resource modal');
        assert.equal(await modal.getByRole('tab', { name: '设计引用', exact: true }).getAttribute('aria-selected'), 'true');
        await modal.evaluate(el => { window.__settingsDialog = el; });
        await modal.getByRole('tab', { name: '会话详情', exact: true }).click();
        assert.equal(await modal.locator('.execution-backend-menu').count(), 0, 'no duplicate Agent in details');
        await modal.getByRole('tab', { name: '设计引用', exact: true }).click();
        assert(await modal.evaluate(el => el === window.__settingsDialog));
        const segments = modal.locator('#session-settings-design-panel > .resource-segments');
        for (const name of ['Packages', '扩展', 'Skills', 'Tools', '提示词', '执行后端']) {
          const button = segments.getByRole('button', { name, exact: true });
          await button.click();
          assert.match(await button.getAttribute('class'), /active/);
          const b = await button.boundingBox(), r = await segments.boundingBox();
          assert(b.height >= 44 && b.x >= r.x - 1 && b.x + b.width <= r.x + r.width + 1, `${name} stays inside expanded mobile tabs`);
        }
        await segments.getByRole('button', { name: 'Packages', exact: true }).click();
        await modal.screenshot({ path: `artifacts/mobile-resources-${mode}-${width}-expanded.png`, animations: 'disabled' });
        await page.keyboard.press('Escape'); await modal.waitFor({ state: 'hidden' });
        assert.equal(await draft.inputValue(), draftText);
        await config.focus(); await page.keyboard.press('Enter'); await modal.waitFor();
        await page.keyboard.press('Escape'); await modal.waitFor({ state: 'hidden' });
        assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
        pass(`${mode}/${width}: single compact entry; all six resource sections open; keyboard and draft intact`);
      }
      await page.setViewportSize({ width: 1280, height: 900 });
    }
    assert.deepEqual(await measure(), desktop, 'desktop resource geometry remains unchanged after resize');
    await page.locator('.app-header .header-more').click();
    await page.getByRole('menuitem', { name: '会话设置', exact: true }).click();
    await modal.waitFor(); assert.equal(await modal.getByRole('tab', { name: '会话详情', exact: true }).getAttribute('aria-selected'), 'true');
    await modal.getByRole('tab', { name: '设计引用', exact: true }).click();
    await modal.locator('#session-settings-design-panel > .resource-segments').getByRole('button', { name: 'Tools', exact: true }).click();
    await page.keyboard.press('Escape');
    assert.equal(fixture.requests.filter(r => ['prompt', 'queue_message', 'set_active_tools'].includes(r.body.type)).length, 0);
    assert.deepEqual(errors, []);
    pass('unified desktop settings retain details/design navigation; no prompts, configuration mutations or application errors');
  } catch (error) {
    await page.screenshot({ path: 'artifacts/mobile-resources-failure.png', animations: 'disabled' }); throw error;
  } finally {
    fs.writeFileSync('artifacts/mobile-resources-validation.json', JSON.stringify({ results, geometry, errors }, null, 2));
    await browser.close(); await fixture.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
