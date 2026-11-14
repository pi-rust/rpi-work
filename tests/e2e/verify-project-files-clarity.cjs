// Production WASM, canonical Clarity HTML and isolated project data.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { pathToFileURL } = require('node:url');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');
const UI = process.env.WORK_UI_URL || 'http://127.0.0.1:14180/';
const DRAFT = '文件与上下文抽屉关闭后保留草稿';
const near = (actual, expected, label) => assert(Math.abs(actual - expected) <= 1, `${label}: ${actual} != ${expected}`);

(async () => {
  fs.mkdirSync('artifacts', { recursive: true });
  const fixture = await startFixture();
  fixture.controls.fileEntriesByPath = {
    '': [
      { name: 'src', path: 'src', kind: 'directory', size: 0 },
      { name: 'README.md', path: 'README.md', kind: 'file', size: 60 },
      { name: 'preview.html', path: 'preview.html', kind: 'file', size: 100 },
    ],
    src: [{ name: 'theme.rs', path: 'src/theme.rs', kind: 'file', size: 40 }],
  };
  const files = [
    { path: 'src/theme.rs', status: 'MM', staged: true, unstaged: true },
    { path: 'src/added.rs', status: '??', staged: false, unstaged: true },
    { path: 'src/deleted.rs', status: ' D', staged: false, unstaged: true },
  ];
  const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
  const context = await browser.newContext({ viewport: { width: 1440, height: 1000 } });
  const page = await context.newPage();
  const reference = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
  page.setDefaultTimeout(10000);
  reference.setDefaultTimeout(10000);
  const errors = [], blocked = [], diffRequests = [];
  page.on('pageerror', error => errors.push(error.message));
  page.on('websocket', socket => socket.on('framesent', ({ payload }) => {
    const frame = JSON.parse(payload);
    if (['reserve', 'execute'].includes(frame.type)) blocked.push(frame.type);
  }));
  await context.route('https://api.github.com/**', route => route.fulfill({ status: 404, body: '{}' }));
  await context.route('**/api/v1/projects/*/git/status', route => route.fulfill({
    json: { isRepository: true, branch: 'clarity-files', repositoryRoot: null, worktreePath: null, files },
  }));
  await context.route('**/api/v1/projects/*/git/diff?**', async route => {
    diffRequests.push(new URL(route.request().url()).searchParams.get('staged'));
    await route.continue();
  });
  const modal = page.locator('#project-files');
  const button = (name, scope = page) => scope.getByRole('button', { name, exact: true });
  const settle = async target => target.evaluate(async () => {
    await document.fonts.ready;
    await Promise.all(document.getAnimations()
      .filter(animation => Number.isFinite(animation.effect.getTiming().iterations))
      .map(animation => animation.finished.catch(() => {})));
    await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
  });
  const compare = async (actualSelector, expectedSelector, properties) => {
    const read = (element, keys) => {
      const style = getComputedStyle(element);
      return Object.fromEntries(keys.map(key => [key, style[key]]));
    };
    assert.deepEqual(
      await page.locator(actualSelector).first().evaluate(read, properties),
      await reference.locator(expectedSelector).first().evaluate(read, properties),
      `${actualSelector} must match ${expectedSelector}`,
    );
  };
  const resize = async (width, height) => {
    await page.setViewportSize({ width, height });
    await reference.setViewportSize({ width, height });
    await page.waitForFunction(width => document.querySelector('.app-shell').dataset.deviceMode === (width < 768 ? 'mobile' : 'desktop'), width);
    await settle(page);
    await settle(reference);
  };
  try {
    await page.goto(UI);
    await button('打开设置').click();
    await button('工作协同', page.getByRole('navigation', { name: '设置分类' })).click();
    await button('连接设备', page.locator('.collaboration-hub .connect-card')).click();
    await page.getByLabel('节点地址', { exact: true }).fill(fixture.url);
    await button('连接节点').click();
    await page.locator('.collaboration-page-heading [role=status]').filter({ hasText: '已连接' }).waitFor();
    await button('返回会话').click();
    const session = page.getByLabel('会话列表', { exact: true }).locator('[data-session-id="dark"] .session-item');
    if (!await session.isVisible()) await page.locator('.project-row').filter({ hasText: 'rpi-work' }).click();
    await session.click();
    await page.locator('.composer-main textarea').fill(DRAFT);
    await button('打开项目文件').click();
    await modal.waitFor();
    await reference.goto(pathToFileURL(path.resolve('design/clarity-workbench/index.html')).href + '#workspace');
    await button('打开项目文件', reference).click();

    for (const [width, height] of [[1920, 1080], [1440, 1000], [1280, 900], [1024, 768], [768, 768], [390, 844], [320, 740], [320, 420]]) {
      await resize(width, height);
      const actual = await modal.boundingBox();
      const expected = await reference.locator('#project-files-panel').boundingBox();
      for (const key of ['x', 'y', 'width', 'height']) near(actual[key], expected[key], `${width}x${height} file drawer ${key}`);
      const composer = await page.locator('.composer-main .composer').boundingBox();
      const referenceComposer = await reference.locator('.composer').first().boundingBox();
      near(composer.x, referenceComposer.x, `${width} file drawer preserves the canonical composer origin`);
      near(composer.width, referenceComposer.width, `${width} file drawer preserves the canonical composer width`);
      for (const [actualSelector, expectedSelector, keys] of [
        ['#project-files', '#project-files-panel', ['padding', 'borderRadius', 'fontSize', 'lineHeight', 'backgroundColor']],
        ['#project-files .modal-header h2', '.project-files-heading strong', ['fontSize', 'fontWeight', 'lineHeight', 'color']],
        ['#project-files .modal-subtitle', '.project-files-heading small', ['fontSize', 'lineHeight', 'color']],
        ['#project-files .project-files-scope', '.project-files-scope', ['padding', 'minHeight', 'gap', 'fontSize', 'lineHeight', 'color']],
        ['#project-files .project-files-section-title', '.project-files-section-title', ['padding', 'fontSize', 'lineHeight']],
        ['#project-files .project-change-row', '.project-changes .project-file-row', ['padding', 'fontSize', 'lineHeight', 'borderRadius', 'minHeight']],
        ['#project-files .project-files-note', '.project-files-note', ['padding', 'gap', 'fontSize', 'lineHeight', 'color']],
      ]) await compare(actualSelector, expectedSelector, keys);
      const close = await modal.locator('.modal-header button').boundingBox();
      assert(close.x >= actual.x && close.x + close.width <= width && close.y >= actual.y && close.y + close.height <= height);
      if (width < 768) assert(close.width >= 44 && close.height >= 44);
      assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
      assert.equal(await modal.locator('.project-files-note .icon').getAttribute('data-lucide'), 'shield-check');
      await page.screenshot({ path: `artifacts/files-clarity-${width}-${height}.png`, animations: 'disabled' });
      await reference.screenshot({ path: `artifacts/files-reference-${width}-${height}.png`, animations: 'disabled' });
      console.log('PASS file drawer geometry, typography, rows and close target', width, height);
    }

    await resize(1440, 1000);
    await modal.locator('.project-file-row').filter({ hasText: 'src' }).first().click();
    await modal.locator('.project-file-tree .project-file-row').filter({ hasText: 'theme.rs' }).waitFor();
    await modal.getByLabel('搜索项目文件', { exact: true }).fill('theme.rs');
    await page.waitForFunction(() => document.querySelectorAll('#project-files .project-file-tree .project-file-row').length === 1);
    assert.equal(await modal.locator('.project-file-tree .project-file-row').count(), 1);
    await modal.getByLabel('搜索项目文件', { exact: true }).fill('');
    await button('折叠所有目录', modal).click();
    await page.waitForFunction(() => document.querySelectorAll('#project-files .project-file-tree .project-file-row').length === 3);
    assert.equal(await modal.locator('.project-file-tree .project-file-row').count(), 3);
    await button('刷新根目录', modal).click();
    await modal.locator('.project-file-row').filter({ hasText: 'README.md' }).waitFor();
    await modal.locator('.project-change-row').first().click();
    await modal.locator('.diff-line').first().waitFor();
    await reference.locator('.project-changes .project-file-row').first().click();
    for (const [width, height] of [[1440, 1000], [1024, 768], [390, 844], [320, 420]]) {
      await resize(width, height);
      const actual = await modal.boundingBox();
      const expected = await reference.locator('#project-files-panel').boundingBox();
      for (const key of ['x', 'y', 'width', 'height']) near(actual[key], expected[key], `${width} diff drawer ${key}`);
      for (const [actualSelector, expectedSelector, keys] of [
        ['#project-files .project-file-open-tabs', '.project-file-tabs', ['margin', 'padding', 'gap']],
        ['#project-files .project-file-open-tab', '.project-file-tab', ['minWidth', 'maxWidth', 'minHeight', 'padding', 'borderRadius', 'gap']],
        ['#project-files .project-file-open-tab span', '.project-file-tab span', ['fontSize']],
        ['#project-files .project-diff-path', '.project-file-diff-path', ['padding', 'gap', 'borderRadius', 'fontSize']],
        ['#project-files .project-diff-path code', '.project-file-diff-path code', ['fontSize', 'lineHeight']],
        ['#project-files .diff-line', '.project-file-diff-code>div', ['padding', 'fontSize', 'lineHeight', 'minWidth']],
        ['#project-files .project-file-diff-note', '.project-file-diff-note', ['margin', 'gap', 'fontSize', 'lineHeight', 'color']],
      ]) await compare(actualSelector, expectedSelector, keys);
      assert(await button('工作区 diff', modal).isVisible());
      assert(await modal.getByLabel('已审阅（仅本地）', { exact: true }).isVisible());
      assert(await modal.evaluate(element => element.scrollWidth <= element.clientWidth + 1));
      if (height <= 420) {
        const line = modal.locator('.diff-line').first();
        await line.scrollIntoViewIfNeeded();
        const row = await line.boundingBox();
        const viewport = await modal.locator('.change-preview').boundingBox();
        assert(row.y >= viewport.y && row.y + row.height <= viewport.y + viewport.height + 1, 'a complete diff row can be read on short screens');
      }
      await page.screenshot({ path: `artifacts/files-diff-clarity-${width}-${height}.png`, animations: 'disabled' });
      console.log('PASS diff drawer, file tabs, path and code typography with retained controls', width, height);
    }
    await resize(1440, 1000);
    await Promise.all([
      page.waitForResponse(response => response.url().includes('/git/diff') && response.url().includes('staged=true')),
      button('已暂存 diff', modal).click(),
    ]);
    assert.equal(await button('已暂存 diff', modal).getAttribute('aria-pressed'), 'true');
    await modal.getByLabel('已审阅（仅本地）', { exact: true }).check();
    await button('返回文件列表', modal).click();
    await modal.locator('.project-change-row').filter({ hasText: 'added.rs' }).click();
    assert.equal(await modal.locator('.project-file-open-tab.active em').textContent(), 'A');
    await button('返回文件列表', modal).click();
    await modal.locator('.project-change-row').filter({ hasText: 'deleted.rs' }).click();
    assert.equal(await modal.locator('.project-file-open-tab.active em').textContent(), 'D');
    assert(diffRequests.includes('true') && diffRequests.includes('false'));
    await page.keyboard.press('Escape');
    await modal.waitFor({ state: 'hidden' });
    assert.equal(await page.locator('.composer-main textarea').inputValue(), DRAFT);
    await reference.locator('.project-files-heading button').click();
    await button('任务上下文', reference).click();
    await button('查看任务上下文').click();
    await page.locator('.inspector.visible').waitFor();
    for (const [width, height] of [[1440, 1000], [1024, 768], [768, 768], [390, 844], [320, 420]]) {
      await resize(width, height);
      const actual = await page.locator('.inspector.visible').boundingBox();
      const expected = await reference.locator('#context-dialog').boundingBox();
      for (const key of ['x', 'y', 'width', 'height']) near(actual[key], expected[key], `${width} context drawer ${key}`);
      const overview = await page.locator('.overview-card').boundingBox();
      const workbench = await page.locator('.tool-program-inspector-card').boundingBox();
      assert(overview.y < workbench.y, 'core runtime information appears before extra tools');
      await page.screenshot({ path: `artifacts/context-clarity-${width}-${height}.png`, animations: 'disabled' });
      console.log('PASS context drawer occupies the canonical viewport column', width, height);
    }
    await button('关闭任务上下文', page.locator('.inspector')).click();
    assert.equal(await page.locator('.composer-main textarea').inputValue(), DRAFT);
    await resize(1440, 1000);
    await button('打开设置').click();
    await button('深色', page.locator('#appearance')).click();
    await button('保存更改', page.locator('#appearance')).click();
    await page.locator('#appearance').getByText('已应用更改。', { exact: true }).waitFor();
    await button('返回会话').click();
    for (const [width, height] of [[1440, 1000], [768, 768], [390, 844], [320, 420]]) {
      await resize(width, height);
      await button('打开项目文件').click();
      await modal.waitFor();
      await settle(page);
      const color = await modal.evaluate(element => getComputedStyle(element).backgroundColor);
      assert(color.match(/\d+/g).slice(0, 3).every(channel => Number(channel) < 100), 'file drawer follows the saved dark theme');
      assert(await modal.evaluate(element => element.scrollWidth <= element.clientWidth + 1));
      await page.screenshot({ path: `artifacts/files-dark-${width}-${height}.png`, animations: 'disabled' });
      await modal.locator('.project-change-row').first().click();
      await modal.locator('.diff-line').first().waitFor();
      assert.equal(await modal.locator('.project-file-diff-note .icon').getAttribute('data-lucide'), 'shield-check');
      assert(await modal.evaluate(element => element.scrollWidth <= element.clientWidth + 1));
      await page.screenshot({ path: `artifacts/files-diff-dark-${width}-${height}.png`, animations: 'disabled' });
      await page.keyboard.press('Escape');
      await modal.waitFor({ state: 'hidden' });
      assert.equal(await page.locator('.composer-main textarea').inputValue(), DRAFT);
      console.log('PASS dark files and diff drawers preserve theme, controls and draft', width, height);
    }
    assert.deepEqual(errors, []);
    assert.deepEqual(blocked, []);
    console.log('PASS search, folders, staged diff, review progress, added/deleted tabs, draft recovery and no model writes');
  } catch (error) {
    await page.screenshot({ path: 'artifacts/project-files-clarity-failure.png', animations: 'disabled' }).catch(() => {});
    throw error;
  } finally {
    await browser.close();
    await fixture.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
