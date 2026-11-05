// Formal WASM + deterministic local node; no model calls or prototype styling.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');
const baseline = process.env.CODEX_SURFACES_BASELINE === '1';
const url = process.env.WORK_UI_URL || 'http://127.0.0.1:14182/app/';
const prefix = `artifacts/codex-surfaces-${baseline ? 'before-' : ''}`;
const draft = '梳理现有功能，并保持节点状态、草稿和主题配置独立。';
const packageDesign = {
  schemaVersion: 1, kind: 'rpi-work-design-package', id: 'visual.review', name: '工作区审阅', version: '1.0.0', author: 'Local fixture',
  description: '保留文件审阅步骤、提示词与版本快照。这里只是本地设计，不代表已经应用到执行节点。',
  resources: { skills: [{ id: 'review', name: '变更审阅', description: '检查边界和验证结果', content: '# 审阅\n先检查真实变更，再执行本地验证。' }], tools: ['read'], prompts: [] },
  design: { resourceKind: 'Package', sources: { 'README.md': '# 工作区审阅\n设计资源，不执行代码。' } },
};
(async () => {
  fs.mkdirSync('artifacts', { recursive: true });
  const fixture = await startFixture(0);
  const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
  const page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
  const results = [], errors = [], measurements = {};
  page.setDefaultTimeout(10000); page.on('pageerror', error => errors.push(error.message));
  const pass = text => { results.push(text); console.log('PASS', text); };
  const shot = name => page.screenshot({ path: `${prefix}${name}.png`, animations: 'disabled', timeout: 10000 });
  const computed = (selector, property) => page.locator(selector).first().evaluate((e, p) => getComputedStyle(e).getPropertyValue(p), property);
  const themeDialog = () => page.getByRole('dialog', { name: '外观与主题', exact: true });
  const openTheme = () => page.getByRole('button', { name: '外观与主题', exact: true }).click();
  const saveMode = async mode => {
    await openTheme(); await themeDialog().getByLabel('明暗模式', { exact: true }).selectOption(mode);
    await page.waitForFunction(mode => document.documentElement.dataset.mode === mode, mode);
    await themeDialog().getByRole('button', { name: '保存', exact: true }).click();
    await themeDialog().waitFor({ state: 'hidden' });
  };
  const bounded = async selector => {
    assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), 'document overflow');
    assert(await page.locator(selector).first().evaluate(e => e.scrollWidth <= e.clientWidth + 1), `overflow: ${selector}`);
  };
  const openContext = async () => {
    const inspector = page.locator('.inspector');
    if (!await inspector.isVisible()) await page.getByRole('button', { name: '查看任务上下文', exact: true }).click();
    await inspector.waitFor({ state: 'visible' });
    await inspector.getByRole('button', { name: '管理上下文', exact: true }).click();
    await page.getByRole('dialog', { name: '上下文与压缩', exact: true }).waitFor();
  };
  const measure = async (name, selectors) => {
    measurements[name] = {};
    for (const selector of selectors) measurements[name][selector] = await computed(selector, 'font-size');
  };
  try {
    await page.goto(url);
    await page.locator('.transcript > .empty-state').waitFor();
    await shot('welcome');
    if (!baseline) {
      assert.equal(await computed('.workspace-welcome', 'border-top-style'), 'none');
      assert.equal(await computed('.workspace-welcome > strong', 'font-size'), '24px');
    }
    await page.getByRole('button', { name: '打开设置', exact: true }).click();
    await page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '执行节点', exact: true }).click();
    await page.getByRole('button', { name: '连接其他节点', exact: true }).click();
    await page.getByLabel('节点地址', { exact: true }).fill(fixture.url);
    await page.getByRole('button', { name: '连接节点', exact: true }).click();
    await page.getByRole('status').filter({ hasText: '在线' }).waitFor();
    await page.getByRole('button', { name: '返回会话', exact: true }).click();
    await page.locator('.project-row').filter({ hasText: 'rpi-work' }).click();
    await page.locator('.sidebar-sessions').getByRole('button', { name: '为工作台添加深色模式', exact: true }).click();
    await page.locator('.transcript').getByText('主题切换已经准备好，接下来运行组件测试。', { exact: true }).waitFor();
    await page.locator('.composer textarea').fill(draft);
    const tool = page.locator('.conversation-widget .inline-call').first();
    await tool.locator('summary').click(); await tool.locator('.call-body').waitFor();
    await measure('conversation', ['.call-name strong', '.call-name code', '.call-status', '.call-body pre', '.composer-select', '.capability-menu > summary', '.runtime-footer .cache-metric', '.composer-foot']);
    if (!baseline) {
      for (const selector of ['.call-name code', '.call-status', '.call-body pre', '.capability-menu > summary', '.composer-foot']) assert.equal(await computed(selector, 'font-size'), '12px', selector);
      const footerFont = parseFloat(await computed('.runtime-footer .cache-metric', 'font-size'));
      assert(footerFont >= 10 && footerFont <= 12, 'readable auxiliary footer text');
      assert.equal(await computed('.call-name strong', 'font-size'), '12px');
      assert.equal(await computed('.composer-select', 'font-size'), '14px');
    }
    const cdp = await page.context().newCDPSession(page);
    await cdp.send('DOM.enable'); await cdp.send('CSS.enable');
    const { root } = await cdp.send('DOM.getDocument');
    const { nodeId } = await cdp.send('DOM.querySelector', { nodeId: root.nodeId, selector: '.conversation-user-text' });
    measurements.platformFonts = (await cdp.send('CSS.getPlatformFontsForNode', { nodeId })).fonts;
    await cdp.detach();
    await shot('conversation-light'); await saveMode('dark'); await shot('conversation-dark'); await saveMode('light');
    pass(baseline ? 'conversation and tool baseline captured in both modes' : 'conversation, expanded tools and editor use readable role tokens in both modes');

    await openTheme(); await themeDialog().locator('.theme-preset').filter({ hasText: 'Reddit 橙' }).click();
    await page.waitForFunction(() => document.documentElement.dataset.theme === 'reddit');
    assert.equal(await computed('.call-name code', 'font-size'), '9px');
    await page.keyboard.press('Escape'); await themeDialog().waitFor({ state: 'hidden' });
    await page.waitForFunction(() => document.documentElement.dataset.theme === 'codex');
    assert.equal(await page.locator('.composer textarea').inputValue(), draft);
    pass('Codex-only treatment does not alter the legacy theme or the saved draft');

    await page.locator('.sidebar').getByRole('button', { name: 'Packages', exact: true }).click();
    await page.getByRole('button', { name: '导入资源包', exact: true }).click();
    const importing = page.getByRole('dialog', { name: '导入设计包', exact: true });
    await importing.locator('textarea').fill(JSON.stringify(packageDesign));
    await importing.getByRole('button', { name: '校验并导入', exact: true }).click();
    await importing.waitFor({ state: 'hidden' }); await page.locator('.resource-card').first().waitFor();
    await page.locator('.resource-import-feedback button').click();
    await measure('resources', ['.resource-card > p', '.resource-card-title code', '.resource-actions .button']);
    if (!baseline) {
      assert.equal(await computed('.resource-card > p', 'font-size'), '15px');
      assert.equal(await computed('.resource-card-title code', 'font-size'), '12px');
      assert.equal(await computed('.resource-actions .button', 'font-size'), '14px');
    }
    await shot('resources');
    await page.getByRole('button', { name: '打开设置', exact: true }).click();
    await page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '通用', exact: true }).click();
    await page.locator('.settings-card').first().waitFor();
    await measure('settings', ['.settings-card > p', '.settings-card > small']);
    if (!baseline) assert.equal(await computed('.settings-card > p', 'font-size'), '15px');
    await shot('settings');
    await page.getByRole('button', { name: '返回会话', exact: true }).click();
    await page.getByRole('button', { name: '查看任务上下文', exact: true }).click();
    await page.locator('.inspector').waitFor({ state: 'visible' });
    await measure('inspector', ['.environment-card > code', '.environment-heading strong']);
    if (!baseline) assert.equal(await computed('.environment-card > code', 'font-size'), '12px');
    await shot('inspector');
    await page.getByRole('button', { name: '查看任务上下文', exact: true }).click();
    await page.locator('.inspector').waitFor({ state: 'hidden' });
    const footer = page.locator('.runtime-footer');
    await footer.waitFor({ state: 'visible' });
    assert.match(await footer.locator('[data-item-id="input-output"]').getAttribute('aria-label'), /会话累计\s*↑32k\s*↓4\.8k/);
    assert.equal(await footer.locator('[data-item-id="input-output"] svg[data-icon="diff"]').count(), 1);
    assert.equal(await footer.locator('.context-ring[data-known="true"]').count(), 1);
    assert.equal(await footer.locator('button, a, [role="button"], [tabindex]').count(), 0);
    await shot('footer');
    pass('resources, settings and inspector remain reachable; inline read-only totals retain capability boundaries');

    await page.getByRole('button', { name: '打开项目文件', exact: true }).click();
    const files = page.getByRole('dialog', { name: '项目文件', exact: true });
    await files.getByRole('heading', { name: 'RPI Work', exact: true }).waitFor();
    await measure('files', ['.project-file-row', '.project-preview-title strong']);
    if (!baseline) assert.equal(await computed('.project-file-row', 'font-size'), '14px');
    await shot('files'); await page.keyboard.press('Escape');
    await page.getByRole('tab', { name: /^变更/ }).click();
    await page.locator('.change-file').first().click(); await page.locator('.selected-diff-scroll').waitFor();
    await measure('review', ['.diff-lines', '.diff-hunk']);
    if (!baseline) {
      assert.equal(await computed('.diff-lines', 'background-color'), await computed('.change-preview', 'background-color'));
      assert.notEqual(await computed('.diff-line.add', 'background-color'), await computed('.diff-line.del', 'background-color'));
    }
    await shot('review');
    await saveMode('dark');
    if (!baseline) assert.equal(await computed('.diff-lines', 'background-color'), await computed('.change-preview', 'background-color'));
    await shot('review-dark');
    await saveMode('light');
    await page.getByRole('tab', { name: '对话', exact: true }).click();
    pass('read-only files and actual fixture diffs retain the existing review actions');

    for (const width of [1280, 390, 320]) {
      await page.setViewportSize({ width, height: width < 700 ? 844 : 900 });
      await bounded('.composer');
      assert(await page.locator('.runtime-footer').evaluate(e => {
        const r = e.getBoundingClientRect(), s = getComputedStyle(e);
        return r.left >= 0 && r.right <= innerWidth + 1 && r.height <= 32 && s.flexWrap === 'nowrap' && s.overflowX === 'auto';
      }), 'footer stays compact and contains its own horizontal scrolling');
      await shot(`conversation-${width}`);
      if (!baseline && width < 700) {
        assert((await page.locator('.send-button').first().boundingBox()).height >= 44, 'primary send target');
        const selector = page.getByRole('button', { name: '选择供应商与模型', exact: true });
        const box = await selector.boundingBox();
        assert(box && box.height >= 28 && box.height <= 36, `model picker: compact auxiliary control ${box?.height}`);
        assert.equal(await page.locator('.composer-capabilities').isVisible(), false);
      }
      assert((await footer.boundingBox()).height <= 80, 'default footer remains compact without touch-target sizing');
      await openContext();
      const context = page.getByRole('dialog', { name: '上下文与压缩', exact: true });
      await bounded('#runtime-context'); await shot(`context-${width}`); await page.keyboard.press('Escape');
      await context.waitFor({ state: 'hidden' });
      await page.locator('.inspector .inspector-close').click();
      await page.locator('.inspector').waitFor({ state: 'hidden' });
      if (width < 700) {
        await page.getByRole('button', { name: '打开项目导航', exact: true }).click();
        await page.locator('.sidebar').getByRole('button', { name: 'Packages', exact: true }).click();
        await bounded('.resources-page'); await shot(`resources-${width}`);
        if (!baseline) assert((await page.locator('.resource-actions .button').first().boundingBox()).height >= 44);
        await page.getByRole('button', { name: '打开项目导航', exact: true }).click();
        await page.getByRole('button', { name: '打开设置', exact: true }).click();
        await page.locator('.settings-card').first().waitFor(); await bounded('.settings-page'); await shot(`settings-${width}`);
        await page.getByRole('button', { name: '返回会话', exact: true }).click();
        await page.getByRole('button', { name: '打开项目文件', exact: true }).click();
        await files.locator('.project-file-row').filter({ hasText: 'README.md' }).click();
        await files.getByRole('heading', { name: 'RPI Work', exact: true }).waitFor();
        await bounded('.project-files-modal'); await shot(`files-${width}`); await page.keyboard.press('Escape');
        await page.locator('.section-tabs').evaluate(e => { e.scrollLeft = 0; });
      }
      assert.equal(await page.locator('.composer textarea').inputValue(), draft);
    }
    pass('1280/390/320 layouts, Inspector context, resource/settings/file pages and draft stay intact');
    if (!baseline) {
      await page.setViewportSize({ width: 390, height: 400 });
      const send = await page.locator('.send-button').boundingBox();
      assert(send.y >= 0 && send.y + send.height <= 400, 'Send must remain visible in a short viewport');
      await page.setViewportSize({ width: 320, height: 844 });
      await page.getByRole('button', { name: '选择供应商与模型', exact: true }).focus(); await page.keyboard.press('Tab');
      assert.equal(await page.evaluate(() => getComputedStyle(document.activeElement).outlineStyle), 'solid');
      const large = JSON.parse(fs.readFileSync('examples/themes/codex-reading.json', 'utf8'));
      large.theme.name = '大字主题验证';
      Object.assign(large.theme.design.typography, { scale: 1.25, navSize: 16, assistSize: 14, codeSize: 14 });
      Object.assign(large.theme.design.geometry, { hitSize: 64, controlHeight: 48 });
      await openTheme();
      await themeDialog().locator('#appearance-import-file').setInputFiles({ name: 'large.json', mimeType: 'application/json', buffer: Buffer.from(JSON.stringify(large)) });
      await page.waitForFunction(() => document.documentElement.dataset.theme === 'codex-reading' && getComputedStyle(document.body).fontSize === '20px');
      assert.equal(await computed('.call-status', 'font-size'), `${9 * large.theme.design.typography.scale}px`, 'Codex-only tool styles must not leak into explicit custom themes');
      await measure('customDesign', ['body', '.call-status', '.call-name code']);
      await bounded('.composer');
      // Legacy/custom styles are intentionally outside the Codex-only polish.
      // Record the large-type footer's existing constraint rather than claiming it is fixed.
      measurements.customDesign.footer = await page.locator('.runtime-footer').evaluate(e => ({ clientWidth: e.clientWidth, scrollWidth: e.scrollWidth }));
      assert((await page.locator('.send-button').boundingBox()).height >= 64);
      await page.keyboard.press('Escape'); await themeDialog().waitFor({ state: 'hidden' });
      assert.equal(await computed('.call-name code', 'font-size'), '12px');
      assert.equal(await page.locator('.composer textarea').inputValue(), draft);
      pass('keyboard focus, short viewport Send and an explicit v2 theme preview remain safe');
    }
    assert.equal(fixture.requests.filter(r => r.body.type === 'prompt').length, 0);
    assert.deepEqual(errors, []); pass('no model invocation or application JavaScript errors');
  } catch (error) {
    await shot('failure'); throw error;
  } finally {
    fs.writeFileSync(`${prefix}validation.json`, JSON.stringify({ baseline, results, measurements, errors }, null, 2));
    await browser.close(); await fixture.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
