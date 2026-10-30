// Requires an actual installed rpi. No external model: only loopback provider responses are fixtures.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { spawn } = require('node:child_process');
const { once } = require('node:events');
const { chromium } = require('../support/browser.cjs');
const repo = path.resolve(__dirname, '../..');

(async () => {
  const runner = spawn(process.execPath, ['tools/dev/extension-demo.mjs'], { cwd: repo, stdio: ['pipe', 'pipe', 'pipe'] });
  const runnerExit = new Promise(resolve => runner.once('exit', resolve));
  let output = '', errors = '', browser, server;
  const results = [], pageErrors = [];
  const pass = text => { results.push(text); console.log('PASS', text); };
  runner.stderr.on('data', chunk => { errors += chunk; });
  const ready = await new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error(`Demo startup timed out: ${errors}`)), 180000);
    runner.once('error', reject);
    runner.once('exit', code => { clearTimeout(timer); reject(new Error(`Demo exited ${code}: ${errors}`)); });
    runner.stdout.on('data', chunk => {
      output += chunk;
      const line = output.split('\n').find(line => line.startsWith('DEMO_READY '));
      if (line) { clearTimeout(timer); resolve(JSON.parse(line.slice('DEMO_READY '.length))); }
    });
  }).catch(async error => {
    runner.stdin.end('stop\n');
    const timer = setTimeout(() => runner.kill(), 25000);
    try { await runnerExit; } finally { clearTimeout(timer); }
    throw error;
  });
  try {
    // Own an ephemeral review server so this test never terminates someone else's server.
    const net = require('node:net');
    const reserve = net.createServer(); await new Promise(resolve => reserve.listen(0, '127.0.0.1', resolve));
    const port = reserve.address().port; await new Promise(resolve => reserve.close(resolve));
    server = spawn(process.execPath, ['tools/review/ui-review-server.mjs'], { cwd: repo, env: { ...process.env, PORT: String(port) }, stdio: ['ignore', 'pipe', 'pipe'] });
    await Promise.race([once(server.stdout, 'data'), once(server, 'exit').then(() => { throw new Error('Review server exited'); })]);
    const moduleUrl = `http://127.0.0.1:${port}/modules/all/`;
    assert.equal((await fetch(new URL('state', ready.endpoint))).status, 403);
    const forbidden = new URL('state', ready.endpoint); forbidden.search = new URL(ready.endpoint).search;
    assert.equal((await fetch(forbidden, { headers: { Origin: 'https://untrusted.invalid' } })).status, 403);
    pass('demo API rejects missing tokens and non-loopback browser origins');

    browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
    const page = await browser.newPage({ viewport: { width: 1000, height: 900 } });
    page.setDefaultTimeout(10000); page.on('pageerror', error => pageErrors.push(error.message));
    await page.goto(`${moduleUrl}#demo=${encodeURIComponent(ready.endpoint)}`);
    await page.getByRole('button', { name: 'extension-demo', exact: true }).click();
    await page.getByRole('button', { name: '连接真实 Demo', exact: true }).click();
    const panel = page.getByRole('region', { name: 'Real extension demo', exact: true });
    await panel.getByText('Actual DLL count: 0', { exact: true }).waitFor();
    assert.match(await panel.innerText(), /原生安装：true.*DLL 加载：true/);
    pass('Dioxus connects to installed DLL data through real rpi RPC, not simulated tool events');
    assert(await panel.getByLabel('Demo endpoint', { exact: true }).isDisabled());

    // Start one run through the browser; observe an intermediate native partial.
    await panel.getByRole('button', { name: '运行真实计数工具', exact: true }).click();
    await page.waitForFunction(() => { const progress = document.querySelector('.extension-widgets progress'); return progress && Number(progress.value) > 0 && Number(progress.value) < 4; });
    await panel.getByRole('button', { name: '隐藏实时 UI', exact: true }).click();
    assert.equal(await panel.locator('.extension-widgets').count(), 0);
    const stateUrl = new URL('state', ready.endpoint); stateUrl.search = new URL(ready.endpoint).search;
    const runUrl = new URL('run', ready.endpoint); runUrl.search = stateUrl.search;
    assert.equal((await fetch(runUrl, { method: 'POST' })).status, 409);
    await page.waitForFunction(async address => { const state = await (await fetch(address)).json(); return !state.busy && state.count === 4; }, stateUrl.href);
    await panel.getByRole('button', { name: '显示实时 UI', exact: true }).click();
    await panel.getByText('Actual DLL count: 4', { exact: true }).waitFor();
    assert.equal(await panel.locator('progress').getAttribute('value'), '4');
    pass('actual partials mount live; hiding UI leaves the native run active; concurrent run is rejected');

    await panel.getByRole('button', { name: '运行真实计数工具', exact: true }).click();
    await panel.getByText('Actual DLL count: 8', { exact: true }).waitFor();
    await page.waitForFunction(async address => !(await (await fetch(address)).json()).busy, stateUrl.href);
    fs.mkdirSync('artifacts', { recursive: true });
    await page.screenshot({ path: 'artifacts/extension-demo-desktop.png', fullPage: true });
    await page.setViewportSize({ width: 320, height: 844 });
    assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
    fs.mkdirSync('artifacts', { recursive: true });
    await page.screenshot({ path: 'artifacts/extension-demo-mobile.png', fullPage: true });
    assert.deepEqual(pageErrors, []);
    assert.equal(await panel.getByRole('alert').count(), 0);
    const settledState = await (await fetch(stateUrl)).json();
    assert.deepEqual(settledState.settlements.map(item => item.outcome), ['completed', 'completed']);
    assert.equal(settledState.failure, undefined);
    pass('second real run updates the same UI to count 8; 320px layout and JavaScript checks pass');
  } finally {
    if (browser) await browser.close();
    if (server) server.kill();
    runner.stdin.end('stop\n');
    const timer = setTimeout(() => runner.kill(), 30000);
    try { assert.equal(await runnerExit, 0, errors); } finally { clearTimeout(timer); }
    fs.mkdirSync('artifacts', { recursive: true });
    fs.writeFileSync('artifacts/extension-demo-browser-validation.json', JSON.stringify({ results, pageErrors, hostVersion: ready.hostVersion, model: 'loopback fixed fixture', productionLiveUiProtocol: false }, null, 2));
  }
  const evidence = JSON.parse(fs.readFileSync('artifacts/extension-demo-validation.json', 'utf8'));
  assert.equal(evidence.count, 8);
  assert.equal(evidence.cleanupVerified, true);
  assert.equal(evidence.installed, false);
  assert.equal(evidence.loaded, false);
  assert.equal(evidence.failure, undefined);
  assert.deepEqual(evidence.settlements.map(item => item.outcome), ['completed', 'completed']);
  assert(evidence.frames.filter(frame => frame.type === 'tool_execution_update').length >= 6);
  assert(evidence.checks.some(check => check.includes('unlink')));
  pass('cleanup unlinks real package and a fresh host no longer loads its tool');
})().catch(error => { console.error(error); process.exitCode = 1; });
