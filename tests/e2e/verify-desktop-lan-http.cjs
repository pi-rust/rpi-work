// Actual Windows desktop owner HTTP acceptance, deliberately NOT a GUI/QR rendering test.
// No CDP, prompts, queue/model APIs, real credentials, or standalone work-server.
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const net = require('node:net');
const http = require('node:http');
const dgram = require('node:dgram');
const crypto = require('node:crypto');
const { spawn, spawnSync } = require('node:child_process');

(async () => {
  const root = path.resolve(__dirname, '../..');
  const prefix = path.join(root, 'artifacts/desktop-lan-http');
  fs.mkdirSync(path.dirname(prefix), { recursive: true });
  const report = {
    scope: 'Real work-ui.exe embedded HTTP server, same-machine private routed IPv4 and loopback; not a second-device/firewall acceptance test',
    gui: { verified: false, reason: 'No CDP or UI Automation used; native dialog visibility, QR rendering and camera scanning are NOT verified' },
    safety: 'Random env-only test password; isolated temporary data/profile; credential-free rpc-fixture; only health/auth/projects requests; no existing processes stopped',
    checks: [], launches: [], cleanup: [], failed: false,
  };
  let stage = 'setup', directory, owned;
  const binary = path.join(root, 'target/debug/work-ui.exe');
  const fixture = path.join(root, 'target/debug/rpc-fixture.exe');
  const quote = value => "'" + String(value).replace(/'/g, "''") + "'";
  const ps = script => {
    const result = spawnSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', "$ErrorActionPreference='Stop'; " + script],
      { windowsHide: true, timeout: 20000, encoding: 'utf8' });
    if (result.status !== 0) throw new Error('PowerShell operation failed');
    return result.stdout.trim(); // Never invoke with bearer material or print diagnostics.
  };
  const check = (value, label) => { if (!value) { report.failedCheck = label; throw new Error('Check failed'); } };
  const pass = label => { report.checks.push(label); console.log('PASS', label); };
  const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
  const port = () => new Promise((resolve, reject) => {
    const server = net.createServer(); server.once('error', reject);
    server.listen(0, '0.0.0.0', () => { const p = server.address().port; server.close(() => resolve(p)); });
  });
  // Match production's UDP route selection: connect only, no datagram sent.
  const routedIP = () => new Promise((resolve, reject) => {
    const socket = dgram.createSocket('udp4');
    socket.once('error', error => { socket.close(); reject(error); });
    socket.connect(9, '192.0.2.1', () => { const ip = socket.address().address; socket.close(); resolve(ip); });
  });
  const request = (base, pathname, { method = 'GET', headers = {}, body } = {}) => new Promise((resolve, reject) => {
    const payload = body === undefined ? undefined : JSON.stringify(body);
    const req = http.request(new URL(pathname, base), { method, headers: {
      Connection: 'close', ...payload === undefined ? {} : { 'Content-Type': 'application/json', 'Content-Length': Buffer.byteLength(payload), 'X-RPI-Web': '1' }, ...headers,
    } }, res => {
      let text = '';
      res.setEncoding('utf8'); res.on('data', data => { text += data; if (text.length > 1024 * 1024) req.destroy(); });
      res.once('error', reject);
      res.on('end', () => resolve({ status: res.statusCode, headers: res.headers, json: () => JSON.parse(text) }));
    });
    req.setTimeout(2500, () => req.destroy(new Error('HTTP timeout')));
    req.once('error', reject); if (payload !== undefined) req.write(payload); req.end();
  });
  const snapshot = () => JSON.parse(ps('@(Get-CimInstance Win32_Process | Select-Object -ExpandProperty ProcessId) | ConvertTo-Json -Compress'));
  // Every close/kill rechecks PID + exact executable + creation time + parent.
  // No taskkill /T, process-name kills, or actions against pre-existing processes.
  const identityGuard = record => `$p=Get-CimInstance Win32_Process -Filter 'ProcessId = ${record.child.pid}'; if($p -and $p.ExecutablePath -eq ${quote(binary)} -and $p.ParentProcessId -eq ${process.pid} -and $p.CreationDate.ToUniversalTime().Ticks.ToString() -eq ${quote(record.created)})`;
  const stop = async () => {
    if (!owned) return;
    const record = owned;
    if (record.child.exitCode === null && record.child.signalCode === null) {
      check(record.created, 'cannot clean up unverified process');
      ps(identityGuard(record) + ` { $w=Get-Process -Id ${record.child.pid}; $null=$w.CloseMainWindow(); 'owned-close' } else { throw 'ownership mismatch' }`);
      const deadline = Date.now() + 15000;
      while (record.child.exitCode === null && record.child.signalCode === null && Date.now() < deadline) await delay(100);
      if (record.child.exitCode === null && record.child.signalCode === null) {
        ps(identityGuard(record) + ` { Stop-Process -Id ${record.child.pid} -Force; 'owned-stop' } else { throw 'ownership mismatch' }`);
        const end = Date.now() + 5000;
        while (record.child.exitCode === null && record.child.signalCode === null && Date.now() < end) await delay(100);
      }
    }
    check(record.child.exitCode !== null || record.child.signalCode !== null, 'owned process did not stop');
    report.cleanup.push({ pid: record.child.pid, stopped: true });
    owned = null;
  };
  const launch = async (name, extra = {}) => {
    stage = name;
    const apiPort = await port();
    const before = new Set(snapshot());
    const env = { ...process.env };
    for (const key of Object.keys(env)) {
      if (/^(RPI_|WORK_|WEBVIEW2_)/i.test(key) || /(?:API_KEY|ACCESS_TOKEN|AUTH_TOKEN|SECRET|PASSWORD|CREDENTIAL)/i.test(key)) delete env[key];
    }
    Object.assign(env, {
      RPI_WORK_RPI_BIN: fixture, RPI_WORK_DATA_DIR: path.join(directory, 'node'),
      RPI_WORK_NODE_URL: 'rpi-work://local', RPI_WORK_HTTP_PORT: String(apiPort),
      RPI_WORK_HTTP_PASSWORD: crypto.randomBytes(32).toString('hex'),
      USERPROFILE: path.join(directory, 'profile'), HOME: path.join(directory, 'profile'),
      LOCALAPPDATA: path.join(directory, 'local'), APPDATA: path.join(directory, 'roaming'),
      WEBVIEW2_USER_DATA_FOLDER: path.join(directory, 'webview'), ...extra,
    });
    const password = env.RPI_WORK_HTTP_PASSWORD;
    const child = spawn(binary, [], { cwd: directory, env, stdio: 'ignore', windowsHide: false });
    child.on('error', () => {});
    owned = { child };
    check(child.pid && !before.has(child.pid), 'new desktop PID required');
    owned.created = ps(`$p=Get-CimInstance Win32_Process -Filter 'ProcessId = ${child.pid}'; if(!$p -or $p.ExecutablePath -ne ${quote(binary)} -or $p.ParentProcessId -ne ${process.pid}) { throw 'ownership mismatch' }; $p.CreationDate.ToUniversalTime().Ticks.ToString()`);
    const loop = `http://127.0.0.1:${apiPort}`;
    let health;
    const deadline = Date.now() + 30000;
    while (Date.now() < deadline && child.exitCode === null) {
      try { const res = await request(loop, '/api/v1/health'); if (res.status === 200) { health = res.json(); break; } } catch {}
      await delay(150);
    }
    check(health?.ok && health.service === 'rpi-work-desktop', 'desktop HTTP health ready');
    stage = name + ': isolated node identity';
    const nodeId = fs.readFileSync(path.join(directory, 'node/node-id'), 'utf8').trim();
    check(health.nodeId === nodeId, 'health identity matches fresh isolated data');
    check(!before.has(child.pid), 'isolated identity was created by a newly owned process');
    const netstat = spawnSync('netstat.exe', ['-ano', '-p', 'tcp'], { encoding: 'utf8', windowsHide: true, timeout: 10000 });
    const observed = netstat.stdout.split(/\r?\n/).map(line => line.trim().split(/\s+/)).filter(columns => columns[0] === 'TCP' && columns[1]?.endsWith(`:${apiPort}`) && columns[3] === 'LISTENING').map(columns => ({ LocalAddress: columns[1], OwningProcess: Number(columns[4]) }));
    // Some execution environments expose a different TCP compartment to
    // netstat/CIM. Do not mistake an empty OS enumeration for ownership proof.
    if (observed.length) check(observed.every(listener => listener.OwningProcess === child.pid), 'observed listener belongs to spawned desktop');
    report.launches.push({ name, pid: child.pid, port: apiPort, bind: extra.RPI_WORK_HTTP_BIND || 'unset (production default)',
      ownershipProof: 'Fresh isolated node-id matches HTTP health; PID/executable/parent/creation time verified',
      osListenerObserved: observed.length > 0 });
    return { loop, apiPort, password, nodeId, health };
  };
  const cookieFrom = response => {
    const cookie = response.headers['set-cookie']?.[0]?.split(';')[0];
    check(typeof cookie === 'string' && cookie.length > 10, 'authentication cookie required');
    return cookie;
  };
  try {
    check(process.platform === 'win32', 'Windows desktop required');
    check(fs.existsSync(binary) && fs.existsSync(fixture), 'prebuilt desktop and fixture required');
    directory = fs.mkdtempSync(path.join(os.tmpdir(), 'desktop-lan-http-'));
    for (const folder of ['node', 'profile', 'local', 'roaming']) fs.mkdirSync(path.join(directory, folder));
    const preference = path.join(directory, 'node/sharing.json');
    const ip = await routedIP();
    check(/^(10\.|192\.168\.|172\.(1[6-9]|2\d|3[01])\.)/.test(ip), 'private default-route IPv4 required (no loopback substitution)');
    report.routedPrivateIPv4 = ip;
    check(!fs.existsSync(preference), 'first run has no sharing.json');
    let service = await launch('default bind, missing preference, no assets');
    const lan = `http://${ip}:${service.apiPort}`;
    const health = await request(lan, '/api/v1/health');
    check(health.status === 200 && health.json().sharingEnabled === true && health.json().nodeId === service.nodeId && service.health.sharingEnabled === true, 'LAN and loopback default sharing health');
    check((await request(lan, '/.well-known/rpi-work-entry')).status === 404, 'no hosted assets');
    pass('Unset bind / absent sharing preference / no assets: real private-route LAN and loopback health, sharingEnabled=true, hosted entry=404');
    if (process.env.WORK_VERIFY_NATIVE_ICON === '1') {
      const helper = path.join(root, 'tests/support/verify-native-brand.ps1');
      report.nativeIcon = JSON.parse(ps(identityGuard(owned) + ` { try { & ${quote(helper)} -Executable ${quote(binary)} -WindowProcessId ${owned.child.pid} } catch { @{ error=$_.Exception.Message } | ConvertTo-Json -Compress } } else { throw 'ownership mismatch' }`));
      check(report.nativeIcon.executable?.brandSamplesMatch && report.nativeIcon.window?.brandSamplesMatch, 'executable and live window match the Web brand');
      pass('Actual executable shell icon and owned native window HICON match the Web brand (no WebView/QR pixels inspected)');
    }
    // Optional accessibility smoke: only fixed labels/counts leave the window,
    // never text trees, QR pixels, payloads or credentials. CDP is not required.
    try {
      await delay(2000);
      const ui = JSON.parse(ps(`Add-Type -AssemblyName UIAutomationClient; $w=Get-Process -Id ${owned.child.pid}; $r=[System.Windows.Automation.AutomationElement]::FromHandle($w.MainWindowHandle); $scope=[System.Windows.Automation.TreeScope]::Descendants; $d=New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty,'手机 App 扫码授权'); $i=New-Object System.Windows.Automation.AndCondition((New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty,'手机 App 一次性授权二维码')),(New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ControlTypeProperty,[System.Windows.Automation.ControlType]::Image))); @{dialogCount=$r.FindAll($scope,$d).Count; qrImages=$r.FindAll($scope,$i).Count} | ConvertTo-Json -Compress`));
      report.gui = { verified: ui.dialogCount > 0 && ui.qrImages > 0, ...ui,
        scope: 'Native accessibility fixed-label observation only; no payload decoding, pixel validation or Android scanning' };
      if (report.gui.verified) pass('Actual desktop accessibility tree exposes the automatic authorization dialog and QR image before Settings (no decoded-payload/camera claim)');
    } catch { report.gui = { verified: false, reason: 'Native accessibility smoke unavailable; QR rendering not verified by this HTTP test' }; }
    check((await request(lan, '/api/v1/health', { headers: { Host: 'wrong.invalid' } })).status === 403, 'wrong Host rejected');
    check((await request(lan, '/api/v1/health', { headers: { Origin: 'http://wrong.invalid' } })).status === 403, 'wrong Origin rejected');
    check((await request(lan, '/api/v1/projects')).status === 401, 'anonymous projects rejected');
    check((await request(lan, '/api/v1/auth/qr')).status === 401, 'anonymous GET cannot issue ticket');
    check((await request(lan, '/api/v1/auth/qr', { method: 'POST', body: {} })).status === 401, 'anonymous POST cannot issue ticket');
    pass('Wrong Host/Origin=403; anonymous projects and QR issuance=401');
    const login = await request(lan, '/api/v1/auth', { method: 'POST', body: { password: service.password } });
    check(login.status === 200, 'password login succeeds');
    const cookie = cookieFrom(login);
    const projects = await request(lan, '/api/v1/projects', { headers: { Cookie: cookie } });
    check(projects.status === 200 && Array.isArray(projects.json().projects), 'authenticated projects schema');
    check((await request(lan, '/api/v1/auth/qr', { headers: { Cookie: cookie } })).status === 405, 'GET is not QR issuance route');
    const issued = await request(lan, '/api/v1/auth/qr', { method: 'POST', headers: { Cookie: cookie }, body: {} });
    check(issued.status === 200, 'POST QR issuance');
    const grant = issued.json();
    check(grant.nodeId === service.nodeId && /^[a-f0-9]{64}$/.test(grant.ticket), 'ticket schema and owner identity');
    const redeem = () => request(lan, '/api/v1/auth/qr/redeem', { method: 'POST', body: { ticket: grant.ticket } });
    const redeemed = await redeem();
    check(redeemed.status === 200, 'ticket redemption succeeds');
    const redeemedCookie = cookieFrom(redeemed);
    check((await request(lan, '/api/v1/projects', { headers: { Cookie: redeemedCookie } })).status === 200, 'fresh redeemed session accesses projects');
    check((await redeem()).status === 401, 'ticket replay rejected');
    pass('Password POST yields working cookie; authenticated GET QR=405, POST issues real ticket; fresh redemption works once and replay=401 (HTTP only)');
    await stop();

    fs.writeFileSync(preference, JSON.stringify({ allow_sharing: false }));
    for (let i = 0; i < 2; i++) {
      service = await launch(`saved sharing=false restart ${i + 1}`);
      const base = `http://${ip}:${service.apiPort}`;
      check(service.health.sharingEnabled === false, 'saved false remains disabled');
      check((await request(base, '/api/v1/health')).json().sharingEnabled === false, 'LAN sees disabled policy');
      for (const headers of [{}, { Cookie: cookie }, { Cookie: redeemedCookie }]) {
        const denied = await request(base, '/api/v1/projects', { headers });
        check(denied.status === 403 && denied.json().code === 'sharing_disabled', 'actual service rejects disabled sharing');
      }
      check((await request(base, '/api/v1/auth', { method: 'POST', body: { password: service.password } })).status === 403, 'disabled policy rejects login');
      await delay(1200);
      check((await request(base, '/api/v1/health')).json().sharingEnabled === false, 'startup does not auto-enable');
      check(JSON.parse(fs.readFileSync(preference, 'utf8')).allow_sharing === false, 'saved false not overwritten');
      await stop();
    }
    pass('Saved false survives two real restarts: actual LAN service returns sharing_disabled/403, including prior cookies; startup never auto-enables');

    fs.writeFileSync(preference, JSON.stringify({ allow_sharing: true }));
    service = await launch('explicit loopback bind', { RPI_WORK_HTTP_BIND: '127.0.0.1' });
    check(service.health.sharingEnabled === true, 'explicit loopback health');
    const loopbackOnly = await request(service.loop, '/api/v1/health');
    check(loopbackOnly.status === 200, 'explicit loopback remains healthy');
    let lanStatus = null;
    try { lanStatus = (await request(`http://${ip}:${service.apiPort}`, '/api/v1/health')).status; } catch {}
    check(lanStatus === null || lanStatus === 403, 'explicit loopback never exposes a usable LAN API');
    await stop();
    pass('Explicit 127.0.0.1 configuration keeps loopback health but no usable advertised LAN API (connection rejected or Host denied); OS socket scope and GUI not claimed');

    for (const invalid of ['{', '{"allow_sharing":true,"unexpected":true}']) {
      fs.writeFileSync(preference, invalid);
      service = await launch('invalid preference fails closed');
      check(service.health.sharingEnabled === false, 'invalid preference health disabled');
      const denied = await request(`http://${ip}:${service.apiPort}`, '/api/v1/projects');
      check(denied.status === 403 && denied.json().code === 'sharing_disabled', 'invalid preference actual service fails closed');
      check(fs.readFileSync(preference, 'utf8') === invalid, 'invalid preference not silently replaced');
      await stop();
    }
    pass('Malformed JSON and unknown-field preference both fail closed in the actual desktop HTTP service');
  } catch {
    report.failed = true; report.failedStage = stage; process.exitCode = 1;
    console.error('FAIL desktop LAN HTTP:', stage); // No error objects, response bodies or secrets.
  } finally {
    try { await stop(); } catch {
      report.failed = true; report.cleanupFailed = true; process.exitCode = 1;
      console.error('FAIL ownership-checked desktop cleanup; no unverified process terminated');
    }
    if (directory && !owned) {
      try { fs.rmSync(directory, { recursive: true, force: true, maxRetries: 10, retryDelay: 250 }); report.temporaryDataRemoved = true; }
      catch { report.temporaryDataRemoved = false; report.failed = true; process.exitCode = 1; }
    }
    fs.writeFileSync(`${prefix}-validation.json`, JSON.stringify(report, null, 2));
    console.log('Report: artifacts/desktop-lan-http-validation.json');
    console.log('GUI/native QR rendering: NOT VERIFIED');
  }
})().catch(() => { console.error('FAIL desktop LAN HTTP setup'); process.exitCode = 1; });
