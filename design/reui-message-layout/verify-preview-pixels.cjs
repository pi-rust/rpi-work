const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { createRequire } = require('node:module');
const driverRequire = createRequire(require.resolve(process.env.RPI_LAYOUT_PLAYWRIGHT ?? '../../tools/node_modules/playwright'));
const { PNG } = driverRequire('playwright-core/lib/utilsBundle');

async function checkPreviewPixels(browser, url) {
  const captures = [];
  const errors = [];
  for (const controlled of [false, true]) {
    const mode = controlled ? 'clock' : 'realtime';
    const page = await browser.newPage({ viewport: { width: 1440, height: 940 }, deviceScaleFactor: 1, colorScheme: 'light', reducedMotion: 'reduce' });
    page.on('pageerror', error => errors.push(error.message));
    const button = name => page.getByRole('button', { name, exact: true });
    const frame = page.frameLocator('iframe[title="页面预览"]');
    const theme = async value => {
      await button('设置').click();
      await button('外观').click();
      await page.locator('#theme').selectOption(value);
      await page.waitForFunction(preference => document.documentElement.dataset.theme ===
        (preference === 'system' ? (matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light') : preference), value);
      await button('关闭设置').click();
    };
    // Sample the original full-window PNG, not a locator screenshot that could
    // scroll/repaint the target. No refresh, injected styles or DOM repair.
    const sample = async name => {
      const heading = await frame.locator('h1').boundingBox();
      const main = await frame.locator('main').boundingBox();
      assert.ok(heading && main);
      const bytes = await page.screenshot();
      const image = PNG.sync.read(bytes);
      const ink = box => {
        const left = Math.ceil(box.x), top = Math.ceil(box.y);
        const right = Math.floor(box.x + box.width), bottom = Math.floor(box.y + box.height);
        assert.ok(left >= 0 && top >= 0 && right <= image.width && bottom <= image.height, 'Fixture region must be inside the captured viewport');
        let count = 0;
        for (let y = top; y < bottom; y++) {
          for (let x = left; x < right; x++) {
            const pixel = (y * image.width + x) * 4;
            if (image.data[pixel] < 180 && image.data[pixel + 1] < 180 && image.data[pixel + 2] < 180) count++;
          }
        }
        return count;
      };
      const result = { name: mode + '-' + name, headingInk: ink(heading), mainInk: ink(main) };
      assert.ok(result.headingInk > 100, 'A visible heading must actually paint ink in the PNG');
      assert.ok(result.mainInk > 500, 'The preview body must not be blank in the PNG');
      captures.push(result);
      if (process.env.RPI_WORKBENCH_SHOTS) {
        const directory = process.env.RPI_WORKBENCH_SHOTS;
        fs.writeFileSync(path.join(directory, 'preview-paint-' + result.name + '.png'), bytes);
        const crop = new PNG({ width: Math.floor(main.width), height: Math.floor(main.height) });
        PNG.bitblt(image, crop, Math.ceil(main.x), Math.ceil(main.y), crop.width, crop.height, 0, 0);
        fs.writeFileSync(path.join(directory, 'preview-body-' + result.name + '.png'), PNG.sync.write(crop));
      }
      return result;
    };
    const compare = (actual, baseline) => {
      for (const key of ['headingInk', 'mainInk']) {
        // Allow small antialiasing/caret differences without accepting a blank
        // page. Counts are relative; do not hard-code macOS font rasterization.
        assert.ok(actual[key] >= baseline[key] * .9 && actual[key] <= baseline[key] * 1.1,
          `${actual.name}: ${key} changed from ${baseline[key]} to ${actual[key]}`);
      }
    };
    try {
      if (controlled) await page.clock.install();
      await page.goto(url + '?view=new');
      await frame.getByRole('heading', { name: '项目概览', exact: true }).waitFor();
      const baseline = await sample('overview-light');
      await theme('light');
      compare(await sample('dialog-only'), baseline);
      await theme('dark');
      compare(await sample('overview-dark'), baseline);
      await theme('light');
      compare(await sample('overview-light-return'), baseline);
      await theme('system');
      for (const colorScheme of ['dark', 'light']) {
        await page.emulateMedia({ colorScheme });
        await page.waitForFunction(value => document.documentElement.dataset.theme === value, colorScheme);
        compare(await sample('system-' + colorScheme), baseline);
      }

      await frame.getByRole('button', { name: '设置', exact: true }).click();
      await frame.getByRole('heading', { name: '项目设置', exact: true }).waitFor();
      const input = frame.getByLabel('项目名称', { exact: true });
      await input.fill('未保存的项目名称');
      await input.evaluate(node => node.setSelectionRange(2, 5));
      const documentId = await frame.locator('body').evaluate(node => node.dataset.paintProbe = 'same-document');
      const formBaseline = await sample('form-light');
      for (const value of ['dark', 'light']) {
        await theme(value);
        assert.equal(await input.inputValue(), '未保存的项目名称', 'Theme switching must not discard unsaved iframe input');
        assert.deepEqual(await input.evaluate(node => [node.selectionStart, node.selectionEnd]), [2, 5]);
        assert.equal(await frame.locator('body').getAttribute('data-paint-probe'), documentId, 'The iframe document must not reload');
        compare(await sample('form-' + value + '-retained'), formBaseline);
      }
      assert.equal(await page.locator('iframe[title="页面预览"]').getAttribute('sandbox'), 'allow-scripts');
      assert.equal(await frame.locator('html').evaluate(() => window.origin), 'null');
    } finally {
      await page.close();
    }
  }
  assert.deepEqual(errors, []);
  if (process.env.RPI_WORKBENCH_SHOTS) {
    fs.writeFileSync(path.join(process.env.RPI_WORKBENCH_SHOTS, 'preview-paint-metrics.json'),
      JSON.stringify({ browser: browser.version(), platform: process.platform, captures }, null, 2) + '\n');
  }
  console.log('Preview PNG ink, light/dark/system themes, dialog round trips, realtime/controlled clocks, unsaved iframe form/caret retention and opaque sandbox passed.');
}

module.exports = { checkPreviewPixels };
