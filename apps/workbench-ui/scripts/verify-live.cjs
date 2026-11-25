// Live-mode acceptance: a real node through the wasm work-client binding.
//
// Self-contained: starts the repository fixture node, starts `vite preview`
// with the /api proxy pointed at it, drives the UI and stops both.
//
// Usage: node scripts/verify-live.cjs
const path = require('node:path')
const net = require('node:net')
const { spawn } = require('node:child_process')

// Resolve the repository's browser test dependency without a second install.
function resolvePlaywright() {
  try {
    return require.resolve('playwright', { paths: [path.resolve(__dirname, '../../../tools')] })
  } catch {
    throw new Error('先运行 npm ci --prefix tools（仓库根目录），或在 tools/ 下安装 playwright')
  }
}

const assert = require('node:assert/strict')
const { chromium } = require(resolvePlaywright())
const { startFixture } = require(path.resolve(__dirname, '../../../tests/fixtures/ui-fixture.cjs'))
const appRoot = path.resolve(__dirname, '..')

function freePort() {
  return new Promise((resolve, reject) => {
    const server = net.createServer()
    server.once('error', reject)
    server.listen(0, '127.0.0.1', () => {
      const { port } = server.address()
      server.close(() => resolve(port))
    })
  })
}

async function workspaceQueueRefresh(page) {
  // Re-selecting the session reloads its queue from the node.
  await page.getByRole('button', { name: '梳理 Agent 架构', exact: true }).click()
  await page.getByText('这是 梳理 Agent 架构 的独立历史。').waitFor()
  await page.getByRole('button', { name: '为工作台添加深色模式', exact: true }).click()
  await page.getByText('帮我为工作台增加一个深色主题，并保持手机端可用。').waitFor()
}

async function waitForServer(url, timeoutMs = 30000) {
  const deadline = Date.now() + timeoutMs
  while (Date.now() < deadline) {
    try {
      const response = await fetch(url)
      if (response.ok) return
    } catch {
      /* not listening yet */
    }
    await new Promise((resolve) => setTimeout(resolve, 200))
  }
  throw new Error(`preview server did not start: ${url}`)
}

;(async () => {
  const fixture = await startFixture(0, { nativeQueries: true, includeHistoryAssistant: true })
  const port = await freePort()
  const preview = spawn(process.execPath, [path.join(appRoot, 'node_modules/vite/bin/vite.js'), 'preview', '--port', String(port), '--strictPort'], {
    cwd: appRoot,
    env: { ...process.env, RPI_NODE_URL: fixture.url },
    stdio: 'ignore',
  })
  const origin = `http://127.0.0.1:${port}`
  const errors = []
  const passed = []
  const pass = (name) => {
    passed.push(name)
    console.log('PASS', name)
  }
  const browser = await chromium.launch({ headless: true })
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 2 })
  page.setDefaultTimeout(20000)
  page.on('pageerror', (error) => errors.push(error.message))
  // Commands travel over the realtime socket, so assert on the frames the
  // browser actually sent rather than on HTTP request bodies.
  const frames = []
  page.on('websocket', (socket) => socket.on('framesent', (frame) => frames.push(String(frame.payload))))
  const commands = (name) => frames.filter((frame) => frame.includes(`"${name}"`))
  try {
    await waitForServer(`${origin}/`)
    await page.goto(`${origin}/?live=1`)

    await page.getByRole('button', { name: '为工作台添加深色模式', exact: true }).waitFor()
    for (const title of ['梳理 Agent 架构', '检查手机端布局', '验证 RPC 事件协议']) {
      await page.getByRole('button', { name: title, exact: true }).waitFor()
    }
    pass('live node projects and sessions rendered through work-client wasm')

    await page.getByText('在线', { exact: true }).waitFor()
    pass('header reports the live connection')

    await page.getByRole('button', { name: '梳理 Agent 架构', exact: true }).click()
    await page.getByText('这是 梳理 Agent 架构 的独立历史。').waitFor()
    pass('selecting a session loads its messages and orders them in Rust')

    await page.getByRole('button', { name: '为工作台添加深色模式', exact: true }).click()
    await page.getByText('帮我为工作台增加一个深色主题，并保持手机端可用。').waitFor()
    pass('switching back restores the first session through the same presenter')

    // Layout parity: the prompt is a compact chip, the answer owns the column.
    const geometry = await page.evaluate(() => {
      const log = document.querySelector('[role="log"]')
      const prose = log?.querySelector('.wb-prose')
      const user = log?.querySelector('.wb-prompt')
      const container = log?.firstElementChild
      return {
        proseWidth: prose ? Math.round(prose.getBoundingClientRect().width) : 0,
        userWidth: user ? Math.round(user.getBoundingClientRect().width) : 0,
        containerWidth: container ? Math.round(container.getBoundingClientRect().width) : 0,
        gap: container ? getComputedStyle(container).rowGap : '',
      }
    })
    assert(geometry.proseWidth > 0 && geometry.proseWidth === geometry.containerWidth, 'prose column: ' + JSON.stringify(geometry))
    assert(geometry.userWidth > 0 && geometry.userWidth < geometry.proseWidth * .91, 'user chip stays compact: ' + JSON.stringify(geometry))
    assert(parseFloat(geometry.gap) >= 24, 'turns are separated: ' + geometry.gap)
    pass('message area keeps a compact prompt chip, a bounded prose column and turn spacing')

    // P3: completed process defaults folded; expansion reveals the same live cards.
    const completedProcess = page.locator('.wb-activity').first()
    assert.equal(await completedProcess.getAttribute('open'), null)
    await completedProcess.locator(':scope > summary').click()
    await page.getByText('src/theme.rs', { exact: true }).first().waitFor()
    await page.getByText('cargo test -p work-ui --lib', { exact: true }).first().waitFor()
    pass('live WS events produce the read/bash tool cards')
    await page.getByText('bash', { exact: true }).first().click()
    await page.getByText(/12 passed; 0 failed/).waitFor()
    pass('expanding a live tool card shows the streamed result')

    // P4: send a prompt and clear the draft only after the node accepts it.
    const composer = page.getByRole('textbox', { name: '消息', exact: true })
    await composer.fill('把深色模式也应用到设置页')
    await page.getByRole('button', { name: '发送' }).click()
    await page.waitForFunction(() => document.querySelector('textarea')?.value === '')
    await page.waitForFunction(() => document.body.innerText.length > 0)
    assert(commands('prompt').length >= 1, 'prompt command reached the node: ' + JSON.stringify(commands('prompt')))
    assert(commands('prompt').some((frame) => frame.includes('把深色模式也应用到设置页')), 'prompt carries the draft text')
    pass('send posts one prompt through work-client and clears the draft')
    // Admission is not settlement. The fixture explicitly supplies the latter.
    fixture.emit({ type: 'agent_settled', outcome: 'completed', pendingResume: false })
    await page.getByRole('button', { name: '发送', exact: true }).waitFor()

    // Rejected prompts keep the draft and surface the node error.
    fixture.controls.failPrompt = true
    await composer.fill('这条会被拒绝')
    await page.getByRole('button', { name: '发送' }).click()
    await page.getByText(/Test fixture: .*rejected|节点拒绝了这次发送|发送失败/).waitFor()
    assert.equal(await composer.inputValue(), '这条会被拒绝')
    fixture.controls.failPrompt = false
    pass('a rejected prompt keeps the draft and shows the error')

    // IME: a composing keystroke must not send; the committed text must.
    const promptCount = () => commands('prompt').length
    const beforeCompose = promptCount()
    await composer.fill('候选词测试')
    await page.evaluate(() => {
      document.querySelector('textarea')?.dispatchEvent(new CompositionEvent('compositionstart', { bubbles: true }))
    })
    await page.evaluate(() => {
      document.querySelector('textarea')?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', metaKey: true, bubbles: true }))
    })
    await page.waitForTimeout(400)
    assert.equal(promptCount(), beforeCompose, 'no prompt while composing')
    await page.evaluate(() => {
      document.querySelector('textarea')?.dispatchEvent(new CompositionEvent('compositionend', { bubbles: true }))
    })
    await page.keyboard.press('Meta+Enter')
    await page.waitForFunction((previous) => true, beforeCompose)
    await page.waitForTimeout(600)
    assert(promptCount() > beforeCompose, 'prompt sent after compositionend')
    assert(commands('prompt').at(-1).includes('候选词测试'), 'committed text is what gets sent')
    await page.waitForFunction(() => document.querySelector('textarea')?.value === '')
    pass('IME composition blocks the shortcut, committed text sends')

    // Approval gate: a dialog event renders a card, and answering goes back
    // through `ui_dialog_response`.
    fixture.emit({ type: 'agent_start' })
    fixture.emit({
      type: 'ui_dialog_request',
      requestId: 'approve-1',
      toolCallId: 'shell-2',
      ui: {
        purpose: 'bash',
        question: '允许执行 cargo test 吗？',
        options: [{ label: '允许一次', value: 'allow-once' }, { label: '拒绝', value: 'reject' }],
      },
    })
    const approval = page.getByRole('group', { name: '待批准的请求' })
    await approval.waitFor()
    assert.match(await approval.innerText(), /允许执行 cargo test 吗/)
    await approval.getByRole('button', { name: '允许一次', exact: true }).click()
    const replies = () => frames.map(frame => JSON.parse(frame)).filter(frame => frame.type === 'execute' && frame.payload?.requestId === 'approve-1')
    for (let attempt = 0; attempt < 100 && replies().length === 0; attempt++) await page.waitForTimeout(20)
    assert(frames.some(frame => { const value = JSON.parse(frame); return value.type === 'reserve' && value.kind === 'dialogResponse' && value.sessionId === 'dark' }), 'the SDK reserves its existing typed dialog operation')
    const reply = replies()[0]?.payload
    assert.equal(reply?.requestId, 'approve-1')
    assert.deepEqual(reply?.answer?.values, ['allow-once'], 'send the observed option value, not its display label')
    assert.equal(await approval.count(), 1, 'ACK does not fabricate a closed request')
    fixture.emit({ type: 'ui_dialog_closed', requestId: 'approve-1' })
    fixture.emit({ type: 'agent_settled', outcome: 'completed', pendingResume: false })
    await approval.waitFor({ state: 'detached' })
    pass('approval uses the current observed option value and waits for authoritative closure')

    // P4: model and thinking switches read back the authoritative state.
    // Runtime diagnostics are now explicitly selected instead of the default browser.
    await page.getByRole('button', { name: '运行面板', exact: true }).click()
    const runPanel = page.getByTestId('run-panel')
    const runtimeRows = () => runPanel.innerText()
    await page.getByLabel('选择模型').selectOption({ label: 'OpenAI' })
    await page.waitForFunction(() => document.body.innerText.includes('openai'))
    assert.match(await runtimeRows(), /openai/, 'runtime state readback shows the new provider')
    pass('model switch is confirmed by an authoritative runtime-state readback')

    await page.getByLabel('选择思考等级').selectOption('low')
    await page.waitForFunction(() => /low/.test(document.querySelector('[data-testid=run-panel]')?.innerText ?? ''))
    pass('thinking-level switch is confirmed by the same readback')

    // P4: abort while the node is still working.
    fixture.controls.promptDelayMs = 1500
    await composer.fill('这条会被中止')
    await page.getByRole('button', { name: '发送' }).click()
    await page.getByRole('button', { name: '中止' }).click()
    await page.waitForFunction(() => true)
    await page.waitForTimeout(500)
    assert(commands('abort').length >= 1, 'abort command reached the node: ' + JSON.stringify(commands('abort')))
    fixture.controls.promptDelayMs = 0
    fixture.emit({ type: 'agent_settled', outcome: 'aborted', pendingResume: false })
    await page.getByRole('button', { name: '发送' }).waitFor()
    pass('abort reaches the node; only actual settlement restores send')

    // P4: queue listing, cancellation and drafts per scope.
    fixture.setQueue([{ entryId: 'q-1', queue: 'followUp', text: '排队的一条消息', status: 'queued' }], 'dark')
    await workspaceQueueRefresh(page)
    await page.getByText('排队的一条消息', { exact: true }).waitFor()
    pass('queue items come from the node and render in the run panel')

    await page.getByRole('button', { name: '取消排队 排队的一条消息' }).click()
    await page.waitForFunction(() => true)
    await page.waitForTimeout(600)
    assert(commands('cancel_queued').length >= 1, 'cancel_queued reached the node')
    await page.getByText('排队的一条消息', { exact: true }).waitFor({ state: 'detached' }).catch(() => {})
    pass('cancelling a queued prompt round-trips with the node')

    await composer.fill('这个草稿要跨会话保留')
    await page.getByRole('button', { name: '梳理 Agent 架构', exact: true }).click()
    await page.getByText('这是 梳理 Agent 架构 的独立历史。').waitFor()
    assert.equal(await composer.inputValue(), '', 'another session starts without the draft')
    await page.getByRole('button', { name: '为工作台添加深色模式', exact: true }).click()
    await page.getByText('帮我为工作台增加一个深色主题，并保持手机端可用。').waitFor()
    await page.waitForFunction(() => document.querySelector('textarea')?.value === '这个草稿要跨会话保留')
    pass('drafts are stored per origin/session and restored on switch')
    await composer.fill('')

    // P6: changed files and diffs come from the node.
    const panel = page.getByTestId('run-panel')
    await page.getByRole('button', { name: '文件面板' }).click()
    await panel.getByTestId('tab-changes').waitFor()
    await panel.getByText('src/theme.rs').first().waitFor()
    assert.match(await panel.innerText(), /文件\s+状态\s+暂存/, 'changes render in the ReUI data grid')
    pass('file panel lists the node worktree changes')

    await panel.getByText('src/theme.rs').first().click()
    await page.getByText(/let theme = "dark"/).waitFor()
    assert.match(await panel.innerText(), /let theme = "light"/)
    pass('diff for the selected change renders both sides')

    // P6b: staged toggle and the directory browser.
    await panel.getByTestId('staged-toggle').check()
    await page.waitForFunction(() => true)
    await page.waitForTimeout(600)
    assert(
      fixture.requests.some((request) => request.path.includes('/git/diff') && request.query.includes('staged=true')),
      'staged diff requested: ' + JSON.stringify(fixture.requests.filter((r) => r.path.includes('git/diff')).slice(-2)),
    )
    pass('staged toggle changes the diff request')
    await panel.getByTestId('staged-toggle').uncheck()

    await page.getByRole('button', { name: '运行面板' }).click()
    await page.screenshot({ path: '/tmp/reui-live.png' })
    await page.getByRole('button', { name: '断开节点' }).click()
    await page.getByTestId('sidebar-brand').waitFor()
    assert.match(await page.locator('header').innerText(), /未连接/)
    assert.equal(await page.locator('.wb-thread').count(), 0)
    assert.equal(await page.locator('.wb-project').count(), 0)
    assert.doesNotMatch(await page.locator('footer').innerText(), /32k|4\.8k|75%|38%|0\.0184/)
    pass('disconnect clears node facts and never falls back to samples or sample metrics')

    assert.deepEqual(errors, [], 'page errors: ' + errors.join(' | '))
    pass('no page errors')
    console.log('RESULT', passed.length, 'checks passed')
  } catch (error) {
    console.error('FAIL', error.message.split('\n')[0])
    try {
      console.error('panel now:', (await page.getByTestId('run-panel').innerText()).slice(0, 200).replace(/\n/g, ' | '))
      console.error('main now:', (await page.locator('main').innerText()).slice(0, 400).replace(/\n/g, ' | '))
    } catch {}
    console.error('page errors:', errors.slice(0, 3))
    await page.screenshot({ path: '/tmp/reui-live-fail.png' })
    process.exitCode = 1
  } finally {
    await browser.close()
    preview.kill()
    await fixture.close()
  }
})()
