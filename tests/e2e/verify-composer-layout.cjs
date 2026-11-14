// Focused composer geometry regression. Default: integrated WASM served separately.
// node tests/e2e/verify-composer-layout.cjs
// --css-only renders a semantic layout fixture with current source CSS, no build/server.
// This mode validates the cascade, NOT Rust rendering, IME or backend integration.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');
const SOURCE = process.argv.includes('--css-only');
const ROOT = path.resolve(__dirname, '../..');
const UI = process.env.WORK_UI_URL || 'http://127.0.0.1:14180/';
const CURRENT = 'deepseek/deepseek-v4-pro';
const LONG = '超长供应商模型名称-without-spaces-'.repeat(25);
const SHORT = '请优化工作区布局，保留草稿 😀';
const MULTILINE = Array.from({ length: 14 }, (_, i) => `${i + 1}. ${SHORT} 代码与长行仍可编辑。`).join('\n');
const WIDTHS = [1440, 1920, 1024, 768, 390, 320];
const css = file => fs.readFileSync(path.join(ROOT, 'apps/work-ui/src', file), 'utf8');
const icon = '<svg class="icon" viewBox="0 0 16 16" aria-hidden="true"><path d="m4 6 4 4 4-4" fill="none" stroke="currentColor"/></svg>';
async function sourceFixture(page) {
  // Keep the production order: theme surfaces, base layout, conversation, picker,
  // attachment and activity. In particular, do not test picker CSS in isolation.
  const files = ['shared/theme/themes.css', 'shared/theme/codex.css', 'shared/theme/codex-components.css',
    'shared/theme/theme-design.css', 'shared/theme/tool-presentation.css', 'shared/theme/styles.css',
    'features/conversation/conversation.css', 'features/conversation/model_picker.css',
    'features/conversation/composer_extras.css', 'features/conversation/activity.css',
    'features/observability/footer_plugins/footer_plugins.css'];
  await page.setContent(`<!doctype html><html data-theme="codex" data-mode="light"><head>
    <meta name="viewport" content="width=device-width,initial-scale=1">
    ${files.map(file => `<style>${css(file)}</style>`).join('')}</head><body>
    <div class="app-shell context-hidden"><aside class="sidebar"></aside><main class="main-column">
    <header class="app-header">会话布局测试</header><div class="workspace"><section class="conversation-panel">
    <div class="transcript"><div class="message-turn" data-message-row><article class="conversation-widget message agent-message">
      <div class="message-heading">Agent</div><div class="message-content"><div class="conversation-markdown">可读正文保持行宽，工具工作区可利用宽屏。</div></div>
    </article></div></div><div class="composer-main"><div class="composer">
      <div class="composer-workspace"><div class="composer-context-line"><div class="composer-workspace-path"><span class="composer-workspace-label">D:/Projects/rpi-work</span></div><div class="composer-workspace-meta"><div class="composer-workspace-branch">codex/ui-parity</div></div></div></div>
      <textarea aria-label="发送给 agent 的消息" rows="2" data-ime-ready="true"></textarea>
      <div class="composer-capabilities"><button>旧快捷栏</button></div>
      <div class="composer-bottom"><div class="composer-options">
        <div class="composer-extras attachment-picker attachment-button-slot"><label class="extras-add" aria-label="添加附件">${icon}<input type="file" aria-label="添加附件"></label></div>
        <div class="composer-model-controls"><button class="composer-select composer-model-picker" aria-label="选择供应商与模型" title="deepseek / ${CURRENT}" data-model-id="${CURRENT}">
          <span class="composer-model-provider">${LONG}</span><span class="composer-model-chevron">${icon}</span></button>
        <button class="composer-select composer-thinking-select" aria-label="思考等级"><span class="composer-thinking-label">思考 · 高</span><span class="composer-model-chevron">${icon}</span></button></div>
      </div><div class="composer-actions"><button class="send-button" aria-label="发送消息">${icon}</button></div></div>
      </div><div class="composer-foot"><span>已连接</span><button class="session-trace-trigger">Trace</button></div>
      <footer class="runtime-footer plugin-footer"><div class="footer-plugin"><span class="footer-item"><span class="footer-value">↑32k ↓4.8k</span></span><span class="footer-item tool-metric"><span class="footer-value">2 tools</span></span></div></footer></div>
    </section></div></main><aside class="inspector"></aside></div></body></html>`);
}
async function integratedFixture(page, fixture) {
  fixture.controls.models = [{ id: CURRENT, name: LONG, provider: 'deepseek', reasoning: true }];
  const allowed = [new URL(UI).origin, new URL(fixture.url).origin];
  await page.route('**/*', async route => {
    const request = route.request(), url = new URL(request.url());
    if (!['http:', 'https:'].includes(url.protocol)) return route.continue();
    if (url.origin === 'https://api.github.com') return route.fulfill({ status: 404, json: {} });
    if (!allowed.includes(url.origin)) return route.abort('blockedbyclient');
    if (url.origin === allowed[1] && request.method() !== 'GET' && request.method() !== 'OPTIONS') {
      const body = request.postDataJSON();
      if (!url.pathname.endsWith('/open') && !url.pathname.endsWith('/tool-program/snapshot') && (!url.pathname.endsWith('/commands') || !body.type?.startsWith('get_'))) {
        throw new Error(`Layout test forbids fixture mutation: ${request.method()} ${url.pathname}`);
      }
    }
    return route.continue();
  });
  await page.addInitScript(() => localStorage.setItem('rpi-work-appearance-v1', JSON.stringify({ theme: 'codex', mode: 'light', density: 'standard', toolRendering: 'simple', customThemes: [] })));
  await page.goto(UI);
  await page.getByRole('button', { name: '打开设置', exact: true }).click();
  await page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '工作协同', exact: true }).click();
  await page.locator('.collaboration-hub .connect-card').getByRole('button', { name: '连接设备', exact: true }).click();
  await page.getByLabel('节点地址', { exact: true }).fill(fixture.url);
  await page.getByRole('button', { name: '连接节点', exact: true }).click();
  await page.locator('.collaboration-page-heading [role=status]').filter({ hasText: '已连接' }).waitFor();
  await page.getByRole('button', { name: '返回会话', exact: true }).click();
  await page.locator('.project-row').filter({ hasText: 'rpi-work' }).click();
  await page.getByLabel('会话列表', { exact: true }).locator('[data-session-id="dark"] .session-item').click();
  await page.waitForFunction(() => {
    const field = document.querySelector('.composer-main textarea');
    return field && !field.disabled && field.dataset.imeReady === 'true';
  });
  await page.waitForFunction(name => document.querySelector('.composer-model-provider')?.textContent === name, LONG);
}
async function geometry(page) {
  return page.locator('.conversation-panel').evaluate(panel => {
    const box = selector => {
      const el = panel.querySelector(selector);
      if (!el) return null;
      const r = el.getBoundingClientRect(), s = getComputedStyle(el);
      return { x: r.x, y: r.y, right: r.right, bottom: r.bottom, width: r.width, height: r.height,
        scroll: el.scrollWidth, client: el.clientWidth, font: parseFloat(s.fontSize),
        max: parseFloat(s.maxWidth),
        visible: !!el.getClientRects().length && s.visibility !== 'hidden' && s.display !== 'none' };
    };
    const selectors = ['.composer-main', '.composer', '.composer-workspace', 'textarea', '.composer-bottom', '.composer-model-picker',
      '.composer-thinking-select', '.send-button', '.attachment-button-slot .extras-add', '.composer-capabilities',
      '.transcript', '.transcript > [data-message-row]', '.composer-model-provider', '.composer-foot', '.runtime-footer'];
    return Object.fromEntries(selectors.map(selector => [selector, box(selector)]));
  });
}
(async () => {
  let browser, fixture, page;
  const errors = [], measurements = [];
  try {
    if (!SOURCE) fixture = await startFixture(0);
    browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
    page = await browser.newPage({ viewport: { width: 1440, height: 900 }, reducedMotion: 'reduce' });
    page.setDefaultTimeout(10000);
    page.on('pageerror', error => errors.push(error.message));
    page.on('websocket', socket => socket.on('framesent', ({ payload }) => {
      const frame = JSON.parse(payload);
      if (['reserve', 'execute'].includes(frame.type) || (frame.type === 'query' && !frame.payload.type?.startsWith('get_'))) {
        errors.push(`Layout test forbids mutating WebSocket frame: ${frame.type}`);
      }
    }));
    if (SOURCE) await sourceFixture(page); else await integratedFixture(page, fixture);
    const field = page.getByRole('textbox', { name: '发送给 agent 的消息', exact: true });
    const controls = ['.composer-model-picker', '.composer-thinking-select', '.send-button', '.attachment-button-slot .extras-add'];
    for (const mode of ['light', 'dark']) {
      if (SOURCE) await page.locator('html').evaluate((root, value) => root.dataset.mode = value, mode);
      else {
        await page.setViewportSize({ width: 1440, height: 900 });
        await page.getByRole('button', { name: '打开设置', exact: true }).click();
        const theme = page.locator('#appearance.appearance-inline');
        await theme.getByRole('button', { name: mode === 'dark' ? '深色' : '浅色', exact: true }).click();
        await theme.getByRole('button', { name: '保存更改', exact: true }).click();
        await theme.locator('.theme-import-notice').filter({ hasText: '已应用更改' }).waitFor();
        await page.getByRole('button', { name: '返回会话', exact: true }).click();
      }
      for (const width of WIDTHS) {
        const sendSize = 44;
        await page.setViewportSize({ width, height: 900 });
        await page.locator('.sidebar').evaluate(async el => { await Promise.all(el.getAnimations().map(a => a.finished.catch(() => {}))); });
        let shortHeight;
        for (const content of ['', SHORT, MULTILINE]) {
          await field.fill(content);
          const g = await geometry(page), label = `${mode}/${width}/${content ? content === SHORT ? 'short' : 'multiline' : 'empty'}`;
          measurements.push({ label, geometry: g });
          const check = (condition, reason) => assert(condition, `${label}: ${reason}\n${JSON.stringify(g)}`);
          check(g.textarea.height >= (width < 768 ? 47.5 : 63.5), 'text entry matches the prototype writing area: 48px mobile / 64px desktop');
          if (!content) check(g.textarea.height <= (width < 768 ? 49 : 65), 'empty input stays compact instead of reserving a tall writing area');
          check(g.textarea.height <= 226, 'long input is bounded at 25dvh');
          if (content === SHORT) shortHeight = g.textarea.height;
          if (content === MULTILINE && await field.evaluate(e => CSS.supports('field-sizing', 'content'))) check(g.textarea.height > shortHeight, 'multiline entry grows before scrolling');
          check(g['.composer-bottom'].height <= 53, 'one compact control row, not an inflated provider bar');
          check(g['.composer'].height <= g.textarea.height + g['.composer-bottom'].height + g['.composer-workspace'].height + 42, 'no spare chrome beyond the project context, input and compact controls');
          check(!g['.composer-capabilities'] || g['.composer-capabilities'].height === 0, 'capability controls leave no extra row in the composer');
          check(g['.composer-model-picker'].height <= 45, 'legacy select heights cannot override picker geometry');
          check(g['.composer-model-provider'].scroll > g['.composer-model-provider'].client, 'long model label truncates');
          check(g['.composer-model-picker'].right <= g['.composer-thinking-select'].x + 1, 'model shrinks before thinking');
          check(g['.composer-thinking-select'].right <= g['.send-button'].x + 1, 'thinking does not overlap send');
          check(g['.composer-bottom'].scroll <= g['.composer-bottom'].client + 1, 'toolbar does not overflow');
          check(g['.transcript'].height >= (content === MULTILINE ? 350 : 450), 'composer leaves a useful conversation viewport even when a long draft expands');
          check(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1), 'no page-wide horizontal scroll');
          check(g['.composer'].width <= g['.composer'].max + 1, 'editor respects the shared workspace cap, not the old 1512px cap');
          check(Math.abs(g['.composer'].width - g['.transcript > [data-message-row]'].width) <= 20, 'conversation and editor share a usable measure (scrollbar tolerance)');
          for (const selector of ['.composer-foot', '.runtime-footer']) {
            check(Math.abs(g[selector].width - g['.composer'].width) <= 1, `${selector} shares composer width`);
            check(Math.abs(g[selector].x - g['.composer'].x) <= 1, `${selector} shares composer center`);
          }
          check(g['.runtime-footer'].height <= 32, 'default tools stay in one compact row');
          check(g['.runtime-footer'].scroll <= g['.runtime-footer'].client + 1, 'default tools fit without horizontal clipping');
          if (width === 1920) check(Math.abs(g['.composer'].width - 900) <= 1 && Math.abs(g['.transcript > [data-message-row]'].width - 900) <= 1, 'wide view preserves the canonical 900px conversation and editor column');
          for (const selector of controls) {
            const r = g[selector];
            check(r.visible && r.x >= 0 && r.right <= width + 1 && r.bottom <= 900, `${selector} remains in viewport`);
            if (width < 768) check(r.width >= 43.5 && r.height >= 43.5, `${selector} retains a 44px touch target`);
            check(await page.locator(selector).evaluate(el => {
              const r = el.getBoundingClientRect(); return el.contains(document.elementFromPoint(r.x + r.width / 2, r.y + r.height / 2));
            }), `${selector} is not occluded`);
          }
          if (width < 768) check(g.textarea.font >= 16, 'mobile entry avoids iOS zoom');
          assert.equal(await field.inputValue(), content, `${label}: draft unchanged`);
          if (content === SHORT) {
            for (const [selector, size] of [['.attachment-button-slot .extras-add', 44], ['.send-button', sendSize]]) {
              const visual = await page.locator(selector).evaluate(el => {
                const s = getComputedStyle(el, '::before'); return [parseFloat(s.width), parseFloat(s.height)];
              });
              assert.deepEqual(visual, [size, size], `${label}: ${selector} paints its expected surface`);
            }
          }
          if (!SOURCE && content === SHORT && [1920, 1440, 390, 320].includes(width)) {
            fs.mkdirSync('artifacts', { recursive: true });
            await page.screenshot({ path: `artifacts/composer-layout-${mode}-${width}.png`, animations: 'disabled' });
          }
        }
        if (!SOURCE) {
          for (const [trigger, dialog] of [['选择供应商与模型', '选择供应商与模型'], ['思考等级', '选择思考等级']]) {
            await page.getByRole('button', { name: trigger, exact: true }).click();
            const picker = page.getByRole('dialog', { name: dialog, exact: true });
            await picker.waitFor(); await page.keyboard.press('Escape'); await picker.waitFor({ state: 'hidden' });
          }
          assert.equal(await field.inputValue(), MULTILINE, 'browsing configuration preserves draft');
        }
        // Check the same button surface in busy/stop states without sending a
        // prompt or clicking stop. Integrated mode uses real fixture events.
        if (SOURCE) {
          for (const state of ['is-loading', 'is-stop is-loading']) {
            await page.locator('.send-button').evaluate((el, state) => { el.className = `send-button ${state}`; el.disabled = true; }, state);
            assert.deepEqual(await page.locator('.send-button').evaluate(el => {
              const s = getComputedStyle(el, '::before'); return [parseFloat(s.width), parseFloat(s.height)];
            }), [sendSize, sendSize], `${mode}/${width}: ${state} keeps the button surface`);
          }
          await page.locator('.send-button').evaluate(el => { el.className = 'send-button'; el.disabled = false; });
        } else {
          fixture.emit({ type: 'agent_start', runId: 'layout-stop-check' });
          const stop = page.getByRole('button', { name: '停止运行', exact: true });
          await stop.waitFor();
          assert.deepEqual(await stop.evaluate(el => {
            const s = getComputedStyle(el, '::before'); return [parseFloat(s.width), parseFloat(s.height)];
          }), [sendSize, sendSize], `${mode}/${width}: stop keeps the button surface`);
          if (width < 768) { const r = await stop.boundingBox(); assert(r.width >= 44 && r.height >= 44, 'stop retains a 44px touch target'); }
          fixture.emit({ type: 'agent_settled', runId: 'layout-stop-check', outcome: 'completed', pendingResume: false });
          await page.getByRole('button', { name: '发送消息', exact: true }).waitFor();
          assert.equal(await field.inputValue(), MULTILINE, 'streaming and settlement preserve draft');
        }
        console.log(`PASS ${SOURCE ? 'source CSS' : 'integrated WASM'} ${mode}/${width}: compact toolbar, input space, long label, targets, shared width`);
      }
      await page.setViewportSize({ width: 1440, height: 900 });
    }
    assert.deepEqual(errors, []);
    if (fixture) assert.equal(fixture.requests.filter(r => !r.body.type?.startsWith('get_') && r.path.endsWith('/commands')).length, 0, 'no fixture mutations');
    console.log(`PASS ${measurements.length} layout samples; ${SOURCE ? 'CSS fixture only; integrated/IME behavior not exercised' : 'real fixture and draft preserved; no model writes'}`);
  } catch (error) {
    errors.push(error.message);
    fs.mkdirSync('artifacts', { recursive: true });
    if (page) {
      await page.screenshot({ path: 'artifacts/composer-layout-failure.png', animations: 'disabled' }).catch(() => {});
      console.error(await page.locator('.composer-main').innerText().catch(() => 'No composer'));
    }
    throw error;
  } finally {
    fs.mkdirSync('artifacts', { recursive: true });
    fs.writeFileSync(`artifacts/composer-layout-${SOURCE ? 'css' : 'wasm'}-validation.json`, JSON.stringify({ errors, measurements }, null, 2));
    if (browser) await browser.close();
    if (fixture) await fixture.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
