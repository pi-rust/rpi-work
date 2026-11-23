const assert = require('node:assert/strict');
const path = require('node:path');
const { pathToFileURL } = require('node:url');
const { withQuery } = require('./verify-url.cjs');

async function checkProjectNodes(browser, url) {
  const page = await browser.newPage({ viewport: { width: 1440, height: 940 } });
  const errors = [], requests = [];
  page.on('pageerror', error => errors.push(error.message));
  page.on('request', request => { if (!request.url().startsWith('file:')) requests.push(request.url()); });
  const button = name => page.getByRole('button', { name, exact: true });
  const node = id => page.locator(`.project-node-group[data-project-node="${id}"]`);
  const toggle = id => node(id).locator('.project-node-toggle');
  const names = root => root.locator('[data-project-group]').evaluateAll(items => items.map(item => item.dataset.projectGroup));
  const sessions = () => page.locator('#recent-threads [data-session], #pinned-threads [data-session]').evaluateAll(items => items.map(item => item.dataset.session));
  const mode = async value => {
    await button('设置').click(); await button('常规').click();
    await page.getByLabel('项目列表显示', { exact: true }).selectOption(value);
    await button('关闭设置').click();
  };
  const shot = async name => {
    if (process.env.RPI_WORKBENCH_SHOTS) await page.screenshot({ path: path.join(process.env.RPI_WORKBENCH_SHOTS, name + '.png') });
  };
  try {
    await page.goto(withQuery(url, { runtime: 'ready' }));
    assert.equal(await page.locator('#projects').getAttribute('data-layout'), 'node');
    assert.deepEqual(await names(node('local')), ['rpi-work']);
    assert.deepEqual(await names(node('devbox')), ['pi-rust']);
    assert.equal(await node('devbox').locator('.project-node-demo').textContent(), '演示');
    const threadIds = await sessions();
    const draft = page.locator('#draft');
    await draft.fill('节点显示切换不改变草稿');
    await draft.evaluate(input => input.setSelectionRange(2, 5));
    await page.evaluate(() => {
      window.nodeTestTranscript = document.querySelector('#transcript');
      window.nodeTestFrame = document.querySelector('iframe');
      window.nodeTestScroll = document.querySelector('.conversation').scrollTop;
    });
    await toggle('devbox').focus(); await page.keyboard.press('Enter');
    assert.equal(await toggle('devbox').getAttribute('aria-expanded'), 'false');
    assert.equal(await toggle('devbox').evaluate(item => item === document.activeElement), true);
    await mode('flat');
    assert.equal(await page.locator('.project-node-group').count(), 0);
    assert.equal(await page.locator('#projects > .sidebar-project').count(), 2);
    const localIcon = page.locator('[data-project-group="rpi-work"] .project-node-icon');
    const remoteIcon = page.locator('[data-project-group="pi-rust"] .project-node-icon');
    assert.equal(await page.locator('.project-node-label').count(), 0);
    for (const indicator of [localIcon, remoteIcon]) {
      assert.equal(await indicator.getAttribute('role'), 'img');
      assert.equal(await indicator.locator('svg').count(), 1);
      assert.equal(await indicator.textContent(), '');
    }
    assert.equal(await localIcon.getAttribute('title'), '所属节点：此电脑');
    assert.equal(await remoteIcon.getAttribute('title'), '所属节点：开发主机（演示）');
    assert.equal(await remoteIcon.getAttribute('aria-label'), '所属节点：开发主机（演示）');
    assert.notEqual(await localIcon.innerHTML(), await remoteIcon.innerHTML());
    assert.equal(await button('打开项目 pi-rust').getAttribute('aria-description'), '所属节点：开发主机（演示）');
    assert.match(await button('打开项目 pi-rust').getAttribute('title'), /开发主机（演示）/);
    assert.equal(await draft.inputValue(), '节点显示切换不改变草稿');
    assert.deepEqual(await draft.evaluate(input => [input.selectionStart, input.selectionEnd]), [2, 5]);
    assert.equal(await page.evaluate(() => window.nodeTestTranscript === document.querySelector('#transcript') && window.nodeTestFrame === document.querySelector('iframe')), true);
    assert.equal(await page.evaluate(() => document.querySelector('.conversation').scrollTop === window.nodeTestScroll), true);
    assert.deepEqual(await sessions(), threadIds);
    await shot('projects-flat');
    await page.reload();
    assert.equal(await page.locator('#projects').getAttribute('data-layout'), 'flat');
    await mode('node');
    assert.equal(await toggle('devbox').getAttribute('aria-expanded'), 'false');
    await page.reload();
    assert.equal(await toggle('devbox').getAttribute('aria-expanded'), 'false');
    await toggle('devbox').focus(); await page.keyboard.press('Space');
    assert.equal(await toggle('devbox').getAttribute('aria-expanded'), 'true');
    await shot('projects-by-node');

    // Adding into a collapsed remote node reveals only that node, not chats or peers.
    await toggle('local').click(); await toggle('devbox').click(); await button('最近').click();
    await button('添加项目').click();
    await page.getByLabel('所属节点', { exact: true }).selectOption('devbox');
    assert.equal(await button('选择文件夹').isDisabled(), true);
    assert.match(await page.locator('#project-path-help').textContent(), /不访问远程文件系统/);
    await page.locator('#project-name').fill('remote-ui');
    await page.locator('#project-path').fill('/srv/projects/remote-ui');
    await page.locator('#project-dialog').getByRole('button', { name: '添加项目', exact: true }).click();
    assert.equal(await node('devbox').locator('[data-project-group="remote-ui"]').isVisible(), true);
    assert.equal(await toggle('local').getAttribute('aria-expanded'), 'false');
    assert.equal(await page.locator('#recent-threads').isVisible(), false);
    assert.deepEqual(await sessions(), threadIds);
    await button('remote-ui 项目操作').click();
    await page.getByRole('menuitem', { name: '置顶项目', exact: true }).click();
    assert.deepEqual(await names(node('devbox')), ['remote-ui', 'pi-rust']);
    assert.deepEqual(await page.locator('#projects > .project-node-group').evaluateAll(items => items.map(item => item.dataset.projectNode)), ['local', 'devbox']);
    await page.reload();
    assert.equal(await node('devbox').locator('[data-project-group="remote-ui"]').getAttribute('data-node-id'), 'devbox');
    await button('打开项目 remote-ui').click();
    await toggle('devbox').click(); await button('项目').click();
    assert.equal(await toggle('devbox').getAttribute('aria-expanded'), 'true');
    assert.equal(await button('打开项目 remote-ui').evaluate(item => item === document.activeElement), true);
    assert.equal(await toggle('local').getAttribute('aria-expanded'), 'false');

    // A local chooser returning after a node change cannot write into a remote form.
    await button('添加项目').click();
    assert.equal(await page.getByLabel('所属节点', { exact: true }).inputValue(), 'local');
    assert.equal(await button('选择文件夹').isEnabled(), true);
    await page.evaluate(() => Object.defineProperty(window, 'showDirectoryPicker', { configurable: true, value: () => new Promise(resolve => { window.resolveNodePicker = resolve; }) }));
    await button('选择文件夹').click();
    await page.getByLabel('所属节点', { exact: true }).selectOption('devbox');
    await page.locator('#project-name').fill('kept-name'); await page.locator('#project-path').fill('/srv/kept');
    await page.evaluate(async () => { window.resolveNodePicker({ kind: 'directory', name: 'local-result' }); await Promise.resolve(); });
    assert.equal(await page.locator('#project-name').inputValue(), 'kept-name');
    assert.equal(await page.locator('#project-path').inputValue(), '/srv/kept');
    assert.equal(await button('选择文件夹').isDisabled(), true);
    await page.getByLabel('所属节点', { exact: true }).selectOption('local');
    assert.equal(await button('选择文件夹').isEnabled(), true);
    await button('取消').click();

    // Old records belong to this computer; an unknown identity stays unknown, not local.
    await page.evaluate(() => {
      const [key, raw] = Object.entries(localStorage).find(([, value]) => { try { return Array.isArray(JSON.parse(value)?.projects); } catch { return false; } });
      const saved = JSON.parse(raw);
      saved.projectGrouping = 'invalid';
      saved.projects.push({ name: 'legacy-local', path: '/legacy', branch: 'main' }, { name: 'unknown-project', path: '/remote', branch: 'main', nodeId: 'retired-node' });
      localStorage.setItem(key, JSON.stringify(saved));
      const navigation = JSON.parse(localStorage.getItem('rpi-work-layout-navigation-v1'));
      navigation.collapsedNodes = [];
      localStorage.setItem('rpi-work-layout-navigation-v1', JSON.stringify(navigation));
    });
    await page.reload();
    assert.equal(await page.locator('#projects').getAttribute('data-layout'), 'node');
    assert.equal(await node('local').locator('[data-project-group="legacy-local"]').count(), 1);
    assert.equal(await node('retired-node').locator('[data-project-group="unknown-project"]').count(), 1);
    assert.equal(await node('local').locator('[data-project-group="unknown-project"]').count(), 0);
    await button('设置').click();
    await page.getByLabel('搜索设置', { exact: true }).fill('项目列表显示');
    assert.equal(await page.locator('#settings-search-results button').filter({ hasText: '项目列表显示' }).count(), 1);
    await page.locator('#settings-search-results button').filter({ hasText: '项目列表显示' }).click();
    assert.equal(await page.getByLabel('项目列表显示', { exact: true }).inputValue(), 'node');
    await button('外观').click(); await page.getByLabel('外观', { exact: true }).selectOption('dark');
    await button('关闭设置').click();
    for (const width of [1440, 1280]) {
      await page.setViewportSize({ width, height: 940 });
      assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false);
      assert.equal(await toggle('local').isVisible(), true);
    }
    assert.deepEqual(errors, []); assert.deepEqual(requests, []);
    console.log('Node/flat project settings, persistence, independent collapse, scoped pins, remote path metadata, local chooser isolation, legacy/unknown nodes and unchanged conversations passed.');
  } finally { await page.close(); }
}
module.exports = { checkProjectNodes };
if (require.main === module) {
  const { chromium } = require(process.env.RPI_LAYOUT_PLAYWRIGHT ?? '../../tools/node_modules/playwright');
  (async () => {
    const browser = await chromium.launch();
    try { await checkProjectNodes(browser, pathToFileURL(path.join(__dirname, 'index.html')).href); }
    finally { await browser.close(); }
  })().catch(error => { console.error(error); process.exitCode = 1; });
}
