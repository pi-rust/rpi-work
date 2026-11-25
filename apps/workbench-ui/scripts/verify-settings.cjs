// Settings acceptance: provider catalogue and one-time authorization.
//
// Isolated from the long live flow: the node here is the repository fixture with
// two route mocks, so a flaky route in another script cannot hide a regression.
//
// Usage: node scripts/verify-settings.cjs
const path = require('node:path')
const net = require('node:net')
const http = require('node:http')
const fs = require('node:fs/promises')
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
  const preview = spawn(process.execPath, [path.join(appRoot, 'node_modules/vite/bin/vite.js'), 'preview', '--port', String(port), '--strictPort'], {
    cwd: appRoot,
    env: { ...process.env, RPI_NODE_URL: fixture.url },
    stdio: 'ignore',
  })
  const origin = `http://127.0.0.1:${port}`
  const errors = []
  let previewLoads = 0, providersServed = 0
  const site = http.createServer((req, res) => {
    if (req.url === '/') previewLoads++
    res.setHeader('Content-Type', 'text/html')
    res.end('<!doctype html><html><body><label>Settings preview<input aria-label="Settings preview"></label></body></html>')
  })
  const artifacts = path.resolve(appRoot, '../../artifacts/settings')
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
    await fs.mkdir(artifacts, { recursive: true })
    await new Promise(resolve => site.listen(0, '127.0.0.1', resolve))
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
    await page.getByRole('textbox', { name: '预览地址', exact: true }).fill(`http://127.0.0.1:${site.address().port}/`)
    await page.getByRole('button', { name: '打开', exact: true }).click()
    const previewDraft = page.frameLocator('.real-browser iframe').getByLabel('Settings preview')
    await previewDraft.fill('do not reload while settings are open')
    const loads = previewLoads
    await page.locator('textarea').fill('设置窗口保留聊天草稿与选区')
    await page.locator('textarea').evaluate(node => node.setSelectionRange(2, 7))
    await page.evaluate(() => { window.settingsNodes = { transcript: document.querySelector('.wb-transcript'), composer: document.querySelector('textarea'), frame: document.querySelector('.real-browser iframe') } })
    await page.getByRole('button', { name: '打开显示选项', exact: true }).click()
    await page.locator('header').getByRole('button', { name: '设置', exact: true }).click()
    const dialog = page.getByRole('dialog', { name: '设置', exact: true })
    await dialog.waitFor()
    assert.equal(await dialog.evaluate(node => node.matches(':modal')), true)
    const box = await dialog.boundingBox()
    assert(Math.abs(box.width - 1200) < 2 && Math.abs(box.height - 765) < 2, JSON.stringify(box))
    assert(Math.abs(box.x - (1440 - box.width) / 2) < 2 && Math.abs(box.y - (900 - box.height) / 2) < 2, JSON.stringify(box))
    assert.equal(await page.getByRole('searchbox', { name: '搜索设置' }).evaluate(node => node === document.activeElement), true)
    pass('confirmed desktop dimensions are centered in a native modal with named initial focus')
    await page.getByText('magpie', { exact: true }).waitFor()

    assert(providersServed >= 1, 'provider catalogue was requested')
    const main = await page.locator('main').innerText()
    assert.match(main, /已配置/, 'a configured provider renders')
    assert.match(main, /缺失/, 'a missing credential renders as missing')
    assert.match(main, /rev-settings/, 'revision is shown')
    const titles = await page.locator('main [title]').evaluateAll((nodes) => nodes.map((node) => node.getAttribute('title')))
    assert(titles.some((title) => title && title.includes('missing key')), 'diagnostic is exposed as a title: ' + JSON.stringify(titles))
    pass('provider catalogue lists configured and missing providers')

    const refreshed = page.waitForResponse(response => new URL(response.url()).pathname === '/api/v1/providers' && response.request().method() === 'GET')
    await page.getByTestId('refresh-providers').click()
    await (await refreshed).finished()
    assert(providersServed >= 2, 'refresh re-reads the catalogue')
    pass('refresh re-reads the catalogue from the node')

    await dialog.getByRole('button', { name: '工作协同', exact: true }).click()
    await page.getByRole('button', { name: '生成一次性授权码' }).click()
    await page.getByAltText('一次性授权二维码').waitFor()
    const payload = await dialog.locator('code:visible').first().innerText()
    assert(payload.includes('rpi-work-qr'), 'sealed envelope type present: ' + payload.slice(0, 80))
    assert(payload.includes('"v":2'), 'envelope version present')
    assert.match(await page.locator('main').innerText(), /5 分钟内有效/)
    pass('authorization ticket is sealed by Rust and rendered as a QR')
    await dialog.getByRole('button', { name: '提供商', exact: true }).click()

    // Provider key write: CAS revision + fresh catalogue come back in one receipt.
    const writes = []
    let requiresRestart = true
    await page.route(
      (url) => url.pathname === '/api/v1/providers' && url.search === '',
      async (route) => {
        if (route.request().method() !== 'POST') return route.fallback()
        writes.push(route.request().postDataJSON())
        await route.fulfill({
          json: {
            requiresRestart,
            catalog: {
              revision: requiresRestart ? 'rev-settings-0043' : 'rev-settings-0044',
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
    assert.match(afterSave, /节点报告需要重启/)
    assert.match(afterSave, /加载状态仍以会话观察为准/)
    requiresRestart = false
    await page.getByRole('button', { name: '设置 deepseek 的密钥' }).click()
    await page.getByLabel('deepseek 的 API 密钥').fill('sk-fixture-replacement')
    await page.getByRole('button', { name: '保存 deepseek 的密钥' }).click()
    await page.getByText('已保存；节点未要求重启，不代表当前 Agent 已加载。', { exact: true }).waitFor()
    assert.equal(writes.length, 2)
    assert.equal(writes[1].expectedRevision, 'rev-settings-0043')
    assert.doesNotMatch(await page.locator('main').innerText(), /已保存并生效/)
    pass('neither restart nor no-restart management receipts claim the current Agent loaded credentials')

    // Update section is read-only by design: the Web entry cannot self-update.
    await dialog.getByRole('button', { name: '关于', exact: true }).click()
    const updateSection = await page.locator('main').innerText()
    assert.match(updateSection, /应用更新/)
    assert.match(updateSection, /Web 入口不支持自更新/)
    // Version rows depend on what the node reports; the boundary statement is
    // the contract this section exists for.
    assert.match(updateSection, /节点版本/)
    pass('update section explains the desktop-only boundary and shows the node version')

    await page.screenshot({ path: path.join(artifacts, 'desktop-light.png') })
    const search = dialog.getByRole('searchbox', { name: '搜索设置', exact: true })
    await search.fill('密钥')
    assert.equal(await dialog.getByRole('button', { name: '提供商', exact: true }).count(), 1)
    assert.equal(await dialog.getByRole('button', { name: '工作协同', exact: true }).count(), 0)
    await search.fill('no-such-setting')
    await dialog.getByText('没有匹配的设置', { exact: true }).waitFor()
    await search.fill('')
    await dialog.getByRole('button', { name: '提供商', exact: true }).click()
    await page.getByRole('button', { name: '设置 deepseek 的密钥' }).click()
    await page.getByLabel('deepseek 的 API 密钥').fill('memory-only-unsaved-settings-key')
    const saveKey = page.getByRole('button', { name: '保存 deepseek 的密钥', exact: true })
    await saveKey.hover()
    await saveKey.evaluate(node => Promise.all(node.getAnimations().map(animation => animation.finished.catch(() => {}))))
    assert.equal(await saveKey.evaluate(node => getComputedStyle(node).backgroundColor === getComputedStyle(document.querySelector('.wb-settings-nav button[aria-current="page"]')).backgroundColor), false, 'navigation hover must not override the primary save button colors')
    await dialog.getByRole('button', { name: '关于', exact: true }).click()
    await dialog.getByRole('button', { name: '提供商', exact: true }).click()
    assert.equal(await page.getByLabel('deepseek 的 API 密钥').inputValue(), 'memory-only-unsaved-settings-key')
    assert.equal(writes.length, 2, 'search and category selection do not write credentials')
    pass('search filters real destinations and category changes preserve an unsent key without writing it')

    await search.focus()
    for (let index = 0; index < 24; index++) {
      await page.keyboard.press('Tab')
      assert.equal(await dialog.evaluate(node => node.contains(document.activeElement)), true, `focus escaped at Tab ${index}`)
    }
    await page.getByRole('button', { name: '关闭设置', exact: true }).focus()
    await page.keyboard.press('Shift+Tab')
    assert.equal(await page.getByRole('button', { name: '取消', exact: true }).evaluate(node => node === document.activeElement), true, 'reverse Tab wraps to the last visible control')
    await page.evaluate(() => document.querySelector('textarea').focus())
    assert.equal(await dialog.evaluate(node => node.contains(document.activeElement)), true, 'background cannot receive focus')
    await page.keyboard.press('Meta+k')
    assert.equal(await page.locator('[role="dialog"][aria-label="搜索项目与会话"]').count(), 0, 'background shortcut cannot open a second dialog')
    await page.mouse.click(2, 2)
    assert.equal(await dialog.evaluate(node => node.matches(':modal')), true, 'backdrop does not discard an edited key')
    pass('native modal contains keyboard focus, blocks background shortcuts and ignores backdrop clicks')

    const projectRows = page.locator('#wb-projects .wb-project')
    const nodeToggle = page.locator('.wb-node-toggle')
    const projectLayout = dialog.getByRole('combobox', { name: '项目列表显示', exact: true })
    await page.keyboard.press('Escape')
    await nodeToggle.waitFor()
    const selected = await page.locator('.wb-thread[aria-current="true"]').textContent()
    await page.getByRole('button', { name: '置顶项目 pi-rust', exact: true }).press('Enter')
    assert.deepEqual(await projectRows.allTextContents(), ['pi-rust', 'rpi-work'])
    await nodeToggle.press('Enter')
    assert.equal(await nodeToggle.getAttribute('aria-expanded'), 'false')
    const projectTraffic = []
    page.on('request', request => {
      if (/\/api\/v1\/(projects|sessions)(?:[/?]|$)/.test(request.url())) projectTraffic.push(request.url())
    })
    const openGeneral = async () => {
      await page.getByRole('button', { name: '打开显示选项', exact: true }).click()
      const settings = page.locator('header').getByRole('button', { name: '设置', exact: true })
      await settings.click()
      await dialog.getByRole('searchbox').fill('项目列表显示')
      await dialog.getByRole('button', { name: '常规', exact: true }).click()
    }
    await openGeneral()
    assert.equal(await projectLayout.inputValue(), 'node')
    await projectLayout.selectOption('flat')
    await page.keyboard.press('Escape')
    await projectRows.first().waitFor()
    assert.equal(await nodeToggle.count(), 0)
    assert.deepEqual(await projectRows.allTextContents(), ['pi-rust', 'rpi-work'])
    assert.match(await projectRows.first().getAttribute('aria-description'), /所属节点：127\.0\.0\.1:/)
    assert.equal(await page.locator('.wb-project-node-icon').count(), 2)
    assert.equal(await page.locator('.wb-thread[aria-current="true"]').textContent(), selected)
    await page.screenshot({ path: path.join(artifacts, 'projects-flat.png') })
    await openGeneral()
    await projectLayout.selectOption('node')
    await page.keyboard.press('Escape')
    await nodeToggle.waitFor()
    assert.equal(await nodeToggle.getAttribute('aria-expanded'), 'false')
    await nodeToggle.press('Space')
    await projectRows.first().waitFor()
    assert.deepEqual(projectTraffic, [], 'changing project display cannot reload project or session state')
    assert.deepEqual(await page.locator('textarea').evaluate(node => [node.value, node.selectionStart, node.selectionEnd]), ['设置窗口保留聊天草稿与选区', 2, 7])
    assert.equal(await previewDraft.inputValue(), 'do not reload while settings are open')
    assert.equal(previewLoads, loads)
    await page.screenshot({ path: path.join(artifacts, 'projects-grouped.png') })
    await openGeneral()
    await projectLayout.selectOption('flat')
    await dialog.getByRole('searchbox').fill('')
    pass('project layout is searchable, keeps node identity and ordering, and preserves the collapsed group and active workspace')

    await dialog.getByRole('button', { name: '外观', exact: true }).click()
    await dialog.getByRole('combobox', { name: '颜色模式' }).selectOption('dark')
    await dialog.getByRole('combobox', { name: '界面密度' }).selectOption('compact')
    await dialog.getByRole('combobox', { name: '消息布局' }).selectOption('classic')
    const presets = dialog.getByRole('combobox', { name: '选择主题' })
    assert(await presets.locator('option').count() >= 5)
    const alternate = await presets.locator('option').nth(1).getAttribute('value')
    await page.screenshot({ path: path.join(artifacts, 'desktop-dark.png') })
    await presets.selectOption(alternate)
    await page.keyboard.press('Escape')
    assert.equal(await dialog.count(), 0)
    assert.equal(await page.getByRole('button', { name: '打开显示选项', exact: true }).evaluate(node => node === document.activeElement), true)
    assert.equal(await page.evaluate(() => settingsNodes.transcript === document.querySelector('.wb-transcript') && settingsNodes.composer === document.querySelector('textarea') && settingsNodes.frame === document.querySelector('.real-browser iframe')), true)
    assert.deepEqual(await page.locator('textarea').evaluate(node => [node.value, node.selectionStart, node.selectionEnd]), ['设置窗口保留聊天草稿与选区', 2, 7])
    assert.equal(await previewDraft.inputValue(), 'do not reload while settings are open')
    assert.equal(previewLoads, loads)
    assert.equal(await page.evaluate(() => Object.values(localStorage).some(value => value.includes('memory-only-unsaved-settings-key'))), false)
    pass('Escape restores the visible opener without replacing chat, draft selection or the real iframe')

    await page.getByRole('button', { name: '工作台设置', exact: true }).click()
    assert.equal(await page.getByLabel('deepseek 的 API 密钥').count(), 0, 'closing clears private unsent key inputs')
    await page.getByRole('button', { name: '关闭设置', exact: true }).click()
    assert.equal(await page.getByRole('button', { name: '工作台设置', exact: true }).evaluate(node => node === document.activeElement), true)
    await page.reload()
    await page.getByText('在线', { exact: true }).waitFor()
    await page.getByRole('button', { name: '工作台设置', exact: true }).click()
    await dialog.getByRole('button', { name: '常规', exact: true }).click()
    assert.equal(await projectLayout.inputValue(), 'flat')
    assert.equal(await page.locator('#wb-projects').getAttribute('data-layout'), 'flat')
    await dialog.getByRole('button', { name: '外观', exact: true }).click()
    assert.equal(await dialog.getByRole('combobox', { name: '颜色模式' }).inputValue(), 'dark')
    assert.equal(await dialog.getByRole('combobox', { name: '界面密度' }).inputValue(), 'compact')
    assert.equal(await dialog.getByRole('combobox', { name: '消息布局' }).inputValue(), 'classic')
    assert.equal(await dialog.getByRole('combobox', { name: '选择主题' }).inputValue(), alternate)
    pass('settings reuse persistent device preferences, clear private keys on close and return focus to the rail opener')

    for (const width of [390, 320]) {
      await page.setViewportSize({ width, height: 844 })
      const mobile = await dialog.boundingBox()
      assert(Math.abs(mobile.x) < 1 && Math.abs(mobile.y) < 1 && Math.abs(mobile.width - width) < 1 && Math.abs(mobile.height - 844) < 1, JSON.stringify(mobile))
      const browse = dialog.getByRole('button', { name: '浏览分类', exact: true })
      await browse.click()
      await dialog.getByRole('button', { name: '提供商', exact: true }).click()
      assert.equal(await browse.getAttribute('aria-expanded'), 'false')
      assert.equal(await page.locator('.wb-settings-content').evaluate(node => node === document.activeElement), true)
      await page.getByRole('button', { name: '设置 deepseek 的密钥' }).click()
      await page.getByLabel('deepseek 的 API 密钥').fill('mobile-private-key')
      const overflow = await dialog.evaluate(node => {
        const content = node.querySelector('.wb-settings-content')
        return { dialog: node.scrollWidth - node.clientWidth, content: content.scrollWidth - content.clientWidth }
      })
      assert(overflow.dialog <= 1 && overflow.content <= 1, JSON.stringify({ width, overflow }))
      await page.screenshot({ path: path.join(artifacts, `mobile-${width}.png`) })
      await page.getByRole('button', { name: '取消', exact: true }).click()
      await browse.click()
      await dialog.getByRole('button', { name: '常规', exact: true }).click()
      assert.equal(await projectLayout.inputValue(), 'flat')
      assert(await projectLayout.evaluate(node => node.getBoundingClientRect().right <= innerWidth))
    }
    await page.getByRole('button', { name: '关闭设置', exact: true }).click()
    pass('390px and 320px use full-screen settings with collapsible categories and contained credential forms')
    await page.setViewportSize({ width: 1440, height: 900 })
    await page.getByRole('button', { name: '断开节点', exact: true }).click()
    await page.getByRole('button', { name: '工作台设置', exact: true }).click()
    assert.equal(await page.getByTestId('refresh-providers').isDisabled(), true)
    assert.equal(await page.getByRole('button', { name: '设置 deepseek 的密钥' }).count(), 0)
    await dialog.getByRole('button', { name: '工作协同', exact: true }).click()
    await page.getByText('当前没有连接真实节点；连接后才能读取节点信息。', { exact: true }).waitFor()
    assert.equal(await page.getByRole('button', { name: '生成一次性授权码' }).count(), 0)
    await dialog.getByRole('button', { name: '常规', exact: true }).click()
    assert.equal(await projectLayout.inputValue(), 'flat')
    await projectLayout.selectOption('node')
    assert.equal(writes.length, 2)
    await page.getByRole('button', { name: '关闭设置', exact: true }).click()
    pass('disconnected settings keep local preferences separate and never present writable node facts')
    assert.deepEqual(errors, [], 'page errors: ' + errors.join(' | '))
    pass('no page errors')
    console.log('RESULT', passed.length, 'checks passed')
  } catch (error) {
    console.error('FAIL', error.stack)
    console.error('page errors:', errors.slice(0, 3))
    try {
      console.error('main:', (await page.locator('main').innerText()).slice(0, 300).replace(/\n/g, ' | '))
      console.error('providersServed:', providersServed)
    } catch {}
    await page.screenshot({ path: path.join(artifacts, 'failed.png') })
    process.exitCode = 1
  } finally {
    await browser.close()
    if (preview.exitCode === null && preview.signalCode === null) await new Promise(resolve => { preview.once('exit', resolve); preview.kill('SIGTERM') })
    await fixture.close()
    if (site.listening) await new Promise(resolve => site.close(resolve))
  }
})()
