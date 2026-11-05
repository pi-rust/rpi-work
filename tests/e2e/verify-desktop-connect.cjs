// Windows native --connect acceptance. No model, real agent, user data, API interception or DOM state injection.
// Run from an interactive Windows user terminal: node tests/e2e/verify-desktop-connect.cjs
// Exit 2 + environment-report.json means blocked preflight, NOT a passed UI test.
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const os = require('node:os');
const http = require('node:http');
const net = require('node:net');
const crypto = require('node:crypto');
const { spawn, spawnSync } = require('node:child_process');
const { chromium } = require('../support/browser.cjs');
const { desktopPreflight } = require('../support/windows-desktop-preflight.cjs');
const root = path.resolve(__dirname, '../..');
const output = path.join(root, 'artifacts/headless-node/desktop-connect');
const binaries = Object.fromEntries(['work-ui', 'work-cli', 'work-server-rpc-fixture'].map(n => [n, path.join(root, `target/debug/${n}.exe`)]));
const sleep = ms => new Promise(r => setTimeout(r, ms));
const check = (v) => { if (!v) throw new Error('AcceptanceAssertion'); };
const hash = p => crypto.createHash('sha256').update(fs.readFileSync(p)).digest('hex');
const checks = [], screenshots = [], owners = [], guis = [], servers = [], cleanups = [];
let stage = 'setup', workspace, page, failure, failed = false, guard, dataViolation = false;
const pass = label => { checks.push(label); console.log('PASS', label); };
const until = async (fn, ms = 20000) => { const end = Date.now() + ms; do { if (await fn()) return; await sleep(100); } while (Date.now() < end); throw new Error('DeadlineExceeded'); };
function envFor(dir) {
  const env = { ...process.env };
  for (const k of Object.keys(env)) if (/^(RPI_|WORK_|WEBVIEW2_|XDG_|NODE_OPTIONS$|NODE_PATH$)|API.?KEY|TOKEN|SECRET|PASSWORD|PROXY|CREDENTIAL/i.test(k)) delete env[k];
  for (const n of ['home', 'roaming', 'local', 'agent', 'cwd', 'temp']) fs.mkdirSync(path.join(dir, n), { recursive: true });
  Object.assign(env, { HOME: path.join(dir, 'home'), USERPROFILE: path.join(dir, 'home'), APPDATA: path.join(dir, 'roaming'), LOCALAPPDATA: path.join(dir, 'local'), RPI_CODING_AGENT_DIR: path.join(dir, 'agent'), XDG_DATA_HOME: path.join(dir, 'local'), TMP: path.join(dir, 'temp'), TEMP: path.join(dir, 'temp'), NO_PROXY: '127.0.0.1,localhost' });
  return env;
}
function child(binary, args, env, cwd) {
  const p = spawn(binary, args, { env, cwd, windowsHide: false, stdio: ['pipe', 'pipe', 'pipe'] });
  p.output = ''; p.outputBytes = 0; p.spawnFailure = false;
  p.on('error', () => { p.spawnFailure = true; });
  for (const s of [p.stdout, p.stderr]) s.on('data', b => { p.outputBytes += b.length; p.output = (p.output + b.toString('utf8')).slice(-65536); });
  return p;
}
async function exited(p, ms) { await until(() => p.exitCode !== null || p.signalCode !== null || p.spawnFailure, ms); }
async function cli(args, env) {
  const p = child(binaries['work-cli'], args, env, path.join(workspace, 'owner/cwd'));
  try { await exited(p, 25000); check(p.exitCode === 0); return JSON.parse(p.output); }
  catch (e) { if (p.exitCode === null && !p.signalCode) { p.kill(); await exited(p, 10000); cleanups.push('cli-force-cleanup-failure'); } throw e; }
}
async function startOwner(env) {
  const p = child(binaries['work-cli'], ['serve', '--shutdown-stdin'], env, path.join(workspace, 'owner/cwd'));
  const owner = { p, env }; owners.push(owner);
  let line = ''; p.stdout.on('data', b => { if (!line.includes('\n')) line += b.toString('utf8'); });
  await until(() => line.includes('\n') || p.exitCode !== null, 25000);
  const info = JSON.parse(line.split('\n')[0]);
  check(info.ready && info.nodeId && /^http:\/\/127\.0\.0\.1:\d+\/?$/.test(info.address));
  check(!line.includes(env.RPI_WORK_HTTP_PASSWORD));
  owner.info = info;
  owner.client = { ...env, RPI_WORK_URL: info.address, RPI_WORK_PASSWORD: env.RPI_WORK_HTTP_PASSWORD };
  return owner;
}
async function stopOwner(owner) {
  if (owner.p.exitCode === null && !owner.p.signalCode) owner.p.stdin.end();
  await exited(owner.p, 20000); check(owner.p.exitCode === 0);
}
const find = (o, key) => { if (o && typeof o === 'object') { if (Object.hasOwn(o, key)) return o[key]; for (const v of Object.values(o)) { const r = find(v, key); if (r !== undefined) return r; } } };
async function listener(s) { servers.push(s); await new Promise((r, j) => { s.once('error', j); s.listen(0, '127.0.0.1', r); }); return s.address().port; }
async function endpoint(port, route) { return fetch(`http://127.0.0.1:${port}${route}`, { signal: AbortSignal.timeout(700) }); }
function ps(command) { const r = spawnSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', command], { windowsHide: true, timeout: 15000, encoding: 'utf8' }); check(r.status === 0); return r.stdout.trim(); }
const quote = s => s.replace(/'/g, "''");
function processProof(gui) {
  // No command lines leave PowerShell: report booleans only. PID and expected profile must both belong to this launch.
  const profile = quote(gui.profile), exe = quote(binaries['work-ui']);
  return JSON.parse(ps(`$p=Get-CimInstance Win32_Process -Filter "ProcessId=${gui.p.pid}"; $children=@(Get-CimInstance Win32_Process -Filter "ParentProcessId=${gui.p.pid}" | Where-Object {$_.Name -eq 'msedgewebview2.exe'}); $w=@(Get-CimInstance Win32_Process -Filter "Name='msedgewebview2.exe'" | Where-Object {$_.ParentProcessId -eq ${gui.p.pid} -and $_.CommandLine.Contains('${profile}') -and $_.CommandLine.Contains('--remote-debugging-port=${gui.port}') -and $_.CommandLine -notlike '*--type=*'}); @{ownExe=($p.ExecutablePath -eq '${exe}'); ownWindow=([bool](Get-Process -Id ${gui.p.pid}).MainWindowHandle); ownProfile=($w.Count -eq 1); webViewChild=($children.Count -gt 0); childHasProfile=(@($children | Where-Object {$_.CommandLine.Contains('${profile}')}).Count -gt 0); childHasDebugPort=(@($children | Where-Object {$_.CommandLine.Contains('--remote-debugging-port=${gui.port}')}).Count -gt 0); childHasLoopbackArgument=(@($children | Where-Object {$_.CommandLine.Contains('--remote-debugging-address=127.0.0.1')}).Count -gt 0)} | ConvertTo-Json -Compress`));
}
async function stopGui(gui, requireNormal = false) {
  // Cleanup may encounter an exited child. Acceptance may not mistake a crash
  // or spontaneous exit for the WM_CLOSE action under test.
  if (requireNormal) check(gui.p.exitCode === null && !gui.p.signalCode);
  if (gui.p.exitCode === null && !gui.p.signalCode) {
    const exe = quote(binaries['work-ui']);
    const closed = ps(`$p=Get-Process -Id ${gui.p.pid} -ErrorAction Stop; if($p.Path -ne '${exe}') {exit 3}; [bool]$p.CloseMainWindow()`);
    gui.closeRequested = true; gui.closeReturned = closed === 'True';
    check(gui.closeReturned); await exited(gui.p, 20000); check(gui.p.exitCode === 0);
    gui.normalClose = true;
  }
  if (gui.browser) { await gui.browser.close().catch(() => {}); gui.browser = null; }
}
function noDesktopData() { for (const g of guis) if (fs.existsSync(g.data)) dataViolation = true; check(!dataViolation); }
async function launch(address, name) {
  const dir = path.join(workspace, name), env = envFor(dir);
  const reservation = net.createServer(s => s.destroy()); const debugPort = await listener(reservation);
  const trap = net.createServer(s => { s.destroy(); }); const trapPort = await listener(trap);
  // Sample the reserved debug endpoint before allowing this exact child to bind it.
  let unavailable = false; try { await endpoint(debugPort, '/json/version'); } catch { unavailable = true; } check(unavailable);
  await new Promise(r => reservation.close(r));
  const gui = { port: debugPort, data: path.join(dir, 'must-not-exist'), profile: path.join(dir, 'webview'), normalClose: false };
  Object.assign(env, { RPI_WORK_RPI_BIN: path.join(dir, 'no-such-executor.exe'), RPI_WORK_DATA_DIR: gui.data, RPI_WORK_HTTP_BIND: '127.0.0.1', RPI_WORK_HTTP_PORT: String(trapPort), RPI_WORK_NODE_URL: 'rpi-work://local', WEBVIEW2_USER_DATA_FOLDER: gui.profile, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-address=127.0.0.1 --remote-debugging-port=${debugPort}` });
  gui.p = child(binaries['work-ui'], ['--connect', address], env, path.join(dir, 'cwd')); guis.push(gui);
  try { await until(async () => { check(gui.p.exitCode === null && !gui.p.spawnFailure); try { return (await endpoint(debugPort, '/json/version')).ok; } catch { return false; } }, 25000); }
  catch (e) { try { gui.proof = processProof(gui); } catch { gui.proof = { probeFailed: true }; } throw e; }
  gui.proof = processProof(gui); check(gui.proof.ownExe && gui.proof.ownWindow && gui.proof.ownProfile);
  const targets = await (await endpoint(debugPort, '/json/list')).json();
  check(targets.some(t => t.type === 'page' && /^https?:\/\/dioxus\.index\/?/.test(t.url)));
  gui.browser = await chromium.connectOverCDP(`http://127.0.0.1:${debugPort}`, { timeout: 12000 });
  page = gui.browser.contexts()[0].pages().find(p => /^https?:\/\/dioxus\.index\/?/.test(p.url())); check(page); gui.page = page; page.setDefaultTimeout(12000);
  await page.getByRole('button', { name: '打开设置', exact: true }).waitFor();
  noDesktopData(); return gui;
}
async function settings(p) { await p.getByRole('button', { name: '打开设置', exact: true }).click(); await p.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '执行节点', exact: true }).click(); await p.locator('.devices-page').waitFor(); }
async function screenshot(p, name) {
  const file = path.join(output, `${name}.png`);
  await p.screenshot({ path: file, mask: [p.locator('input'), p.locator('[id*="qr"], .pairing-qr, textarea')] });
  screenshots.push(path.relative(root, file).split(path.sep).join('/'));
}
async function probes(p) { return p?.evaluate(() => ({ settings: !!document.querySelector('.devices-page'), connectionPanel: !!document.querySelector('.connection-panel-shell'), passwordInput: !!document.querySelector('input[type=password]'), errorNonempty: Array.from(document.querySelectorAll('.connection-form-error')).some(e => !!e.textContent.trim()), noLocalButton: !Array.from(document.querySelectorAll('button')).some(b => b.textContent === '使用这台电脑'), noSharingSwitch: !document.querySelector('[role=switch][aria-label="允许共享这台电脑"]'), bodyNonempty: !!document.body.textContent.trim() })).catch(() => null); }
(async () => {
  fs.mkdirSync(output, { recursive: true });
  // Session 0 is not the logged-in desktop. Fail before spawning an owner or
  // window instead of repeating a known-invalid UI acceptance environment.
  const environment = desktopPreflight();
  const preflight = { status: environment.interactiveDesktop ? 'ready-to-test' : 'blocked',
    uiAcceptancePassed: false, environment, scriptSha256: hash(__filename),
    preflightHelperSha256: hash(require.resolve('../support/windows-desktop-preflight.cjs')),
    ownerOrGuiProcessesCreated: false, generatedAtUtc: new Date().toISOString(),
    note: 'Preflight is not UI acceptance. A prior report.json, if present, is historical.' };
  fs.writeFileSync(path.join(output, 'environment-report.json'), JSON.stringify(preflight, null, 2) + '\n');
  if (!environment.interactiveDesktop) {
    console.error('BLOCKED desktop-connect: interactive Windows session required; no owner/window created');
    process.exitCode = 2;
    return;
  }
  const hashes = {};
  try {
    check(process.platform === 'win32'); for (const [n, p] of Object.entries(binaries)) hashes[n] = hash(p);
    workspace = fs.mkdtempSync(path.join(os.tmpdir(), 'rpi-work-desktop-connect-'));
    guard = setInterval(() => { for (const g of guis) if (fs.existsSync(g.data)) dataViolation = true; }, 50);
    stage = 'owner-seed';
    const env = envFor(path.join(workspace, 'owner'));
    Object.assign(env, { RPI_WORK_RPI_BIN: binaries['work-server-rpc-fixture'], RPI_WORK_DATA_DIR: path.join(workspace, 'owner/node'), RPI_WORK_HTTP_BIND: '127.0.0.1', RPI_WORK_HTTP_PORT: '0', RPI_WORK_HTTP_PASSWORD: crypto.randomBytes(32).toString('hex') });
    let owner = await startOwner(env); const nodeId = owner.info.nodeId;
    const projectDir = path.join(workspace, 'owner/project'); fs.mkdirSync(projectDir);
    const projectName = 'Desktop connect isolated project', title = 'Desktop connect active fixture', sessionId = 'desktop-connect-synthetic-session';
    const project = await cli(['project-create', projectName, projectDir], owner.client); check(project.id);
    await stopOwner(owner);
    const registry = path.join(env.RPI_WORK_DATA_DIR, 'rpc/projects', project.id, 'sessions.json'); fs.mkdirSync(path.dirname(registry), { recursive: true });
    fs.writeFileSync(registry, JSON.stringify({ sessions: [{ sessionId, projectId: project.id }] }));
    owner = await startOwner(env); check(owner.info.nodeId === nodeId);
    await cli(['session-rename', sessionId, title], owner.client);
    const state = await cli(['state', sessionId], owner.client), fixturePid = find(state, 'fixturePid'); check(Number.isInteger(fixturePid) && fixturePid > 0);
    check((await cli(['prompt', sessionId, 'headless-owner-hold-fixture'], owner.client)).success);
    const active = await cli(['state', sessionId], owner.client); check(find(active, 'fixturePid') === fixturePid && find(active, 'isRunning') === true);
    pass('real-owner-ready-seeded-active-synthetic-child');
    stage = 'native-connect-launch';
    const gui = await launch(owner.info.address, 'desktop-success');
    pass('native-window-isolated-profile-explicit-connect-no-local-owner');
    stage = 'native-wrong-password'; await settings(page);
    let ui = await probes(page); check(ui.noLocalButton && ui.noSharingSwitch);
    const password = page.locator('input[type=password]:visible').first(); await password.fill('wrong-desktop-fixture-password');
    await page.getByRole('button', { name: '登录', exact: true }).click();
    await until(async () => (await probes(page))?.errorNonempty);
    check(!await page.getByText(projectName, { exact: true }).count());
    await screenshot(page, 'wrong-password-masked'); pass('native-ui-wrong-password-rejected');
    stage = 'native-correct-password';
    await password.fill(env.RPI_WORK_HTTP_PASSWORD); await page.getByRole('button', { name: '登录', exact: true }).click();
    await page.getByText('节点已认证。', { exact: true }).waitFor();
    ui = await probes(page); check(ui.noLocalButton && ui.noSharingSwitch); noDesktopData();
    await screenshot(page, 'remote-settings-masked'); pass('native-ui-correct-password-same-owner-remote-only-settings');
    stage = 'native-project-session-readonly';
    await page.getByRole('button', { name: '关闭设置', exact: true }).click();
    await page.getByText(projectName, { exact: true }).first().click();
    await page.getByText(title, { exact: true }).first().click();
    await screenshot(page, 'active-session-masked'); noDesktopData();
    pass('native-ui-real-project-and-seed-session-visible-opened-readonly');
    stage = 'native-normal-close'; await stopGui(gui, true); noDesktopData();
    check(owner.p.exitCode === null);
    check(find(await cli(['health'], owner.client), 'nodeId') === nodeId);
    const projects = await cli(['projects'], owner.client), sessions = await cli(['sessions'], owner.client);
    check(projects.some(p => p.id === project.id && p.name === projectName)); check(sessions.some(s => s.sessionId === sessionId && s.title === title));
    const after = await cli(['state', sessionId], owner.client); check(find(after, 'fixturePid') === fixturePid && find(after, 'isRunning') === true);
    pass('wm-close-preserves-external-owner-identity-titles-and-same-active-fixture');
    stage = 'native-503-no-fallback';
    let requests = 0;
    const stub = http.createServer((req, res) => { requests++; req.resume(); res.writeHead(503, { 'Content-Type': 'application/json' }); res.end('{"error":"desktop-connect-fixture-unavailable"}'); });
    const stubPort = await listener(stub);
    const failedGui = await launch(`http://127.0.0.1:${stubPort}`, 'desktop-failure');
    await settings(page);
    await until(async () => requests > 0 && (await probes(page))?.errorNonempty);
    const failedUi = await probes(page); check(failedUi.noLocalButton && failedUi.noSharingSwitch); noDesktopData();
    await screenshot(page, '503-failure-masked'); await stopGui(failedGui, true); noDesktopData();
    pass('fresh-native-profile-explicit-503-fails-without-local-fallback');
    check(guis.length === 2 && guis.every(gui => gui.normalClose));
    stage = 'owner-eof'; await stopOwner(owner);
    pass('external-owner-cooperative-stdin-eof-shutdown');
  } catch (e) {
    failed = true; process.exitCode = 1;
    failure = { stage, exceptionName: e.name, sourceLocations: String(e.stack || '').split('\n').slice(1).map(l => l.match(/verify-desktop-connect\.cjs:\d+:\d+/)?.[0]).filter(Boolean), ui: await probes(page), native: guis.map(g => ({ outputBytes: g.p.outputBytes, exitCode: g.p.exitCode, windowInitialized: g.p.output.includes('window initialized'), proof: g.proof || null })) };
    if (page && !page.isClosed()) await screenshot(page, 'failure-masked').catch(() => {});
    console.error('FAIL desktop-connect', stage, e.name);
  } finally {
    clearInterval(guard);
    for (const g of guis.slice().reverse()) {
      try { await stopGui(g); } catch { failed = true; cleanups.push('gui-normal-close-failed'); if (g.p.exitCode === null && !g.p.signalCode) { g.p.kill(); await exited(g.p, 10000).catch(() => {}); cleanups.push('own-gui-force-cleanup-failure'); } await g.browser?.close().catch(() => {}); }
    }
    for (const o of owners.slice().reverse()) try { await stopOwner(o); } catch { failed = true; cleanups.push('owner-eof-failed'); if (o.p.exitCode === null && !o.p.signalCode) { o.p.kill(); await exited(o.p, 10000).catch(() => {}); cleanups.push('own-owner-force-cleanup-failure'); } }
    for (const s of servers) if (s.listening) await new Promise(r => s.close(r));
    try { noDesktopData(); } catch { failed = true; }
    if (workspace) {
      try {
        // Read-only query restricted to this run's unique profile prefix. Never signal WebView processes.
        const absent = ps(`$items=@(Get-CimInstance Win32_Process -Filter "Name='msedgewebview2.exe'" | Where-Object {$_.CommandLine -and $_.CommandLine.Contains('${quote(workspace)}')}); [bool]($items.Count -eq 0)`);
        check(absent === 'True'); cleanups.push('no-own-profile-webview-processes-remain');
      } catch { failed = true; cleanups.push('own-profile-webview-cleanup-unconfirmed'); }
      try { fs.rmSync(workspace, { recursive: true, force: true, maxRetries: 30, retryDelay: 300 }); cleanups.push('temporary-profiles-and-node-data-deleted'); } catch { failed = true; cleanups.push('temporary-profile-removal-failed'); }
    }
    const report = { passlabel: 'windows-native-desktop-connect-e2e', passed: !failed, stage, exceptionName: failure?.exceptionName || null, failure: failure || null, checks, binaryHashes: hashes, scriptHash: hash(__filename), screenshotPaths: screenshots, dataViolation, cleanups, normalWindowCloses: guis.map(g => ({ passed: g.normalClose, wmCloseRequested: !!g.closeRequested, wmCloseReturned: !!g.closeReturned })), boundary: { intendedNativeCoverage: 'Windows work-ui --connect, WebView2 CDP UI input, Rust HTTP client, real work-cli serve owner and CLI requests; only checks listed above actually executed', fixtureOnly: 'work-server-rpc-fixture synthetic session and special hold prompt; owned 503 stub only in failure case', notProven: 'No real rpi, model, tool, ACP, Android or LAN acceptance; CDP request absence is not a native-network assertion' } };
    fs.writeFileSync(path.join(output, 'report.json'), JSON.stringify(report, null, 2) + '\n');
    fs.writeFileSync(path.join(root, 'target/headless-desktop-connect.log'), JSON.stringify({ passlabel: report.passlabel, passed: report.passed, stage, checks, cleanups }) + '\n');
    process.exitCode = failed ? 1 : 0;
  }
})().catch(() => { console.error('FAIL desktop-connect setup'); process.exitCode = 1; });
