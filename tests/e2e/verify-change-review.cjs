// Real WASM UI, isolated HTTP fixture: Git metadata and diff data semantics.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');
(async () => {
  fs.mkdirSync('artifacts', { recursive: true });
  const fixture = await startFixture(0);
  const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
  const page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
  page.setDefaultTimeout(15000);
  const errors = [], diffs = [];
  page.on('pageerror', e => errors.push(e.message));
  await page.route('https://api.github.com/**', route => route.fulfill({ status: 404, body: '{}' }));
  const file = (path, status, staged, unstaged, originalPath = null) => ({ path, status, staged, unstaged, originalPath });
  let files = [file('src/staged.rs', 'M ', true, false), file('src/both.rs', 'MM', true, true),
    file('src/new.rs', '??', false, true), file('src/deleted.rs', ' D', false, true),
    file('src/renamed.rs', 'R ', true, false, 'src/old.rs'), file('src-other/main.rs', ' M', false, true),
    file('src/conflict.rs', 'UU', true, true)];
  let failStatus = false, repository = true, failDiff = false, releaseSlow, holdStatus = false, releaseStatus;
  await page.route('**/api/v1/projects/*/git/status', async route => {
    const reply = failStatus ? { status: 503, json: { error: 'Git temporarily unavailable' } } :
      { json: { isRepository: repository, branch: 'review-fixture', files, repositoryRoot: null, worktreePath: null } };
    if (holdStatus) {
      holdStatus = false;
      await new Promise(resolve => { releaseStatus = resolve; });
    }
    await route.fulfill(reply).catch(() => {});
  });
  await page.route('**/api/v1/projects/*/git/diff?**', async route => {
    const url = new URL(route.request().url()), path = url.searchParams.get('path'), staged = url.searchParams.get('staged') === 'true';
    diffs.push({ path, staged });
    if (path === 'src/staged.rs' && !staged) throw Error('staged-only file must never request worktree diff');
    if (failDiff) { await route.fulfill({ status: 503, json: { error: 'Diff temporarily unavailable' } }); return; }
    if (path === 'src/new.rs') await new Promise(resolve => { releaseSlow = resolve; });
    const patch = path === 'src/renamed.rs'
      ? 'diff --git a/src/old.rs b/src/renamed.rs\nsimilarity index 100%\nrename from src/old.rs\nrename to src/renamed.rs\n'
      : `diff --git a/${path} b/${path}\n--- a/${path}\n+++ b/${path}\n@@ -1 +1 @@\n--- old header\n+++ ${staged ? 'STAGED' : 'WORKTREE'} ${path}\n`;
    await route.fulfill({ json: { path, patch } }).catch(() => {});
  });
  const panel = page.locator('#changes-panel');
  const row = path => panel.locator(`.change-file[data-change-path="${path}"]`);
  const openMore = async () => page.getByRole('button', { name: '更多操作', exact: true }).click();
  const refresh = async () => {
    await openMore();
    await Promise.all([
      page.waitForResponse(response => response.url().includes('/git/status')),
      page.getByRole('menuitem', { name: '刷新变更', exact: true }).click(),
    ]);
    await page.locator('#changes-panel[aria-busy="false"]').waitFor();
  };
  const pass = text => console.log('PASS', text);
  try {
    await page.goto(process.env.WORK_UI_URL || 'http://127.0.0.1:14180/');
    await page.getByRole('button', { name: '打开设置', exact: true }).click();
    await page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '执行节点', exact: true }).click();
    await page.getByRole('button', { name: '连接其他节点', exact: true }).click();
    await page.getByLabel('节点地址', { exact: true }).fill(fixture.url);
    await page.getByRole('button', { name: '连接节点', exact: true }).click();
    await page.getByRole('status').filter({ hasText: '在线' }).first().waitFor();
    await page.getByRole('button', { name: '返回会话', exact: true }).click();
    await page.locator('[data-project-id="rpi-work"] .project-toggle').click();
    await page.locator('[data-session-id="dark"] .session-item').click();
    const top = page.locator('.app-header .top-actions');
    assert.equal(await top.locator(':scope > button').count(), 3, 'only timeline, context and more remain as header buttons');
    assert.equal(await top.locator('.theme-trigger,.session-settings-toggle').count(), 0);
    await openMore();
    const menu = page.getByRole('menu', { name: '更多操作', exact: true });
    await menu.getByRole('menuitem', { name: '会话设置', exact: true }).click();
    await page.getByRole('dialog', { name: '会话设置', exact: true }).waitFor();
    await page.keyboard.press('Escape');
    await page.getByRole('dialog', { name: '会话设置', exact: true }).waitFor({ state: 'hidden' });
    await openMore();
    await menu.getByRole('menuitem', { name: '外观与主题', exact: true }).click();
    await page.getByRole('dialog', { name: '外观与主题', exact: true }).waitFor();
    await page.keyboard.press('Escape');
    await page.getByRole('dialog', { name: '外观与主题', exact: true }).waitFor({ state: 'hidden' });
    await openMore();
    assert(await menu.getByRole('menuitem', { name: '刷新会话', exact: true }).isVisible());
    assert(await menu.getByRole('menuitem', { name: '重命名', exact: true }).isVisible());
    await page.keyboard.press('ArrowDown');
    assert(await menu.evaluate(el => el.contains(document.activeElement)), 'keyboard focus stays within open menu');
    await page.keyboard.press('Escape');
    await menu.waitFor({ state: 'hidden' });
    pass('compact header preserves settings, appearance, refresh and session actions in one keyboard-accessible menu');
    await page.getByRole('tab', { name: /变更/ }).click();
    assert.equal(await top.locator(':scope > button').count(), 2, 'review header does not expose the unavailable timeline toggle');
    assert.equal(await panel.getByRole('button', { name: '刷新变更', exact: true }).count(), 0, 'refresh is menu-only');
    await row('src/staged.rs').waitFor();
    assert.equal(await panel.locator('.change-file').count(), 7);
    assert.equal(await row('src/new.rs').locator('.file-change-status').textContent(), 'A');
    assert.equal(await row('src/deleted.rs').locator('.file-change-status').textContent(), 'D');
    assert.equal(await row('src/renamed.rs').locator('.file-change-status').textContent(), 'R');
    assert.equal(await row('src/conflict.rs').locator('.file-change-status').textContent(), 'U');
    assert((await row('src/both.rs').textContent()).includes('已暂存 + 工作区变更'));
    await panel.getByLabel('筛选目录', { exact: true }).fill('src');
    assert.equal(await panel.locator('.change-file').count(), 6);
    await panel.getByLabel('筛选变更类型', { exact: true }).selectOption('A');
    assert.equal(await panel.locator('.change-file').count(), 1);
    await panel.getByRole('button', { name: '清除筛选', exact: true }).click();
    assert.equal(await panel.getByLabel('筛选目录', { exact: true }).inputValue(), '');
    assert.equal(await panel.getByLabel('筛选变更类型', { exact: true }).inputValue(), 'all');
    pass('porcelain A/D/R/U classification, mixed-stage labels, directory boundary and type filters');

    const originalFiles = files;
    const longPath = 'apps/very-long-project-name/src/features/conversation/nested-directory/这是一个很长的变更文件名称-with-extra-details.rs';
    files = [...files, file(longPath, ' M', false, true)];
    await refresh();
    const initialMode = await page.evaluate(() => document.documentElement.dataset.mode);
    const colors = new Set();
    for (const mode of ['light', 'dark']) {
      await page.evaluate(mode => { document.documentElement.dataset.mode = mode; }, mode);
      colors.add(await panel.evaluate(el => getComputedStyle(el).backgroundColor));
      for (const width of [1440, 1024, 768, 390, 320]) {
        await page.setViewportSize({ width, height: width < 768 ? 844 : 1000 });
        if (width < 768) await page.waitForFunction(() => document.querySelector('.sidebar')?.getBoundingClientRect().right <= 1);
        const back = panel.getByRole('button', { name: '返回文件列表', exact: true });
        if (await back.isVisible()) await back.click();
        await row(longPath).waitFor();
        const geometry = await panel.evaluate(el => {
          const box = node => { const r = node.getBoundingClientRect(); return { left: r.left, right: r.right, top: r.top, bottom: r.bottom, width: r.width, height: r.height }; };
          return { root: box(el), directory: box(el.querySelector('[aria-label="筛选目录"]')), type: box(el.querySelector('[aria-label="筛选变更类型"]')), browser: box(el.querySelector('.change-browser')) };
        });
        assert(geometry.root.left >= -1 && geometry.root.right <= width + 1, `${mode}/${width}: review fits viewport`);
        assert(Math.abs(geometry.directory.top - geometry.type.top) <= 1, 'filter controls align');
        assert(geometry.directory.right <= geometry.type.left, 'filter controls do not overlap');
        assert(geometry.browser.right <= geometry.root.right + 1);
        if (width < 768) assert(geometry.directory.height >= 44 && geometry.type.height >= 44, 'phone filter touch targets');
        await page.screenshot({ path: `artifacts/change-review-${mode}-${width}-list.png`, animations: 'disabled' });
        await row('src/both.rs').click();
        await panel.getByText('++ WORKTREE src/both.rs', { exact: true }).waitFor();
        const toolbar = await panel.evaluate(el => {
          const a = el.querySelector('.review-diff-switch').getBoundingClientRect();
          const b = el.querySelector('.review-mark-control').getBoundingClientRect();
          return { nonOverlapping: a.right <= b.left + 1 || a.bottom <= b.top + 1, bounded: el.scrollWidth <= el.clientWidth + 1 };
        });
        assert(toolbar.nonOverlapping && toolbar.bounded, `${mode}/${width}: bounded toolbar without overlapping controls`);
        if (width < 768) {
          const header = await page.locator('.app-header').boundingBox();
          assert(header.height <= 56, 'compact phone header stays on one row');
        }
        await page.screenshot({ path: `artifacts/change-review-${mode}-${width}-diff.png`, animations: 'disabled' });
        if (width === 320 || width === 1440) {
          await openMore();
          const box = await menu.boundingBox();
          assert(box.x >= 0 && box.y >= 0 && box.x + box.width <= width + 1, 'more menu stays inside viewport');
          assert(await menu.getByRole('menuitem', { name: '刷新变更', exact: true }).isVisible());
          await page.screenshot({ path: `artifacts/header-more-${mode}-${width}.png`, animations: 'disabled' });
          await page.keyboard.press('Escape');
          await menu.waitFor({ state: 'hidden' });
          assert(await page.getByRole('button', { name: '更多操作', exact: true }).evaluate(el => el === document.activeElement), 'Escape returns focus to More');
        }
      }
    }
    assert.equal(colors.size, 2, 'review inherits distinct light and dark theme surfaces');
    // Long paths retain their full value in an accessible title, not a wide row.
    await panel.getByRole('button', { name: '返回文件列表', exact: true }).click();
    await row(longPath).click();
    await panel.locator('.review-file-title strong').filter({ hasText: longPath }).waitFor();
    assert.equal(await panel.locator('.review-file-title strong').getAttribute('title'), longPath);
    assert(await panel.evaluate(el => el.scrollWidth <= el.clientWidth + 1));
    await page.setViewportSize({ width: 1440, height: 1000 });
    await page.evaluate(mode => { document.documentElement.dataset.mode = mode; }, initialMode);
    files = originalFiles;
    await refresh();
    pass('light/dark 1440/1024/768/390/320 list and diff layouts: aligned filters, touch targets, long paths and non-overlapping controls');

    await row('src/staged.rs').click();
    await panel.getByText('++ STAGED src/staged.rs', { exact: true }).waitFor();
    assert.equal(diffs.at(-1).staged, true);
    assert.equal(await panel.locator('.diff-line').count(), 2);
    assert.equal(await panel.locator('.diff-line.del .line-number').first().textContent(), '1');
    await row('src/both.rs').click();
    await panel.getByText('++ WORKTREE src/both.rs', { exact: true }).waitFor();
    await panel.getByRole('button', { name: '已暂存 diff', exact: true }).click();
    await panel.getByText('++ STAGED src/both.rs', { exact: true }).waitFor();
    await row('src/renamed.rs').click();
    await panel.getByText('重命名文件', { exact: true }).waitFor();
    pass('staged-only defaults to cached diff, mixed files switch bases, header-like source lines and rename-only patches render');

    failDiff = true;
    await row('src/both.rs').click();
    await panel.getByText('Git patch 读取失败', { exact: true }).waitFor();
    failDiff = false;
    await panel.getByRole('button', { name: '重试读取 diff', exact: true }).click();
    await panel.getByText('++ WORKTREE src/both.rs', { exact: true }).waitFor();
    await panel.getByLabel('已审阅（仅本地）', { exact: true }).check();
    await row('src/new.rs').click();
    await page.waitForFunction(() => document.querySelector('.selected-file-heading')?.textContent.includes('src/new.rs'));
    await row('src/both.rs').click();
    await panel.getByText('++ WORKTREE src/both.rs', { exact: true }).waitFor();
    releaseSlow?.();
    await page.waitForTimeout(150);
    assert(!(await panel.locator('.selected-diff-scroll').textContent()).includes('src/new.rs'));
    files = files.slice(0, 2);
    await refresh();
    assert.equal(await panel.locator('.change-file').count(), 2);
    assert((await panel.locator('.review-progress').textContent()).includes('0/2'));
    assert.equal(await panel.locator('.selected-diff-scroll').count(), 0);
    pass('diff retry and late-response isolation; refresh updates counts and resets stale patch/review marks');

    failStatus = true;
    await refresh();
    await panel.getByText('Git 状态读取失败', { exact: true }).waitFor();
    assert.equal(await panel.locator('.change-file').count(), 0);
    failStatus = false; files = [];
    await panel.getByRole('button', { name: '重试读取变更', exact: true }).click();
    await panel.getByText('工作区干净，没有变更。', { exact: true }).waitFor();
    repository = false;
    await refresh();
    await panel.getByText('当前项目不是 Git 仓库。', { exact: true }).waitFor();
    pass('Git errors are retryable and distinct from a clean worktree or a non-Git project');

    repository = true; files = [file('stale-project.rs', ' M', false, true)];
    holdStatus = true;
    await openMore();
    await page.getByRole('menuitem', { name: '刷新变更', exact: true }).click();
    await panel.getByText('正在读取 Git 状态…', { exact: true }).waitFor();
    // Set a filter while metadata is pending; changing project must clear it.
    await panel.getByLabel('搜索变更文件', { exact: true }).fill('stale');
    files = [file('src/both.rs', 'MM', true, true)];
    await page.locator('[data-project-id="pi-rust"] .project-row').click();
    await row('src/both.rs').waitFor();
    assert.equal(await panel.getByLabel('搜索变更文件', { exact: true }).inputValue(), '');
    releaseStatus?.();
    await page.waitForTimeout(150);
    assert.equal(await row('stale-project.rs').count(), 0);
    assert.equal(await panel.locator('.change-file').count(), 1);
    pass('project switching clears filters and isolates delayed metadata from the previous project');
    await page.setViewportSize({ width: 390, height: 844 });
    await row('src/both.rs').click();
    await panel.getByText('++ WORKTREE src/both.rs', { exact: true }).waitFor();
    await panel.getByRole('button', { name: '已暂存 diff', exact: true }).click();
    await panel.getByText('++ STAGED src/both.rs', { exact: true }).waitFor();
    assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
    await page.screenshot({ path: 'artifacts/change-review-mobile-390.png' });
    await panel.getByRole('button', { name: '返回文件列表', exact: true }).click();
    await row('src/both.rs').waitFor();
    pass('390px mobile list/detail, base switching and return preserve a bounded layout');
    assert.deepEqual(errors, []);
  } catch (error) {
    await page.screenshot({ path: 'artifacts/change-review-failure.png' }).catch(() => {});
    throw error;
  } finally { releaseSlow?.(); releaseStatus?.(); await browser.close(); await fixture.close(); }
})().catch(e => { console.error(e); process.exitCode = 1; });
