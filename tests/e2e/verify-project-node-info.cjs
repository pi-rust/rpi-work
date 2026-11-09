// Integrated WASM must be built/served separately. Run: node tests/e2e/verify-project-node-info.cjs
// Fixture-only diagnostics: no prompts, queue writes, model/thinking changes or credentials.
// Deliberately uses real time for the one 30s periodic case: advancing all browser
// timers also advances unrelated WebSocket/snapshot machinery and would confound it.
// First browser run: artifacts/project-node-info-browser.log and
// artifacts/project-node-info-validation.json recorded 9/14 passes. Corrected
// assumptions below need a rebuilt integrated UI and caller-run browser rerun.
// Case 03 retains the Tab trap assertion for the production modal fix; case 06
// retains the post-close snapshot without navigation retries/error suppression.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');
const APP = process.env.WORK_UI_URL || 'http://127.0.0.1:14180/';
const DRAFT = '查看节点信息不改变草稿 😀';
const SECRET = 'fixture-secret-token-MUST-NOT-RENDER';
const LONG = '超长项目名称-LongProjectNameWithoutBreaks'.repeat(8);
const LABELS = ['节点类型', '节点地址', '当前状态', '最近检查', '服务', '版本', '协议', '传输', '节点认证', '可用命令', '可用操作', '项目', '工作目录'];
const FORBIDDEN = new Set(['prompt', 'queue_message', 'cancel_queued', 'clear_queue', 'set_model', 'set_thinking_level', 'set_api_key']);
const results = [], geometry = [], errors = [];
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
let browser;
async function until(check, label, timeout = 10000) {
  const end = Date.now() + timeout;
  while (Date.now() < end) { if (await check()) return; await delay(40); }
  throw new Error(`Timed out: ${label}`);
}
function save() {
  fs.writeFileSync('artifacts/project-node-info-validation.json', JSON.stringify({ results, geometry, errors,
    assumptions: [
      'An integrated WASM build is already served at WORK_UI_URL (default http://127.0.0.1:14180/); this script never builds it.',
      'Main mounts one Workspace probe; project-entry data-project-id, sibling badge, menu actions and scoped panel follow node_info.rs.',
      'Actual fixture routes are /api/v1/health and /api/v1/auth; handshake is left healthy until the connected workspace probe completes.',
      'HTTP localhost is remote. Health/auth probes are independent of selected-session WebSocket projection.',
      'A reconnect increments connection epoch even when the HTTP client/address is reused; URL plus epoch scopes panels and probe results.',
      'Only fixture metadata rename is written directly; an orphan session is synthesized in the session-list response without editing shared fixtures.',
      'Probe refresh reads health then auth only when health.ok; failures use sanitized UI messages. Tests do not require auth after failed health.',
      'Badge accessible names include node kind; capability rows show counts from the recorded fixture capabilities response.',
      'Sidebar geometry waits for the whole sidebar and matching viewport; dialog geometry waits for its animations.',
      'Failed session WebSocket can project reconnecting (连接中), independently of a healthy node probe.',
    ], limitations: [
      results.length
        ? `Browser execution recorded ${results.filter(r => r.status === 'PASS').length}/${results.length} passing cases; failures remain in results/errors. Acceptance requires all 14 cases on the separately rebuilt integrated UI.`
        : 'No browser cases recorded by this invocation; node --check alone is syntax-only, not browser acceptance.',
      '390/320px Chromium emulation is not physical Android, native WebView, keyboard or safe-area acceptance.',
      'Periodic coverage waits at most 35s in real time, not page.clock fast-forward of all UI timers.',
      'Scope races use a bounded one-shot route response gate after initial connection, not a handshake lock.',
    ] }, null, 2));
}
async function test(name, verify, options = {}) {
  let context, page, deadline;
  const started = Date.now(), fixtures = [], gates = [], requests = [], external = [], localErrors = [];
  const proof = { name, status: 'FAIL', requests, external, errors: localErrors };
  let closing = false;
  try {
    const fixture = await startFixture(0); fixtures.push(fixture);
    const policies = new Map();
    const policyFor = node => {
      const origin = new URL(node.url).origin;
      if (!policies.has(origin)) policies.set(origin, { health: 'healthy', auth: 'normal', service: 'Node A', version: '1.2.3', empty: false, orphan: false, blockEvents: false });
      return policies.get(origin);
    };
    const policy = policyFor(fixture);
    if (options.orphan) policy.orphan = true;
    if (options.longName) {
      const response = await fetch(`${fixture.url}api/v1/projects/rpi-work/rename`, {
        method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ name: LONG }),
      });
      assert(response.ok, 'isolated fixture metadata rename succeeds');
    }
    context = await browser.newContext({ viewport: { width: 1440, height: 900 }, serviceWorkers: 'block' });
    page = await context.newPage(); page.setDefaultTimeout(8000);
    page.on('pageerror', error => localErrors.push(error.message));
    page.on('websocket', socket => {
      const socketUrl = new URL(socket.url());
      const node = fixtures.find(candidate => {
        const nodeUrl = new URL(candidate.url);
        return nodeUrl.hostname === socketUrl.hostname && nodeUrl.port === socketUrl.port;
      });
      if (node && policyFor(node).blockEvents) socket.close();
    });
    function gate(label, budget = 14000) {
      let release, timer;
      const promise = new Promise(resolve => { release = resolve; });
      const item = { label, entered: false, released: false, delivered: false };
      item.release = () => { if (!item.released) { item.released = true; clearTimeout(timer); release(); } };
      item.wait = async () => {
        item.entered = true;
        if (!item.released) timer = setTimeout(() => { localErrors.push(`Gate expired: ${label}`); item.release(); }, budget);
        await promise;
      };
      gates.push(item); return item;
    }
    const h = {
      page, context, fixture, policy, requests, gate,
      async addNode() { const node = await startFixture(0); fixtures.push(node); policyFor(node).service = 'Node B'; return node; },
      policyFor,
      entry: (id = 'rpi-work') => page.locator(`.project-entry[data-project-id="${id}"]`),
      badge: (id = 'rpi-work') => h.entry(id).locator('.project-node-button'),
      session: id => page.locator(`[data-session-id="${id}"] .session-item`),
      dialog: () => page.getByRole('dialog', { name: '节点信息', exact: true }),
      field: () => page.locator('.composer-main textarea'),
      value: label => h.dialog().locator('.node-info-row').filter({ has: page.locator('dt').getByText(label, { exact: true }) }).locator('dd'),
      count: (path, node = fixture) => requests.filter(r => r.origin === new URL(node.url).origin && r.path === `/api/v1/${path}` && r.method === 'GET').length,
      sessionReads: () => requests.filter(r => /\/sessions\//.test(r.path)).length,
      async sidebar() {
        const trigger = page.getByRole('button', { name: '打开项目导航', exact: true });
        // A translated-offscreen sidebar still satisfies Playwright isVisible().
        if (await trigger.isVisible() && !await page.locator('.sidebar').evaluate(e => e.classList.contains('open')))
          await trigger.click();
        await h.badge().waitFor();
        // A badge can enter the viewport before the whole sidebar finishes sliding.
        const viewport = page.viewportSize();
        await until(() => page.locator('.sidebar').evaluate((e, expected) => {
          const r = e.getBoundingClientRect();
          return innerWidth === expected.width && innerHeight === expected.height && r.x >= -0.5 && r.right <= innerWidth;
        }, viewport), 'whole sidebar entrance transition and matching viewport');
      },
      async nodes() {
        await h.sidebar();
        await page.getByRole('button', { name: '打开设置', exact: true }).click();
        await page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '执行节点', exact: true }).click();
      },
      async connect(node = fixture) {
        if (!await page.getByLabel('节点地址', { exact: true }).isVisible())
          await page.getByRole('button', { name: '连接其他节点', exact: true }).click();
        await page.getByLabel('节点地址', { exact: true }).fill(node.url);
        await page.getByRole('button', { name: '连接节点', exact: true }).click();
        await page.locator('.server-details [role=status]').filter({ hasText: '在线' }).waitFor();
        await page.getByRole('button', { name: '返回会话', exact: true }).click();
        await h.sidebar(); await h.state('online');
      },
      async state(expected, id = 'rpi-work', timeout = 10000) {
        await until(async () => await h.badge(id).locator('.project-node-status').getAttribute('data-state') === expected, `badge ${id}: ${expected}`, timeout);
        if (await h.dialog().isVisible()) assert.equal(await h.dialog().locator('.node-info-summary').getAttribute('data-state'), expected);
      },
      async open(id = 'rpi-work') { await h.sidebar(); await h.badge(id).click(); await h.dialog().waitFor(); },
      async close(keyboard = false) {
        if (keyboard) await page.keyboard.press('Escape');
        else await h.dialog().getByRole('button', { name: '关闭', exact: true }).click();
        await h.dialog().waitFor({ state: 'hidden' });
      },
      async refresh() { await h.dialog().getByRole('button', { name: '刷新节点状态', exact: true }).click(); },
      async refreshTo(expected) { const before = h.count('health'); await h.refresh(); await until(() => h.count('health') > before, 'refresh health request'); await h.state(expected); },
      async select(id = 'dark') {
        await h.sidebar();
        const project = id === 'rpc' ? 'pi-rust' : 'rpi-work';
        if (!await h.session(id).isVisible()) await h.entry(project).locator('.project-toggle').click();
        await h.session(id).click();
        await page.waitForFunction(() => { const field = document.querySelector('.composer-main textarea'); return field && !field.disabled && field.dataset.imeReady === 'true'; });
        if (id === 'dark') await page.locator('.transcript').getByText('主题切换已经准备好，接下来运行组件测试。', { exact: true }).waitFor();
        await h.field().fill(DRAFT);
      },
      async snapshot() {
        return page.evaluate(() => ({
          selected: [...document.querySelectorAll('.session-entry.selected')].map(e => e.dataset.sessionId),
          projects: [...document.querySelectorAll('.project-entry')].map(e => ({ id: e.dataset.projectId, selected: !!e.querySelector('.project-row.selected'), expanded: e.querySelector('.project-toggle')?.getAttribute('aria-expanded') })),
          draft: document.querySelector('.composer-main textarea')?.value ?? null,
        }));
      },
      async unchanged(before) { assert.deepEqual(await h.snapshot(), before, 'viewing node info preserves selected session/project, expansion and source draft'); },
      async menuInfo(menu) {
        await menu.waitFor(); await menu.getByRole('menuitem', { name: '节点信息', exact: true }).click();
        await h.dialog().waitFor();
        assert.equal(await page.locator('.workspace-menu:popover-open').count(), 0, 'menus hidden before modal');
      },
      async reasonableFocus() {
        assert(await page.evaluate(() => {
          const e = document.activeElement, r = e?.getBoundingClientRect();
          return !!e && e !== document.body && !e.closest('dialog:not([open]), .workspace-menu:not(:popover-open)') && !e.disabled && r.width > 0 && r.height > 0;
        }), 'focus returns to a visible enabled trigger, never the hidden menu item/body');
      },
    };
    await context.route('**/*', async route => {
      const request = route.request(), url = new URL(request.url());
      if (!['http:', 'https:'].includes(url.protocol)) return route.continue();
      if (url.origin === 'https://api.github.com') return route.fulfill({ status: 404, json: {}, headers: { 'access-control-allow-origin': '*' } });
      if (url.origin === new URL(APP).origin) return route.continue();
      const node = fixtures.find(f => new URL(f.url).origin === url.origin);
      if (!node) { external.push(request.url()); return route.abort('blockedbyclient'); }
      const p = policyFor(node), method = request.method();
      if (method === 'OPTIONS') return route.continue();
      const record = { origin: url.origin, path: url.pathname, method };
      requests.push(record);
      const headers = { 'access-control-allow-origin': new URL(APP).origin, 'access-control-allow-credentials': 'true' };
      if (method === 'POST' && url.pathname.endsWith('/commands')) {
        record.command = request.postDataJSON().type;
        if (FORBIDDEN.has(record.command)) {
          localErrors.push(`Forbidden command: ${record.command}`); return route.abort('blockedbyclient');
        }
      }
      if (url.pathname === '/api/v1/auth' && method !== 'GET') {
        localErrors.push(`Forbidden auth write: ${method}`); return route.abort('blockedbyclient');
      }
      if (method === 'GET' && url.pathname === '/api/v1/capabilities') {
        // Record the actual handshake read; no extra requests or fixture writes.
        const response = await route.fetch();
        record.capabilities = await response.json();
        return route.fulfill({ response, json: record.capabilities });
      }
      if (method === 'GET' && url.pathname === '/api/v1/sessions' && p.orphan) {
        const response = await route.fetch(), json = await response.json();
        json.sessions.find(s => s.sessionId === 'architecture').projectId = null;
        return route.fulfill({ response, json });
      }
      if (method === 'GET' && url.pathname === '/api/v1/health') {
        // Snapshot the upstream result and policy BEFORE waiting. A newer node or
        // epoch can proceed, and this late response cannot acquire newer values.
        const held = p.healthGate; p.healthGate = null;
        const health = p.health, service = p.service, version = p.version, empty = p.empty;
        const response = await route.fetch();
        const json = health === 'fail' ? { error: `Bearer ${SECRET} https://private.invalid/?token=${SECRET}` }
          : { ok: health !== 'okfalse', ...(empty ? {} : { service, version, protocol: 'rpi-web-v1' }) };
        if (held) { held.snapshot = json; await held.wait(); }
        try { await route.fulfill({ response, status: health === 'fail' ? 503 : 200, headers, json }); }
        catch (error) { if (!closing && !request.failure()) throw error; }
        finally { if (held) held.delivered = true; }
        return;
      }
      if (method === 'GET' && url.pathname === '/api/v1/auth' && p.auth !== 'normal') {
        return route.fulfill({ status: p.auth === 'fail' ? 503 : 200, headers,
          json: p.auth === 'fail' ? { error: `Bearer ${SECRET}` } : { required: true, authenticated: p.auth === 'authenticated' } });
      }
      return route.continue();
    });
    const run = async () => {
      await page.goto(APP);
      await page.getByRole('button', { name: '打开设置', exact: true }).click();
      await page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '执行节点', exact: true }).click();
      await h.connect();
      await verify(h);
      assert.equal(requests.filter(r => FORBIDDEN.has(r.command)).length, 0);
      for (const node of fixtures) assert.equal(node.requests.filter(r => FORBIDDEN.has(r.body?.type)).length, 0);
      assert.deepEqual(external, []); assert.deepEqual(localErrors, []);
    };
    await Promise.race([run(), new Promise((_, reject) => { deadline = setTimeout(() => reject(new Error('Bounded case exceeded 90s')), 90000); })]);
    proof.status = 'PASS'; console.log('PASS', name);
  } catch (error) {
    proof.error = error.stack || String(error); errors.push({ name, error: proof.error });
    console.error('FAIL', name, error.message);
    if (page) await page.screenshot({ path: `artifacts/project-node-info-failure-${results.length + 1}.png`, animations: 'disabled', timeout: 3000 }).catch(() => {});
  } finally {
    clearTimeout(deadline); closing = true; gates.forEach(g => g.release());
    if (context) await context.close().catch(() => {});
    for (const fixture of fixtures) await fixture.close();
    proof.durationMs = Date.now() - started; results.push(proof); save();
  }
}
(async () => {
  fs.mkdirSync('artifacts', { recursive: true });
  const suiteDeadline = setTimeout(() => { console.error('Project node info suite exceeded 600s'); save(); process.exit(1); }, 600000);
  try {
    browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
    await test('01 grouped remote badges precede folder; idle sidebar status needs no selected session', async h => {
      const before = await h.snapshot();
      for (const id of ['rpi-work', 'pi-rust']) {
        assert.equal(await h.badge(id).getAttribute('data-node-kind'), 'remote', 'HTTP localhost is never local transport');
        assert.equal(await h.badge(id).getAttribute('aria-label'), `${id} · 远程节点 · 节点在线 · 查看节点信息`);
        assert.match(await h.badge(id).getAttribute('title'), /远程节点.*节点在线.*点击查看节点信息/);
        assert.equal(await h.badge(id).getAttribute('aria-haspopup'), 'dialog');
        const box = await h.entry(id).evaluate(e => {
          const badge = e.querySelector('.project-node-button'), row = e.querySelector('.project-row'), folder = row?.querySelector('.icon');
          const b = badge.getBoundingClientRect(), f = folder.getBoundingClientRect();
          return { sibling: badge.parentElement === row.parentElement, before: !!(badge.compareDocumentPosition(row) & Node.DOCUMENT_POSITION_FOLLOWING), nested: !!badge.closest('button button'), badge: { x: b.x, y: b.y, width: b.width, height: b.height }, folder: { x: f.x, y: f.y, width: f.width, height: f.height } };
        });
        assert(box.sibling && box.before && !box.nested); assert(box.badge.x + box.badge.width <= box.folder.x + 1);
        assert(Math.abs(box.badge.y + box.badge.height / 2 - box.folder.y - box.folder.height / 2) <= 3);
        geometry.push({ case: 'grouping', id, ...box });
      }
      await h.open('pi-rust');
      assert.equal(await h.dialog().getAttribute('id'), 'project-node-info');
      assert.deepEqual(await h.dialog().locator('.node-info-row dt').allTextContents(), LABELS);
      assert.equal(await h.value('项目').innerText(), 'pi-rust');
      assert.equal(await h.value('工作目录').innerText(), 'D:/Projects/pi-rust');
      assert.equal(await h.value('节点地址').innerText(), h.fixture.url.replace(/\/$/, ''));
      assert.match(await h.value('节点类型').innerText(), /远程节点/);
      assert.equal(await h.value('服务').innerText(), 'Node A');
      assert.equal(await h.value('版本').innerText(), '1.2.3');
      assert.notEqual(await h.value('最近检查').innerText(), '尚未检查');
      const capabilities = h.requests.find(r => r.origin === new URL(h.fixture.url).origin && r.path === '/api/v1/capabilities' && r.method === 'GET')?.capabilities;
      assert(Array.isArray(capabilities?.supportedCommands), 'fixture command capabilities recorded');
      assert(Array.isArray(capabilities?.supportedActions), 'fixture action capabilities recorded');
      assert.equal(await h.value('可用命令').innerText(), `${capabilities.supportedCommands.length} 项`);
      assert.equal(await h.value('可用操作').innerText(), `${capabilities.supportedActions.length} 项`);
      await h.close(); await h.unchanged(before);
    });
    await test('02 hovering and repeated viewing do not multiply probes or session reads', async h => {
      await h.select(); await delay(250);
      const before = await h.snapshot(), counts = [h.count('health'), h.count('auth'), h.sessionReads()];
      await h.badge().hover(); await delay(250);
      for (let i = 0; i < 2; i++) { await h.open(); await h.close(); }
      assert.deepEqual([h.count('health'), h.count('auth'), h.sessionReads()], counts);
      await h.unchanged(before);
    });
    await test('03 keyboard opens modal, traps focus, Escape restores badge and draft', async h => {
      await h.select(); const before = await h.snapshot();
      await h.badge().focus(); await h.page.keyboard.press('Enter'); await h.dialog().waitFor();
      await until(() => h.dialog().evaluate(e => e.contains(document.activeElement)), 'modal initial focus');
      for (let i = 0; i < 7; i++) {
        await h.page.keyboard.press('Tab'); assert(await h.dialog().evaluate(e => e.contains(document.activeElement)), 'Tab stays inside modal');
      }
      await h.close(true); assert(await h.badge().evaluate(e => e === document.activeElement));
      await h.unchanged(before);
    });
    await test('04 icon context and project menu capture nonselected project without expansion/selection', async h => {
      await h.select(); const before = await h.snapshot();
      await h.badge('pi-rust').click({ button: 'right' });
      await h.menuInfo(h.page.locator('#project-menu-pi-rust[role=menu][aria-label="项目操作"]'));
      assert.equal(await h.value('项目').innerText(), 'pi-rust');
      await h.close(); await h.reasonableFocus(); await h.unchanged(before);
      await h.entry('pi-rust').locator('.project-row-menu').click();
      await h.menuInfo(h.page.locator('#project-menu-pi-rust'));
      assert.equal(await h.value('工作目录').innerText(), 'D:/Projects/pi-rust');
      await h.close(true); await h.reasonableFocus(); await h.unchanged(before);
    });
    await test('05 session menu uses associated project, orphan shows independent session', async h => {
      await h.select();
      if (!await h.session('rpc').isVisible()) await h.entry('pi-rust').locator('.project-toggle').click();
      for (const [id, project, cwd] of [['rpc', 'pi-rust', 'D:/Projects/pi-rust'], ['architecture', '独立会话（未关联项目）', '未报告']]) {
        const before = await h.snapshot();
        await h.session(id).click({ button: 'right' });
        await h.menuInfo(h.page.locator(`#session-menu-${id}[role=menu][aria-label="会话操作"]`));
        assert.equal(await h.value('项目').innerText(), project); assert.equal(await h.value('工作目录').innerText(), cwd);
        await h.close(); await h.reasonableFocus(); await h.unchanged(before);
      }
    }, { orphan: true });
    await test('06 failed health is sanitized, refresh recovers, missing fields never become blank', async h => {
      await h.select(); const before = await h.snapshot(); await h.open();
      h.policy.health = 'fail'; await h.refreshTo('offline');
      assert.match(await h.dialog().locator('[role=alert]').innerText(), /节点不可达/);
      assert.doesNotMatch(await h.dialog().innerText(), new RegExp(`${SECRET}|private\\.invalid|Bearer`));
      h.policy.health = 'healthy'; h.policy.empty = true; await h.refreshTo('online');
      assert.equal(await h.value('服务').innerText(), '未报告');
      // Descriptor fallback for version/protocol/transport is intentionally allowed.
      for (const label of LABELS) assert((await h.value(label).innerText()).trim(), `${label} has a value/fallback`);
      assert.equal(await h.dialog().locator('[role=alert]').count(), 0);
      await h.close(); await h.unchanged(before);
    });
    await test('07 health ok:false is unready not green and skips authentication', async h => {
      await h.open(); const auth = h.count('auth'); h.policy.health = 'okfalse';
      await h.refreshTo('unready'); assert.equal(h.count('auth'), auth);
      assert.equal(await h.value('节点认证').innerText(), '未知（当前未确认）');
      h.policy.health = 'healthy'; await h.refreshTo('online');
    });
    await test('08 unauthenticated required auth is not green; authenticated recovery is online', async h => {
      await h.open(); h.policy.auth = 'required'; await h.refreshTo('auth');
      assert.equal(await h.value('节点认证').innerText(), '需要登录');
      h.policy.auth = 'authenticated'; await h.refreshTo('online');
      assert.equal(await h.value('节点认证').innerText(), '已认证');
    });
    await test('09 auth read failure is unknown, sanitized, never inherited authenticated green', async h => {
      await h.open(); h.policy.auth = 'fail'; await h.refreshTo('unknown');
      assert.equal(await h.value('节点认证').innerText(), '未知（当前未确认）');
      assert.match(await h.dialog().locator('[role=alert]').innerText(), /无法读取节点认证状态/);
      assert.doesNotMatch(await h.dialog().innerText(), new RegExp(`${SECRET}|Bearer`));
      h.policy.auth = 'normal'; await h.refreshTo('online');
    });
    await test('10 duplicate synchronous refresh schedules one health/auth pair and zero session reads', async h => {
      await h.select(); await delay(250); await h.open(); const before = await h.snapshot();
      const counts = [h.count('health'), h.count('auth'), h.sessionReads()];
      const held = h.gate('duplicate refresh health'); h.policy.healthGate = held;
      // Same JS turn, before Dioxus commits disabled: exercises the handler guard,
      // not merely Playwright refusing to click an already-disabled element.
      await h.dialog().getByRole('button', { name: '刷新节点状态', exact: true }).evaluate(e => { e.click(); e.click(); });
      await until(() => held.entered, 'refresh snapshot held'); await h.state('checking');
      assert(await h.dialog().getByRole('button', { name: '刷新节点状态', exact: true }).isDisabled());
      assert.equal(h.count('health'), counts[0] + 1); assert.equal(h.count('auth'), counts[1]);
      held.release(); await h.state('online'); await delay(200);
      assert.deepEqual([h.count('health'), h.count('auth'), h.sessionReads()], [counts[0] + 1, counts[1] + 1, counts[2]]);
      await h.close(); await h.unchanged(before);
    });
    await test('11 one Workspace periodic loop checks even with panel closed; offline within 35 seconds', async h => {
      await h.open(); await h.refreshTo('online'); await h.close();
      const health = h.count('health'), auth = h.count('auth'), reads = h.sessionReads();
      h.policy.health = 'fail';
      await h.state('offline', 'rpi-work', 35000); await h.state('offline', 'pi-rust');
      assert.equal(h.count('health'), health + 1, 'one probe per workspace, not per project');
      assert.equal(h.count('auth'), auth, 'failed health skips auth'); assert.equal(h.sessionReads(), reads);
      await h.open(); assert.equal(await h.dialog().locator('.node-info-summary').getAttribute('data-state'), 'offline');
      h.policy.health = 'healthy'; await h.refreshTo('online');
    });
    await test('12 WebSocket disconnection leaves node online and reports session connection separately', async h => {
      await h.select(); const before = await h.snapshot();
      h.policy.blockEvents = true; h.fixture.disconnect('dark', true); await h.open();
      // Failed WebSocket retries project Reconnecting, not necessarily Offline.
      await until(async () => /会话连接：\s*(?:连接中|未连接)/.test(await h.dialog().locator('.node-info-session').innerText()), 'session projection reconnecting or disconnected');
      await h.state('online'); await h.refreshTo('online');
      assert.match(await h.dialog().locator('.node-info-session').innerText(), /独立于节点健康检查/);
      await h.close(); await h.unchanged(before);
    });
    await test('13 URL and same-address epoch reject late health, clear old probe and close old panel', async h => {
      const nodeB = await h.addNode();
      let current = h.fixture;
      for (const [target, oldService, newService] of [[nodeB, 'Node A stale', 'Node B'], [nodeB, 'Node B old epoch', 'Node B new epoch']]) {
        const oldPolicy = h.policyFor(current), nextPolicy = h.policyFor(target);
        await h.open(); oldPolicy.service = oldService;
        const held = h.gate(`late health: ${oldService}`); oldPolicy.healthGate = held;
        await h.refresh(); await until(() => held.entered, 'old health captured after initial connect');
        assert.equal(held.snapshot.service, oldService); await h.state('checking');
        nextPolicy.service = newService;
        // Native modal blocks pointer input outside it. Dispatch the real settings
        // trigger to initiate an out-of-band reconnect while the scoped panel is
        // still open; do NOT use the dialog's settings action (which closes it).
        await h.page.getByRole('button', { name: '打开设置', exact: true, includeHidden: true }).evaluate(e => e.click());
        await h.page.getByRole('navigation', { name: '设置分类', includeHidden: true }).getByRole('button', { name: '执行节点', exact: true, includeHidden: true }).evaluate(e => e.click());
        const address = h.page.getByLabel('节点地址', { exact: true });
        if (!await address.isVisible()) await h.page.getByRole('button', { name: '连接其他节点', exact: true, includeHidden: true }).evaluate(e => e.click());
        await address.evaluate((e, value) => {
          const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set;
          setter.call(e, value); e.dispatchEvent(new Event('input', { bubbles: true })); e.dispatchEvent(new Event('change', { bubbles: true }));
        }, target.url);
        await h.page.getByRole('button', { name: '连接节点', exact: true, includeHidden: true }).evaluate(e => e.click());
        await h.dialog().waitFor({ state: 'hidden' });
        await h.page.locator('.server-details [role=status]').filter({ hasText: '在线' }).waitFor();
        await h.page.getByRole('button', { name: '返回会话', exact: true }).click();
        await h.sidebar(); await h.state('online'); await h.open();
        assert.equal(await h.value('服务').innerText(), newService);
        assert.equal(await h.value('节点地址').innerText(), target.url.replace(/\/$/, ''));
        held.release(); await until(() => held.delivered, 'late response released'); await delay(250);
        assert(await h.dialog().isVisible(), 'old completion cannot close new panel');
        await h.state('online'); assert.equal(await h.value('服务').innerText(), newService);
        assert.doesNotMatch(await h.dialog().innerText(), new RegExp(oldService));
        await h.close(); current = target;
      }
    });
    await test('14 long-name sidebar and scrollable dialog: 1440/390/320 light/dark screenshots and geometry', async h => {
      await h.select(); const before = await h.snapshot();
      for (const mode of ['light', 'dark']) {
        await h.page.setViewportSize({ width: 1440, height: 900 });
        await h.page.getByRole('button', { name: '外观与主题', exact: true }).click();
        const appearance = h.page.getByRole('dialog', { name: '外观与主题', exact: true });
        await appearance.getByLabel('明暗模式', { exact: true }).selectOption(mode);
        await appearance.getByRole('button', { name: '保存', exact: true }).click();
        await appearance.waitFor({ state: 'hidden' });
        await h.page.waitForFunction(mode => document.documentElement.dataset.mode === mode, mode);
        for (const width of [1440, 390, 320]) {
          await h.page.setViewportSize({ width, height: 844 }); await h.sidebar();
          const sidebar = await h.page.locator('.sidebar').evaluate(e => ({ client: e.clientWidth, scroll: e.scrollWidth, x: e.getBoundingClientRect().x, width: e.getBoundingClientRect().width }));
          assert(sidebar.scroll <= sidebar.client + 1, 'project sidebar has no horizontal overflow');
          assert(sidebar.x >= -1 && sidebar.x + sidebar.width <= width + 1);
          await h.page.screenshot({ path: `artifacts/project-node-info-${mode}-${width}-sidebar.png`, animations: 'disabled' });
          await h.open(); assert.equal(await h.value('项目').innerText(), LONG);
          await h.dialog().evaluate(async e => {
            await Promise.all(e.getAnimations({ subtree: true }).map(animation => animation.finished));
          });
          const box = await h.dialog().evaluate(e => {
            const rect = n => { const r = n.getBoundingClientRect(); return { x: r.x, y: r.y, width: r.width, height: r.height, bottom: r.bottom, right: r.right }; };
            const body = e.querySelector('.node-info-body');
            return { dialog: rect(e), body: { ...rect(body), client: body.clientHeight, scroll: body.scrollHeight, width: body.clientWidth, scrollWidth: body.scrollWidth, overflowY: getComputedStyle(body).overflowY },
              actions: [...e.querySelectorAll('.node-info-actions button, .modal-header button')].map(rect),
              document: { client: document.documentElement.clientWidth, scroll: document.documentElement.scrollWidth } };
          });
          geometry.push({ case: 'responsive', mode, width, sidebar, ...box });
          assert(box.dialog.x >= 0 && box.dialog.right <= width + 1 && box.dialog.y >= 0 && box.dialog.bottom <= 845);
          assert(box.body.scrollWidth <= box.body.width + 1); assert(box.document.scroll <= box.document.client + 1);
          assert(['auto', 'scroll'].includes(box.body.overflowY));
          if (width <= 390) {
            assert(box.body.scroll > box.body.client, 'mobile body scrolls independently of fixed actions');
            for (const action of box.actions) assert(action.width >= 44 && action.height >= 44 && action.bottom <= 845, 'mobile actions >=44px and visible');
            const badge = await h.badge().boundingBox(); assert(badge.width >= 44 && badge.height >= 44);
          }
          await h.dialog().locator('.node-info-body').evaluate(e => { e.scrollTop = e.scrollHeight; });
          if (width <= 390) assert(await h.dialog().locator('.node-info-body').evaluate(e => e.scrollTop > 0));
          await h.page.screenshot({ path: `artifacts/project-node-info-${mode}-${width}-dialog.png`, animations: 'disabled' });
          await h.close(); await h.unchanged(before);
        }
      }
      await h.open(); await h.dialog().getByRole('button', { name: '节点设置', exact: true }).click();
      await h.dialog().waitFor({ state: 'hidden' });
      await h.page.locator('.devices-page').waitFor();
      // Mobile keeps the manual connection details collapsed; the shared node
      // badge remains observable without expanding the form or reconnecting.
      await h.state('online');
    }, { longName: true });
    await test('15 hanging health times out, clears loading and rejects its late success', async h => {
      await h.select(); const before = await h.snapshot(); await h.open();
      const auth = h.count('auth'), held = h.gate('health beyond production timeout', 22000);
      h.policy.healthGate = held; await h.refresh(); await until(() => held.entered, 'hanging health entered');
      await h.state('checking');
      await h.state('offline', 'rpi-work', 18000);
      assert.equal(held.released, false, 'production timeout, not test gate, ended the request');
      assert.equal(await h.value('节点认证').innerText(), '未知（当前未确认）');
      assert.equal(h.count('auth'), auth, 'timed-out health cannot advance to auth');
      assert.equal(await h.dialog().getByRole('button', { name: '刷新节点状态', exact: true }).isDisabled(), false);
      held.release(); await until(() => held.delivered, 'late health success delivered'); await delay(200);
      await h.state('offline'); assert.equal(h.count('auth'), auth);
      await h.refreshTo('online'); await h.close(); await h.unchanged(before);
    });
  } catch (error) {
    errors.push({ name: 'suite setup', error: error.stack || String(error) }); console.error(error);
  } finally {
    clearTimeout(suiteDeadline); if (browser) await browser.close(); save();
  }
  if (errors.length) process.exitCode = 1;
  else console.log(`PASS project node info: ${results.length} cases; zero prompts/model/thinking/queue/credential writes`);
})();
