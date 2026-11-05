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
      if (!localStorage.getItem('rpi-work-appearance-v1')) localStorage.setItem('rpi-work-appearance-v1', JSON.stringify({ theme: 'codex', mode: 'system', density: 'standard', toolRendering: 'simple', customThemes: [] }));
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
    const projectEntry = page.locator('.project-entry').filter({ hasText: project.name });
    await projectEntry.waitFor();
    // Selecting a project with zero sessions must not run a model; the owned
    // legacy-only synthetic fixture cannot support exact native creation.
    await projectEntry.locator('.project-row').click();
    await page.locator('.sidebar').getByRole('button', { name: 'Packages', exact: true }).click();
    const discovery = page.getByRole('region', { name: '精选 Package 发现' });
    await discovery.waitFor();
    await until(async () => await discovery.locator('.package-card').count() === 3, 'curated catalog');
    assert.match(await discovery.innerText(), /非在线市场/);
    assert.equal(requests.filter(r => r.path.endsWith('/packages/acquire') || r.path.endsWith('/packages/install')).length, 0);
    pass('fresh formal resource page reads three real curated offers without acquisition/installation');
    stage = 'preview-filter';
    await discovery.getByLabel('筛选目录', { exact: true }).fill('work-basics');
    await until(async () => await discovery.locator('.package-card').count() === 2, 'filtered catalog');
    await discovery.getByLabel('资源类型', { exact: true }).selectOption('skill');
    await until(async () => await discovery.locator('.package-card').count() === 1, 'skill filter');
    await discovery.getByRole('button', { name: '详情 / 内容预览', exact: true }).click();
    const detail = discovery.getByRole('region', { name: '精选包详情' });
    await detail.getByText('SKILL.md', { exact: true }).waitFor();
    await detail.getByText('SKILL.md', { exact: true }).click();
    await detail.getByText(/Read project standards and relevant source first/).waitFor();
    const install = detail.getByRole('button', { name: '安装', exact: true });
    assert(await install.isDisabled());
    const before = await api(`projects/${project.id}/packages`);
    assert(before.packages.every(p => !p.acquired));
    pass('filters and canonical Skill preview are read-only; installation starts disabled');
    stage = 'download';
    const got = page.waitForEvent('download');
    await detail.getByRole('button', { name: '下载到本机', exact: true }).click();
    const download = await got;
    const downloaded = path.join(output, 'curated-skill.rpi-package.json');
    await download.saveAs(downloaded);
    const bundle = JSON.parse(fs.readFileSync(downloaded, 'utf8'));
    const offer = before.packages.find(p => p.manifest.kind === 'skill');
    assert.equal(bundle.manifest.id, offer.manifest.id);
    assert.equal(bundle.sha256, offer.sha256);
    assert.equal(crypto.createHash('sha256').update(JSON.stringify([bundle.manifest, bundle.sources])).digest('hex'), bundle.sha256);
    assert((await api(`projects/${project.id}/packages`)).packages.every(p => !p.acquired));
    pass('real browser JSON download matches node metadata/source SHA-256 and does not acquire');
    stage = 'acquire';
    await detail.getByRole('button', { name: '获取到当前节点', exact: true }).click();
    await until(async () => (await discovery.locator('.package-card').innerText()).includes('当前执行节点：已获取'), 'authoritative acquired reread');
    const acquired = await api(`projects/${project.id}/packages`);
    assert.equal(acquired.packages.filter(p => p.acquired).length, 1);
    assert(await install.isDisabled());
    const trust = detail.getByRole('checkbox');
    await trust.check(); assert(await install.isEnabled());
    await detail.getByLabel('安装范围', { exact: true }).selectOption('global');
    assert(!await trust.isChecked()); assert(await install.isDisabled());
    await detail.getByLabel('安装范围', { exact: true }).selectOption('project');
    await trust.check();
    const submitted = page.waitForResponse(r => r.url().endsWith('/packages/install') && r.request().method() === 'POST');
    await install.click();
    const response = await submitted;
    // Legacy-only synthetic host: explicit Unsupported, never fake installed/loaded.
    assert(!response.ok());
    await discovery.getByRole('alert').filter({ hasText: '安装失败或结果未知' }).waitFor();
    assert(!await trust.isChecked());
    assert(!((await discovery.innerText()).includes('当前节点安装回执成功')));
    assert.equal((await api('sessions')).sessions.length, 0);
    pass('node acquisition persists; scope change clears trust; unsupported synthetic host refuses install without fake success/run');
    stage = 'responsive';
    for (const theme of ['light', 'dark']) {
      await page.emulateMedia({ colorScheme: theme });
      await until(() => page.evaluate(expected => document.documentElement.dataset.mode === expected, theme), 'actual appearance mode');
      for (const width of [1440, 390, 320]) {
        await page.setViewportSize({ width, height: 1000 });
        await delay(120);
        await discovery.scrollIntoViewIfNeeded();
        const measured = await discovery.evaluate(el => {
          const bounds = el.getBoundingClientRect();
          const controls = [...el.querySelectorAll('button,input:not([type=checkbox]),select,summary')].filter(e => e.getClientRects().length).map(e => ({ tag: e.tagName, height: e.getBoundingClientRect().height }));
          return { left: bounds.left, right: bounds.right, width: innerWidth, scroll: el.scrollWidth, client: el.clientWidth, controls, color: getComputedStyle(el).color, background: getComputedStyle(el).backgroundColor };
        });
        assert(measured.left >= -1 && measured.right <= width + 1, 'discovery stays in viewport');
        assert(measured.scroll <= measured.client + 1, 'discovery has no horizontal overflow');
        assert(measured.controls.every(c => c.height >= 43.9), '44px targets');
        geometry.push({ theme, width, ...measured });
        await page.screenshot({ path: path.join(output, `packages-${theme}-${width}.png`), fullPage: true });
      }
    }
    assert.notEqual(geometry.find(g => g.theme === 'light').background, geometry.find(g => g.theme === 'dark').background, 'actual light/dark palette changes');
    pass('shared Chromium presentation fits 1440/390/320 in verified light/dark modes with 44px targets');
    stage = 'reload';
    await page.setViewportSize({ width: 1440, height: 1000 });
    await page.reload();
    await page.locator('.sidebar').waitFor();
    await page.locator('.project-entry').filter({ hasText: project.name }).locator('.project-row').click();
    await page.locator('.sidebar').getByRole('button', { name: 'Packages', exact: true }).click();
    await until(async () => (await discovery.innerText()).includes('当前执行节点：已获取'), 'reload acquired readback');
    const skillCard = discovery.locator('.package-card').filter({ hasText: '独立基础 Skill' });
    await skillCard.getByRole('button', { name: '详情 / 内容预览', exact: true }).click();
    assert(!await detail.getByRole('checkbox').isChecked());
    const refresh = discovery.getByRole('button', { name: '刷新精选目录', exact: true });
    await refresh.focus(); await page.keyboard.press('Enter');
    await until(async () => await discovery.locator('.package-card').count() === 3, 'keyboard refresh');
    assert.equal((await api('sessions')).sessions.length, 0);
    assert.equal(requests.filter(r => r.path.endsWith('/packages/acquire')).length, 1);
    assert.equal(requests.filter(r => r.path.endsWith('/packages/install')).length, 1);
    stage = 'project-scope';
    await skillCard.getByRole('button', { name: /详情 \/ 内容预览/ }).click();
    await detail.getByRole('checkbox').check();
    const second = (await api('projects', 'POST', { name: 'Second curated scope', initGit: false })).project;
    // Explicit catalog reload is a normal UI action; no state/DOM injection.
    await page.getByRole('button', { name: '刷新会话', exact: true }).click();
    await page.locator('.project-entry').filter({ hasText: second.name }).locator('.project-row').click();
    await page.locator('.sidebar').getByRole('button', { name: 'Packages', exact: true }).click();
    await until(async () => await discovery.locator('.package-card').count() === 3, 'second project catalog');
    assert.equal(await detail.count(), 0, 'selection and trust from old project are discarded');
    await discovery.locator('.package-card').filter({ hasText: '独立基础 Skill' }).getByRole('button', { name: '详情 / 内容预览', exact: true }).click();
    assert(!await detail.getByRole('checkbox').isChecked());
    assert(await detail.getByRole('button', { name: '安装', exact: true }).isDisabled());
    assert.equal((await api('sessions')).sessions.length, 0);
    pass('switching project remounts discovery and clears selection/trust; node cache is not a project installation');
    assert.equal(errors.length, 0, errors.join('\n'));
    pass('reload rereads acquired bytes and clears trust; keyboard refresh and no automatic retries/model sessions');
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
    if (temp && (!child || child.exitCode !== null || child.signalCode)) fs.rmSync(temp, { recursive: true, force: true });
    fs.writeFileSync(report, JSON.stringify({ status: failure ? 'failed' : 'passed', stage, results, failure, errors, requests, geometry, blockedExternal: external, forcedCleanup, ownerExitCode: exitCode, tempRemoved: temp ? !fs.existsSync(temp) : true, cliSha256: fs.existsSync(cli) ? sha(cli) : null, wasmSha256: artifactHash, fixtureSha256: fs.existsSync(fixture) ? sha(fixture) : null, scriptSha256: sha(__filename), processOutput, limitations: ['Embedded curated data only, not an online market or arbitrary URL/import API.', 'Legacy-only synthetic host: this tests install refusal, NOT successful native install/session loading.', 'Chromium viewport/media checks are not Windows WebView2 or Android device acceptance.'], generatedAtUtc: new Date().toISOString() }, null, 2) + '\n');
    console.log(report);
  }
})();
