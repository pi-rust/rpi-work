// Actual formal WASM against an isolated fixture. No model calls or real node.
'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { createHash } = require('node:crypto');
const { chromium } = require('../support/browser.cjs');
const { startFixture, fixtureCommand } = require('../fixtures/ui-fixture.cjs');
const root = path.resolve(__dirname, '../..');
const reference = JSON.parse(fs.readFileSync(path.join(root, 'assets/icons/lucide-catalog.json')));
const cases = [], seen = new Set(), errors = [], screenshots = [];
const draft = '图标对齐验证：不发送消息，不改节点，只保留草稿。';
(async () => {
  const fixture = await startFixture(0);
  const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
  let fatal;
  try {
    const page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
    page.setDefaultTimeout(12000);
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
    await page.locator('.transcript').getByText('主题切换已经准备好，接下来运行组件测试。', { exact: true }).waitFor();
    await page.locator('.composer textarea').fill(draft);
    // Avoid sampling an in-flight press transform; preserve the real layout.
    await page.addStyleTag({ content: '*,*::before,*::after{animation-duration:0s!important;transition-duration:0s!important}' });
    async function inspect(label) {
      const icons = await page.locator('svg.icon[data-icon]').evaluateAll(elements => elements.map(svg => {
        const box = svg.getBoundingClientRect(), style = getComputedStyle(svg);
        const parent = svg.parentElement, parentBox = parent.getBoundingClientRect();
        return { semantic: svg.dataset.icon, lucide: svg.dataset.lucide, parentClass: parent.className,
          nodes: [...svg.children].map(node => [node.tagName, Object.fromEntries([...node.attributes].map(a => [a.name, a.value]))]),
          viewBox: svg.getAttribute('viewBox'), stroke: svg.getAttribute('stroke'), strokeWidth: svg.getAttribute('stroke-width'), hidden: svg.getAttribute('aria-hidden'), focusable: svg.getAttribute('focusable'),
          size: [box.width, box.height], centerDelta: Math.abs(box.y+box.height/2-parentBox.y-parentBox.height/2),
          centeredControl: parent.matches('button,summary,a') && getComputedStyle(parent).alignItems === 'center',
          visible: box.width > 0 && box.height > 0 && box.right > 0 && box.left < innerWidth && box.bottom > 0 && box.top < innerHeight,
          renderedSize: [parseFloat(style.width), parseFloat(style.height)],
        };
      }));
      assert(icons.length > 10, `${label}: actual rendered catalog`);
      let visible = 0;
      for (const icon of icons) {
        assert(Object.hasOwn(reference.map, icon.semantic), `${label}: unknown icon ${icon.semantic}`);
        const expected = reference.map[icon.semantic];
        assert.equal(icon.lucide, expected, `${icon.semantic}: no hand-drawn substitute`);
        assert.deepEqual(icon.nodes, reference.catalog[expected].nodes, `${icon.semantic}: exact pinned Web primitives`);
        assert.equal(icon.viewBox, '0 0 24 24'); assert.equal(icon.stroke, 'currentColor'); assert.equal(icon.strokeWidth, '2');
        assert.equal(icon.hidden, 'true'); assert.equal(icon.focusable, 'false');
        seen.add(icon.semantic);
        if (!icon.visible) continue;
        visible++;
        assert(Math.abs(icon.renderedSize[0]-icon.renderedSize[1]) < 0.1, `${label}: ${icon.semantic} (${icon.parentClass}) is stretched: ${icon.renderedSize}`);
        if (icon.centeredControl) assert(icon.centerDelta <= 1, `${label}: ${icon.semantic} control baseline delta ${icon.centerDelta}`);
      }
      assert(visible > 5, `${label}: visible icons measured`);
      assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
      assert.equal(await page.locator('.composer textarea').inputValue(), draft);
      cases.push({ label, rendered: icons.length, visible });
      console.log('PASS', label, ': exact Lucide nodes, square sizing, centered controls and retained draft');
    }
    for (const width of [1440, 390, 320]) {
      await page.setViewportSize({ width, height: 1000 });
      for (const mode of ['light', 'dark']) {
        await page.evaluate(mode => { document.documentElement.dataset.mode = mode; }, mode);
        await inspect(`workspace-${width}-${mode}`);
        if (width === 1440 || mode === 'light') {
          const screenshot = `artifacts/icon-alignment-${width}-${mode}.png`;
          await page.screenshot({ path: screenshot, animations: 'disabled' }); screenshots.push(screenshot);
        }
      }
      if (width < 500) {
        for (const selector of ['.mobile-menu', '.send-button', '.attachment-button-slot .extras-add']) {
          const control = page.locator(selector).first();
          if (await control.isVisible()) {
            const box = await control.boundingBox();
            assert(box.width >= 44 && box.height >= 44, `${selector}: mobile hit area must not shrink with its icon`);
          }
        }
        await page.getByRole('button', { name: '打开项目导航', exact: true }).click();
        await page.locator('.sidebar.open').waitFor();
        await inspect(`sidebar-${width}`);
        await page.getByRole('button', { name: '打开设置', exact: true }).click();
        await page.getByRole('button', { name: '返回会话', exact: true }).click();
      }
    }
    await page.setViewportSize({ width: 1440, height: 1000 });
    await page.getByRole('button', { name: '外观与主题', exact: true }).click();
    const dialog = page.getByRole('dialog', { name: '外观与主题', exact: true });
    await dialog.waitFor();
    await inspect('appearance-modal');
    const close = dialog.getByRole('button', { name: '关闭弹框', exact: true });
    assert(await close.isVisible(), 'icon-only close retains an accessible name');
    await page.keyboard.press('Escape'); await dialog.waitFor({ state: 'hidden' });
    assert.equal(await page.locator('.composer textarea').inputValue(), draft);
    assert.deepEqual(errors, []);
    assert.equal(fixture.requests.filter(request => ['prompt', 'queue_message', 'ui_dialog_response', 'set_model', 'set_thinking_level', 'abort'].includes(fixtureCommand(request))).length, 0);
  } catch (error) { fatal = error; }
  finally {
    await browser.close(); await fixture.close();
    fs.writeFileSync('artifacts/icon-alignment-browser-validation.json', JSON.stringify({
      passed: !fatal, scope: 'Formal WASM/isolated fixture; actual visible workspace/sidebar/modal icon primitives and role geometry, not whole-app pixel or native/Android acceptance',
      reference: { package: reference.package, version: reference.version, catalogSha256: createHash('sha256').update(fs.readFileSync(path.join(root, 'assets/icons/lucide-catalog.json'))).digest('hex') },
      cases, seen: [...seen].sort(), screenshots, errors, ...(fatal ? { failure: fatal.message } : {}),
    }, null, 2));
  }
  if (fatal) throw fatal;
})().catch(error => { console.error(error); process.exitCode = 1; });
