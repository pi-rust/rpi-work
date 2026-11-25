// Session admission/lifecycle acceptance: isolated HTTP/WS protocol fixture only.
// No installed Agent, model, credentials or preview on 4181/4193 is used.
const assert = require('node:assert/strict')
const path = require('node:path')
const net = require('node:net')
const fs = require('node:fs/promises')
const { spawn } = require('node:child_process')
const { chromium } = require(require.resolve('playwright', { paths: [path.resolve(__dirname, '../../../tools')] }))
const { startFixture } = require('../../../tests/fixtures/ui-fixture.cjs')
const root = path.resolve(__dirname, '..')
const freePort = () => new Promise(resolve => { const server = net.createServer(); server.listen(0, '127.0.0.1', () => { const port = server.address().port; server.close(() => resolve(port)) }) })
;(async () => {
  const fixture = await startFixture(0, { nativeQueries: true, includeHistoryAssistant: true, resumePreview: true })
  const port = await freePort(), origin = `http://127.0.0.1:${port}`
  const preview = spawn(process.execPath, [path.join(root, 'node_modules/vite/bin/vite.js'), 'preview', '--host', '127.0.0.1', '--port', String(port), '--strictPort'], { cwd: root, env: { ...process.env, RPI_NODE_URL: fixture.url }, stdio: 'ignore' })
  let browser
  const failures = [], errors = []
  const check = async (name, run) => { try { await run(); console.log('PASS', name) } catch (error) { failures.push(name); console.error('FAIL', name, error.message) } }
  try {
    for (let i = 0; i < 120; i++) { try { if ((await fetch(origin)).ok) break } catch {} await new Promise(resolve => setTimeout(resolve, 100)) }
    browser = await chromium.launch()
    const page = await browser.newPage({ viewport: { width: 1440, height: 940 } })
    page.setDefaultTimeout(10000)
    page.on('pageerror', error => errors.push(error.message))
    let socket
    page.on('websocket', next => { if (next.url().includes('/api/v1/realtime')) socket = next })
    const ack = command => socket.waitForEvent('framereceived', { predicate: frame => JSON.parse(frame.payload).status?.outcome?.value?.command === command, timeout: 10000 })
    const nextSnapshot = () => page.waitForResponse(response => /\/sessions\/dark\/snapshot(?:\?|$)/.test(response.url()) && response.ok())
    const rendered = () => page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))))
    const mutationCount = () => fixture.requests.filter(request => request.method === 'WS' && ['reserve', 'execute'].includes(request.body.type)).length
    const waitCheck = () => page.waitForFunction(() => [...document.querySelectorAll('button')].some(button => button.textContent.trim() === '只读核对' && !button.disabled))
    const open = async () => {
      await page.goto(origin + '/?live=1')
      await page.getByText('帮我为工作台增加一个深色主题，并保持手机端可用。', { exact: true }).waitFor().catch(async error => {
        console.error('session open diagnostic:', await page.locator('main').innerText(), 'page errors:', errors)
        throw error
      })
      await page.locator('textarea').fill('权限与结算阶段必须保留的草稿')
    }
    await check('unknown session access never enables send or queue', async () => {
      fixture.controls.accessStatus = 404
      await open()
      assert.equal(await page.getByRole('button', { name: '发送', exact: true }).isDisabled(), true)
      assert.equal(await page.getByRole('button', { name: '加入队列', exact: true }).isDisabled(), true)
      assert.equal(await page.locator('textarea').inputValue(), '权限与结算阶段必须保留的草稿')
      assert.equal(await page.getByRole('status', { name: '会话运行观察' }).textContent(), '会话观察：尚无运行阶段观察')
      assert.equal(mutationCount(), 0)
    })
    delete fixture.controls.accessStatus
    await check('exact session commands and configuration observations cannot borrow global support', async () => {
      fixture.controls.sessionAccess = { supportedCommands: ['get_state', 'get_message_summaries', 'get_message_window', 'get_queue', 'get_available_models', 'get_available_thinking_levels'] }
      fixture.controls.configurationCommands = ['get_state', 'get_available_models', 'get_available_thinking_levels']
      await open()
      assert.equal(await page.getByRole('button', { name: '发送', exact: true }).isDisabled(), true)
      assert.equal(await page.getByRole('button', { name: '加入队列', exact: true }).isDisabled(), true)
      assert.equal(await page.getByLabel('选择模型', { exact: true }).isDisabled(), true)
      assert.equal(await page.getByLabel('选择思考等级', { exact: true }).isDisabled(), true)
    })
    delete fixture.controls.sessionAccess
    delete fixture.controls.configurationCommands
    await check('accepted prompt stays locked until actual settlement; ACK is not idle', async () => {
      await open()
      await page.getByLabel('选择模型', { exact: true }).waitFor()
      await page.getByRole('button', { name: '发送', exact: true }).click()
      await page.waitForFunction(() => document.querySelector('textarea')?.value === '')
      await page.locator('textarea').fill('不能因ACK提前变成第二次prompt')
      assert.equal(await page.getByLabel('选择模型', { exact: true }).isDisabled(), true)
      await page.getByRole('button', { name: '中止', exact: true }).waitFor()
      fixture.emit({ type: 'agent_settled', outcome: 'completed', pendingResume: false })
      await page.waitForFunction(() => !document.querySelector('select[aria-label="选择模型"]')?.disabled)
      assert.equal(await page.locator('textarea').inputValue(), '不能因ACK提前变成第二次prompt')
    })
    await check('snapshot/replay of an active run disables configuration and offers observed abort', async () => {
      fixture.emit({ type: 'agent_start' })
      await open()
      assert.equal(await page.getByLabel('选择模型', { exact: true }).isDisabled(), true)
      await page.getByRole('button', { name: '中止', exact: true }).waitFor()
      fixture.emit({ type: 'agent_settled', outcome: 'completed', pendingResume: false })
      await page.waitForFunction(() => !document.querySelector('select[aria-label="选择模型"]')?.disabled)
    })
    await check('canonical history indices survive empty or unsupported renderer records', async () => {
      fixture.emit({ type: 'message_end', message: { entryId: 'empty-native-record', role: 'assistant', content: [] } })
      fixture.emit({ type: 'message_end', message: { entryId: 'after-empty-user', role: 'user', content: '空原生记录之后的真实用户消息' } })
      fixture.emit({ type: 'message_end', message: { entryId: 'after-empty-answer', role: 'assistant', content: '空原生记录之后的真实回复' } })
      await open()
      await page.getByText('空原生记录之后的真实用户消息', { exact: true }).waitFor()
      await page.getByText('空原生记录之后的真实回复', { exact: true }).waitFor()
    })
    await check('a later failed access observation revokes the old binding affordances', async () => {
      await open()
      await page.waitForFunction(() => { const model = document.querySelector('select[aria-label="选择模型"]'); return model && !model.disabled })
      fixture.controls.accessStatus = 404
      try {
        const failed = page.waitForResponse(response => response.url().endsWith('/access') && response.status() === 404)
        fixture.emit({ type: 'agent_start' })
        fixture.emit({ type: 'agent_settled', outcome: 'completed', pendingResume: false })
        await failed
        await page.getByText(/session access unavailable/).waitFor()
        assert.equal(await page.getByRole('button', { name: '发送', exact: true }).isDisabled(), true)
        assert.equal(await page.getByRole('button', { name: '加入队列', exact: true }).isDisabled(), true)
      } finally { delete fixture.controls.accessStatus }
    })
    await check('a late project diff cannot reopen after project A to B to A', async () => {
      await open()
      await page.getByRole('button', { name: '文件面板', exact: true }).click()
      const panel = page.getByTestId('run-panel')
      await panel.getByTestId('tab-changes').waitFor()
      let release, entered, delivered
      const held = new Promise(resolve => { release = resolve })
      const started = new Promise(resolve => { entered = resolve })
      const finished = new Promise(resolve => { delivered = resolve })
      const pattern = /\/api\/v1\/projects\/[^/]+\/git\/diff(?:\?|$)/
      await page.route(pattern, async route => {
        const response = await route.fetch(), body = await response.json()
        entered(); await held
        await route.fulfill({ response, json: { ...body, patch: 'diff --git a/old b/old\n+迟到的旧项目差异' } })
        delivered()
      })
      try {
        const request = page.waitForRequest(request => pattern.test(request.url()))
        await panel.getByText('src/theme.rs', { exact: true }).first().click()
        await request
        await Promise.race([started, new Promise((_, reject) => { const timer = setTimeout(() => reject(Error('Diff interception did not complete')), 10000); timer.unref() })])
        await page.getByRole('button', { name: '验证 RPC 事件协议', exact: true }).click()
        await page.getByText('在线', { exact: true }).waitFor()
        await page.getByRole('button', { name: '为工作台添加深色模式', exact: true }).click()
        await page.getByText('在线', { exact: true }).waitFor()
        release(); await finished
        await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))))
        assert.doesNotMatch(await panel.innerText(), /迟到的旧项目差异/)
      } finally { release(); await page.unroute(pattern) }
    })
    await check('an answered dialog remains visible but cannot be submitted again before closure', async () => {
      await open()
      fixture.emit({ type: 'agent_start' })
      fixture.emit({ type: 'ui_dialog_request', requestId: 'reply-ledger', ui: { kind: 'selector', purpose: 'permission', question: '等待真实关闭事件', options: [{ title: '仅批准本次', value: 'actual-option-once' }] } })
      const approve = page.getByRole('button', { name: '仅批准本次', exact: true })
      await approve.click()
      await page.waitForFunction(() => document.querySelector('button') && [...document.querySelectorAll('button')].some(button => button.textContent.trim() === '仅批准本次' && button.disabled))
      assert.equal(await page.getByText('等待真实关闭事件', { exact: true }).count(), 1)
      fixture.emit({ type: 'ui_dialog_closed', requestId: 'reply-ledger' })
      await approve.waitFor({ state: 'detached' })
      fixture.emit({ type: 'agent_settled', outcome: 'completed', pendingResume: false })
    })
    await check('a canonical terminal snapshot after reconnect releases the known accepted window', async () => {
      await open()
      await page.locator('textarea').fill('断线期间完成的真实请求')
      await page.getByRole('button', { name: '发送', exact: true }).click()
      await page.waitForFunction(() => document.querySelector('textarea')?.value === '')
      fixture.emit({ type: 'agent_start' })
      fixture.emit({ type: 'message_end', message: { entryId: 'before-disconnect', role: 'assistant', content: '断线前已观察到的运行片段' } })
      await page.getByText('断线前已观察到的运行片段', { exact: true }).waitFor()
      let terminalSeq = Number.MAX_SAFE_INTEGER
      const restored = page.waitForResponse(async response => {
        if (!/\/sessions\/dark\/snapshot(?:\?|$)/.test(response.url()) || !response.ok()) return false
        return (await response.json()).lastSeq >= terminalSeq
      })
      fixture.disconnect('dark', true)
      terminalSeq = fixture.emit({ type: 'agent_settled', outcome: 'completed', pendingResume: false })
      await restored
      await page.getByText('在线', { exact: true }).waitFor()
      await page.getByRole('button', { name: '发送', exact: true }).waitFor()
      await page.locator('textarea').fill('重连后可发送的新草稿')
      await page.waitForFunction(() => document.querySelector('button[aria-label="发送"]')?.disabled === false)
      assert.equal(await page.getByRole('button', { name: '发送', exact: true }).isEnabled(), true)
    })
    await check('neither an old terminal nor a restored active run unlocks the accepted window', async () => {
      await open()
      await page.getByRole('button', { name: '发送', exact: true }).click()
      await page.waitForFunction(() => document.querySelector('textarea')?.value === '')
      let restored = nextSnapshot()
      fixture.disconnect('dark', true)
      await restored
      await page.getByText('在线', { exact: true }).waitFor()
      await page.getByRole('button', { name: '中止', exact: true }).waitFor()
      assert.equal(await page.getByRole('button', { name: '发送', exact: true }).count(), 0)
      assert.equal(await page.getByLabel('选择模型', { exact: true }).isDisabled(), true)
      fixture.emit({ type: 'agent_start' })
      fixture.emit({ type: 'message_end', message: { entryId: 'active-reconnect', role: 'assistant', content: '旧结算不能关闭当前运行' } })
      await page.getByText('旧结算不能关闭当前运行', { exact: true }).waitFor()
      restored = nextSnapshot(); fixture.disconnect('dark', true)
      await restored
      await page.getByText('在线', { exact: true }).waitFor()
      await page.getByRole('button', { name: '中止', exact: true }).waitFor()
      assert.equal(await page.getByRole('button', { name: '发送', exact: true }).count(), 0)
      assert.equal(await page.getByLabel('选择模型', { exact: true }).isDisabled(), true)
      fixture.emit({ type: 'agent_settled', outcome: 'completed', pendingResume: false })
      await page.getByRole('button', { name: '发送', exact: true }).waitFor()
    })
    await check('a missing exact abort command stays disabled even for an active snapshot', async () => {
      fixture.controls.sessionAccess = { supportedCommands: ['prompt', 'get_state', 'get_available_models', 'set_model', 'get_available_thinking_levels', 'set_thinking_level'] }
      fixture.emit({ type: 'agent_start' })
      try {
        await open()
        assert.equal(await page.getByRole('button', { name: '中止', exact: true }).isDisabled(), true)
        assert.equal(await page.getByLabel('选择模型', { exact: true }).isDisabled(), true)
      } finally {
        delete fixture.controls.sessionAccess
        fixture.emit({ type: 'agent_settled', outcome: 'completed', pendingResume: false })
      }
    })
    await check('an observed settlement before a deliberately delayed ACK does not relock the run', async () => {
      fixture.controls.promptDelayMs = 1500
      try {
        await open()
        let acknowledged = false
        const accepted = ack('prompt').then(() => { acknowledged = true })
        const dispatched = socket.waitForEvent('framesent', { predicate: frame => { const value = JSON.parse(frame.payload); return value.type === 'execute' && value.payload?.type === 'prompt' }, timeout: 10000 })
        await page.getByRole('button', { name: '发送', exact: true }).click()
        await dispatched
        const terminal = socket.waitForEvent('framereceived', { predicate: frame => JSON.parse(frame.payload).events?.some(event => event.event_type === 'agent_settled'), timeout: 10000 })
        fixture.emit({ type: 'agent_start' })
        fixture.emit({ type: 'agent_settled', outcome: 'completed', pendingResume: false })
        await terminal; await rendered()
        assert.equal(acknowledged, false, 'the terminal observation really precedes ACK delivery')
        await accepted
        await page.getByRole('button', { name: '发送', exact: true }).waitFor()
        await page.waitForFunction(() => document.querySelector('textarea')?.value === '')
        await page.locator('textarea').fill('结算后可以编辑的新稿')
        await page.waitForFunction(() => document.querySelector('button[aria-label="发送"]')?.disabled === false)
        assert.equal(await page.getByRole('button', { name: '发送', exact: true }).isEnabled(), true)
      } finally { delete fixture.controls.promptDelayMs }
    })
    await check('an accepted abort ACK is not settlement and does not unlock configuration', async () => {
      await open()
      await page.getByRole('button', { name: '发送', exact: true }).click()
      await page.waitForFunction(() => document.querySelector('textarea')?.value === '')
      const accepted = ack('abort')
      await page.getByRole('button', { name: '中止', exact: true }).click()
      await accepted; await rendered()
      assert.equal(await page.getByRole('button', { name: '发送', exact: true }).count(), 0)
      assert.equal(await page.getByRole('button', { name: '中止', exact: true }).isDisabled(), true)
      assert.equal(await page.getByLabel('选择模型', { exact: true }).isDisabled(), true)
      fixture.emit({ type: 'agent_settled', outcome: 'cancelled', pendingResume: false })
      await page.getByRole('button', { name: '发送', exact: true }).waitFor()
    })
    await check('a confirmed configuration write followed by rejected readback stays unknown and locked', async () => {
      await open()
      const model = page.getByLabel('选择模型', { exact: true })
      await page.waitForFunction(() => document.querySelector('select[aria-label="选择模型"]')?.disabled === false)
      fixture.controls.failState = true
      try {
        const written = ack('set_model')
        await model.selectOption(JSON.stringify(['openai', 'openai/gpt-6-astra']))
        assert.equal(JSON.parse((await written).payload).status.outcome.value.success, true, 'the model write really succeeded before readback failed')
        await page.waitForFunction(() => [...document.querySelectorAll('body *')].some(element => element.textContent?.includes('Test fixture: command rejected')))
        await rendered()
        assert.equal(await model.isDisabled(), true, 'a readback rejection is not a rejected write')
        assert.equal(await model.inputValue(), '', 'the pre-write model is no longer current authoritative runtime')
        assert.equal(await page.getByRole('button', { name: '发送', exact: true }).isDisabled(), true)
        assert.equal(await page.locator('textarea').inputValue(), '权限与结算阶段必须保留的草稿')
      } finally { delete fixture.controls.failState }
      const writes = mutationCount()
      await waitCheck()
      await page.getByRole('button', { name: '只读核对', exact: true }).click()
      await page.waitForFunction(() => document.querySelector('select[aria-label="选择模型"]')?.disabled === false)
      assert.equal(await model.inputValue(), JSON.stringify(['openai', 'openai/gpt-6-astra']))
      assert.equal(mutationCount(), writes, 'a proven setter ACK needs only fresh readback, never another write')
    })
    await check('a newer failed access observation cannot be overwritten by older successful metadata', async () => {
      await open()
      await page.waitForFunction(() => document.querySelector('select[aria-label="选择模型"]')?.disabled === false)
      let release, enter, deadline
      const gate = new Promise(resolve => { release = resolve })
      const entered = new Promise(resolve => { enter = resolve })
      const path = url => url.pathname === '/api/v1/sessions/dark/configuration'
      const handler = async route => { const response = await route.fetch(); enter(response.status()); await gate; await route.fulfill({ response }) }
      await page.route(path, handler)
      try {
        fixture.controls.models = [{ id: 'late-read-only-model', provider: 'read-proof', name: '旧读取专属模型', reasoning: false, contextWindow: 64000 }]
        fixture.emit({ type: 'agent_settled', outcome: 'completed', pendingResume: false })
        assert.equal(await Promise.race([entered, new Promise((_, reject) => { deadline = setTimeout(() => reject(new Error('older metadata did not enter the configuration gate')), 10000) })]), 200)
        fixture.controls.accessStatus = 404
        const denied = page.waitForResponse(response => response.url().includes('/sessions/dark/access') && response.status() === 404)
        fixture.emit({ type: 'agent_settled', outcome: 'completed', pendingResume: false })
        await denied
        await page.waitForFunction(() => document.querySelector('button[aria-label="发送"]')?.title.includes('未取得当前会话能力'))
        const completed = socket.waitForEvent('framereceived', { predicate: frame => JSON.parse(frame.payload).value?.command === 'get_available_thinking_levels', timeout: 10000 })
        release()
        await completed; await rendered()
        assert.equal(await page.getByRole('button', { name: '发送', exact: true }).isDisabled(), true)
        const model = page.getByLabel('选择模型', { exact: true })
        assert.equal(await model.isDisabled(), true, 'old metadata must not revive revoked affordances')
        assert.equal((await model.locator('option').allTextContents()).includes('旧读取专属模型'), false, 'the late successful catalogue was discarded')
      } finally {
        clearTimeout(deadline); release(); delete fixture.controls.accessStatus; delete fixture.controls.models
        await page.unroute(path, handler)
      }
    })
    await check('a lost prompt ACK recovers its real receipt with GET only, not settlement or draft resubmission', async () => {
      await open()
      fixture.controls.dropAckFor = 'prompt'
      await page.getByRole('button', { name: '发送', exact: true }).click()
      await waitCheck()
      const writes = mutationCount()
      const receipt = page.waitForResponse(response => /\/realtime\/operations\/fixture-owner\/fixture-operation-\d+$/.test(response.url()))
      await page.getByRole('button', { name: '只读核对', exact: true }).click()
      assert.equal((await receipt).request().method(), 'GET')
      await page.getByText('只读核对完成；受理不等于运行结算，保留的草稿未自动发送。', { exact: true }).waitFor()
      assert.equal(await page.getByRole('button', { name: '发送', exact: true }).count(), 0)
      assert.equal(await page.locator('textarea').inputValue(), '权限与结算阶段必须保留的草稿')
      assert.equal(mutationCount(), writes)
      fixture.emit({ type: 'agent_start' })
      fixture.emit({ type: 'agent_settled', outcome: 'completed', pendingResume: false })
      await page.getByRole('button', { name: '发送', exact: true }).waitFor()
    })
    await check('a transport-success receipt with HostResponse success false is a refusal, not acceptance', async () => {
      await open()
      fixture.controls.dropAckFor = 'prompt'; fixture.controls.failPrompt = true
      try {
        await page.getByRole('button', { name: '发送', exact: true }).click()
        await waitCheck()
        const writes = mutationCount()
        await page.getByRole('button', { name: '只读核对', exact: true }).click()
        await page.getByText('原请求回执明确拒绝: Test fixture: command rejected', { exact: true }).waitFor()
        assert.equal(await page.getByRole('button', { name: '发送', exact: true }).isEnabled(), true)
        assert.equal(await page.locator('textarea').inputValue(), '权限与结算阶段必须保留的草稿')
        assert.equal(mutationCount(), writes)
      } finally { delete fixture.controls.failPrompt; delete fixture.controls.dropAckFor }
    })
    await check('runtime matching an unknown model write cannot replace its missing receipt outcome', async () => {
      await open()
      const model = page.getByLabel('选择模型', { exact: true })
      await page.waitForFunction(() => document.querySelector('select[aria-label="选择模型"]')?.disabled === false)
      fixture.controls.dropAckFor = 'set_model'; fixture.controls.receiptUnknown = true
      try {
        await model.selectOption(JSON.stringify(['deepseek', 'deepseek/deepseek-v4-pro']))
        await waitCheck()
        const writes = mutationCount()
        await page.getByRole('button', { name: '只读核对', exact: true }).click()
        await page.getByText('仍有请求结果或效果未核实，继续锁定；不会自动重发。', { exact: true }).waitFor()
        assert.equal(await model.isDisabled(), true)
        assert.equal(await page.getByRole('button', { name: '发送', exact: true }).isDisabled(), true)
        delete fixture.controls.receiptUnknown
        await waitCheck()
        await page.getByRole('button', { name: '只读核对', exact: true }).click()
        await page.waitForFunction(() => document.querySelector('select[aria-label="选择模型"]')?.disabled === false)
        assert.equal(await model.inputValue(), JSON.stringify(['deepseek', 'deepseek/deepseek-v4-pro']))
        assert.equal(mutationCount(), writes)
      } finally { delete fixture.controls.receiptUnknown; delete fixture.controls.dropAckFor }
    })
    await check('without an SDK operation reference or setter ACK proof, malformed results stay locked', async () => {
      await open()
      const model = page.getByLabel('选择模型', { exact: true })
      await page.waitForFunction(() => document.querySelector('select[aria-label="选择模型"]')?.disabled === false)
      fixture.controls.malformedAckFor = 'set_model'
      await model.selectOption(JSON.stringify(['openai', 'openai/gpt-6-astra']))
      await page.getByText('部分请求未取得可核对回执，保持锁定；不会自动重发。', { exact: true }).waitFor()
      assert.equal(await page.getByRole('button', { name: '只读核对', exact: true }).isDisabled(), true)
      assert.equal(await model.isDisabled(), true)
      assert.equal(await model.inputValue(), '')
      assert.equal(await page.locator('textarea').inputValue(), '权限与结算阶段必须保留的草稿')
    })
    await check('a recovered abort receipt still waits for canonical settlement', async () => {
      await open()
      await page.getByRole('button', { name: '发送', exact: true }).click()
      await page.waitForFunction(() => document.querySelector('textarea')?.value === '')
      fixture.emit({ type: 'agent_start' })
      fixture.emit({ type: 'message_end', message: { entryId: 'abort-receipt-active', role: 'assistant', content: '停止回执不是结算' } })
      await page.getByText('停止回执不是结算', { exact: true }).waitFor()
      fixture.controls.dropAckFor = 'abort'
      await page.getByRole('button', { name: '中止', exact: true }).click()
      await waitCheck()
      const writes = mutationCount()
      await page.getByRole('button', { name: '只读核对', exact: true }).click()
      await page.getByText('只读核对完成；受理不等于运行结算，保留的草稿未自动发送。', { exact: true }).waitFor()
      assert.equal(await page.getByRole('button', { name: '中止', exact: true }).isDisabled(), true)
      assert.equal(await page.getByLabel('选择模型', { exact: true }).isDisabled(), true)
      assert.equal(mutationCount(), writes)
      fixture.emit({ type: 'agent_settled', outcome: 'cancelled', pendingResume: false })
      await page.getByRole('button', { name: '发送', exact: true }).waitFor()
    })
    await check('a receipt read started before reconnect cannot unlock the new observation scope', async () => {
      await open()
      await page.waitForFunction(() => document.querySelector('select[aria-label="选择模型"]')?.disabled === false)
      fixture.controls.dropAckFor = 'set_model'
      await page.getByLabel('选择模型', { exact: true }).selectOption(JSON.stringify(['deepseek', 'deepseek/deepseek-v4-pro']))
      await waitCheck()
      const writes = mutationCount()
      let release, enter, deadline
      const gate = new Promise(resolve => { release = resolve })
      const entered = new Promise(resolve => { enter = resolve })
      const path = url => /\/realtime\/operations\//.test(url.pathname)
      const handler = async route => { const response = await route.fetch(); enter(); await gate; await route.fulfill({ response }) }
      await page.route(path, handler)
      try {
        await page.getByRole('button', { name: '只读核对', exact: true }).click()
        await Promise.race([entered, new Promise((_, reject) => { deadline = setTimeout(() => reject(new Error('receipt GET did not enter its gate')), 10000) })])
        const restored = nextSnapshot(); fixture.disconnect('dark', true)
        await restored
        await page.getByText('在线', { exact: true }).waitFor()
        release()
        await waitCheck()
        assert.equal(await page.getByLabel('选择模型', { exact: true }).isDisabled(), true)
        assert.equal(mutationCount(), writes)
      } finally { clearTimeout(deadline); release(); await page.unroute(path, handler) }
      await page.getByRole('button', { name: '只读核对', exact: true }).click()
      await page.waitForFunction(() => document.querySelector('select[aria-label="选择模型"]')?.disabled === false)
      assert.equal(mutationCount(), writes)
    })
    await check('receipt recovery cannot strand metadata still being read after reconnect', async () => {
      await open()
      await page.waitForFunction(() => document.querySelector('select[aria-label="选择模型"]')?.disabled === false)
      let release, enter, deadline, calls = 0
      const gate = new Promise(resolve => { release = resolve })
      const entered = new Promise(resolve => { enter = resolve })
      const path = url => url.pathname === '/api/v1/sessions/dark/configuration'
      const handler = async route => { const response = await route.fetch(); if (++calls === 1) { enter(); await gate }; await route.fulfill({ response }) }
      await page.route(path, handler)
      try {
        fixture.controls.dropAckFor = 'prompt'
        await page.getByRole('button', { name: '发送', exact: true }).click()
        await Promise.race([entered, new Promise((_, reject) => { deadline = setTimeout(() => reject(new Error('reconnected metadata did not reach its gate')), 10000) })])
        await waitCheck()
        const writes = mutationCount()
        await page.getByRole('button', { name: '只读核对', exact: true }).click()
        await page.getByText('只读核对完成；受理不等于运行结算，保留的草稿未自动发送。', { exact: true }).waitFor()
        release()
        await page.waitForFunction(() => document.querySelector('button[aria-label="中止"]')?.disabled === false)
        assert.equal(mutationCount(), writes)
        fixture.emit({ type: 'agent_start' })
        fixture.emit({ type: 'agent_settled', outcome: 'completed', pendingResume: false })
      } finally { clearTimeout(deadline); release(); await page.unroute(path, handler); delete fixture.controls.dropAckFor }
    })
    await check('a lost queue ACK recovers the original HostResponse envelope without repeating queue_message', async () => {
      await open()
      fixture.controls.dropAckFor = 'queue_message'
      try {
        await page.getByRole('button', { name: '加入队列', exact: true }).click()
        await waitCheck()
        const writes = mutationCount()
        await page.getByRole('button', { name: '只读核对', exact: true }).click()
        await page.getByText('只读核对完成；受理不等于运行结算，保留的草稿未自动发送。', { exact: true }).waitFor()
        await page.waitForFunction(() => document.querySelector('button[aria-label="发送"]')?.disabled === false)
        assert.equal(await page.locator('textarea').inputValue(), '权限与结算阶段必须保留的草稿')
        assert.equal(mutationCount(), writes)
      } finally { delete fixture.controls.dropAckFor }
    })
    const button = name => page.getByRole('button', { name, exact: true })
    const resume = button('继续队列')
    const resumeWrites = () => fixture.requests.filter(request => request.method === 'WS' && request.body.type === 'execute' && request.body.payload.type === 'resume_pending')
    const resumeSettlement = (pendingResume, requestId = 'fixture-resume-request') =>
      fixture.emit({ type: 'agent_settled', outcome: pendingResume ? 'interrupted' : 'completed', pendingResume, requestId })
    const openQueue = async () => {
      await open()
      await button('运行面板').click()
      await page.getByTestId('run-panel').getByText(/^队列 \d+$/).waitFor()
    }
    const enabled = locator => locator.evaluate(button => !button.disabled)
    const waitEnabled = name => page.waitForFunction(label => Array.from(document.querySelectorAll('button')).some(button => button.textContent.trim() === label && !button.disabled), name)
    await check('queued rows and failed settlements need an explicit resume marker and exact session capability', async () => {
      fixture.setQueue([{ entryId: 'q-resume', queue: 'followUp', text: '等待恢复的后续消息', status: 'queued' }])
      fixture.emit({ type: 'agent_settled', outcome: 'failed', pendingResume: false })
      await openQueue()
      await page.getByText('等待恢复的后续消息', { exact: true }).waitFor()
      assert.equal(await resume.count(), 0)
      assert.equal(await button('重试').count(), 0)
      fixture.controls.sessionAccess = { supportedCommands: ['get_state', 'get_queue', 'prompt'] }
      try {
        resumeSettlement(true, 'interrupted-before-resume')
        await openQueue()
        await page.getByText('等待恢复的后续消息', { exact: true }).waitFor()
        assert.equal(await resume.count(), 0)
        assert.equal(resumeWrites().length, 0)
        fixture.controls.sessionAccess = { capabilities: { history: false } }
        await openQueue()
        await page.getByText('等待恢复的后续消息', { exact: true }).waitFor()
        assert.equal(await resume.count(), 0)
      } finally { delete fixture.controls.sessionAccess }
    })
    await check('one execution across repeated input, ACK held through exact settlement, draft and queue preserved until readback', async () => {
      await open()
      await button('浏览器').click()
      await page.getByRole('textbox', { name: '预览地址', exact: true }).fill(new URL('resume-preview', fixture.url).href)
      await button('打开').click()
      const previewInput = page.frameLocator('.real-browser iframe').getByLabel('继续队列预览')
      await previewInput.fill('继续队列后保留网页输入')
      await page.evaluate(() => { window.resumeFrame = document.querySelector('.real-browser iframe') })
      const previewLoads = () => fixture.requests.filter(request => request.path === '/resume-preview').length
      const loads = previewLoads()
      await button('运行面板').click()
      await waitEnabled('继续队列')
      const draft = page.locator('textarea')
      await draft.evaluate(input => input.setSelectionRange(2, 7))
      let release, enter, timer
      const gate = new Promise(resolve => { release = resolve })
      const entered = new Promise(resolve => { enter = resolve })
      fixture.controls.resumeGate = async () => { enter(); await gate }
      const writes = resumeWrites().length
      try {
        const accepted = ack('resume_pending')
        await resume.press('Enter')
        await Promise.race([entered, new Promise((_, reject) => { timer = setTimeout(() => reject(Error('resume did not reach the owner')), 10000) })])
        await page.waitForFunction(() => [...document.querySelectorAll('button')].some(button => button.textContent.trim() === '继续队列' && button.disabled))
        await resume.evaluate(button => button.click())
        assert.equal(resumeWrites().length, writes + 1)
        assert.deepEqual(resumeWrites().at(-1).body.payload, { type: 'resume_pending' })
        release(); await accepted; await rendered()
        assert.equal(await enabled(resume), false)
        assert.equal(await button('发送').count(), 0)
        await page.getByText('等待恢复的后续消息', { exact: true }).waitFor()
        const shots = path.resolve(root, '../../artifacts/queue-resume')
        await fs.mkdir(shots, { recursive: true })
        await page.screenshot({ path: path.join(shots, 'awaiting-settlement-1440.png'), animations: 'disabled' })
        fixture.setQueue([])
        resumeSettlement(false)
        await resume.waitFor({ state: 'hidden' })
        await page.getByTestId('run-panel').getByText('队列 0', { exact: true }).waitFor()
        assert.equal(await draft.inputValue(), '权限与结算阶段必须保留的草稿')
        assert.deepEqual(await draft.evaluate(input => [input.selectionStart, input.selectionEnd]), [2, 7])
        await button('浏览器').click()
        assert.equal(await page.evaluate(() => window.resumeFrame === document.querySelector('.real-browser iframe')), true)
        assert.equal(await previewInput.inputValue(), '继续队列后保留网页输入')
        assert.equal(previewLoads(), loads)
      } finally { clearTimeout(timer); release(); delete fixture.controls.resumeGate }
    })
    await check('definite refusal permits a fresh attempt, and a skipped resume releases immediately at 320px', async () => {
      await page.setViewportSize({ width: 320, height: 940 })
      resumeSettlement(true, 'interrupted-before-refusal')
      try {
        await openQueue()
        await waitEnabled('继续队列')
        assert((await resume.boundingBox()).height >= 44)
        fixture.controls.failResume = true
        const denied = ack('resume_pending')
        await resume.click()
        assert.equal(JSON.parse((await denied).payload).status.outcome.value.success, false)
        await page.getByText('Test fixture: command rejected', { exact: true }).waitFor({ state: 'attached' })
        await waitEnabled('继续队列')
        delete fixture.controls.failResume
        fixture.controls.resumeReply = { resumed: false }
        await resume.click()
        await resume.waitFor({ state: 'hidden' })
        await button('关闭面板').click()
        await page.getByText('节点确认没有待恢复的运行', { exact: true }).waitFor()
        await page.waitForFunction(() => document.querySelector('button[aria-label="发送"]')?.disabled === false)
        assert.equal(await page.locator('textarea').inputValue(), '权限与结算阶段必须保留的草稿')
      } finally {
        delete fixture.controls.resumeReply
        delete fixture.controls.failResume
        await page.setViewportSize({ width: 1440, height: 940 })
      }
    })
    await check('lost ACK is checked with one GET after a previously active run restores its settlement', async () => {
      resumeSettlement(true, 'interrupted-before-lost-receipt')
      await openQueue()
      await waitEnabled('继续队列')
      const route = /\/sessions\/dark\/snapshot(?:\?|$)/
      let release, releaseAck, enter, timer
      const gate = new Promise(resolve => { release = resolve })
      const heldAck = new Promise(resolve => { releaseAck = resolve })
      const entered = new Promise(resolve => { enter = resolve })
      fixture.controls.resumeGate = async () => { enter(); await heldAck }
      const handler = async request => { await gate; await request.continue() }
      try {
        const writes = mutationCount()
        fixture.controls.dropAckFor = 'resume_pending'
        await resume.click()
        await Promise.race([entered, new Promise((_, reject) => { timer = setTimeout(() => reject(Error('resume did not reach the owner')), 10000) })])
        // The SDK must first observe a run consequence while the ACK is held.
        fixture.emit({ type: 'agent_start' })
        fixture.emit({ type: 'message_end', message: { entryId: 'resume-active-answer', role: 'assistant', content: '已经观察到恢复运行的输出' } })
        await page.getByText('已经观察到恢复运行的输出', { exact: true }).waitFor()
        await page.route(route, handler)
        const snapshot = page.waitForRequest(request => route.test(request.url()))
        releaseAck()
        await snapshot
        resumeSettlement(false)
        release()
        await waitCheck()
        const receipt = page.waitForResponse(response => response.url().includes('/realtime/operations/') && response.request().method() === 'GET')
        await button('只读核对').click()
        assert((await receipt).ok())
        await page.getByText('只读核对完成；受理不等于运行结算，保留的草稿未自动发送。', { exact: true }).waitFor()
        await page.waitForFunction(() => document.querySelector('button[aria-label="发送"]')?.disabled === false)
        assert.equal(mutationCount(), writes + 2)
        assert.equal(await resume.count(), 0)
      } finally { clearTimeout(timer); release(); releaseAck(); await page.unroute(route, handler); delete fixture.controls.dropAckFor; delete fixture.controls.resumeGate }
    })
    await check('a late no-op ACK from another selection cannot change the current draft or controls', async () => {
      resumeSettlement(true, 'interrupted-before-selection')
      await openQueue()
      await waitEnabled('继续队列')
      let release, enter, timer
      const gate = new Promise(resolve => { release = resolve })
      const entered = new Promise(resolve => { enter = resolve })
      fixture.controls.resumeGate = async () => { enter(); await gate }
      fixture.controls.resumeReply = { resumed: false }
      try {
        await resume.click()
        await Promise.race([entered, new Promise((_, reject) => { timer = setTimeout(() => reject(Error('resume did not reach the owner')), 10000) })])
        await button('验证 RPC 事件协议').click()
        await page.getByText('在线', { exact: true }).waitFor()
        await page.locator('textarea').fill('当前会话单独保存的草稿')
        const delivered = ack('resume_pending')
        release(); await delivered; await rendered()
        assert.equal(await page.locator('textarea').inputValue(), '当前会话单独保存的草稿')
        assert.equal(await page.getByText('节点确认没有待恢复的运行', { exact: true }).count(), 0)
        assert.equal(await resume.count(), 0)
      } finally { clearTimeout(timer); release(); delete fixture.controls.resumeGate; delete fixture.controls.resumeReply }
    })
    await check('a recovery ACK cannot supply a run lifecycle missed during disconnect', async () => {
      resumeSettlement(true, 'interrupted-before-gap')
      await openQueue()
      await waitEnabled('继续队列')
      const route = /\/sessions\/dark\/snapshot(?:\?|$)/
      let release
      const gate = new Promise(resolve => { release = resolve })
      const handler = async request => { await gate; await request.continue() }
      await page.route(route, handler)
      try {
        const snapshot = page.waitForRequest(request => route.test(request.url()))
        const writes = resumeWrites().length
        fixture.controls.dropAckFor = 'resume_pending'
        await resume.click()
        await snapshot
        resumeSettlement(false)
        release()
        await waitCheck()
        await button('只读核对').click()
        await page.getByText('只读核对完成；受理不等于运行结算，保留的草稿未自动发送。', { exact: true }).waitFor()
        await page.getByText('会话观察：请求尚待运行结算观察', { exact: true }).waitFor()
        assert.equal(await resume.count(), 0)
        assert.equal(await button('发送').count(), 0)
        assert.equal(resumeWrites().length, writes + 1)
        assert.equal(await page.locator('textarea').inputValue(), '权限与结算阶段必须保留的草稿')
      } finally { release(); await page.unroute(route, handler); delete fixture.controls.dropAckFor }
    })
    assert.deepEqual(errors, [])
    assert.deepEqual(failures, [], 'Session capability/lifecycle regressions remain')
  } finally {
    await browser?.close()
    if (preview.exitCode === null && preview.signalCode === null) await new Promise(resolve => { preview.once('exit', resolve); preview.kill('SIGTERM') })
    await fixture.close()
  }
})().catch(error => { console.error(error); process.exitCode = 1 })
