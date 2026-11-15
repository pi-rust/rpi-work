// Windows native Work, isolated data/WebView profile and credential-free RPC fixture.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const net = require('node:net');
const { spawn, spawnSync } = require('node:child_process');
const { chromium } = require('../support/browser.cjs');

(async () => {
  const root = path.resolve(__dirname, '../..');
  const binary = path.resolve(process.env.WORK_DESKTOP_BINARY || 'target/access-background-native/debug/work-ui.exe');
  const fixture = path.join(root, 'target/debug/rpc-fixture.exe');
  const directory = fs.mkdtempSync(path.join(root, 'artifacts/independent-access-'));
  const freePort = () => new Promise(resolve => {
    const listener = net.createServer();
    listener.listen(0, '127.0.0.1', () => { const port = listener.address().port; listener.close(() => resolve(port)); });
  });
  const [remotePort, webPort, debugPort] = await Promise.all([freePort(), freePort(), freePort()]);
  const remote = `http://127.0.0.1:${remotePort}/`, web = `http://127.0.0.1:${webPort}/`;
  const env = { ...process.env };
  for (const key of Object.keys(env)) if (/^RPI_WORK_|^RPI_HTTP_PASSWORD$|^WEBVIEW2_/.test(key)) delete env[key];
  for (const folder of ['profile', 'local', 'roaming', 'node', 'project']) fs.mkdirSync(path.join(directory, folder));
  const projectPath = path.join(directory, 'project');
  fs.writeFileSync(path.join(projectPath, '中文 文件.rs'), 'fn native_preview() { println!("works"); }\n');
  Object.assign(env, {
    RPI_WORK_RPI_BIN: fixture, RPI_WORK_DATA_DIR: path.join(directory, 'node'),
    RPI_WORK_NODE_URL: 'rpi-work://local', RPI_WORK_HTTP_BIND: '127.0.0.1',
    RPI_WORK_HTTP_PORT: String(remotePort), RPI_WORK_WEB_LISTEN_PORT: String(webPort),
    RPI_WORK_HTTP_ASSETS: path.resolve(process.env.WORK_UI_BUNDLE || 'target/parity-web'),
    RPI_WORK_HTTP_PASSWORD: 'test-only', RPI_OFFLINE: '1',
    RPI_CODING_AGENT_DIR: path.join(directory, 'profile', 'agent'),
    LOCALAPPDATA: path.join(directory, 'local'), APPDATA: path.join(directory, 'roaming'),
    USERPROFILE: path.join(directory, 'profile'), HOME: path.join(directory, 'profile'),
    WEBVIEW2_USER_DATA_FOLDER: path.join(directory, 'webview'),
    ...(process.env.WORK_TEST_WEBVIEW_RUNTIME ? {WEBVIEW2_BROWSER_EXECUTABLE_FOLDER:process.env.WORK_TEST_WEBVIEW_RUNTIME} : {}),
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${debugPort}`,
  });
  let child, browser, page, diagnostics = "";
  const errors = [];
  const request = (origin, resource, cookie, method = 'GET', body) => fetch(origin + resource, {
    method, headers: { 'x-rpi-web': '1', ...(cookie ? { cookie } : {}),
      ...(body ? { 'content-type': 'application/json' } : {}) },
    body: body ? JSON.stringify(body) : undefined,
  });
  const login = async origin => {
    const response = await request(origin, 'api/v1/auth', null, 'POST', { password: 'test-only' });
    assert.equal(response.status, 200);
    return response.headers.get('set-cookie').split(';')[0];
  };
  const stop = async () => {
    if (child && child.exitCode === null) {
      const expected = binary.replace(/'/g, "''");
      spawnSync('powershell.exe', ['-NoProfile', '-Command',
        `$p=Get-Process -Id ${child.pid} -ErrorAction SilentlyContinue; if($p -and $p.Path -eq '${expected}'){$null=$p.CloseMainWindow()}`],
      { windowsHide: true, timeout: 10000 });
      const deadline = Date.now() + 15000;
      while (child.exitCode === null && Date.now() < deadline) await new Promise(resolve => setTimeout(resolve, 100));
      if (child.exitCode === null) child.kill();
    }
    await browser?.close().catch(() => {});
  };
  try {
    child = spawn(binary, [], { cwd: directory, env, stdio: ['ignore', 'ignore', 'pipe'], windowsHide: true });
    diagnostics = '';
    child.stderr.on('data', chunk => { diagnostics += chunk; });
    for (let attempt = 0; attempt < 400; attempt++) {
      try { if ((await fetch(`http://127.0.0.1:${debugPort}/json/version`)).ok) break; } catch {}
      if (child.exitCode !== null) throw new Error('Isolated Work exited before opening its window');
      await new Promise(resolve => setTimeout(resolve, 100));
    }
    browser = await chromium.connectOverCDP(`http://127.0.0.1:${debugPort}`);
    page = browser.contexts()[0].pages()[0];
    page.setDefaultTimeout(20000);
    page.on('pageerror', error => errors.push(error.message));
    await page.route('https://api.github.com/**', route => route.fulfill({ status: 404, json: {} }));
    await page.getByRole('button', { name: '打开设置', exact: true }).waitFor();
    await page.getByRole('button', { name: '打开设置', exact: true }).click();
    await page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '工作协同', exact: true }).click();
    const collaboration = page.getByRole('switch', { name: '允许远程协作', exact: true });
    const webSwitch = page.getByRole('switch', { name: '允许 Web 访问', exact: true });
    await collaboration.waitFor();
    assert.equal(await collaboration.getAttribute('aria-checked'), 'false');
    assert.equal(await webSwitch.getAttribute('aria-checked'), 'false');
    assert.equal((await request(remote, 'api/v1/projects')).status, 403);
    assert.equal((await request(web, '')).status, 403);

    await webSwitch.click();
    await page.waitForFunction(() => document.querySelector('[aria-label="允许 Web 访问"][role=switch]')?.getAttribute('aria-checked') === 'true');
    assert.equal(await collaboration.getAttribute('aria-checked'), 'false');
    assert.equal(await page.getByRole('link', { name: '打开 Web 入口', exact: true }).getAttribute('href'), web);
    const webCookie = await login(web);
    assert.equal((await request(web, 'api/v1/projects', webCookie)).status, 200);
    assert.equal((await request(remote, 'api/v1/projects')).status, 403);
    console.log('PASS Web-only access serves Work assets and APIs with collaboration off');

    await collaboration.click();
    await page.waitForFunction(() => document.querySelector('[aria-label="允许远程协作"][role=switch]')?.getAttribute('aria-checked') === 'true');
    const remoteCookie = await login(remote);
    const response = await request(remote, 'api/v1/projects', remoteCookie, 'POST', {
      name: 'native-files', path: projectPath, initGit: false,
    });
    assert.equal(response.status, 200);
    const project = (await response.json()).project;
    const webProjects = await (await request(web, 'api/v1/projects', webCookie)).json();
    assert(webProjects.projects.some(item => item.id === project.id));
    const nativeFile = await request(remote, `api/v1/projects/${project.id}/file?path=${encodeURIComponent('中文 文件.rs')}`, remoteCookie);
    assert.equal(nativeFile.status, 200);
    assert((await nativeFile.json()).content.includes('native_preview'));
    console.log('PASS both entrances share one owner and native Unicode file content reads correctly');

    await collaboration.click();
    await page.waitForFunction(() => document.querySelector('[aria-label="允许远程协作"][role=switch]')?.getAttribute('aria-checked') === 'false');
    assert.equal((await request(remote, 'api/v1/projects', remoteCookie)).status, 403);
    assert.equal((await request(web, 'api/v1/projects', webCookie)).status, 200);
    await collaboration.click();
    await page.waitForFunction(() => document.querySelector('[aria-label="允许远程协作"][role=switch]')?.getAttribute('aria-checked') === 'true');
    const newRemoteCookie = await login(remote);
    await webSwitch.click();
    await page.waitForFunction(() => document.querySelector('[aria-label="允许 Web 访问"][role=switch]')?.getAttribute('aria-checked') === 'false');
    assert.equal((await request(web, '')).status, 403);
    assert.equal((await request(remote, 'api/v1/projects', newRemoteCookie)).status, 200);
    assert.equal(JSON.parse(fs.readFileSync(path.join(directory, 'node/sharing.json'))).allow_sharing, true);
    assert.equal(JSON.parse(fs.readFileSync(path.join(directory, 'node/web-sharing.json'))).allow_sharing, false);
    console.log('PASS each switch persists separately and revokes only its own entrance');
    await page.screenshot({ path: path.join(directory, 'separate-switches.png') });

    await page.getByRole('button', { name: '一键后台运行', exact: true }).click();
    await page.waitForFunction(() => document.hidden);
    assert.equal((await request(remote, 'api/v1/projects', newRemoteCookie)).status, 200);
    assert.equal((await request(web, '')).status, 403);
    assert.equal(child.exitCode, null);
    assert.deepEqual(errors, []);
    fs.writeFileSync(path.join(directory, 'validation.json'), JSON.stringify({
      passed: true, pid: child.pid, directory, remote, web, debugPort,
      checks: ['two real native switches', 'Web-only', 'shared owner', 'Unicode file read',
        'independent policy and persistence', 'background visibility and service continuity'],
    }, null, 2));
    console.log('PASS background hides the WebView while owner services keep running');
    if (process.env.WORK_KEEP_NATIVE_TEST === '1') {
      child.unref();
      child.stderr.destroy();
      console.log('NATIVE_TEST_STATE', JSON.stringify({ pid: child.pid, directory, remote, web, debugPort }));
      // Keep the isolated app available for the actual tray restore/exit check.
      await browser.close();
      child = null;
      browser = null;
    }
  } catch (error) {
    fs.writeFileSync(path.join(directory, 'failure.log'), diagnostics);
    await page?.screenshot({ path: path.join(directory, 'failure.png') }).catch(() => {});
    throw error;
  } finally {
    await stop();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
