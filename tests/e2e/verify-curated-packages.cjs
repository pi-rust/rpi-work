// Fresh formal WASM + real isolated headless owner. No model, installed Agent,
// API mocking, real credentials, deployment or native desktop acceptance.
// Build: cargo build -p work-cli --bin work-cli --locked --offline
//        tools/dev/build-ui-web.ps1 -DebugBuild -OutputRoot target/curated-package-build
// Run: node tests/e2e/verify-curated-packages.cjs
'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const os = require('node:os');
const crypto = require('node:crypto');
const { spawn } = require('node:child_process');
const { chromium } = require('../support/browser.cjs');
const root = path.resolve(__dirname, '../..');
const cli = path.resolve(process.env.WORK_PACKAGE_CLI || path.join(root, 'target/debug/work-cli.exe'));
const fixture = path.resolve(process.env.WORK_PACKAGE_RPC || path.join(root, 'target/debug/work-server-rpc-fixture.exe'));
const assets = path.resolve(process.env.WORK_PACKAGE_ASSETS || path.join(root, 'target/curated-package-build/parity-web-server'));
const output = path.resolve(process.env.WORK_PACKAGE_REPORT || path.join(root, 'artifacts/curated-packages'));
const sha = file => crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
const delay = ms => new Promise(r => setTimeout(r, ms));
const results = [], errors = [], requests = [], geometry = [], external = [];
let child, browser, temp, page, failure, forcedCleanup = false, stage = 'preflight';
const pass = text => { results.push(text); console.log('PASS', text); };
const until = async (fn, label, timeout = 20000) => {
  const end = Date.now() + timeout;
  do { if (await fn()) return; await delay(50); } while (Date.now() < end);
  throw new Error(`Deadline: ${label}`);
};
const bounded = async (promise, label, ms = 20000) => {
  let timer;
  try { return await Promise.race([promise, new Promise((_, reject) => { timer = setTimeout(() => reject(new Error(`Deadline: ${label}`)), ms); })]); }
  finally { clearTimeout(timer); }
};
(async () => {
  fs.mkdirSync(output, { recursive: true });
  const report = path.join(output, `browser-${Date.now()}.json`);
  const password = crypto.randomBytes(24).toString('hex');
  const redact = value => String(value).split(password).join('[REDACTED]');
  let processOutput = '', info, artifactHash;
  try {
    assert(fs.existsSync(cli) && fs.existsSync(fixture));
    const build = JSON.parse(fs.readFileSync(path.join(assets, 'build.json'), 'utf8').replace(/^\uFEFF/, ''));
    for (const [file, expected] of Object.entries(build.sourceHashes)) assert.equal(sha(path.join(root, file)).toUpperCase(), expected.toUpperCase(), `stale bundle: ${file}`);
    artifactHash = sha(path.join(assets, 'assets/work-ui_bg.wasm'));
    assert.equal(artifactHash.toUpperCase(), build.wasmSha256.toUpperCase());
    temp = fs.mkdtempSync(path.join(os.tmpdir(), 'rpi-curated-packages-'));
    const env = { ...process.env };
    for (const k of Object.keys(env)) if (/^(RPI_|WORK_|WEBVIEW2_|XDG_|NODE_OPTIONS$|NODE_PATH$)|API.?KEY|TOKEN|SECRET|PASSWORD|PROXY|CREDENTIAL/i.test(k)) delete env[k];
    for (const n of ['home', 'roaming', 'local', 'agent', 'cwd', 'temp']) fs.mkdirSync(path.join(temp, n));
    fs.mkdirSync(path.join(temp, 'data'));
    fs.writeFileSync(path.join(temp, 'data', 'sharing.json'), JSON.stringify({ allow_sharing: true }));
    Object.assign(env, { HOME: path.join(temp, 'home'), USERPROFILE: path.join(temp, 'home'), APPDATA: path.join(temp, 'roaming'), LOCALAPPDATA: path.join(temp, 'local'), XDG_DATA_HOME: path.join(temp, 'local'), RPI_CODING_AGENT_DIR: path.join(temp, 'agent'), TMP: path.join(temp, 'temp'), TEMP: path.join(temp, 'temp'), NO_PROXY: '127.0.0.1,localhost', RPI_WORK_RPI_BIN: fixture, RPI_WORK_DATA_DIR: path.join(temp, 'data'), RPI_WORK_HTTP_BIND: '127.0.0.1', RPI_WORK_HTTP_PORT: '0', RPI_WORK_HTTP_PASSWORD: password, RPI_WORK_HTTP_ASSETS: assets });
    stage = 'owner-start';
    child = spawn(cli, ['serve', '--shutdown-stdin'], { env, cwd: path.join(temp, 'cwd'), stdio: ['pipe', 'pipe', 'pipe'], windowsHide: true });
    let ready = '', spawnError;
    child.on('error', e => { spawnError = e; });
    child.stdout.on('data', b => { if (!ready.includes('\n')) ready += b.toString('utf8'); processOutput = (processOutput + redact(b)).slice(-32768); });
    child.stderr.on('data', b => { processOutput = (processOutput + redact(b)).slice(-32768); });
    await until(() => ready.includes('\n') || spawnError || child.exitCode !== null, 'owner ready');
    assert(!spawnError);
    assert(ready.includes('\n'), `owner exited before Ready: ${processOutput}`);
    info = JSON.parse(ready.split('\n')[0]);
    assert(info.ready && info.webHosted && info.sharingEnabled);
    const base = new URL(info.address).origin;
    const client = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
    browser = client;
    const context = await browser.newContext({ viewport: { width: 1440, height: 1000 }, acceptDownloads: true, serviceWorkers: 'block' });
    // Test-owned device preference, consumed by the real appearance adapter.
    // No DOM tokens/business state or API responses are injected.
    await context.addInitScript(() => {
      if (!localStorage.getItem('rpi-work-settings-v1')) localStorage.setItem('rpi-work-settings-v1', JSON.stringify({ version: 1, appearance: { theme: 'codex', mode: 'system', density: 'standard', toolRendering: 'simple', customThemes: [] } }));
    });
    // Only this fresh owner's origin is reachable from the browser; no response mocking.
    await context.route(/^https?:\/\//, route => {
      const req = route.request(), u = new URL(req.url());
      if (u.origin !== base) { external.push({ origin: u.origin, path: u.pathname }); return route.abort(); }
      if (u.pathname.startsWith('/api/')) requests.push({ method: req.method(), path: u.pathname });
      return route.continue();
    });
    const api = async (route, method = 'GET', body) => {
      const response = await context.request.fetch(`${base}/api/v1/${route}`, { method, headers: { 'x-rpi-web': '1' }, data: body });
      assert(response.ok(), `fixture setup/read failed ${response.status()}`);
      return response.json();
    };
    await api('auth', 'POST', { password });
    fs.mkdirSync(path.join(temp, 'project'));
    const project = (await api('projects', 'POST', { name: 'Curated Package fixture', path: path.join(temp, 'project'), initGit: false })).project;
    assert(project.id);
    page = await context.newPage(); page.setDefaultTimeout(15000);
    page.on('pageerror', e => errors.push(redact(e.message)));
    stage = 'formal-ui';
    await page.goto(`${base}/`);
    await page.locator('.sidebar').waitFor();
    await page.locator('.project-entry').filter({ hasText: project.name }).locator('.project-row').click();
    await page.locator('.primary-nav').getByRole('button', { name: '资源中心', exact: true }).click();
    const center = page.locator('.resource-center-v2');
    const button = (name, root = center) => root.getByRole('button', { name, exact: true });
    const source = async name => {
      await button(name, center.getByRole('navigation', { name: '资源视图' })).click();
      await until(() => center.getAttribute('aria-busy').then(value => value === 'false'), 'resource readback');
    };
    await until(async () => await center.locator('.resource-workbench-row').count() === 3, 'real curated catalog');
    assert.equal(requests.filter(request => request.path.endsWith('/packages/acquire') || request.path.endsWith('/packages/install')).length, 0);
    pass('formal resource center reads the real embedded catalog without acquisition, installation or model sessions');
    const before = await api(`projects/${project.id}/packages`);
    assert(before.packages.every(offer => !offer.acquired));
    const offer = before.packages.find(offer => offer.manifest.kind === 'skill');
    assert(offer);
    const skillRow = () => center.locator('.resource-workbench-row[data-resource-kind="skills"]').filter({ hasText: offer.manifest.id });
    await button('查看详情', skillRow()).click();
    const detail = page.getByRole('dialog', { name: '资源详情', exact: true });
    await until(async () => await detail.getByRole('button', { name: '下载到本机', exact: true }).isEnabled(), 'verified content preview');
    await detail.getByText('查看资源内容', { exact: true }).click();
    await detail.getByText('SKILL.md', { exact: true }).click();
    assert.match(await detail.innerText(), /Read project standards and relevant source first/);
    const pendingDownload = page.waitForEvent('download');
    await button('下载到本机', detail).click();
    const downloaded = path.join(output, 'curated-skill.rpi-package.json');
    await (await pendingDownload).saveAs(downloaded);
    const bundle = JSON.parse(fs.readFileSync(downloaded, 'utf8'));
    assert.equal(bundle.manifest.id, offer.manifest.id);
    assert.equal(bundle.sha256, offer.sha256);
    assert.equal(crypto.createHash('sha256').update(JSON.stringify([bundle.manifest, bundle.sources])).digest('hex'), bundle.sha256);
    assert((await api(`projects/${project.id}/packages`)).packages.every(offer => !offer.acquired));
    await page.keyboard.press('Escape');
    pass('real node preview and browser download preserve canonical metadata, source bytes and SHA-256');
    stage = 'installation';
    await button('安装', skillRow()).click();
    let install = page.getByRole('dialog', { name: '选择安装范围', exact: true });
    assert(await button('确认安装', install).isDisabled());
    await install.locator('[name=install-scope][value=global]').check();
    await install.getByRole('checkbox').check();
    await install.locator('[name=install-scope][value=project]').check();
    assert(!await install.getByRole('checkbox').isChecked());
    assert(await button('确认安装', install).isDisabled());
    await button('取消', install).click();
    assert((await api(`projects/${project.id}/packages`)).packages.every(offer => !offer.acquired));
    await button('查看详情', skillRow()).click();
    await until(async () => await button('获取到当前节点', detail).isEnabled(), 'standalone acquisition');
    await button('获取到当前节点', detail).click();
    await button('节点已获取', detail).waitFor();
    assert.equal((await api(`projects/${project.id}/packages`)).packages.filter(offer => offer.acquired).length, 1);
    await page.keyboard.press('Escape');
    pass('standalone node acquisition remains available and is confirmed by authoritative catalog readback');
    await button('安装', skillRow()).click();
    install = page.getByRole('dialog', { name: '选择安装范围', exact: true });
    await install.locator('[name=install-scope][value=project]').check();
    await install.getByRole('checkbox').check();
    await button('确认安装', install).click();
    const expectInstall = process.env.WORK_PACKAGE_EXPECT_INSTALL === '1';
    await until(async () => await button('完成安装', install).count() || await install.getByRole('alert').count(), 'installation outcome', 30000);
    const acquiredCount = (await api(`projects/${project.id}/packages`)).packages.filter(offer => offer.acquired).length;
    if (expectInstall) {
      assert.equal(acquiredCount, 1);
      assert.equal(await install.getByRole('alert').count(), 0, await install.innerText());
      await button('完成安装', install).click();
      const installed = (await api(`projects/${project.id}/native-resources`)).resources;
      assert(installed.some(resource => resource.scope === 'project' && resource.name === offer.manifest.name && resource.facts.installed === true));
      pass('project installation succeeds through the real native manager and is confirmed by directory readback');
      await source('公共库');
      await button('安装', skillRow()).click();
      install = page.getByRole('dialog', { name: '选择安装范围', exact: true });
      await install.locator('[name=install-scope][value=global]').check();
      await install.getByRole('checkbox').check();
      await button('确认安装', install).click();
      await button('完成安装', install).waitFor({ timeout: 30000 });
      await button('完成安装', install).click();
      await center.getByLabel('已安装资源范围', { exact: true }).selectOption(project.id);
      await until(async () => (await center.locator('.resource-workbench-row').count()) === 1, 'same-version deduplication');
      assert.match(await center.innerText(), /项目安装 · 全局也可用/);
      const globalResource = (await api(`projects/${project.id}/native-resources`)).resources.find(resource => resource.scope === 'global' && resource.name === offer.manifest.name);
      assert(globalResource && globalResource.facts.installed === true);
      await button('查看详情', center.locator('.native-resource-card')).click();
      const installedDetail = page.getByRole('dialog', { name: '资源详情', exact: true });
      await installedDetail.locator('summary').filter({ hasText: '管理此资源' }).click();
      await button('移除资源', installedDetail).click();
      await button('确认移除', installedDetail).click();
      await until(async () => !(await api(`projects/${project.id}/native-resources`)).resources.some(resource => resource.scope === 'project' && resource.name === offer.manifest.name && resource.facts.installed === true), 'project removal');
      await page.keyboard.press('Escape');
      assert((await api(`projects/${project.id}/native-resources`)).resources.some(resource => resource.reference === globalResource.reference && resource.facts.installed === true));
      pass('global installation and project installation deduplicate; project removal preserves the global installation');
      fs.mkdirSync(path.join(temp, 'second-project'));
      const second = (await api('projects', 'POST', { name: 'Second resource scope', path: path.join(temp, 'second-project'), initGit: false })).project;
      const inherited = (await api(`projects/${second.id}/native-resources`)).resources;
      assert(inherited.some(resource => resource.scope === 'global' && resource.name === offer.manifest.name && resource.facts.installed === true));
      assert(!inherited.some(resource => resource.scope === 'project' && resource.name === offer.manifest.name && resource.facts.installed === true));
      pass('the second real project inherits global installation without a project installation record');
    } else {
      assert(acquiredCount <= 1);
      assert.equal(await button('完成安装', install).count(), 0);
      assert(!await install.getByRole('checkbox').isChecked());
      await page.keyboard.press('Escape');
      pass('unsupported synthetic runtime refuses installation and exposes explicit recovery without fake success');
    }
    assert.equal((await api('sessions')).sessions.length, 0, 'installation never starts a model session');
    await source('公共库');
    stage = 'responsive';
    for (const theme of ['light', 'dark']) {
      await page.emulateMedia({ colorScheme: theme });
      await until(() => page.evaluate(expected => document.documentElement.dataset.mode === expected, theme), 'actual color mode');
      for (const width of [1440, 390, 320]) {
        await page.setViewportSize({ width, height: 1000 });
        await center.locator('.resource-center-heading').scrollIntoViewIfNeeded();
        const measured = await center.evaluate(element => {
          const box = element.getBoundingClientRect();
          return { width: innerWidth, left: box.left, right: box.right, overflow: element.scrollWidth - element.clientWidth,
            color: getComputedStyle(element).color, background: getComputedStyle(element.closest('.page-view')).backgroundColor };
        });
        assert(measured.left >= -1 && measured.right <= width + 1 && measured.overflow <= 1);
        geometry.push({ theme, ...measured });
        await page.screenshot({ path: path.join(output, `packages-${theme}-${width}.png`), animations: 'disabled' });
      }
    }
    assert.notEqual(geometry.find(item => item.theme === 'light').background, geometry.find(item => item.theme === 'dark').background);
    pass('formal resource center fits 1440/390/320px with real light/dark theme changes');
    await page.reload();
    await page.locator('.sidebar').waitFor();
    await page.setViewportSize({ width: 1440, height: 1000 });
    await page.locator('.project-entry').filter({ hasText: project.name }).locator('.project-row').click();
    await page.locator('.primary-nav').getByRole('button', { name: '资源中心', exact: true }).click();
    await until(async () => await center.locator('.resource-workbench-row').count() === 3, 'reloaded catalog');
    await button('安装', skillRow()).click();
    install = page.getByRole('dialog', { name: '选择安装范围', exact: true });
    assert(!await install.getByRole('checkbox').isChecked());
    assert.equal(await install.locator('[name=install-scope]:checked').count(), 0);
    assert.equal((await api('sessions')).sessions.length, 0);
    pass('reload discards install scope and authorization and never creates a session');
    assert.deepEqual(errors, []);
    stage = 'cleanup';
  } catch (error) {
    failure = redact(error.stack || error); console.error(failure); process.exitCode = 1;
    if (page) await page.screenshot({ path: path.join(output, 'browser-failure.png'), fullPage: true }).catch(() => {});
  } finally {
    if (browser) await browser.close().catch(e => errors.push(redact(e.message)));
    if (child && child.exitCode === null && !child.signalCode) {
      child.stdin.end();
      try { await until(() => child.exitCode !== null || child.signalCode, 'owner cooperative exit', 20000); }
      catch { forcedCleanup = true; child.kill(); await until(() => child.exitCode !== null || child.signalCode, 'owned child forced cleanup', 10000).catch(() => {}); }
    }
    const exitCode = child?.exitCode;
    if (errors.length || forcedCleanup) { process.exitCode = 1; failure ||= 'cleanup/browser error'; }
    if (child && exitCode !== 0) { process.exitCode = 1; failure ||= `owner exit ${exitCode}`; }
    if (temp && (!child || child.exitCode !== null || child.signalCode)) { assert(path.resolve(temp).startsWith(path.resolve(os.tmpdir()) + path.sep) && path.basename(temp).startsWith('rpi-curated-packages-')); fs.rmSync(temp, { recursive: true, force: true }); }
    fs.writeFileSync(report, JSON.stringify({ status: failure ? 'failed' : 'passed', stage, results, failure, errors, requests, geometry, blockedExternal: external, forcedCleanup, ownerExitCode: exitCode, tempRemoved: temp ? !fs.existsSync(temp) : true, cliSha256: fs.existsSync(cli) ? sha(cli) : null, wasmSha256: artifactHash, fixtureSha256: fs.existsSync(fixture) ? sha(fixture) : null, scriptSha256: sha(__filename), processOutput, limitations: ['Embedded curated data only, not an online market or arbitrary URL/import API.', process.env.WORK_PACKAGE_EXPECT_INSTALL === '1' ? 'Real native resource manager and isolated projects; no model session is created.' : 'Synthetic host tests install refusal; successful native installation requires WORK_PACKAGE_EXPECT_INSTALL=1 with an explicit native binary.', 'Chromium viewport/media checks are not Windows WebView2 or Android device acceptance.'], generatedAtUtc: new Date().toISOString() }, null, 2) + '\n');
    console.log(report);
  }
})();
