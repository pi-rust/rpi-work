// Explicit live integration: actual local node, real model, real Dioxus UI.
// The node password is read in-process; credentials are never written to reports.
const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const { chromium } = require(require.resolve('playwright', { paths: [process.cwd(), path.resolve('../rpi-package/packages/rpi-web/frontend')] }));
if (!process.argv.includes('--run-model')) throw new Error('Pass --run-model to invoke a real provider.');
const base = process.env.RPI_WORK_NODE_URL || 'http://127.0.0.1:3030';
const ui = process.env.WORK_UI_URL || 'http://127.0.0.1:4173/';
assert.equal(new URL(base).hostname, '127.0.0.1', 'Live verification uses a local node.');
const report = { startedAt: new Date().toISOString(), node: base, ui, passed: false, checks: [], events: {}, errors: [] };
const recordPath = 'target/live-chat-session.json';
let cookie = '';
let password = process.env.RPI_WORK_REVIEW_PASSWORD;
if (!password && process.env.RPI_WORK_NODE_ENV_FILE) {
  const environment = JSON.parse(fs.readFileSync(process.env.RPI_WORK_NODE_ENV_FILE, 'utf8').replace(/^\uFEFF/, ''));
  password = environment.RPI_HTTP_PASSWORD || environment.RPI_WEB_PASSWORD;
}
async function api(route, body) {
  const response = await fetch(`${base}/api/v1/${route}`, {
    signal: AbortSignal.timeout(30000),
    headers: { cookie, ...(body === undefined ? {} : { 'content-type': 'application/json', 'x-rpi-web': '1' }) },
    ...(body === undefined ? {} : { method: 'POST', body: JSON.stringify(body) }),
  });
  assert(response.ok, `${route}: HTTP ${response.status}`);
  if (route === 'auth' && body) cookie = response.headers.getSetCookie().map(value => value.split(';')[0]).join('; ');
  return response.json();
}
const pause = ms => new Promise(resolve => setTimeout(resolve, ms));
(async () => {
  let browser, page, streamTask, originalTools;
  const abort = new AbortController();
  const pass = name => { report.checks.push(name); console.log('PASS', name); };
  try {
    const auth = await api('auth');
    if (auth.required) { assert(password, 'Provide the node password through environment or RPI_WORK_NODE_ENV_FILE'); await api('auth', { password }); }
    const projects = (await api('projects')).projects;
    const project = projects.find(item => path.resolve(item.path).toLowerCase() === process.cwd().toLowerCase());
    assert(project, 'Register this workspace on the node first.');
    let sessionId;
    if (fs.existsSync(recordPath)) {
      const saved = JSON.parse(fs.readFileSync(recordPath, 'utf8'));
      if (saved.node === base && saved.projectId === project.id) sessionId = saved.sessionId;
    }
    if (!sessionId) {
      sessionId = (await api('sessions', { projectId: project.id })).sessionId;
      fs.writeFileSync(recordPath, JSON.stringify({ node: base, projectId: project.id, sessionId }, null, 2));
    }
    report.sessionId = sessionId;
    const prefix = `sessions/${sessionId}`;
    const title = '真实聊天 · 模型接入验证';
    await api(prefix + '/open', {});
    await api(prefix + '/rename', { title });
    const command = async (type, extra = {}) => {
      const reply = await api(prefix + '/commands', { id: `live-${Date.now()}`, type, ...extra });
      assert.equal(reply.success, true, `${type} was rejected`);
      return reply.data;
    };
    const before = await command('get_state');
    originalTools = before.activeTools;
    await command('set_active_tools', { tools: [] });
    const models = (await command('get_available_models')).models;
    const model = models.find(item => process.env.RPI_WORK_REVIEW_MODEL
      ? `${item.provider}/${item.id}` === process.env.RPI_WORK_REVIEW_MODEL
      : item.provider === before.provider && item.id === before.modelId);
    assert(model, 'Requested configured model is not available.');
    report.model = { id: model.id, provider: model.provider };

    report.initialSeq = (await api(prefix + '/snapshot')).lastSeq;
    streamTask = (async () => {
      try {
        const response = await fetch(`${base}/api/v1/${prefix}/events?after=${report.initialSeq}`, { headers: { cookie, accept: 'text/event-stream' }, signal: abort.signal });
        assert(response.ok, `SSE: HTTP ${response.status}`);
        const decoder = new TextDecoder(); let pending = '';
        for await (const chunk of response.body) {
          pending += decoder.decode(chunk, { stream: true }).replace(/\r\n/g, '\n');
          let boundary;
          while ((boundary = pending.indexOf('\n\n')) !== -1) {
            const block = pending.slice(0, boundary); pending = pending.slice(boundary + 2);
            const data = block.split('\n').filter(line => line.startsWith('data:')).map(line => line.slice(5).trimStart()).join('\n');
            if (!data) continue;
            let value; try { value = JSON.parse(data); } catch { continue; }
            const event = value.event || value;
            const type = event.type || block.match(/^event:\s*(.+)$/m)?.[1] || 'unknown';
            report.events[type] = (report.events[type] || 0) + 1;
            if (event.eventType === 'text_delta' || event.assistantMessageEvent?.type === 'text_delta') report.textDeltas = (report.textDeltas || 0) + 1;
          }
        }
      } catch (error) { if (!abort.signal.aborted) throw error; }
    })();
    // Attach rejection handling immediately while UI setup is in progress.
    streamTask.catch(() => {});
    browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH || 'C:/Users/bigfish/scoop/apps/googlechrome/current/chrome.exe' });
    const context = await browser.newContext({ viewport: { width: 1440, height: 1000 } });
    page = await context.newPage();
    page.setDefaultTimeout(15000);
    page.on('pageerror', error => report.errors.push(error.message));
    await page.goto(ui);
    await page.getByRole('button', { name: '打开设置', exact: true }).click();
    await page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '执行节点', exact: true }).click();
    await page.getByRole('button', { name: '连接本机节点', exact: true }).click();
    if (auth.required) {
      await page.getByLabel('节点密码', { exact: true }).fill(password);
      await page.getByRole('button', { name: '登录', exact: true }).click();
    }
    await page.locator('.server-details [role=status]').filter({ hasText: '在线' }).waitFor();
    pass('real node authentication and workspace load through the client proxy');
    await page.getByRole('button', { name: '返回会话', exact: true }).click();
    await page.locator('.project-row').filter({ hasText: project.name }).click();
    await page.locator('.sidebar-sessions').getByRole('button', { name: title, exact: true }).click();
    await page.getByRole('combobox', { name: '模型', exact: true }).selectOption(model.id);
    let state;
    for (let index = 0; index < 60; index++) {
      state = await command('get_state');
      if (state.modelId === model.id && state.provider === model.provider) break;
      await pause(100);
    }
    assert.equal(state.modelId, model.id); assert.equal(state.provider, model.provider);
    assert.equal(await page.getByRole('combobox', { name: '模型', exact: true }).inputValue(), model.id);
    pass('the composer model selector applies the real session model');
    const thinking = page.getByRole('combobox', { name: '思考等级', exact: true });
    if (await thinking.locator('option[value=off]').count()) await thinking.selectOption('off');

    const send = async (prompt, expected, tool) => {
      const oldRun = (await api(prefix)).latestSettlement?.runId;
      await page.locator('.composer-main textarea').fill(prompt);
      await page.getByRole('button', { name: '发送消息', exact: true }).click();
      let settlement;
      for (let index = 0; index < 120; index++) {
        const session = await api(prefix);
        settlement = session.latestSettlement;
        if (settlement && settlement.runId !== oldRun) break;
        await pause(1000);
      }
      assert.equal(settlement?.outcome, 'completed', 'The actual provider turn did not complete.');
      report.lastSettlement = { outcome: settlement.outcome, runId: settlement.runId };
      await page.locator('.transcript .conversation-markdown').filter({ hasText: expected }).last().waitFor();
      if (tool) assert((report.events.tool_execution_end || 0) > 0, 'No actual completed tool event');
    };
    await send('你好。请只回复“你好，真实聊天已连接。”，不要执行任何工具。', '你好，真实聊天已连接');
    assert((report.events.message_update || 0) > 0, 'No real streaming message events received.');
    pass('a message sent from the UI receives the actual provider reply over SSE and completes');

    await command('set_active_tools', { tools: ['read'] });
    await send('请调用 read 工具读取 README.md 的前 3 行，确认后只回复“项目基于 Rust + Dioxus。”。必须实际调用工具，只读取文件，不执行命令、不修改文件。', '项目基于 Rust + Dioxus', true);
    await page.locator('.transcript').getByText('read', { exact: true }).first().waitFor();
    pass('the real read tool result and assistant reply are displayed inside the same conversation');
    const snapshot = await api(prefix + '/snapshot');
    const assistantTextCount = snapshot.messages.filter(message => {
      if ((message.role || message.kind) !== 'assistant') return false;
      return typeof message.content === 'string' ? Boolean(message.content.trim())
        : message.content?.some(block => ['text', 'output_text'].includes(block.type) && block.text?.trim());
    }).length;
    await page.waitForFunction(expected => [...document.querySelectorAll('.transcript .agent-message .conversation-markdown')].filter(item => item.textContent.trim()).length === expected, assistantTextCount);
    const users = snapshot.messages.filter(message => (message.role || message.kind) === 'user').length;
    await page.waitForFunction(expected => document.querySelectorAll('.transcript .user-message').length === expected, users);
    const toolCalls = snapshot.messages.flatMap(message => Array.isArray(message.content) ? message.content : []).filter(block => ['toolCall', 'tool_use', 'tool_call'].includes(block.type)).length;
    assert.equal(await page.locator('.transcript .conversation-tool').count(), toolCalls);
    assert.equal(await page.locator('.transcript .call-status').filter({ hasText: /等待工具结果|结果尚未提供|运行中/ }).count(), 0);
    pass('user messages, assistant text and tool cards match canonical history counts without duplicate partial/final rows');
    assert.deepEqual(report.errors, []);
    await page.screenshot({ path: 'artifacts/live-chat-desktop.png', animations: 'disabled' });
    await page.setViewportSize({ width: 390, height: 844 });
    assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
    await page.screenshot({ path: 'artifacts/live-chat-mobile.png', animations: 'disabled' });
    pass('the real conversation remains usable on a mobile viewport without overflow');
    const shortcut = await page.context().newPage();
    shortcut.on('pageerror', error => report.errors.push(error.message));
    await shortcut.goto(`${ui}?session=${encodeURIComponent(sessionId)}`);
    await shortcut.getByRole('button', { name: '发送消息', exact: true }).waitFor();
    await shortcut.locator('.transcript .conversation-markdown').filter({ hasText: '项目基于 Rust + Dioxus' }).last().waitFor();
    await shortcut.close();
    pass('a session URL reconnects to its serving node and opens the actual saved conversation');
    if (auth.required) {
      const lockedContext = await browser.newContext();
      await lockedContext.addInitScript(() => localStorage.setItem('rpi-work-nodes-v1', JSON.stringify({ version: 1, recent: ['http://127.0.0.1:1'] })));
      const locked = await lockedContext.newPage();
      locked.on('pageerror', error => report.errors.push(error.message));
      await locked.goto(`${ui}?session=${encodeURIComponent(sessionId)}`);
      await locked.getByLabel('节点密码', { exact: true }).fill(password);
      await locked.getByRole('button', { name: '登录', exact: true }).click();
      await locked.getByRole('button', { name: '发送消息', exact: true }).waitFor();
      await locked.locator('.transcript .conversation-markdown').filter({ hasText: '项目基于 Rust + Dioxus' }).last().waitFor();
      await lockedContext.close();
      pass('an unauthenticated session link asks for the existing node password and resumes the target chat after login');
    }
    await command('set_active_tools', { tools: originalTools }); originalTools = undefined;
    assert.deepEqual(report.errors, []);
    report.passed = true;
  } catch (error) {
    report.error = error.message;
    if (page) await page.screenshot({ path: 'artifacts/live-chat-failure.png' }).catch(() => {});
    throw error;
  } finally {
    abort.abort(); if (streamTask) await streamTask.catch(error => { report.streamError = error.message; });
    if (originalTools && report.sessionId) await api(`sessions/${report.sessionId}/commands`, { type: 'set_active_tools', tools: originalTools }).catch(() => {});
    if (browser) await browser.close();
    report.finishedAt = new Date().toISOString();
    fs.writeFileSync('artifacts/live-chat-validation.json', JSON.stringify(report, null, 2));
  }
})().catch(error => { console.error(error.message); process.exitCode = 1; });
