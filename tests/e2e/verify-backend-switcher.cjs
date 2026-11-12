// Formal production WASM; synthetic HTTP/SSE node only. Never starts an agent.
// WORK_BACKEND_ASSETS=target/backend-switch-build/parity-web-server node tests/e2e/verify-backend-switcher.cjs
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const http = require('node:http');
const crypto = require('node:crypto');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');
const root = path.resolve(__dirname, '../..');
const assets = path.resolve(root, process.env.WORK_BACKEND_ASSETS || 'target/backend-switch-build/parity-web-server');
const out = path.join(root, 'artifacts/backend-switch');
const sha = file => crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
const report = { results: [], errors: [], geometry: [], blockedExternal: [], coverageGaps: [
  'Synthetic HTTP catalog and persisted fixture bindings only: no real ACP process, model call, credentials or agent lifecycle.',
  'Chromium viewport/media checks are not native WebView2, Android or physical-device acceptance.',
  'Same-address reconnect is not exercised; initial disconnected-state locking and stale-dialog session-scope remount are exercised.',
  'Unknown selected-session binding cannot be reached through normal navigation: the workspace clears selection until binding observation succeeds.'
] };
let fixture, host, browser, page;
async function serve() {
  const sockets = new Set(), upstream = new Set();
  const server = http.createServer((req, res) => {
    const pathname = new URL(req.url, 'http://localhost').pathname;
    if (pathname.startsWith('/api/')) {
      // Fixed test-only hosted auth; no product CORS or auth policy is changed.
      const proxy = http.request(new URL(req.url, fixture.url), { method: req.method, headers: { ...req.headers, cookie: 'rpi_web_session=fixture-session-only' } }, response => {
        res.writeHead(response.statusCode, response.headers); response.pipe(res);
      });
      upstream.add(proxy); proxy.on('close', () => upstream.delete(proxy));
      proxy.on('error', () => { if (!res.headersSent) res.writeHead(502); res.end(); });
      res.on('close', () => proxy.destroy()); req.pipe(proxy); return;
    }
    const relative = pathname === '/' ? 'index.html' : decodeURIComponent(pathname).replace(/^\//, '');
    const file = path.resolve(assets, relative);
    if (!file.startsWith(assets + path.sep)) { res.writeHead(403).end(); return; }
    fs.readFile(file, (error, bytes) => {
      if (error) { res.writeHead(404).end(); return; }
      const type = file.endsWith('.wasm') ? 'application/wasm' : file.endsWith('.js') ? 'text/javascript' : file.endsWith('.css') ? 'text/css' : file.endsWith('.svg') ? 'image/svg+xml' : file.endsWith('.png') ? 'image/png' : 'text/html';
      res.writeHead(200, { 'content-type': type, 'cache-control': 'no-store' }); res.end(bytes);
    });
  });
  server.requestTimeout = 15000; server.headersTimeout = 10000;
  server.on('connection', socket => { sockets.add(socket); socket.on('close', () => sockets.delete(socket)); });
  await new Promise((resolve, reject) => { server.once('error', reject); server.listen(0, '127.0.0.1', resolve); });
  return { url: `http://127.0.0.1:${server.address().port}/`, close: async () => {
    for (const req of upstream) req.destroy(); for (const socket of sockets) socket.destroy();
    await new Promise(resolve => server.close(resolve));
  } };
}
function validateBuild() {
  const build = JSON.parse(fs.readFileSync(path.join(assets, 'build.json'), 'utf8').replace(/^\uFEFF/, ''));
  assert(Object.keys(build.sourceHashes || {}).length, 'build must identify source hashes');
  for (const [file, hash] of Object.entries(build.sourceHashes)) assert.equal(sha(path.resolve(root, file)), hash.toLowerCase(), `stale build source: ${file}`);
  assert.equal(sha(path.join(assets, 'assets/work-ui_bg.wasm')), build.wasmSha256.toLowerCase(), 'formal WASM hash');
  report.build = build;
}
(async () => {
  fs.mkdirSync(out, { recursive: true });
  const pass = text => { report.results.push(text); console.log('PASS', text); };
  try {
    validateBuild();
    fixture = await startFixture(0, { authRequired: true, agentBackends: [
      { id: 'rpi', name: 'RPI', available: true, reason: null },
      { id: 'claude', name: 'Claude', available: true, reason: null },
      { id: 'codex', name: 'Codex', available: true, reason: null },
      { id: 'opencode', name: 'OpenCode', available: false, reason: 'Synthetic unavailable backend' }
    ], sessionBackends: { dark: 'claude' } });
    host = await serve();
    browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
    const context = await browser.newContext({ viewport: { width: 1440, height: 1000 } });
    await context.route('**/*', route => {
      const url = new URL(route.request().url());
      if (!/^https?:$/.test(url.protocol) || url.origin === new URL(host.url).origin) return route.continue();
      report.blockedExternal.push({ url: url.href, method: route.request().method() });
      return route.fulfill({ status: 404, contentType: 'application/json', body: '{}' });
    });
    page = await context.newPage(); page.setDefaultTimeout(12000);
    page.on('pageerror', error => report.errors.push(error.message));
    const button = name => page.getByRole('button', { name, exact: true });
    const entry = () => page.locator('.app-header .top-actions').getByRole('button', { name: '会话设置', exact: true });
    const settings = () => page.getByRole('dialog', { name: '会话设置', exact: true });
    const designEntry = () => page.locator('.app-header').getByRole('button', { name: '设计引用', exact: true });
    const dialog = () => page.getByRole('dialog', { name: '设计引用', exact: true });
    const close = async modal => { await page.keyboard.press('Escape'); await modal.waitFor({ state: 'hidden' }); };
    const design = async () => {
      if (await settings().isVisible()) await close(settings());
      if (!await dialog().isVisible()) { await designEntry().click(); await dialog().waitFor(); }
      assert.equal(await dialog().getByRole('tab').count(), 0, 'design references are independent of settings tabs');
      if (!await dialog().locator('.execution-backend-menu').count()) await dialog().getByRole('button', { name: '执行后端', exact: true }).click();
      assert.equal(await page.locator('dialog:modal').count(), 1, 'one design dialog, not nested session settings');
      assert.equal(await page.locator('.execution-backend-menu').count(), 1, 'exactly one Agent selector');
    };
    const confirm = () => page.getByRole('dialog', { name: '使用其他执行后端创建新会话？', exact: true });
    const option = name => dialog().getByRole('button', { name, exact: true });
    const field = () => page.locator('.composer-main textarea');
    const creates = () => fixture.requests.filter(r => r.method === 'POST' && r.path === '/api/v1/sessions');
    const session = id => page.locator(`[data-session-id="${id}"] .session-item`);
    const selected = id => page.waitForFunction(id => document.querySelector('.session-entry.selected')?.dataset.sessionId === id, id);
    const label = async value => {
      const wasOpen = await dialog().isVisible();
      await design();
      await page.waitForFunction(value => document.querySelector('.session-backend-settings summary')?.textContent.trim() === `执行后端 · ${value}`, value);
      if (!wasOpen) { await page.keyboard.press('Escape'); await dialog().waitFor({ state: 'hidden' }); }
    };
    const settled = async () => { await page.evaluate(async () => {
      await Promise.all(document.getAnimations().filter(a => a.effect?.getTiming().iterations !== Infinity).map(a => a.finished.catch(() => {})));
      await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    }); };
    const hit = async locator => { const box = await locator.boundingBox(); assert(box && box.width >= 44 && box.height >= 44, `44px target: ${JSON.stringify(box)}`); return box; };
    const choose = async name => { await design(); await option(name).click(); };
    await page.goto(host.url);
    await entry().waitFor();
    await entry().click(); await settings().waitFor();
    assert.equal(await settings().getByRole('tab', { name: '配置', exact: true }).getAttribute('aria-selected'), 'true');
    assert.equal(await page.locator('.execution-backend-menu, .session-agent-settings').count(), 0, 'no duplicate Agent in session configuration');
    await close(settings());
    await designEntry().click(); await dialog().waitFor();
    await dialog().getByText('选择会话后可管理设计引用与执行后端。', { exact: true }).waitFor();
    await page.keyboard.press('Escape'); await dialog().waitFor({ state: 'hidden' });
    assert.equal(creates().length, 0);
    assert.equal(await page.locator('.app-header .backend-switch-trigger, .app-header .execution-backend-menu').count(), 0);
    pass('disconnected session settings remain inspectable; no-session design settings cannot mutate fake session bindings');
    await button('打开设置').click();
    await page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '工作协同', exact: true }).click();
    await button('连接其他节点').click();
    await page.getByLabel('节点地址', { exact: true }).fill(host.url);
    await button('连接节点').click();
    await page.locator('.server-details [role=status]').filter({ hasText: '在线' }).waitFor();
    await button('返回会话').click();
    await page.locator('[data-project-id="rpi-work"]').waitFor();
    assert.equal(await page.locator('.composer .execution-backend-menu, .composer .backend-switch-trigger').count(), 0);
    assert.equal(creates().length, 0);
    await page.locator('[data-project-id="rpi-work"] .project-new-session').click();
    await page.waitForFunction(() => document.querySelector('.session-entry.selected')?.dataset.sessionId?.startsWith('new-'));
    await label('rpi');
    assert.equal(creates().length, 1); assert.deepEqual(creates()[0].body, { projectId: 'rpi-work' });
    const preferenceId = await page.locator('.session-entry.selected').getAttribute('data-session-id');
    await field().fill('independent new-session draft');
    pass('project + retains the default RPI preference and persisted independent session binding');
    await session('dark').click(); await selected('dark'); await label('claude');
    const draft = 'original Claude draft — synthetic only'; await field().fill(draft);
    await entry().click(); await settings().waitFor();
    assert.equal(await settings().getByRole('tab', { name: '配置', exact: true }).getAttribute('aria-selected'), 'true');
    assert.deepEqual(await settings().getByRole('tab').allTextContents(), ['配置', '运行信息', '会话操作']);
    assert.equal(await settings().locator('.execution-backend-menu, .session-agent-settings').count(), 0, 'configuration contains no second Agent switch');
    await design();
    assert.equal(await settings().isVisible(), false, 'settings closes before opening design references');
    assert.equal(await page.getByRole('dialog', { name: '会话资源配置', exact: true }).count(), 0);
    for (const section of ['Packages', '扩展', 'Skills', 'Tools', '提示词', '执行后端']) assert.equal(await dialog().getByRole('button', { name: section, exact: true }).count(), 1);
    await close(dialog());
    await entry().click(); await settings().waitFor();
    await settings().getByRole('tab', { name: '运行信息', exact: true }).click();
    assert.equal(await settings().getByRole('tabpanel').locator('input, select, textarea, button').count(), 0, 'runtime information is read-only');
    await settings().getByRole('tab', { name: '会话操作', exact: true }).click();
    assert.equal(await settings().getByRole('button', { name: '重命名', exact: true }).count(), 1);
    assert.equal(await settings().getByRole('button', { name: '删除', exact: true }).count(), 1);
    await close(settings());
    await designEntry().click(); await dialog().waitFor();
    assert.equal(await dialog().getByRole('tab').count(), 0);
    assert.equal(await page.locator('dialog:modal').count(), 1);
    await close(dialog());
    assert.equal(await field().inputValue(), draft); assert.equal(creates().length, 1);
    pass('configuration/runtime/actions and independent design references retain all sections/actions and only one Agent selector');
    for (const cancel of ['button', 'Escape']) {
      await choose('Codex'); await confirm().waitFor();
      if (cancel === 'button') await confirm().getByRole('button', { name: '取消', exact: true }).click();
      else await page.keyboard.press('Escape');
      await confirm().waitFor({ state: 'hidden' });
      assert.equal(creates().length, 1); assert.equal(await field().inputValue(), draft); await label('claude');
      await page.keyboard.press('Escape'); await dialog().waitFor({ state: 'hidden' });
    }
    pass('persisted binding overrides preference; cancel and Escape preserve draft without creating');
    await choose('Codex'); await confirm().waitFor();
    fixture.controls.sessionDelayMs = 5000;
    await confirm().getByRole('button', { name: '确认创建新会话', exact: true }).click();
    await page.waitForFunction(() => document.querySelector('.new-task')?.disabled);
    await entry().click(); await settings().waitFor();
    await close(settings());
    await designEntry().click(); await dialog().waitFor();
    await dialog().getByText('选择会话后可管理设计引用与执行后端。', { exact: true }).waitFor();
    assert.equal(await dialog().locator('.execution-backend-menu').count(), 0, 'creation detaches selection, so no second creation can be initiated');
    await page.keyboard.press('Escape'); await dialog().waitFor({ state: 'hidden' }); fixture.controls.sessionDelayMs = 0;
    await label('codex'); await selected('new-5');
    assert.equal(creates().length, 2); assert.deepEqual(creates()[1].body, { projectId: 'rpi-work', backend: 'codex' });
    assert.equal(await field().inputValue(), '');
    await session('dark').click(); await selected('dark'); await label('claude'); assert.equal(await field().inputValue(), draft);
    await session(preferenceId).click(); await selected(preferenceId); assert.equal(await field().inputValue(), 'independent new-session draft');
    await session('dark').click(); await selected('dark'); await label('claude');
    pass('confirm creates exactly one typed same-project POST; old bindings and independent drafts restore');

    await design(); assert(await option('OpenCode').isDisabled());
    await page.keyboard.press('Escape'); await dialog().waitFor({ state: 'hidden' });
    pass('authoritative catalog unavailable backend and in-flight creation choices are disabled');

    // Real sidebar click delivered programmatically because native modal blocks physical outside input.
    await choose('Codex'); await confirm().waitFor();
    await session('architecture').evaluate(e => e.click()); await selected('architecture');
    await confirm().waitFor({ state: 'hidden' });
    if (await dialog().isVisible()) { await page.keyboard.press('Escape'); await dialog().waitFor({ state: 'hidden' }); }
    assert.equal(creates().length, 2);
    await session('dark').click(); await selected('dark'); await label('claude');
    pass('session navigation destroys stale pending dialog without creation');

    for (const mode of ['light', 'dark']) {
      await button('外观与主题').click();
      const appearance = page.getByRole('dialog', { name: '外观与主题', exact: true });
      await appearance.getByLabel('明暗模式', { exact: true }).selectOption(mode);
      await appearance.getByRole('button', { name: '保存', exact: true }).click(); await appearance.waitFor({ state: 'hidden' });
      for (const width of [1440, 390, 320]) {
        await page.setViewportSize({ width, height: width === 1440 ? 1000 : 844 }); await settled();
        const entryBox = await hit(entry());
        const header = await page.locator('.app-header').evaluate(e => { const r = e.getBoundingClientRect(); return { x: r.x, width: r.width, client: e.clientWidth, scroll: e.scrollWidth, background: getComputedStyle(e).backgroundColor }; });
        assert(header.x >= -1 && header.x + header.width <= width + 1 && header.scroll <= header.client + 1, `header overflow ${JSON.stringify(header)}`);
        await page.screenshot({ path: path.join(out, `${mode}-${width}-header.png`) });
        await design(); await settled();
        const targets = {}; for (const name of ['RPI', 'Claude', 'Codex', 'OpenCode']) targets[name] = await hit(option(name));
        const modal = await dialog().evaluate(e => { const r = e.getBoundingClientRect(); return { x: r.x, width: r.width, client: e.clientWidth, scroll: e.scrollWidth, background: getComputedStyle(e).backgroundColor }; });
        assert(modal.x >= -1 && modal.x + modal.width <= width + 1 && modal.scroll <= modal.client + 1, `modal overflow ${JSON.stringify(modal)}`);
        assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), 'viewport overflow');
        report.geometry.push({ mode, width, entryBox, header, modal, targets });
        await page.screenshot({ path: path.join(out, `${mode}-${width}-dialog.png`) });
        await page.keyboard.press('Escape'); await dialog().waitFor({ state: 'hidden' });
        assert.equal(await field().inputValue(), draft);
      }
      await page.setViewportSize({ width: 1440, height: 1000 });
    }
    assert.notEqual(report.geometry.find(g => g.mode === 'light').modal.background, report.geometry.find(g => g.mode === 'dark').modal.background, 'actual modal backgrounds differ by mode');
    pass('1440/390/320 light/dark settled screenshots, actual backgrounds, 44px entry/options, no header/modal/viewport overflow');
    assert.equal(creates().length, 2);
    assert.equal(fixture.requests.filter(r => ['prompt', 'queue_message', 'set_model', 'set_thinking_level'].includes(r.body.type)).length, 0);
    assert.deepEqual(report.errors, []); validateBuild(); report.status = 'passed';
  } catch (error) {
    report.status = 'failed'; report.failure = error.stack;
    if (page) await page.screenshot({ path: path.join(out, 'failure.png') }).catch(() => {});
    throw error;
  } finally {
    // Request details are entirely synthetic and intentionally omit credentials.
    report.requests = fixture?.requests.map(({ method, path, body }) => ({ method, path, body }));
    report.generatedAtUtc = new Date().toISOString();
    const cleanup = await Promise.allSettled([browser?.close(), host?.close(), fixture?.close()]);
    report.cleanupErrors = cleanup.filter(item => item.status === 'rejected').map(item => String(item.reason));
    fs.writeFileSync(path.join(out, 'validation.json'), JSON.stringify(report, null, 2) + '\n');
    if (report.cleanupErrors.length) throw new Error(`Cleanup failed: ${report.cleanupErrors.join('; ')}`);
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
