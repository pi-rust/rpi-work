const assert = require('node:assert/strict');
const { readFileSync, mkdirSync } = require('node:fs');
const { resolve } = require('node:path');
const { createHash } = require('node:crypto');
const { chromium } = require('../support/browser.cjs');

(async () => {
  mkdirSync(resolve(__dirname, '../../artifacts'), { recursive: true });
  const official = readFileSync(resolve(__dirname, '../../assets/branding/rpi.svg'), 'utf8');
  assert.equal(createHash('sha256').update(official).digest('hex'), '67f5fd2e2791443e3dace12dc02219a387248f4948e2bd840bf34c88bba36744');
  const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
  try {
    for (const width of [1280, 390]) {
      const page = await browser.newPage({ viewport: { width, height: 900 } });
      const errors = [];
      page.on('pageerror', error => errors.push(error.message));
      // No shared node, registry, or actual agent requests.
      await page.route('**/api/**', route => route.fulfill({ status: 503, json: { error: 'Isolated branding test' } }));
      await page.route('https://api.github.com/**', route => route.fulfill({ status: 404, body: '{}' }));
      await page.goto(process.env.WORK_UI_URL || 'http://127.0.0.1:14180/');
      if (width === 390) await page.getByRole('button', { name: '打开项目导航', exact: true }).click();
      const mark = page.locator('.brand .brand-mark');
      await mark.waitFor({ state: 'visible' });
      await page.waitForFunction(() => {
        const sidebar = document.querySelector('.sidebar').getBoundingClientRect();
        return sidebar.left >= 0 && sidebar.right <= innerWidth;
      });
      const svg = mark.locator('svg');
      assert.equal(await svg.getAttribute('viewBox'), '0 0 32 32');
      assert.equal(await svg.locator('rect').count(), 4);
      assert.equal(await svg.locator(':scope > rect').getAttribute('fill'), '#f4f1e9');
      assert.equal(await svg.locator('g rect').nth(0).getAttribute('fill'), '#101113');
      assert.equal(await svg.locator('g rect').nth(1).getAttribute('fill'), '#ec6b38');
      const box = await mark.boundingBox();
      const svgBox = await svg.boundingBox();
      assert.deepEqual([box.width, box.height], [25, 25], 'match the actual Web workspace brand role');
      assert.equal(svgBox.width, box.width);
      assert.equal(svgBox.height, box.height);
      assert(box.x >= 0 && box.x + box.width <= width);
      const favicon = page.locator('link[rel="icon"][type="image/svg+xml"]');
      await favicon.waitFor({ state: 'attached' });
      const href = await favicon.getAttribute('href');
      assert.equal(decodeURIComponent(href.slice('data:image/svg+xml,'.length)), official);
      assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
      assert.deepEqual(errors, []);
      await page.screenshot({ animations: 'disabled', path: resolve(__dirname, `../../artifacts/brand-icon-${width}.png`) });
      console.log(`PASS official logo, layout and favicon at ${width}px`);
      await page.close();
    }
  } finally {
    await browser.close();
  }
})().catch(error => { console.error(error); process.exit(1); });
