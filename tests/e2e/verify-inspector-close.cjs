// Formal WASM + isolated fixture; simulated insets/font scaling, not device acceptance.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');
const results = [], geometry = [], errors = [];
const draftText = '关闭右侧栏时保留草稿 😀';
(async () => {
  fs.mkdirSync('artifacts', { recursive: true });
  const fixture = await startFixture();
  const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
  page.setDefaultTimeout(10000);
  page.on('pageerror', error => errors.push(error.message));
  const panel = () => page.locator('.app-shell > .inspector');
  const close = () => panel().getByRole('button', { name: '关闭任务上下文', exact: true });
  const draft = () => page.getByRole('textbox', { name: '发送给 agent 的消息', exact: true });
  const open = async () => {
    if (!await panel().isVisible()) await page.getByRole('button', { name: '查看任务上下文', exact: true }).click();
    await panel().waitFor({ state: 'visible' });
  };
  const closed = async () => {
    await panel().waitFor({ state: 'hidden' });
    assert.equal(await draft().inputValue(), draftText);
    assert.equal(await page.locator('dialog[open]').count(), 0);
  };
  const inspect = async (scenario, mode) => {
    const data = await close().evaluate(button => {
      const r = button.getBoundingClientRect(), header = button.parentElement;
      const content = button.closest('.inspector').querySelector('.inspector-content');
      // Probe the rounded button's interior and the middle of all four edges;
      // its transparent rounded corners intentionally hit the header.
      const coordinates = [
        [r.left + 2, r.top + r.height / 2], [r.right - 2, r.top + r.height / 2],
        [r.left + r.width / 2, r.top + 2], [r.left + r.width / 2, r.bottom - 2],
      ];
      for (const fx of [.25, .5, .75]) for (const fy of [.25, .5, .75]) coordinates.push([r.left + r.width * fx, r.top + r.height * fy]);
      const points = coordinates.map(([x, y]) => {
        const hit = document.elementFromPoint(x, y);
        return { x, y, own: button.contains(hit), hit: hit?.tagName, class: hit?.getAttribute('class') };
      });
      return { button: r.toJSON(), header: header.getBoundingClientRect().toJSON(),
        content: content.getBoundingClientRect().toJSON(), scrollTop: content.scrollTop,
        scrollable: content.scrollHeight > content.clientHeight,
        headerBackground: getComputedStyle(header).backgroundColor, points,
        documentWidth: document.documentElement.scrollWidth };
    });
    const { button: r, header: h } = data;
    const [top, right, bottom, left] = scenario.insets || [0, 0, 0, 0];
    assert(r.width >= 44 && r.height >= 44, 'close needs an uncompressed 44px target');
    assert(r.left >= left && r.right <= scenario.width - right + 1);
    assert(r.top >= top && r.bottom <= scenario.height - bottom + 1);
    assert(r.left >= h.left && r.right <= h.right && r.top >= h.top && r.bottom <= h.bottom);
    assert(data.points.every(point => point.own), `close is covered: ${JSON.stringify(data.points)}`);
    assert.notEqual(data.headerBackground, 'rgba(0, 0, 0, 0)', 'header must cover content behind it');
    assert(data.documentWidth <= scenario.width, 'no horizontal document overflow');
    assert(data.content.top >= h.bottom - 1, 'content cannot occupy the close/header row');
    geometry.push({ mode, scenario: scenario.name, ...data });
    return data;
  };
  try {
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
    await draft().fill(draftText);
    const mutationsBefore = fixture.requests.filter(r => !String(r.body.type).startsWith('get_')).length;
    const scenarios = [
      { name: 'desktop', width: 1440, height: 900 },
      { name: 'tablet', width: 1100, height: 844 },
      { name: 'breakpoint', width: 700, height: 844 },
      { name: 'phone', width: 390, height: 844 },
      { name: 'small-phone', width: 320, height: 740 },
      { name: 'large-type', width: 320, height: 740, scale: 1.8 },
      { name: 'cutout', width: 390, height: 844, insets: [47, 8, 34, 0] },
      { name: 'landscape', width: 844, height: 390, insets: [0, 44, 21, 44] },
      { name: 'keyboard', width: 390, height: 430, insets: [24, 0, 0, 0] },
    ];
    for (const mode of ['light', 'dark']) {
      await page.setViewportSize({ width: 1440, height: 900 });
      await page.evaluate(() => {
        for (const side of ['top', 'right', 'bottom', 'left']) document.documentElement.style.setProperty('--app-safe-' + side, '0px');
        document.querySelector('.app-shell').style.removeProperty('--type-scale');
      });
      if (await panel().isVisible()) { await close().click(); await closed(); }
      await page.getByRole('button', { name: '外观与主题', exact: true }).click();
      const appearance = page.getByRole('dialog', { name: '外观与主题', exact: true });
      await appearance.getByLabel('明暗模式', { exact: true }).selectOption(mode);
      await appearance.getByRole('button', { name: '保存', exact: true }).click();
      await appearance.waitFor({ state: 'hidden' });
      for (const scenario of scenarios) {
        await page.setViewportSize({ width: scenario.width, height: scenario.height });
        await page.evaluate(({ insets = [0, 0, 0, 0], scale = 1 }) => {
          ['top', 'right', 'bottom', 'left'].forEach((side, i) => document.documentElement.style.setProperty('--app-safe-' + side, insets[i] + 'px'));
          document.querySelector('.app-shell').style.setProperty('--type-scale', String(scale));
        }, scenario);
        await open();
        const before = await inspect(scenario, mode);
        const body = await panel().locator('.inspector-content').boundingBox();
        await page.mouse.move(body.x + body.width / 2, body.y + Math.min(body.height / 2, 80));
        await page.mouse.wheel(0, 3000);
        if (before.scrollable) await page.waitForFunction(() => document.querySelector('.inspector-content').scrollTop > 0);
        const after = await inspect(scenario, mode);
        assert.equal(after.header.top, before.header.top, 'scroll cannot move close/header out of view');
        await page.screenshot({ path: `artifacts/inspector-close-${mode}-${scenario.name}.png`, animations: 'disabled' });
        // Real pointer clicks near both outer edges: no forced clicks or DOM click().
        await page.mouse.click(after.button.right - 2, after.button.top + after.button.height / 2); await closed();
        await open();
        const b = await close().boundingBox();
        await page.mouse.click(b.x + 2, b.y + b.height / 2); await closed();
        await open(); await close().focus(); await page.keyboard.press('Enter'); await closed();
        await open(); await close().focus(); await page.keyboard.press('Space'); await closed();
        results.push(`${mode}/${scenario.name}: unobscured target, pinned header, edge clicks and keyboard close preserve draft`);
        console.log('PASS', results.at(-1));
      }
    }
    assert.equal(fixture.requests.filter(r => !String(r.body.type).startsWith('get_')).length, mutationsBefore);
    assert.deepEqual(errors, []);
    console.log('PASS inspector close: 18 layout cases, no runtime mutations or browser errors');
  } catch (error) {
    await page.screenshot({ path: 'artifacts/inspector-close-failure.png', animations: 'disabled' }).catch(() => {});
    throw error;
  } finally {
    fs.writeFileSync('artifacts/inspector-close-validation.json', JSON.stringify({ results, geometry, errors,
      scope: 'Formal WASM + isolated HTTP/SSE fixture. Insets and larger typography are browser simulations; no real Android or native titlebar acceptance.' }, null, 2));
    await browser.close(); await fixture.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
