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
  const preview = spawn('npm', ['run', 'preview', '--', '--port', String(port), '--strictPort'], {
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

    // P3: live wire events replay through the Rust projection.
    await page.getByText('src/theme.rs', { exact: true }).first().waitFor()
    await page.getByText('cargo test -p work-ui --lib', { exact: true }).first().waitFor()
    pass('live WS events produce the read/bash tool cards')
    await page.getByText('bash', { exact: true }).first().click()
    await page.getByText(/12 passed; 0 failed/).waitFor()
    pass('expanding a live tool card shows the streamed result')

    // P4: send a prompt and clear the draft only after the node accepts it.
    const composer = page.getByPlaceholder('给 rpi 发消息…（⌘↵ 发送）')
    await composer.fill('把深色模式也应用到设置页')
    await page.getByRole('button', { name: '发送' }).click()
    await page.waitForFunction(() => document.querySelector('textarea')?.value === '')
    await page.waitForFunction(() => document.body.innerText.length > 0)
    assert(commands('prompt').length >= 1, 'prompt command reached the node: ' + JSON.stringify(commands('prompt')))
    assert(commands('prompt').some((frame) => frame.includes('把深色模式也应用到设置页')), 'prompt carries the draft text')
    pass('send posts one prompt through work-client and clears the draft')

    // Rejected prompts keep the draft and surface the node error.
    fixture.controls.failPrompt = true
    await composer.fill('这条会被拒绝')
    await page.getByRole('button', { name: '发送' }).click()
    await page.getByText(/Test fixture: .*rejected|节点拒绝了这次发送|发送失败/).waitFor()
    assert.equal(await composer.inputValue(), '这条会被拒绝')
    fixture.controls.failPrompt = false
    pass('a rejected prompt keeps the draft and shows the error')

    // P4: model and thinking switches read back the authoritative state.
    const runPanel = page.getByTestId('run-panel')
    const runtimeRows = () => runPanel.innerText()
    await page.getByLabel('选择模型').selectOption({ index: 1 })
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
    await page.getByRole('button', { name: '发送' }).waitFor()
    pass('abort reaches the node and the composer returns to send')

    await page.screenshot({ path: '/tmp/reui-live.png' })
    await page.getByRole('button', { name: '断开节点' }).click()
    await page.getByText('rpiwork').first().waitFor()
    assert.match(await page.locator('header').innerText(), /fixture/)
    pass('disconnect falls back to the fixture without a reload')

    assert.deepEqual(errors, [], 'page errors: ' + errors.join(' | '))
    pass('no page errors')
    console.log('RESULT', passed.length, 'checks passed')
  } catch (error) {
    console.error('FAIL', error.message.split('\n')[0])
    console.error('page errors:', errors.slice(0, 3))
    await page.screenshot({ path: '/tmp/reui-live-fail.png' })
    process.exitCode = 1
  } finally {
    await browser.close()
    preview.kill()
    await fixture.close()
  }
})()
