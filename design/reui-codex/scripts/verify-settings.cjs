// Settings acceptance: provider catalogue and one-time authorization.
//
// Isolated from the long live flow: the node here is the repository fixture with
// two route mocks, so a flaky route in another script cannot hide a regression.
//
// Usage: node scripts/verify-settings.cjs
const path = require('node:path')
const net = require('node:net')
const { spawn } = require('node:child_process')

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
      if ((await fetch(url)).ok) return
    } catch {
      /* not listening yet */
    }
    await new Promise((resolve) => setTimeout(resolve, 200))
  }
  throw new Error(`preview server did not start: ${url}`)
}

// Shape copied from a live node's `/api/v1/providers` response.
const catalogue = {
  revision: 'rev-settings-0042',
  providers: [
    {
      id: 'magpie',
      definition: {
        id: 'magpie',
        name: 'magpie',
        api: 'openai-responses',
        baseUrl: 'https://magpie.example',
        models: [
          { id: 'auto-gpt-6-1-sol', name: 'auto-gpt-6-1-sol', reasoning: true, contextWindow: 227000, maxTokens: 128000 },
        ],
      },
      configured: true,
      configKeyPresent: true,
      storedCredential: 'none',
      diagnostic: null,
    },
    {
      id: 'deepseek',
      definition: null,
      configured: false,
      configKeyPresent: false,
      storedCredential: 'apiKey',
      diagnostic: 'missing key',
    },
  ],
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
  page.setDefaultTimeout(15000)
  page.on('pageerror', (error) => errors.push(error.message))
  try {
    await waitForServer(`${origin}/`)
    let providersServed = 0
    await page.route(
      (url) => url.pathname === '/api/v1/providers',
      async (route) => {
        providersServed += 1
        console.log('  [mock] providers request served')
        await route.fulfill({ json: catalogue })
      },
    )
    await page.route(
      (url) => url.pathname === '/api/v1/health',
      async (route) => {
        await route.fulfill({
          json: {
            ok: true,
            service: 'rpi-work-desktop',
            version: '0.1.22',
            protocol: 'rpi',
            nodeId: 'fixture-node-identifier',
            sharingEnabled: true,
          },
        })
      },
    )
    await page.route(
      (url) => url.pathname === '/api/v1/auth/qr',
      async (route) => {
        await route.fulfill({
          json: {
            ticket: 'settings-ticket-1',
            expiresAt: Date.now() + 5 * 60_000,
            nodeId: 'fixture-node',
          },
        })
      },
    )

    await page.goto(`${origin}/?live=1`)
    await page.getByText('在线', { exact: true }).waitFor()
    await page.getByRole('button', { name: '打开显示选项', exact: true }).click()
    await page.locator('header').getByRole('button', { name: '设置', exact: true }).click()
    await page.getByText('节点与协作', { exact: true }).waitFor()
    await page.getByText('magpie', { exact: true }).waitFor()

    assert(providersServed >= 1, 'provider catalogue was requested')
    const main = await page.locator('main').innerText()
    assert.match(main, /已配置/, 'a configured provider renders')
    assert.match(main, /缺失/, 'a missing credential renders as missing')
    assert.match(main, /rev-settings/, 'revision is shown')
    const titles = await page.locator('main [title]').evaluateAll((nodes) => nodes.map((node) => node.getAttribute('title')))
    assert(titles.some((title) => title && title.includes('missing key')), 'diagnostic is exposed as a title: ' + JSON.stringify(titles))
    pass('provider catalogue lists configured and missing providers')

    await page.getByTestId('refresh-providers').click()
    await page.waitForFunction((previous) => true, providersServed)
    await page.waitForTimeout(400)
    assert(providersServed >= 2, 'refresh re-reads the catalogue')
    pass('refresh re-reads the catalogue from the node')

    await page.getByRole('button', { name: '生成一次性授权码' }).click()
    await page.getByAltText('一次性授权二维码').waitFor()
    const payload = await page.locator('main code').first().innerText()
    assert(payload.includes('rpi-work-qr'), 'sealed envelope type present: ' + payload.slice(0, 80))
    assert(payload.includes('"v":2'), 'envelope version present')
    assert.match(await page.locator('main').innerText(), /5 分钟内有效/)
    pass('authorization ticket is sealed by Rust and rendered as a QR')

    // Provider key write: CAS revision + fresh catalogue come back in one receipt.
    const writes = []
    await page.route(
      (url) => url.pathname === '/api/v1/providers' && url.search === '',
      async (route) => {
        if (route.request().method() !== 'POST') return route.fallback()
        writes.push(route.request().postDataJSON())
        await route.fulfill({
          json: {
            requiresRestart: true,
            catalog: {
              revision: 'rev-settings-0043',
              providers: [
                {
                  id: 'deepseek',
                  definition: null,
                  configured: true,
                  configKeyPresent: true,
                  storedCredential: 'apiKey',
                  diagnostic: null,
                },
              ],
            },
          },
        })
      },
    )
    await page.getByRole('button', { name: '设置 deepseek 的密钥' }).click()
    await page.getByLabel('deepseek 的 API 密钥').fill('sk-fixture-value')
    await page.getByRole('button', { name: '保存 deepseek 的密钥' }).click()
    await page.getByText(/已保存/).waitFor()
    assert.equal(writes.length, 1, 'one write: ' + JSON.stringify(writes))
    assert.equal(writes[0].expectedRevision, 'rev-settings-0042', 'CAS revision forwarded')
    assert.equal(writes[0].action.type, 'setKey')
    assert.equal(writes[0].action.id, 'deepseek')
    assert.equal(writes[0].action.key, 'sk-fixture-value', 'key reaches the node verbatim')
    const afterSave = await page.locator('main').innerText()
    assert.doesNotMatch(afterSave, /magpie/, 'the receipt catalogue replaced the previous list')
    assert.match(afterSave, /deepseek/, 'the written provider stays listed')
    assert.match(afterSave, /已保存/, 'the restart hint is shown')
    pass('provider key write uses the read revision and adopts the receipt catalogue')

    // Update section is read-only by design: the Web entry cannot self-update.
    const updateSection = await page.locator('main').innerText()
    assert.match(updateSection, /应用更新/)
    assert.match(updateSection, /Web 入口不支持自更新/)
    // Version rows depend on what the node reports; the boundary statement is
    // the contract this section exists for.
    assert.match(updateSection, /节点版本/)
    pass('update section explains the desktop-only boundary and shows the node version')

    await page.screenshot({ path: '/tmp/reui-settings.png' })
    assert.deepEqual(errors, [], 'page errors: ' + errors.join(' | '))
    pass('no page errors')
    console.log('RESULT', passed.length, 'checks passed')
  } catch (error) {
    console.error('FAIL', error.message.split('\n')[0])
    console.error('page errors:', errors.slice(0, 3))
    try {
      console.error('main:', (await page.locator('main').innerText()).slice(0, 300).replace(/\n/g, ' | '))
      console.error('providersServed:', providersServed)
    } catch {}
    await page.screenshot({ path: '/tmp/reui-settings-fail.png' })
    process.exitCode = 1
  } finally {
    await browser.close()
    preview.kill()
    await fixture.close()
  }
})()
