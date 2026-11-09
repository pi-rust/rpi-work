// Formal prebuilt Web/WASM + isolated HTTP/WebSocket fixture; no models, tools or builds.
// After the parent candidate build (never point this at a live/dev service):
// WORK_UI_BUNDLE=target/message-alignment-candidate/parity-web node tests/support/run-static-ui.cjs tests/e2e/verify-message-presentation.cjs
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { setTimeout: delay } = require('node:timers/promises');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');

// Deliberately no default URL: the static harness owns the server and its cleanup.
const WEB = process.env.WORK_UI_URL;
const DRAFT = '消息展示本地回归草稿：不发送、不执行。';
const CODE = [
  '// fixture only: 中文 & < >',
  'const label = "<script>window.__messagePresentationExecuted = true</script>";',
  'let count = 42;',
  'const enabled = true;',
  '\tconsole.log(label, "quote: \\\"", "path: C:\\\\fixture");',
  '',
  '// trailing blank line is intentional',
  '',
  '',
].join('\n');
// Mirror Rust str::lines().join("\n"): normalize CRLF and omit exactly the
// final line terminator, not arbitrary trailing whitespace or all blank lines.
function codeText(source) {
  const lines = source.split('\n');
  if (lines.at(-1) === '') lines.pop();
  return lines.map(line => line.endsWith('\r') ? line.slice(0, -1) : line).join('\n');
}
const MARKDOWN = [
  '## Message presentation / 消息展示',
  '',
  '普通正文中的 [x] 不是任务，`[ ] inline` 也不是。',
  '',
  '- ordinary bullet 保留圆点',
  '- [ ] pending task',
  '- [x] completed task',
  '- [X] uppercase completed',
  '- [x]ordinary is not a task',
  '',
  '<script>window.__messagePresentationExecuted = true</script>',
  '<img src="https://presentation.invalid/escape-probe" onerror="window.__messagePresentationExecuted = true">',
  '[unsafe](javascript:window.__messagePresentationExecuted=true)',
  '',
  '```js',
  CODE.replace(/\n/g, '\r\n') + '```',
  '',
  '最后一段 **Markdown source** 必须原样复制。',
  '',
].join('\r\n');
const UNKNOWN = 'const mystery = "<b>not HTML</b>";\n\t中文 & untouched\n';
const OVER_BUDGET = 'const preserved = "<b>safe</b>";\n' + 'x'.repeat(64 * 1024) + '\n// END-OF-OVER-BUDGET 中文\n';
const messages = [
  { entryId: 'presentation-user', role: 'user', content: '仅本地合成消息；不要发送或调用工具。' },
  { entryId: 'presentation-main', role: 'assistant', content: MARKDOWN },
  { entryId: 'presentation-unknown', role: 'assistant', content: '```not-a-real-language\n' + UNKNOWN + '```' },
  { entryId: 'presentation-budget', role: 'assistant', content: '```js\n' + OVER_BUDGET + '```' },
];
const results = [], errors = [], blocked = [], writes = [], screenshots = [], geometry = [], clipboardWrites = [];
const pass = name => { results.push(name); console.log('PASS', name); };
async function poll(check, label, timeout = 12000) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) { if (await check()) return; await delay(25); }
  throw new Error(`Timed out: ${label}`);
}
function isWrite(record) {
  if (['GET', 'HEAD', 'OPTIONS'].includes(record.method)) return false;
  return !(record.method === 'POST' && (/^\/api\/v1\/sessions\/[^/]+\/open$/.test(record.path)
    || (/^\/api\/v1\/sessions\/[^/]+\/commands$/.test(record.path) && /^get_/.test(record.body?.type || ''))));
}

(async () => {
  fs.mkdirSync('artifacts', { recursive: true });
  let fixture, browser, context, page, fatal;
  const row = id => page.locator(`.transcript [data-history-id="${id}"]`);
  const code = container => container.locator('.conversation-code pre code');
  const codeButton = container => container.locator('.conversation-code .markdown-copy');
  const status = button => button.locator('..').locator('[role="status"]');
  async function success(button, expected, label) {
    await button.scrollIntoViewIfNeeded();
    await button.focus(); // Copy stays available to keyboard users in the code toolbar.
    await button.press('Enter');
    await poll(async () => await button.getAttribute('data-copied') === 'true', `${label} copy succeeds`);
    const actual = await page.evaluate(() => navigator.clipboard.readText());
    const submitted = await page.evaluate(() => window.__presentationClipboardCalls.at(-1));
    const normalize = text => text.replace(/\r\n/g, '\n');
    clipboardWrites.push({ label, expectedLength: expected.length, actualLength: actual.length, exactInput: submitted === expected, normalizedReadback: normalize(actual) === normalize(expected) });
    assert.equal(submitted, expected, `${label}: writeText receives exact source, not rendered text or HTML`);
    // Windows clipboard readback normalizes LF to CRLF; validate API input
    // byte-for-byte above, and OS readback modulo that one documented transform.
    assert.equal(normalize(actual), normalize(expected), `${label}: OS clipboard preserves all source content`);
    assert.equal(await button.getAttribute('aria-busy'), 'false');
    assert.equal(await status(button).textContent(), '已复制');
    assert.equal(await status(button).getAttribute('class'), 'tool-label-hidden', 'success remains visually hidden accessible status');
    assert.equal(await button.locator('svg').getAttribute('data-icon'), 'check');
  }
  async function lossless(container, expected) {
    assert.equal(await code(container).textContent(), expected, 'code source preserved losslessly');
    assert.equal(await code(container).locator('span').evaluateAll(nodes => nodes.map(node => node.textContent).join('')), expected, 'token spans preserve source order and every character');
    assert.equal(await code(container).locator('script, img, b, iframe, a, input').count(), 0, 'code never becomes authored HTML');
    assert.equal(await page.evaluate(() => window.__messagePresentationExecuted), undefined, 'authored HTML/script never executes');
  }
  async function safety() {
    assert.equal(await page.locator('.composer textarea').inputValue(), DRAFT, 'draft is retained; nothing is submitted');
    assert.deepEqual(errors, [], 'no application JavaScript errors');
    assert.deepEqual(blocked, [], 'no attempted external or non-fixture API requests');
    assert.deepEqual(writes, [], 'no attempted mutation requests');
    assert.deepEqual(fixture.requests.filter(isWrite), [], 'fixture saw only reads, open and get_* commands');
  }
  async function layout(mode, width) {
    await page.setViewportSize({ width, height: 1000 });
    const main = row('presentation-main');
    await main.locator('.conversation-code').scrollIntoViewIfNeeded();
    const measurement = await main.evaluate(element => {
      const rect = node => { const r = node.getBoundingClientRect(); return { width: r.width, height: r.height, left: r.left, right: r.right }; };
      const pre = element.querySelector('.conversation-code pre');
      return { viewport: innerWidth, pageWidth: document.documentElement.scrollWidth,
        code: rect(element.querySelector('.conversation-code')), pre: { ...rect(pre), client: pre.clientWidth, scroll: pre.scrollWidth },
        codeButton: rect(element.querySelector('.markdown-copy')),
        markdownButton: !!element.querySelector('.message-copy-markdown'),
        messageActions: !!element.querySelector('.message-actions') };
    });
    geometry.push({ mode, width, ...measurement });
    assert(measurement.pageWidth <= width + 1, `${mode} ${width}: no whole-page horizontal overflow`);
    assert(measurement.code.left >= -1 && measurement.code.right <= width + 1, 'code wrapper fits viewport');
    assert.equal(measurement.markdownButton, false, 'standalone Markdown copy button is removed');
    assert.equal(measurement.messageActions, false, 'message actions container is removed');
    if (width < 768) {
      assert(measurement.codeButton.width >= 44 && measurement.codeButton.height >= 44, 'mobile code copy target is at least 44 × 44px');
    }
    await lossless(main, codeText(CODE));
    await safety();
    const path = `artifacts/message-presentation-${mode}-${width}.png`;
    await page.screenshot({ path, animations: 'disabled' }); screenshots.push(path);
    pass(`${mode} ${width}px: lossless escaped code, bounded layout and accessible copy controls`);
  }
  async function drawer(width) {
    await page.setViewportSize({ width, height: 1000 });
    const menu = page.getByRole('button', { name: '打开项目导航', exact: true });
    const sidebar = page.locator('#project-sidebar');
    await menu.waitFor({ state: 'visible' });
    await poll(() => sidebar.evaluate(el => el.inert), `${width}px closed sidebar is inert`);
    // getByRole selectors can still match inert DOM; inspect Chromium's actual
    // accessibility tree rather than mistaking selector behavior for AX exposure.
    const ax = await context.newCDPSession(page);
    try {
      const tree = await ax.send('Accessibility.getFullAXTree');
      assert.equal(tree.nodes.filter(node => !node.ignored && node.role?.value === 'navigation' && node.name?.value === '项目与会话').length, 0, 'closed drawer is absent from accessibility tree');
    } finally { await ax.detach(); }
    await menu.click();
    await poll(() => sidebar.evaluate(el => !el.inert && el.classList.contains('open')), `${width}px drawer opens`);
    assert(await page.getByRole('navigation', { name: '项目与会话', exact: true }).isVisible());
    const scrim = page.getByRole('button', { name: '关闭项目导航', exact: true });
    const box = await scrim.boundingBox();
    assert(box && box.width > 0, 'drawer close scrim is accessible');
    // The sidebar overlays the left of the full-viewport scrim. Click its exposed right edge.
    await scrim.click({ position: { x: box.width - 8, y: 20 } });
    await poll(() => sidebar.evaluate(el => el.inert && !el.classList.contains('open')), `${width}px scrim closes drawer`);
    await scrim.waitFor({ state: 'hidden' });
    assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1));
    pass(`${width}px: accessible mobile menu opens drawer; scrim closes and restores sidebar inertness`);
  }
  try {
    assert(WEB, 'Use tests/support/run-static-ui.cjs with the freshly built candidate; WORK_UI_URL is required');
    const preview = new URL(WEB);
    assert(['127.0.0.1', 'localhost', '[::1]'].includes(preview.hostname), 'only an explicitly supplied local static preview is allowed');
    fixture = await startFixture(0, { pagedHistory: true, nativeQueries: true });
    fixture.setMessages(messages, 'dark');
    browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
    context = await browser.newContext({ viewport: { width: 1440, height: 1000 }, permissions: ['clipboard-read', 'clipboard-write'] });
    page = await context.newPage(); page.setDefaultTimeout(12000);
    page.on('pageerror', error => errors.push(error.message));
    const fixtureOrigin = new URL(fixture.url).origin;
    await context.route('**/*', async route => {
      const request = route.request(), target = new URL(request.url());
      if (!['http:', 'https:'].includes(target.protocol)) return route.continue();
      let body = {};
      try { body = request.postDataJSON() || {}; } catch {}
      const record = { method: request.method(), path: target.pathname, body };
      if (target.origin === 'https://api.github.com' && ['GET', 'HEAD', 'OPTIONS'].includes(record.method)) {
        return route.fulfill({ status: 404, contentType: 'application/json', body: '{}' });
      }
      const isNode = target.origin === fixtureOrigin, isApp = target.origin === preview.origin;
      if (isWrite(record)) { writes.push({ origin: target.origin, ...record }); return route.abort('blockedbyclient'); }
      if (isApp && !target.pathname.startsWith('/api/') && ['GET', 'HEAD', 'OPTIONS'].includes(record.method)) return route.continue();
      // Only the initial default-node catalog probe may originate at the static UI.
      // Everything else must already target this ephemeral fixture explicitly.
      if (!isNode && !(isApp && target.pathname === '/api/v1/agent-backends' && record.method === 'GET')) {
        blocked.push({ origin: target.origin, ...record }); return route.abort('blockedbyclient');
      }
      const response = await route.fetch({ url: new URL(target.pathname + target.search, fixture.url).href });
      if (target.pathname.endsWith('/snapshot')) {
        const json = await response.json();
        json.events = []; delete json.latestSettlement; // Remove unrelated seeded projection tools/replies.
        return route.fulfill({ response, json });
      }
      return route.fulfill({ response });
    });
    await page.addInitScript(() => {
      localStorage.setItem('rpi-work-appearance-v1', JSON.stringify({ theme: 'codex', mode: 'light', density: 'standard', toolRendering: 'simple', customThemes: [] }));
      window.__presentationClipboardCalls = [];
      const write = navigator.clipboard.writeText.bind(navigator.clipboard);
      Object.defineProperty(navigator.clipboard, 'writeText', { configurable: true, value: async text => {
        window.__presentationClipboardCalls.push(text);
        return write(text);
      } });
    });
    await page.goto(WEB);
    await page.getByRole('button', { name: '打开设置', exact: true }).click();
    await page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '执行节点', exact: true }).click();
    await page.getByRole('button', { name: '连接其他节点', exact: true }).click();
    await page.getByLabel('节点地址', { exact: true }).fill(fixture.url);
    await page.getByRole('button', { name: '连接节点', exact: true }).click();
    await page.locator('.server-details [role="status"]').filter({ hasText: '在线' }).waitFor();
    await page.getByRole('button', { name: '返回会话', exact: true }).click();
    const session = page.locator('[data-session-id="dark"] .session-item');
    if (!await session.isVisible()) await page.locator('.project-row').filter({ hasText: 'rpi-work' }).click();
    await session.click();
    await row('presentation-main').waitFor();
    await code(row('presentation-budget')).waitFor({ state: 'attached' });
    await page.waitForFunction(() => !document.querySelector('.composer textarea')?.disabled);
    await page.locator('.composer textarea').fill(DRAFT);
    const main = row('presentation-main');
    assert.equal(await codeButton(main).getAttribute('aria-label'), '复制代码');
    assert.equal(await main.locator('.message-copy-markdown').count(), 0, 'standalone Markdown copy button is absent');
    assert.equal(await main.locator('.message-actions').count(), 0, 'message actions container is absent');
    await lossless(main, codeText(CODE));
    for (const kind of ['keyword', 'string', 'comment', 'number', 'literal']) {
      assert(await main.locator(`.conversation-code .code-token-${kind}`).count() > 0, `${kind} syntax tokens rendered`);
    }
    const items = main.locator('.conversation-markdown li');
    assert.equal(await items.count(), 5, 'mixed list has five items');
    assert.equal(await items.locator('input[type="checkbox"]').count(), 3, 'only task prefixes become checkboxes');
    for (let index = 0; index < 5; index++) {
      const task = [1, 2, 3].includes(index), item = items.nth(index), input = item.locator('input');
      assert.equal(await input.count(), task ? 1 : 0);
      if (task) { assert(await input.isDisabled()); assert.equal(await input.isChecked(), index !== 1); }
      else assert.notEqual(await item.evaluate(el => getComputedStyle(el).listStyleType), 'none', 'ordinary list item retains its marker');
    }
    assert.equal(await main.locator('script, img, iframe').count(), 0, 'HTML in prose is escaped');
    assert((await main.textContent()).includes('<script>window.__messagePresentationExecuted = true</script>'));
    assert.equal(await main.locator('a[href^="javascript:" i]').count(), 0, 'unsafe link does not become an anchor');
    assert.equal(await main.getByRole('link', { name: 'unsafe', exact: true }).count(), 0);
    await success(codeButton(main), codeText(CODE), 'JavaScript code without final LF');
    for (const [id, source] of [['presentation-unknown', UNKNOWN], ['presentation-budget', OVER_BUDGET]]) {
      const container = row(id);
      await lossless(container, codeText(source));
      assert.equal(await container.locator('.conversation-code [class*="code-token-"]').count(), 0, `${id}: fallback is entirely uncolored, not partially truncated`);
      await success(codeButton(container), codeText(source), id);
    }
    pass('mixed readonly tasks, escaped HTML/unsafe links, syntax tokens, unknown-language and >64KiB lossless plaintext fallbacks');

    // Deterministically reject clipboard writes, then restore the exact descriptor.
    await page.evaluate(() => {
      const clipboard = navigator.clipboard;
      window.__presentationClipboardDescriptor = Object.getOwnPropertyDescriptor(clipboard, 'writeText');
      Object.defineProperty(clipboard, 'writeText', { configurable: true, value: async () => { throw new DOMException('Fixture clipboard denied', 'NotAllowedError'); } });
    });
    try {
      const button = codeButton(main);
      await button.scrollIntoViewIfNeeded(); await button.focus(); await button.press('Enter');
      await poll(async () => await status(button).textContent() === '复制失败，请手动选择内容', 'truthful clipboard failure');
      assert.equal(await button.getAttribute('data-copied'), 'false');
      assert.equal(await button.getAttribute('aria-busy'), 'false');
      assert.equal(await button.locator('svg').getAttribute('data-icon'), 'copy');
      assert.equal(await status(button).getAttribute('class'), 'copy-error');
      assert(await status(button).isVisible(), 'clipboard failure is visible as role=status');
    } finally {
      await page.evaluate(() => {
        const descriptor = window.__presentationClipboardDescriptor;
        if (descriptor) Object.defineProperty(navigator.clipboard, 'writeText', descriptor);
        else delete navigator.clipboard.writeText;
        delete window.__presentationClipboardDescriptor;
      });
    }
    await success(codeButton(main), codeText(CODE), 'restored clipboard');
    pass('code-block clipboard denial never reports success; visible status and copy icon recover after writeText restoration');

    // Start and update one unfinished fence through real fixture WebSocket events, not DOM injection.
    const entryId = 'presentation-stream';
    const live = page.locator('.transcript .agent-message').filter({ hasText: 'streamValue' });
    const first = '```js\nconst streamValue = "<script>window.__messagePresentationExecuted = true';
    const second = '</script>";\n// streamed 中文\nlet value = 7;\n';
    const last = 'const complete = true;\n```\n\n最新 Markdown 完成。';
    fixture.emit({ type: 'agent_start', runId: 'presentation-run' }, 'dark');
    fixture.emit({ type: 'message_start', message: { entryId, role: 'assistant', content: [] } }, 'dark');
    fixture.emit({ type: 'message_update', message: { entryId, role: 'assistant' }, eventType: 'text_delta', delta: first }, 'dark');
    await live.waitFor();
    assert.equal(await live.locator('.message-copy-markdown').count(), 0, 'streaming message has no standalone Markdown copy button');
    assert.equal(await live.locator('.message-actions').count(), 0, 'streaming message has no message actions container');
    await lossless(live, codeText(first.slice('```js\n'.length)));
    await success(codeButton(live), codeText(first.slice('```js\n'.length)), 'unfinished stream code');
    fixture.emit({ type: 'message_update', message: { entryId, role: 'assistant' }, eventType: 'text_delta', delta: second }, 'dark');
    await poll(async () => await code(live).textContent() === codeText((first + second).slice('```js\n'.length)), 'stream code receives latest delta');
    const liveCodeButton = codeButton(live);
    assert.equal(await liveCodeButton.getAttribute('data-copied'), 'false', 'new stream source clears stale copied state');
    assert.equal(await status(liveCodeButton).textContent(), '', 'old copied notice does not describe new text');
    await lossless(live, codeText((first + second).slice('```js\n'.length)));
    await success(liveCodeButton, codeText((first + second).slice('```js\n'.length)), 'updated stream code');
    fixture.emit({ type: 'message_update', message: { entryId, role: 'assistant' }, eventType: 'text_delta', delta: last }, 'dark');
    await live.getByText('最新 Markdown 完成。', { exact: true }).waitFor();
    const finalCode = codeText((first + second).slice('```js\n'.length) + 'const complete = true;\n');
    await lossless(live, finalCode);
    await success(codeButton(live), finalCode, 'completed streamed fence');
    assert.equal(await live.locator('.message-copy-markdown').count(), 0, 'completed stream has no standalone Markdown copy button');
    assert.equal(await live.locator('.message-actions').count(), 0, 'completed stream has no message actions container');
    fixture.emit({ type: 'message_end', message: { entryId, role: 'assistant', content: first + second + last } }, 'dark');
    fixture.emit({ type: 'agent_end', runId: 'presentation-run' }, 'dark');
    fixture.emit({ type: 'agent_settled', runId: 'presentation-run', outcome: 'completed' }, 'dark');
    await row(entryId).waitFor();
    assert.equal(await live.count(), 1, 'settlement does not duplicate streamed message');
    await lossless(row(entryId), finalCode);
    assert.equal(await row(entryId).locator('.message-copy-markdown').count(), 0, 'settled stream has no standalone Markdown copy button');
    assert.equal(await row(entryId).locator('.message-actions').count(), 0, 'settled stream has no message actions container');
    pass('unfinished/final streamed fences remain escaped and span-lossless; code copy state resets while Markdown actions stay absent');

    for (const mode of ['light', 'dark']) {
      await page.setViewportSize({ width: 1440, height: 1000 });
      await page.getByRole('button', { name: '更多操作', exact: true }).click();
      await page.getByRole('menuitem', { name: '外观与主题', exact: true }).click();
      const appearance = page.getByRole('dialog', { name: '外观与主题', exact: true });
      await appearance.getByLabel('明暗模式', { exact: true }).selectOption(mode);
      await appearance.getByRole('button', { name: '保存', exact: true }).click();
      for (const width of [1440, 740, 390, 320]) await layout(mode, width);
    }
    await page.setViewportSize({ width: 1440, height: 1000 });
    await page.getByRole('button', { name: '更多操作', exact: true }).click();
    await page.getByRole('menuitem', { name: '外观与主题', exact: true }).click();
    const paper = page.getByRole('dialog', { name: '外观与主题', exact: true });
    await paper.locator('.theme-preset').filter({ hasText: '纸与苔绿' }).click();
    await paper.getByRole('button', { name: '保存', exact: true }).click();
    await page.waitForFunction(() => document.documentElement.dataset.theme === 'paper');
    await page.setViewportSize({ width: 390, height: 1000 });
    const baseFont = await main.locator('.conversation-markdown').first().evaluate(el => parseFloat(getComputedStyle(el).fontSize));
    await page.locator('.app-shell').evaluate(el => el.style.setProperty('--type-scale', '1.5'));
    try {
      const font = await main.locator('.conversation-markdown').first().evaluate(el => parseFloat(getComputedStyle(el).fontSize));
      assert(font >= baseFont * 1.4, 'non-default theme respects larger type scale');
      assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
      const button = await codeButton(main).boundingBox();
      assert(button.width >= 44 && button.height >= 44);
      await lossless(main, codeText(CODE));
      await safety();
    } finally { await page.locator('.app-shell').evaluate(el => el.style.removeProperty('--type-scale')); }
    pass('Paper theme retains semantic rendering, 1.5× larger prose, 44px copy targets and viewport bounds');
    for (const width of [700, 701, 740, 767]) await drawer(width);
    await page.setViewportSize({ width: 768, height: 1000 });
    await poll(() => page.locator('#project-sidebar').evaluate(el => !el.inert), '768px desktop sidebar is not inert');
    assert.equal(await page.getByRole('button', { name: '打开项目导航', exact: true }).isVisible(), false);
    assert(await page.getByRole('navigation', { name: '项目与会话', exact: true }).isVisible());
    pass('768px desktop menu is hidden and sidebar remains accessible');
    await safety();
  } catch (error) {
    fatal = error.stack || String(error);
    if (page) {
      const path = 'artifacts/message-presentation-failure.png';
      await page.screenshot({ path, animations: 'disabled' }).then(() => screenshots.push(path)).catch(() => {});
    }
    throw error;
  } finally {
    // All resources belong to this test; never connect to or terminate an existing service.
    try { if (context) await context.close(); }
    finally {
      try { if (browser) await browser.close(); }
      finally {
        try { if (fixture) await fixture.close(); }
        finally {
          fs.writeFileSync('artifacts/message-presentation-validation.json', JSON.stringify({
            scope: 'prebuilt formal Web/WASM + synthetic dark-session fixture only; no native desktop/model/tool acceptance',
            results, errors, fatal, blocked, writes, unexpectedWrites: fixture ? fixture.requests.filter(isWrite) : [],
            clipboardWrites, geometry, screenshots, requests: fixture?.requests,
          }, null, 2));
        }
      }
    }
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
