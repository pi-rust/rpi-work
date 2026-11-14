const assert = require('node:assert/strict');
const { readFileSync, mkdirSync } = require('node:fs');
const { resolve } = require('node:path');
const { chromium } = require('../support/browser.cjs');

(async () => {
  mkdirSync(resolve(__dirname, '../../artifacts'), { recursive: true });
  const official = readFileSync(resolve(__dirname, '../../assets/branding/rpi-gear.png'));
  const imageURL = `data:image/png;base64,${official.toString('base64')}`;
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
      const image = mark.locator('.brand-image');
      const mask = await image.evaluate(element => getComputedStyle(element).maskImage);
      assert.equal(mask, `url("${imageURL}")`, 'the supplied gear asset is embedded in the UI');
      const box = await mark.boundingBox();
      const imageBox = await image.boundingBox();
      assert.deepEqual([box.width, box.height], [29, 29], 'retain the prototype sidebar geometry');
      assert.equal(imageBox.width, box.width);
      assert.equal(imageBox.height, box.height);
      assert.equal(await mark.getAttribute('aria-hidden'), 'true');
      assert(box.x >= 0 && box.x + box.width <= width);
      const favicon = page.locator('link[rel="icon"][type="image/svg+xml"]');
      await favicon.waitFor({ state: 'attached' });
      const href = await favicon.getAttribute('href');
      assert(href.startsWith('data:image/svg+xml;base64,'));
      const iconSVG = Buffer.from(href.split(',')[1], 'base64').toString('utf8');
      assert(iconSVG.includes(`href="${imageURL}"`));
      assert(iconSVG.includes('prefers-color-scheme:dark'), 'favicon remains visible in dark browser chrome');
      const alpha = await page.evaluate(async url => {
        const image = new Image();
        image.src = url;
        await image.decode();
        const canvas = document.createElement('canvas');
        canvas.width = image.naturalWidth;
        canvas.height = image.naturalHeight;
        const context = canvas.getContext('2d');
        context.drawImage(image, 0, 0);
        const { data } = context.getImageData(0, 0, canvas.width, canvas.height);
        let transparent = 0, ink = 0;
        for (let i = 3; i < data.length; i += 4) {
          if (data[i] === 0) transparent++;
          if (data[i] > 240) ink++;
        }
        return { corner: data[3], transparent, ink };
      }, imageURL);
      assert.equal(alpha.corner, 0);
      assert(alpha.transparent > 100000 && alpha.ink > 10000, 'transparent negative spaces and solid mark both survive export');
      if (width === 390) await page.keyboard.press('Escape');
      await page.locator('.home-mark .brand-image').waitFor();
      assert.equal(await page.locator('.home-mark .brand-image').evaluate(element => getComputedStyle(element).maskImage), mask);
      assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
      assert.deepEqual(errors, []);
      await page.screenshot({ animations: 'disabled', path: resolve(__dirname, `../../artifacts/brand-icon-${width}.png`) });
      if (width === 390) await page.getByRole('button', { name: '打开项目导航', exact: true }).click();
      await page.getByRole('button', { name: '打开设置', exact: true }).click();
      await page.locator('#appearance').getByRole('button', { name: '深色', exact: true }).click();
      await page.waitForFunction(() => document.documentElement.dataset.mode === 'dark');
      await page.locator('#appearance').getByRole('button', { name: '保存更改', exact: true }).click();
      await page.locator('#appearance').getByText('已应用更改。', { exact: true }).waitFor();
      await page.getByRole('button', { name: '返回会话', exact: true }).click();
      const darkImage = page.locator('.home-mark .brand-image');
      const darkFill = await darkImage.evaluate(element => getComputedStyle(element).backgroundColor);
      assert.equal(darkFill, await page.locator('.home-mark').evaluate(element => getComputedStyle(element).color));
      assert(darkFill.match(/\d+/g).slice(0, 3).every(channel => Number(channel) > 150), 'dark mode uses a light logo');
      await page.screenshot({ animations: 'disabled', path: resolve(__dirname, `../../artifacts/brand-icon-dark-${width}.png`) });
      console.log(`PASS supplied gear logo, transparent asset, layout and adaptive favicon at ${width}px`);
      await page.close();
    }
  } finally {
    await browser.close();
  }
})().catch(error => { console.error(error); process.exit(1); });
