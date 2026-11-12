// Production WASM + isolated fixture. No real node, Agent, model or session writes.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');
const CONFLICT = 'node operation conflicts with current state';
(async () => {
  const fixture = await startFixture(0);
  const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
  const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
  page.setDefaultTimeout(10000);
  const results = [], errors = [], intercepted = [];
  const policy = { create: false, configuration: false, tools: false, setter: false };
  let configurationReads = 0, setters = 0;
  const pass = text => { results.push(text); console.log('PASS', text); };
  const openSettings = async () => {
    await page.locator('.sidebar-sessions [data-session-id="dark"]').first().getByRole('button', { name: /的会话操作$/ }).click();
    await page.getByRole('menuitem', { name: '详情与配置', exact: true }).click();
  };
  const dialog = () => page.getByRole('dialog', { name: '会话设置', exact: true });
  const config = () => dialog().locator('.session-config');
  const field = () => page.locator('.composer-main textarea');
  const draft = '配置读取冲突时保留原草稿 😀';
  const ui = process.env.WORK_UI_URL || 'http://127.0.0.1:14180/';
  page.on('pageerror', error => errors.push(error.message));
  // Keep the real handshake, subscriptions and reservations; reject only the
  // selected command before it reaches the fixture (no successful writes).
  await page.routeWebSocket('**/api/v1/realtime', socket => {
    assert.equal(new URL(socket.url()).host, new URL(fixture.url).host);
    const server = socket.connectToServer();
    server.onMessage(message => socket.send(message));
    socket.onMessage(message => {
      const frame = JSON.parse(String(message));
      if (policy.setter && frame.type === 'execute') {
        setters++;
        intercepted.push({transport:'websocket',type:frame.type,payload:frame.payload});
        return socket.send(JSON.stringify({type:'status',version:1,requestId:frame.requestId,operation:frame.operation,status:{state:'knownResult',outcome:{status:'failed',error:{code:'conflict',message:CONFLICT}}}}));
      }
      if (policy.tools && frame.type === 'query' && frame.payload?.type === 'get_available_tools') {
        intercepted.push({ transport: 'websocket', type: frame.type, payload: frame.payload });
        return socket.send(JSON.stringify({ type: 'error', version: 1, requestId: frame.requestId,
          error: { code: 'conflict', message: CONFLICT } }));
      }
      server.send(message);
    });
  });
  await page.route(`${fixture.url.replace(/\/$/, '')}/**`, async route => {
    const request = route.request(), path = new URL(request.url()).pathname;
    if (request.method() === 'OPTIONS') return route.continue();
    if (request.method() === 'GET' && (path.endsWith('/native/resources') || path.endsWith('/native-resource-config'))) return route.fulfill({status:501, contentType:'application/json', headers:{'access-control-allow-origin':new URL(ui).origin,'access-control-allow-credentials':'true'}, body:JSON.stringify({error:'Synthetic fixture has no native selection'})});
    const body = request.method() === 'POST' ? request.postDataJSON() : null;
    if (request.method() === 'GET' && path === '/api/v1/sessions/dark/configuration') configurationReads++;
    if (request.method() === 'POST' && path.endsWith('/configuration')) setters++;
    const blocked = policy.create && request.method() === 'POST' && (path === '/api/v1/sessions' || path.endsWith('/native/create') || path.endsWith('/resource-sessions'))
      || policy.configuration && request.method() === 'GET' && path === '/api/v1/sessions/dark/configuration'
      || policy.tools && request.method() === 'POST' && body?.type === 'get_available_tools'
      || policy.setter && request.method() === 'POST' && path.endsWith('/configuration');
    if (!blocked) return route.continue();
    intercepted.push({ method: request.method(), path, body });
    return route.fulfill({ status: 409, contentType: 'application/json',
      headers: { 'access-control-allow-origin': new URL(ui).origin, 'access-control-allow-credentials': 'true' },
      body: JSON.stringify({ error: CONFLICT }) });
  });
  const close = async () => { await page.keyboard.press('Escape'); await dialog().waitFor({ state: 'hidden' }); };
  const loaded = async () => {
    await page.waitForFunction(() => {
      const model = document.querySelector('#session-model');
      return model && !model.disabled && model.options.length > 1;
    });
    await config().getByRole('checkbox', { name: 'read', exact: true }).waitFor();
    assert.equal(await config().getByRole('alert').count(), 0);
  };
  try {
    await page.goto(ui);
    await page.getByRole('button', { name: '打开设置', exact: true }).click();
    await page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '工作协同', exact: true }).click();
    await page.getByRole('button', { name: '连接其他节点', exact: true }).click();
    await page.getByLabel('节点地址', { exact: true }).fill(fixture.url);
    await page.getByRole('button', { name: '连接节点', exact: true }).click();
    await page.locator('.server-details [role=status]').filter({ hasText: '在线' }).waitFor();
    await page.getByRole('button', { name: '返回会话', exact: true }).click();
    await page.locator('.project-row').filter({ hasText: 'rpi-work' }).click();
    await page.locator('.sidebar-sessions [data-session-id="dark"] .session-item').first().click();
    await page.waitForFunction(() => document.querySelector('.composer-main textarea')?.dataset.imeReady === 'true');
    await field().fill(draft);

    // Manufacture an unrelated, rejected operation, not a successful session mutation.
    policy.create = true;
    await page.locator('[data-project-id="rpi-work"] .project-new-session').click();
    await page.getByRole('alert').filter({ hasText: CONFLICT }).waitFor();
    assert(intercepted.some(r => r.method === 'POST' && (r.path === '/api/v1/sessions' || r.path.endsWith('/resource-sessions') || r.path.endsWith('/native/create'))));
    assert.equal(fixture.requests.filter(r => r.method === 'POST' && (r.path === '/api/v1/sessions' || r.path.endsWith('/resource-sessions') || r.path.endsWith('/native/create'))).length, 0);
    await openSettings(); await dialog().waitFor();
    assert.equal((await dialog().innerText()).includes(CONFLICT), false, 'old global conflict is not a new settings failure');
    assert(await page.getByRole('alert').filter({ hasText: CONFLICT }).count() > 0, 'original failure is retained outside settings, not erased');
    await close(); policy.create = false;
    await page.locator('.sidebar-sessions [data-session-id="dark"] .session-item').first().click();
    await page.waitForFunction(() => document.querySelector('.session-entry.selected')?.dataset.sessionId === 'dark');
    assert.equal(await field().inputValue(), draft);
    pass('opening settings does not inherit an unrelated global 409; original operation error and session draft are retained');

    policy.configuration = true;
    await openSettings(); await dialog().waitFor();
    await config().getByRole('alert').filter({ hasText: 'HTTP 409' }).waitFor();
    assert.match(await config().innerText(), /无法读取会话配置能力/);
    assert.match(await config().innerText(), /本次读取未修改会话/);
    assert.equal((await config().innerText()).includes(CONFLICT), false, 'read conflict has an actionable explanation');
    const reads = configurationReads;
    await page.waitForTimeout(200);
    assert.equal(configurationReads, reads, 'no automatic read retry');
    assert.equal(setters, 0);
    policy.configuration = false;
    await config().getByRole('button', { name: '重新读取配置', exact: true }).click();
    await loaded();
    assert.equal(configurationReads, reads + 1, 'manual retry performs one fresh configuration observation');
    assert.equal(setters, 0, 'manual read recovery never submits a setter');
    assert.equal(await field().inputValue(), draft);
    pass('real configuration observation 409 is visible and actionable; manual read-only recovery succeeds without automatic writes');

    await close(); policy.tools = true;
    await openSettings(); await dialog().waitFor();
    await config().getByRole('alert').filter({ hasText: '工具：' }).waitFor();
    assert.match(await config().innerText(), /工具：节点当前状态与配置读取请求冲突/);
    assert(intercepted.some(item => item.transport === 'websocket' && item.payload.type === 'get_available_tools'));
    assert.match(await config().innerText(), /本次读取未修改会话/);
    assert.equal(await config().locator('#session-model').isDisabled(), false, 'one failed catalog does not hide successful model discovery');
    policy.tools = false;
    await config().getByRole('button', { name: '重新读取配置', exact: true }).click();
    await loaded(); assert.equal(setters, 0);
    pass('tool catalog conflicts retain their operation context and recover without changing enabled tools');

    // Genuine mutation failures remain visible; the read-retry UI must not replay them.
    policy.setter = true;
    await config().locator('#session-model').selectOption('openai/gpt-6-astra');
    await config().getByRole('alert').filter({ hasText: '操作失败' }).waitFor();
    assert.equal(await config().getByRole('button', { name: '重新读取配置', exact: true }).count(), 0);
    await page.waitForTimeout(200); assert.equal(setters, 1);
    assert.equal(await field().inputValue(), draft);
    await close(); policy.setter = false;
    await openSettings(); await dialog().waitFor(); await loaded();
    await close();
    pass('genuine setter rejection remains visible, is never automatically replayed and does not contaminate reopened settings');

    assert.equal(fixture.requests.filter(r => r.method === 'POST' && (r.path.endsWith('/configuration') || ['prompt', 'queue_message', 'set_model', 'set_thinking_level', 'set_active_tools'].includes(r.body?.type))).length, 0);
    assert.deepEqual(errors, []);
    pass('all rejected mutations were intercepted locally; zero fixture model/configuration writes and no application errors');
  } catch (error) {
    errors.push(error.stack);
    console.error('BODY', await page.locator('body').innerText());
    await page.screenshot({ path: 'artifacts/session-settings-conflict-failure.png' }).catch(() => {});
    throw error;
  } finally {
    fs.writeFileSync('artifacts/session-settings-conflict-validation.json', JSON.stringify({ results, errors, intercepted, configurationReads, setters }, null, 2));
    await browser.close(); await fixture.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
