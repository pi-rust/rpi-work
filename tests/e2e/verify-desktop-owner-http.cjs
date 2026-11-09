// Formal Web bundle -> direct work-node-service OR real dev-web proxy -> the SAME native LocalWorkspace.
// Only unexpected external HTTP is intercepted, never fixture API responses.
// No installed desktop, fixed ports, user data or models.
const http = require('node:http');
const { spawn } = require('node:child_process');
const path = require('node:path');
const fs = require('node:fs');
const readline = require('node:readline');
const assert = require('node:assert/strict');
const { chromium, request } = require('../support/browser.cjs');
const runDevWeb = require('../support/run-dev-web.cjs');
(async () => {
  assert(process.env.WORK_OWNER_FIXTURE, 'Build work-node-service example and set WORK_OWNER_FIXTURE');
  assert(process.env.WORK_RPC_FIXTURE, 'Set WORK_RPC_FIXTURE to work-server-rpc-fixture binary');
  const proxyMode = process.env.WORK_OWNER_PROXY === '1';
  const mode = proxyMode ? 'proxy' : 'direct';
  const assets = path.resolve(process.env.WORK_UI_BUNDLE || 'target/parity-web');
  const reportDir = path.resolve(process.env.WORK_ENTRY_REPORT_DIR || 'artifacts/desktop-web-entry');
  fs.mkdirSync(reportDir, { recursive: true });
  const evidence = name => path.join(reportDir, `${mode}-${name}`);
  const pending = new Map(), results = [], errors = [], unexpected = [], apiRequests = [];
  const apiContexts = [], blockedUpdates = [], sentinelCalls = [];
  const authorizedContexts = new WeakSet();
  let failure, proxy, webUrl, backendUrl;
  // An owned, random-port other node: never point stale history at a user service.
  const sentinel = http.createServer((req, res) => {
    sentinelCalls.push(`${req.method} ${req.url}`);
    unexpected.push(`sentinel ${req.method} ${req.url}`);
    res.writeHead(503); res.end('Unexpected stale node request');
  });
  await new Promise(resolve => sentinel.listen(0, '127.0.0.1', resolve));
  const other = `http://127.0.0.1:${sentinel.address().port}`;
  const child = spawn(process.env.WORK_OWNER_FIXTURE, [process.env.WORK_RPC_FIXTURE, proxyMode ? '--web-proxy' : assets], { stdio: ['pipe', 'pipe', 'pipe'] });
  let stderr = ''; child.stderr.on('data', b => { stderr += b; });
  let resolveReady, rejectReady;
  const ready = new Promise((resolve, reject) => { resolveReady = resolve; rejectReady = reject; });
  const exit = new Promise(resolve => child.once('exit', code => { rejectReady(new Error(`Fixture exited ${code}: ${stderr}`)); resolve(code); }));
  readline.createInterface({ input: child.stdout }).on('line', line => {
    try { const value = JSON.parse(line); if (value.ready) resolveReady(value); else if (pending.has(value.id)) { pending.get(value.id)(value.result); pending.delete(value.id); } }
    catch (error) { rejectReady(error); }
  });
  const bounded = (promise, ms = 15000) => Promise.race([promise, new Promise((_, reject) => { const t = setTimeout(() => reject(new Error('fixture timeout')), ms); t.unref(); })]);
  let seq = 0, browser;
  const control = data => bounded(new Promise(resolve => { const id = ++seq; pending.set(id, resolve); child.stdin.write(JSON.stringify({ ...data, id }) + '\n'); }));
  const pass = text => { results.push(text); console.log('PASS', text); };
  try {
    const node = await bounded(ready);
    backendUrl = node.url; // Raw backend is reserved for fixture/discovery checks, never browser traffic.
    const backendMetadata = await (await fetch(node.url + '.well-known/rpi-work-entry')).json();
    assert.deepEqual(backendMetadata, { version: 1, mode: 'hosted', nodeId: 'browser-native-owner-fixture' });
    webUrl = node.url;
    if (proxyMode) {
      assert.equal(node.webProxy, true, 'fixture declares no-assets proxy mode');
      assert.equal((await fetch(node.url)).status, 404, 'backend root must not serve any Web assets');
      proxy = runDevWeb({ backendUrl: node.url, assets });
      webUrl = await bounded(proxy.ready);
      assert.notEqual(new URL(webUrl).origin, new URL(node.url).origin, 'Web and backend must use separate listeners');
      pass('no-assets backend root is 404; owned dev-web publishes a separate ephemeral Web origin');
    }
    const origin = new URL(webUrl).origin;
    const metadataResponse = await fetch(webUrl + '.well-known/rpi-work-entry');
    assert.equal(metadataResponse.status, 200);
    assert.deepEqual(await metadataResponse.json(), backendMetadata, 'Web metadata preserves backend version, mode and owner ID');
    assert.equal((await fetch(webUrl)).status, 200, 'Web origin serves the selected bundle');
    pass('Web entry metadata identifies the exact native owner');
    browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
    const newContext = async viewport => {
      const context = await browser.newContext({ viewport, isMobile: viewport.width < 500, hasTouch: viewport.width < 500, serviceWorkers: 'block' });
      await context.route(/^https?:\/\//, route => {
        const url = route.request().url();
        // In proxy mode ONLY the Web listener is allowed. Never whitelist node.url:
        // a direct-backend bypass must fail even though that fixture is owned.
        if (new URL(url).origin === origin) return route.continue();
        // The mounted App checks releases. Block this known request too, but
        // only after successful entry is underway; never permit network egress.
        if (authorizedContexts.has(context) && route.request().method() === 'GET'
          && url === 'https://api.github.com/repos/pi-rust/rpi-work/releases/latest') {
          blockedUpdates.push(url);
        } else unexpected.push(`${route.request().method()} ${url}`);
        return route.abort('blockedbyclient');
      });
      await context.addInitScript(({ other }) => {
        localStorage.setItem('rpi-work-nodes-v1', JSON.stringify({ version: 1, selected: other, recent: [other] }));
      }, { other });
      return context;
    };
    const observe = page => {
      const workspace = [];
      page.setDefaultTimeout(15000);
      page.on('pageerror', e => errors.push(e.message));
      page.on('request', req => {
        const url = new URL(req.url());
        if (url.pathname.startsWith('/api/v1/')) {
          apiRequests.push({ method: req.method(), origin: url.origin, path: url.pathname });
          if (!/^\/api\/v1\/(auth(?:\/|$)|health$)/.test(url.pathname)) workspace.push(url.pathname);
        }
      });
      return workspace;
    };
    const assertNoManualConnect = async page => {
      assert.equal(await page.locator('#node-token, #node-url, #scanner-placeholder').count(), 0);
      assert.equal(await page.getByRole('button', { name: /^(手动输入地址|扫码连接|连接电脑|连接其他节点)$/ }).count(), 0);
      const address = page.locator('#phone-node-address');
      if (await address.count()) {
        assert.equal(await address.inputValue(), origin);
        assert.notEqual(await address.getAttribute('readonly'), null, 'hosted target is immutable');
      }
    };
    // Settings uses these shared labels according to DeviceMode (not viewport alone).
    const openConnectionSettings = async page => {
      const button = page.getByRole('navigation', { name: '设置分类' })
        .getByRole('button', { name: /^(连接我的电脑|执行节点)$/ });
      assert.equal(await button.count(), 1, 'exactly one shared connection settings section');
      await button.click();
    };
    const assertGate = async (page, workspace) => {
      await page.getByLabel('宿主密码', { exact: true }).waitFor();
      await page.waitForTimeout(350); // Observe a settled gate, not just its first render.
      assert.equal(await page.getByRole('button', { name: '登录宿主', exact: true }).count(), 1);
      assert.equal(await page.getByRole('button', { name: '打开设置', exact: true }).count(), 0);
      assert.equal(await page.locator('.project-entry, .session-entry, #scanner-placeholder, #node-token').count(), 0);
      await assertNoManualConnect(page);
      assert.deepEqual(workspace, [], 'no workspace/directory API before authentication');
      assert.deepEqual(sentinelCalls, [], 'stale other-node sentinel receives zero calls');
      assert.deepEqual(unexpected, [], 'never contact stale history or external HTTP');
    };
    const loginUI = async page => {
      authorizedContexts.add(page.context());
      await page.getByLabel('宿主密码', { exact: true }).fill('111111');
      await page.getByRole('button', { name: '登录宿主', exact: true }).click();
      await page.locator(`.project-entry[data-project-id="${node.projectId}"]`).waitFor();
      await assertNoManualConnect(page);
      assert.equal(new URL(page.url()).origin, origin);
    };
    for (const width of [390, 320]) {
      const mobile = await newContext({ width, height: 844 });
      const mobilePage = await mobile.newPage();
      const workspace = observe(mobilePage);
      await mobilePage.goto(webUrl);
      await assertGate(mobilePage, workspace);
      await mobilePage.screenshot({ path: evidence(`entry-owner-${width}.png`) });
      await mobile.close();
      pass(`fresh ${width}px mobile browser shows hosted password gate, not scanner/workspace`);
    }
    // The fixed-host deployment supplies the SAME session controls as native entry.
    const mobileSession = await newContext({ width: 390, height: 844 });
    const mobileSettings = await mobileSession.newPage();
    const mobileWorkspace = observe(mobileSettings);
    await mobileSettings.goto(webUrl);
    await assertGate(mobileSettings, mobileWorkspace);
    await mobileSettings.getByLabel('宿主密码',{exact:true}).fill('111111');
    let releaseLogin, beginLogin;
    const receivedLogin=new Promise(resolve=>{beginLogin=resolve;});
    const delayedLogin=async route=>{
      if(route.request().method()!=='POST') return route.continue();
      await new Promise(resolve=>{releaseLogin=resolve;beginLogin();});
      await route.fulfill({status:200,contentType:'application/json',body:'{"required":true,"authenticated":true}'}).catch(()=>{});
    };
    const authUrl = webUrl + 'api/v1/auth';
    await mobileSettings.route(authUrl,delayedLogin);
    await mobileSettings.getByRole('button',{name:'登录宿主',exact:true}).click();
    await bounded(receivedLogin);
    await mobileSettings.getByRole('button',{name:'取消连接',exact:true}).click();
    releaseLogin();
    await mobileSettings.unroute(authUrl,delayedLogin);
    await mobileSettings.waitForTimeout(200);
    assert.equal(await mobileSettings.getByLabel('宿主密码',{exact:true}).inputValue(),'');
    assert.equal(await mobileSettings.getByRole('button',{name:'打开设置',exact:true}).count(),0);
    pass('shared cancel action clears the password and ignores a late hosted-login result');
    await loginUI(mobileSettings);
    await mobileSettings.locator('.conversation-panel').waitFor({state:'attached'});
    await mobileSettings.getByRole('button',{name:'打开项目导航',exact:true}).click();
    await mobileSettings.getByRole('button',{name:'打开设置',exact:true}).click();
    await openConnectionSettings(mobileSettings);
    await mobileSettings.getByRole('button',{name:'连接其他电脑',exact:true}).click();
    await mobileSettings.getByLabel('电脑节点地址',{exact:true}).waitFor();
    assert.equal(await mobileSettings.locator('input[type=password]').count(),0);
    assert.equal(await mobileSettings.locator('#phone-node-address').getAttribute('readonly'),null);
    assert.equal(await mobileSettings.locator('.phone-current-node').count(),1);
    assert.equal(new URL(mobileSettings.url()).origin,origin);
    pass('mobile-width hosted Web offers node selection without unmounting the workspace or collecting target credentials');
    const failLogout=route=>route.request().method()==='DELETE' ? route.fulfill({status:503,contentType:'application/json',body:'{"error":"fixture unavailable"}'}) : route.continue();
    await mobileSettings.route(authUrl,failLogout);
    await mobileSettings.getByRole('button',{name:'退出并清除登录',exact:true}).click();
    await mobileSettings.getByRole('status').filter({hasText:'未能确认退出'}).waitFor();
    assert.equal(await mobileSettings.locator('.phone-current-node').count(),1);
    await mobileSettings.unroute(authUrl,failLogout);
    await mobileSettings.getByRole('button',{name:'退出并清除登录',exact:true}).click();
    await mobileSettings.getByLabel('宿主密码',{exact:true}).waitFor();
    await mobileSettings.reload();
    await mobileSettings.getByLabel('宿主密码',{exact:true}).waitFor();
    assert.equal(await mobileSettings.getByRole('button',{name:'打开设置',exact:true}).count(),0);
    pass('shared logout keeps the workspace on failure and clears browser login only after authoritative success');
    await mobileSession.close();
    const context = await newContext({ width: 1440, height: 1000 });
    const page = await context.newPage();
    const workspace = observe(page);
    await page.goto(webUrl);
    await assertGate(page, workspace);
    await page.getByLabel('宿主密码', { exact: true }).fill('wrong-password');
    await page.getByRole('button', { name: '登录宿主', exact: true }).click();
    await page.getByRole('alert').filter({ hasText: '登录失败' }).waitFor();
    await assertGate(page, workspace);
    pass('stale other-node history cannot bypass hosted gate; wrong password stays outside with zero workspace API reads');
    await loginUI(page);
    const project = page.locator(`.project-entry[data-project-id="${node.projectId}"]`);
    const toggle = project.locator('.project-toggle');
    if (await toggle.getAttribute('aria-expanded') !== 'true') await toggle.click();
    const session = page.locator(`.session-entry[data-session-id="${node.sessionId}"]`);
    await session.getByText('Shared desktop session', { exact: true }).waitFor();
    assert(workspace.length > 0, 'authenticated App loads workspace from owner automatically');
    pass('hosted password login automatically syncs owner projects/sessions without manual connect despite stale history');
    const beforeReload = apiRequests.length;
    await page.reload();
    await project.waitFor();
    assert.equal(await page.getByLabel('宿主密码', { exact: true }).count(), 0);
    await assertNoManualConnect(page);
    assert(!apiRequests.slice(beforeReload).some(req => req.path === '/api/v1/auth' && req.method === 'POST'), 'valid cookie autoentry never submits another login');
    pass('valid cookie reload mounts authenticated workspace without another password or manual connect');
    // workspace.rs accepts URL session IDs scoped to the serving origin. The
    // fixture's legacy ID deliberately satisfies that public link contract.
    assert.match(node.sessionId, /^[a-zA-Z0-9_-]{1,128}$/);
    await page.goto(webUrl + '?session=' + encodeURIComponent(node.sessionId));
    await project.waitFor();
    if (await toggle.getAttribute('aria-expanded') !== 'true') await toggle.click();
    await session.locator('.session-item[aria-current="true"]').waitFor();
    assert.equal(await page.getByLabel('宿主密码', { exact: true }).count(), 0);
    await assertNoManualConnect(page);
    assert.deepEqual(sentinelCalls, []);
    assert.deepEqual(unexpected, []);
    pass('cookie-authenticated scoped session link selects the serving owner session, ignoring stale other-node history');
    await page.getByRole('button', { name: '打开设置', exact: true }).click();
    await openConnectionSettings(page);
    await page.locator('.phone-current-node [role=status]').filter({ hasText: '在线' }).waitFor();
    assert.equal(await page.locator('#node-token,#node-url').count(), 0);
    assert.equal(await page.locator('input[type=password]').count(), 0);
    assert.equal(new URL(await page.locator('.phone-current-node code').textContent()).origin, origin);
    assert.equal(await page.getByRole('button',{name:'重新连接',exact:true}).count(),0);
    assert.equal(await page.getByRole('button',{name:'连接其他电脑',exact:true}).count(),1);
    pass('wide hosted Web shares the connection screen and exposes explicit node selection after initial authentication');
    const health = await page.request.get(webUrl + 'api/v1/health');
    assert.equal((await health.json()).nodeId, 'browser-native-owner-fixture');
    assert.equal(await page.getByRole('switch', { name: '允许共享这台电脑', exact: true }).count(), 0);
    assert.equal(await page.getByRole('button', { name: '手机 App 扫码授权', exact: true }).isDisabled(), true);
    pass('loopback disables phone QR UI; API checks below do not claim camera or real Android coverage');
    pass('formal Web connects with a password in one step but cannot control desktop sharing');
    await page.getByRole('button', { name: '返回会话', exact: true }).click();
    if (await toggle.getAttribute('aria-expanded') !== 'true') await toggle.click();
    await session.getByText('Shared desktop session', { exact: true }).waitFor();
    await control({ op: 'rename', title: 'Native owner mutation' });
    await session.getByText('Native owner mutation', { exact: true }).waitFor();
    pass('a native owner mutation becomes visible in Web without reload or file copying');
    await session.locator('.session-row-menu').click();
    await page.getByRole('menu', { name: '会话操作', exact: true }).getByRole('menuitem', { name: '重命名', exact: true }).click();
    const dialog = page.getByRole('dialog', { name: '重命名会话', exact: true });
    await dialog.getByLabel('会话名称', { exact: true }).fill('Web mutation in native owner');
    await dialog.getByRole('button', { name: '保存', exact: true }).click();
    await dialog.waitFor({ state: 'hidden' });
    const catalog = await control({ op: 'catalog' });
    assert.equal(catalog.sessions.find(s => s.sessionId === node.sessionId).title, 'Web mutation in native owner');
    const webCatalogResponse = await page.request.get(webUrl + 'api/v1/sessions');
    assert.equal(webCatalogResponse.status(), 200);
    const webCatalog = await webCatalogResponse.json();
    assert.deepEqual(webCatalog.sessions.find(s => s.sessionId === node.sessionId), catalog.sessions.find(s => s.sessionId === node.sessionId), 'Web and native control read the exact same mutated session');
    pass('a real UI rename changes the same in-memory native owner and matching HTTP catalog');
    const location = page.locator('.workspace-location');
    assert.equal(await location.getAttribute('data-node-kind'), 'remote');
    assert.match(await location.innerText(), /远程工作空间/);
    const environment = page.locator('.inspector-environment');
    if (!await environment.isVisible()) await page.getByRole('button', { name: '查看任务上下文', exact: true }).click();
    await environment.waitFor();
    assert.equal(await environment.getAttribute('data-node-kind'), 'remote');
    assert.match(await environment.innerText(), /远程工作空间/);
    assert.match(await environment.innerText(), /远程节点（含局域网）/);
    assert.doesNotMatch(await environment.innerText(), /本机工作空间|云端/);
    pass('navigation and right context distinguish a remote workspace without claiming cloud hosting');
    const freshApi = async () => {
      const api = await request.newContext({ baseURL: webUrl, extraHTTPHeaders: { 'x-rpi-web': '1' } });
      apiContexts.push(api); return api;
    };
    const anonymous = await freshApi();
    assert.equal((await anonymous.get('api/v1/auth/qr')).status(), 401);
    assert.equal((await anonymous.post('api/v1/auth/qr', { data: {} })).status(), 401);
    const issued = await page.request.post(webUrl + 'api/v1/auth/qr', { headers: { 'x-rpi-web': '1' }, data: {} });
    assert.equal(issued.status(), 200);
    const authorization = await issued.json();
    assert.equal(authorization.nodeId, 'browser-native-owner-fixture');
    assert.match(authorization.ticket, /^[a-f0-9]{64}$/);
    const redeemed = await freshApi();
    assert.equal((await redeemed.get('api/v1/auth')).status(), 200);
    assert.equal((await redeemed.get('api/v1/sessions')).status(), 401);
    assert.equal((await redeemed.post('api/v1/auth/qr/redeem', { data: { ticket: authorization.ticket } })).status(), 200);
    const qrSessions = await redeemed.get('api/v1/sessions');
    assert.equal(qrSessions.status(), 200);
    assert((await qrSessions.json()).sessions.some(s => s.sessionId === node.sessionId));
    assert.equal((await anonymous.post('api/v1/auth/qr/redeem', { data: { ticket: authorization.ticket } })).status(), 401);
    assert.equal((await anonymous.get('api/v1/sessions')).status(), 401);
    pass('unauthenticated GET/POST cannot issue QR; authenticated issue redeems in fresh API context, sessions work, replay is 401');
    await control({ op: 'sharing', enabled: false });
    assert.equal((await redeemed.get('api/v1/sessions')).status(), 403);
    const rejected = await page.request.get(webUrl + 'api/v1/sessions');
    assert.equal(rejected.status(), 403);
    assert.equal((await rejected.json()).code, 'sharing_disabled');
    await control({ op: 'rename', title: 'Native owner survives disabled sharing' });
    assert.equal((await control({ op: 'catalog' })).sessions.find(s => s.sessionId === node.sessionId).title, 'Native owner survives disabled sharing');
    pass('turning off consent rejects real HTTP access without stopping the native owner');
    await control({ op: 'sharing', enabled: true });
    const auth = await page.request.get(webUrl + 'api/v1/auth');
    assert.equal((await auth.json()).authenticated, false);
    assert.equal((await page.request.get(webUrl + 'api/v1/sessions')).status(), 401);
    assert.equal((await redeemed.get('api/v1/sessions')).status(), 401);
    pass('sharing off/on revokes the QR-redeemed session as well as browser access');
    workspace.length = 0;
    authorizedContexts.delete(context);
    await page.reload();
    await assertGate(page, workspace);
    await loginUI(page);
    const restored = await page.request.get(webUrl + 'api/v1/sessions');
    assert.equal((await restored.json()).sessions.find(s => s.sessionId === node.sessionId).title, 'Native owner survives disabled sharing');
    pass('reopening sharing requires new login and restores access to the unchanged owner');
    assert.deepEqual(errors, []);
    assert.deepEqual(unexpected, []);
    assert.deepEqual(sentinelCalls, [], 'stale node received zero calls across all regressions');
    assert(apiRequests.every(req => req.origin === origin), 'all browser API traffic stays on the fixed Web origin, never the backend listener');
    pass('all browser API requests stay on the Web origin; stale-node sentinel receives zero calls');
    await page.screenshot({ path: evidence('desktop-owner-http.png'), animations: 'disabled' });
  } catch (error) {
    failure = error.stack || String(error);
    throw error;
  } finally {
    try {
      for (const api of apiContexts) await api.dispose();
      if (browser) await browser.close();
    } finally {
      try {
        if (proxy) await proxy.stop();
      } finally {
        if (child.exitCode === null) child.stdin.end(JSON.stringify({ op: 'shutdown' }) + '\n');
        let fixtureExit;
        try {
          fixtureExit = await bounded(exit);
        } finally {
          if (child.exitCode === null) child.kill('SIGKILL');
          await new Promise(resolve => sentinel.close(resolve));
          const report = { mode, backendUrl, webUrl, assets, results, errors, unexpected, sentinelCalls, blockedUpdates, apiRequests, failure, fixtureExit, stderr, proxy: proxy?.diagnostics };
          fs.writeFileSync(evidence('desktop-owner-http-validation.json'), JSON.stringify(report, null, 2));
          fs.writeFileSync(evidence('entry-owner-browser-report.json'), JSON.stringify(report, null, 2));
        }
        assert.equal(fixtureExit, 0, 'owned fixture cooperatively shuts down cleanly');
      }
    }
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
