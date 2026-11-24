// Scope race acceptance through the real Rust/WASM client and isolated HTTP mocks.
// No real provider, credential, installer, model or installed Agent is used.
const assert = require('node:assert/strict')
const path = require('node:path')
const net = require('node:net')
const { createHash } = require('node:crypto')
const { spawn } = require('node:child_process')
const { chromium } = require(require.resolve('playwright', { paths: [path.resolve(__dirname, '../../../tools')] }))
const { startFixture } = require('../../../tests/fixtures/ui-fixture.cjs')
const root = path.resolve(__dirname, '..')
const freePort = () => new Promise(resolve => { const server = net.createServer(); server.listen(0, '127.0.0.1', () => { const port = server.address().port; server.close(() => resolve(port)) }) })
const bounded = async (promise, label) => { let timer; try { return await Promise.race([promise, new Promise((_, reject) => { timer = setTimeout(() => reject(Error(label)), 10000) })]) } finally { clearTimeout(timer) } }
const catalog = id => ({ revision: id === 'current-provider' ? 'rev-current' : 'rev-old', providers: [{ id, definition: null, configured: false, configKeyPresent: false, storedCredential: 'apiKey', diagnostic: null }] })
const manifest = { schemaVersion: 1, id: 'scope-skill', name: 'scope-skill', version: '1.0.0', kind: 'skill', title: 'Scope fixture skill', description: 'Data-only scope fixture', author: 'fixture', origin: 'fixture', instructions: [] }
const sources = { 'SKILL.md': '# Scope fixture\nNo executable code.\n' }
const sha256 = createHash('sha256').update(JSON.stringify([manifest, sources])).digest('hex')
const bundle = { manifest, sources, sha256 }
const offer = { manifest, sha256, fileCount: 1, sizeBytes: Buffer.byteLength(sources['SKILL.md']), acquired: false }
;(async () => {
  const fixture = await startFixture(0, { nativeQueries: true, includeHistoryAssistant: true })
  const port = await freePort(), origin = `http://127.0.0.1:${port}`
  const preview = spawn(process.execPath, [path.join(root, 'node_modules/vite/bin/vite.js'), 'preview', '--host', '127.0.0.1', '--port', String(port), '--strictPort'], { cwd: root, env: { ...process.env, RPI_NODE_URL: fixture.url }, stdio: 'ignore' })
  let browser
  const failures = []
  try {
    let ready = false
    for (let i = 0; i < 120; i++) { try { if ((await fetch(origin)).ok) { ready = true; break } } catch {} await new Promise(resolve => setTimeout(resolve, 100)) }
    assert(ready, 'owned product preview started')
    browser = await chromium.launch()
    const check = async (name, run) => {
      const page = await browser.newPage({ viewport: { width: 1440, height: 940 } })
      page.setDefaultTimeout(10000)
      const errors = [], gates = []
      page.on('pageerror', error => errors.push(error.message))
      const rendered = () => page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))))
      const gate = () => {
        let enter, resume, finish, request
        const entered = new Promise(resolve => { enter = resolve })
        const blocked = new Promise(resolve => { resume = resolve })
        const finished = new Promise(resolve => { finish = resolve })
        const value = {
          hold: async (route, response) => { request = route.request(); enter(); try { await blocked; await route.fulfill(response) } finally { finish() } },
          entered: () => bounded(entered, 'HTTP gate was never entered'),
          release: resume,
          done: async () => { if (request) await bounded(finished, 'HTTP gate failed to finish') },
          flush: async () => {
            const delivered = page.waitForResponse(response => response.request() === request)
            resume()
            await bounded(finished, 'HTTP gate failed to finish')
            await (await delivered).finished()
            await rendered()
          },
        }
        gates.push(value)
        return value
      }
      const open = async () => { await page.goto(origin + '/?live=1'); await page.getByText('帮我为工作台增加一个深色主题，并保持手机端可用。', { exact: true }).waitFor() }
      const display = async name => { await page.getByRole('button', { name: '打开显示选项', exact: true }).click(); await page.locator('header').getByRole('button', { name, exact: true }).click() }
      const reconnect = async () => {
        const wasSettings = await page.getByRole('dialog', { name: '设置', exact: true }).count() > 0
        if (wasSettings) await page.getByRole('button', { name: '关闭设置', exact: true }).click()
        await page.getByRole('button', { name: '断开节点', exact: true }).click()
        await page.getByRole('textbox', { name: '节点地址', exact: true }).fill(origin)
        await page.getByRole('button', { name: '连接节点', exact: true }).click()
        await page.getByText('在线', { exact: true }).waitFor()
        await page.getByRole('button', { name: '为工作台添加深色模式', exact: true }).waitFor()
        if (wasSettings) await display('设置')
      }
      const resources = async () => { await display('资源中心'); await page.getByRole('button', { name: '审阅 Scope fixture skill', exact: true }).waitFor() }
      const review = async () => { await page.getByRole('button', { name: '审阅 Scope fixture skill', exact: true }).click(); await page.getByRole('checkbox', { name: '我已审阅来源', exact: true }).waitFor() }
      const confirm = async () => { await page.getByRole('checkbox', { name: '我已审阅来源', exact: true }).check(); await page.getByRole('button', { name: '确认安装', exact: true }).click() }
      try {
        await page.route(url => url.pathname === '/api/v1/providers', route => route.fulfill({ json: catalog('current-provider') }))
        await run({ page, gate, open, display, reconnect, resources, review, confirm })
        assert.deepEqual(errors, [], 'browser errors')
        console.log('PASS', name)
      } catch (error) { failures.push(name); console.error('FAIL', name, error.message) }
      finally { for (const value of gates) value.release(); for (const value of gates) await value.done(); await page.close() }
    }
    await check('late provider catalogue cannot replace facts after reconnecting to the same origin', async ({ page, gate, open, display, reconnect }) => {
      const old = gate()
      let calls = 0
      await page.route(url => url.pathname === '/api/v1/providers', route => ++calls === 1 ? old.hold(route, { json: catalog('old-provider') }) : route.fulfill({ json: catalog('current-provider') }))
      await open()
      await display('设置')
      await old.entered()
      await reconnect()
      await page.getByTestId('refresh-providers').click()
      await page.getByText('current-provider', { exact: true }).waitFor()
      await old.flush()
      const text = await page.locator('main').innerText()
      assert.match(text, /rev-current/)
      assert.doesNotMatch(text, /old-provider/)
    })
    await check('late provider write receipt cannot replace the new catalogue, notice or edited key', async ({ page, gate, open, display, reconnect }) => {
      const old = gate()
      let fresh = false, writes = 0
      await page.route(url => url.pathname === '/api/v1/providers', async route => {
        if (route.request().method() === 'POST') { writes++; return old.hold(route, { json: { requiresRestart: true, catalog: catalog('stale-receipt-provider') } }) }
        await route.fulfill({ json: catalog(fresh ? 'current-provider' : 'old-provider') })
      })
      await open()
      await display('设置')
      await page.getByRole('button', { name: '设置 old-provider 的密钥', exact: true }).click()
      await page.getByLabel('old-provider 的 API 密钥', { exact: true }).fill('fixture-old-key')
      await page.getByRole('button', { name: '保存 old-provider 的密钥', exact: true }).click()
      await old.entered()
      fresh = true
      await reconnect()
      await page.getByTestId('refresh-providers').click()
      await page.getByRole('button', { name: '设置 current-provider 的密钥', exact: true }).click()
      await page.getByLabel('current-provider 的 API 密钥', { exact: true }).fill('fixture-new-unsaved-key')
      await old.flush()
      assert.equal(await page.getByLabel('current-provider 的 API 密钥', { exact: true }).inputValue(), 'fixture-new-unsaved-key')
      assert.doesNotMatch(await page.locator('main').innerText(), /stale-receipt-provider|已保存/)
      assert.equal(writes, 1, 'the new draft was not submitted')
    })
    for (const rejected of [false, true]) await check(`late QR ${rejected ? 'error' : 'ticket'} cannot replace the new authorization after same-origin reconnect`, async ({ page, gate, open, display, reconnect }) => {
      const old = gate()
      let calls = 0
      await page.route(url => url.pathname === '/api/v1/auth/qr', route => ++calls === 1
        ? old.hold(route, rejected ? { status: 403, json: { error: 'old-qr-failure' } } : { json: { ticket: 'old-scope-ticket', expiresAt: Date.now() + 300000, nodeId: 'fixture' } })
        : route.fulfill({ json: { ticket: 'current-scope-ticket', expiresAt: Date.now() + 300000, nodeId: 'fixture' } }))
      await open()
      await display('设置')
      await page.getByRole('dialog', { name: '设置', exact: true }).getByRole('button', { name: '工作协同', exact: true }).click()
      await page.getByRole('button', { name: '生成一次性授权码', exact: true }).click()
      await old.entered()
      await reconnect()
      await page.getByRole('dialog', { name: '设置', exact: true }).getByRole('button', { name: '工作协同', exact: true }).click()
      await page.getByRole('button', { name: '生成一次性授权码', exact: true }).click()
      await page.getByAltText('一次性授权二维码').waitFor()
      const payload = await page.locator('.wb-settings-dialog code:visible').first().innerText()
      await old.flush()
      assert.equal(await page.locator('.wb-settings-dialog code:visible').first().innerText(), payload)
      assert.doesNotMatch(await page.locator('main').innerText(), /node access denied|old-scope-ticket/)
      assert.equal(calls, 2)
    })
    const mockResources = async page => {
      await page.route(url => url.pathname.endsWith('/native-resources'), route => route.fulfill({ json: { profile: {}, resources: [], diagnostics: [] } }))
      await page.route(url => url.pathname.endsWith('/packages'), route => route.fulfill({ json: { source: 'scope-fixture', packages: [offer], diagnostics: [] } }))
      await page.route(url => url.pathname.endsWith('/packages/bundle'), route => route.fulfill({ json: bundle }))
      await page.route(url => url.pathname.endsWith('/packages/acquire'), route => {
        assert.deepEqual(route.request().postDataJSON(), { id: manifest.id, version: manifest.version, sha256 })
        return route.fulfill({ json: { package: route.request().postDataJSON(), source: 'scope-fixture' } })
      })
    }
    await check('a delayed bundle cannot resurrect a cancelled newer review after session A to B to A', async ({ page, gate, open, resources, review }) => {
      await mockResources(page)
      const old = gate()
      let calls = 0
      await page.route(url => url.pathname.endsWith('/packages/bundle'), route => ++calls === 1 ? old.hold(route, { json: bundle }) : route.fulfill({ json: bundle }))
      await open()
      await resources()
      await page.getByRole('button', { name: '审阅 Scope fixture skill', exact: true }).click()
      await old.entered()
      await page.getByRole('button', { name: '梳理 Agent 架构', exact: true }).click()
      await page.getByText('这是 梳理 Agent 架构 的独立历史。', { exact: true }).waitFor()
      await page.getByRole('button', { name: '为工作台添加深色模式', exact: true }).click()
      await page.getByText('帮我为工作台增加一个深色主题，并保持手机端可用。', { exact: true }).waitFor()
      await resources()
      await review()
      await page.getByRole('button', { name: '取消', exact: true }).click()
      await old.flush()
      assert.equal(await page.getByRole('checkbox', { name: '我已审阅来源', exact: true }).count(), 0)
      assert.equal(calls, 2)
    })
    await check('an old install finally cannot unlock or strand a newer scoped install', async ({ page, gate, open, resources, review, confirm }) => {
      await mockResources(page)
      const old = gate(), current = gate()
      const writes = []
      await page.route(url => url.pathname.endsWith('/packages/install'), route => {
        writes.push({ path: new URL(route.request().url()).pathname, body: route.request().postDataJSON() })
        return writes.length === 1 ? old.hold(route, { status: 403, json: { error: 'old-install-failure' } }) : current.hold(route, { status: 400, json: { error: 'current-install-failure' } })
      })
      await open()
      await resources()
      await review()
      await confirm()
      await old.entered()
      await page.getByRole('button', { name: '梳理 Agent 架构', exact: true }).click()
      await page.getByText('这是 梳理 Agent 架构 的独立历史。', { exact: true }).waitFor()
      await resources()
      await review()
      await confirm()
      await current.entered()
      await old.flush()
      assert.equal(await page.getByRole('button', { name: '安装中…', exact: true }).isDisabled(), true)
      assert.doesNotMatch(await page.locator('main').innerText(), /node access denied|安装失败/)
      await current.flush()
      await page.waitForFunction(() => [...document.querySelectorAll('button')].some(button => button.textContent.trim() === '确认安装' && !button.disabled))
      assert.match(await page.locator('main').innerText(), /invalid node request/)
      assert.equal(writes.length, 2)
      assert.equal(writes[0].path, writes[1].path, 'same project still has distinct session/request lifetimes')
      for (const write of writes) assert.deepEqual(write.body, { package: { id: manifest.id, version: manifest.version, sha256 }, scope: 'project', trusted: true })
    })
    assert.deepEqual(failures, [], 'scope race regressions remain')
  } finally {
    if (browser) await browser.close()
    if (preview.exitCode === null && preview.signalCode === null) await new Promise(resolve => { preview.once('exit', resolve); preview.kill('SIGTERM') })
    await fixture.close()
  }
})().catch(error => { console.error(error); process.exitCode = 1 })
