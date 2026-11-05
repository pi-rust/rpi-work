// Real WASM (build/serve separately at :14180) + isolated HTTP/SSE fixture per case.
// Run against an integrated build; syntax-only validation does not exercise WASM behavior.
// No provider traffic, prompts, queue submissions, credential writes or fixture edits.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
const INITIAL = 'deepseek/deepseek-v4-pro';
const DRAFT = '模型配置期间保留草稿 😀';
const catalog = () => [
  { id: INITIAL, name: 'Runtime original', provider: 'deepseek', reasoning: true },
  { id: 'alpha/a1', name: 'Shared display name', provider: 'alpha', reasoning: true },
  { id: 'alpha/a2', name: 'Alpha second', provider: 'alpha', reasoning: true },
  { id: 'beta/b1', name: 'Shared display name', provider: 'beta', reasoning: true },
];
const results = [], errors = [], geometry = [];
let browser;
async function until(check, description) {
  const end = Date.now() + 10000;
  while (Date.now() < end) { if (await check()) return; await delay(30); }
  throw new Error(`Timed out: ${description}`);
}
function envelope(body, data, error = null) {
  return { type: 'response', command: body.type, id: body.id ?? null, success: error === null, data, error };
}
async function test(name, configure, verify) {
  if (process.env.WORK_UI_PICKER_CASE && !name.startsWith(process.env.WORK_UI_PICKER_CASE)) return;
  let fixture, context, page;
  const started = Date.now(), commands = [], external = [], localErrors = [], gates = [], configRequests = new Map();
  const policy = { models: 'normal', setter: 'normal', readback: 'normal', unsupported: false, catalogUnsupported: false, afterSet: false,
    thinkingSetter: 'normal', thinkingReadback: 'normal', thinkingAfterSet: false, responseGates: {}, thinkingDiscovery: 'normal' };
  const proof = { name, status: 'FAIL', commands, external, errors: localErrors };
  // Gates always expire, and teardown releases them even after an assertion fails.
  function gate(label) {
    let release;
    const item = { label, entered: false, released: false };
    const promise = new Promise(resolve => { release = resolve; });
    let timer;
    item.release = () => { if (!item.released) { item.released = true; clearTimeout(timer); release(); } };
    item.wait = async () => {
      if (!item.entered && !item.released) timer = setTimeout(() => { localErrors.push(`Gate expired: ${label}`); item.release(); }, 15000);
      item.entered = true; await promise;
    };
    gates.push(item); return item;
  }
  try {
    fixture = await startFixture(0);
    fixture.controls.models = catalog();
    context = await browser.newContext({ viewport: { width: 1280, height: 900 } });
    page = await context.newPage(); page.setDefaultTimeout(10000);
    page.on('pageerror', error => localErrors.push(error.message));
    page.on('request', request => {
      if (!request.url().startsWith(fixture.url)) return;
      const configurationRead = request.method() === 'GET' && new URL(request.url()).pathname.endsWith('/configuration');
      const data = request.method() === 'POST' ? request.postDataJSON() : null;
      const type = configurationRead ? 'session_configuration' : data?.type || ({ model: 'set_model', thinking: 'set_thinking_level' })[data?.configId];
      if (['session_configuration', 'get_available_models', 'get_available_thinking_levels', 'get_state', 'set_model', 'set_thinking_level'].includes(type)) configRequests.set(request, type);
    });
    page.on('requestfinished', request => configRequests.delete(request));
    page.on('requestfailed', request => configRequests.delete(request));
    const h = {
      page, fixture, policy, commands, gate,
      trigger: () => page.getByRole('button', { name: '选择供应商与模型', exact: true }),
      dialog: () => page.getByRole('dialog', { name: '选择供应商与模型', exact: true }),
      field: () => page.locator('.composer-main textarea'),
      thinking: () => page.getByRole('button', { name: '思考等级', exact: true }),
      thinkingDialog: () => page.getByRole('dialog', { name: '选择思考等级', exact: true }),
      thinkingOption: level => h.thinkingDialog().locator(`.composer-thinking-option[data-thinking-level=${JSON.stringify(level)}]`),
      async openThinking() {
        await until(async () => !await h.thinking().isDisabled(), 'thinking choices ready');
        await h.thinking().click(); await h.thinkingDialog().waitFor();
        assert.equal(await h.thinkingDialog().getAttribute('id'), 'composer-thinking-picker');
        // The trigger now permits inspecting discovery in flight. Existing
        // choice-oriented cases still need settled discovery before selecting.
        await until(async () => await h.thinkingDialog().getByRole('status').filter({ hasText: '正在加载思考等级' }).count() === 0, 'thinking discovery settled');
      },
      async closeThinking(escape = false) {
        if (escape || await h.thinking().isDisabled()) await page.keyboard.press('Escape');
        else await h.thinking().click();
        await h.thinkingDialog().waitFor({ state: 'hidden' });
        await until(async () => await h.thinking().getAttribute('aria-expanded') === 'false', 'thinking close lifecycle completed');
      },
      async thinkingValue(level) {
        await until(async () => await h.thinking().getAttribute('data-thinking-level') === level, 'authoritative thinking trigger value');
      },
      async thinkingChoice() {
        const current = await h.thinking().getAttribute('data-thinking-level');
        assert(current, 'thinking trigger reports a raw current level');
        const available = await h.thinkingDialog().locator('.composer-thinking-option').evaluateAll(options => options.map(option => option.dataset.thinkingLevel));
        const target = ['off', 'low'].find(level => level !== current && available.includes(level));
        assert(target, 'fixture thinking dialog must offer an alternative off/low level');
        return { current, target, available };
      },
      send: () => page.getByRole('button', { name: '发送消息', exact: true }),
      group: provider => page.locator(`details.composer-provider-group[data-provider="${provider}"]`),
      leaf: (provider, id) => page.locator(`.composer-model-option[data-provider="${provider}"][data-model-id="${id}"]`),
      session: id => page.locator(`[data-session-id="${id}"] .session-item`),
      count: type => commands.filter(item => item.body.type === type).length,
      async open() { await h.trigger().click(); await h.dialog().waitFor(); },
      async expand(provider) {
        const group = h.group(provider); await group.waitFor();
        if (!await group.evaluate(element => element.open)) await group.locator('summary').click();
      },
      async close() {
        await page.keyboard.press('Escape'); await h.dialog().waitFor({ state: 'hidden' });
        // Native close hides the dialog before its close callback completes.
        // Wait for logical closure/focus restoration before sending another key.
        await until(async () => await h.trigger().getAttribute('aria-expanded') === 'false', 'Provider close lifecycle completed');
      },
      async refresh() { await h.dialog().getByRole('button', { name: '刷新模型列表', exact: true }).click(); },
      thinkingRefresh: () => h.thinkingDialog().getByRole('button', { name: '刷新思考等级', exact: true }),
      async backendState() {
        // Read the isolated fixture directly; never send a second setter to
        // manufacture recovery, and do not depend on a timed-out UI response.
        const response = await fetch(`${fixture.url.replace(/\/$/, '')}/api/v1/sessions/dark/commands`, {
          method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ type: 'get_state' }),
        });
        assert(response.ok); const snapshot = await response.json(); assert.equal(snapshot.success, true);
        return snapshot.data;
      },
      async deadline(held) {
        await until(() => held.entered, `${held.label} captured by fixture`);
        // A refresh starts three independent RPCs. Settle unheld siblings before
        // jumping the clock, otherwise fastForward could timeout a perfectly
        // healthy sibling merely because its HTTP response is still in flight.
        await until(() => [...configRequests.values()].every(type => type === held.snapshot.command), 'unheld config responses finished');
        await delay(50); // allow WASM continuations to consume those responses
        assert.equal(held.released, false, 'operation is still genuinely pending at the WASM deadline');
        // This advances the browser's actual gloo timer, NOT the Node gate's
        // 15s wall-clock expiry. Never sleep for the deadline in real time.
        await page.clock.fastForward(15_100);
      },
      async choose(provider = 'alpha', id = 'alpha/a1') { await h.expand(provider); await h.leaf(provider, id).click(); },
      async title(provider, id) {
        await until(async () => await h.trigger().getAttribute('title') === `${provider} / ${id}`, 'authoritative trigger title');
        const reported = catalog().find(model => model.provider === provider && model.id === id);
        assert([reported?.name, id].includes((await h.trigger().innerText()).trim()), 'trigger shows a catalog model name or raw runtime ID, never just a provider');
      },
      async ready() {
        await h.send().waitFor();
        await page.waitForFunction(() => {
          const field = document.querySelector('.composer-main textarea');
          return field && !field.disabled && field.dataset.imeReady === 'true';
        });
      },
      async noWrites() {
        await delay(150);
        assert.equal(h.count('set_model'), 0);
        assert.equal(h.count('set_thinking_level'), 0);
      },
      async unconfirmed(dialog) {
        await dialog.locator('.composer-config-unconfirmed').waitFor();
        assert(await h.send().isDisabled(), 'unconfirmed config blocks sending');
        const before = ['set_model', 'set_thinking_level', 'get_state', 'get_available_models', 'get_available_thinking_levels'].map(h.count);
        const options = dialog.locator('.composer-model-option, .composer-thinking-option');
        assert(await options.count() > 0, 'recovery keeps existing choices visible');
        for (const option of await options.all()) {
          assert(await option.isDisabled(), 'unconfirmed config blocks every new mutation');
          await option.evaluate(button => { button.click(); button.click(); });
        }
        await h.field().evaluate(element => element.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', code: 'Enter', bubbles: true, cancelable: true })));
        await h.page.evaluate(() => document.activeElement?.blur());
        await h.page.keyboard.press('Enter');
        await h.send().evaluate(button => button.click());
        await delay(150);
        assert.deepEqual(['set_model', 'set_thinking_level', 'get_state', 'get_available_models', 'get_available_thinking_levels'].map(h.count), before,
          'blocked choices, Enter and idle time must not retry setters or automatically refresh');
        assert.equal(h.count('prompt'), 0); assert.equal(h.count('queue_message'), 0);
        assert.equal(await h.field().inputValue(), DRAFT);
        assert(await dialog.isVisible(), 'blocked interactions retain the recovery dialog');
      },
    };
    if (configure) await configure(h);
    const uiOrigin = new URL(process.env.WORK_UI_URL || 'http://127.0.0.1:14180/').origin;
    const fixtureOrigin = new URL(fixture.url).origin;
    // Only the real UI server and this case's fixture are allowed. GitHub's
    // background update check is stubbed, never forwarded to the public network.
    await context.route('**/*', async route => {
      const request = route.request(), url = new URL(request.url());
      if (!['http:', 'https:'].includes(url.protocol)) return route.continue();
      if (url.origin === 'https://api.github.com') return route.fulfill({ status: 404, json: {} });
      if (![uiOrigin, fixtureOrigin].includes(url.origin)) {
        external.push(request.url()); return route.abort('blockedbyclient');
      }
      if (url.origin !== fixtureOrigin) return route.continue();
      if (url.pathname === '/api/v1/capabilities' && policy.globalCatalogUnsupported) {
        const response = await route.fetch(), data = await response.json();
        data.supportedCommands = data.supportedCommands.filter(command => !['get_available_models', 'set_model', 'get_available_thinking_levels', 'set_thinking_level'].includes(command));
        return route.fulfill({ response, json: data });
      }
      if (request.method() === 'GET' && url.pathname.endsWith('/configuration')) {
        if (policy.configurationStatus) return route.fulfill({ status: policy.configurationStatus, json: { error: 'fixture configuration observation unavailable' } });
        const response = await route.fetch(), data = await response.json();
        if (policy.thinkingUnsupported) data.supportedCommands = data.supportedCommands.filter(command => command !== 'set_thinking_level');
        if (policy.unsupported) {
          data.supportedCommands = data.supportedCommands.filter(command => command !== 'set_model');
          assert(data.supportedCommands.includes('get_available_models'));
        }
        if (policy.catalogUnsupported) {
          data.supportedCommands = data.supportedCommands.filter(command => command !== 'get_available_models');
          assert(data.supportedCommands.includes('set_model'));
        }
        if (policy.configurationGate) {
          const held = policy.configurationGate; policy.configurationGate = null;
          held.snapshot = { command: 'session_configuration', data: structuredClone(data) };
          await held.wait();
          await route.fulfill(held.failureStatus ? { status: held.failureStatus, json: { error: 'stale observation failure' } } : { response, json: data });
          held.delivered = true; return;
        }
        return route.fulfill({ response, json: data });
      }
      if (request.method() !== 'POST' || !['/commands', '/configuration'].some(suffix => url.pathname.endsWith(suffix))) return route.continue();
      const wireBody = request.postDataJSON(), typed = url.pathname.endsWith('/configuration');
      const body = typed ? (wireBody.configId === 'model' ? { type: 'set_model', modelId: wireBody.value } : { type: 'set_thinking_level', level: wireBody.value }) : wireBody;
      commands.push({ path: url.pathname, body, wireBody });
      // Policies operate on the same logical setter; the actual wire response
      // remains the typed endpoint's raw Value, never a command envelope.
      const snapshotOf = async response => typed ? envelope(body, await response.json()) : response.json();
      const fulfill = options => {
        if (typed && options.json) {
          const snapshot = options.json;
          options = { ...options, status: snapshot.success ? 200 : 500, json: snapshot.success ? snapshot.data : { error: snapshot.error } };
        }
        return route.fulfill(options);
      };
      const reply = (data, error = null) => fulfill({ status: 200,
        headers: { 'access-control-allow-origin': uiOrigin, 'access-control-allow-credentials': 'true' },
        json: envelope(body, data, error) });
      if (['prompt', 'queue_message', 'set_api_key'].includes(body.type)) {
        localErrors.push(`Forbidden command: ${body.type}`);
        return reply(null, 'This test forbids prompts, queue sends and credential writes');
      }
      // New timeout cases opt into one-shot RESPONSE gates. Consume before
      // awaiting, then fetch/mutate the real fixture and capture its response.
      // A setter gate therefore holds only its ACK, never the actual mutation.
      // Original policies below are unchanged for all existing race cases.
      const responseGate = policy.responseGates[body.type];
      if (responseGate && (body.type !== 'get_state' || url.pathname.includes('/dark/'))) {
        delete policy.responseGates[body.type];
        if (body.type === 'set_model') policy.afterSet = true;
        if (body.type === 'set_thinking_level') policy.thinkingAfterSet = true;
        const response = await route.fetch(), snapshot = await snapshotOf(response);
        responseGate.snapshot = snapshot;
        if (Object.hasOwn(responseGate, 'replyData')) snapshot.data = responseGate.replyData;
        await responseGate.wait();
        await fulfill({ response, json: snapshot }); responseGate.delivered = true; return;
      }
      if (body.type === 'get_available_models') {
        if (policy.modelGate) await policy.modelGate.wait();
        if (policy.models === 'fail') return reply(null, 'fixture catalog unavailable');
        if (policy.models === 'empty') return reply({ models: [] });
      }
      if (body.type === 'get_available_thinking_levels') {
        if (policy.thinkingGate) await policy.thinkingGate.wait();
        if (policy.thinkingDiscovery === 'fail') return reply(null, 'fixture thinking discovery unavailable');
        if (policy.thinkingEmpty) {
          await reply({ levels: [] }); policy.thinkingDiscoveryDone = true; return;
        }
      }
      if (body.type === 'set_thinking_level') {
        policy.thinkingAfterSet = true;
        if (policy.thinkingSetter === 'reject') return reply(null, 'fixture thinking setter rejected');
        // Mutate the real fixture before delaying the acknowledgment. These
        // policies are thinking-only and cannot capture model readback tests.
        const response = await route.fetch();
        const held = policy.thinkingSetGate; policy.thinkingSetGate = null;
        if (held) await held.wait();
        if (policy.thinkingSetter === 'requires-restart') {
          const snapshot = await snapshotOf(response); snapshot.data.requiresRestart = true;
          return fulfill({ response, json: snapshot });
        }
        await fulfill({ response }); if (held) held.delivered = true; return;
      }
      // These one-shot gates take precedence over the generic afterSet handler.
      // Consume synchronously, capture a snapshot BEFORE waiting, and let newer
      // requests (including same-address reconnect hydration) proceed normally.
      if (body.type === 'get_state' && url.pathname.includes('/dark/')) {
        const key = policy.refreshGate ? 'refreshGate' : policy.thinkingStateGate ? 'thinkingStateGate' : null;
        if (key) {
          const held = policy[key]; policy[key] = null;
          const response = await route.fetch();
          const snapshot = await snapshotOf(response);
          held.snapshot = snapshot;
          await held.wait();
          await fulfill({ response, json: snapshot }); held.delivered = true; return;
        }
      }
      if (body.type === 'get_state' && policy.thinkingAfterSet && url.pathname.includes('/dark/') && policy.thinkingReadback !== 'normal') {
        if (policy.thinkingReadback === 'fail') return reply(null, 'fixture thinking readback unavailable');
        const response = await route.fetch(), snapshot = await snapshotOf(response);
        if (policy.thinkingReadback === 'no-state') snapshot.data = null;
        else if (policy.thinkingReadback === 'missing') delete snapshot.data.thinkingLevel;
        else snapshot.data.thinkingLevel = policy.thinkingReadback === 'blank' ? '   ' : policy.thinkingOld;
        return fulfill({ response, json: snapshot });
      }
      if (body.type === 'set_model') {
        policy.afterSet = true;
        if (policy.setGate) await policy.setGate.wait();
        if (policy.setter === 'reject') return reply(null, 'fixture setter rejected');
        // Actually mutate the isolated fixture. Never synthesize a successful
        // setter without changing runtime, except deliberately stale readback.
        const response = await route.fetch();
        if (policy.setter === 'requires-restart') {
          const snapshot = await snapshotOf(response); snapshot.data.requiresRestart = true;
          return fulfill({ response, json: snapshot });
        }
        return fulfill({ response });
      }
      if (body.type === 'get_state' && policy.afterSet && url.pathname.includes('/dark/')) {
        // Capture the true response before waiting: a late old-scope response
        // remains old even if another session is selected in the meantime.
        const held = policy.stateGate;
        if (held && policy.stateGateOnce) policy.stateGate = null;
        const response = await route.fetch();
        const snapshot = await snapshotOf(response);
        if (held) { held.snapshot = snapshot; await held.wait(); }
        if (policy.readback === 'fail') return reply(null, 'fixture readback unavailable');
        if (policy.readback === 'no-state') snapshot.data = null;
        else if (policy.readback === 'missing') delete snapshot.data.modelId;
        else if (policy.readback === 'blank') snapshot.data.modelId = '   ';
        if (policy.readback === 'old' || policy.readback === 'wrong-provider') {
          const data = snapshot;
          if (policy.readback === 'old') { data.data.modelId = INITIAL; data.data.provider = 'deepseek'; }
          else data.data.provider = 'beta';
          return fulfill({ response, json: data });
        }
        await fulfill({ response, json: snapshot });
        if (held) held.delivered = true;
        return;
      }
      return route.continue();
    });
    await page.goto(process.env.WORK_UI_URL || 'http://127.0.0.1:14180/');
    await page.getByRole('button', { name: '打开设置', exact: true }).click();
    await page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '执行节点', exact: true }).click();
    await page.getByRole('button', { name: '连接其他节点', exact: true }).click();
    await page.getByLabel('节点地址', { exact: true }).fill(fixture.url);
    await page.getByRole('button', { name: '连接节点', exact: true }).click();
    await page.locator('.server-details [role=status]').filter({ hasText: '在线' }).waitFor();
    await page.getByRole('button', { name: '返回会话', exact: true }).click();
    if (!await h.session('dark').isVisible()) await page.locator('.project-row').filter({ hasText: 'rpi-work' }).click();
    await h.session('dark').click();
    // Snapshot seed contains agent_start AND agent_settled. Do not select during
    // its transient busy state or use catalog arrival as proof of idle runtime.
    await page.locator('.transcript').getByText('主题切换已经准备好，接下来运行组件测试。', { exact: true }).waitFor();
    await h.ready(); await h.title('deepseek', INITIAL);
    await h.field().fill(DRAFT);
    await verify(h);
    assert.equal(h.count('prompt'), 0); assert.equal(h.count('queue_message'), 0);
    assert.equal(h.count('set_api_key'), 0);
    assert.equal(fixture.requests.filter(r => ['prompt', 'queue_message', 'set_api_key'].includes(r.body.type)).length, 0);
    assert.deepEqual(external, []); assert.deepEqual(localErrors, []);
    for (const item of commands.filter(item => ['set_model', 'set_thinking_level'].includes(item.body.type))) {
      assert(item.path.endsWith('/configuration'), 'composer writes use the typed workspace endpoint');
      assert.deepEqual(Object.keys(item.wireBody).sort(), ['configId', 'value']);
      assert.equal(item.wireBody.configId, item.body.type === 'set_model' ? 'model' : 'thinking');
    }
    proof.status = 'PASS'; console.log('PASS', name);
  } catch (error) {
    proof.error = error.stack || String(error); errors.push({ name, error: proof.error });
    console.error('FAIL', name, error.message);
    if (page) await page.screenshot({ path: `artifacts/composer-model-picker-failure-${results.length + 1}.png`, animations: 'disabled' }).catch(() => {});
  } finally {
    gates.forEach(item => item.release());
    proof.durationMs = Date.now() - started;
    results.push(proof);
    fs.writeFileSync('artifacts/composer-model-picker-validation.json', JSON.stringify({ results, geometry, errors, assumptions: [
      'Real integrated WASM is already built and served at WORK_UI_URL or http://127.0.0.1:14180/.',
      'ModelPicker/ThinkingPicker DOM and Chinese labels match model_picker.rs; composer uses idle/write_supported and IME readiness.',
      'Fixture controls.models supports original catalog IDs; set_model mutates per-session config and get_state derives provider from raw ID prefix.',
      'Main composer discovery/mutation/readback guards both generation and connection_epoch after every await; stale session replies must not affect current scope.',
      'Deadline cases require Playwright page.clock: install after readiness, settle unheld RPC siblings, then fastForward(15_100) while one actual HTTP response gate remains held.',
      'Any accepted setter followed by unconfirmed readback or requiresRestart requires explicit nonblank-field get_state recovery; neither late responses nor recovery may invent a commit revision, retry a setter, or close a reopened dialog.',
    ], limitations: 'Browser execution requires a separately built/served integrated WASM UI; node --check validates syntax only. Model session and same-address reconnect late-readback races are covered by cases 12 and 16; thinking session isolation by case 20. Light/dark 1280/390/320 screenshots check both dialogs, with thinking selected/focused options, 44px option hit targets and draft retention. Physical mobile keyboard acceptance requires separate validation. Existing mobile composer tests own the 44px primary hit target/32px visible-circle contract.' }, null, 2));
    if (context) await context.close().catch(() => {});
    if (fixture) await fixture.close();
  }
}
(async () => {
  fs.mkdirSync('artifacts', { recursive: true });
  try {
    browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
    await test('01 provider grouping, raw identity, browsing and Escape never mutate', null, async h => {
      await h.open(); await h.expand('alpha'); await h.expand('beta');
      assert.equal(await h.group('alpha').locator('.composer-model-option').count(), 2);
      assert.equal(await h.group('beta').locator('.composer-model-option').count(), 1);
      for (const [provider, id] of [['alpha', 'alpha/a1'], ['beta', 'beta/b1']]) {
        assert.equal(await h.leaf(provider, id).locator('.composer-model-label > span').innerText(), 'Shared display name');
        assert.equal(await h.leaf(provider, id).locator('code').innerText(), id);
      }
      assert.equal(await h.leaf('deepseek', INITIAL).getAttribute('aria-pressed'), 'true');
      await h.close(); await h.noWrites(); assert.equal(await h.field().inputValue(), DRAFT);
    });
    await test('02 cross-provider duplicate raw IDs are disabled', h => {
      h.fixture.controls.models.push({ id: 'ambiguous', name: 'Duplicate A', provider: 'alpha' }, { id: 'ambiguous', name: 'Duplicate B', provider: 'beta' });
    }, async h => {
      await h.open(); await h.expand('alpha'); await h.expand('beta');
      for (const provider of ['alpha', 'beta']) {
        assert(await h.leaf(provider, 'ambiguous').isDisabled());
        await h.leaf(provider, 'ambiguous').evaluate(button => button.click());
      }
      assert.equal(await h.leaf('alpha', 'alpha/a1').isDisabled(), false);
      await h.dialog().getByText(/多个供应商报告了相同模型 ID/).waitFor(); await h.noWrites();
    });
    await test('03 catalog failure, empty result and explicit retry recover', h => { h.policy.models = 'fail'; }, async h => {
      await h.open(); await h.dialog().getByRole('alert').waitFor();
      const before = h.count('get_available_models'); h.policy.models = 'empty'; await h.refresh();
      await h.dialog().getByText(/暂无可用模型/).waitFor();
      assert(h.count('get_available_models') > before);
      h.policy.models = 'normal'; await h.refresh(); await h.expand('beta');
      await h.leaf('beta', 'beta/b1').waitFor(); await h.noWrites();
    });
    await test('04 catalog loading state settles independently of delayed thinking discovery', h => {
      h.policy.modelGate = h.gate('catalog'); h.policy.thinkingGate = h.gate('thinking discovery');
    }, async h => {
      await until(() => h.policy.modelGate.entered && h.policy.thinkingGate.entered, 'both independent requests started');
      await h.open(); await h.dialog().getByRole('status').filter({ hasText: '正在加载模型列表' }).waitFor();
      assert(await h.dialog().getByRole('button', { name: '刷新模型列表', exact: true }).isDisabled());
      h.policy.modelGate.release(); await h.expand('alpha');
      await until(async () => !await h.leaf('alpha', 'alpha/a1').isDisabled(), 'catalog ready while thinking gate remains held');
      assert.equal(h.policy.thinkingGate.released, false);
      assert.equal(await h.thinking().isDisabled(), false, 'pending thinking discovery remains inspectable/retryable');
      // Release immediately after proving independence, before later lock checks.
      h.policy.thinkingGate.release(); await h.close(); await h.noWrites();
    });
    await test('05 rejected setter retains current runtime, dialog and draft', h => { h.policy.setter = 'reject'; }, async h => {
      await h.open(); await h.choose(); await h.dialog().getByRole('alert').waitFor();
      assert.equal(h.count('set_model'), 1); await h.title('deepseek', INITIAL);
      assert.equal(await h.field().inputValue(), DRAFT);
      await until(async () => !await h.leaf('alpha', 'alpha/a1').isDisabled(), 'rejection clears busy');
    });
    await test('06 failed authoritative readback cannot commit setter response; explicit refresh recovers', h => { h.policy.readback = 'fail'; }, async h => {
      await h.open(); await h.choose(); await h.dialog().getByRole('alert').waitFor();
      await h.title('deepseek', INITIAL); assert.equal(h.count('set_model'), 1);
      assert.equal((await h.backendState()).modelId, 'alpha/a1', 'setter really changed runtime despite failed readback');
      assert.equal(await h.field().inputValue(), DRAFT);
      await h.unconfirmed(h.dialog());
      const stateBefore = h.count('get_state'), modelsBefore = h.count('get_available_models');
      h.policy.readback = 'normal'; await h.refresh();
      await h.title('alpha', 'alpha/a1');
      await until(() => h.count('get_state') > stateBefore && h.count('get_available_models') > modelsBefore, 'refresh rehydrates state and catalog');
      assert(await h.dialog().isVisible(), 'refresh recovery does not masquerade as a new model commit');
      await h.expand('alpha');
      await until(async () => await h.leaf('alpha', 'alpha/a1').getAttribute('aria-pressed') === 'true', 'refreshed authoritative choice');
      assert.equal(h.count('set_model'), 1, 'refresh must not retry the setter');
      assert.equal(h.count('get_state'), stateBefore + 1, 'only explicit refresh initiates recovery');
      assert.equal(h.count('set_thinking_level'), 0);
      await until(async () => await h.dialog().locator('.composer-config-unconfirmed').count() === 0 && !await h.leaf('alpha', 'alpha/a1').isDisabled(), 'refresh clears unknown and unlocks model choices');
      await h.close(); await until(async () => !await h.send().isDisabled(), 'refresh recovery leaves send unlocked');
      assert.equal(await h.field().inputValue(), DRAFT);
    });
    await test('07 old model and wrong provider readbacks lock mutations until separate explicit refreshes', h => { h.policy.readback = 'old'; }, async h => {
      await h.open();
      for (const [mode, id, previous] of [['old', 'alpha/a1', INITIAL], ['wrong-provider', 'alpha/a2', 'alpha/a1']]) {
        h.policy.readback = mode;
        await h.choose('alpha', id); await h.dialog().getByRole('alert').waitFor();
        await h.title(previous === INITIAL ? 'deepseek' : 'alpha', previous);
        assert.equal((await h.backendState()).modelId, id, `${mode}: fixture changed, display remains previously confirmed`);
        const setters = h.count('set_model');
        await h.unconfirmed(h.dialog());
        const reads = h.count('get_state'); h.policy.readback = 'normal'; await h.refresh();
        await h.title('alpha', id);
        await until(async () => await h.dialog().locator('.composer-config-unconfirmed').count() === 0 && !await h.leaf('alpha', 'alpha/a2').isDisabled(), `${mode}: explicit true state unlocks next mutation`);
        assert(await h.dialog().isVisible(), 'recovery is not another commit');
        assert.equal(h.count('set_model'), setters, `${mode}: recovery never repeats the setter`);
        assert.equal(h.count('get_state'), reads + 1, `${mode}: no automatic recovery readback`);
      }
      assert.equal(h.count('set_model'), 2); assert.equal(h.count('set_thinking_level'), 0);
      await h.close(); await until(async () => !await h.send().isDisabled(), 'both mismatch scopes recovered');
      assert.equal(await h.field().inputValue(), DRAFT);
    });
    await test('08 double-click sends one original raw ID; commit waits for true get_state', h => {
      h.policy.setGate = h.gate('setter acknowledgment'); h.policy.stateGate = h.gate('authoritative readback');
    }, async h => {
      await h.open(); await h.expand('beta');
      await h.leaf('beta', 'beta/b1').evaluate(button => { button.click(); button.click(); });
      await until(() => h.policy.setGate.entered, 'setter entered'); await h.title('deepseek', INITIAL);
      assert.equal(h.count('set_model'), 1);
      assert.equal(h.commands.find(r => r.body.type === 'set_model').body.modelId, 'beta/b1');
      h.policy.setGate.release(); await until(() => h.policy.stateGate.entered, 'readback entered');
      await h.title('deepseek', INITIAL); assert(await h.dialog().isVisible());
      h.policy.stateGate.release(); await h.dialog().waitFor({ state: 'hidden' }); await h.title('beta', 'beta/b1');
      assert.equal(h.count('set_model'), 1); assert.equal(await h.field().inputValue(), DRAFT);
      await h.open(); await h.expand('beta');
      assert.equal(await h.leaf('beta', 'beta/b1').getAttribute('aria-pressed'), 'true');
      assert.equal(await h.leaf('beta', 'beta/b1').locator('code').innerText(), 'beta/b1');
      await h.leaf('beta', 'beta/b1').click(); await h.dialog().waitFor({ state: 'hidden' });
      assert.equal(h.count('set_model'), 1, 'clicking current model must be a no-op');
    });
    await test('09 mutation locks send, Enter and thinking until readback settles', h => {
      h.policy.setGate = h.gate('config setter'); h.policy.stateGate = h.gate('config readback');
    }, async h => {
      await h.open(); await h.choose(); await until(() => h.policy.setGate.entered, 'config pending');
      await h.close();
      async function assertLocked() {
        assert(await h.send().isDisabled());
        const thinking = h.thinking();
        assert.equal(await thinking.count(), 1, 'main composer thinking button must expose its existing label');
        assert(await thinking.isDisabled());
        // Adversarial dispatch supplements native keyboard: disabled controls
        // must also have a handler-level guard, with zero prompt/queue requests.
        await h.field().evaluate(element => element.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', code: 'Enter', bubbles: true, cancelable: true })));
        if (!await h.field().isDisabled()) await h.field().press('Enter');
        else {
          await h.page.evaluate(() => document.activeElement?.blur());
          await h.page.keyboard.press('Enter');
        }
        await h.send().evaluate(button => button.click()); await delay(100);
        assert.equal(h.count('prompt'), 0); assert.equal(h.count('queue_message'), 0);
        assert.equal(await h.field().inputValue(), DRAFT);
      }
      await assertLocked(); h.policy.setGate.release();
      await until(() => h.policy.stateGate.entered, 'readback pending'); await assertLocked();
      h.policy.stateGate.release(); await h.title('alpha', 'alpha/a1'); await h.ready();
      await until(async () => !await h.send().isDisabled(), 'send unlocked after verified commit');
      assert.equal(await h.field().inputValue(), DRAFT); assert.equal(h.count('set_thinking_level'), 0);
    });
    await test('10 active run allows provider browsing but locks leaf mutation', null, async h => {
      h.fixture.emit({ type: 'agent_start', runId: 'picker-active-run' });
      await h.page.getByRole('button', { name: '停止运行', exact: true }).waitFor();
      assert(await h.thinking().isDisabled(), 'active run disables thinking trigger');
      await h.open(); await h.dialog().getByText('运行中暂不可切换模型', { exact: true }).waitFor();
      await h.expand('alpha'); await h.expand('beta');
      assert(await h.leaf('alpha', 'alpha/a1').isDisabled());
      await h.leaf('alpha', 'alpha/a1').evaluate(button => button.click()); await h.noWrites();
      h.fixture.emit({ type: 'agent_settled', runId: 'picker-active-run', outcome: 'completed' });
      await until(async () => !await h.leaf('alpha', 'alpha/a1').isDisabled(), 'settlement unlocks leaf');
      await h.close(); await h.ready(); assert.equal(await h.field().inputValue(), DRAFT);
    });
    await test('11 missing set_model capability retains read-only browsing with reason', h => { h.policy.unsupported = true; }, async h => {
      await h.open(); await h.expand('alpha');
      await h.dialog().getByText(/当前节点不支持切换模型/).waitFor();
      assert(await h.leaf('alpha', 'alpha/a1').isDisabled());
      await h.leaf('alpha', 'alpha/a1').evaluate(button => button.click());
      await h.refresh(); await h.expand('beta');
      await until(() => h.count('get_available_models') >= 2, 'read-only refresh request');
      assert(await h.leaf('beta', 'beta/b1').isDisabled()); await h.noWrites();
    });
    await test('12 late session readback has no overwrite/busy bleed; light/dark 1280/390/320 fit', h => {
      h.policy.stateGate = h.gate('old dark session readback');
    }, async h => {
      await h.open(); await h.choose(); await until(() => h.policy.stateGate.entered, 'old readback pending');
      await h.close(); await h.session('architecture').click(); await h.ready();
      await h.field().fill('架构会话独立草稿'); await h.title('deepseek', INITIAL);
      await h.open(); await h.expand('beta');
      assert.equal(await h.leaf('beta', 'beta/b1').isDisabled(), false, 'old busy must not bleed into new session');
      h.policy.stateGate.release(); await delay(250);
      assert(await h.dialog().isVisible(), 'old completion must not close a new-scope dialog');
      await h.title('deepseek', INITIAL); assert.equal(await h.field().inputValue(), '架构会话独立草稿');
      await h.close();
      // A fresh mutation on the new session proves the stale task did not clear
      // or poison its generation/busy ownership.
      await h.open(); await h.choose('beta', 'beta/b1'); await h.dialog().waitFor({ state: 'hidden' }); await h.title('beta', 'beta/b1');
      await h.session('dark').click(); await h.ready(); await h.title('alpha', 'alpha/a1');
      assert.equal(await h.field().inputValue(), DRAFT);
      for (const mode of ['light', 'dark']) {
        await h.page.setViewportSize({ width: 1280, height: 900 });
        await h.page.getByRole('button', { name: '外观与主题', exact: true }).click();
        const theme = h.page.getByRole('dialog', { name: '外观与主题', exact: true });
        await theme.getByLabel('明暗模式', { exact: true }).selectOption(mode);
        await theme.getByRole('button', { name: '保存', exact: true }).click(); await theme.waitFor({ state: 'hidden' });
        for (const width of [1280, 390, 320]) {
          await h.page.setViewportSize({ width, height: 900 }); await h.open(); await h.expand('alpha'); await h.expand('beta');
          await h.dialog().evaluate(async element => { await Promise.all(element.getAnimations({ subtree: true }).map(animation => animation.finished.catch(() => {}))); });
          const sizes = await h.dialog().evaluate(element => {
            const rect = element.getBoundingClientRect();
            return { left: rect.left, right: rect.right, bottom: rect.bottom, viewport: innerWidth, viewportHeight: innerHeight,
              scroll: element.scrollWidth, client: element.clientWidth,
              targets: [...element.querySelectorAll('summary, .composer-model-option, .composer-model-refresh')].filter(e => e.getClientRects().length).map(e => ({ height: e.getBoundingClientRect().height, right: e.getBoundingClientRect().right })) };
          });
          geometry.push({ mode, width, ...sizes });
          assert(sizes.left >= 0 && sizes.right <= width + 1 && sizes.bottom <= sizes.viewportHeight + 1, JSON.stringify(sizes));
          assert(sizes.scroll <= sizes.client + 1, 'dialog has no horizontal overflow');
          assert(sizes.targets.every(target => target.height >= (width <= 700 ? 43.5 : 27.5) && target.right <= width + 1), JSON.stringify(sizes));
          await h.page.screenshot({ path: `artifacts/composer-model-picker-${mode}-${width}.png`, animations: 'disabled' });
          await h.close(); assert.equal(await h.field().inputValue(), DRAFT);
          assert(await h.trigger().isVisible());
          await h.openThinking();
          const selected = await h.thinking().getAttribute('data-thinking-level');
          assert(selected, 'screenshots require an actual authoritative selection');
          assert.equal(await h.thinkingOption(selected).getAttribute('aria-pressed'), 'true');
          assert.equal(await h.thinkingDialog().locator('.composer-thinking-option[aria-pressed=true]').count(), 1);
          await h.thinkingOption(selected).focus();
          await h.thinkingDialog().evaluate(async element => { await Promise.all(element.getAnimations({ subtree: true }).map(animation => animation.finished.catch(() => {}))); });
          const thinkingSizes = await h.thinkingDialog().evaluate(element => {
            const rect = element.getBoundingClientRect();
            return { left: rect.left, right: rect.right, top: rect.top, bottom: rect.bottom, viewportHeight: innerHeight,
              scroll: element.scrollWidth, client: element.clientWidth, pageScroll: document.documentElement.scrollWidth,
              focusInside: element.contains(document.activeElement),
              targets: [...element.querySelectorAll('.composer-thinking-option')].filter(e => e.getClientRects().length).map(e => {
                const r = e.getBoundingClientRect(); return { width: r.width, height: r.height, left: r.left, right: r.right };
              }) };
          });
          geometry.push({ picker: 'thinking', mode, width, selected, ...thinkingSizes });
          assert(thinkingSizes.left >= 0 && thinkingSizes.top >= 0 && thinkingSizes.right <= width + 1 && thinkingSizes.bottom <= thinkingSizes.viewportHeight + 1, JSON.stringify(thinkingSizes));
          assert(thinkingSizes.scroll <= thinkingSizes.client + 1 && thinkingSizes.pageScroll <= width + 1, 'thinking dialog and page have no horizontal overflow');
          assert(thinkingSizes.targets.length >= 2 && thinkingSizes.targets.every(target => target.width >= 43.5 && target.height >= (width <= 700 ? 43.5 : 35.5) && target.left >= 0 && target.right <= width + 1), JSON.stringify(thinkingSizes));
          assert(thinkingSizes.focusInside, 'thinking screenshot includes focused selected option');
          assert(await h.thinkingOption(selected).evaluate(element => element === document.activeElement));
          await h.page.screenshot({ path: `artifacts/composer-thinking-picker-${mode}-${width}.png`, animations: 'disabled' });
          await h.closeThinking(true);
          await until(() => h.thinking().evaluate(element => element === document.activeElement), 'Escape restores thinking trigger focus');
          assert.equal(await h.field().inputValue(), DRAFT);
          await h.thinkingValue(selected);
        }
      }
      assert.equal(h.count('set_model'), 2); assert.equal(h.count('set_thinking_level'), 0, 'thinking screenshots only browse');
    });
    await test('13 missing get_available_models capability explains unavailable catalog without discovery', h => {
      h.policy.catalogUnsupported = true;
    }, async h => {
      await h.open();
      await h.dialog().getByText('当前节点不支持读取模型列表。', { exact: true }).waitFor();
      assert.equal(await h.dialog().locator('.composer-model-option').count(), 0);
      assert.equal(await h.dialog().locator('.composer-provider-group').count(), 0);
      assert.equal(h.count('get_available_models'), 0, 'set_model availability must not imply catalog support');
      await h.refresh();
      await h.dialog().getByText('当前节点不支持读取模型列表。', { exact: true }).waitFor();
      await h.close(); await h.open();
      assert.equal(await h.dialog().locator('.composer-model-option').count(), 0);
      await h.noWrites(); assert.equal(h.count('get_available_models'), 0);
      await h.title('deepseek', INITIAL); assert.equal(await h.field().inputValue(), DRAFT);
    });
    await test('14 thinking setter and authoritative readback lock model leaves, send and Enter', null, async h => {
      const thinking = h.thinking();
      await h.openThinking();
      const { current, target } = await h.thinkingChoice();
      const setter = h.gate('thinking setter acknowledgment'), state = h.gate('thinking authoritative readback');
      h.policy.thinkingSetGate = setter; h.policy.thinkingStateGate = state;
      await h.thinkingOption(target).click();
      await until(() => setter.entered, 'thinking fixture mutated, acknowledgment pending');
      assert.equal(h.count('set_thinking_level'), 1);
      assert.equal(h.commands.find(item => item.body.type === 'set_thinking_level').body.level, target);
      async function assertLocked() {
        // Shared Modal owns the backdrop: explicitly close thinking first,
        // then prove Provider remains browsable throughout the pending write.
        if (await h.thinkingDialog().isVisible()) {
          await h.thinkingDialog().locator('p[role=status]').waitFor();
          await h.thinkingValue(current);
          await h.closeThinking();
        }
        await h.open(); await h.expand('alpha'); await h.expand('beta');
        await h.dialog().getByText('正在切换思考等级，暂不可切换模型。', { exact: true }).waitFor();
        for (const [provider, id] of [['alpha', 'alpha/a1'], ['beta', 'beta/b1']]) {
          assert(await h.leaf(provider, id).isDisabled());
          await h.leaf(provider, id).evaluate(button => button.click());
        }
        assert.equal(h.count('set_model'), 0);
        await h.close(); assert(await h.send().isDisabled()); assert(await thinking.isDisabled());
        await h.field().evaluate(element => element.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', code: 'Enter', bubbles: true, cancelable: true })));
        if (!await h.field().isDisabled()) await h.field().press('Enter');
        else { await h.page.evaluate(() => document.activeElement?.blur()); await h.page.keyboard.press('Enter'); }
        await h.send().evaluate(button => button.click()); await delay(100);
        assert.equal(h.count('prompt'), 0); assert.equal(h.count('queue_message'), 0);
        assert.equal(await h.field().inputValue(), DRAFT); await h.title('deepseek', INITIAL);
      }
      await assertLocked(); setter.release();
      await until(() => state.entered, 'thinking readback pending');
      assert.equal(state.snapshot.data.thinkingLevel, target, 'setter truly changed fixture runtime');
      await assertLocked(); state.release();
      await until(async () => !await h.send().isDisabled() && !await thinking.isDisabled() && await thinking.getAttribute('data-thinking-level') === target, 'thinking readback unlocks composer');
      await h.open(); await h.choose('beta', 'beta/b1');
      await h.dialog().waitFor({ state: 'hidden' }); await h.title('beta', 'beta/b1');
      assert.equal(h.count('set_model'), 1); assert.equal(h.count('set_thinking_level'), 1);
      assert.equal(await h.field().inputValue(), DRAFT);
    });
    await test('15 late explicit refresh snapshot cannot overwrite a newer verified model commit', null, async h => {
      // Install only after readiness: this is a pre-set refresh snapshot, not
      // the generic afterSet readback route used by mutation tests.
      await h.open(); await h.expand('beta');
      const held = h.gate('pre-set explicit refresh state'); h.policy.refreshGate = held;
      const modelsBefore = h.count('get_available_models'); await h.refresh();
      await until(() => held.entered, 'refresh captured old runtime before model setter');
      assert.equal(held.snapshot.data.modelId, INITIAL); assert.equal(h.count('set_model'), 0);
      await until(() => h.count('get_available_models') > modelsBefore, 'refresh catalog request runs independently');
      await h.expand('beta');
      await until(async () => !await h.leaf('beta', 'beta/b1').isDisabled(), 'catalog usable while refresh state is held');
      assert.equal(held.released, false);
      await h.choose('beta', 'beta/b1'); await h.dialog().waitFor({ state: 'hidden' }); await h.title('beta', 'beta/b1');
      // Keep a newly opened dialog across completion as an additional check
      // that a stale refresh cannot masquerade as another successful commit.
      await h.open(); await h.expand('beta'); held.release();
      await until(() => held.delivered, 'late refresh response delivered'); await delay(250);
      await h.title('beta', 'beta/b1'); assert(await h.dialog().isVisible());
      assert.equal(await h.leaf('beta', 'beta/b1').getAttribute('aria-pressed'), 'true');
      assert.equal(h.count('set_model'), 1); assert.equal(await h.field().inputValue(), DRAFT);
      await h.close(); assert.equal(await h.send().isDisabled(), false);
    });
    await test('16 same-address reconnect ignores old-epoch readback without closing new dialog or leaking busy', h => {
      h.policy.stateGate = h.gate('old connection dark model readback'); h.policy.stateGateOnce = true;
    }, async h => {
      const held = h.policy.stateGate;
      await h.open(); await h.choose(); await until(() => held.entered, 'old connection captured alpha snapshot');
      assert.equal(held.snapshot.data.modelId, 'alpha/a1');
      assert.equal(h.policy.stateGate, null, 'only the old readback may be gated');
      await h.close();
      // Bypass page routing to change authoritative runtime, not the captured
      // alpha response. This is the isolated fixture only, never a provider.
      const response = await fetch(`${h.fixture.url.replace(/\/$/, '')}/api/v1/sessions/dark/commands`, {
        method: 'POST', headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ type: 'set_model', modelId: INITIAL }),
      });
      assert(response.ok); const reset = await response.json();
      assert.equal(reset.success, true); assert.equal(reset.data.modelId, INITIAL);
      const catalogsBefore = h.count('get_available_models');
      await h.page.getByRole('button', { name: '打开设置', exact: true }).click();
      await h.page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '执行节点', exact: true }).click();
      // Reuse the existing connection form even while the node is online;
      // clicking Connect again deliberately creates a new connection epoch.
      if (!await h.page.getByLabel('节点地址', { exact: true }).isVisible()) {
        await h.page.getByRole('button', { name: '连接其他节点', exact: true }).click();
      }
      await h.page.getByLabel('节点地址', { exact: true }).fill(h.fixture.url);
      await h.page.getByRole('button', { name: '连接节点', exact: true }).click();
      await h.page.locator('.server-details [role=status]').filter({ hasText: '在线' }).waitFor();
      await h.page.getByRole('button', { name: '返回会话', exact: true }).click();
      // Reconnection may retain dark or return to the workspace landing view.
      if (!await h.page.locator('[data-session-id="dark"].selected .session-item').isVisible()) {
        if (!await h.session('dark').isVisible()) await h.page.locator('.project-row').filter({ hasText: 'rpi-work' }).click();
        await h.session('dark').click();
      }
      await h.ready();
      // Session restoration reads the authoritative snapshot endpoint; it need
      // not issue get_state. A fresh catalog plus unlocked scope proves reconnect.
      await until(() => h.count('get_available_models') > catalogsBefore, 'new connection rediscovers dark catalog without old gate');
      await h.field().fill(DRAFT);
      await h.title('deepseek', INITIAL); await h.open(); await h.expand('beta');
      await until(async () => !await h.leaf('beta', 'beta/b1').isDisabled(), 'old-epoch busy does not leak');
      held.release(); await until(() => held.delivered, 'old alpha readback delivered after reconnect'); await delay(250);
      await h.title('deepseek', INITIAL);
      assert(await h.dialog().isVisible(), 'old completion must not close the new connection dialog');
      assert.equal(await h.leaf('beta', 'beta/b1').isDisabled(), false);
      await h.choose('beta', 'beta/b1'); await h.dialog().waitFor({ state: 'hidden' }); await h.title('beta', 'beta/b1');
      await until(async () => !await h.send().isDisabled(), 'new-epoch mutation and readback unlock send');
      assert.equal(h.count('set_model'), 2, 'direct fixture reset is intentionally outside page command log');
    });
    await test('16b stale Provider refresh cannot overwrite a newer verified thinking commit', null, async h => {
      await h.open();
      const held = h.gate('refresh captured before thinking commit'); h.policy.refreshGate = held;
      await h.refresh(); await until(() => held.entered, 'pre-thinking refresh captured');
      await h.close(); await h.openThinking();
      const { target } = await h.thinkingChoice();
      await h.thinkingOption(target).click(); await h.thinkingDialog().waitFor({ state: 'hidden' });
      await h.thinkingValue(target); await h.openThinking();
      held.release(); await until(() => held.delivered, 'old Provider refresh delivered');
      await delay(200); await h.thinkingValue(target);
      assert(await h.thinkingDialog().isVisible(), 'stale refresh must not close the new thinking dialog');
      assert.equal(await h.thinkingOption(target).getAttribute('aria-pressed'), 'true');
      assert.equal(h.count('set_thinking_level'), 1); assert.equal(h.count('set_model'), 0);
      assert.equal(await h.field().inputValue(), DRAFT);
    });
    await test('17 thinking open, current choice, close and Escape retain focus/draft with zero writes', null, async h => {
      await h.openThinking();
      const { current, available } = await h.thinkingChoice();
      assert.deepEqual(available, ['off', 'low', 'medium', 'high']);
      for (const level of available) {
        const labels = { off: '关闭', low: '低', medium: '中', high: '高' };
        assert.equal((await h.thinkingOption(level).innerText()).trim(), labels[level] ?? level, 'option localizes only its display, preserving the raw protocol identity');
        assert.equal(await h.thinkingOption(level).getAttribute('aria-pressed'), String(level === current));
      }
      await h.thinkingOption(current).click(); await h.thinkingDialog().waitFor({ state: 'hidden' });
      await until(() => h.thinking().evaluate(element => element === document.activeElement), 'current choice restores trigger focus');
      await h.openThinking(); await h.closeThinking(true);
      await until(() => h.thinking().evaluate(element => element === document.activeElement), 'Escape restores trigger focus');
      await h.openThinking(); await h.closeThinking();
      await until(() => h.thinking().evaluate(element => element === document.activeElement), 'trigger toggles closed and restores focus');
      await h.thinkingValue(current); await h.noWrites(); assert.equal(await h.field().inputValue(), DRAFT);
    });
    await test('18 thinking setter rejection keeps current selection and dialog; retry double-click commits only once', h => {
      h.policy.thinkingSetter = 'reject';
    }, async h => {
      await h.openThinking(); const { current, target } = await h.thinkingChoice();
      await h.thinkingOption(target).click(); await h.thinkingDialog().locator('p[role=alert]').waitFor();
      await h.thinkingValue(current); assert(await h.thinkingDialog().isVisible());
      assert.equal(await h.thinkingOption(current).getAttribute('aria-pressed'), 'true');
      assert.equal(h.count('set_thinking_level'), 1); assert.equal(await h.field().inputValue(), DRAFT);
      await until(async () => !await h.thinkingOption(target).isDisabled(), 'rejected thinking setter clears busy');
      h.policy.thinkingSetter = 'normal';
      const setter = h.gate('thinking retry setter'), state = h.gate('thinking retry readback');
      h.policy.thinkingSetGate = setter; h.policy.thinkingStateGate = state;
      await h.thinkingOption(target).evaluate(button => { button.click(); button.click(); });
      await until(() => setter.entered, 'one retry setter entered');
      await h.thinkingDialog().locator('p[role=status]').waitFor();
      assert.equal(h.count('set_thinking_level'), 2, 'double click issues only one retry');
      assert(await h.thinkingOption(target).isDisabled()); assert(await h.thinking().isDisabled()); assert(await h.send().isDisabled());
      await h.thinkingValue(current); setter.release();
      await until(() => state.entered, 'retry waits for authoritative readback');
      assert.equal(state.snapshot.data.thinkingLevel, target); await h.thinkingValue(current);
      assert(await h.thinkingDialog().isVisible()); state.release();
      await h.thinkingDialog().waitFor({ state: 'hidden' }); await h.thinkingValue(target);
      await until(async () => !await h.send().isDisabled(), 'successful thinking retry unlocks send');
      assert.deepEqual(h.commands.filter(item => item.body.type === 'set_thinking_level').map(item => item.body.level), [target, target]);
      assert.equal(h.count('set_model'), 0); assert.equal(await h.field().inputValue(), DRAFT);
    });
    for (const mode of ['old', 'fail']) {
      await test(`19-${mode} thinking ${mode === 'old' ? 'mismatched' : 'failed'} authoritative readback retains current value; explicit refresh recovers`, null, async h => {
        await h.openThinking(); const { current, target } = await h.thinkingChoice();
        h.policy.thinkingOld = current; h.policy.thinkingReadback = mode;
        const statesBefore = h.count('get_state');
        await h.thinkingOption(target).click(); await h.thinkingDialog().locator('p[role=alert]').waitFor();
        assert(h.count('get_state') > statesBefore, 'setter acknowledgment alone is not authoritative');
        assert.equal(h.count('set_thinking_level'), 1); await h.thinkingValue(current);
        assert.equal((await h.backendState()).thinkingLevel, target, 'thinking setter actually mutated the fixture');
        assert(await h.thinkingDialog().isVisible()); assert.equal(await h.thinkingOption(current).getAttribute('aria-pressed'), 'true');
        await h.unconfirmed(h.thinkingDialog());
        const reads = h.count('get_state');
        h.policy.thinkingReadback = 'normal'; await h.thinkingRefresh().click();
        await h.thinkingValue(target);
        await until(async () => await h.thinkingDialog().locator('.composer-config-unconfirmed').count() === 0 && !await h.thinkingOption(target).isDisabled(), 'explicit thinking refresh clears unknown and unlocks options');
        assert(await h.thinkingDialog().isVisible(), 'refresh recovery must not auto-close the dialog');
        assert.equal(await h.thinkingOption(target).getAttribute('aria-pressed'), 'true');
        assert.equal(h.count('get_state'), reads + 1, 'only the explicit refresh reads recovery state');
        assert.equal(h.count('set_thinking_level'), 1, 'recovery never retries the thinking setter'); assert.equal(h.count('set_model'), 0);
        await h.closeThinking(); await until(async () => !await h.send().isDisabled(), 'readback recovery unlocks send');
        assert.equal(await h.field().inputValue(), DRAFT);
      });
    }
    await test('20 late thinking readback cannot overwrite or close a new-session thinking dialog', null, async h => {
      await h.openThinking(); const { target } = await h.thinkingChoice();
      const held = h.gate('old dark thinking readback'); h.policy.thinkingStateGate = held;
      await h.thinkingOption(target).click(); await until(() => held.entered, 'old thinking snapshot held');
      assert.equal(held.snapshot.data.thinkingLevel, target);
      await h.closeThinking(); await h.session('architecture').click(); await h.ready();
      await h.field().fill('思考等级新会话独立草稿'); await h.openThinking();
      const next = await h.thinkingChoice();
      assert.equal(await h.thinkingOption(next.target).isDisabled(), false, 'old thinking busy cannot bleed into new scope');
      held.release(); await until(() => held.delivered, 'old thinking response delivered'); await delay(250);
      assert(await h.thinkingDialog().isVisible(), 'old completion cannot close new-scope dialog');
      await h.thinkingValue(next.current);
      assert.equal(await h.thinkingOption(next.current).getAttribute('aria-pressed'), 'true');
      assert.equal(await h.field().inputValue(), '思考等级新会话独立草稿');
      await h.thinkingOption(next.target).click(); await h.thinkingDialog().waitFor({ state: 'hidden' }); await h.thinkingValue(next.target);
      await h.session('dark').click(); await h.ready(); await h.thinkingValue(target);
      assert.equal(await h.field().inputValue(), DRAFT); assert.equal(h.count('set_thinking_level'), 2); assert.equal(h.count('set_model'), 0);
    });
    for (const mode of ['unsupported', 'empty']) {
      await test(`21-${mode} thinking trigger stays disabled without setter capability or reported choices`, h => {
        if (mode === 'unsupported') h.policy.thinkingUnsupported = true;
        else h.policy.thinkingEmpty = true;
      }, async h => {
        await until(() => h.count('get_available_models') > 0 && (mode === 'unsupported' || h.policy.thinkingDiscoveryDone), 'discovery response delivered');
        await h.open(); await h.expand('alpha'); await h.close();
        assert(await h.thinking().isDisabled()); await h.thinking().evaluate(button => button.click());
        assert.equal(await h.thinkingDialog().count(), 0);
        await h.noWrites(); assert.equal(await h.field().inputValue(), DRAFT);
      });
    }
    // Deadline tests install the clock only after boot and successful initial
    // discovery. Every held operation starts AFTER installation; each deadline
    // is advanced only once the Node route has captured the actual response.
    await test('22 model discovery deadline enables manual retry and ignores its late catalog', null, async h => {
      await h.open(); await h.expand('alpha');
      await until(async () => !await h.leaf('alpha', 'alpha/a1').isDisabled(), 'initial model discovery settled');
      await h.close(); await h.openThinking(); await h.thinkingChoice(); await h.closeThinking();
      await h.page.clock.install();
      const held = h.gate('timed-out catalog response');
      h.fixture.controls.models = [...catalog(), { id: 'late/obsolete', name: 'Late obsolete', provider: 'late' }];
      h.policy.responseGates.get_available_models = held;
      await h.open(); const statesBefore = h.count('get_state'), thinkingBefore = h.count('get_available_thinking_levels');
      await h.refresh(); await until(() => held.entered, 'old catalog captured');
      assert(held.snapshot.data.models.some(model => model.id === 'late/obsolete'));
      await h.dialog().getByRole('status').filter({ hasText: '正在加载模型列表' }).waitFor();
      assert(await h.dialog().getByRole('button', { name: '刷新模型列表', exact: true }).isDisabled());
      await h.deadline(held);
      await h.dialog().getByRole('alert').filter({ hasText: /超时/ }).waitFor();
      assert.equal(await h.dialog().getByRole('status').filter({ hasText: '正在加载模型列表' }).count(), 0);
      assert.equal(await h.dialog().getByRole('button', { name: '刷新模型列表', exact: true }).isDisabled(), false);
      const catalogs = h.count('get_available_models'); await delay(100);
      assert.equal(h.count('get_available_models'), catalogs, 'deadline never automatically retries discovery');
      h.fixture.controls.models = catalog(); await h.refresh(); await h.expand('beta');
      await until(async () => !await h.leaf('beta', 'beta/b1').isDisabled(), 'explicit model retry recovered');
      assert.equal(h.count('get_available_models'), catalogs + 1);
      assert(h.count('get_state') >= statesBefore + 2 && h.count('get_available_thinking_levels') >= thinkingBefore + 2, 'each refresh reads state and both catalogs');
      await h.close(); await h.open(); held.release();
      await until(() => held.delivered, 'late catalog delivered'); await delay(150);
      assert(await h.dialog().isVisible(), 'late discovery cannot close a reopened dialog');
      assert.equal(await h.group('late').count(), 0, 'obsolete timed-out catalog is ignored');
      await h.expand('beta'); assert.equal(await h.leaf('beta', 'beta/b1').isDisabled(), false);
      await h.title('deepseek', INITIAL); assert.equal((await h.backendState()).modelId, INITIAL);
      assert.equal(await h.field().inputValue(), DRAFT); await h.noWrites();
    });
    await test('23 thinking discovery rejection and deadline remain inspectable until explicit retry', null, async h => {
      await h.openThinking(); const { current } = await h.thinkingChoice(); await h.closeThinking();
      await h.open(); await h.expand('alpha'); await h.close();
      await h.page.clock.install();
      h.policy.thinkingDiscovery = 'fail'; await h.openThinking();
      const statesBefore = h.count('get_state'), modelsBefore = h.count('get_available_models');
      await h.thinkingRefresh().click();
      await h.thinkingDialog().getByRole('alert').filter({ hasText: 'fixture thinking discovery unavailable' }).waitFor();
      assert.equal(await h.thinking().isDisabled(), false, 'discovery failure does not hide retry UI');
      assert.equal(await h.thinkingRefresh().isDisabled(), false);
      h.policy.thinkingDiscovery = 'normal';
      const held = h.gate('thinking discovery response'); h.policy.responseGates.get_available_thinking_levels = held;
      await h.thinkingRefresh().click(); await until(() => held.entered, 'thinking discovery captured');
      await h.thinkingDialog().getByRole('status').filter({ hasText: '正在加载思考等级' }).waitFor();
      assert.equal(await h.thinking().isDisabled(), false, 'loading is inspectable');
      assert(await h.thinkingRefresh().isDisabled());
      await h.deadline(held);
      await h.thinkingDialog().getByRole('alert').filter({ hasText: /超时/ }).waitFor();
      assert.equal(await h.thinkingDialog().getByRole('status').filter({ hasText: '正在加载思考等级' }).count(), 0);
      assert.equal(await h.thinkingRefresh().isDisabled(), false);
      const discoveries = h.count('get_available_thinking_levels'); await delay(100);
      assert.equal(h.count('get_available_thinking_levels'), discoveries, 'no automatic discovery retry');
      await h.thinkingRefresh().click();
      await until(async () => !await h.thinkingOption('low').isDisabled(), 'explicit thinking discovery retry recovered');
      assert.equal(await h.thinkingDialog().getByRole('alert').count(), 0);
      assert.equal(h.count('get_available_thinking_levels'), discoveries + 1);
      assert(h.count('get_state') >= statesBefore + 3 && h.count('get_available_models') >= modelsBefore + 3, 'thinking refresh reads true state and both catalogs');
      await h.closeThinking(); await h.openThinking(); held.release();
      await until(() => held.delivered, 'late thinking discovery delivered'); await delay(150);
      assert(await h.thinkingDialog().isVisible()); await h.thinkingValue(current);
      assert.equal((await h.backendState()).thinkingLevel, current);
      assert.equal(await h.field().inputValue(), DRAFT); await h.noWrites();
    });
    for (const field of ['model', 'thinking']) for (const phase of ['ack', 'readback']) {
      await test(`24-${field}-${phase} real mutation deadline locks unknown config; explicit recovery deadline and retry`, null, async h => {
        await h.openThinking(); const { current, target } = await h.thinkingChoice(); await h.closeThinking();
        await h.open(); await h.expand('alpha');
        await until(async () => !await h.leaf('alpha', 'alpha/a1').isDisabled(), 'initial model catalog settled');
        await h.close(); await h.page.clock.install();
        const setter = field === 'model' ? 'set_model' : 'set_thinking_level';
        const held = h.gate(`${field} ${phase} response`);
        h.policy.responseGates[phase === 'ack' ? setter : 'get_state'] = held;
        const dialog = () => field === 'model' ? h.dialog() : h.thinkingDialog();
        const open = () => field === 'model' ? h.open() : h.openThinking();
        const close = () => field === 'model' ? h.close() : h.closeThinking();
        const refresh = () => field === 'model' ? h.refresh() : h.thinkingRefresh().click();
        const refreshButton = () => field === 'model' ? h.dialog().getByRole('button', { name: '刷新模型列表', exact: true }) : h.thinkingRefresh();
        const notice = () => dialog().locator('.composer-config-unconfirmed');
        async function oldDisplay() { await h.title('deepseek', INITIAL); await h.thinkingValue(current); assert.equal(await h.field().inputValue(), DRAFT); }
        async function actualRuntime() {
          const state = await h.backendState();
          assert.equal(state.modelId, field === 'model' ? 'alpha/a1' : INITIAL);
          assert.equal(state.thinkingLevel, field === 'thinking' ? target : current);
          return state;
        }
        function exactlyOneSetter() {
          assert.equal(h.count(setter), 1); assert.equal(h.count(field === 'model' ? 'set_thinking_level' : 'set_model'), 0);
          assert.equal(h.fixture.requests.filter(request => (request.body.type === setter || (request.path.endsWith('/configuration') && request.body.configId === (setter === 'set_model' ? 'model' : 'thinking')))).length, 1, 'exactly one actual fixture mutation');
        }
        await open();
        if (field === 'model') await h.choose(); else await h.thinkingOption(target).click();
        await until(() => held.entered, 'actual fixture mutation completed before response hold');
        if (phase === 'readback') assert.equal(held.snapshot.data[field === 'model' ? 'modelId' : 'thinkingLevel'], field === 'model' ? 'alpha/a1' : target);
        else assert.equal(held.snapshot.data.accepted, true, 'typed ACK intentionally omits runtime state; only true get_state may commit');
        await oldDisplay(); await actualRuntime();
        await h.deadline(held);
        await dialog().getByRole('alert').filter({ hasText: /超时/ }).first().waitFor(); await notice().waitFor();
        await until(async () => !await refreshButton().isDisabled(), 'timeout clears mutation busy, refresh remains enabled');
        await oldDisplay(); assert(await dialog().isVisible(), 'timeout is not a commit'); exactlyOneSetter();
        for (const width of [320, 1280]) {
          await h.page.setViewportSize({ width, height: 900 });
          // This case installed the browser clock: deliver resize/layout frames
          // before measuring the anchored menu, without advancing the RPC deadline.
          await until(async () => {
            await h.page.clock.runFor(50);
            return dialog().evaluate(el => {
              const rect = el.getBoundingClientRect();
              return rect.left >= 0 && rect.right <= innerWidth + 1;
            });
          }, 'resize and visualViewport delivery re-anchor the open menu');
          const layout = await dialog().evaluate(el => {
            const r = el.getBoundingClientRect(), body = el.querySelector('.composer-model-body');
            return { left: r.left, right: r.right, width: innerWidth, overflow: body.scrollWidth - body.clientWidth, pageOverflow: document.documentElement.scrollWidth - innerWidth,
              style: el.getAttribute('style'), registry: window.__rpiComposerPickers?.size,
              visual: [visualViewport?.width, visualViewport?.height], popover: el.matches(':popover-open') };
          });
          assert(layout.left >= 0 && layout.right <= layout.width + 1 && layout.overflow <= 1 && layout.pageOverflow <= 1, `timeout recovery must fit ${width}px: ${JSON.stringify(layout)}`);
          await h.page.screenshot({ path: `artifacts/composer-config-timeout-${field}-${phase}-${width}.png`, animations: 'disabled' });
        }
        // Close/reopen BEFORE late completion. Its successful ACK/readback may
        // neither clear unknown nor invent commit_revision and dismiss this UI.
        const readsAfterTimeout = h.count('get_state');
        await close(); await open(); held.release(); await until(() => held.delivered, 'late mutation response delivered'); await delay(150);
        assert(await dialog().isVisible(), 'late response cannot close reopened picker'); await notice().waitFor(); await oldDisplay();
        assert.equal(h.count('get_state'), readsAfterTimeout, 'late ACK must not initiate post-timeout readback');
        await close();
        assert(await h.send().isDisabled());
        await h.field().evaluate(element => element.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', code: 'Enter', bubbles: true, cancelable: true })));
        if (!await h.field().isDisabled()) await h.field().press('Enter');
        else { await h.page.evaluate(() => document.activeElement?.blur()); await h.page.keyboard.press('Enter'); }
        await h.send().evaluate(button => button.click());
        await h.open(); await h.expand('alpha'); await h.dialog().locator('.composer-config-unconfirmed').waitFor();
        for (const id of ['alpha/a1', 'alpha/a2']) {
          assert(await h.leaf('alpha', id).isDisabled()); await h.leaf('alpha', id).evaluate(button => button.click());
        }
        await h.close(); assert.equal(await h.thinking().isDisabled(), false, 'unknown config still permits inspecting thinking recovery');
        await h.openThinking(); await h.thinkingDialog().locator('.composer-config-unconfirmed').waitFor();
        for (const level of ['off', 'low']) {
          assert(await h.thinkingOption(level).isDisabled()); await h.thinkingOption(level).evaluate(button => button.click());
        }
        await h.closeThinking(); await delay(100); exactlyOneSetter();
        assert.equal(h.count('prompt'), 0); assert.equal(h.count('queue_message'), 0); assert.equal(h.count('set_api_key'), 0);
        assert.equal(h.count('get_state'), readsAfterTimeout, 'unknown state never triggers automatic recovery');
        await oldDisplay(); await actualRuntime(); await open();
        if (field === 'model' && phase === 'ack') {
          const empty = h.gate('empty recovery state'); empty.replyData = {}; h.policy.responseGates.get_state = empty;
          await refresh(); await until(() => empty.entered, 'empty recovery response captured'); empty.release();
          await until(() => empty.delivered, 'empty recovery response delivered');
          await dialog().getByRole('alert').filter({ hasText: '节点仍未报告待核对的配置' }).waitFor();
          await notice().waitFor(); await oldDisplay(); assert(await h.send().isDisabled()); exactlyOneSetter();
          await until(async () => !await refreshButton().isDisabled(), 'empty recovery permits explicit retry');
        }
        const recovery = h.gate(`${field} recovery get_state response`); h.policy.responseGates.get_state = recovery;
        const modelsBefore = h.count('get_available_models'), thinkingBefore = h.count('get_available_thinking_levels');
        await refresh(); await h.deadline(recovery);
        await dialog().getByRole('alert').filter({ hasText: /回读当前配置失败.*超时/ }).first().waitFor();
        await notice().waitFor(); await oldDisplay(); assert(await h.send().isDisabled()); exactlyOneSetter();
        if (field === 'model') { await h.expand('alpha'); assert(await h.leaf('alpha', 'alpha/a2').isDisabled()); }
        else assert(await h.thinkingOption(target).isDisabled());
        await until(async () => !await refreshButton().isDisabled(), 'recovery timeout permits a second explicit refresh');
        const recoveryReads = h.count('get_state'); await delay(100);
        assert.equal(h.count('get_state'), recoveryReads, 'recovery timeout also never retries itself');
        await refresh();
        if (field === 'model') await h.title('alpha', 'alpha/a1'); else await h.thinkingValue(target);
        await until(async () => !await h.send().isDisabled() && await notice().count() === 0, 'true state clears scoped unknown and send lock');
        assert(await dialog().isVisible(), 'recovery must not invent a commit revision or auto-close');
        assert(h.count('get_available_models') >= modelsBefore + 2 && h.count('get_available_thinking_levels') >= thinkingBefore + 2, 'recovery refresh always reloads both catalogs');
        assert.equal(h.count('get_state'), recoveryReads + 1, 'only the second explicit refresh performs recovery readback');
        // Deliver the first recovery response only across a freshly reopened
        // dialog, after true-state recovery has already cleared the lock.
        await close(); await open(); recovery.release(); await until(() => recovery.delivered, 'late recovery response delivered'); await delay(150);
        assert(await dialog().isVisible(), 'late recovery cannot close reopened dialog'); assert.equal(await notice().count(), 0);
        await h.title(field === 'model' ? 'alpha' : 'deepseek', field === 'model' ? 'alpha/a1' : INITIAL);
        await h.thinkingValue(field === 'thinking' ? target : current);
        if (field === 'model') {
          await h.expand('alpha'); assert.equal(await h.leaf('alpha', 'alpha/a1').getAttribute('aria-pressed'), 'true');
          assert.equal(await h.leaf('alpha', 'alpha/a2').isDisabled(), false);
        } else {
          assert.equal(await h.thinkingOption(target).getAttribute('aria-pressed'), 'true');
          assert.equal(await h.thinkingOption(current).isDisabled(), false);
        }
        await actualRuntime(); exactlyOneSetter(); assert.equal(await h.field().inputValue(), DRAFT);
        await close(); assert.equal(await h.send().isDisabled(), false);
      });
    }
    for (const field of ['model', 'thinking']) for (const mode of ['missing', 'blank', 'no-state', 'requires-restart']) {
      await test(`25-${field}-${mode} accepted setter stays unconfirmed until explicit nonblank affected-field refresh`, null, async h => {
        await h.openThinking(); const { current, target } = await h.thinkingChoice(); await h.closeThinking();
        const isModel = field === 'model', key = isModel ? 'modelId' : 'thinkingLevel';
        const setter = isModel ? 'set_model' : 'set_thinking_level';
        const dialog = () => isModel ? h.dialog() : h.thinkingDialog();
        const refresh = () => isModel ? h.refresh() : h.thinkingRefresh().click();
        const refreshButton = () => isModel ? h.dialog().getByRole('button', { name: '刷新模型列表', exact: true }) : h.thinkingRefresh();
        const oldDisplay = async () => { await h.title('deepseek', INITIAL); await h.thinkingValue(current); };
        if (isModel) { await h.open(); await h.expand('alpha'); }
        else await h.openThinking();
        // Only the page route changes response policy. The real isolated
        // fixture always executes exactly one setter, including requiresRestart.
        if (mode === 'requires-restart') h.policy[isModel ? 'setter' : 'thinkingSetter'] = mode;
        else h.policy[isModel ? 'readback' : 'thinkingReadback'] = mode;
        const readsBefore = h.count('get_state');
        if (isModel) await h.choose(); else await h.thinkingOption(target).click();
        await dialog().getByRole('alert').first().waitFor();
        await oldDisplay();
        assert.equal((await h.backendState())[key], isModel ? 'alpha/a1' : target, 'accepted setter actually changed runtime');
        assert.equal(h.count('get_state'), readsBefore + (mode === 'requires-restart' ? 0 : 1), 'no automatic retry or readback after requiresRestart');
        await h.unconfirmed(dialog());
        assert.equal(h.count(setter), 1);
        if (mode === 'missing') {
          // A usable unrelated field must not unlock the affected field. Even
          // an explicit refresh with whitespace-only affected state stays locked.
          const blank = h.gate(`${field} blank recovery state`);
          blank.replyData = { ...await h.backendState(), [key]: '   ' };
          h.policy.responseGates.get_state = blank;
          await refresh(); await until(() => blank.entered, 'blank affected-field recovery captured');
          blank.release(); await until(() => blank.delivered, 'blank recovery delivered');
          await dialog().getByRole('alert').filter({ hasText: '节点仍未报告待核对的配置' }).waitFor();
          await oldDisplay(); await h.unconfirmed(dialog());
        }
        await until(async () => !await refreshButton().isDisabled(), 'unconfirmed config permits explicit recovery');
        h.policy[isModel ? 'readback' : 'thinkingReadback'] = 'normal';
        const recoveryReads = h.count('get_state'); await refresh();
        if (isModel) await h.title('alpha', 'alpha/a1'); else await h.thinkingValue(target);
        await until(async () => await dialog().locator('.composer-config-unconfirmed').count() === 0 && !await h.send().isDisabled(), 'nonblank affected field clears unknown config');
        assert(await dialog().isVisible(), 'explicit recovery never invents a commit or closes the dialog');
        assert.equal(h.count('get_state'), recoveryReads + 1, 'only explicit recovery reads state');
        assert.equal(h.count(setter), 1, 'refresh does not repeat the accepted setter');
        assert.equal(h.count(isModel ? 'set_thinking_level' : 'set_model'), 0);
        assert.equal(h.fixture.requests.filter(request => (request.body.type === setter || (request.path.endsWith('/configuration') && request.body.configId === (setter === 'set_model' ? 'model' : 'thinking')))).length, 1, 'exactly one actual fixture mutation');
        if (isModel) {
          await h.expand('alpha');
          assert.equal(await h.leaf('alpha', 'alpha/a1').getAttribute('aria-pressed'), 'true');
          assert.equal(await h.leaf('alpha', 'alpha/a2').isDisabled(), false);
          await h.close();
        } else {
          assert.equal(await h.thinkingOption(target).getAttribute('aria-pressed'), 'true');
          assert.equal(await h.thinkingOption(current).isDisabled(), false);
          await h.closeThinking();
        }
        assert.equal(await h.field().inputValue(), DRAFT);
      });
    }
    for (const status of [401, 404, 500]) {
      await test(`26-${status} failed session observation disables config without native or global fallback; manual retry recovers`, h => {
        h.policy.configurationStatus = status;
      }, async h => {
        await h.open();
        await h.dialog().getByRole('alert').filter({ hasText: 'fixture configuration observation unavailable' }).waitFor();
        assert.equal(h.count('get_available_models'), 0); assert.equal(h.count('get_available_thinking_levels'), 0);
        assert.equal(await h.dialog().locator('.composer-model-option').count(), 0);
        await h.noWrites();
        h.policy.configurationStatus = null;
        await h.refresh(); await h.expand('alpha');
        await until(async () => !await h.leaf('alpha', 'alpha/a1').isDisabled(), 'explicit observation retry restores actual session capabilities');
        await h.choose(); await h.dialog().waitFor({ state: 'hidden' }); await h.title('alpha', 'alpha/a1');
        assert.equal(h.count('set_model'), 1); assert.equal(await h.field().inputValue(), DRAFT);
      });
    }
    for (const failure of [false, true]) {
      await test(`27-${failure ? 'failure' : 'success'} observation deadline is manual-retryable; late old results never replace newer capabilities`, null, async h => {
        await h.open(); await h.expand('alpha');
        await until(async () => !await h.leaf('alpha', 'alpha/a1').isDisabled(), 'initial observation ready');
        await h.page.clock.install();
        const held = h.gate('old configuration observation');
        if (failure) held.failureStatus = 500;
        h.fixture.controls.configurationCommands = [];
        h.policy.configurationGate = held;
        await h.refresh(); await until(() => held.entered, 'configuration GET captured');
        const before = h.count('get_available_models');
        await h.deadline(held);
        await h.dialog().getByRole('alert').filter({ hasText: '读取会话配置失败' }).waitFor();
        assert.equal(h.count('get_available_models'), before, 'no discovery before a successful observation');
        h.fixture.controls.configurationCommands = undefined;
        await h.refresh(); await h.expand('alpha');
        await until(async () => !await h.leaf('alpha', 'alpha/a1').isDisabled(), 'fresh observation recovers');
        held.release(); await until(() => held.delivered, 'old observation response delivered'); await delay(150);
        assert.equal(await h.dialog().getByRole('alert').count(), 0);
        assert.equal(await h.leaf('alpha', 'alpha/a1').isDisabled(), false);
        await h.title('deepseek', INITIAL); await h.noWrites();
        assert.equal(await h.field().inputValue(), DRAFT);
      });
    }
    await test('28 late previous-session observation never changes current-session catalog or error', null, async h => {
      await h.open(); await h.expand('alpha');
      await until(async () => !await h.leaf('alpha', 'alpha/a1').isDisabled(), 'first catalog ready');
      const held = h.gate('previous-session configuration'); held.failureStatus = 500;
      h.policy.configurationGate = held; await h.refresh(); await until(() => held.entered, 'old session observation pending');
      await h.close(); await h.session('mobile').click(); await h.ready();
      await h.open(); await h.expand('beta');
      await until(async () => !await h.leaf('beta', 'beta/b1').isDisabled(), 'new session actual catalog ready');
      held.release(); await until(() => held.delivered, 'old session observation failure delivered'); await delay(150);
      assert.equal(await h.dialog().getByRole('alert').count(), 0);
      assert.equal(await h.leaf('beta', 'beta/b1').isDisabled(), false);
      await h.noWrites();
    });
    await test('29 thinking configuration failure stays inspectable and refreshable without enabling mutation', null, async h => {
      await h.openThinking();
      const { target } = await h.thinkingChoice();
      h.policy.configurationStatus = 404;
      await h.thinkingRefresh().click();
      await h.thinkingDialog().getByRole('alert').filter({ hasText: 'fixture configuration observation unavailable' }).waitFor();
      assert.equal(await h.thinkingDialog().locator('.composer-thinking-option').count(), 0);
      assert.equal(await h.thinkingRefresh().isDisabled(), false, 'capability uncertainty blocks writes, not manual recovery');
      await h.closeThinking(); await h.openThinking();
      await h.thinkingDialog().getByRole('alert').filter({ hasText: 'fixture configuration observation unavailable' }).waitFor();
      await h.noWrites();
      h.policy.configurationStatus = null;
      await h.thinkingRefresh().click();
      await until(async () => !await h.thinkingOption(target).isDisabled(), 'thinking observation recovered explicitly');
      await h.thinkingOption(target).click(); await h.thinkingDialog().waitFor({ state: 'hidden' });
      await h.thinkingValue(target); assert.equal(h.count('set_thinking_level'), 1);
      assert.equal(await h.field().inputValue(), DRAFT);
    });
    await test('30 catalog affordances follow the session observation, not global protocol capability', h => {
      h.policy.globalCatalogUnsupported = true;
    }, async h => {
      await h.open(); await h.choose(); await h.dialog().waitFor({ state: 'hidden' });
      await h.title('alpha', 'alpha/a1');
      await h.openThinking(); const { target } = await h.thinkingChoice();
      await h.thinkingOption(target).click(); await h.thinkingDialog().waitFor({ state: 'hidden' }); await h.thinkingValue(target);
      assert.equal(h.count('set_model'), 1); assert.equal(h.count('set_thinking_level'), 1);
      assert.equal(await h.field().inputValue(), DRAFT);
    });
    await test('31 late old-epoch configuration failure cannot disable a same-address reconnected session', null, async h => {
      await h.open(); await h.expand('alpha');
      await until(async () => !await h.leaf('alpha', 'alpha/a1').isDisabled(), 'first epoch observed');
      const held = h.gate('old-epoch configuration observation'); held.failureStatus = 500;
      h.policy.configurationGate = held; await h.refresh(); await until(() => held.entered, 'old epoch GET held'); await h.close();
      const catalogsBefore = h.count('get_available_models');
      await h.page.getByRole('button', { name: '打开设置', exact: true }).click();
      await h.page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '执行节点', exact: true }).click();
      if (!await h.page.getByLabel('节点地址', { exact: true }).isVisible()) await h.page.getByRole('button', { name: '连接其他节点', exact: true }).click();
      await h.page.getByLabel('节点地址', { exact: true }).fill(h.fixture.url);
      await h.page.getByRole('button', { name: '连接节点', exact: true }).click();
      await h.page.locator('.server-details [role=status]').filter({ hasText: '在线' }).waitFor();
      await h.page.getByRole('button', { name: '返回会话', exact: true }).click();
      if (!await h.page.locator('[data-session-id="dark"].selected .session-item').isVisible()) {
        if (!await h.session('dark').isVisible()) await h.page.locator('.project-row').filter({ hasText: 'rpi-work' }).click();
        await h.session('dark').click();
      }
      await h.ready(); await until(() => h.count('get_available_models') > catalogsBefore, 'new epoch observed independently');
      await h.field().fill(DRAFT); await h.open(); await h.expand('alpha');
      await until(async () => !await h.leaf('alpha', 'alpha/a1').isDisabled(), 'current catalog ready');
      held.release(); await until(() => held.delivered, 'old epoch response delivered'); await delay(150);
      assert.equal(await h.dialog().getByRole('alert').count(), 0);
      assert.equal(await h.leaf('alpha', 'alpha/a1').isDisabled(), false);
      await h.title('deepseek', INITIAL); await h.noWrites(); assert.equal(await h.field().inputValue(), DRAFT);
    });
  } catch (error) {
    errors.push({ name: 'suite setup', error: error.stack || String(error) });
    console.error(error); process.exitCode = 1;
  } finally {
    if (browser) await browser.close();
    // Also persist failures before the first test (e.g. missing browser).
    if (!results.length) fs.writeFileSync('artifacts/composer-model-picker-validation.json', JSON.stringify({ results, geometry, errors }, null, 2));
  }
  if (errors.length) process.exitCode = 1;
  else console.log(`PASS composer model picker: ${results.length} cases; zero prompts/credential writes`);
})();
