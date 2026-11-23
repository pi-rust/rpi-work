const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { createRequire } = require('node:module');
const driverRequire = createRequire(require.resolve(process.env.RPI_LAYOUT_PLAYWRIGHT ?? '../../tools/node_modules/playwright'));
const { PNG } = driverRequire('playwright-core/lib/utilsBundle');

function edgeInk(image, color) {
  const counts = { top: 0, bottom: 0, left: 0, right: 0 };
  for (let y = 0; y < image.height; y++) for (let x = 0; x < image.width; x++) {
    const offset = (y * image.width + x) * 4;
    if (!color.every((value, index) => Math.abs(value - image.data[offset + index]) <= 2)) continue;
    if (x >= 8 && x < image.width - 8) {
      if (y < 5) counts.top++;
      if (y >= image.height - 5) counts.bottom++;
    }
    if (y >= 8 && y < image.height - 8) {
      if (x < 5) counts.left++;
      if (x >= image.width - 5) counts.right++;
    }
  }
  return counts;
}

async function checkCodeFocus(browser, url) {
  const page = await browser.newPage({ viewport: { width: 1440, height: 940 }, deviceScaleFactor: 1, reducedMotion: 'reduce' });
  const errors = [], results = [], violations = [];
  page.on('pageerror', error => errors.push(error.message));
  try {
    await page.goto(url);
    const pre = page.locator('.message-code pre');
    const firstLine = page.locator('.message-code .code-line').first();
    const originalLine = await firstLine.textContent();
    // Probe native scrolling once in a fresh widget, independently of paint fixtures.
    await firstLine.evaluate((node, text) => { node.textContent = text + ' // ' + 'long_identifier '.repeat(50); }, originalLine);
    await page.locator('[data-action="copy-code"]').scrollIntoViewIfNeeded();
    await page.locator('[data-action="copy-code"]').focus();
    await page.keyboard.press('Tab');
    const top = await page.locator('.conversation').evaluate(node => node.scrollTop);
    await page.keyboard.press('ArrowRight');
    await page.waitForFunction(() => document.querySelector('.message-code pre').scrollLeft > 0);
    const nativeScroll = await pre.evaluate(node => ({ scrollLeft: node.scrollLeft, width: node.clientWidth, scrollWidth: node.scrollWidth }));
    assert.equal(await page.locator('.conversation').evaluate(node => node.scrollTop), top);
    assert.equal(await page.locator('.conversation').evaluate(node => node.scrollLeft), 0);
    const draft = page.locator('#draft');
    for (const width of [1440, 1024]) {
      await page.setViewportSize({ width, height: 940 });
      for (const theme of ['light', 'dark']) for (const layout of ['reading', 'classic']) {
        for (const fixture of ['original', 'long']) {
          // Start each fixture before the code region's first keyboard focus.
          await page.goto(url);
          await draft.fill('代码阅读期间的草稿');
          await draft.evaluate(node => node.setSelectionRange(3, 6));
          await page.getByRole('button', { name: '设置', exact: true }).click();
          await page.getByRole('button', { name: '外观', exact: true }).click();
          await page.locator('#theme').selectOption(theme);
          await page.getByLabel('消息布局', { exact: true }).selectOption(layout);
          await page.getByRole('button', { name: '关闭设置', exact: true }).click();
          await firstLine.evaluate((node, text) => { node.textContent = text; }, originalLine + (fixture === 'long' ? ' // ' + 'long_identifier '.repeat(50) : ''));
          await page.locator('[data-action="copy-code"]').focus();
          await page.keyboard.press('Tab');
          assert.equal(await pre.evaluate(node => document.activeElement === node && node.matches(':focus-visible')), true);
          const buffer = await pre.screenshot();
          assert.equal(await page.locator('.conversation').evaluate(node => node.scrollLeft), 0);
          assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
          const style = await pre.evaluate(node => {
            const style = getComputedStyle(node);
            return { color: style.outlineColor, offset: style.outlineOffset, style: style.outlineStyle, width: style.outlineWidth, scrollLeft: node.scrollLeft };
          });
          assert.equal(style.style, 'solid');
          assert.ok(parseFloat(style.width) >= 2);
          const color = style.color.match(/[\d.]+/g).slice(0, 3).map(Number);
          const image = PNG.sync.read(buffer);
          const edges = edgeInk(image, color);
          const result = { width, theme, layout, fixture, style, dimensions: [image.width, image.height], edges };
          results.push(result);
          for (const edge of ['top', 'bottom', 'left', 'right']) {
            const length = (edge === 'top' || edge === 'bottom' ? image.width : image.height) - 16;
            if (edges[edge] < length * .75) violations.push({ width, theme, layout, fixture, edge, pixels: edges[edge], minimum: length * .75 });
          }
          if (process.env.RPI_WORKBENCH_SHOTS) {
            fs.writeFileSync(path.join(process.env.RPI_WORKBENCH_SHOTS, `code-focus-${theme}-${layout}-${width}-${fixture}.png`), buffer);
            if (width === 1440 && theme === 'light' && layout === 'reading' && fixture === 'long') await page.screenshot({ path: path.join(process.env.RPI_WORKBENCH_SHOTS, 'code-focus-window.png') });
          }
          await page.keyboard.press('Tab');
          assert.equal(await pre.evaluate(node => document.activeElement === node), false);
          await page.keyboard.press('Shift+Tab');
          assert.equal(await pre.evaluate(node => document.activeElement === node), true);
          assert.equal(await draft.inputValue(), '代码阅读期间的草稿');
          assert.deepEqual(await draft.evaluate(node => [node.selectionStart, node.selectionEnd]), [3, 6]);
        }
      }
    }
    assert.deepEqual(errors, []);
    if (process.env.RPI_WORKBENCH_SHOTS) fs.writeFileSync(path.join(process.env.RPI_WORKBENCH_SHOTS, 'code-focus-metrics.json'), JSON.stringify({ nativeScroll, results, violations }, null, 2) + '\n');
    assert.deepEqual(violations, [], 'Code focus ring must paint every edge inside its clipping container');
    console.log('Code focus painted on four edges, Tab/Shift+Tab and draft/caret retention passed across 16 desktop cases; independent fresh-widget native ArrowRight scrolling passed.');
  } finally {
    await page.close();
  }
}

module.exports = { checkCodeFocus };
