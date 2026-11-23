const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

function rgba(value) {
  assert.match(value, /^rgba?\([\d.,\s]+\)$/, 'Only serialized sRGB colors are supported by this audit');
  const channels = value.match(/[\d.]+/g).map(Number);
  return channels.length === 3 ? [...channels, 1] : channels;
}
function composite(foreground, background) {
  return foreground.slice(0, 3).map((channel, index) => channel * foreground[3] + background[index] * (1 - foreground[3]));
}
function luminance(color) {
  const linear = color.map(value => value / 255).map(value => value <= .04045 ? value / 12.92 : ((value + .055) / 1.055) ** 2.4);
  return linear[0] * .2126 + linear[1] * .7152 + linear[2] * .0722;
}
function contrast(foreground, background) {
  const a = luminance(foreground), b = luminance(background);
  return (Math.max(a, b) + .05) / (Math.min(a, b) + .05);
}

async function checkReadingContrast(browser, url) {
  assert.equal(contrast([0, 0, 0], [255, 255, 255]), 21);
  assert.equal(contrast([115, 115, 115], [115, 115, 115]), 1);
  assert.ok(contrast([115, 115, 115], [242, 242, 242]) < 4.5);
  assert.deepEqual(composite([0, 0, 0, .5], [255, 255, 255]), [127.5, 127.5, 127.5]);
  const page = await browser.newPage({ viewport: { width: 1440, height: 940 }, deviceScaleFactor: 1, colorScheme: 'light', reducedMotion: 'reduce' });
  const results = [], violations = [], errors = [];
  page.on('pageerror', error => errors.push(error.message));
  const sites = [
    ['.assistant > p'], ['.reply > p'], ['.reply > ul'], ['.activity summary'], ['.activity pre'],
    ['.message-code-header'], ['.message-code-header button'], ['.message-code pre'], ['.code-line', '::before'],
    ['.diff-caption > button'], ['.diff-range'], ['.diff-line > span'], ['.diff-line code'],
  ];
  try {
    await page.goto(url);
    await page.getByRole('tab', { name: '变更', exact: true }).click();
    await page.locator('details[data-activity="initial"] > summary').click();
    assert.equal(await page.locator('html').getAttribute('data-code-numbers'), 'true');
    for (const theme of ['light', 'dark']) {
      await page.getByRole('button', { name: '设置', exact: true }).click();
      await page.getByRole('button', { name: '外观', exact: true }).click();
      await page.locator('#theme').selectOption(theme);
      await page.getByRole('button', { name: '关闭设置', exact: true }).click();
      await page.mouse.move(0, 0);
      for (const [selector, pseudo] of sites) {
        const samples = await page.locator(selector).evaluateAll((nodes, pseudo) => nodes.map(node => {
          const style = getComputedStyle(node, pseudo);
          const ancestors = [];
          for (let current = node; current; current = current.parentElement) {
            const parentStyle = getComputedStyle(current);
            ancestors.unshift({ background: parentStyle.backgroundColor, image: parentStyle.backgroundImage, opacity: parentStyle.opacity, blend: parentStyle.mixBlendMode });
          }
          if (pseudo) ancestors.push({ background: style.backgroundColor, image: style.backgroundImage, opacity: style.opacity, blend: style.mixBlendMode });
          return { color: style.color, size: style.fontSize, ancestors, row: node.closest('.diff-line')?.className };
        }), pseudo ?? null);
        assert.ok(samples.length > 0, 'Missing audit site ' + selector);
        for (const sample of samples) {
          let background = [255, 255, 255], opaque = false;
          for (const layer of sample.ancestors) {
            assert.equal(layer.image, 'none', 'Gradients/images require a separate pixel audit');
            assert.equal(layer.opacity, '1', 'Group opacity requires a separate pixel audit');
            assert.equal(layer.blend, 'normal', 'Blend modes require a separate pixel audit');
            const color = rgba(layer.background);
            opaque ||= color[3] === 1;
            background = composite(color, background);
          }
          assert.equal(opaque, true, 'An explicit opaque canvas must establish the background');
          const foreground = composite(rgba(sample.color), background);
          const ratio = contrast(foreground, background);
          const result = { theme, selector: selector + (pseudo ?? ''), row: sample.row, size: sample.size, foreground, background, ratio };
          results.push(result);
          if (ratio < 4.5) violations.push(result);
        }
      }
      if (process.env.RPI_WORKBENCH_SHOTS) {
        await page.locator('.message-code').scrollIntoViewIfNeeded();
        await page.screenshot({ path: path.join(process.env.RPI_WORKBENCH_SHOTS, 'reading-contrast-' + theme + '.png') });
      }
    }
    if (process.env.RPI_WORKBENCH_SHOTS) fs.writeFileSync(path.join(process.env.RPI_WORKBENCH_SHOTS, 'reading-contrast-metrics.json'), JSON.stringify({ threshold: 4.5, results, violations }, null, 2) + '\n');
    assert.deepEqual(errors, []);
    assert.deepEqual(violations, [], 'Small message/code/diff text must meet 4.5:1 on its actual uniform CSS background');
    console.log(`Message/code/diff computed contrast passed for ${results.length} light/dark text samples, including counters and add/remove gutters (minimum ${Math.min(...results.map(result => result.ratio)).toFixed(2)}:1).`);
  } finally {
    await page.close();
  }
}

module.exports = { checkReadingContrast };
