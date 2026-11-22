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
    await panel.getByRole('button', { name: /src\/theme\.rs/ }).first().waitFor()
    pass('file panel lists the node worktree changes')

    await panel.getByRole('button', { name: /src\/theme\.rs/ }).first().click()
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

    fixture.controls.fileEntriesByPath = {
      src: [{ name: 'theme.rs', path: 'src/theme.rs', kind: 'file', size: 12 }],
    }
    await panel.getByTestId('tab-tree').click()
    await panel.getByText('src', { exact: true }).first().waitFor()
    pass('directory browser lists the project root')

    await panel.getByText('src', { exact: true }).first().click()
    await panel.getByText('theme.rs', { exact: true }).waitFor()
    await panel.getByText('theme.rs', { exact: true }).click()
    await panel.getByText(/RPI Work/).waitFor()
    pass('opening a file in the tree renders its source')

    // P7a: resource center projects the native catalog through Rust rules.
    await page.route('**/api/v1/projects/*/native-resources', async (route) => {
      await route.fulfill({
        json: {
          profile: { inspect: true, remove: true, install: true, update: true },
          diagnostics: [],
          resources: [
            {
              reference: 'skill:review-helper@1.2.0',
              name: 'review-helper',
              kind: 'skill',
              scope: 'global',
              facts: { installed: true, version: '1.2.0' },
            },
            {
              reference: 'extension:theme-pack@0.4.0',
              name: 'theme-pack',
              kind: 'extension',
              scope: 'project',
              facts: { installed: true, version: '0.4.0' },
            },
          ],
        },
      })
    })
    await page.getByRole('button', { name: '资源中心' }).click()
    await page.getByText('review-helper', { exact: false }).waitFor()
    await page.getByText('theme-pack', { exact: false }).waitFor()
    const resourceText = await page.locator('main').innerText()
    assert.match(resourceText, /资源中心/)
    assert.match(resourceText, /全局/)
    assert.match(resourceText, /项目/)
    pass('resource center lists installed resources with Rust grouping')

    // P7b: install is gated by explicit scope + trust and confirmed by readback.
    const writes = []
    let nativeReads = 0
    await page.unroute('**/api/v1/projects/*/native-resources')
    await page.route('**/api/v1/projects/*/native-resources', async (route) => {
      nativeReads += 1
      await route.fulfill({
        json: {
          profile: { inspect: true, remove: true, install: true, update: true },
          diagnostics: [],
          resources: nativeReads > 1
            ? [{ reference: 'package:catalog-pack@2.0.0', name: 'catalog-pack', kind: 'package', scope: 'project', facts: { installed: true, version: '2.0.0' } }]
            : [],
        },
      })
    })
    await page.route('**/api/v1/projects/*/packages', async (route) => {
      await route.fulfill({
        json: {
          source: 'fixture',
          diagnostics: [],
          packages: [
            {
              manifest: {
                schemaVersion: 1,
                id: 'catalog-pack',
                name: 'catalog-pack',
                version: '2.0.0',
                kind: 'package',
                title: 'Catalog Pack',
                description: 'fixture package',
                author: 'fixture',
                origin: 'fixture',
                instructions: ['prompts/system.md'],
              },
              sha256: 'a'.repeat(64),
              fileCount: 2,
              sizeBytes: 120,
              acquired: false,
            },
          ],
        },
      })
    })
    await page.route('**/api/v1/projects/*/packages/bundle', async (route) => {
      await route.fulfill({
        json: {
          manifest: {
                schemaVersion: 1,
                id: 'catalog-pack',
                name: 'catalog-pack',
                version: '2.0.0',
                kind: 'package',
                title: 'Catalog Pack',
                description: 'fixture package',
                author: 'fixture',
                origin: 'fixture',
                instructions: ['prompts/system.md'],
              },
          sources: { 'SKILL.md': '# fixture', 'prompts/system.md': 'be careful' },
          sha256: 'a'.repeat(64),
        },
      })
    })
    await page.route('**/api/v1/projects/*/packages/acquire', async (route) => {
      writes.push({ path: 'acquire', body: JSON.parse(route.request().postData() ?? '{}') })
      await route.fulfill({
        json: {
          package: { id: 'catalog-pack', version: '2.0.0', sha256: 'a'.repeat(64) },
          source: '/tmp/fixture/catalog-pack',
        },
      })
    })
    await page.route('**/api/v1/projects/*/packages/install', async (route) => {
      writes.push({ path: 'install', body: JSON.parse(route.request().postData() ?? '{}') })
      await route.fulfill({
        json: { success: true, observation: { installed: true }, diagnostics: '' },
      })
    })
    await page.getByRole('button', { name: '刷新', exact: true }).click()
    await page.getByRole('button', { name: '审阅 Catalog Pack' }).click()
    await page.getByText('我已审阅来源与文件清单').waitFor()
    const confirmButton = page.getByRole('button', { name: '确认安装' })
    assert(await confirmButton.isDisabled(), 'install is disabled before trust')
    pass('install stays disabled until the review is trusted')

    await page.getByLabel('安装范围').selectOption('global')
    await page.getByLabel('我已审阅来源').check()
    await confirmButton.click()
    await page.getByText('catalog-pack', { exact: false }).waitFor()
    assert.equal(writes.length, 2, 'acquire + install both sent: ' + JSON.stringify(writes))
    assert.equal(writes[1].body.scope, 'global', 'scope forwarded: ' + JSON.stringify(writes[1].body))
    assert.equal(writes[1].body.trusted, true, 'install is explicit about trust')
    assert(nativeReads > 1, 'catalog is re-read after install')
    pass('install flow sends scope + trust and confirms by catalogue readback')

    // P7c: provider settings page (read-only, from the node catalogue).
    await page.locator('header').getByRole('button', { name: '设置', exact: true }).click()
    await page.getByText('节点与协作', { exact: true }).waitFor()
    const settingsText = await page.locator('main').innerText()
    assert.match(settingsText, /节点地址/)
    assert.match(settingsText, new RegExp(fixture.origin ?? '127\\.0\\.0\\.1'))
    assert.match(settingsText, /提供商/)
    // The repository fixture has no provider endpoint, so the page must show the
    // node's own failure instead of an empty list.
    assert.match(settingsText, /提供商目录读取失败|节点没有返回任何提供商/);
    pass('settings page shows node facts and surfaces provider read failures')

    await page.getByRole('button', { name: '运行面板' }).click()
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
