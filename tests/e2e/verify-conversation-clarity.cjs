// Real production WASM + canonical Clarity reference + an isolated read-only node.
// Build first: tools/dev/build-ui-web.ps1 -DebugBuild
// Screenshots use fixture data; no model calls or changes to a real node.
const assert = require('node:assert/strict');
const http = require('node:http');
const fs = require('node:fs/promises');
const path = require('node:path');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');
const root = path.resolve(__dirname, '../..');
const CODE = 'const escaped = "<script>window.__conversationXss = true</script>"; // ' + 'long-code-'.repeat(100);
const DRAFT = '保留本地草稿，不发送。';
const near = (a, b, label) => assert(Math.abs(a - b) <= 1, `${label}: ${a} != ${b}`);

(async () => {
  const fixture = await startFixture(0);
  fixture.setMessages([
    { entryId: 'clarity-user', role: 'user', content: '重新设计工作台界面，要好看、清晰，也要保留现有功能。' },
    { entryId: 'clarity-answer', role: 'assistant', content: [{ type: 'text', text: '已梳理现有功能，下面是这次界面调整的重点。\n\n## 把工作台，留给真正重要的事。\n\n**更专注的对话**　过程按需展开，模型、思考等级与能力就近配置。\n\n```javascript\n' + CODE + '\n```\n\n| 一 | 二 | 三 | 四 | 五 | 六 | 七 | 八 |\n|---|---|---|---|---|---|---|---|\n| 保留 | 表格 | 内容 | 横向 | 滚动 | 不挤 | 页面 | 边界 |' }] },
    { entryId: 'clarity-tools', role: 'assistant', content: [{ type: 'toolCall', id: 'clarity-read', name: 'read', arguments: { path: 'apps/work-ui/src/app/workspace.rs' } }, { type: 'toolCall', id: 'clarity-check', name: 'bash', arguments: { command: 'cargo check -p work-ui --locked', timeout: 120 } }] },
    { entryId: 'clarity-read-result', role: 'toolResult', toolCallId: 'clarity-read', content: [{ type: 'text', text: 'Isolated fixture file contents.' }] },
    { entryId: 'clarity-check-result', role: 'toolResult', toolCallId: 'clarity-check', isError: true, content: [{ type: 'text', text: 'error: isolated fixture check failure' }] },
  ]);
  const server = http.createServer(async (request, response) => {
    try {
      if (!['GET', 'HEAD'].includes(request.method)) { response.writeHead(405); response.end(); return; }
      const url = new URL(request.url, 'http://localhost');
      const reference = url.pathname.startsWith('/reference/');
      const base = path.join(root, reference ? 'design/clarity-workbench' : 'target/parity-web');
      const relative = decodeURIComponent(url.pathname.slice(reference ? '/reference/'.length : '/app/'.length));
      const file = path.resolve(base, relative || 'index.html');
      if (!file.startsWith(base + path.sep)) { response.writeHead(403); response.end(); return; }
      const content = await fs.readFile(file);
      const types = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.wasm': 'application/wasm' };
      response.writeHead(200, { 'content-type': types[path.extname(file)] || 'application/octet-stream', 'cache-control': 'no-store' });
      response.end(request.method === 'HEAD' ? undefined : content);
    } catch { response.writeHead(404); response.end(); }
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  const origin = `http://127.0.0.1:${server.address().port}`;
  let browser;
  const errors = [], blocked = [];
  try {
    browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
    const context = await browser.newContext({ viewport: { width: 1440, height: 1000 }, permissions: ['clipboard-read', 'clipboard-write'] });
    await context.route('**/*', async route => {
      const request = route.request(), url = new URL(request.url());
      if (!['http:', 'https:'].includes(url.protocol)) return route.continue();
      if (url.origin === 'https://api.github.com') return route.fulfill({ status: 404, body: '{}' });
      const local = [origin, new URL(fixture.url).origin].includes(url.origin);
      let body = {};
      try { body = request.postDataJSON() || {}; } catch { /* reject below */ }
      const read = ['GET', 'HEAD', 'OPTIONS'].includes(request.method()) ||
        (request.method() === 'POST' && (url.pathname.endsWith('/open') || url.pathname.endsWith('/tool-program/snapshot') || body.type?.startsWith('get_')));
      if (!local || !read) { blocked.push(`${request.method()} ${url.href}`); return route.abort('blockedbyclient'); }
      return route.continue();
    });
    const page = await context.newPage(), reference = await context.newPage();
    page.setDefaultTimeout(12000);
    page.on('pageerror', error => errors.push(error.message));
    page.on('websocket', socket => socket.on('framesent', ({ payload }) => {
      const frame = JSON.parse(payload);
      if (['reserve', 'execute'].includes(frame.type) || (frame.type === 'query' && !frame.payload.type?.startsWith('get_'))) blocked.push(`WebSocket ${frame.type}`);
    }));
    await page.goto(origin + '/app/');
    await page.getByRole('button', { name: '打开设置', exact: true }).click();
    const categories = page.getByRole('navigation', { name: '设置分类', exact: true });
    await categories.getByRole('button', { name: '工作协同', exact: true }).click();
    await page.locator('.collaboration-hub .connect-card').getByRole('button', { name: '连接设备', exact: true }).click();
    await page.getByLabel('节点地址', { exact: true }).fill(fixture.url);
    await page.getByRole('button', { name: '连接节点', exact: true }).click();
    await page.locator('.collaboration-page-heading [role=status]').filter({ hasText: '已连接' }).waitFor();
    await page.getByRole('button', { name: '返回会话', exact: true }).click();
    const session = page.getByLabel('会话列表', { exact: true }).locator('[data-session-id=dark] .session-item');
    if (!await session.isVisible()) await page.locator('.project-row').filter({ hasText: 'rpi-work' }).click();
    await session.click();
    await page.locator('[data-history-id=clarity-answer]').waitFor();
    await page.locator('.composer textarea').fill(DRAFT);
    await reference.goto(origin + '/reference/#workspace');
    await reference.locator('#chat-panel').waitFor();
    const setWidth = async width => {
      await page.setViewportSize({ width, height: 1000 });
      await page.waitForFunction(w => document.querySelector('.app-shell')?.dataset.deviceMode === (w < 768 ? 'mobile' : 'desktop'), width);
      await reference.setViewportSize({ width, height: 1000 });
      await page.evaluate(async () => { await document.fonts.ready; await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))); });
    };
    const design = () => reference.evaluate(() => {
      const style = selector => getComputedStyle(document.querySelector(selector));
      const number = (selector, property) => parseFloat(style(selector)[property]);
      return { proseFont: number('.assistant-content>p', 'fontSize'), proseLine: number('.assistant-content>p', 'lineHeight'), headingFont: number('.assistant-content h2', 'fontSize'), userPadding: style('.user-bubble').padding, userRadius: style('.user-bubble').borderRadius, editorPadding: style('.composer').padding, editorHeight: document.querySelector('.composer').getBoundingClientRect().height, editorGutter: number('.composer-wrap', 'paddingLeft'), gutter: number('.transcript', 'paddingLeft'), top: number('.transcript', 'paddingTop') };
    });
    const layout = () => page.evaluate(() => {
      const box = selector => {
        const element = document.querySelector(selector), r = element.getBoundingClientRect(), s = getComputedStyle(element);
        return { x:r.x,y:r.y,width:r.width,height:r.height,right:r.right,bottom:r.bottom,client:element.clientWidth,scroll:element.scrollWidth,font:parseFloat(s.fontSize),line:parseFloat(s.lineHeight),padding:s.padding,radius:s.borderRadius,left:parseFloat(s.paddingLeft),rightPadding:parseFloat(s.paddingRight),top:parseFloat(s.paddingTop),border:parseFloat(s.borderTopWidth),overflow:s.overflowX };
      };
      return { width:innerWidth, pageWidth:document.documentElement.scrollWidth, transcript:box('.transcript'), prose:box('[data-history-id=clarity-answer] .conversation-markdown'), row:box('[data-history-id=clarity-answer]'), heading:box('[data-history-id=clarity-answer] h2'), avatar:box('.assistant-avatar'), user:box('.user-message .message-content'), editor:box('.composer-main>.composer'), editorWrap:box('.composer-main'), prompt:box('.composer textarea'), read:box('[data-tool-id=clarity-read]'), summary:box('[data-tool-id=clarity-read]>summary'), table:box('.conversation-table-scroll'), code:box('.conversation-code pre') };
    });
    const compareStyles = async (actualSelector, expectedSelector, properties, width) => {
      const read = (element, keys) => {
        const style = getComputedStyle(element);
        return Object.fromEntries(keys.map(key => [key, style[key]]));
      };
      const [actual, expected] = await Promise.all([
        page.locator(actualSelector).first().evaluate(read, properties),
        reference.locator(expectedSelector).first().evaluate(read, properties),
      ]);
      assert.deepEqual(actual, expected, `${width}: ${actualSelector} differs from prototype`);
    };
    await setWidth(1440);
    for (const [actual, expected, properties] of [
      ['.sidebar .search-trigger', '.sidebar .search-trigger', ['padding', 'gap', 'fontSize', 'lineHeight', 'backgroundColor', 'color']],
      ['.primary-nav > .nav-item', '.main-nav > .nav-link', ['padding', 'margin', 'gap', 'fontSize', 'fontWeight', 'lineHeight', 'color']],
      ['.sidebar-recent-label', '.sidebar-section-label', ['padding', 'margin', 'fontSize', 'lineHeight', 'color']],
      ['.sidebar-recent .session-item.current', '.sidebar-recent .session.active', ['padding', 'fontSize', 'fontWeight', 'lineHeight', 'borderRadius', 'color']],
      ['.sidebar-recent .session-entry.selected', '.sidebar-recent .session.active', ['backgroundColor', 'borderRadius']],
      ['.sidebar-session-divider', '.sidebar-session-divider', ['margin', 'gap', 'fontSize', 'lineHeight', 'color']],
      ['.quick-session', '.session-trigger', ['padding', 'gap', 'borderRadius']],
      ['.quick-session small', '#session-origin', ['fontSize', 'lineHeight']],
    ]) await compareStyles(actual, expected, properties, 1440);
    near((await page.locator('.sidebar-recent .session-entry').first().boundingBox()).x,
      (await reference.locator('.sidebar-recent .sidebar-session-entry').first().boundingBox()).x, 'recent sessions have the canonical left edge');
    for (const [width, height] of [[1440, 1000], [1024, 768], [768, 768], [390, 844], [320, 740], [320, 420]]) {
      await setWidth(width);
      await page.setViewportSize({ width, height });
      await reference.setViewportSize({ width, height });
      await page.getByRole('button', { name: '切换会话', exact: true }).click();
      await reference.getByRole('button', { name: '切换会话', exact: true }).click();
      await page.locator('#session-switcher').evaluate(async element => {
        await Promise.all(element.getAnimations().map(animation => animation.finished.catch(() => {})));
      });
      const actual = await page.locator('#session-switcher').boundingBox();
      const expected = await reference.locator('#session-dialog').boundingBox();
      for (const axis of ['x', 'y', 'width', 'height']) near(actual[axis], expected[axis], `${width}x${height}: switcher ${axis}`);
      for (const [actualSelector, expectedSelector, properties] of [
        ['#session-switcher', '#session-dialog', ['padding', 'borderRadius']],
        ['#session-switcher h2', '#session-dialog h2', ['fontSize', 'fontWeight', 'lineHeight', 'letterSpacing', 'color']],
        ['.session-switcher-search', '#session-dialog .session-search', ['padding', 'gap', 'borderRadius', 'borderColor']],
        ['.session-switcher-search > .icon', '#session-dialog .session-search > .icon', ['width', 'height']],
        ['.session-switcher-row.current', '#switcher-sessions .session.active', ['padding', 'borderRadius', 'backgroundColor']],
      ]) await compareStyles(actualSelector, expectedSelector, properties, width);
      assert(await page.locator('#session-switcher').evaluate(element => element.scrollWidth <= element.clientWidth), 'switcher content stays inside the panel');
      await page.screenshot({ path: `artifacts/navigation-clarity-${width}-${height}.png`, animations: 'disabled' });
      await reference.screenshot({ path: `artifacts/navigation-reference-${width}-${height}.png`, animations: 'disabled' });
      await page.keyboard.press('Escape');
      await reference.keyboard.press('Escape');
      await page.locator('#session-switcher').waitFor({ state: 'hidden' });
      assert.equal(await page.locator('.composer textarea').inputValue(), DRAFT);
      console.log('PASS sidebar details, session header and prototype switcher geometry', width, height);
    }
    for (const width of [1920, 1440, 1280, 1024, 768, 390, 320]) {
      await setWidth(width);
      const m = await layout(), d = await design();
      assert(m.pageWidth <= width + 1, 'no document horizontal overflow');
      const available = m.transcript.client - m.transcript.left - m.transcript.rightPadding;
      near(m.prose.width, Math.min(900, available), 'prose uses full canonical column');
      near(m.row.width, m.prose.width, 'protocol row and prose share width');
      near(m.prose.x, m.row.x, 'prose shares row left edge');
      near(m.prose.font, d.proseFont, 'reference body text size');
      near(m.prose.line, d.proseLine, 'reference body line height');
      near(m.heading.font, d.headingFont, 'reference heading text size');
      near(m.transcript.top, d.top, 'reference top gutter');
      near(m.user.width, Math.min(640, m.prose.width - (width < 360 ? 12 : width < 768 ? 24 : 0)), 'reference right-aligned user bubble');
      near(m.user.right, m.prose.right, 'user and answer share right edge');
      assert.equal(m.user.padding, d.userPadding);
      assert.equal(m.user.radius, d.userRadius);
      assert.equal(m.editor.padding, d.editorPadding);
      // The native picker may need four more pixels than the reference's static labels.
      assert(Math.abs(m.editor.height - d.editorHeight) <= 5, 'editor has no extra project row or divider');
      near(m.prompt.height, width < 768 ? 48 : 64, 'reference initial input height');
      near(m.avatar.width, 26, 'local brand mark size'); near(m.avatar.height, 26, 'local brand mark size');
      assert.equal(m.read.border, 1, 'simple tools have the reference surface border');
      assert(m.summary.height >= 44, 'tool disclosures retain 44px targets');
      assert(m.code.scroll > m.code.client, 'long code remains horizontally scrollable');
      assert.equal(m.code.overflow, 'auto');
      assert(m.table.width <= m.prose.width + 1, 'table viewport stays within the reading column');
      if (width < 768) assert(m.table.scroll > m.table.client, 'wide table retains its own horizontal scroll');
      assert(m.transcript.scroll <= m.transcript.client + 1, 'code and table overflow cannot widen the transcript');
      if (width >= 768) { near(m.editor.x, m.prose.x, 'desktop editor and prose left edges'); near(m.editor.width, m.prose.width, 'desktop editor and prose widths'); }
      else {
        near(m.editor.x - m.transcript.x, d.editorGutter, 'mobile editor gutter');
        near(m.prose.x - m.transcript.x, d.gutter, 'mobile reading gutter');
        assert.equal(await page.locator('.header-connection-status').isVisible(), false, 'duplicate connection text cannot squeeze the title');
        const switcher = await page.locator('.quick-session').boundingBox();
        const title = await page.locator('.quick-session strong').boundingBox();
        assert(switcher.width >= 64 && title.width >= 44, 'mobile session switcher retains a label alongside the existing timeline control');
        assert.match(await page.locator('.quick-session').getAttribute('title'), /为工作台添加深色模式/, 'full session identity remains available when truncated');
      }
      assert.equal(await page.locator('.composer textarea').inputValue(), DRAFT);
      console.log('PASS Clarity conversation column, typography, bubbles, tools and editor', width);
    }
    const stylePairs = [
      ['.user-message .message-content', '.user-bubble', ['backgroundColor', 'padding', 'borderRadius']],
      ['.conversation-user-text', '.user-bubble', ['fontSize', 'lineHeight', 'color']],
      ['.assistant-message-heading', '.assistant-meta', ['fontSize', 'gap', 'color']],
      ['.assistant-message-heading strong', '.assistant-meta strong', ['fontSize', 'fontWeight', 'color']],
      ['[data-history-id=clarity-answer] .conversation-markdown > p', '.assistant-content > p', ['fontSize', 'lineHeight', 'color']],
      ['[data-history-id=clarity-answer] h2', '.assistant-content h2', ['fontSize', 'lineHeight', 'letterSpacing', 'color']],
      ['.composer-main > .composer', '.composer', ['padding', 'borderRadius', 'borderColor', 'boxShadow']],
      ['.composer-main textarea', '.composer textarea', ['fontSize', 'lineHeight', 'padding', 'color']],
      ['.composer-select.composer-model-picker', '.model-button', ['fontSize', 'lineHeight', 'padding', 'gap', 'color', 'borderRadius']],
      ['.composer-select.composer-thinking-select', '.thinking-button', ['fontSize', 'lineHeight', 'padding', 'gap', 'color', 'borderRadius']],
      ['.composer-select.composer-model-picker .icon', '.model-button > .icon', ['width', 'height']],
      ['.composer-select.composer-thinking-select .icon', '.thinking-button > .icon', ['width', 'height']],
      ['.attachment-button-slot .extras-add', '.attach', ['width', 'height', 'borderRadius', 'borderWidth']],
      ['.attachment-button-slot .extras-add > .icon', '.attach > .icon', ['width', 'height']],
      ['[data-tool-id=clarity-read]', '.tool-process', ['backgroundColor', 'borderColor', 'borderRadius']],
      ['[data-tool-id=clarity-read] > summary', '.tool-process summary', ['fontSize', 'lineHeight', 'padding', 'gap', 'color']],
      ['.quick-session strong', '.workspace-identity h1', ['fontSize', 'fontWeight', 'lineHeight']],
      ['.composer-actions .send-button', '.send-button', ['width', 'height', 'borderRadius']],
      ['.composer-actions .send-button > .icon', '.send-button > .icon', ['width', 'height']],
    ];
    const settleComposers = () => Promise.all([page, reference].map(current =>
      current.locator('.composer').evaluate(async element => {
        await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
        await Promise.all(element.getAnimations({ subtree: true }).map(animation => animation.finished.catch(() => {})));
      }),
    ));
    await reference.locator('.composer textarea').fill(DRAFT);
    for (const width of [1440, 1024, 390, 320]) {
      await setWidth(width);
      await page.locator('.assistant-message-heading').first().click();
      await reference.locator('.assistant-meta').click();
      await page.mouse.move(0, 0);
      await settleComposers();
      for (const [actualSelector, expectedSelector, properties] of stylePairs) {
        const read = (element, keys) => {
          const style = getComputedStyle(element);
          return Object.fromEntries(keys.map(key => [key, style[key]]));
        };
        const [actual, expected] = await Promise.all([
          page.locator(actualSelector).first().evaluate(read, properties),
          reference.locator(expectedSelector).first().evaluate(read, properties),
        ]);
        assert.deepEqual(actual, expected, `${width}: ${actualSelector} style differs from prototype`);
      }
      assert.equal(await page.locator('.composer-thinking-label').textContent(), '思考 · 高');
      assert(await page.locator('.composer-thinking-label').evaluate(element => {
        const text = document.createRange();
        text.selectNodeContents(element);
        return element.getBoundingClientRect().width >= text.getBoundingClientRect().width;
      }), `${width}: thinking level stays fully visible without fractional-pixel ellipsis`);
      for (const text of ['', DRAFT]) {
        await page.locator('.composer textarea').fill(text);
        await reference.locator('.composer textarea').fill(text);
        await settleComposers();
        assert.equal(await page.locator('.composer-actions .send-button').isDisabled(), text === '');
        assert.equal(await reference.locator('.send-button').isDisabled(), text === '');
        assert.equal(
          await page.locator('.composer-actions .send-button').evaluate(element => getComputedStyle(element, '::before').backgroundColor),
          await reference.locator('.send-button').evaluate(element => getComputedStyle(element).backgroundColor),
          'empty and ready send surfaces match the prototype',
        );
      }
      assert.equal(
        await page.locator('.composer').evaluate(element => getComputedStyle(element).boxShadow),
        await reference.locator('.composer').evaluate(element => getComputedStyle(element).boxShadow),
        'focused editor shadow matches the prototype',
      );
      await page.locator('.assistant-message-heading').first().click();
      await reference.locator('.assistant-meta').click();
      await page.locator('.transcript').evaluate(element => { element.scrollTop = 0; });
      await page.screenshot({path: `artifacts/conversation-clarity-${width}.png`, animations: 'disabled'});
      console.log('PASS conversation surfaces, inner typography, editor focus and send states', width);
    }
    for (const width of [1440, 390, 320]) {
      await setWidth(width);
      await page.getByRole('button', { name: '显示时间线', exact: true }).click();
      await page.locator('.conversation-timeline').waitFor();
      const rail = await page.locator('.conversation-timeline').boundingBox();
      const prose = await page.locator('[data-history-id=clarity-answer] .conversation-markdown').boundingBox();
      assert(prose.x >= rail.x + rail.width, 'timeline never covers message text');
      await page.getByRole('button', { name: '隐藏时间线', exact: true }).click();
      assert.equal(await page.locator('.composer textarea').inputValue(), DRAFT);
      console.log('PASS timeline gutter and retained draft', width);
    }
    await setWidth(1440);
    const read = page.locator('[data-tool-id=clarity-read]');
    await read.locator('summary').click();
    await read.getByText('Isolated fixture file contents.', { exact: true }).waitFor();
    await read.locator('summary').click();
    assert.equal(await read.getAttribute('open'), null);
    const failed = page.locator('[data-tool-id=clarity-check]');
    assert.notEqual(await failed.getAttribute('open'), null, 'failures still open automatically');
    await failed.getByText('error: isolated fixture check failure', { exact: true }).waitFor();
    await page.locator('[data-history-id=clarity-answer]').getByRole('button', { name: '复制代码', exact: true }).click();
    assert.equal((await page.evaluate(() => navigator.clipboard.readText())).trimEnd(), CODE);
    assert.equal(await page.evaluate(() => window.__conversationXss), undefined);
    await page.getByRole('button', { name: '选择供应商与模型', exact: true }).click();
    await page.locator('.composer-model-dialog').waitFor();
    await page.keyboard.press('Escape');
    await page.getByRole('button', { name: '打开设置', exact: true }).click();
    await page.locator('#appearance').getByRole('button', { name: '深色', exact: true }).click();
    await page.locator('#appearance').getByRole('button', { name: '保存更改', exact: true }).click();
    await page.getByRole('button', { name: '返回会话', exact: true }).click();
    await page.waitForFunction(() => document.documentElement.dataset.mode === 'dark');
    for (const width of [1440, 390, 320]) {
      await setWidth(width);
      const m = await layout();
      assert(m.pageWidth <= width + 1);
      near(m.prose.width, Math.min(900, m.transcript.client - m.transcript.left - m.transcript.rightPadding), 'dark reading width');
      assert.equal(await page.locator('.composer textarea').inputValue(), DRAFT);
      await page.locator('.transcript').evaluate(element => { element.scrollTop = 0; });
      await page.screenshot({ path: `artifacts/conversation-clarity-dark-${width}.png`, animations: 'disabled' });
      console.log('PASS dark conversation geometry and draft', width);
    }
    assert.deepEqual(blocked, []);
    assert.deepEqual(errors, []);
    console.log('PASS tool disclosure, visible failures, escaped code copy, model picker and theme persistence; no real node writes or JavaScript errors');
  } finally {
    if (browser) await browser.close();
    await fixture.close();
    await new Promise(resolve => server.close(resolve));
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
