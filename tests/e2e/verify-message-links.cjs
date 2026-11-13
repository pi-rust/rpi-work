// Formal WASM + isolated node fixture; no user files, uploads, shells or editors.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');
const logRoot = 'C:/Users/bigfish/.rpi/worktrees/message-query-logs';
(async () => {
  fs.mkdirSync(logRoot, { recursive: true });
  const fixture = await startFixture(0);
  const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
  const context = await browser.newContext({ viewport: { width: 1440, height: 1000 } });
  const page = await context.newPage(); page.setDefaultTimeout(20000);
  const errors = [], results = [], reads = [];
  const pass = text => { results.push(text); console.log('PASS', text); };
  page.on('pageerror', error => errors.push(error.message));
  page.on('request', req => { if (/\/file\?/.test(req.url())) reads.push(req.url()); });
  await context.route('https://api.github.com/**', route => route.fulfill({ status: 404, body: '{}' }));
  await context.route('https://example.org/**', route => route.fulfill({ body: 'External URL fixture' }));
  const screenshotPath = 'artifacts/web-tool-display-dark-1280-continuous-tools.png';
  const raster = Buffer.from(await page.evaluate(() => {
    const canvas = document.createElement('canvas'); canvas.width = 640; canvas.height = 360;
    const ctx = canvas.getContext('2d'); ctx.fillStyle = '#397ecb'; ctx.fillRect(0, 0, 640, 360);
    return canvas.toDataURL('image/png').split(',')[1];
  }), 'base64');
  let imageReads = 0;
  await page.route('**/api/v1/projects/*/image?**', async route => {
    assert.equal(new URL(route.request().url()).searchParams.get('path'), screenshotPath);
    imageReads++;
    if (imageReads === 1) await route.fulfill({ status: 503, json: { error: 'Transient image read failure' } });
    else await route.fulfill({ contentType: 'image/png', body: raster });
  });
  let releaseSlow;
  const retryReads = new Map();
  await page.route('**/api/v1/projects', async route => {
    const response = await route.fetch(), body = await response.json();
    body.projects.push(...['empty-a', 'empty-b'].map(id => ({ id, name: id, path: `D:/Projects/${id}` })));
    await route.fulfill({ response, json: body });
  });
  await page.route('**/api/v1/projects/empty-*/git/status', async route => {
    const id = new URL(route.request().url()).pathname.split('/')[4];
    // Let scope cleanup run before metadata completes; each project has no sessions.
    await new Promise(resolve => setTimeout(resolve, 150));
    const count = id === 'empty-a' ? 3 : 7;
    await route.fulfill({ json: { isRepository: true, branch: id, repositoryRoot: `D:/Projects/${id}`, worktreePath: `D:/Projects/${id}`,
      files: Array.from({ length: count }, (_, i) => ({ path: `${id}-${i}.rs`, originalPath: null, status: 'M', staged: false, unstaged: true })) } });
  });
  await page.route('**/api/v1/projects/*/file?**', async route => {
    const url = new URL(route.request().url()), file = url.searchParams.get('path');
    if (file === 'src/slow.rs') {
      await new Promise(resolve => { releaseSlow = resolve; });
      await route.fulfill({ contentType: 'application/json', body: JSON.stringify({ path: file, content: 'STALE_PREVIEW_DO_NOT_SHOW' }) }).catch(() => {});
    } else if (file === 'src/main.rs') {
      await route.fulfill({ contentType: 'application/json', body: JSON.stringify({ path: file, content: Array.from({ length: 80 }, (_, i) => `fn line_${i + 1}() {}`).join('\n') }) });
    } else if (file === 'README.md' || file === 'preview.html') {
      const attempt = (retryReads.get(file) || 0) + 1; retryReads.set(file, attempt);
      if (attempt === 1) await route.fulfill({ status: 503, json: { error: 'Transient preview read failure' } });
      else await route.fulfill({ json: { path: file, content: Array.from({ length: 30 }, (_, i) => file.endsWith('.md') ? `# Markdown line ${i + 1}` : `<p>HTML line ${i + 1}</p>`).join('\n') } });
    } else { await route.continue(); }
  });
  const source = [
    '打开 `src/main.rs:12`，或 [源码](D:/Projects/rpi-work/src/main.rs#L12-L20)。',
    '[中文空格](docs/%E4%B8%AD%E6%96%87%20name.rs) [缺失](src/module-2.rs) [慢文件](src/slow.rs)',
    '[网页](https://example.org/authored) https://example.org/a_(b). https://example.org/中文，后续。',
    '`../private.rs` `D:/Projects/pi-rust/private.rs` `\\\\host\\share\\private.rs` [危险](javascript:alert) [双编码](%252e%252e/private.rs)',
    '普通代码 `let value = 1;`，伪网址 evilhttps://example.org/not-a-link',
    '```rust\n// src/main.rs https://example.org/fenced\n```',
    '[Markdown retry](README.md#L12-L15) [HTML retry](preview.html#L8-L10)',
    `截图：\`${screenshotPath}\``, 
  ].join('\n\n');
  fixture.setMessages([{ entryId: 'file-links', role: 'assistant', content: [{ type: 'text', text: source }] }]);
  fixture.setMessages([{ entryId: 'rpc-links', role: 'assistant', content: [{ type: 'text', text: '`src/main.rs:2`' }] }], 'rpc');
  const row = page.locator('[data-history-id="file-links"]');
  const modal = page.locator('#project-files');
  const file = target => row.locator(`[data-file-path="${target}"]`);
  const close = async () => { await modal.getByRole('button', { name: '关闭弹框', exact: true }).click(); await modal.waitFor({ state: 'detached' }); };
  try {
    await page.goto(process.env.WORK_UI_URL || 'http://127.0.0.1:14180/');
    await page.getByRole('button', { name: '打开设置', exact: true }).click();
    await page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '工作协同', exact: true }).click();
    await page.getByRole('button', { name: '连接其他节点', exact: true }).click();
    await page.getByLabel('节点地址', { exact: true }).fill(fixture.url);
    await page.getByRole('button', { name: '连接节点', exact: true }).click();
    await page.getByRole('status').filter({ hasText: '在线' }).first().waitFor();
    await page.getByRole('button', { name: '返回会话', exact: true }).click();
    await page.locator('[data-project-id="rpi-work"] .project-toggle').click();
    await page.locator('[data-session-id="dark"] .session-item').click();
    await row.waitFor(); await file('src/main.rs').first().waitFor();
    assert.equal(reads.length, 0, 'rendering must not read any file');
    assert.equal(await row.locator('.message-file-link').count(), 8);
    assert.equal(imageReads, 0, 'rendering screenshot links must not prefetch images');
    assert.equal(await row.locator('.conversation-code a,.conversation-code pre button,.conversation-code .message-file-link').count(), 0);
    assert.equal(await row.locator('.conversation-code button').count(), 1, 'only the authored copy toolbar may contain a button');
    assert.equal(await row.locator('.conversation-code button').getAttribute('aria-label'), '复制代码');
    assert.equal(await row.locator('a[href*="fenced"],a[href*="not-a-link"]').count(), 0);
    assert.equal(await row.locator('a[href^="javascript:"],a[href^="file:"]').count(), 0);
    pass('render has no file prefetch; unsafe/outside/UNC/traversal targets and fenced code stay inert');

    await file('src/main.rs').first().focus(); await page.keyboard.press('Enter');
    await modal.locator('.project-code-line[data-line="12"]').waitFor();
    assert.equal(reads.length, 1); assert(reads[0].includes('/projects/rpi-work/file?path=src%2Fmain.rs'));
    assert((await modal.locator('[data-line="12"]').getAttribute('style')).includes('background'));
    assert.equal(await modal.locator('[data-line="13"]').getAttribute('style'), '');
    assert.equal(await modal.locator('textarea,input[type="text"]').count(), 0);
    await close();
    await file('src/main.rs').nth(1).click();
    await modal.locator('[data-line="20"]').waitFor();
    assert((await modal.locator('[data-line="20"]').getAttribute('style')).includes('background'));
    assert.equal(await modal.locator('[data-line="21"]').getAttribute('style'), '');
    const lineBox = await modal.locator('[data-line="12"]').boundingBox();
    const previewBox = await modal.locator('.project-preview-content').boundingBox();
    assert(lineBox.y >= previewBox.y && lineBox.y < previewBox.y + previewBox.height);
    await close();
    pass('keyboard inline-code and authored absolute file links read only on click in the correct project, show read-only code and locate/highlight lines');

    await file('docs/中文 name.rs').click(); await modal.locator('.project-code-line').first().waitFor();
    assert.equal(new URL(reads.at(-1)).searchParams.get('path'), 'docs/中文 name.rs'); await close();
    await file('src/module-2.rs').click(); await modal.getByText('文件内容读取失败', { exact: true }).waitFor();
    assert((await modal.textContent()).includes('File deleted in fixture')); await close();
    pass('encoded Chinese/space path is decoded exactly once; API errors stay in the existing preview');

    const anchors = await row.locator('a').evaluateAll(nodes => nodes.map(a => ({ href: a.getAttribute('href'), target: a.target, rel: a.rel })));
    assert.equal(anchors.length, 3);
    assert(anchors.every(a => a.target === '_blank' && a.rel.includes('noopener') && a.rel.includes('noreferrer')));
    assert(anchors.some(a => a.href === 'https://example.org/a_(b)'));
    assert(anchors.some(a => a.href === 'https://example.org/中文'));
    const popupPromise = context.waitForEvent('page'); await row.getByRole('link', { name: '网页', exact: true }).click();
    const popup = await popupPromise; await popup.waitForLoadState();
    assert.equal(popup.url(), 'https://example.org/authored'); assert.equal(await popup.evaluate(() => window.opener), null); await popup.close();
    pass('authored and bare HTTP(S) links retain safe URL boundaries, open a new tab and cannot access opener');

    for (const [target, start, end] of [['README.md', 12, 15], ['preview.html', 8, 10]]) {
      await file(target).click();
      await modal.getByText('文件内容读取失败', { exact: true }).waitFor();
      await modal.getByRole('button', { name: '重试读取文件', exact: true }).click();
      await modal.locator(`.project-code-line[data-line="${end}"]`).waitFor();
      assert.equal(retryReads.get(target), 2);
      assert.equal(await modal.getByRole('button', { name: '源码', exact: true }).getAttribute('class'), 'active');
      for (let line = start; line <= end; line++) assert((await modal.locator(`[data-line="${line}"]`).getAttribute('style')).includes('background'));
      assert.equal(await modal.locator(`[data-line="${start - 1}"]`).getAttribute('style'), '');
      assert.equal(await modal.locator(`[data-line="${end + 1}"]`).getAttribute('style'), '');
      assert.equal(await modal.locator('.project-markdown,.project-html-preview').count(), 0);
      // Selecting another file through the normal tree must still reset mode and range.
      const other = target === 'README.md' ? 'preview.html' : 'README.md';
      retryReads.set(other, 1);
      await modal.getByRole('button', { name: other, exact: true }).click();
      await modal.locator(other.endsWith('.md') ? '.project-markdown' : '.project-html-preview').waitFor();
      assert.equal(await modal.getByRole('button', { name: '预览', exact: true }).getAttribute('class'), 'active');
      await modal.getByRole('button', { name: '源码', exact: true }).click();
      await modal.locator('.project-code-line').first().waitFor();
      assert.equal(await modal.locator('.project-code-line[style*="background"]').count(), 0);
      await close();
      retryReads.delete(other);
    }
    pass('Markdown/HTML failed reads retry in source mode with the clicked line range; selecting another file resets mode and highlights');

    const readsBeforeImage = reads.length;
    await file(screenshotPath).click();
    await modal.getByText('文件内容读取失败', { exact: true }).waitFor();
    await modal.getByRole('button', { name: '重试读取文件', exact: true }).click();
    const projectImage = modal.locator('.conversation-message-image');
    const enlarge = projectImage.getByRole('button', { name: '放大查看项目图片', exact: true });
    await enlarge.waitFor();
    await page.waitForFunction(() => {
      const img = document.querySelector('#project-files .message-image-preview img');
      return img?.complete && img.naturalWidth === 640 && !document.querySelector('#project-files .message-image-heading button').disabled;
    });
    assert.equal(reads.length, readsBeforeImage, 'PNG screenshot must never use the UTF-8 text endpoint');
    assert.equal(imageReads, 2, 'retry remains an authenticated image read');
    assert.equal(await modal.getByRole('button', { name: '源码', exact: true }).count(), 0);
    const blobUrl = await projectImage.locator('img').getAttribute('src');
    assert(blobUrl.startsWith('blob:'), 'no file://, server URL or credential in the rendered image source');
    await enlarge.focus(); await page.keyboard.press('Enter');
    const viewer = page.locator('dialog.message-image-viewer');
    await viewer.waitFor();
    await viewer.getByRole('button', { name: '放大图片', exact: true }).click();
    await viewer.getByRole('button', { name: '关闭图片查看器', exact: true }).click();
    await viewer.waitFor({ state: 'detached' });
    await close();
    assert.equal(await page.evaluate(async url => { try { await fetch(url); return false; } catch { return true; } }, blobUrl), true, 'closing project preview revokes the image Blob');
    pass('screenshot file links use the image API, retry correctly, decode PNG, zoom by keyboard and release Blob URLs on close');

    await file('src/slow.rs').click(); await modal.getByText('正在加载文件内容…', { exact: true }).waitFor();
    assert.equal(typeof releaseSlow, 'function', 'slow read must have started before switching');
    await page.keyboard.press('Escape');
    await modal.waitFor({ state: 'detached' });
    await page.locator('[data-project-id="pi-rust"] .project-toggle').click();
    await page.locator('[data-session-id="rpc"] .session-item').click();
    await page.locator('[data-history-id="rpc-links"] .message-file-link').waitFor();
    releaseSlow?.();
    await page.locator('[data-history-id="rpc-links"] .message-file-link').click();
    await modal.locator('.project-code-line').first().waitFor();
    assert(reads.at(-1).includes('/projects/pi-rust/file?'));
    assert(!(await modal.textContent()).includes('STALE_PREVIEW_DO_NOT_SHOW')); await close();
    pass('switching projects isolates late reads and subsequent clicks use the session project, not the previous project');

    for (const [id, count] of [['empty-a', 3], ['empty-b', 7], ['empty-a', 3]]) {
      const response = page.waitForResponse(response => response.url().includes(`/projects/${id}/git/status`));
      await page.locator(`[data-project-id="${id}"] .project-row`).click();
      await response;
      await page.getByRole('button', { name: '打开项目文件', exact: true }).click();
      const projectFiles = page.getByRole('dialog', { name: '项目文件', exact: true });
      await projectFiles.locator('.project-change-row').first().waitFor();
      await projectFiles.locator('.project-change-row').first().click();
      await page.locator('.change-file').first().waitFor();
      assert.equal(await page.locator('.change-file').count(), count);
      assert((await page.locator('.change-file').first().textContent()).includes(id));
      assert(fixture.requests.some(req => req.path === `/api/v1/projects/${id}/files`), 'metadata directory read must run');
    }
    pass('switching between projects with no resumable session retains each new directory/Git metadata result after scope cleanup');
    assert.deepEqual(errors, []);
    await page.screenshot({ path: path.join(logRoot, 'file-links-work-review-browser.png') });
  } catch (error) {
    errors.push(error.message);
    await page.screenshot({ path: path.join(logRoot, 'file-links-work-review-failure.png') }).catch(() => {});
    throw error;
  } finally {
    releaseSlow?.();
    fs.writeFileSync(path.join(logRoot, 'file-links-work-review-browser.json'), JSON.stringify({ results, errors, reads, retryReads: Object.fromEntries(retryReads), requests: fixture.requests }, null, 2));
    await browser.close(); await fixture.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
