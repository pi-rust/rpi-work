// Production WASM and an isolated node exercise the restored normal entry paths.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');
const { bridgeRealtimePolicies } = require('../support/realtime-http-policy-bridge.cjs');

(async () => {
  fs.mkdirSync('artifacts', { recursive: true });
  const fixture = await startFixture();
  const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
  const page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
  page.setDefaultTimeout(12000);
  const results = [], errors = [], commands = [];
  const draft = '功能入口修复后保留草稿 😀';
  const button = (name, scope = page) => scope.getByRole('button', { name, exact: true });
  const field = page.locator('.composer-main textarea');
  const more = page.locator('#composer-more');
  const files = page.locator('#project-files');
  const stats = page.locator('#runtime-stats');
  const search = page.locator('#global-search');
  const pass = message => { results.push(message); console.log('PASS', message); };
  const escape = async dialog => {
    await page.keyboard.press('Escape');
    await dialog.waitFor({ state: 'hidden' });
  };
  const withinViewport = async dialog => {
    assert(await dialog.evaluate(element => {
      const rect = element.getBoundingClientRect();
      return rect.x >= -1 && rect.y >= -1 && rect.right <= innerWidth + 1
        && rect.bottom <= innerHeight + 1 && element.scrollWidth <= element.clientWidth + 1;
    }), 'dialog fits the viewport without horizontal clipping');
  };
  page.on('pageerror', error => errors.push(error.message));
  page.on('websocket', socket => socket.on('framesent', ({ payload }) => {
    const frame = JSON.parse(payload);
    if (['reserve', 'execute'].includes(frame.type)) commands.push(frame);
  }));
  await bridgeRealtimePolicies(page, () => [new URL(fixture.url).origin]);
  await page.route('https://api.github.com/**', route => route.fulfill({ status: 404, body: '{}' }));
  await page.route('**/api/v1/projects/*/native-resource-config', route =>
    route.fulfill({ status: 501, json: { error: 'Unsupported by fixture' } }));
  let gitReads = 0;
  await page.route('**/api/v1/projects/*/git/status', route => {
    gitReads++;
    return route.fulfill({ json: {
      isRepository: true, branch: 'feature-entries', repositoryRoot: null, worktreePath: null,
      files: [
        { path: 'src/theme.rs', status: 'MM', staged: true, unstaged: true },
        { path: 'docs/new.md', status: '??', staged: false, unstaged: true },
        { path: 'src/deleted.rs', status: ' D', staged: false, unstaged: true },
      ],
    } });
  });
  try {
    await page.goto(process.env.WORK_UI_URL || 'http://127.0.0.1:14180/');
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
    await field.fill(draft);

    for (const [width, height] of [[1440, 1000], [390, 844], [320, 420]]) {
      await page.setViewportSize({ width, height });
      await page.waitForFunction(width => document.querySelector('.app-shell').dataset.deviceMode === (width < 768 ? 'mobile' : 'desktop'), width);
      await button('打开项目文件').click();
      await button('筛选变更', files).click();
      const query = files.getByLabel('搜索变更文件', { exact: true });
      const directory = files.getByLabel('筛选目录', { exact: true });
      const type = files.getByLabel('筛选变更类型', { exact: true });
      await query.fill('theme');
      assert.equal(await files.locator('.change-file').count(), 1);
      await directory.fill('docs');
      await files.getByText('没有匹配的变更', { exact: true }).waitFor();
      await button('清除筛选', files).click();
      await type.selectOption('A');
      await files.locator('.change-file[data-change-path="docs/new.md"]').waitFor();
      assert.equal(await files.locator('.change-file').count(), 1);
      await button('清除筛选', files).click();
      await files.locator('.change-file[data-change-path="src/theme.rs"]').click();
      await files.locator('.diff-line').first().waitFor();
      await files.getByLabel('已审阅（仅本地）', { exact: true }).check();
      await button('返回文件列表', files).click();
      await files.getByLabel('仅未审阅', { exact: true }).check();
      assert.equal(await files.locator('.change-file').count(), 2);
      assert.equal(await files.locator('.change-file[data-change-path="src/theme.rs"]').count(), 0);
      await withinViewport(files);
      if (height <= 420) {
        const last = files.locator('.change-file').last();
        await last.scrollIntoViewIfNeeded();
        assert(await last.evaluate(element => {
          const row = element.getBoundingClientRect();
          const viewport = element.closest('.change-browser').getBoundingClientRect();
          return row.top >= viewport.top - 1 && row.bottom <= viewport.bottom + 1;
        }), 'short screens can reveal a complete change row');
        await query.scrollIntoViewIfNeeded();
        assert(await query.evaluate(element => {
          const input = element.getBoundingClientRect();
          const viewport = element.closest('.change-browser').getBoundingClientRect();
          return input.top >= viewport.top - 1 && input.bottom <= viewport.bottom + 1
            && getComputedStyle(element.closest('.change-browser')).overflowY === 'auto';
        }), 'search remains reachable through the scrollable browser');
      }
      await page.screenshot({ path: `artifacts/feature-entries-filters-${width}.png` });
      const before = gitReads;
      await button('刷新变更', files).click();
      await files.locator('.change-file[data-change-path="src/theme.rs"]').waitFor();
      assert(gitReads > before, 'refresh reads Git status and resets local review progress');
      await button('清除筛选', files).click();
      await button('返回项目目录', files).click();
      assert.equal(await files.locator('.project-change-row').count(), 3);
      await escape(files);
      assert.equal(await field.inputValue(), draft);
      pass(`change filters, diff return, review progress, refresh and tree at ${width}×${height}`);

      if (!await page.locator('.inspector.visible').isVisible()) await button('查看任务上下文').click();
      await button('用量与运行统计', page.locator('.inspector')).click();
      await stats.waitFor();
      await button('会话累计', stats).click();
      assert.match(await stats.locator('.metric-grid').innerText(), /32,000/);
      assert.match(await stats.locator('.metric-grid').innerText(), /4,800/);
      assert.match(await stats.locator('.metric-grid').innerText(), /\$0\.0184/);
      await button('上一轮', stats).click();
      assert.match(await stats.locator('.metric-grid').innerText(), /—/);
      await withinViewport(stats);
      await page.screenshot({ path: `artifacts/feature-entries-stats-${width}.png` });
      await escape(stats);
      await button('关闭任务上下文', page.locator('.inspector')).click();
      pass(`usage dialog scope and unknown values at ${width}×${height}`);

      const trigger = button('附件与更多', page.locator('.composer-main'));
      await trigger.click();
      await more.waitFor();
      const chooserPromise = page.waitForEvent('filechooser');
      await button('选择文件', more).click();
      const chooser = await chooserPromise;
      assert(chooser.isMultiple());
      await chooser.setFiles({ name: '功能核对.md', mimeType: 'text/markdown', buffer: Buffer.from('保留当前草稿与附件内容') });
      await page.locator('.draft-attachments .draft-chip').filter({ hasText: '功能核对.md' }).waitFor();
      await withinViewport(more);
      await page.screenshot({ path: `artifacts/feature-entries-menu-${width}.png` });
      await more.locator('.modal-header button').focus();
      await page.keyboard.press('Shift+Tab');
      assert(await more.evaluate(element => element.contains(document.activeElement)), 'focus stays in the menu');
      await more.getByRole('button', { name: /^Skills/ }).click();
      const skills = page.locator('#skill-picker');
      await skills.waitFor();
      await escape(skills);
      assert(await more.isVisible());
      await escape(more);
      assert(await trigger.evaluate(element => element === document.activeElement), 'Escape restores the invoking plus button');
      await trigger.click();
      await button('BTW 旁问', more).click();
      await page.locator('.btw-panel').waitFor();
      await button('关闭旁问面板').click();
      await trigger.click();
      await button('会话设置与 Tools', more).click();
      await page.locator('#session-details').waitFor();
      await escape(page.locator('#session-details'));
      await trigger.click();
      await button('管理能力', more).click();
      await page.locator('#session-details').waitFor();
      await escape(page.locator('#session-details'));
      await trigger.click();
      await button('压缩上下文', more).click();
      await page.locator('#runtime-context').waitFor();
      await escape(page.locator('#runtime-context'));
      assert.equal(await field.inputValue(), draft);
      assert.equal(await page.locator('.draft-attachments .draft-chip').count(), 1);
      await page.locator('.draft-chip').getByRole('button', { name: /移除/ }).click();
      pass(`attachment chooser, Skills, BTW, config, capability and context entries at ${width}×${height}`);

      await field.focus();
      await page.keyboard.press('Control+k');
      await search.waitFor();
      const input = search.getByLabel('搜索项目、目录或会话', { exact: true });
      for (const query of ['完成', '  完成 RPI-WORK 深色 ', 'dark', 'd:\\projects\\rpi-work 深色']) {
        await input.fill(query);
        await search.locator('[data-session-id="dark"]').waitFor();
        assert.equal(await search.getByText('没有匹配的会话', { exact: true }).count(), 0);
      }
      await input.fill('没有这个结果');
      await search.getByText('没有匹配的会话', { exact: true }).waitFor();
      await input.fill('dark');
      fixture.emit({ type: 'agent_start' });
      await search.locator('[data-session-id="dark"]').getByText(/运行中/).waitFor();
      fixture.emit({ type: 'agent_settled', outcome: 'completed', pendingResume: false });
      await search.locator('[data-session-id="dark"]').getByText(/完成/).waitFor();
      assert.equal(await input.inputValue(), 'dark');
      await withinViewport(search);
      await page.screenshot({ path: `artifacts/feature-entries-search-${width}.png` });
      await escape(search);
      assert.equal(await field.inputValue(), draft);
      pass(`global status, multiword, ID, path, live state and Escape at ${width}×${height}`);
    }
    await page.setViewportSize({ width: 1440, height: 1000 });
    fixture.emit({ type: 'agent_start', runId: 'feature-usage' });
    fixture.emit({
      type: 'agent_settled', runId: 'feature-usage', outcome: 'completed',
      usage: { input: 1000, cacheRead: 3000, cacheWrite: 0, output: 250, cost: { total: 0.02 } },
    });
    await button('查看任务上下文').click();
    await button('用量与运行统计', page.locator('.inspector')).click();
    await button('上一轮', stats).click();
    await stats.locator('.metric-grid strong').filter({ hasText: /^4,000$/ }).waitFor();
    assert.match(await stats.locator('.metric-grid').innerText(), /250/);
    assert.match(await stats.locator('.metric-grid').innerText(), /\$0\.02/);
    await button('会话累计', stats).click();
    assert.match(await stats.locator('.metric-grid').innerText(), /32,000/);
    assert.match(await stats.locator('.metric-grid').innerText(), /4,800/);
    assert.match(await stats.locator('.metric-grid').innerText(), /\$0\.0184/);
    await escape(stats);
    assert.equal(await field.inputValue(), draft);
    pass('explicit per-turn usage and authoritative cumulative usage remain independent');
    assert.equal(await page.locator('footer[aria-label="Agent 运行统计"] button').count(), 0);
    assert.deepEqual(commands, [], 'browsing restored entries sends no mutation commands');
    assert.deepEqual(errors, []);
    pass('read-only inspection preserves drafts and emits no mutation commands or JavaScript errors');
    fs.writeFileSync('artifacts/workspace-feature-entries-validation.json', JSON.stringify({ results, errors }, null, 2));
  } catch (error) {
    await page.screenshot({ path: 'artifacts/workspace-feature-entries-failure.png' });
    fs.writeFileSync('artifacts/workspace-feature-entries-validation.json', JSON.stringify({ results, errors, fatal: error.message }, null, 2));
    throw error;
  } finally {
    await browser.close();
    await fixture.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
