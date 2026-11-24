// Product startup must never fall back to sample data. All traffic is isolated.
const assert = require('node:assert/strict')
const fs = require('node:fs')
const path = require('node:path')
const net = require('node:net')
const { spawn } = require('node:child_process')
const { chromium } = require(require.resolve('playwright', { paths: [path.resolve(__dirname, '../../../tools')] }))
const { startFixture } = require('../../../tests/fixtures/ui-fixture.cjs')
const root = path.resolve(__dirname, '..')
const freePort = () => new Promise(resolve => { const server = net.createServer(); server.listen(0, '127.0.0.1', () => { const port = server.address().port; server.close(() => resolve(port)) }) })
;(async () => {
  const children = []
  const gates = []
  let browser, protocolFixture
  const start = async (mode = 'production', target = 'http://127.0.0.1:9') => {
    const port = await freePort()
    const child = spawn(process.execPath, [path.join(root, 'node_modules/vite/bin/vite.js'), 'preview', '--mode', mode, '--host', '127.0.0.1', '--port', String(port), '--strictPort'], { cwd: root, env: { ...process.env, RPI_NODE_URL: target }, stdio: 'ignore' })
    children.push(child)
    const origin = `http://127.0.0.1:${port}`
    for (let i = 0; i < 120; i++) {
      try { if ((await fetch(origin)).ok) return origin } catch {}
      await new Promise(resolve => setTimeout(resolve, 100))
    }
    throw Error('Owned preview did not start')
  }
  try {
    browser = await chromium.launch()
    const errors = []
    const page = await browser.newPage({ viewport: { width: 1440, height: 940 } })
    page.on('pageerror', error => errors.push(error.message))
    await page.route('**/.well-known/rpi-work-entry', route => route.fulfill({ status: 404 }))
    await page.route('**/api/**', route => route.fulfill({ status: 503, json: { error: 'isolated startup check' } }))
    const product = await start()
    for (const query of ['', '?fixture=1']) {
      await page.goto(product + '/' + query)
      await page.waitForFunction(() => { const title = document.querySelector('.wb-window-title'); return title && !title.innerText.includes('加载中') })
      assert.equal(await page.getByRole('button', { name: '为工作台添加深色模式', exact: true }).count(), 0, 'Product startup cannot expose sample sessions, even with fixture=1')
      assert.equal(await page.getByText('主题切换已经准备好').count(), 0)
      await page.locator('textarea').fill('未连接时保留的草稿')
      assert.equal(await page.getByRole('button', { name: '发送', exact: true }).isDisabled(), true)
      assert.doesNotMatch(await page.locator('footer').innerText(), /32k|4\.8k|75%|38%|0\.0184/)
    }
    console.log('PASS product default and fixture query remain disconnected, empty, non-writing and free of sample metrics')
    const assets = path.join(root, 'dist/assets')
    const javascript = fs.readdirSync(assets).filter(name => name.endsWith('.js')).map(name => fs.readFileSync(path.join(assets, name), 'utf8')).join('\n')
    assert.doesNotMatch(javascript, /主题切换已经准备好|为工作台添加深色模式|读取主题契约/)
    console.log('PASS product JavaScript contains no workspace sample payload')
    const fixture = await start('fixture')
    await page.goto(fixture + '/')
    await page.waitForFunction(() => { const title = document.querySelector('.wb-window-title'); return title && !title.innerText.includes('加载中') })
    assert.equal(await page.getByRole('button', { name: '为工作台添加深色模式', exact: true }).count(), 0)
    await page.goto(fixture + '/?fixture=1')
    await page.getByText('主题切换已经准备好').waitFor()
    assert.match(await page.locator('.wb-window-title').innerText(), /示例/)
    assert.deepEqual(errors, [])
    console.log('PASS separate fixture build requires explicit activation and labels its sample data')

    protocolFixture = await startFixture(0, { authRequired: true, nativeQueries: true, includeHistoryAssistant: true })
    protocolFixture.controls.runtimeContext = null
    protocolFixture.controls.runtimeStats = null
    const connected = await start('production', protocolFixture.url)
    const liveContext = await browser.newContext({ viewport: { width: 1440, height: 940 } })
    const live = await liveContext.newPage()
    live.on('pageerror', error => errors.push(error.message))
    await live.goto(connected + '/?live=1')
    await live.getByText('节点需要密码', { exact: true }).waitFor()
    await live.locator('textarea').fill('登录前自己的草稿')
    await live.getByLabel('节点密码', { exact: true }).fill('incorrect-fixture-value')
    await live.getByRole('button', { name: '登录', exact: true }).click()
    await live.getByRole('alert').waitFor()
    assert.equal(await live.locator('.wb-project, .wb-thread').count(), 0)
    assert.equal(await live.getByRole('button', { name: '发送', exact: true }).isDisabled(), true)
    assert.equal(await live.locator('textarea').inputValue(), '登录前自己的草稿')
    console.log('PASS failed authentication displays its error, retains the draft and exposes no workspace or writable fallback')
    await live.getByLabel('节点密码', { exact: true }).fill('test-only')
    await live.getByRole('button', { name: '登录', exact: true }).click()
    await live.getByText('在线', { exact: true }).waitFor()
    await live.getByRole('button', { name: '运行面板', exact: true }).click()
    await live.getByText('deepseek/deepseek-v4-pro', { exact: true }).waitFor()
    assert.doesNotMatch(await live.locator('footer').innerText(), /32k|4\.8k|75%|38%|0\.0184/)
    assert.equal(await live.getByText('读取主题契约', { exact: true }).count(), 0)
    console.log('PASS authoritative model plus missing runtime metrics/context never falls back to sample values or run steps')
    let enteredEngine, releaseEngine
    const engineEntered = new Promise(resolve => { enteredEngine = resolve })
    const engineHeld = new Promise(resolve => { releaseEngine = resolve }); gates.push(releaseEngine)
    const delayed = await live.context().newPage()
    delayed.setDefaultTimeout(7000)
    delayed.on('pageerror', error => errors.push(error.message))
    await delayed.route('**/*work_viewmodel*.wasm', async route => { enteredEngine(); await engineHeld; await route.continue() })
    await delayed.goto(connected + '/?live=1', { waitUntil: 'domcontentloaded' })
    await engineEntered
    await delayed.getByRole('button', { name: '为工作台添加深色模式', exact: true }).waitFor()
    assert.equal(await delayed.getByRole('button', { name: '发送', exact: true }).isDisabled(), true)
    releaseEngine()
    await delayed.getByText('帮我为工作台增加一个深色主题，并保持手机端可用。', { exact: true }).waitFor()
    await delayed.close()
    console.log('PASS fast authenticated node plus delayed presentation WASM cannot leave canonical history invisible')
    await live.evaluate(() => {
      window.entryOriginalSend = WebSocket.prototype.send
      WebSocket.prototype.send = function (data) {
        if (!window.entryHeldOnce && typeof data === 'string' && data.includes('"prompt"')) {
          window.entryHeldOnce = true
          window.entryReleasePrompt = () => window.entryOriginalSend.call(this, data)
          return
        }
        return window.entryOriginalSend.call(this, data)
      }
    })
    const submitted = '迟到ACK不能擦掉同文新稿'
    await live.locator('textarea').fill(submitted)
    await live.locator('textarea').press('Meta+Enter')
    await live.waitForFunction(() => typeof window.entryReleasePrompt === 'function')
    await live.locator('textarea').fill('先改成别的草稿')
    await live.locator('textarea').fill(submitted)
    await live.evaluate(() => { window.entryReleasePrompt(); WebSocket.prototype.send = window.entryOriginalSend })
    // A draft ACK is not idle; supply an observed terminal event, including the
    // valid ordering in which settlement arrives before the delayed ACK.
    protocolFixture.emit({ type: 'agent_settled', outcome: 'completed', pendingResume: false })
    await live.getByRole('button', { name: '发送', exact: true }).waitFor()
    assert.equal(await live.locator('textarea').inputValue(), submitted, 'A later edit remains a new draft even when its text equals the submitted text')
    console.log('PASS a delayed synthetic prompt ACK cannot clear a newer same-scope edit, including A→B→A')
    await live.locator('textarea').fill('真实节点自己的草稿')
    await live.getByRole('button', { name: '断开节点', exact: true }).click()
    await live.getByText('未连接节点', { exact: true }).waitFor()
    assert.equal(await live.locator('.wb-project, .wb-thread').count(), 0)
    assert.equal(await live.locator('textarea').inputValue(), '登录前自己的草稿')
    await live.getByRole('button', { name: '连接节点', exact: true }).click()
    await live.getByText('在线', { exact: true }).waitFor()
    assert.equal(await live.locator('textarea').inputValue(), '真实节点自己的草稿')
    assert.deepEqual(errors, [])
    console.log('PASS same-origin reconnect restores only that node/session draft, keeping disconnected drafts separate')
  } finally {
    for (const release of gates) release()
    await browser?.close()
    await protocolFixture?.close()
    for (const child of children) if (child.exitCode === null && child.signalCode === null) await new Promise(resolve => { child.once('exit', resolve); child.kill('SIGTERM') })
  }
})().catch(error => { console.error(error); process.exitCode = 1 })
