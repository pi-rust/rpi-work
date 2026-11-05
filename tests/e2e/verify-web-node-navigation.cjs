// Isolated REAL owners, real security and (optionally) two real dev-web proxies.
// Build a fresh formal bundle first; this script never builds/deploys or uses models.
// WORK_OWNER_FIXTURE=<desktop_fixture> WORK_RPC_FIXTURE=<rpc-fixture>
// WORK_UI_BUNDLE=<fresh bundle> WORK_OWNER_PROXY=0|1 node tests/e2e/verify-web-node-navigation.cjs
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { randomUUID, randomBytes } = require('node:crypto');
const { spawn } = require('node:child_process');
const readline = require('node:readline');
const { chromium } = require('../support/browser.cjs');
const runDevWeb = require('../support/run-navigation-dev-web.cjs');

async function bounded(promise, label, ms = 20000) {
  let timer;
  try {
    return await Promise.race([promise, new Promise((_, reject) => {
      timer = setTimeout(() => reject(new Error(`${label} timeout`)), ms);
    })]);
  } finally { clearTimeout(timer); }
}

async function ownedFetch(url, options = {}) {
  return fetch(url, { ...options, redirect: 'error', signal: AbortSignal.timeout(10000) });
}
const withoutFragment = url => { const parsed = new URL(url); parsed.hash = ''; return parsed.href; };

(async () => {
  assert(process.env.WORK_OWNER_FIXTURE, 'Set WORK_OWNER_FIXTURE to the built desktop_fixture');
  assert(process.env.WORK_RPC_FIXTURE, 'Set WORK_RPC_FIXTURE to the built RPC fixture');
  assert(process.env.WORK_UI_BUNDLE, 'Set WORK_UI_BUNDLE to a freshly built formal Web bundle');
  const assets = path.resolve(process.env.WORK_UI_BUNDLE);
  assert(fs.existsSync(path.join(assets, 'index.html')), 'Web bundle index.html must exist');
  const proxyMode = process.env.WORK_OWNER_PROXY === '1';
  const mode = proxyMode ? 'proxy' : 'direct';
  const reportDir = path.resolve(process.env.WORK_ENTRY_REPORT_DIR || 'artifacts/web-node-navigation');
  fs.mkdirSync(reportDir, { recursive: true });
  // Reserve with wx: reruns never overwrite any prior report (including failures).
  let reportPath = path.join(reportDir, `${mode}-navigation-validation.json`);
  let reportFd;
  for (;;) {
    try { reportFd = fs.openSync(reportPath, 'wx'); break; }
    catch (error) {
      if (error.code !== 'EEXIST') throw error;
      reportPath = path.join(reportDir, `${mode}-navigation-validation-${randomUUID()}.json`);
    }
  }
  const results = [], errors = [], unexpected = [], blockedUpdates = [], apiRequests = [];
  const navigations = [], securityProbes = [], cleanupErrors = [], screenshots = [], owners = [];
  let browser, failure;
  const runId = randomUUID();
  const pass = text => { results.push(text); console.log('PASS', text); };
  const cookieLimitation = '127.0.0.1 cookies are host-scoped, not port-scoped: the browser may send all same-host cookies to both listeners. Distinct nodeId cookie names plus independent server session stores enforce authentication; this test does NOT claim zero wire-level Cookie forwarding. No application credential/token copying or cross-origin API/login is permitted.';
  const redact = value => {
    let text = String(value);
    for (const owner of owners) text = text.split(owner.password).join('[REDACTED]');
    return text;
  };
  function startOwner(label) {
    const owner = {
      label, nodeId: `navigation-${label}-${runId}`,
      password: `fixture-${label}-${randomBytes(18).toString('hex')}`,
      pending: new Map(), seq: 0, stderr: '',
    };
    owners.push(owner); // Register ownership before awaiting/spawning anything else.
    const child = owner.child = spawn(process.env.WORK_OWNER_FIXTURE,
      [process.env.WORK_RPC_FIXTURE, proxyMode ? '--web-proxy' : assets], {
        env: { ...process.env, WORK_FIXTURE_NODE_ID: owner.nodeId, WORK_FIXTURE_PASSWORD: owner.password },
        stdio: ['pipe', 'pipe', 'pipe'],
      });
    child.stderr.on('data', data => { owner.stderr += redact(data); });
    let resolveReady, rejectReady;
    owner.ready = new Promise((resolve, reject) => { resolveReady = resolve; rejectReady = reject; });
    // Avoid unhandled rejections when the other owner fails first.
    owner.ready.catch(() => {});
    owner.exit = new Promise(resolve => {
      child.once('exit', (code, signal) => {
        owner.exitCode = code; owner.signal = signal;
        const error = new Error(`${label} owner exited ${code}`);
        rejectReady(error);
        for (const pending of owner.pending.values()) pending.reject(error);
        owner.pending.clear(); resolve(code);
      });
      child.once('error', error => { rejectReady(error); owner.spawnError = error.message; resolve(null); });
    });
    child.stdin.on('error', error => {
      rejectReady(error);
      for (const pending of owner.pending.values()) pending.reject(error);
      owner.pending.clear();
    });
    owner.lines = readline.createInterface({ input: child.stdout });
    owner.lines.on('line', line => {
      try {
        const value = JSON.parse(line);
        if (value.ready) resolveReady(value);
        else if (owner.pending.has(value.id)) {
          owner.pending.get(value.id).resolve(value.result); owner.pending.delete(value.id);
        }
      } catch (error) { rejectReady(error); }
    });
    owner.control = async data => {
      const id = ++owner.seq;
      try {
        return await bounded(new Promise((resolve, reject) => {
          owner.pending.set(id, { resolve, reject });
          child.stdin.write(JSON.stringify({ ...data, id }) + '\n');
        }), `${label} control`);
      } finally { owner.pending.delete(id); }
    };
    return owner;
  }
  async function setup(owner) {
    owner.node = await bounded(owner.ready, `${owner.label} readiness`);
    assert.equal(owner.node.nodeId, owner.nodeId);
    assert.equal(owner.node.webProxy, proxyMode);
    owner.backendUrl = owner.node.url;
    const backend = new URL(owner.backendUrl);
    assert.equal(backend.hostname, '127.0.0.1');
    assert(Number(backend.port) > 0, 'OS-assigned loopback port');
    owner.webUrl = owner.backendUrl;
    if (proxyMode) {
      assert.equal((await ownedFetch(owner.backendUrl)).status, 404, 'API-only backend has no assets');
      owner.proxy = runDevWeb({ backendUrl: owner.backendUrl, assets });
      owner.webUrl = await bounded(owner.proxy.ready, `${owner.label} proxy readiness`);
      assert.notEqual(new URL(owner.webUrl).origin, backend.origin);
    }
    owner.origin = new URL(owner.webUrl).origin;
    const metadata = await ownedFetch(owner.webUrl + '.well-known/rpi-work-entry');
    assert.equal(metadata.status, 200);
    assert.deepEqual(await metadata.json(), { version: 1, mode: 'hosted', nodeId: owner.nodeId });
    assert.equal((await ownedFetch(owner.webUrl)).status, 200);
    owner.title = `Isolated ${owner.label} owner ${runId}`;
    await owner.control({ op: 'rename', title: owner.title });
  }
  async function securityProbe(target, foreignOrigin, layer) {
    // Out-of-browser negative checks, separately reported; browser has NO cross-origin API exception.
    for (const method of ['GET', 'OPTIONS']) {
      const response = await ownedFetch(target + 'api/v1/health', {
        method, headers: { Origin: foreignOrigin }, signal: AbortSignal.timeout(10000),
      });
      const acao = response.headers.get('access-control-allow-origin');
      securityProbes.push({ target: new URL(target).origin, foreignOrigin, layer, method, status: response.status, acao });
      assert.equal(response.status, 403, `${layer} rejects foreign Origin before auth`);
      assert.equal(acao, null, 'real security never supplies permissive ACAO');
    }
  }
  try {
    const a = startOwner('A'), b = startOwner('B');
    await setup(a); await setup(b);
    assert.notEqual(a.origin, b.origin);
    assert.notEqual(a.nodeId, b.nodeId);
    assert.equal(a.node.sessionId, b.node.sessionId, 'same session ID deliberately collides across isolated owners');
    assert.notEqual(a.title, b.title);
    for (const [target, foreign] of [[a, b], [b, a]]) {
      await securityProbe(target.webUrl, foreign.origin, 'public-web');
      if (proxyMode) await securityProbe(target.backendUrl, new URL(foreign.backendUrl).origin, 'real-owner');
    }
    pass('two isolated native owners expose distinct node IDs and titles, same sessionId, real cross-origin 403/noACAO');
    browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
    async function newPage(width) {
      const context = await browser.newContext({ viewport: { width, height: 1000 },
        isMobile: width < 500, hasTouch: width < 500, serviceWorkers: 'block' });
      const page = await context.newPage();
      await context.addInitScript(() => {
        localStorage.setItem('rpi-work-appearance-v1', JSON.stringify({ theme: 'codex', mode: 'system', density: 'standard', toolRendering: 'simple', customThemes: [] }));
      });
      page.setDefaultTimeout(15000);
      const state = { context, page, expectedNavigation: null, api: [], navigation: [] };
      page.on('pageerror', error => errors.push(redact(error.message)));
      await context.routeWebSocket('**/*', socket => {
        unexpected.push({ width, kind: 'websocket', origin: new URL(socket.url()).origin });
        socket.close(); // No server connection is ever opened.
      });
      await context.route(/^https?:\/\//, async route => {
        const req = route.request(), url = new URL(req.url());
        let frame;
        try { frame = req.frame(); } catch { /* never authorize worker-origin traffic */ }
        const documentOrigin = frame && /^https?:/.test(frame.url()) ? new URL(frame.url()).origin : null;
        const top = req.isNavigationRequest() && frame === page.mainFrame();
        if (top) {
          // Fragments never travel in HTTP; validate the explicit URL's network portion here.
          const allowed = state.expectedNavigation && req.url() === withoutFragment(state.expectedNavigation);
          const record = { width, from: documentOrigin, to: url.origin, path: url.pathname, allowed: Boolean(allowed) };
          navigations.push(record); state.navigation.push(record);
          if (allowed) { state.expectedNavigation = null; return route.continue(); }
        } else if (url.origin === documentOrigin && [a.origin, b.origin].includes(url.origin)) {
          if (url.pathname.startsWith('/api/')) {
            const record = { width, documentOrigin, origin: url.origin, method: req.method(), path: url.pathname };
            state.api.push(record); apiRequests.push(record);
            if (url.pathname === '/api/v1/auth' && req.method() === 'POST') {
              let submitted;
              try { submitted = req.postDataJSON(); } catch { /* rejected below */ }
              const own = url.origin === a.origin ? a : b;
              // B is deliberately tested once with A's password and must reject it.
              if (!submitted || !(submitted.password === own.password || (own === b && submitted.password === a.password))) {
                unexpected.push({ width, origin: url.origin, path: url.pathname, kind: 'unexpected-auth-body' });
                return route.abort('blockedbyclient');
              }
            }
          }
          return route.continue();
        }
        if (req.method() === 'GET' && req.url() === 'https://api.github.com/repos/pi-rust/rpi-work/releases/latest') {
          blockedUpdates.push({ width, origin: documentOrigin, url: req.url() });
        } else unexpected.push({ width, documentOrigin, origin: url.origin, method: req.method(), path: url.pathname });
        return route.abort('blockedbyclient');
      });
      state.goto = async url => { state.expectedNavigation = new URL(url).href; await page.goto(url); };
      state.clickNavigation = async (button, url) => {
        state.expectedNavigation = new URL(url).href;
        await Promise.all([page.waitForEvent('framenavigated', {
          predicate: frame => frame === page.mainFrame() && state.expectedNavigation === null
            && withoutFragment(frame.url()) === withoutFragment(url),
        }), button.click()]);
        await page.waitForLoadState('load');
        assert.equal(state.expectedNavigation, null, 'top-level navigation consumed explicit user transition');
      };
      state.reload = async () => { state.expectedNavigation = page.url(); await page.reload(); };
      return state;
    }
    const workspaceReads = entries => entries.filter(req => !/^\/api\/v1\/(auth(?:\/|$)|health$)/.test(req.path));
    async function gate(state, owner, start = 0) {
      const { page } = state;
      await page.getByLabel('宿主密码', { exact: true }).waitFor();
      await page.waitForTimeout(400);
      assert.equal(new URL(page.url()).origin, owner.origin);
      assert.equal(await page.getByRole('button', { name: '登录宿主', exact: true }).count(), 1);
      assert.equal(await page.getByRole('button', { name: '打开设置', exact: true }).count(), 0);
      assert.equal(await page.locator('.project-entry,.session-entry,#scanner-placeholder,#node-token').count(), 0);
      assert.equal(await page.getByRole('button', { name: '返回入口节点', exact: true }).count(), 0);
      const address = page.locator('#phone-node-address');
      if (await address.count()) {
        assert.equal(await address.inputValue(), owner.origin);
        assert.notEqual(await address.getAttribute('readonly'), null, 'HostedLogin stays fixed to serving node');
      }
      assert.deepEqual(workspaceReads(state.api.slice(start).filter(req => req.origin === owner.origin)), [], 'no workspace reads until this owner authenticates');
    }
    async function login(state, owner) {
      const { page } = state;
      await page.getByLabel('宿主密码', { exact: true }).fill(owner.password);
      await page.getByRole('button', { name: '登录宿主', exact: true }).click();
      await page.locator(`.project-entry[data-project-id="${owner.node.projectId}"]`).waitFor({ state: 'attached' });
      assert.equal(new URL(page.url()).origin, owner.origin);
    }
    async function openSettings(page) {
      const open = page.getByRole('button', { name: '打开设置', exact: true });
      const toggle = page.getByRole('button', { name: '打开项目导航', exact: true });
      if (await toggle.isVisible() && !/\bopen\b/.test(await page.locator('#project-sidebar').getAttribute('class'))) await toggle.click();
      await open.click();
      await page.getByRole('navigation', { name: '设置分类' })
        .getByRole('button', { name: /^(连接我的电脑|执行节点)$/ }).click();
      await page.locator('.phone-current-node').waitFor();
    }
    async function assertTitle(state, own, other) {
      const { page } = state;
      const nav = page.getByRole('button', { name: '打开项目导航', exact: true });
      if (await nav.isVisible()) await nav.click();
      const project = page.locator(`.project-entry[data-project-id="${own.node.projectId}"]`);
      await project.waitFor();
      const toggle = project.locator('.project-toggle');
      if (await toggle.getAttribute('aria-expanded') !== 'true') await toggle.click();
      const session = page.locator(`.session-entry[data-session-id="${own.node.sessionId}"]`);
      await session.getByText(own.title, { exact: true }).waitFor();
      assert.equal(await page.getByText(other.title, { exact: true }).count(), 0, 'same sessionId never leaks other owner data');
    }
    async function capture(state, width, name) {
      for (const theme of ['light', 'dark']) {
        await state.page.emulateMedia({ colorScheme: theme });
        await state.page.waitForFunction(theme => document.documentElement.dataset.mode === theme, theme);
        assert(await state.page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1), 'no horizontal page overflow');
        const file = `${mode}-${width}-${theme}-${name}-${runId}.png`;
        await state.page.screenshot({ path: path.join(reportDir, file), animations: 'disabled' });
        screenshots.push(file);
      }
    }
    const home = state => state.page.evaluate(() => sessionStorage.getItem('rpi-work-web-home-v1'));
    async function homeOrigin(state, expected) {
      // Storage may wrap its version, but its only address must be a bare origin.
      const stored = await home(state);
      assert(stored, 'return hint survives in this tab');
      let value; try { value = JSON.parse(stored); } catch { value = stored; }
      if (typeof value === 'object') {
        assert(value && !Array.isArray(value));
        const strings = Object.values(value).filter(item => typeof item === 'string');
        assert.deepEqual(strings, [expected], 'storage contains only origin, never credentials/path/query');
        assert(Object.values(value).every(item => item === expected || item === 1), 'no nested data, tokens or credentials in home storage');
      } else assert.equal(value, expected);
      assert(!stored.includes(a.password) && !stored.includes(b.password));
    }
    // Untrusted fragments are only hints: fresh hosted authentication always wins.
    for (const hint of [b.origin, 'javascript:alert(1)', `http://user:secret@127.0.0.1:${new URL(b.origin).port}`, b.origin + '/?password=forged']) {
      const state = await newPage(390);
      try {
        await state.goto(a.webUrl + '#rpi-entry=' + encodeURIComponent(hint));
        await gate(state, a);
        assert.equal(state.navigation.length, 1, 'forged return hint never navigates automatically');
        if (hint !== b.origin) await homeOrigin(state, a.origin); // Invalid hint falls back to the serving origin.
        else await homeOrigin(state, b.origin);
      } finally { await state.context.close(); }
    }
    pass('forged and malformed return fragments never replace the initial serving-node password gate or auto-navigate');
    for (const width of [1440, 390, 320]) {
      const state = await newPage(width), { page } = state;
      try {
        await state.goto(a.webUrl);
        await gate(state, a);
        await login(state, a);
        await assertTitle(state, a, b);
        await openSettings(page);
        await page.locator('.phone-current-node').getByRole('button', { name: '连接其他电脑', exact: true }).click();
        await page.getByLabel('电脑节点地址', { exact: true }).waitFor();
        assert.equal(await page.locator('input[type=password]').count(), 0, 'navigation form never asks for B credentials on A');
        const box = await page.locator('#phone-manual-connection').boundingBox();
        assert(box && box.x >= -1 && box.x + box.width <= width + 1, 'shared address form fits this viewport');
        assert.equal(await page.locator('.workspace-location').count(), 1, 'address form preserves mounted workspace');
        await page.locator('#phone-manual-connection').scrollIntoViewIfNeeded();
        await capture(state, width, 'select-node');
        const beforeInvalid = state.navigation.length;
        for (const invalid of ['javascript:alert(1)', 'file:///tmp/fixture',
          `http://user:secret@127.0.0.1:${new URL(b.origin).port}`, b.origin + '/?token=forged']) {
          await page.getByLabel('电脑节点地址', { exact: true }).fill(invalid);
          await page.getByRole('button', { name: '连接节点', exact: true }).click();
          await page.getByRole('alert').filter({ hasText: /.+/ }).first().waitFor();
          assert.equal(page.url(), a.webUrl);
          assert.equal(state.navigation.length, beforeInvalid, 'invalid input causes no navigation, even blocked attempts');
        }
        const beforeB = state.api.length;
        await page.getByLabel('电脑节点地址', { exact: true }).fill(b.origin);
        const bEntry = b.webUrl + '#rpi-entry=' + encodeURIComponent(a.origin);
        await state.clickNavigation(page.getByRole('button', { name: '连接节点', exact: true }), bEntry);
        await gate(state, b, beforeB);
        await homeOrigin(state, a.origin);
        assert(!state.api.slice(beforeB).some(req => req.method === 'POST'), 'navigation does not copy credentials or log into B');
        // Same-host A cookie is present, but B's node-scoped security still requires B's own password.
        await page.getByLabel('宿主密码', { exact: true }).fill(a.password);
        await page.getByRole('button', { name: '登录宿主', exact: true }).click();
        await page.getByRole('alert').filter({ hasText: '登录失败' }).waitFor();
        await gate(state, b, beforeB);
        await login(state, b);
        await assertTitle(state, b, a);
        const cookies = await state.context.cookies();
        const cookieNames = cookies.map(cookie => cookie.name).filter(name => name.startsWith('rpi_work_desktop_'));
        assert.equal(new Set(cookieNames).size, 2, 'nodeId yields independent cookie names (not port isolation)');
        await openSettings(page);
        await page.locator('.connection-home code').waitFor();
        assert.equal((await page.locator('.connection-home code').textContent()).trim(), a.origin);
        assert.equal(await page.locator('.phone-current-node').getByRole('button', { name: '返回入口节点', exact: true }).count(), 1);
        const beforeReload = state.api.length;
        await state.reload();
        await page.locator(`.project-entry[data-project-id="${b.node.projectId}"]`).waitFor({ state: 'attached' });
        await assertTitle(state, b, a);
        await homeOrigin(state, a.origin);
        assert.equal(new URL(page.url()).origin, b.origin, 'stored return origin never becomes the default connection');
        assert(!state.api.slice(beforeReload).some(req => req.method === 'POST' && req.path === '/api/v1/auth'));
        await openSettings(page);
        await page.locator('.connection-home code').waitFor();
        assert.equal((await page.locator('.connection-home code').textContent()).trim(), a.origin);
        const beforeLogout = state.api.length;
        await page.locator('.connection-home').scrollIntoViewIfNeeded();
        await capture(state, width, 'return-node');
        await page.getByRole('button', { name: '退出并清除登录', exact: true }).click();
        await gate(state, b, beforeLogout);
        await login(state, b);
        await openSettings(page);
        assert.equal((await page.locator('.connection-home code').textContent()).trim(), a.origin);
        const beforeReturn = state.api.length;
        await state.clickNavigation(page.locator('.phone-current-node').getByRole('button', { name: '返回入口节点', exact: true }), a.webUrl);
        await page.locator(`.project-entry[data-project-id="${a.node.projectId}"]`).waitFor({ state: 'attached' });
        await assertTitle(state, a, b);
        assert.equal(await page.getByLabel('宿主密码', { exact: true }).count(), 0);
        assert(!state.api.slice(beforeReturn).some(req => req.path === '/api/v1/auth' && req.method === 'POST'), 'B logout never revokes A; its cookie autoentry requires no new login');
        // Reproduce real expiry/revocation while this App still believes it is online.
        const logout = await page.request.delete(a.webUrl + 'api/v1/auth', { headers: { 'x-rpi-web': '1' } });
        assert.equal(logout.status(), 200);
        assert.equal((await logout.json()).authenticated, false);
        await page.getByRole('button', { name: '添加项目', exact: true }).first().click();
        await page.getByRole('button', { name: '浏览…', exact: true }).click();
        await page.getByRole('button', { name: '连接或登录节点', exact: true }).click();
        const reauthenticate = page.getByRole('button', { name: '重新登录当前节点', exact: true });
        await reauthenticate.waitFor();
        assert.equal(await page.locator('input[type=password]').count(), 0, 'expired browser auth recovers at its own gate, not a hidden-password form');
        const beforeRecovery = state.api.length;
        await state.clickNavigation(reauthenticate, a.webUrl);
        await gate(state, a, beforeRecovery);
        await login(state, a);
        await assertTitle(state, a, b);
        assert(state.api.every(req => req.origin === req.documentOrigin), 'every browser API is serving-document same-origin');
        const catalogA = await a.control({ op: 'catalog' }), catalogB = await b.control({ op: 'catalog' });
        assert.equal(catalogA.sessions.find(session => session.sessionId === a.node.sessionId).title, a.title);
        assert.equal(catalogB.sessions.find(session => session.sessionId === b.node.sessionId).title, b.title);
        pass(`${width}px: A login → B independent auth/data → reload preserves return hint → B logout/relogin keeps A authorized → explicit return A → real revoked-A auth recovers at same-origin gate`);
      } finally { await state.context.close(); }
    }
    assert.deepEqual(unexpected, [], 'all unapproved navigation, external traffic and cross-origin APIs are blocked and fail the run');
    assert.deepEqual(errors, []);
    assert(apiRequests.length > 0 && apiRequests.every(req => req.documentOrigin === req.origin));
    pass('browser APIs/login remain on their own Web origin, with no direct-backend proxy bypass or credential-copy path');
  } catch (error) { failure = redact(error.stack || error); }
  finally {
    try { if (browser) await browser.close(); } catch (error) { cleanupErrors.push(redact(error)); }
    for (const owner of owners.slice().reverse()) {
      try { if (owner.proxy) await bounded(owner.proxy.stop(), `${owner.label} proxy shutdown`); }
      catch (error) { cleanupErrors.push(redact(error)); }
      try {
        if (owner.child.pid && owner.child.exitCode === null && !owner.child.stdin.destroyed) {
          owner.child.stdin.end(JSON.stringify({ op: 'shutdown' }) + '\n');
        }
        const code = await bounded(owner.exit, `${owner.label} owner drain`, 15000);
        if (!owner.spawnError) assert.equal(code, 0, 'owner cooperatively drains HTTP and native workspace');
      } catch (error) {
        cleanupErrors.push(redact(error));
        if (owner.child.pid && owner.child.exitCode === null) {
          owner.child.kill('SIGKILL');
          try { await bounded(owner.exit, `${owner.label} forced exit`, 5000); } catch (e) { cleanupErrors.push(redact(e)); }
        }
      } finally { owner.lines.close(); }
    }
    const report = { mode, assets, results, failure, errors, unexpected, blockedUpdates,
      apiRequests, navigations, securityProbes, screenshots, cookieLimitation, cleanupErrors,
      scope: 'Fresh formal Web bundle; real isolated native owners; no installed desktop, user data, models or permissive CORS. Sharing fail-closed is covered by verify-desktop-owner-http.cjs, not repeated here.',
      owners: owners.map(owner => ({ label: owner.label, nodeId: owner.nodeId,
        backendUrl: owner.backendUrl, webUrl: owner.webUrl, sessionId: owner.node?.sessionId,
        title: owner.title, exitCode: owner.exitCode, signal: owner.signal,
        spawnError: owner.spawnError, stderr: owner.stderr, proxy: owner.proxy?.diagnostics })),
    };
    try { fs.writeFileSync(reportFd, redact(JSON.stringify(report, null, 2))); }
    finally { fs.closeSync(reportFd); }
    console.log('REPORT', reportPath);
  }
  if (failure || cleanupErrors.length) throw new Error(failure || cleanupErrors.join('\n'));
})().catch(error => { console.error(error); process.exitCode = 1; });
