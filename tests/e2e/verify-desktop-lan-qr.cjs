// Windows native WebView2 + actual embedded work-node-service, no standalone service.
// Isolated data/profile, random API/debug ports, credential-free RPC fixture.
// Never print/save authorization payloads, DOM, traces or unmasked QR screenshots.
const fs = require('node:fs');
const path = require('node:path');
const net = require('node:net');
const { spawn, spawnSync } = require('node:child_process');
const { chromium } = require('../support/browser.cjs');

(async () => {
  if (process.platform !== 'win32') throw new Error('Requires Windows WebView2 desktop build');
  const root = path.resolve(__dirname, '../..');
  const binary = path.join(root, 'target/debug/work-ui.exe');
  const fixture = path.join(root, 'target/debug/rpc-fixture.exe');
  for (const file of [binary, fixture]) if (!fs.existsSync(file)) throw new Error('Build work-ui and rpc-fixture first');
  const directory = fs.mkdtempSync(path.join(root, 'artifacts/desktop-lan-qr-native-'));
  const results = [], errors = [];
  let stage = 'setup', child, browser, lastPage, stdout = '', secret = '', failed = false, failure;
  const check = (condition, label) => { if (!condition) throw new Error(label); };
  const pass = label => { results.push(label); console.log('PASS', label); };
  const port = () => new Promise((resolve, reject) => {
    const listener = net.createServer(); listener.on('error', reject);
    listener.listen(0, '127.0.0.1', () => { const number = listener.address().port; listener.close(() => resolve(number)); });
  });
  const stop = async () => {
    if (child && child.exitCode === null) {
      // Only this exact subprocess/binary; WM_CLOSE lets the owner drain normally.
      const expected = binary.replace(/'/g, "''");
      spawnSync('powershell.exe', ['-NoProfile', '-Command', `$p=Get-Process -Id ${child.pid} -ErrorAction SilentlyContinue; if($p -and $p.Path -eq '${expected}') { $null=$p.CloseMainWindow() }`], { windowsHide: true, timeout: 10000 });
      const deadline = Date.now() + 15000;
      while (child.exitCode === null && Date.now() < deadline) await new Promise(resolve => setTimeout(resolve, 100));
      if (child.exitCode === null) { child.kill(); await new Promise(resolve => child.once('exit', resolve)); }
    }
    await browser?.close().catch(() => {}); browser = null; child = null;
  };
  const launch = async extra => {
    const apiPort = await port(), debugPort = await port();
    const env = { ...process.env };
    for (const key of Object.keys(env)) if (/^RPI_WORK_HTTP_|^RPI_HTTP_PASSWORD$|^WEBVIEW2_|^RPI_WORK_NODE_URL$/.test(key)) delete env[key];
    for (const folder of ['profile', 'local', 'roaming', 'node']) fs.mkdirSync(path.join(directory, folder), { recursive: true });
    Object.assign(env, {
      RPI_WORK_RPI_BIN: fixture, RPI_WORK_DATA_DIR: path.join(directory, 'node'),
      RPI_WORK_NODE_URL: 'rpi-work://local', RPI_WORK_HTTP_PORT: String(apiPort),
      LOCALAPPDATA: path.join(directory, 'local'), APPDATA: path.join(directory, 'roaming'),
      USERPROFILE: path.join(directory, 'profile'), HOME: path.join(directory, 'profile'),
      WEBVIEW2_USER_DATA_FOLDER: path.join(directory, 'webview'),
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${debugPort}`,
      ...extra,
    });
    stdout = '';
    child = spawn(binary, [], { cwd: directory, env, stdio: ['ignore', 'pipe', 'pipe'] });
    child.stdout.on('data', value => { stdout += value; }); child.stderr.on('data', value => { stdout += value; });
    let ready = false;
    for (let attempts = 0; attempts < 150 && child.exitCode === null; attempts++) {
      try { ready = (await fetch(`http://127.0.0.1:${debugPort}/json/version`, { signal: AbortSignal.timeout(500) })).ok; } catch {}
      if (ready) break;
      await new Promise(resolve => setTimeout(resolve, 100));
    }
    if (!ready) {
      const expectedDirectory = directory.replace(/'/g, "''");
      const probe = spawnSync('powershell.exe', ['-NoProfile', '-Command', `$items=Get-CimInstance Win32_Process | Where-Object {$_.CommandLine -like '*${expectedDirectory}*' -and $_.Name -eq 'msedgewebview2.exe'}; @($items | ForEach-Object { @{ name=$_.Name; parent=$_.ParentProcessId; hasDebugPort=$_.CommandLine.Contains('--remote-debugging-port=${debugPort}'); browserProcess=($_.CommandLine -notlike '*--type=*') } }) | ConvertTo-Json -Compress`], { windowsHide: true, timeout: 10000, encoding: 'utf8' });
      try { failure = { browserProcesses: JSON.parse(probe.stdout) }; } catch {}
    }
    check(ready, 'isolated native WebView debugger did not become ready');
    browser = await chromium.connectOverCDP(`http://127.0.0.1:${debugPort}`);
    const context = browser.contexts()[0];
    let page;
    for (let attempt = 0; attempt < 50; attempt++) {
      page = context.pages().find(candidate => candidate.url() !== 'about:blank');
      if (page) break;
      await new Promise(resolve => setTimeout(resolve, 100));
    }
    check(page, 'native target missing'); page.setDefaultTimeout(15000);
    lastPage = page;
    page.on('pageerror', () => errors.push(1));
    await page.getByRole('button', { name: '打开设置', exact: true }).waitFor();
    return { page, apiPort };
  };
  const settings = async page => {
    await page.getByRole('button', { name: '打开设置', exact: true }).click();
    await page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '执行节点', exact: true }).click();
  };
  try {
    stage = 'native startup default offer';
    let { page, apiPort } = await launch();
    const dialog = page.getByRole('dialog', { name: '手机 App 扫码授权', exact: true });
    await dialog.locator('#device-auth-qr-code svg').waitFor();
    check(await page.locator('dialog[open]').count() === 1, 'one startup modal');
    check(await page.locator('.devices-page').count() === 0, 'startup does not need Settings');
    await page.addScriptTag({ content: fs.readFileSync(path.join(root, 'assets/vendor/jsQR.js'), 'utf8') });
    const grant = await dialog.locator('svg').evaluate(async svg => {
      const clone = svg.cloneNode(true); clone.setAttribute('width', '768'); clone.setAttribute('height', '768');
      const url = URL.createObjectURL(new Blob([clone.outerHTML], { type: 'image/svg+xml' }));
      try {
        const image = new Image(); image.src = url; await image.decode();
        const canvas = document.createElement('canvas'); canvas.width = canvas.height = 768;
        const ctx = canvas.getContext('2d'); ctx.drawImage(image, 0, 0, 768, 768);
        return JSON.parse(window.jsQR(ctx.getImageData(0, 0, 768, 768).data, 768, 768)?.data || '{}');
      } finally { URL.revokeObjectURL(url); }
    });
    secret = grant.ticket;
    check(grant.type === 'rpi-work-node-auth' && grant.v === 1 && typeof secret === 'string', 'actual native QR is an App authorization');
    const address = new URL(grant.address);
    check(address.protocol === 'http:' && Number(address.port) === apiPort && /^(10\.|192\.168\.|172\.(1[6-9]|2\d|3[01])\.)/.test(address.hostname), 'actual private LAN API address');
    check((await dialog.locator('.connection-share-source code').innerText()) === grant.address, 'visible target matches QR');
    const health = await (await fetch(`${grant.address}/api/v1/health`, { redirect: 'error' })).json();
    check(health.sharingEnabled && health.nodeId === grant.nodeId, 'advertised LAN health belongs to this actual owner');
    check((await fetch(`${grant.address}/api/v1/projects`)).status === 401, 'default sharing still requires authentication');
    check((await fetch(`${grant.address}/.well-known/rpi-work-entry`)).status === 404, 'API-only listener never claims hosted browser UI');
    check((await fetch(`http://127.0.0.1:${apiPort}/api/v1/health`)).ok, 'loopback dev-proxy authority remains allowed');
    const redeem = () => fetch(`${grant.address}/api/v1/auth/qr/redeem`, {
      method: 'POST', headers: { 'Content-Type': 'application/json', 'X-RPI-Web': '1' },
      body: JSON.stringify({ ticket: secret }), redirect: 'error',
    });
    const response = await redeem();
    check(response.ok, 'real server accepts the native-generated ticket');
    const cookie = response.headers.get('set-cookie')?.split(';')[0];
    check(cookie && (await fetch(`${grant.address}/api/v1/projects`, { headers: { Cookie: cookie } })).ok, 'redeemed session accesses the same workspace');
    check((await redeem()).status === 401, 'ticket cannot redeem twice');
    check(!stdout.includes(secret), 'ticket never enters desktop output');
    check(await page.evaluate(ticket => !document.body.innerText.includes(ticket) && !JSON.stringify(Object.entries(localStorage)).includes(ticket), secret), 'ticket not visible text or persisted');
    await page.screenshot({ path: path.join(root, 'artifacts/desktop-lan-qr-native-masked.png'), mask: [page.locator('#device-auth-qr-code')] });
    pass('Native desktop startup defaults to one LAN App authorization QR without Web assets or opening Settings; actual HTTP redemption and one-time authentication succeed');

    stage = 'manual singleton and sharing off';
    await dialog.getByRole('button', { name: '关闭授权码', exact: true }).click();
    await settings(page);
    await page.getByRole('button', { name: '手机 App 扫码授权', exact: true }).click();
    await dialog.locator('#device-auth-qr-code svg').waitFor();
    check(await page.locator('#device-auth-qr').count() === 1, 'Settings reuses singleton');
    await dialog.getByRole('button', { name: '关闭授权码', exact: true }).click();
    await page.getByRole('switch', { name: '允许共享这台电脑' }).click();
    check((await (await fetch(`${grant.address}/api/v1/health`)).json()).sharingEnabled === false, 'explicit off reaches actual HTTP policy');
    check((await fetch(`${grant.address}/api/v1/projects`, { headers: { Cookie: cookie } })).status === 403, 'sharing off revokes authenticated access');
    const preference = JSON.parse(fs.readFileSync(path.join(directory, 'node/sharing.json'), 'utf8'));
    check(preference.allow_sharing === false, 'off is persisted');
    await stop();
    pass('Settings reuses one global authorization instance; explicit sharing off is persisted and revokes the redeemed session');

    stage = 'saved off restart';
    ({ page, apiPort } = await launch());
    await settings(page);
    check(await page.getByRole('switch', { name: '允许共享这台电脑' }).getAttribute('aria-checked') === 'false', 'saved off survives restart');
    check(await page.locator('#device-auth-qr').count() === 0, 'saved off has no auto authorization');
    await stop();
    pass('Native restart preserves saved sharing-off and does not auto-enable or display a QR');

    stage = 'explicit loopback restart';
    fs.writeFileSync(path.join(directory, 'node/sharing.json'), JSON.stringify({ allow_sharing: true }));
    ({ page, apiPort } = await launch({ RPI_WORK_HTTP_BIND: '127.0.0.1' }));
    await settings(page);
    const auth = page.getByRole('button', { name: '手机 App 扫码授权', exact: true });
    check(await auth.isDisabled(), 'explicit loopback has no fake LAN authorization target');
    check(await page.locator('#device-auth-qr').count() === 0, 'no automatic loopback QR');
    check((await fetch(`http://127.0.0.1:${apiPort}/api/v1/health`)).ok, 'explicit loopback listener is healthy');
    await stop();
    pass('Explicit loopback bind remains authoritative and never produces a misleading LAN QR');
    check(errors.length === 0, 'no native application JavaScript errors');
  } catch (error) {
    failed = true; process.exitCode = 1;
    // Record only source locations and fixed UI state, never exception bodies,
    // DOM snapshots or decoded bearer material.
    failure = {
      ...failure,
      type: error.name,
      locations: String(error.stack).split('\n').slice(1).filter(line => line.includes('verify-desktop-lan-qr.cjs')),
      native: child && { exitCode: child.exitCode, signalCode: child.signalCode, outputBytes: stdout.length,
        windowInitialized: stdout.includes('window initialized'),
        panic: stdout.includes('panicked at'),
        loaderFailure: stdout.includes('not found') || stdout.includes('could not'),
      },
      ui: await lastPage?.evaluate(() => ({
        dialogCount: document.querySelectorAll('dialog[open]').length,
        authorizationDialog: !!document.querySelector('#device-auth-qr'),
        authorizationSvg: !!document.querySelector('#device-auth-qr-code svg'),
        pending: document.querySelector('#device-auth-qr [role=status]')?.textContent === '正在验证节点并签发授权…',
        verificationFailed: !!document.querySelector('#device-auth-qr .connection-form-error')?.textContent,
        settingsMounted: !!document.querySelector('.devices-page'),
      })).catch(() => null),
    };
    console.error(`FAIL desktop LAN QR: ${stage}`); // No error bodies or QR diagnostics.
  } finally {
    await stop();
    fs.writeFileSync(path.join(root, 'artifacts/desktop-lan-qr-native-validation.json'), JSON.stringify({
      scope: 'Actual Windows desktop WebView2 + embedded work-node-service; isolated profile/data; credential-free RPC executable; no prompts/models; not Android-device acceptance',
      results, failed, errorCount: errors.length, failedStage: failed ? stage : null, failure,
    }, null, 2));
  }
})().catch(() => { console.error('FAIL desktop LAN QR setup'); process.exitCode = 1; });
