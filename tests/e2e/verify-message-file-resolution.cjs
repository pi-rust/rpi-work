// Formal Work UI with isolated project files and message links.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');

(async () => {
  const fixture = await startFixture();
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  const entry = (path, kind = 'file') => ({ path, name: path.split('/').at(-1), kind, size: 40 });
  fixture.controls.fileEntriesByPath = {
    '': [entry('src', 'directory'), entry('docs', 'directory')],
    src: [entry('src/deep', 'directory'), entry('src/shared.rs')],
    'src/deep': [entry('src/deep/unique.rs'), entry('src/deep/中文 文件.rs')],
    docs: [entry('docs/shared.rs')],
  };
  fixture.setMessages([{ entryId: 'path-resolution', role: 'assistant', content: [{ type: 'text', text:
    '`src/` `unique.rs#L2-L3` `shared.rs` `absent.rs` `src/deep/中文 文件.rs`\n\n' +
    '`telemetry → ai → tools/runtime` `architecture/ agent/ guides/`\n\n[目录](src/deep)' }] }]);
  const contents = { 'src/deep/unique.rs': 'fn first() {}\nfn second() {}\nfn third() {}\n',
    'src/deep/中文 文件.rs': 'fn unicode_path() {}\n', 'src/shared.rs': 'fn src() {}\n', 'docs/shared.rs': 'fn docs() {}\n' };
  let denied = false;
  await page.route('**/api/v1/projects/*/file?**', route => {
    const path = new URL(route.request().url()).searchParams.get('path');
    if (denied) return route.fulfill({ status: 403, json: { error: 'read denied' } });
    if (path === 'src/deep') return route.fulfill({ status: 400, json: { error: 'select a regular file' } });
    if (Object.hasOwn(contents, path)) return route.fulfill({ json: { path, content: contents[path] } });
    return route.fulfill({ status: 404, json: { error: 'native service returned NotFound: file not found' } });
  });
  await page.route('https://api.github.com/**', route => route.fulfill({ status: 404, json: {} }));
  const modal = page.locator('#project-files');
  const row = page.locator('[data-history-id="path-resolution"]');
  const close = async () => { await modal.getByRole('button', { name: '关闭弹框', exact: true }).click(); };
  try {
    await page.addInitScript(address => {
      localStorage.setItem('rpi-work-nodes-v1', JSON.stringify({version:1,selected:address,recent:[address]}));
    }, fixture.url.replace(/\/$/,''));
    await page.goto(process.env.WORK_UI_URL || 'http://127.0.0.1:14180/');
    await page.locator('[data-project-id="rpi-work"] .project-toggle').click();
    await page.locator('[data-session-id="dark"] .session-item').first().click();
    await row.waitFor();
    assert.equal(await row.locator('.message-file-link').count(), 6);

    await row.locator('[data-file-path="src"]').click();
    await modal.getByRole('button', { name: /deep/ }).waitFor();
    assert.equal(await modal.getByText('文件内容读取失败', { exact: true }).count(), 0);
    await close();
    console.log('PASS directory message links expand the project tree');

    await row.locator('[data-file-path="unique.rs"]').click();
    await modal.locator('.project-preview-title').getByText('src/deep/unique.rs', { exact: true }).waitFor();
    assert((await modal.locator('[data-line="2"]').getAttribute('style')).includes('background'));
    await close();
    console.log('PASS missing abbreviated paths resolve to the unique file and retain line selection');

    await row.locator('[data-file-path="shared.rs"]').click();
    await modal.getByText('请选择要查看的文件', { exact: true }).waitFor();
    await modal.getByRole('button', { name: 'docs/shared.rs', exact: true }).click();
    await modal.locator('.project-preview-title').getByText('docs/shared.rs', { exact: true }).waitFor();
    await close();
    console.log('PASS ambiguous paths require an explicit file choice');

    await row.locator('[data-file-path="absent.rs"]').click();
    await modal.getByText('文件内容读取失败', { exact: true }).waitFor();
    assert((await modal.textContent()).includes('文件已移动、删除'));
    assert(!(await modal.textContent()).includes('native service returned'));
    await close();
    await row.getByRole('button', { name: '目录', exact: true }).click();
    await modal.getByRole('button', { name: 'unique.rs', exact: true }).waitFor();
    assert.equal(await modal.getByText('文件内容读取失败', { exact: true }).count(), 0);
    await close();
    console.log('PASS extensionless directory targets open their contents and missing files show actionable errors');

    denied = true;
    const readsBefore = fixture.requests.filter(request => request.path?.endsWith('/files')).length;
    await row.locator('[data-file-path="src/deep/中文 文件.rs"]').click();
    await modal.getByText('文件内容读取失败', { exact: true }).waitFor();
    assert((await modal.textContent()).includes('节点拒绝读取'));
    assert(fixture.requests.filter(request => request.path?.endsWith('/files')).length <= readsBefore + 1,
      'access denial must not initiate a project search');
    assert.deepEqual(errors, []);
    fs.mkdirSync('artifacts/message-file-resolution', { recursive: true });
    await page.screenshot({ path: 'artifacts/message-file-resolution/permission-error.png' });
  } catch(error) {
    fs.mkdirSync('artifacts/message-file-resolution', { recursive:true });
    await page.screenshot({path:'artifacts/message-file-resolution/failure.png'});
    console.log((await modal.textContent().catch(()=>'' )).slice(-5000));
    console.log(JSON.stringify(fixture.requests.filter(request=>request.path?.endsWith('/files')).slice(-12)));
    throw error;
  } finally {
    await browser.close();
    await fixture.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
