// Real Dioxus presentation controls, controlled in-memory AgentTransport. The workspace/HTTP wrapper is tested separately.
// Build once: powershell -File tests/support/build-agent-backends.ps1
// Run: node tests/e2e/verify-agent-backends.cjs
// Optional formal Web supplement: WORK_UI_URL=http://127.0.0.1:14182/app/ node ...
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const http = require('node:http');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');

async function serveHarness() {
  const root = path.resolve(__dirname, '../../target/agent-backends-web');
  assert(fs.existsSync(path.join(root, 'index.html')), 'Build the test harness with tests/support/build-agent-backends.ps1 first');
  const control = { reads: [], values: new Map(), calls: [], pending: [], pendingWrites: [], delayNext: false, delayWriteNext: false, failWriteNext: false, failNext: false };
  const options = session => {
    if (!control.values.has(session)) control.values.set(session, { model: 'provider-a/model-a', mode: 'default' });
    const values = control.values.get(session);
    return [
      { id: 'model', type: 'select', name: 'Model', currentValue: values.model, options: [{ name: 'Provider A', options: [{ value: 'provider-a/model-a', name: 'Model A' }] }, { name: 'Provider B', options: [{ value: 'provider-b/model-b', name: 'Model B' }] }] },
      { id: 'mode', type: 'select', name: 'Mode', currentValue: values.mode, options: [{ value: 'default', name: 'Default' }, { value: 'plan', name: 'Plan' }] },
    ];
  };
  const server = http.createServer((req, res) => {
    const match = new URL(req.url, 'http://localhost').pathname.match(/^\/api\/v1\/sessions\/([^/]+)\/configuration$/);
    if (match) {
      const session = match[1];
      const send = (status, body) => { res.writeHead(status, { 'content-type': 'application/json', 'cache-control': 'no-store' }); res.end(JSON.stringify(body)); };
      if (req.method === 'GET') {
        control.reads.push(session);
        const fail = control.failNext; control.failNext = false;
        const snapshot = { protocol: 'acp', supportedCommands: [], configOptions: options(session) };
        const deliver = () => send(fail ? 500 : 200, fail ? { error: 'stale fixture failure' } : snapshot);
        if (control.delayNext) { control.delayNext = false; control.pending.push(deliver); } else deliver();
      } else if (req.method === 'POST') {
        let body = ''; req.on('data', chunk => { body += chunk; }); req.on('end', () => {
          const value = JSON.parse(body); options(session);
          const fail = control.failWriteNext; control.failWriteNext = false;
          control.calls.push({ session, ...value });
          if (!fail) control.values.get(session)[value.configId] = value.value;
          const deliver = () => send(fail ? 500 : 200, fail ? { error: 'controlled workspace rejection' } : { configOptions: options(session) });
          if (control.delayWriteNext) { control.delayWriteNext = false; control.pendingWrites.push(deliver); } else deliver();
        });
      } else send(405, {});
      return;
    }

    const file = path.resolve(root, `.${new URL(req.url, 'http://localhost').pathname.replace(/\/$/, '/index.html')}`);
    if (!file.startsWith(root + path.sep)) { res.writeHead(403).end(); return; }
    fs.readFile(file, (error, bytes) => {
      if (error) { res.writeHead(404).end(); return; }
      res.writeHead(200, { 'content-type': file.endsWith('.wasm') ? 'application/wasm' : file.endsWith('.js') ? 'text/javascript' : 'text/html', 'cache-control': 'no-store' });
      res.end(bytes);
    });
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  return { control, url: `http://127.0.0.1:${server.address().port}/`, close: () => new Promise(resolve => server.close(resolve)) };
}

(async () => {
  fs.mkdirSync('artifacts', { recursive: true });
  const host = await serveHarness();
  const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
  const page = await browser.newPage({ viewport: { width: 1000, height: 1000 } });
  page.setDefaultTimeout(15000);
  // Disable browser HTTP-cache request coalescing so deliberately overlapping
  // configuration GETs reach the fixture independently.
  await page.route('**/*', route => route.continue());
  const errors = [], results = [], gaps = [
    'Memory fixture tests real BackendMenu/NegotiatedConfigControls, not the workspace/HTTP wrapper or native process lifecycle.',
    'No real ACP adapter, credentials, permissions grants, or model calls; native same-project creation/persisted bindings require desktop integration coverage.'
  ];
  page.on('pageerror', error => errors.push(error.message));
  const pass = text => { results.push(text); console.log('PASS', text); };
  const button = name => page.getByRole('button', { name, exact: true });
  const menu = () => page.locator('.execution-backend-menu');
  const dialog = () => page.getByRole('dialog');
  const summary = () => menu().locator('summary');
  const current = value => page.waitForFunction(value => document.querySelector('.execution-backend-menu summary')?.textContent.includes(value), value);
  const open = async () => { if (!await menu().evaluate(e => e.open)) await summary().click(); };
  const text = id => page.locator(`#${id}`).innerText();
  const entries = async id => (await text(id)).split('\n').filter(Boolean).map(line => JSON.parse(line));
  const valueIs = (name, value) => page.waitForFunction(({ name, value }) => document.querySelector(`[aria-label="ACP ${name}"]`)?.value === value, { name, value });
  const hit = async locator => { const b = await locator.boundingBox(); assert(b && b.height >= 44 && b.width >= 44, `44px target: ${JSON.stringify(b)}`); };
  try {
    await page.goto(host.url);
    await current('rpi');
    assert(await menu().evaluate(e => e.open && e.classList.contains('execution-backend-inline')), 'fixture matches the default-open inline backend tab');
    await summary().click(); assert(!await menu().evaluate(e => e.open)); await open();
    assert.equal(await menu().locator('[aria-pressed="true"]').innerText(), 'RPI');
    assert.equal(await page.locator('.backend-binding').count(), 0, 'no binding details without a selected session');
    await button('claude').click(); await current('claude');
    assert.equal(await dialog().count(), 0); assert.deepEqual(await entries('fixture-creations'), []);
    await button('RPI').click(); await current('rpi');
    pass('RPI default and deselected preference ignore deliberately stale Claude binding, with no hidden creation');

    await button('session-a').click(); await current('claude'); await open();
    assert.match(await page.locator('.backend-binding').innerText(), /claude/);
    await page.getByText('Agent session: external-a', { exact: true }).waitFor();
    await button('codex').click(); await dialog().waitFor();
    await dialog().getByRole('button', { name: '取消', exact: true }).click();
    await current('claude'); assert.equal(await text('fixture-session'), 'session-a');
    assert.deepEqual(await entries('fixture-creations'), []);
    await button('codex').click(); await dialog().waitFor(); await page.keyboard.press('Escape');
    await dialog().waitFor({ state: 'hidden' }); assert.deepEqual(await entries('fixture-creations'), []);
    await button('codex').click(); await dialog().waitFor();
    await dialog().getByRole('button', { name: '确认创建新会话', exact: true }).click();
    await current('codex');
    assert.deepEqual(await entries('fixture-creations'), [{ projectId: 'fixture-project', backend: 'codex', sessionId: 'new-0' }]);
    assert.equal(await page.getByLabel('Session draft').inputValue(), '');
    await button('session-a').click(); await current('claude');
    assert.equal(await page.getByLabel('Session draft').inputValue(), 'draft A');
    await page.getByText('Agent session: external-a', { exact: true }).waitFor();
    await button('session-b').click(); await current('codex'); await open();
    await page.getByText('Agent session: external-b', { exact: true }).waitFor();
    assert.equal(await page.getByLabel('Session draft').inputValue(), 'draft B');
    pass('cancel/Escape never create; confirm emits same-project creation; two session bindings and old drafts remain independent (fixture navigation)');

    await button('Toggle busy').click();
    for (const name of ['RPI', 'claude', 'codex', 'OpenCode']) assert(await button(name).isDisabled());
    await button('Toggle busy').click();
    assert(await button('OpenCode').isDisabled());
    await summary().click(); await button('Toggle remote catalog').click(); await open();
    assert(await button('claude').isDisabled()); assert(await button('codex').isDisabled());
    assert(!await button('RPI').isDisabled());
    await summary().click(); await button('Toggle remote catalog').click();
    await button('Deselect (stale binding)').click(); await current('rpi'); await open();
    assert.equal(await page.locator('.backend-binding').count(), 0);
    pass('busy/unavailable/remote options are disabled; deselection hides the previous session binding');

    await button('session-a').click();
    // Exercise summary collapse; the inline menu never overlays fixture controls.
    if (await menu().evaluate(e => e.open)) await summary().click();
    const model = page.getByLabel('ACP Model', { exact: true });
    const mode = page.getByLabel('ACP Mode', { exact: true });
    assert.deepEqual(await model.locator('optgroup').evaluateAll(es => es.map(e => e.label)), ['Provider A', 'Provider B']);
    assert.deepEqual(await model.locator('option').evaluateAll(es => es.map(e => e.value)), ['provider-a/model-a', 'provider-a/model-long', 'provider-b/model-b']);
    await model.selectOption('provider-b/model-b');
    await page.waitForFunction(() => document.querySelector('[aria-label="ACP Model"]').disabled);
    await page.waitForFunction(() => !document.querySelector('[aria-label="ACP Model"]').disabled);
    await valueIs('Model', 'provider-b/model-b');
    assert.deepEqual((await entries('fixture-calls'))[0], { sessionId: 'session-a', id: 'model', value: 'provider-b/model-b' });
    await mode.selectOption('plan');
    await page.waitForFunction(() => document.querySelector('[aria-label="ACP Mode"]').disabled);
    await page.waitForFunction(() => !document.querySelector('[aria-label="ACP Mode"]').disabled);
    await button('session-b').click(); await valueIs('Model', 'provider-a/model-a'); await valueIs('Mode', 'default');
    await button('session-a').click(); await valueIs('Model', 'provider-b/model-b'); await valueIs('Mode', 'plan');
    pass('grouped model configOptions render native optgroups and send exact advertised ids/values; flat modes and two-session config isolation work');
    const beforeNotification = (await entries('fixture-calls')).length;
    await button('Push config notification').click();
    await valueIs('Model', 'provider-a/model-a');
    assert.equal((await entries('fixture-calls')).length, beforeNotification);
    pass('configuration snapshot changes refresh an idle selector without remounting, changing session or sending a setter');

    const count = (await entries('fixture-calls')).length;
    await button('Toggle running').click(); assert(await model.isDisabled()); assert(await mode.isDisabled());
    assert.equal((await entries('fixture-calls')).length, count);
    await button('Toggle running').click();
    await button('Toggle config failure').click(); await mode.selectOption('default');
    await page.getByRole('alert').filter({ hasText: 'controlled fixture rejection' }).waitFor();
    await button('Toggle config failure').click(); await mode.selectOption('plan');
    await page.getByRole('alert').waitFor({ state: 'hidden' });
    pass('running sessions cannot change configs; setter failures are visible and retryable, without unsafe permission modes');

    for (const width of [320, 390]) {
      await page.setViewportSize({ width, height: 844 }); await open();
      await hit(summary()); await hit(button('RPI')); await hit(button('claude')); await hit(button('codex')); await hit(model); await hit(mode);
      const bounds = await page.locator('.execution-backend-options').boundingBox();
      assert(bounds.x >= 0 && bounds.x + bounds.width <= width + 1, JSON.stringify(bounds));
      assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), 'no horizontal viewport overflow');
      await button('codex').click(); await dialog().waitFor();
      await hit(dialog().getByRole('button', { name: '确认创建新会话' })); await hit(dialog().getByRole('button', { name: '取消', exact: true }));
      const db = await dialog().boundingBox(); assert(db.x >= 0 && db.x + db.width <= width + 1);
      await page.screenshot({ path: `artifacts/agent-backends-${width}.png` });
      await dialog().getByRole('button', { name: '取消', exact: true }).click();
    }
    pass('320/390px backend menu/configs/confirm/cancel have 44px targets and no horizontal overflow');

    // Now exercise the real workspace access wrapper against a credential-free
    // in-memory HTTP fixture (not ACP, native processes, or the production server).
    const until = async predicate => {
      const deadline = Date.now() + 10000;
      while (!predicate()) { assert(Date.now() < deadline, 'fixture request arrived'); await new Promise(resolve => setTimeout(resolve, 10)); }
    };
    await page.setViewportSize({ width: 1000, height: 1000 });
    await button('session-a').click();
    host.control.delayNext = true;
    await button('Use workspace wrapper').click();
    await until(() => host.control.pending.length === 1);
    host.control.values.get('session-a').model = 'provider-b/model-b';
    await button('Reconnect workspace access').click();
    await until(() => host.control.reads.length >= 2);
    await valueIs('Model', 'provider-b/model-b');
    host.control.pending.shift()();
    await page.waitForTimeout(200);
    await valueIs('Model', 'provider-b/model-b');
    assert.equal(await page.getByRole('alert').count(), 0);
    pass('workspace wrapper ignores an older configuration GET after a same-generation reconnect');

    host.control.delayNext = true; host.control.failNext = true;
    await button('Reconnect workspace access').click();
    await until(() => host.control.pending.length === 1);
    await button('Reconnect workspace access').click();
    await valueIs('Model', 'provider-b/model-b');
    host.control.pending.shift()();
    await page.waitForTimeout(200);
    assert.equal(await page.getByRole('alert').count(), 0);
    host.control.delayWriteNext = true;
    await page.getByLabel('ACP Mode', { exact: true }).selectOption('plan');
    await until(() => host.control.pendingWrites.length === 1);
    await button('Reconnect workspace access').click();
    await valueIs('Mode', 'plan');
    assert(await page.getByLabel('ACP Mode', { exact: true }).isDisabled(), 'reconnect must not clear a pending write busy flag');
    host.control.pendingWrites.shift()();
    await page.waitForFunction(() => !document.querySelector('[aria-label="ACP Mode"]').disabled);
    assert.deepEqual(host.control.calls.at(-1), { session: 'session-a', configId: 'mode', value: 'plan' });
    pass('workspace wrapper ignores stale GET errors and keeps setters busy across reconnect until the response completes');

    const beforeParent = host.control.reads.length;
    await button('Use parent session configuration').click();
    await valueIs('Mode', 'plan');
    assert.equal(host.control.reads.length, beforeParent + 1, 'parent observation is reused, not serially fetched again');
    host.control.delayWriteNext = true; host.control.failWriteNext = true;
    await page.getByLabel('ACP Mode', { exact: true }).selectOption('default');
    await until(() => host.control.pendingWrites.length === 1);
    const beforeReconnect = host.control.reads.length;
    await button('Reconnect workspace access').click();
    await until(() => host.control.reads.length > beforeReconnect);
    await valueIs('Mode', 'plan');
    assert(await page.getByLabel('ACP Mode', { exact: true }).isDisabled(), 'parent reload keeps wrapper and pending setter busy');
    host.control.pendingWrites.shift()();
    await page.getByRole('alert').filter({ hasText: 'controlled workspace rejection' }).waitFor();
    await page.waitForTimeout(200);
    assert(await page.getByRole('alert').filter({ hasText: 'controlled workspace rejection' }).isVisible());
    await page.getByLabel('ACP Mode', { exact: true }).selectOption('default');
    await valueIs('Mode', 'default');
    await page.getByRole('alert').filter({ hasText: 'controlled workspace rejection' }).waitFor({ state: 'hidden' });
    pass('parent SessionConfig reuses its initial snapshot, keeps in-flight busy through reconnect, and preserves a write failure until retry');

    // This is supplementary only: formal WASM intentionally has no native ACP lane.
    if (process.env.WORK_UI_URL) {
      const fixture = await startFixture(0);
      try {
        await page.setViewportSize({ width: 1440, height: 1000 });
        await page.goto(process.env.WORK_UI_URL);
        await button('打开设置').click();
        await page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '工作协同', exact: true }).click();
        await button('连接其他节点').click(); await page.getByLabel('节点地址', { exact: true }).fill(fixture.url); await button('连接节点').click();
        await page.getByRole('status').filter({ hasText: '在线' }).first().waitFor(); await button('返回会话').click();
        await page.locator('.project-row').filter({ hasText: 'rpi-work' }).click();
        await page.locator('.sidebar-sessions').getByRole('button', { name: '为工作台添加深色模式', exact: true }).click();
        assert.equal(await page.locator('.execution-backend-menu').count(), 0, 'backend entry is not mounted in the chat composer');
        assert.equal(await page.locator('.composer .negotiated-config').count(), 0);
        const composer = page.getByRole('textbox', { name: '发送给 agent 的消息', exact: true });
        const draft = '执行后端资源页不影响草稿';
        await composer.fill(draft);
        const resources = page.getByRole('dialog', { name: '会话资源配置', exact: true });
        const assertNoCommands = () => {
          assert.equal(fixture.requests.filter(r => r.body.type === 'prompt').length, 0, 'resource navigation must not send prompts');
          assert.equal(fixture.requests.filter(r => r.method === 'POST' && r.path === '/api/v1/sessions').length, 0, 'resource navigation must not create sessions');
        };
        for (const width of [1440, 390, 320]) {
          await page.setViewportSize({ width, height: 1000 });
          assert.equal(await menu().count(), 0, 'backend menu stays unmounted until its resource tab opens');
          assert.equal(await composer.inputValue(), draft);
          await button('打开会话资源配置').click(); await resources.waitFor();
          await resources.getByRole('heading', { name: '会话资源配置', exact: true }).waitFor();
          await resources.evaluate(async element => {
            await Promise.all(element.getAnimations({ subtree: true }).map(animation => animation.finished.catch(() => {})));
          });
          const segments = resources.locator('.resource-segments');
          const backendTab = segments.getByRole('button', { name: '执行后端', exact: true });
          const packagesTab = segments.getByRole('button', { name: 'Packages', exact: true });
          for (const tab of await segments.getByRole('button').all()) await hit(tab);
          assert.equal(await menu().count(), 0, 'Packages does not mount backend controls');
          await backendTab.click();
          const inlineMenu = resources.locator('.execution-backend-menu.execution-backend-inline');
          await inlineMenu.waitFor();
          assert(await inlineMenu.evaluate(e => e.open), 'backend tab is expanded by default');
          assert.equal(await menu().count(), 1, 'only the resource tab mounts a backend menu');
          assert.equal(await inlineMenu.locator('[aria-pressed="true"]').innerText(), 'RPI');
          const rpi = inlineMenu.getByRole('button', { name: 'RPI', exact: true });
          assert(!await rpi.isDisabled(), 'RPI remains available on remote Web nodes');
          await hit(rpi); await rpi.click();
          for (const name of ['Claude', 'Codex', 'OpenCode']) {
            const option = inlineMenu.getByRole('button', { name, exact: true });
            assert(await option.isDisabled(), `${name} cannot start ACP on remote Web nodes`);
            await hit(option);
          }
          assert.equal(await page.getByRole('dialog').count(), 1, 'no new-session confirmation for the current backend');
          assert.equal(await resources.locator('.negotiated-config').count(), 0, 'remote RPI has no native ACP selectors');
          assert.equal(await page.locator('.composer .execution-backend-menu, .composer .negotiated-config').count(), 0);
          assertNoCommands();
          await packagesTab.click();
          assert.equal(await menu().count(), 0, 'leaving the backend tab unmounts its menu');
          await backendTab.click(); await inlineMenu.waitFor();
          assert(await inlineMenu.evaluate(e => e.open), 'returning to backend tab mounts an expanded inline menu');
          const bounds = await resources.boundingBox();
          assert(bounds && bounds.x >= 0 && bounds.x + bounds.width <= width + 1, `resource modal fits ${width}px: ${JSON.stringify(bounds)}`);
          assert(await resources.evaluate(e => e.scrollWidth <= e.clientWidth), 'no horizontal resource modal overflow');
          assert(await resources.locator('.session-resource-body').evaluate(e => e.scrollWidth <= e.clientWidth), 'no horizontal resource body overflow');
          assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), 'no horizontal viewport overflow');
          await page.screenshot({ path: `artifacts/agent-backends-resource-tab-${width === 1440 ? 'desktop' : width}.png` });
          if (width === 390) await page.keyboard.press('Escape');
          else await resources.getByRole('button', { name: '关闭弹框', exact: true }).click();
          await resources.waitFor({ state: 'hidden' });
          assert.equal(await menu().count(), 0, 'closing resources unmounts backend controls instead of revealing them in the composer');
          assert.equal(await page.locator('.composer .negotiated-config').count(), 0);
          assert.equal(await composer.inputValue(), draft, 'opening/switching/closing resource tabs preserves composer drafts');
          assertNoCommands();
        }
        pass('formal resource backend tab is default-open inline, remote-safe and conditionally mounted; Packages round-trip, close/Escape and desktop/390/320px 44px tabs preserve drafts without creating sessions or sending prompts');
      } finally { await fixture.close(); }
    } else gaps.push('Formal Web remote-disabled menu supplement not run: set WORK_UI_URL after the main build.');
    assert.deepEqual(errors, []);
  } catch (error) {
    console.error(await page.locator('select').evaluateAll(es => es.map(e => ({label:e.getAttribute('aria-label'), value:e.value, html:e.outerHTML}))), await page.locator('#fixture-calls').allTextContents());
    await page.screenshot({ path: 'artifacts/agent-backends-failure.png' });
    throw error;
  } finally {
    fs.writeFileSync('artifacts/agent-backends-validation.json', JSON.stringify({ results, errors, coverageGaps: gaps }, null, 2));
    await browser.close(); await host.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
