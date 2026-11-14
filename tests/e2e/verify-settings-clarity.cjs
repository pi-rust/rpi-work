// Real production WASM and the canonical HTML reference, served locally.
// No screenshots, model calls or writes to a real node.
const assert = require('node:assert/strict');
const http = require('node:http');
const fs = require('node:fs/promises');
const path = require('node:path');
const { chromium } = require('../support/browser.cjs');
const root = path.resolve(__dirname, '../..');
const types = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.wasm': 'application/wasm' };

(async () => {
  const server = http.createServer(async (request, response) => {
    try {
      const url = new URL(request.url, 'http://localhost');
      const reference = url.pathname.startsWith('/reference/');
      const base = path.join(root, reference ? 'design/clarity-workbench' : 'target/parity-web');
      const relative = decodeURIComponent(url.pathname.slice(reference ? '/reference/'.length : '/app/'.length));
      const file = path.resolve(base, relative || 'index.html');
      if (!file.startsWith(base + path.sep)) { response.writeHead(403); response.end(); return; }
      const content = await fs.readFile(file);
      response.writeHead(200, { 'content-type': types[path.extname(file)] || 'application/octet-stream', 'cache-control': 'no-store' });
      response.end(content);
    } catch { response.writeHead(404); response.end(); }
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  const url = `http://127.0.0.1:${server.address().port}`;
  const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
  const page = await browser.newPage({ viewport: { width: 1440, height: 960 } });
  const reference = await browser.newPage({ viewport: { width: 1440, height: 960 } });
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  page.setDefaultTimeout(10000);
  const nav = () => page.getByRole('navigation', { name: '设置分类', exact: true });
  const choose = name => nav().getByRole('button', { name, exact: true }).click();
  const tabs = () => page.getByRole('navigation', { name: '界面与显示分类', exact: true });
  const appearance = () => page.locator('section#appearance.appearance-inline');
  const mode = name => appearance().getByRole('button', { name, exact: true }).click();
  const active = value => page.waitForFunction(value => document.documentElement.dataset.mode === value, value);
  const openSettings = async () => {
    if (await page.getByRole('button', { name: '打开项目导航', exact: true }).isVisible()) {
      await page.getByRole('button', { name: '打开项目导航', exact: true }).click();
    }
    await page.getByRole('button', { name: '打开设置', exact: true }).click();
    await appearance().waitFor();
  };
  try {
    await page.route('https://api.github.com/**', route => route.abort());
    await page.goto(url + '/app/');
    await openSettings();
    await reference.goto(url + '/reference/#settings');
    await reference.locator('#settings-page').waitFor();
    assert.deepEqual(await nav().getByRole('button').allTextContents(), ['设置总览', '界面与显示', '工作协同', '供应商', '应用更新', '高级设置']);
    assert.equal(await appearance().locator('.theme-additional').count(), 1, 'rebuild production WASM before running this test');
    for (const width of [1920, 1440, 1280, 1024, 768, 390, 320]) {
      await page.setViewportSize({ width, height: 960 });
      await page.waitForFunction(width => document.querySelector('.app-shell')?.dataset.deviceMode === (width < 768 ? 'mobile' : 'desktop'), width);
      await reference.setViewportSize({ width, height: 960 });
      for (const target of [page, reference]) await target.evaluate(async () => {
        document.querySelector('.settings-page').scrollTop = 0;
        await document.fonts.ready;
        await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
      });
      const production = await page.evaluate(() => {
        const rect = selector => {
          const element = document.querySelector(selector), r = element.getBoundingClientRect(), s = getComputedStyle(element);
          return { x: r.x, y: r.y, width: r.width, height: r.height, direction: s.flexDirection, position: s.position };
        };
        return { width: innerWidth, scroll: document.documentElement.scrollWidth, page: rect('.settings-page'), heading: rect('.settings-page-heading h1'), layout: rect('.settings-layout'), rail: rect('.settings-navigation'), content: rect('.settings-content'), tab: rect('.settings-display-navigation button'), selected: rect('.settings-display-navigation button.active'), grid: getComputedStyle(document.querySelector('.theme-preset-grid')).gridTemplateColumns.split(' ').length };
      });
      const design = await reference.evaluate(() => {
        const rect = selector => { const r = document.querySelector(selector).getBoundingClientRect(); return { x:r.x, y:r.y, width:r.width }; };
        return { page:rect('.settings-page'), heading:rect('.settings-page .page-heading h1'), layout:rect('.settings-layout'), content:rect('.settings-content'), direction:getComputedStyle(document.querySelector('.settings-nav')).flexDirection, tabs:getComputedStyle(document.querySelector('.settings-tabs button.active')).borderBottomWidth };
      });
      const near = (a, b, message) => assert(Math.abs(a - b) <= 2, `${width}: ${message}: ${a} != ${b}\n${JSON.stringify({ production, design })}`);
      near(production.page.x, design.page.x, 'settings fills the main column');
      near(production.page.width, design.page.width, 'no legacy centered page width');
      near(production.heading.x, design.heading.x, 'page heading horizontal anchor');
      near(production.heading.y - production.page.y, design.heading.y - design.page.y, 'page heading top inset');
      near(production.layout.y - production.page.y, design.layout.y - design.page.y, 'category layout vertical anchor');
      near(production.content.x, design.content.x, 'content horizontal anchor');
      near(production.content.width, design.content.width, 'content width');
      assert(production.scroll <= width + 1, JSON.stringify(production));
      assert.equal(production.rail.position, 'static');
      assert.equal(production.rail.direction, design.direction);
      assert.equal(production.grid, 3);
      assert.equal(await appearance().locator('.theme-preset:visible').count(), 3);
      assert.equal(await page.locator('.settings-display-navigation button.active').evaluate(e => getComputedStyle(e).borderBottomWidth), design.tabs);
      if (width > 1050) {
        assert(production.content.x >= production.rail.x + production.rail.width + 20);
        assert(production.rail.height < 500, 'category rail must not stretch to the page height');
      }
      for (const name of ['会话展示', '底部状态栏', '外观与主题']) {
        await tabs().getByRole('button', { name, exact: true }).click();
        assert.equal(await page.locator('.settings-intro h2').textContent(), '界面与显示');
        assert.equal(await tabs().locator('[aria-current="page"]').textContent(), name);
      }
      assert.equal(await appearance().getByLabel('界面密度', { exact: true }).count(), 1);
      if (width < 768) {
        for (const label of ['工作协同', '供应商', '应用更新', '高级设置', '界面与显示']) {
          const button = nav().getByRole('button', { name: label, exact: true });
          await button.focus(); await page.keyboard.press('Enter');
          await page.waitForFunction(() => {
            const e = document.querySelector('.settings-navigation [aria-current="page"]'), rail = e?.closest('.settings-rail');
            if (!rail) return false;
            const a = e.getBoundingClientRect(), b = rail.getBoundingClientRect();
            return a.left >= b.left - 1 && a.right <= b.right + 1;
          });
          assert(await button.evaluate(e => e === document.activeElement));
          assert((await button.boundingBox()).height >= 44);
          assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1));
        }
      }
      console.log('PASS Clarity settings structure, tabs, category navigation', width);
    }
    assert.equal(await page.locator('.settings-navigation .settings-return').count(), 0, 'return action never displaces category navigation');
    assert.equal(await page.locator('.settings-page-heading .settings-return').count(), 1);
    assert.deepEqual(await appearance().locator('.theme-preset-grid').first().locator('.theme-preset-label strong').allTextContents(), ['Codex 黑白', '澄蓝', '石墨']);
    await page.setViewportSize({ width:1440, height:960 });
    await choose('工作协同');
    assert.equal(await page.locator('.settings-page h1').count(), 1, 'category content does not introduce another page title');
    assert.equal(await page.locator('.collaboration-actions > .collaboration-action-card').count(), 3);
    assert.equal(await page.locator('.connection-details').getAttribute('open'), null, 'legacy node diagnostics are collapsed');
    assert.equal(await page.locator('.connection-details .connection-page-heading').isVisible(), false);
    assert.equal(await page.locator('.web-access-card [role=switch]').count(), 0, 'Web status must not invent a separate host permission switch');
    await page.locator('.connect-card').getByRole('button', { name:'连接设备', exact:true }).click();
    assert.equal(await page.getByLabel('节点地址', { exact:true }).count(), 1, 'only one connection form, with unique field IDs');
    await page.locator('.connect-card').getByRole('button', { name:'取消', exact:true }).click();
    await page.locator('.connection-details > summary').click();
    await page.locator('.server-overview').waitFor();
    await choose('供应商');
    assert.equal(await page.locator('.settings-page h1').count(), 1);
    await page.getByLabel('搜索供应商', { exact:true }).fill('missing-provider');
    await page.getByRole('status').filter({ hasText:'未找到匹配的供应商' }).waitFor();
    assert.equal(await page.locator('.provider-section-heading-muted').textContent(), '自定义供应商', 'no hardcoded zero count or extension requirement');
    await choose('界面与显示');
    console.log('PASS settings heading hierarchy, canonical themes, single connection form, collapsed node details and honest Web/provider content');
    await mode('深色'); await active('dark');
    for (const width of [1440, 1024, 390, 320]) {
      await page.setViewportSize({ width, height: 960 });
      await page.waitForFunction(width => document.querySelector('.app-shell')?.dataset.deviceMode === (width < 768 ? 'mobile' : 'desktop'), width);
      assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1));
      assert.equal(await appearance().getByRole('button', { name: '深色', exact: true }).getAttribute('aria-pressed'), 'true');
      const footer = await appearance().locator('.modal-actions').boundingBox();
      const density = await appearance().locator('.appearance-density-row').boundingBox();
      assert(footer.y >= density.y + density.height, 'save actions must follow the form without overlapping it');
      console.log('PASS dark settings and save actions', width);
    }
    await appearance().getByRole('button', { name: '取消', exact: true }).click(); await active('light');
    await page.setViewportSize({ width: 1440, height: 960 });
    await mode('深色'); await active('dark');
    await choose('高级设置'); await active('light');
    await choose('界面与显示');
    await mode('深色'); await active('dark');
    await appearance().getByRole('button', { name: '保存更改', exact: true }).click();
    await page.waitForFunction(() => JSON.parse(localStorage.getItem('rpi-work-settings-v1'))?.appearance?.mode === 'dark');
    await appearance().waitFor();
    assert.equal(await tabs().locator('[aria-current="page"]').textContent(), '外观与主题');
    await mode('浅色'); await active('light');
    await appearance().getByRole('button', { name: '取消', exact: true }).click(); await active('dark');
    await mode('浅色'); await active('light');
    await choose('高级设置'); await active('dark');
    await page.reload(); await openSettings(); await active('dark');
    console.log('PASS preview rollback, save stays on the tab, editing after save, cancel restores last save, persistence across reload');

    await appearance().locator('.theme-additional > summary').click();
    await appearance().locator('.theme-preset').filter({ hasText: '纸与苔绿' }).click();
    await page.waitForFunction(() => document.documentElement.dataset.theme === 'paper');
    await appearance().getByRole('button', { name: '取消', exact: true }).click();
    await page.waitForFunction(() => document.documentElement.dataset.theme === 'codex');
    await appearance().locator('.theme-advanced > summary').click();
    const [download] = await Promise.all([page.waitForEvent('download'), appearance().getByRole('button', { name: '导出偏好 JSON', exact: true }).click()]);
    const preference = JSON.parse(await fs.readFile(await download.path(), 'utf8'));
    assert.equal(preference.preset, 'codex'); assert.equal(preference.mode, 'dark');
    await appearance().locator('#appearance-import-file').setInputFiles({ name: 'settings.json', mimeType: 'application/json', buffer: Buffer.from(JSON.stringify({ ...preference, preset: 'ocean', mode: 'light' })) });
    await page.waitForFunction(() => document.documentElement.dataset.theme === 'ocean');
    await appearance().getByRole('button', { name: '取消', exact: true }).click();
    await page.waitForFunction(() => document.documentElement.dataset.theme === 'codex');
    await tabs().getByRole('button', { name: '会话展示', exact: true }).click();
    await page.getByLabel('会话展示方式', { exact: true }).selectOption('flat');
    await page.reload(); await openSettings();
    await tabs().getByRole('button', { name: '会话展示', exact: true }).click();
    assert.equal(await page.getByLabel('会话展示方式', { exact: true }).inputValue(), 'flat');
    console.log('PASS extra themes, JSON export/import and rollback, session display persistence');
    await tabs().getByRole('button', { name: '外观与主题', exact: true }).click();
    await page.evaluate(() => {
      const write = Storage.prototype.setItem;
      Storage.prototype.setItem = function (key, value) {
        if (key === 'rpi-work-settings-v1') throw new DOMException('Isolated persistence failure', 'QuotaExceededError');
        return write.call(this, key, value);
      };
    });
    await mode('浅色'); await active('light');
    await appearance().getByRole('button', { name: '保存更改', exact: true }).click();
    await appearance().locator('.appearance-storage-error').filter({ hasText: /无法保存/ }).waitFor();
    assert.equal(await page.evaluate(() => JSON.parse(localStorage.getItem('rpi-work-settings-v1')).appearance.mode), 'dark');
    assert.deepEqual(errors, []);
    console.log('PASS persistence failure stays visible without claiming a successful disk write; no JavaScript errors');
  } finally {
    await browser.close();
    await new Promise(resolve => server.close(resolve));
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
