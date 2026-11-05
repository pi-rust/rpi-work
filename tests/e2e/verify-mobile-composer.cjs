// Formal WASM + isolated HTTP fixture. No model calls or physical phone claims.
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
    await page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '执行节点', exact: true }).click();
    await page.getByRole('button', { name: '连接其他节点', exact: true }).click();
    await page.getByLabel('节点地址', { exact: true }).fill(fixture.url);
    await page.getByRole('button', { name: '连接节点', exact: true }).click();
    await page.locator('.server-details [role=status]').filter({ hasText: '在线' }).waitFor();
    await page.getByRole('button', { name: '返回会话', exact: true }).click();
    await page.locator('.project-row').filter({ hasText: 'rpi-work' }).click();
    await page.locator('.sidebar-sessions').getByRole('button', { name: '为工作台添加深色模式', exact: true }).click();
    const draft = page.getByRole('textbox', { name: '发送给 agent 的消息', exact: true });
    await draft.fill('手机工具栏紧凑验证，保留草稿');
    const measure = () => page.locator('.composer-main').evaluate(root => {
      const box = selector => {
        const el = root.querySelector(selector), rect = el.getBoundingClientRect(), css = getComputedStyle(el);
        return { x: rect.x, y: rect.y, width: rect.width, height: rect.height, font: parseFloat(css.fontSize), scroll: el.scrollWidth, client: el.clientWidth };
      };
      return Object.fromEntries(['.composer-capabilities', '.composer-bottom', '.composer-select', '.capability-menu > summary', '.composer-foot', '.runtime-footer', '.cache-metric', '.context-meter', '.send-button', 'textarea'].map(selector => [selector, box(selector)]));
    });
    const attachment = page.locator('.attachment-button-slot .extras-add');
    const checkAttachment = async mobile => {
      const proof = await attachment.evaluate(label => {
        const box = e => { const r = e.getBoundingClientRect(); return [r.width, r.height]; };
        const visual = getComputedStyle(label, '::before');
        return { hit: box(label), input: box(label.querySelector('input')), icon: box(label.querySelector('svg')), visual: [parseFloat(visual.width), parseFloat(visual.height)], border: getComputedStyle(label).borderWidth };
      });
      assert.deepEqual(proof.visual, [24, 24]); assert.deepEqual(proof.icon, [12, 12]);
      assert.equal(proof.border, '0px'); assert.deepEqual(proof.input, proof.hit);
      if (mobile) assert(proof.hit.every(value => value >= 44), 'attachment keeps its full mobile touch area');
      return proof;
    };
    const checkSend = async mobile => {
      // Both platforms paint the same compact disk inside the retained hit area.
      await page.waitForFunction(() => getComputedStyle(document.querySelector('.composer-actions .send-button')).backgroundColor === 'rgba(0, 0, 0, 0)');
      const proof = await page.locator('.composer-actions .send-button').evaluate(button => {
        const rect = button.getBoundingClientRect(), css = getComputedStyle(button), frame = getComputedStyle(button, '::before');
        const icon = button.querySelector('svg').getBoundingClientRect();
        const context = document.createElement('canvas').getContext('2d');
        const normalize = color => { context.fillStyle = color; return context.fillStyle; };
        return { hit: [rect.width, rect.height], visual: [parseFloat(frame.width), parseFloat(frame.height)],
          radius: frame.borderRadius, background: css.backgroundColor, frameBackground: frame.backgroundColor,
          accent: normalize(css.getPropertyValue('--accent').trim()),
          painted: normalize(frame.backgroundColor), icon: [icon.width, icon.height] };
      });
      assert.equal(proof.painted, proof.accent, 'send disk keeps the accent fill');
      assert.deepEqual(proof.visual, [28, 28]); assert.equal(proof.radius, '50%');
      assert.deepEqual(proof.icon, [12, 12]);
      assert.equal(proof.background, 'rgba(0, 0, 0, 0)', 'only the compact disk paints, not the hit target');
      if (mobile) {
        assert(proof.hit.every(value => value >= 44), 'send retains its full mobile hit target');
      }
      return proof;
    };
    const assertQuietToolbar = async () => {
      const root = page.locator('.composer-main');
      assert.equal(await root.locator('.composer-capabilities').isVisible(), false);
      assert.equal(await root.getByRole('button', { name: 'BTW', exact: true }).count(), 0);
      assert.equal(await root.getByRole('button', { name: /^Skills/ }).count(), 0);
      assert.equal(await root.getByText('常用能力', { exact: true }).isVisible(), false);
      const proof = await root.evaluate(root => {
        const row = root.querySelector('.composer-capabilities').getBoundingClientRect();
        const bottom = getComputedStyle(root.querySelector('.composer-bottom'));
        return { rowWidth: row.width, rowHeight: row.height, divider: parseFloat(bottom.borderTopWidth) };
      });
      assert.equal(proof.rowWidth, 0); assert.equal(proof.rowHeight, 0);
      assert(proof.divider >= 1, 'keep the input/toolbar dividing line without an empty shortcut row');
    };
    await assertQuietToolbar();
    const desktop = await measure();
    await checkSend(false);
    await checkAttachment(false);
    assert.equal(desktop['.composer-select'].font, 14);
    assert(desktop['.cache-metric'].font >= 10 && desktop['.cache-metric'].font <= 12, 'read-only footer remains readable auxiliary text');
    assert(desktop['.runtime-footer'].height <= 80, 'desktop default footer remains compact');
    for (const mode of ['light', 'dark']) {
      await page.getByRole('button', { name: '更多操作', exact: true }).click();
      await page.getByRole('menuitem', { name: '外观与主题', exact: true }).click();
      const theme = page.getByRole('dialog', { name: '外观与主题', exact: true });
      await theme.getByLabel('明暗模式', { exact: true }).selectOption(mode);
      await theme.getByRole('button', { name: '保存', exact: true }).click();
      await theme.waitFor({ state: 'hidden' });
      for (const width of [390, 320]) {
        await page.setViewportSize({ width, height: 844 });
        const sizes = await measure(); const attachmentSize = await checkAttachment(true); const sendSize = await checkSend(true);
        geometry.push({ mode, width, sizes, attachment: attachmentSize, send: sendSize });
        assert(sizes['.composer-select'].font <= 12, 'mobile model selector must use compact auxiliary text');
        assert(sizes['.composer-select'].height >= 44 && sizes['.composer-select'].height <= 45, 'mobile model selector keeps a compact 44px touch target');
        await assertQuietToolbar();
        assert.equal(sizes['.composer-capabilities'].height, 0, 'removed shortcuts leave no empty row');
        assert(sizes['.composer-foot'].height <= 32, 'connection/trace row must remain compact');
        assert(sizes['.runtime-footer'].height <= 32, 'default footer stays a single compact row');
        assert(sizes['.cache-metric'].font <= 11);
        assert(sizes['textarea'].font >= 16, 'typing must not trigger iOS auto-zoom');
        assert(sizes['.send-button'].width >= 44 && sizes['.send-button'].height >= 44, 'primary touch target stays accessible');
        assert(sizes['.composer-bottom'].scroll <= sizes['.composer-bottom'].client + 1, `composer overflows at ${width}`);
        assert.equal(await page.locator('.runtime-footer').evaluate(e => getComputedStyle(e).overflowX), 'auto', 'only the footer scrolls horizontally');
        assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
        const footer = page.locator('.runtime-footer');
        assert.equal(await footer.locator('button, a, [role="button"], [tabindex]').count(), 0);
        assert.match(await footer.locator('[data-item-id="input-output"]').getAttribute('aria-label'), /会话累计\s*↑32k\s*↓4\.8k/);
        assert(sizes['.runtime-footer'].scroll <= sizes['.runtime-footer'].client + 1, 'restored default core metrics fit without horizontal scrolling');
        assert.equal(await footer.locator('.cost-metric').isVisible(), false, 'original mobile footer hides the cost metric');
        for (const selector of ['.cache-metric', '.context-meter', '.tool-metric']) {
          const metric = footer.locator(selector);
          await metric.waitFor({ state: 'visible' });
          assert.equal(await metric.evaluate(e => e.tagName), 'SPAN');
          await metric.click();
          assert.equal(await page.locator('dialog[open]').count(), 0, 'footer metrics do not open dialogs');
        }
        await page.getByRole('button', { name: '选择供应商与模型', exact: true }).click();
        const picker = page.getByRole('dialog', { name: '选择供应商与模型', exact: true });
        await picker.waitFor(); await page.keyboard.press('Escape'); await picker.waitFor({ state: 'hidden' });
        assert.equal(await draft.inputValue(), '手机工具栏紧凑验证，保留草稿');
        // Click outside the 24px visible frame but within the full input target.
        const [chooser] = await Promise.all([page.waitForEvent('filechooser'), attachment.click({ position: { x: 3, y: 3 } })]);
        await chooser.setFiles({ name: 'compact-attachment.md', mimeType: 'text/markdown', buffer: Buffer.from('附件按钮缩小验证') });
        const preview = page.getByRole('button', { name: '预览 compact-attachment.md', exact: true });
        await preview.waitFor(); await preview.click();
        await page.getByText('附件按钮缩小验证', { exact: true }).waitFor(); await page.keyboard.press('Escape');
        await page.getByRole('button', { name: '移除 compact-attachment.md', exact: true }).click();
        await preview.waitFor({ state: 'hidden' });
        assert.equal(await draft.inputValue(), '手机工具栏紧凑验证，保留草稿');
        await footer.evaluate(e => e.scrollTo({ left: 0, behavior: 'instant' }));
        await page.locator('.composer-main').screenshot({ path: `artifacts/mobile-composer-${mode}-${width}.png`, animations: 'disabled' });
        pass(`${mode}/${width}: Skills/BTW/common shortcuts hidden with zero row footprint and divider retained; send 28px/12px with 44px target; attachment preview/removal, read-only footer, model picker and draft intact`);
      }
      await page.setViewportSize({ width: 1280, height: 900 });
      await checkSend(false);
    }
    assert.deepEqual(await measure(), desktop, 'desktop geometry and typography must remain unchanged');
    assert.equal(fixture.requests.filter(request => request.body.type === 'prompt').length, 0);
    assert.deepEqual(errors, []);
    pass('desktop geometry unchanged; no model requests or application errors');
  } finally {
    fs.writeFileSync('artifacts/mobile-composer-validation.json', JSON.stringify({ results, geometry, errors }, null, 2));
    await browser.close(); await fixture.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
