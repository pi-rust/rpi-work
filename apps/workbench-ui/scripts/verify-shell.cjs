// Confirmed shell geometry + device-only preferences + node-owned session metadata.
const assert = require('node:assert/strict')
const fs = require('node:fs')
const net = require('node:net')
const path = require('node:path')
const { spawn } = require('node:child_process')
const { chromium } = require(require.resolve('playwright', { paths: [path.resolve(__dirname, '../../../tools')] }))
const { startFixture } = require('../../../tests/fixtures/ui-fixture.cjs')
const root = path.resolve(__dirname, '..')
const freePort = () => new Promise(resolve => { const server = net.createServer(); server.listen(0, '127.0.0.1', () => { const port = server.address().port; server.close(() => resolve(port)) }) })
;(async () => {
  const fixture = await startFixture(0, { nativeQueries: true, includeHistoryAssistant: true })
  const port = await freePort()
  const origin = `http://127.0.0.1:${port}`
  const preview = spawn(process.execPath, [path.join(root, 'node_modules/vite/bin/vite.js'), 'preview', '--mode', 'fixture', '--host', '127.0.0.1', '--port', String(port), '--strictPort'], { cwd: root, env: { ...process.env, RPI_NODE_URL: fixture.url }, stdio: 'ignore' })
  let browser
  const errors = []
  try {
    for (let i = 0; i < 120; i++) { try { if ((await fetch(origin)).ok) break } catch {} await new Promise(resolve => setTimeout(resolve, 100)) }
    browser = await chromium.launch()
    const open = async (width = 1440, scheme = 'light') => {
      const page = await browser.newPage({ viewport: { width, height: 940 }, deviceScaleFactor: 1 })
      page.on('pageerror', error => errors.push(error.message))
      await page.goto(origin + '/?fixture=1')
      await page.getByText('主题切换已经准备好').waitFor()
      if (scheme === 'dark') await page.getByRole('button', { name: '切换主题', exact: true }).click()
      return page
    }
    const page = await open()
    const button = name => page.getByRole('button', { name, exact: true })
    const projects = page.getByRole('region', { name: '项目', exact: true })
    const recent = page.getByRole('region', { name: '最近会话', exact: true })
    assert.equal(await recent.locator('.wb-thread').count(), 4)
    assert.equal(await projects.locator('.wb-thread').count(), 0, 'Projects are not nested conversation lists')
    const before = await recent.locator('.wb-thread').allTextContents()
    await page.locator('textarea').fill('外壳迁移不丢草稿')
    await button('置顶项目 pi-rust').click()
    assert.deepEqual(await projects.locator('.wb-project').allTextContents(), ['pi-rust', 'rpi-work'])
    assert.deepEqual(await recent.locator('.wb-thread').allTextContents(), before, 'Project pinning does not move sessions')
    assert.equal(await page.locator('.wb-window-title').innerText().then(text => text.includes('为工作台添加深色模式')), true)
    await button('项目列表').click()
    assert.equal(await recent.isVisible(), true)
    assert.equal(await page.locator('textarea').inputValue(), '外壳迁移不丢草稿')
    await page.reload(); await page.getByText('主题切换已经准备好').waitFor()
    assert.equal(await button('项目列表').getAttribute('aria-expanded'), 'false')
    await button('项目列表').click()
    assert.equal(await button('置顶项目 pi-rust').getAttribute('aria-pressed'), 'true')
    await button('切换会话侧栏').click(); await button('切换会话侧栏').click()
    assert.equal(await page.locator('textarea').inputValue(), '外壳迁移不丢草稿')
    assert.equal(await button('新对话').isDisabled(), true, 'No fake create/clear-draft action')
    assert.equal(await button('添加项目').isDisabled(), true)
    // Prepare only the isolated fixture; these are not product metadata writes.
    for (const [id, metadata] of [['dark', { pinned: true }], ['architecture', { archived: true }]]) {
      const response = await fetch(new URL(`/api/v1/sessions/${id}/metadata`, fixture.url), { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(metadata) })
      assert.equal(response.ok, true)
    }
    await page.goto(origin + '/?live=1'); await page.getByText('在线', { exact: true }).waitFor()
    assert.equal(await button('置顶项目 pi-rust').getAttribute('aria-pressed'), 'false', 'Fixture preferences cannot leak into a node scope')
    assert.equal(await page.getByRole('region', { name: '置顶会话' }).getByRole('button', { name: '为工作台添加深色模式', exact: true }).count(), 1)
    assert.equal(await recent.getByRole('button', { name: '为工作台添加深色模式', exact: true }).count(), 0)
    assert.equal(await recent.getByRole('button', { name: '梳理 Agent 架构', exact: true }).count(), 0)
    assert.equal(await recent.locator('.wb-thread').count(), 2)
    await button('断开节点').click(); await page.getByText('未连接节点', { exact: true }).waitFor()
    assert.equal(await page.locator('.wb-project').count(), 0, 'Disconnect is not an implicit fixture fallback')
    assert.equal(await page.locator('.wb-thread').count(), 0)
    await page.goto(origin + '/?fixture=1'); await page.getByText('主题切换已经准备好').waitFor()
    assert.equal(await button('置顶项目 pi-rust').getAttribute('aria-pressed'), 'true', 'Only explicit fixture activation restores its own display preferences')
    const empty = await open()
    await empty.route('**/api/v1/projects', route => route.fulfill({ json: { projects: [] } }))
    await empty.route('**/api/v1/sessions', route => route.fulfill({ json: { sessions: [] } }))
    await empty.goto(origin + '/?live=1'); await empty.getByText('在线', { exact: true }).waitFor()
    assert.equal(await empty.locator('.wb-project').count(), 0)
    assert.equal(await empty.getByText('当前节点没有项目', { exact: true }).isVisible(), true)
    assert.equal(await empty.getByText('当前节点没有会话', { exact: true }).isVisible(), true)
    await empty.close()
    const shots = path.resolve(root, '../../artifacts/confirmed-workbench')
    fs.mkdirSync(shots, { recursive: true })
    for (const width of [1440, 1024, 900, 701, 700, 390, 320]) for (const scheme of ['light', 'dark']) {
      const shot = await open(width, scheme)
      const header = await shot.locator('.wb-window-header').boundingBox()
      assert.equal(header.x, 0); assert.equal(header.width, width); assert.equal(header.height, width > 700 ? 46 : 44)
      if (width > 700) {
        const rail = await shot.locator('.wb-activity-rail').boundingBox()
        const sidebar = await shot.locator('.wb-sidebar').boundingBox()
        assert.equal(rail.width, 52); assert.equal(rail.y, 46)
        assert.equal(sidebar.x, 52); assert.equal(sidebar.width, 275); assert.equal(sidebar.y, 46)
        if (width > 900) {
          const pane = await shot.getByTestId('run-panel').boundingBox()
          assert.ok(Math.abs(pane.width - Math.min(560, Math.max(360, width * .38))) < 1)
          assert.equal(pane.y, 46)
        } else assert.equal(await shot.getByTestId('run-panel').isVisible(), false)
      } else {
        assert.equal(await shot.getByTestId('sidebar-brand').count(), 0)
        assert.equal(await shot.locator('.wb-history-controls').isVisible(), false)
      }
      assert.equal(await shot.evaluate(() => document.documentElement.scrollWidth > innerWidth + 1), false)
      await shot.screenshot({ path: path.join(shots, `shell-${width}-${scheme}.png`), animations: 'disabled' })
      await shot.close()
    }
    const setArchived = async (id, archived) => {
      const response = await fetch(new URL(`/api/v1/sessions/${id}/metadata`, fixture.url), {
        method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ archived }),
      })
      assert.equal(response.ok, true)
    }
    await setArchived('architecture', false)
    const nav = await open()
    const navButton = name => nav.getByRole('button', { name, exact: true })
    const back = navButton('上一个会话')
    const forward = navButton('下一个会话')
    const current = name => nav.locator('.wb-window-title').getByText(name, { exact: true })
    const online = () => nav.getByText('在线', { exact: true }).waitFor()
    const dark = '为工作台添加深色模式'
    const architecture = '梳理 Agent 架构'
    const rpc = '验证 RPC 事件协议'
    const mobile = '检查手机端布局'
    await nav.goto(origin + '/?live=1'); await online(); await current(dark).waitFor()
    assert.equal(await back.isDisabled(), true)
    assert.equal(await forward.isDisabled(), true)
    await navButton(architecture).click(); await online()
    await back.click(); await online(); await current(dark).waitFor()
    await navButton(dark).click(); await online()
    assert.equal(await back.isDisabled(), true)
    assert.equal(await forward.isEnabled(), true, 'Reselecting the current session preserves the forward branch')

    // Hold the next session snapshot until the user has already gone back.
    const snapshot = '**/api/v1/sessions/architecture/snapshot*'
    let releaseSnapshot
    const holdSnapshot = new Promise(resolve => { releaseSnapshot = resolve })
    await nav.route(snapshot, async route => {
      await holdSnapshot
      await route.continue()
    }, { times: 1 })
    try {
      const requested = nav.waitForRequest(snapshot)
      await forward.click(); await requested; await current(architecture).waitFor()
      await nav.getByText('连接中', { exact: true }).waitFor()
      await back.click(); await online(); await current(dark).waitFor()
      const delivered = nav.waitForResponse(snapshot)
      releaseSnapshot(); await (await delivered).finished()
      await current(dark).waitFor()
      assert.equal(await back.isDisabled(), true)
      assert.equal(await forward.isEnabled(), true)
    } finally {
      releaseSnapshot()
    }
    await navButton('pi-rust').click(); await online(); await current(rpc).waitFor()
    assert.equal(await forward.isDisabled(), true, 'A different visit replaces the forward branch')
    await navButton('技能与插件').click()
    await navButton('搜索项目与会话').click()
    const search = nav.getByRole('textbox', { name: '搜索项目与会话' })
    await search.fill(mobile); await search.press('Enter')
    await online(); await current(mobile).waitFor()
    await nav.locator('.wb-message-scroll').waitFor()
    await back.click(); await online(); await current(rpc).waitFor()
    await forward.click(); await online(); await current(mobile).waitFor()
    console.log('PASS title-bar history follows exact cross-project sidebar and search selections, branches and ignores late snapshots')

    // The real create flow refreshes the node lists and selects the confirmed empty project.
    await setArchived('rpc', true)
    await nav.route('**/api/v1/sessions', async route => {
      const response = await route.fetch()
      const { sessions } = await response.json()
      await route.fulfill({ response, json: { sessions: sessions.filter(session => session.sessionId !== 'mobile') } })
    })
    await navButton('添加项目').click()
    const add = nav.getByRole('dialog', { name: '添加项目', exact: true })
    await add.getByLabel('名称', { exact: true }).fill('历史中的空项目')
    await add.getByLabel('项目路径', { exact: true }).fill('/fixture/history-empty')
    await add.getByRole('button', { name: '添加项目', exact: true }).click()
    await nav.getByText('尚未选择会话', { exact: true }).waitFor()
    await nav.locator('textarea').fill('空项目草稿')
    await back.click(); await online(); await current(dark).waitFor()
    assert.equal(await back.isDisabled(), true)
    await forward.click(); await nav.getByText('尚未选择会话', { exact: true }).waitFor()
    assert.equal(await navButton('历史中的空项目').getAttribute('aria-current'), 'true')
    assert.equal(await nav.locator('textarea').inputValue(), '空项目草稿')
    await navButton('断开节点').click(); await nav.getByText('未连接节点', { exact: true }).waitFor()
    assert.equal(await back.isDisabled(), true)
    assert.equal(await forward.isDisabled(), true)
    await navButton('连接节点').click(); await online(); await current(dark).waitFor()
    assert.equal(await back.isDisabled(), true)
    assert.equal(await forward.isDisabled(), true)
    await nav.close()
    console.log('PASS history skips archived and removed node sessions, restores empty projects and resets on reconnect')
    assert.deepEqual(errors, [])
    console.log('PASS shell geometry at 7 widths × 2 themes, flat lists, isolated preferences, node pin/archive facts, honest empty/unsupported states, draft retention')
  } finally {
    await browser?.close()
    if (preview.exitCode === null) await new Promise(resolve => { preview.once('exit', resolve); preview.kill('SIGTERM') })
    await fixture.close()
  }
})().catch(error => { console.error(error); process.exitCode = 1 })
