// Formal WASM + isolated HTTP/SSE. Raster bytes are fixture data, not uploads.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');
(async () => {
  fs.mkdirSync('artifacts', { recursive: true });
  const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
  const context = await browser.newContext({ viewport: { width: 1440, height: 1000 }, permissions: ['clipboard-read', 'clipboard-write'] });
  const page = await context.newPage(); page.setDefaultTimeout(20000);
  const results = [], errors = [], external = [];
  page.on('pageerror', error => errors.push(error.message));
  await context.route('https://api.github.com/**', route => route.fulfill({ status: 404, body: '{}' }));
  page.on('request', req => {
    if (/^https?:/.test(req.url()) && !/^https?:\/\/(127\.0\.0\.1|localhost)(:|\/)/.test(req.url()) && !req.url().startsWith('https://api.github.com/')) external.push(req.url());
  });
  const samples = await page.evaluate(() => {
    const canvas = document.createElement('canvas'); canvas.width = 640; canvas.height = 360;
    const ctx = canvas.getContext('2d'); ctx.fillStyle = '#397ecb'; ctx.fillRect(0, 0, 640, 360); ctx.fillStyle = '#fff'; ctx.font = '30px sans-serif'; ctx.fillText('Raster fixture', 40, 100);
    return Object.fromEntries(['image/png', 'image/jpeg', 'image/webp'].map(mime => [mime, canvas.toDataURL(mime).split(',')[1]]));
  });
  samples['image/gif'] = 'R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7';
  const image = (mime = 'image/png', data = samples[mime]) => ({ type: 'image', mimeType: mime, data });
  const text = text => ({ type: 'text', text });
  const fixture = await startFixture(0);
  fixture.setMessages([
    { entryId: 'image-user', role: 'user', content: [text('图片之前'), image(), text('图片之后'), image('image/jpeg')] },
    { entryId: 'image-assistant', role: 'assistant', content: [image('image/webp')] },
    { entryId: 'image-call', role: 'assistant', content: [{ type: 'toolCall', id: 'read-image', name: 'read', arguments: { path: 'fixture.gif' } }] },
    { entryId: 'image-result', role: 'toolResult', toolCallId: 'read-image', toolName: 'read', content: [text('图片工具结果'), image('image/gif')] },
    { entryId: 'image-orphan', role: 'toolResult', toolCallId: 'orphan', toolName: 'orphan', content: [image()] },
    { entryId: 'image-private', role: 'assistant', content: [{ type: 'thinking', text: 'PRIVATE_IMAGE_THOUGHT', content: [image()] }, text('公开内容')] },
  ]);
  const panel = page.locator('#conversation-panel');
  const row = id => panel.locator(`[data-history-id="${id}"]`);
  const figures = locator => locator.locator('.conversation-message-image');
  const ready = async locator => page.waitForFunction(id => {
    const host = document.getElementById(id), img = host?.querySelector('img');
    return host && !host.querySelector('button').disabled && img?.complete && img.naturalWidth > 0;
  }, await locator.getAttribute('id'));
  const viewer = page.locator('dialog.message-image-viewer');
  const open = async figure => { await ready(figure); const button = figure.getByRole('button', { name: '放大查看消息图片', exact: true }); await button.focus(); await page.keyboard.press('Enter'); await viewer.waitFor(); await page.waitForFunction(() => parseFloat(document.querySelector('.message-image-viewer img')?.style.width) > 0); return button; };
  const pass = message => { results.push(message); console.log('PASS', message); };
  const revoked = url => page.evaluate(async url => { try { await fetch(url); return false; } catch (_) { return true; } }, url);
  const emit = (id, content, role = 'assistant') => {
    const message = { entryId: id, role, content };
    fixture.emit({ type: 'message_start', message });
    return fixture.emit({ type: 'message_end', message });
  };
  const latest = async () => {
    const jump = page.getByRole('button', { name: '回到最新消息', exact: true });
    if (await jump.isVisible()) {
      await jump.click();
    } else if (!await panel.evaluate(e => e.scrollHeight - e.clientHeight - e.scrollTop < 3)) {
      await panel.hover();
      await page.mouse.wheel(0, 100000);
    }
    // Do not wheel after an explicit Latest click: that creates fresh user
    // reading intent just before incoming image layout changes.
    await page.waitForFunction(() => {
      const e = document.querySelector('#conversation-panel');
      return e.scrollHeight - e.clientHeight - e.scrollTop < 3;
    });
    await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
  };
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
    await row('image-user').waitFor();
    for (const figure of await figures(panel).all()) await ready(figure);
    assert.equal(await figures(panel).count(), 5);
    assert.deepEqual(await row('image-user').locator('.message-content').evaluate(e => [...e.children].filter(e => e.tagName !== 'STYLE').map(e => e.classList.contains('conversation-message-image') ? 'image' : e.textContent)), ['图片之前', 'image', '图片之后', 'image']);
    assert.equal(await figures(row('image-assistant')).count(), 1);
    assert.equal(await figures(row('image-result')).count(), 0, 'linked result appears only inside its call');
    await row('image-call').locator('details.tool-call > summary').click();
    assert.equal(await figures(row('image-call')).count(), 1);
    assert.equal(await row('image-call').locator('.call-output').textContent(), '图片工具结果');
    assert.equal(await panel.getByText('PRIVATE_IMAGE_THOUGHT', { exact: true }).count(), 0);
    assert(!(await panel.textContent()).includes(samples['image/png']));
    pass('history user/assistant and linked/orphan tool images render PNG/JPEG/GIF/WebP once; text/image order and private-content boundaries are preserved');

    const figure = figures(row('image-user')).first();
    const opener = await open(figure);
    assert.equal(await page.getByRole('dialog', { name: '消息图片', exact: true }).count(), 1);
    assert.equal(await viewer.locator('img').getAttribute('src'), await figure.locator('img').getAttribute('src'));
    const fitted = await viewer.locator('img').evaluate(e => e.getBoundingClientRect().width);
    await viewer.getByRole('button', { name: '放大图片', exact: true }).click();
    assert(await viewer.locator('img').evaluate(e => e.getBoundingClientRect().width) > fitted);
    await viewer.getByRole('button', { name: '缩小图片', exact: true }).click();
    await viewer.getByRole('button', { name: '适应窗口', exact: true }).click();
    assert(Math.abs(await viewer.locator('img').evaluate(e => e.getBoundingClientRect().width) - fitted) < 1);
    for (let i = 0; i < 9; i++) { await page.keyboard.press('Tab'); assert(await viewer.evaluate(e => e.contains(document.activeElement))); }
    await page.keyboard.press('Escape'); await viewer.waitFor({ state: 'detached' });
    assert(await opener.evaluate(e => e === document.activeElement));
    await open(figure);
    await viewer.evaluate((dialog, id) => { dialog.close(); window.__rpiMessageImages.openViewer(id); }, await figure.getAttribute('id'));
    assert.equal(await viewer.count(), 1); assert(await viewer.evaluate(e => e.matches(':modal')));
    await viewer.getByRole('button', { name: '关闭图片查看器', exact: true }).click(); await viewer.waitFor({ state: 'detached' });
    assert(await opener.evaluate(e => e === document.activeElement));
    pass('native viewer reuses the inert Blob, supports zoom/fit, contains focus, restores it on close and survives same-task rapid reopen');
    const readingTop = await panel.evaluate(e => e.scrollTop);
    emit('image-reader-paused', [image()]);
    await ready(figures(row('image-reader-paused')).first());
    assert(Math.abs(await panel.evaluate(e => e.scrollTop) - readingTop) <= 2, 'late image dimensions below the reader must not pull them to the bottom');

    await row('image-call').getByRole('button', { name: '复制工具输出' }).click();
    await page.waitForFunction(() => navigator.clipboard.readText().then(text => text === '图片工具结果'));
    await page.locator('.composer textarea').fill('图片不会清空草稿');
    await latest();
    fixture.emit({ type: 'message_start', message: { entryId: 'image-live', role: 'assistant', content: [image('image/jpeg')] } });
    await ready(figures(row('image-live')).first());
    await page.waitForFunction(() => { const e = document.querySelector('#conversation-panel'); return e.scrollHeight - e.clientHeight - e.scrollTop < 3; });
    fixture.emit({ type: 'message_update', eventType: 'text_delta', contentIndex: 1, delta: '实时字幕', message: { entryId: 'image-live', role: 'assistant' } });
    await row('image-live').getByText('实时字幕', { exact: true }).waitFor();
    fixture.emit({ type: 'message_update', eventType: 'text_delta', contentIndex: 1, delta: '继续', message: { entryId: 'image-live', role: 'assistant' } });
    await row('image-live').getByText('实时字幕继续', { exact: true }).waitFor();
    fixture.emit({ type: 'message_end', message: { entryId: 'image-live', role: 'assistant', content: [image('image/jpeg'), text('实时字幕继续')] } });
    assert.equal(await row('image-live').locator('pre').count(), 0);
    fixture.emit({ type: 'tool_execution_start', toolCallId: 'live-image-tool', toolName: 'read', args: { path: 'live.png' } });
    fixture.emit({ type: 'tool_execution_update', toolCallId: 'live-image-tool', partialResult: JSON.stringify({ content: [text('实时图结果'), image()] }) });
    const tool = panel.locator('.conversation-tool').filter({ has: page.locator('[data-tool-id="live-image-tool"]') });
    await tool.locator('details.tool-call > summary').click(); await ready(figures(tool).first());
    fixture.emit({ type: 'tool_execution_end', toolCallId: 'live-image-tool', result: { content: [text('实时图结果'), image()] }, isError: false });
    assert.equal(await tool.locator('.call-output').textContent(), '实时图结果');
    assert(!(await tool.textContent()).includes(samples['image/png']));
    pass('image-only live assistant messages and structured live tool results display without base64 in visible or copyable output');

    await latest();
    const originalUrl = await figures(row('image-live')).first().locator('img').getAttribute('src');
    await open(figures(row('image-live')).first());
    emit('image-live', [image('image/webp')]);
    await viewer.waitFor({ state: 'detached' });
    await ready(figures(row('image-live')).first());
    assert(await revoked(originalUrl));
    assert.notEqual(await figures(row('image-live')).first().locator('img').getAttribute('src'), originalUrl);
    assert(await figures(row('image-live')).first().evaluate(e => e === document.activeElement), 'an image update restores focus to the stable figure, not a pending/disabled opener');

    await page.evaluate(() => {
      const original = window.createImageBitmap;
      window.createImageBitmap = (...args) => {
        window.createImageBitmap = original;
        return original(...args).then(bitmap => new Promise(resolve => {
          window.__heldMessageBitmap = bitmap;
          window.__releaseMessageBitmap = () => resolve(bitmap);
        }));
      };
    });
    emit('image-pending', [image()]);
    await page.waitForFunction(() => !!window.__releaseMessageBitmap);
    const pending = figures(row('image-pending')).first();
    const staleUrl = await pending.locator('img').getAttribute('src');
    emit('image-pending', [image('image/jpeg')]);
    await ready(pending);
    const replacedUrl = await pending.locator('img').getAttribute('src');
    assert(await revoked(staleUrl));
    await page.evaluate(() => window.__releaseMessageBitmap());
    await page.waitForFunction(() => window.__heldMessageBitmap.width === 0);
    assert.equal(await pending.locator('img').getAttribute('src'), replacedUrl);
    await page.evaluate(() => { delete window.__heldMessageBitmap; delete window.__releaseMessageBitmap; });
    pass('image updates close viewers with usable focus, revoke old URLs, ignore late bitmap results and close stale decoded bitmaps');

    // Browser decoder is never reached for mismatched signatures, active formats,
    // URL inputs or abusive dimensions, even if a MIME claims to be PNG.
    const huge = Buffer.from(samples['image/png'], 'base64'); huge.writeUInt32BE(20000, 16);
    const invalid = [
      image('image/svg+xml', Buffer.from('<svg onload="window.__imageExecuted=true"/>').toString('base64')),
      image('image/png', Buffer.from('<svg onload="window.__imageExecuted=true"/>').toString('base64')),
      image('text/html', Buffer.from('<script>window.__imageExecuted=true</script>').toString('base64')),
      image('image/png', 'https://example.invalid/secret?token=SHOULD_NOT_FETCH'),
      { type: 'image', url: 'https://example.invalid/no-fetch', mimeType: 'image/png' },
      image('image/png', 'data:image/png;base64,' + samples['image/png']),
      image('image/png', 'AA==\n'), image('image/png', 'AB=='), image('image/png', 'aW52YWxpZA=='),
      image('image/png', huge.toString('base64')), image('image/jpeg', samples['image/png']),
    ];
    emit('image-invalid', [text('不支持的图片'), ...invalid]);
    await row('image-invalid').waitFor();
    await page.waitForFunction(n => [...document.querySelectorAll('[data-history-id="image-invalid"] .message-image-status')].filter(e => e.textContent.includes('图片无效')).length === n, invalid.length);
    assert.equal(await row('image-invalid').locator('img[src]').count(), 0);
    assert(!(await row('image-invalid').textContent()).includes('SHOULD_NOT_FETCH'));
    assert.equal(await page.evaluate(() => window.__imageExecuted), undefined);
    assert.deepEqual(external, []);
    pass('SVG/HTML, spoofed MIME, URL-only payloads, invalid base64 and oversized image headers show safe placeholders without fetching or execution');

    // A structurally valid PNG with a broken compressed stream must report an
    // actual decoder error, not leave a ready thumbnail or unbounded spinner.
    const original = Buffer.from(samples['image/png'], 'base64');
    const broken = Buffer.concat([original.subarray(0, 33), Buffer.from('000000004944415435af061e', 'hex'), original.subarray(-12)]);
    emit('image-decode-error', [image('image/png', broken.toString('base64'))]);
    await row('image-decode-error').getByRole('status').filter({ hasText: '图片解码失败' }).waitFor();
    assert.equal(await row('image-decode-error').locator('img[src]').count(), 0);

    for (const width of [390, 320]) {
      await page.setViewportSize({ width, height: 844 });
      assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
      await open(figure);
      assert(await viewer.evaluate(e => e.getBoundingClientRect().width <= innerWidth));
      for (const button of await viewer.locator('button').all()) { const box = await button.boundingBox(); assert(box.width >= 44 && box.height >= 44); }
      await page.screenshot({ path: `artifacts/message-images-${width}.png`, animations: 'disabled' });
      await page.keyboard.press('Escape'); await viewer.waitFor({ state: 'detached' });
    }
    await page.setViewportSize({ width: 1440, height: 1000 });
    const urls = await figures(panel).locator('img[src]').evaluateAll(es => es.map(e => e.src));
    await open(figure);
    await page.locator('#changes-tab').evaluate(e => e.click());
    await viewer.waitFor({ state: 'detached' });
    await page.waitForFunction(() => window.__rpiMessageImages.entries.size === 0);
    for (const url of urls) assert(await revoked(url));
    await page.locator('#conversation-tab').click();
    await ready(figures(row('image-user')).first());
    assert.equal(await page.locator('.composer textarea').inputValue(), '图片不会清空草稿');
    assert.equal(fixture.requests.filter(r => ['prompt', 'resume_pending'].includes(r.body.type)).length, 0);
    assert.deepEqual(errors, []); assert.deepEqual(external, []);
    pass('decoder failures, 390/320px touch layouts, navigation disposal, Blob revocation and draft remount are safe with zero model calls or JS errors');
  } catch (error) {
    await page.screenshot({ path: 'artifacts/message-images-failure.png', animations: 'disabled' });
    results.push({ failure: error.message, statuses: await panel.locator('.message-image-status').allTextContents(), ids: await panel.locator('[data-history-id]').evaluateAll(es => es.map(e => e.dataset.historyId)) }); throw error;
  } finally {
    fs.writeFileSync('artifacts/message-images-validation.json', JSON.stringify({ results, errors, external }, null, 2));
    await browser.close(); await fixture.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
