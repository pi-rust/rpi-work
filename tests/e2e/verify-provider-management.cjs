// Formal WASM + real owned loopback node; only temporary agent files/credentials.
// No API mocks, installed agents, model requests or deployment. Node-owned file
// edits below intentionally exercise external-write CAS; never target user files.
'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const os = require('node:os');
const crypto = require('node:crypto');
const { spawn } = require('node:child_process');
const { chromium } = require('../support/browser.cjs');
const root = path.resolve(__dirname, '../..');
const cli = path.resolve(process.env.WORK_PROVIDER_CLI || path.join(root, 'target/providers-build/native/debug/work-cli.exe'));
const fixture = path.resolve(process.env.WORK_PROVIDER_RPC || path.join(root, 'target/providers-build/native/debug/work-server-rpc-fixture.exe'));
const assets = path.resolve(process.env.WORK_PROVIDER_ASSETS || path.join(root, 'target/providers-build/parity-web-server'));
const output = path.resolve(process.env.WORK_PROVIDER_REPORT || path.join(root, 'artifacts/providers'));
const sha = p => crypto.createHash('sha256').update(fs.readFileSync(p)).digest('hex');
const pause = ms => new Promise(r => setTimeout(r, ms));
const results = [], errors = [], requests = [], geometry = [], external = [];
const password = crypto.randomBytes(24).toString('hex');
const KEY = '!provider-fixture $VAR ${MISSING} $(not-a-command)';
const redact = s => String(s).split(password).join('[PASSWORD]').split(KEY).join('[TEST-KEY]');
let child, browser, temp, page, failure, info, wasmSha256, processOutput = '', stage = 'preflight', forcedCleanup = false;
const pass = text => { results.push(text); console.log('PASS', text); };
const until = async (fn, label, timeout = 20000) => {
  const end = Date.now() + timeout;
  do { if (await fn()) return; await pause(50); } while (Date.now() < end);
  throw new Error(`Deadline: ${label}`);
};
(async () => {
  fs.mkdirSync(output, { recursive: true });
  const report = path.join(output, `browser-${Date.now()}.json`);
  try {
    assert(fs.existsSync(cli) && fs.existsSync(fixture), 'build owned native test binaries first');
    const build = JSON.parse(fs.readFileSync(path.join(assets, 'build.json'), 'utf8').replace(/^\uFEFF/, ''));
    for (const [file, hash] of Object.entries(build.sourceHashes)) assert.equal(sha(path.join(root, file)), hash.toLowerCase(), `stale source ${file}`);
    wasmSha256 = sha(path.join(assets, 'assets/work-ui_bg.wasm')); assert.equal(wasmSha256, build.wasmSha256.toLowerCase());
    temp = fs.mkdtempSync(path.join(os.tmpdir(), 'rpi-providers-'));
    const env = { ...process.env };
    for (const k of Object.keys(env)) if (/^(RPI_|WORK_|WEBVIEW2_|XDG_|NODE_OPTIONS$|NODE_PATH$)|API.?KEY|TOKEN|SECRET|PASSWORD|PROXY|CREDENTIAL/i.test(k)) delete env[k];
    for (const dir of ['home', 'agent', 'cwd', 'temp', 'local', 'roaming']) fs.mkdirSync(path.join(temp, dir));
    const agent = path.join(temp, 'agent'), modelsPath = path.join(agent, 'models.json'), authPath = path.join(agent, 'auth.json');
    const seeded = { preservedRoot: 7, providers: { 'seeded-config': { api: 'openai-completions', baseUrl: 'http://127.0.0.1:1/v1', apiKey: '!never-execute-fixture', headers: { Authorization: 'hidden-header-fixture' }, models: [] } } };
    fs.writeFileSync(modelsPath, JSON.stringify(seeded));
    fs.writeFileSync(authPath, JSON.stringify({ 'seeded-oauth': { type: 'oauth', access: 'hidden-oauth-fixture', refresh: 'hidden-refresh-fixture', expires: 0 } }));
    const initialModels = fs.readFileSync(modelsPath), initialAuth = fs.readFileSync(authPath);
    Object.assign(env, { HOME: path.join(temp, 'home'), USERPROFILE: path.join(temp, 'home'), APPDATA: path.join(temp, 'roaming'), LOCALAPPDATA: path.join(temp, 'local'), XDG_DATA_HOME: path.join(temp, 'local'), RPI_CODING_AGENT_DIR: agent, TMP: path.join(temp, 'temp'), TEMP: path.join(temp, 'temp'), NO_PROXY: '127.0.0.1,localhost', RPI_WORK_RPI_BIN: fixture, RPI_WORK_DATA_DIR: path.join(temp, 'data'), RPI_WORK_HTTP_BIND: '127.0.0.1', RPI_WORK_HTTP_PORT: '0', RPI_WORK_HTTP_PASSWORD: password, RPI_WORK_HTTP_ASSETS: assets });
    stage = 'owner-start';
    child = spawn(cli, ['serve', '--shutdown-stdin'], { env, cwd: path.join(temp, 'cwd'), stdio: ['pipe', 'pipe', 'pipe'], windowsHide: true });
    let ready = '', spawnError;
    child.on('error', e => { spawnError = e; });
    child.stdout.on('data', b => { if (!ready.includes('\n')) ready += b.toString('utf8'); processOutput = (processOutput + redact(b)).slice(-16000); });
    child.stderr.on('data', b => { processOutput = (processOutput + redact(b)).slice(-16000); });
    await until(() => ready.includes('\n') || spawnError || child.exitCode !== null, 'owner ready');
    assert(!spawnError && ready.includes('\n'), processOutput);
    info = JSON.parse(ready.split('\n')[0]); assert(info.ready && info.webHosted && info.sharingEnabled);
    const base = new URL(info.address).origin;
    browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
    const context = await browser.newContext({ viewport: { width: 1440, height: 1000 }, serviceWorkers: 'block' });
    await context.addInitScript(() => {
      if (!localStorage.getItem('rpi-work-appearance-v1')) localStorage.setItem('rpi-work-appearance-v1', JSON.stringify({ theme: 'codex', mode: 'system', density: 'standard', toolRendering: 'simple', customThemes: [] }));
    });
    await context.route(/^https?:\/\//, route => {
      const req = route.request(), u = new URL(req.url());
      if (u.origin !== base) { external.push({ origin: u.origin, path: u.pathname }); return route.abort(); }
      if (u.pathname.startsWith('/api/')) requests.push({ method: req.method(), path: u.pathname });
      return route.continue();
    });
    const api = async (resource, method = 'GET', data) => {
      const response = await context.request.fetch(`${base}/api/v1/${resource}`, { method, headers: { 'x-rpi-web': '1' }, data });
      assert(response.ok(), `API ${resource}: ${response.status()}`);
      const json = await response.json();
      assert(!JSON.stringify(json).includes('hidden-') && !JSON.stringify(json).includes('never-execute-fixture') && !JSON.stringify(json).includes(KEY), 'write-only sources are never returned');
      return json;
    };
    await api('auth', 'POST', { password });
    page = await context.newPage(); page.setDefaultTimeout(15000); page.on('pageerror', e => errors.push(redact(e.message)));
    const openSettings = async () => {
      await page.locator('.sidebar').waitFor();
      await page.getByRole('button', { name: '打开设置', exact: true }).click();
      await page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '供应商', exact: true }).click();
      await until(async () => await page.locator('.provider-entry').count() >= 1, 'provider catalog');
    };
    const manager = () => page.getByRole('region', { name: '节点供应商配置', exact: true });
    const card = id => manager().locator('.provider-entry').filter({ has: page.locator('header > code').filter({ hasText: new RegExp(`^${id}$`) }) });
    const notice = () => manager().getByRole('status').filter({ hasText: '已权威回读' });
    const dialog = () => page.getByRole('dialog');
    stage = 'read-only'; await page.goto(`${base}/`); await openSettings();
    assert.equal(await manager().locator('.provider-entry').count(), 2);
    assert.equal((await api('sessions')).sessions.length, 0);
    assert.deepEqual(fs.readFileSync(modelsPath), initialModels); assert.deepEqual(fs.readFileSync(authPath), initialAuth);
    assert(!requests.some(r => r.path === '/api/v1/providers' && r.method === 'POST'));
    assert(!await page.locator('body').innerText().then(t => t.includes('hidden-')));
    pass('node-wide settings show redacted real saved files without a project, control host or mutation');
    stage = 'create';
    await manager().getByRole('button', { name: '添加供应商', exact: true }).click();
    await dialog().getByLabel('供应商 ID', { exact: true }).fill('browser-gateway');
    await dialog().getByLabel('显示名称', { exact: true }).fill('Browser gateway');
    await dialog().getByLabel('协议', { exact: true }).selectOption('openai-responses');
    await dialog().getByLabel('Base URL', { exact: true }).fill('https://gateway.invalid/v1');
    // The form provides one initial model row.
    await dialog().getByLabel('模型 ID', { exact: true }).fill('first-model');
    await dialog().getByLabel('模型名称', { exact: true }).fill('First model');
    await dialog().getByLabel('上下文窗口（tokens）', { exact: true }).fill('32768');
    await dialog().getByLabel('最大输出（tokens）', { exact: true }).fill('2048');
    await dialog().getByLabel('支持推理', { exact: true }).check();
    await dialog().getByRole('button', { name: '仅保存定义', exact: true }).click(); await notice().waitFor();
    let saved = await api('providers'); let row = saved.providers.find(p => p.id === 'browser-gateway');
    assert.equal(row.definition.baseUrl, 'https://gateway.invalid/v1'); assert.equal(row.definition.api, 'openai-responses'); assert.equal(row.definition.models[0].reasoning, true);
    assert.deepEqual(fs.readFileSync(authPath), initialAuth); assert.equal(JSON.parse(fs.readFileSync(modelsPath)).preservedRoot, 7);
    pass('add provider saves actual endpoint/protocol/model/token metadata and preserves other configuration');
    stage = 'edit-key';
    await card('browser-gateway').getByRole('button', { name: '编辑定义', exact: true }).click();
    assert(await dialog().getByLabel('供应商 ID', { exact: true }).isDisabled());
    assert(await dialog().getByLabel('模型 ID', { exact: true }).first().isDisabled(), 'existing model identity is immutable');
    await dialog().getByLabel('Base URL', { exact: true }).fill('https://edited.invalid/v2');
    await dialog().getByRole('button', { name: '添加模型行', exact: true }).click();
    await dialog().getByLabel('模型 ID', { exact: true }).nth(1).fill('second-model');
    await dialog().getByRole('button', { name: '仅保存定义', exact: true }).click(); await notice().waitFor();
    row = (await api('providers')).providers.find(p => p.id === 'browser-gateway'); assert.equal(row.definition.models.length, 2); assert.equal(row.definition.baseUrl, 'https://edited.invalid/v2');
    await card('browser-gateway').getByRole('button', { name: '保存独立密钥', exact: true }).click();
    await dialog().getByLabel('新 API Key', { exact: true }).fill(KEY);
    await dialog().getByRole('button', { name: '仅保存 API Key', exact: true }).click(); await notice().waitFor();
    const stored = JSON.parse(fs.readFileSync(authPath)); assert.equal(stored['browser-gateway'].key, '$!provider-fixture $$VAR $${MISSING} $$(not-a-command)');
    assert.equal((await api('providers')).providers.find(p => p.id === 'browser-gateway').storedCredential, 'apiKey');
    assert.equal(await page.locator('#provider-secret').count(), 0);
    await card('browser-gateway').getByRole('button', { name: '替换已存凭据为密钥', exact: true }).click();
    await dialog().getByRole('button', { name: '确认并继续', exact: true }).click(); await dialog().getByLabel('新 API Key', { exact: true }).fill('discard-on-cancel');
    await dialog().getByRole('button', { name: '取消并清空', exact: true }).click();
    assert.equal(JSON.parse(fs.readFileSync(authPath))['browser-gateway'].key, stored['browser-gateway'].key);
    pass('edit changes persist; literal key is write-only, safely escaped and cancelled input never saves');
    stage = 'external-cas';
    await card('browser-gateway').getByRole('button', { name: '编辑定义', exact: true }).click();
    await dialog().getByLabel('Base URL', { exact: true }).fill('https://stale-edit.invalid/v1');
    const externalDoc = JSON.parse(fs.readFileSync(modelsPath)); externalDoc.providers['browser-gateway'].name = 'Externally edited'; fs.writeFileSync(modelsPath, JSON.stringify(externalDoc));
    const postsBefore = requests.filter(r => r.path === '/api/v1/providers' && r.method === 'POST').length;
    await dialog().getByRole('button', { name: '仅保存定义', exact: true }).click();
    await manager().getByRole('alert').filter({ hasText: '结果待核对' }).waitFor();
    assert(await manager().getByRole('button', { name: '添加供应商', exact: true }).isDisabled());
    assert.equal(JSON.parse(fs.readFileSync(modelsPath)).providers['browser-gateway'].baseUrl, 'https://edited.invalid/v2');
    await page.reload(); await openSettings();
    await until(async () => await manager().getByRole('button', { name: '添加供应商', exact: true }).isEnabled(), 'automatic reread recovery');
    assert.equal(requests.filter(r => r.path === '/api/v1/providers' && r.method === 'POST').length, postsBefore);
    await manager().getByRole('button', { name: '恢复未保存定义草稿', exact: true }).click();
    assert.equal(await dialog().getByLabel('Base URL', { exact: true }).inputValue(), 'https://stale-edit.invalid/v1');
    await dialog().getByRole('button', { name: '取消', exact: true }).click();
    pass('external file edit rejects stale CAS; uncertain result locks writes until explicit reread without retry');
    stage = 'clear-delete';
    const confirm = async button => { await button.click(); await dialog().getByRole('button', { name: '确认并继续', exact: true }).click(); await notice().waitFor(); };
    await confirm(card('seeded-config').getByRole('button', { name: '清除配置凭据', exact: true }));
    assert(!Object.hasOwn(JSON.parse(fs.readFileSync(modelsPath)).providers['seeded-config'], 'apiKey'));
    assert.equal(JSON.parse(fs.readFileSync(modelsPath)).providers['seeded-config'].headers.Authorization, 'hidden-header-fixture');
    await confirm(card('seeded-oauth').getByRole('button', { name: '清除已存 OAuth', exact: true }));
    assert(!Object.hasOwn(JSON.parse(fs.readFileSync(authPath)), 'seeded-oauth'));
    await confirm(card('browser-gateway').getByRole('button', { name: '删除自定义配置', exact: true }));
    row = (await api('providers')).providers.find(p => p.id === 'browser-gateway'); assert(!row.configured && row.storedCredential === 'apiKey');
    await confirm(card('browser-gateway').getByRole('button', { name: '清除已存 API Key', exact: true }));
    assert(!(await api('providers')).providers.some(p => p.id === 'browser-gateway'));
    await manager().getByRole('button', { name: '释放空闲原生宿主', exact: true }).click();
    await manager().getByRole('status').filter({ hasText: '已释放 0 个空闲宿主' }).waitFor();
    assert.equal((await api('sessions')).sessions.length, 0);
    pass('explicit source-specific credential clearing, OAuth record deletion and config deletion preserve truthful independent states');
    stage = 'responsive';
    for (const theme of ['light', 'dark']) {
      await page.emulateMedia({ colorScheme: theme }); await until(() => page.evaluate(t => document.documentElement.dataset.mode === t, theme), 'actual color mode');
      for (const width of [1440, 390, 320]) {
        await page.setViewportSize({ width, height: 1000 }); await pause(100);
        await manager().getByRole('button', { name: '添加供应商', exact: true }).click();
        // Measure the settled presentation, not the shared dialog's scale-in frame.
        await until(() => dialog().evaluate(el => el.getAnimations({ subtree: true }).every(a => a.playState !== 'running')), 'dialog animation settles');
        const measured = await dialog().evaluate(el => {
          const r = el.getBoundingClientRect(), area = el.querySelector('.modal-body');
          return { left: r.left, right: r.right, scroll: area.scrollWidth, client: area.clientWidth, background: getComputedStyle(el).backgroundColor,
            controls: [...el.querySelectorAll('button,input:not([type=checkbox]),select')].filter(e => e.getClientRects().length).map(e => ({ tag: e.tagName, name: e.getAttribute('aria-label') || e.textContent, class: e.className, height: e.getBoundingClientRect().height })) };
        });
        assert(measured.left >= -1 && measured.right <= width + 1 && measured.scroll <= measured.client + 1, 'responsive dialog does not overflow');
        geometry.push({ theme, width, ...measured });
        assert(measured.controls.every(c => c.height >= 43.9), `44px targets: ${JSON.stringify(measured.controls.filter(c => c.height < 43.9))}`);
        await page.screenshot({ path: path.join(output, `providers-${theme}-${width}.png`), fullPage: true });
        await dialog().getByRole('button', { name: '取消', exact: true }).click();
      }
    }
    assert.notEqual(geometry.find(g => g.theme === 'light').background, geometry.find(g => g.theme === 'dark').background, 'actual dialog palettes change');
    pass('verified real light/dark 1440/390/320 shared form layout, keyboard labels and 44px targets');
    stage = 'reload'; await page.setViewportSize({ width: 1440, height: 1000 }); await page.reload(); await openSettings();
    assert.equal(await manager().locator('.provider-entry').count(), 1);
    await page.reload(); await openSettings();
    await until(async () => await manager().getByRole('button', { name: '添加供应商', exact: true }).isEnabled(), 'reload provider catalog');
    assert.equal((await api('sessions')).sessions.length, 0); assert.equal(errors.length, 0, errors.join('\n'));
    pass('reload retains saved state with no secret draft or automatic host/model session');
  } catch (e) {
    failure = redact(e.stack || e); process.exitCode = 1; console.error(failure);
    if (page) { await page.screenshot({ path: path.join(output, 'failure.png'), fullPage: true }).catch(() => {}); }
  } finally {
    stage = failure ? stage : 'cleanup';
    await browser?.close().catch(e => errors.push(redact(e.message)));
    if (child && child.exitCode === null) {
      child.stdin.end();
      try { await until(() => child.exitCode !== null, 'cooperative owner exit', 20000); }
      catch { forcedCleanup = true; child.kill(); }
    }
    if (child && child.exitCode !== 0) { failure ||= 'owner did not exit cleanly'; process.exitCode = 1; }
    if (errors.length || forcedCleanup) { failure ||= 'cleanup/browser error'; process.exitCode = 1; }
    let tempRemoved = !temp;
    if (temp) { try { fs.rmSync(temp, { recursive: true }); tempRemoved = !fs.existsSync(temp); } catch { failure ||= 'temp removal failed'; process.exitCode = 1; } }
    fs.writeFileSync(report, JSON.stringify({ status: failure ? 'failed' : 'passed', stage, results, failure, errors, requests, geometry, externalBlocked: external,
      ownerExitCode: child?.exitCode, forcedCleanup, tempRemoved, cliSha256: fs.existsSync(cli) ? sha(cli) : null, fixtureSha256: fs.existsSync(fixture) ? sha(fixture) : null, wasmSha256,
      processOutput, limitations: ['No real remote authentication or model request.', 'No native installed-agent reload acceptance.', 'No interactive WebView2 or Android device acceptance.', 'OAuth record clearing is not OAuth login or remote grant revocation.'] }, null, 2));
    console.log(report);
  }
})();
