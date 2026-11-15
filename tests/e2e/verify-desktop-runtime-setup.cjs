// Windows WebView2 acceptance with a private profile and private user/runtime
// directories. The missing-runtime case downloads the official GitHub release.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const net = require('node:net');
const { spawn, spawnSync } = require('node:child_process');
const { chromium } = require('../support/browser.cjs');
const { desktopPreflight } = require('../support/windows-desktop-preflight.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');

(async () => {
  const preflight = desktopPreflight();
  assert(preflight.interactiveDesktop, JSON.stringify(preflight));
  const root = path.resolve(__dirname, '../..');
  const binary = process.env.WORK_UI_DESKTOP_BIN || path.join(root, 'target/debug/work-ui.exe');
  assert(fs.existsSync(binary), 'build the native work-ui binary first');
  const directory = fs.mkdtempSync(path.join(root, 'artifacts/runtime-setup-native-'));
  const results = [], errors = [];
  const fixture = await startFixture();
  let child, browser, page, logs = '';
  const wait = ms => new Promise(resolve => setTimeout(resolve, ms));
  const pass = message => { results.push(message); console.log('PASS', message); };
  const until = async (check, timeout = 20000) => {
    const deadline = Date.now() + timeout;
    while (Date.now() < deadline) {
      if (await check()) return;
      if (child?.exitCode !== null) throw new Error(`Desktop exited\n${logs}`);
      await wait(100);
    }
    throw new Error(`Timed out waiting for native desktop\n${logs}`);
  };
  const port = () => new Promise((resolve, reject) => {
    const listener = net.createServer();
    listener.once('error', reject);
    listener.listen(0, '127.0.0.1', () => {
      const number = listener.address().port;
      listener.close(() => resolve(number));
    });
  });
  const stop = async () => {
    if (child && child.exitCode === null) {
      const expected = binary.replace(/'/g, "''");
      spawnSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command',
        `$p=Get-Process -Id ${child.pid} -ErrorAction SilentlyContinue; if($p -and $p.Path -eq '${expected}') { $null=$p.CloseMainWindow() }`],
      { windowsHide: true, timeout: 10000 });
      const deadline = Date.now() + 20000;
      while (child.exitCode === null && Date.now() < deadline) await wait(100);
      if (child.exitCode === null) {
        child.kill();
        await new Promise(resolve => child.once('exit', resolve));
        throw new Error('Native desktop required forced cleanup');
      }
      assert.equal(child.exitCode, 0, logs);
    }
    await browser?.close().catch(() => {});
    browser = null;
    child = null;
  };
  const launch = async (scope, args, extra = {}) => {
    const data = path.join(directory, scope);
    for (const name of ['home', 'local', 'roaming', 'agent', 'temp', 'cwd', 'cargo']) {
      fs.mkdirSync(path.join(data, name), { recursive: true });
    }
    const env = { ...process.env };
    for (const key of Object.keys(env)) {
      if (/^(RPI_|WORK_|WEBVIEW2_|XDG_|NODE_OPTIONS$|NODE_PATH$)|API.?KEY|TOKEN|SECRET|PASSWORD|CREDENTIAL/i.test(key)) delete env[key];
    }
    const debugPort = await port();
    Object.assign(env, {
      HOME: path.join(data, 'home'), USERPROFILE: path.join(data, 'home'),
      LOCALAPPDATA: path.join(data, 'local'), APPDATA: path.join(data, 'roaming'),
      CARGO_HOME: path.join(data, 'cargo'), RPI_CODING_AGENT_DIR: path.join(data, 'agent'),
      RPI_WORK_DATA_DIR: path.join(data, 'node'), TEMP: path.join(data, 'temp'), TMP: path.join(data, 'temp'),
      RPI_WORK_HTTP_BIND: '127.0.0.1', RPI_WORK_HTTP_PORT: '0',
      PATH: [path.join(env.SystemRoot, 'System32'), env.SystemRoot].join(path.delimiter),
      WEBVIEW2_USER_DATA_FOLDER: path.join(data, 'webview'),
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${debugPort}`,
      ...extra,
    });
    logs = '';
    child = spawn(binary, args, { cwd: path.join(data, 'cwd'), env, windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'] });
    child.stdout.on('data', chunk => { logs = (logs + chunk).slice(-20000); });
    child.stderr.on('data', chunk => { logs = (logs + chunk).slice(-20000); });
    try {
      await until(async () => {
        try { return (await fetch(`http://127.0.0.1:${debugPort}/json/version`, { signal: AbortSignal.timeout(500) })).ok; }
        catch { return false; }
      });
    } catch (error) {
      const profile = path.join(data, 'webview').replace(/'/g, "''");
      const proof = spawnSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command',
        `$children=@(Get-CimInstance Win32_Process -Filter "ParentProcessId=${child.pid}"); $w=@(Get-CimInstance Win32_Process -Filter "Name='msedgewebview2.exe'" | Where-Object {$_.CommandLine -and $_.CommandLine.Contains('${profile}')}); @{childNames=@($children.Name); profileProcesses=$w.Count; debugArguments=@($w | ForEach-Object { [regex]::Matches($_.CommandLine,'--remote-debugging-[^\\s"]+') | ForEach-Object {$_.Value} }); listeners=@(Get-NetTCPConnection -State Listen -ErrorAction SilentlyContinue | Where-Object {$_.OwningProcess -in $w.ProcessId} | Select-Object LocalAddress,LocalPort); hasWindow=([bool](Get-Process -Id ${child.pid}).MainWindowHandle)} | ConvertTo-Json -Compress -Depth 4`],
      { windowsHide: true, timeout: 10000, encoding: 'utf8' });
      fs.writeFileSync(path.join(directory, 'desktop-context.json'), proof.stdout || '{}');
      throw error;
    }
    browser = await chromium.connectOverCDP(`http://127.0.0.1:${debugPort}`);
    await until(() => browser.contexts()[0]?.pages().some(candidate => candidate.url() !== 'about:blank'));
    page = browser.contexts()[0].pages().find(candidate => candidate.url() !== 'about:blank');
    page.setDefaultTimeout(20000);
    page.on('pageerror', error => errors.push(error.message));
    return data;
  };
  try {
    const invalid = await launch('invalid', [], { RPI_WORK_RPI_BIN: path.join(directory, 'missing-rpi.exe') });
    await page.getByRole('heading', { name: '运行环境需要处理', exact: true }).waitFor();
    await page.getByRole('alert').filter({ hasText: 'missing-rpi.exe' }).waitFor();
    assert(!fs.existsSync(path.join(invalid, 'node/node.lock')));
    await page.screenshot({ path: path.join(directory, 'invalid-path.png') });
    await page.getByRole('button', { name: '连接远程节点或稍后配置', exact: true }).click();
    await page.locator('.collaboration-hub').waitFor();
    await page.getByRole('button', { name: '准备本机运行环境', exact: true }).click();
    await page.getByRole('heading', { name: '运行环境需要处理', exact: true }).waitFor();
    await stop();
    pass('invalid explicit runtime opens recovery, remote setup and reentry without acquiring a node lock');

    const automatic = await launch('automatic', []);
    const guide = page.getByRole('region', { name: '首次使用引导', exact: true });
    await until(async () => {
      const failed = page.getByRole('heading', { name: '运行环境需要处理', exact: true });
      if (await failed.isVisible()) throw new Error(await page.locator('.runtime-setup-card').innerText());
      return guide.isVisible();
    }, 180000);
    const runtime = path.join(automatic, 'local/rpi-work/runtime/rpi.exe');
    assert(fs.existsSync(runtime), 'missing runtime was downloaded and installed into this isolated user directory');
    assert(fs.existsSync(path.join(automatic, 'node/node.lock')));
    const installed = fs.statSync(runtime);
    assert(installed.size > 1000000);
    assert(fs.readdirSync(path.dirname(runtime)).every(name => !name.startsWith('.rpi-')));
    await guide.getByRole('button', { name: '配置模型', exact: true }).waitFor();
    await page.screenshot({ path: path.join(directory, 'installed-and-onboarding.png') });
    await stop();
    pass('missing runtime downloads, verifies, passes an actual RPC probe and reaches model onboarding in one process');

    await launch('automatic', []);
    await page.getByRole('region', { name: '首次使用引导', exact: true }).waitFor({ timeout: 40000 });
    assert.equal(fs.statSync(runtime).mtimeMs, installed.mtimeMs, 'relaunch reuses the installed binary');
    await stop();
    pass('reopening reuses the installed runtime without another download');

    const remote = await launch('remote', ['--connect', fixture.url], {
      RPI_WORK_RPI_BIN: path.join(directory, 'remote-missing-rpi.exe'),
    });
    await page.getByRole('button', { name: '打开设置', exact: true }).waitFor();
    assert.equal(await page.locator('.runtime-setup').count(), 0);
    assert(!fs.existsSync(path.join(remote, 'node')));
    assert(!fs.existsSync(path.join(remote, 'local/rpi-work/runtime')));
    await stop();
    pass('explicit remote launches bypass local detection, installation and ownership');
    assert.deepEqual(errors, []);
    fs.writeFileSync(path.join(directory, 'validation.json'), JSON.stringify({ results, errors, preflight }, null, 2));
  } catch (error) {
    if (page && !page.isClosed()) await page.screenshot({ path: path.join(directory, 'failure.png') }).catch(() => {});
    fs.writeFileSync(path.join(directory, 'validation.json'), JSON.stringify({ results, errors, fatal: error.message, logs }, null, 2));
    throw error;
  } finally {
    await stop();
    await fixture.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
