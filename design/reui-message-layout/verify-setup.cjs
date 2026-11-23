const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { pathToFileURL } = require('node:url');
const { withQuery } = require('./verify-url.cjs');

async function checkSetup(browser, url) {
  url = withQuery(url, { runtime: null });
  const page = await browser.newPage({ viewport: { width: 1440, height: 940 } });
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'rpi-setup-'));
  const projectDir = path.join(root, 'picker-project');
  const runtimeDir = path.join(root, 'rpi-install');
  for (const dir of [projectDir, runtimeDir]) {
    fs.mkdirSync(dir);
    fs.writeFileSync(path.join(dir, 'rpi'), 'test fixture: never execute or read');
  }
  const errors = [];
  const requests = [];
  page.on('pageerror', error => errors.push(error.message));
  page.on('request', request => { if (!request.url().startsWith('file:')) requests.push(request.url()); });
  const dialog = page.locator('#runtime-dialog');
  const button = (name, scope = page) => scope.getByRole('button', { name, exact: true });
  const stage = () => page.locator('#runtime-card').getAttribute('data-state');
  const cache = () => page.evaluate(() => Object.values(localStorage).map(value => { try { return JSON.parse(value); } catch { return null; } }).find(value => value?.demo === true) ?? null);
  const choose = async (trigger, files) => {
    const [picker] = await Promise.all([page.waitForEvent('filechooser'), trigger.click()]);
    await picker.setFiles(files);
  };
  const fallback = () => page.evaluate(() => Object.defineProperty(window, 'showDirectoryPicker', { configurable: true, writable: true, value: undefined }));
  const shot = async name => {
    if (!process.env.RPI_WORKBENCH_SHOTS) return;
    fs.mkdirSync(process.env.RPI_WORKBENCH_SHOTS, { recursive: true });
    await page.screenshot({ path: path.join(process.env.RPI_WORKBENCH_SHOTS, name + '.png') });
  };
  try {
    await page.clock.install();
    await page.goto(withQuery(url, { runtime: 'ready' }));
    assert.equal(await stage(), 'checking');
    assert.equal(await dialog.isVisible(), false);
    await page.clock.runFor(1400);
    assert.equal(await stage(), 'ready');
    assert.equal(await dialog.isVisible(), false, 'Detected rpi must not open startup setup');
    assert.equal(await page.locator('#draft').isEnabled(), true);
    await shot('rpi-startup-ready');

    await page.goto(url);
    assert.equal(await stage(), 'checking');
    assert.equal(await dialog.isVisible(), false);
    const draft = page.locator('#draft');
    await draft.fill('安装引导不应清空草稿');
    await draft.evaluate(input => input.setSelectionRange(2, 5));
    await page.clock.runFor(700);
    assert.equal(await stage(), 'missing');
    assert.equal(await dialog.isVisible(), true, 'Missing rpi must automatically offer setup choices');
    assert.equal(await button('演示下载', dialog).isVisible(), true);
    assert.equal(await button('选择 rpi 程序', dialog).isVisible(), true);
    assert.equal(await button('选择 rpi 目录', dialog).isVisible(), true);
    assert.equal(await button('演示下载', dialog).evaluate(node => node === document.activeElement), true);
    assert.match(await page.locator('#runtime-boundary').textContent(), /不扫描本机、不联网下载、不执行程序/);
    assert.equal(await button('演示验证', dialog).isDisabled(), true);
    await shot('rpi-startup-missing');
    await page.locator('#runtime-install-dir').fill('/chosen/install');
    await button('演示下载', dialog).click();
    assert.equal(await stage(), 'downloading');
    assert.equal(await button('演示下载', dialog).isDisabled(), true);
    await page.clock.runFor(1000);
    assert.equal(await stage(), 'downloaded');
    assert.equal(await cache(), null);
    await button('演示验证', dialog).click();
    await page.clock.runFor(700);
    assert.equal(await stage(), 'ready');
    assert.equal((await cache()).kind, 'download-demo');
    await shot('rpi-setup-ready');
    await button('返回工作台', dialog).click();
    // Native close is asynchronous; restoration runs in its close event.
    await page.waitForFunction(() => !document.querySelector('#runtime-dialog').open && document.querySelector('#draft').selectionStart === 2);
    assert.equal(await draft.inputValue(), '安装引导不应清空草稿');
    assert.deepEqual(await draft.evaluate(input => [input.selectionStart, input.selectionEnd]), [2, 5]);
    assert.equal(await draft.evaluate(node => node === document.activeElement), true);
    await page.reload();
    await page.clock.runFor(700);
    assert.equal(await stage(), 'ready');
    assert.equal(await dialog.isVisible(), false, 'A saved demo installation must enter silently on reload');

    // File picking is real, but application code must not read file contents.
    await page.evaluate(() => { File.prototype.text = File.prototype.arrayBuffer = File.prototype.stream = () => { throw new Error('Unexpected file read'); }; });
    await fallback();
    await page.locator('[data-action="add-project"]').click();
    await choose(button('选择文件夹'), projectDir);
    assert.equal(await page.locator('#project-name').inputValue(), 'picker-project');
    assert.equal(await page.locator('#project-path').inputValue(), 'picker-project/');
    assert.match(await page.locator('#project-path-help').textContent(), /浏览器只提供目录名/);
    await page.locator('#project-name').fill('custom-name');
    await page.evaluate(() => Object.defineProperty(window, 'showDirectoryPicker', { configurable: true, writable: true, value: async () => ({ kind: 'directory', name: 'empty-project', entries() { throw new Error('Unexpected traversal'); } }) }));
    await button('选择文件夹').click();
    assert.equal(await page.locator('#project-name').inputValue(), 'custom-name');
    assert.equal(await page.locator('#project-path').inputValue(), 'empty-project/');
    await page.evaluate(() => { window.showDirectoryPicker = async () => { throw new DOMException('Cancelled', 'AbortError'); }; });
    await button('选择文件夹').click();
    assert.equal(await page.locator('#project-error').isHidden(), true);
    assert.equal(await page.locator('#project-path').inputValue(), 'empty-project/');
    await page.evaluate(() => { window.showDirectoryPicker = async () => { throw new DOMException('Denied', 'SecurityError'); }; });
    await button('选择文件夹').click();
    assert.match(await page.locator('#project-error').textContent(), /已有填写未改变/);
    await button('添加项目', page.locator('#project-dialog')).click();
    assert.equal(await page.locator('#project-picker').inputValue(), 'custom-name');
    assert.equal(await page.locator('#projects [data-project-group="custom-name"]').isVisible(), true);
    await page.locator('[data-action="add-project"]').click();
    await page.evaluate(() => { window.showDirectoryPicker = () => new Promise(resolve => { window.resolveProjectPicker = resolve; }); });
    await button('选择文件夹').click();
    await button('取消', page.locator('#project-dialog')).click();
    await page.locator('[data-action="add-project"]').click();
    await page.locator('#project-name').fill('kept-name');
    await page.locator('#project-path').fill('/kept/project');
    await page.evaluate(async () => { window.resolveProjectPicker({ kind: 'directory', name: 'stale-project' }); await Promise.resolve(); });
    assert.equal(await page.locator('#project-name').inputValue(), 'kept-name');
    assert.equal(await page.locator('#project-path').inputValue(), '/kept/project');
    await button('取消', page.locator('#project-dialog')).click();

    // Reset only this test's demo runtime record, not unrelated app preferences.
    await page.evaluate(() => { for (const [key, value] of Object.entries(localStorage)) { try { if (JSON.parse(value)?.demo === true) localStorage.removeItem(key); } catch {} } });
    await page.reload(); await page.clock.runFor(700);
    assert.equal(await dialog.isVisible(), true);
    await dialog.locator('.runtime-demo-options summary').click();
    await page.locator('#runtime-scenario').selectOption('download-error');
    await button('演示下载', dialog).click(); await page.clock.runFor(1000);
    assert.equal(await stage(), 'error');
    assert.match(await page.locator('#runtime-error').textContent(), /下载失败/);
    assert.equal(await cache(), null);
    await page.locator('#runtime-scenario').selectOption('success');
    await button('演示下载', dialog).click(); await page.clock.runFor(1000);
    assert.equal(await stage(), 'downloaded');
    await page.locator('#runtime-scenario').selectOption('verify-error');
    await button('演示验证', dialog).click(); await page.clock.runFor(700);
    assert.equal(await stage(), 'error');
    assert.match(await page.locator('#runtime-error').textContent(), /验证失败/);
    assert.equal(await cache(), null);
    await page.locator('#runtime-scenario').selectOption('success');
    await button('演示下载', dialog).click();
    await page.keyboard.press('Escape');
    await page.clock.runFor(1500);
    assert.equal(await stage(), 'missing');
    assert.equal(await cache(), null);

    await button('选择已有 rpi').click();
    await page.locator('#runtime-location').fill('/kept/rpi');
    await choose(button('选择 rpi 程序', dialog), { name: 'README.md', mimeType: 'text/plain', buffer: Buffer.from('do not read') });
    assert.equal(await page.locator('#runtime-location').inputValue(), '/kept/rpi');
    assert.match(await page.locator('#runtime-error').textContent(), /rpi 或 rpi.exe/);
    await choose(button('选择 rpi 程序', dialog), { name: 'rpi.exe', mimeType: 'application/octet-stream', buffer: Buffer.from('never execute') });
    assert.equal(await page.locator('#runtime-location').inputValue(), 'rpi.exe');
    await button('演示验证', dialog).click(); await page.clock.runFor(700);
    assert.equal((await cache()).kind, 'file-name');
    await button('返回工作台', dialog).click();
    await button('选择已有 rpi').click();
    await fallback();
    await choose(button('选择 rpi 目录', dialog), runtimeDir);
    assert.equal(await page.locator('#runtime-location').inputValue(), 'rpi-install/');
    await button('演示验证', dialog).click(); await page.clock.runFor(700);
    assert.equal((await cache()).kind, 'directory-name');
    await button('返回工作台', dialog).click();
    await page.reload(); await page.clock.runFor(700);
    assert.equal(await stage(), 'ready');
    assert.equal((await cache()).label, 'rpi-install/');
    await page.goto(withQuery(url, { runtime: 'missing' })); await page.clock.runFor(700);
    assert.equal(await stage(), 'missing');
    assert.equal(await dialog.isVisible(), true);
    await page.keyboard.press('Escape');
    await page.clock.runFor(1400);
    assert.equal(await dialog.isVisible(), false, 'Dismissed startup setup must not reopen itself');
    assert.equal(await button('下载引导').isVisible(), true);
    await page.goto(withQuery(url, { runtime: 'ready' })); await page.clock.runFor(700);
    assert.equal(await stage(), 'ready');
    assert.equal(await dialog.isVisible(), false);
    assert.deepEqual(errors, []);
    assert.deepEqual(requests, []);
    console.log('Project directory/native/fallback/cancel/stale picking and rpi startup, demo download/verify failures, cancellation, manual file/directory selection and persistence passed without file reads or network downloads.');
  } finally {
    await page.close();
    fs.rmSync(root, { recursive: true, force: true });
  }
}
module.exports = { checkSetup };
if (require.main === module) {
  const { chromium } = require(process.env.RPI_LAYOUT_PLAYWRIGHT ?? '../../tools/node_modules/playwright');
  (async () => {
    const browser = await chromium.launch();
    try { await checkSetup(browser, pathToFileURL(path.join(__dirname, 'index.html')).href); }
    finally { await browser.close(); }
  })().catch(error => { console.error(error); process.exitCode = 1; });
}
