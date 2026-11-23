// Isolated protocol fixture only: no model, installed agent, or user directories.
const assert = require('node:assert/strict')
const path = require('node:path')
const fs = require('node:fs')
const net = require('node:net')
const { spawn } = require('node:child_process')
const { chromium } = require(require.resolve('playwright', { paths: [path.resolve(__dirname, '../../../tools')] }))
const { startFixture } = require('../../../tests/fixtures/ui-fixture.cjs')
const root = path.resolve(__dirname, '..')
const freePort = () => new Promise(resolve => { const server = net.createServer(); server.listen(0, '127.0.0.1', () => { const port = server.address().port; server.close(() => resolve(port)) }) })
const gate = () => { let release; const promise = new Promise(resolve => { release = resolve }); return { promise, release } }
;(async () => {
  const fixture = await startFixture(0, { nativeQueries: true, includeHistoryAssistant: true })
  const port = await freePort()
  const origin = `http://127.0.0.1:${port}`
  const preview = spawn(process.execPath, [path.join(root, 'node_modules/vite/bin/vite.js'), 'preview', '--host', '127.0.0.1', '--port', String(port), '--strictPort'], { cwd: root, env: { ...process.env, RPI_NODE_URL: fixture.url }, stdio: 'ignore' })
  let browser
  const errors = [], gates = []
  const pass = name => console.log('PASS', name)
  const reads = async kind => (await (await fetch(new URL(`/api/v1/${kind}`, fixture.url))).json())[kind]
  const posts = kind => fixture.requests.filter(request => request.path === `/api/v1/${kind}` && request.method === 'POST')
  try {
    let listening = false
    for (let i = 0; i < 120; i++) { try { if ((await fetch(origin)).ok) { listening = true; break } } catch {} await new Promise(resolve => setTimeout(resolve, 100)) }
    assert(listening, 'owned preview started')
    browser = await chromium.launch()
    const open = async (width = 1440) => {
      const page = await browser.newPage({ viewport: { width, height: 940 }, deviceScaleFactor: 1 })
      page.on('pageerror', error => errors.push(error.message))
      page.setDefaultTimeout(15000)
      await page.goto(origin + '/?live=1')
      await page.getByText('在线', { exact: true }).waitFor()
      return page
    }
    const button = (page, name) => page.getByRole('button', { name, exact: true })
    const modal = page => page.getByRole('dialog', { name: '添加项目', exact: true })
    const project = (page, name) => page.locator('.wb-project').filter({ hasText: name })
    const fill = async (page, name, directory) => {
      await button(page, '添加项目').click()
      await modal(page).getByLabel('名称', { exact: true }).fill(name)
      await modal(page).getByLabel('项目路径', { exact: true }).fill(directory)
    }
    const submit = page => modal(page).getByRole('button', { name: '添加项目', exact: true }).click()
    const page = await open()
    await page.locator('textarea').fill('旧会话草稿不清空')
    const beforeProjects = (await reads('projects')).length
    await fill(page, '真实登记测试', '/fixture/existing-project')
    assert.equal(await modal(page).getByRole('button', { name: '选择文件夹', exact: true }).isDisabled(), true)
    assert.equal(await modal(page).getByLabel('所属节点', { exact: true }).isDisabled(), true)
    await submit(page)
    await modal(page).waitFor({ state: 'hidden' })
    await project(page, '真实登记测试').waitFor()
    assert.equal((await reads('projects')).length, beforeProjects + 1)
    assert.deepEqual(posts('projects').at(-1).body, { name: '真实登记测试', path: '/fixture/existing-project', initGit: false })
    assert.equal(await page.locator('textarea').inputValue(), '')
    await page.locator('textarea').fill('空项目草稿')
    assert.equal(await button(page, '发送').isDisabled(), true)
    assert.equal(await button(page, '加入队列').isDisabled(), true)
    await page.locator('textarea').press('Control+Enter')
    assert.equal(fixture.requests.some(request => request.body?.type === 'prompt'), false)
    const createdProject = (await reads('projects')).find(item => item.name === '真实登记测试')
    const beforeDoubleClick = posts('sessions').length
    await button(page, '新对话').evaluate(element => { element.click(); element.click() })
    await page.locator('.wb-thread[aria-current="true"]').filter({ hasText: '新会话' }).waitFor()
    await page.getByText('在线', { exact: true }).waitFor()
    assert.equal(posts('sessions').at(-1).body.projectId, createdProject.id)
    assert.equal(posts('sessions').length, beforeDoubleClick + 1, 'Even same-turn duplicate activation sends one request')
    assert.equal(await page.locator('textarea').inputValue(), '')
    await page.locator('textarea').fill('新会话自己的草稿')
    await button(page, '为工作台添加深色模式').click()
    await page.getByText('在线', { exact: true }).waitFor()
    assert.equal(await page.locator('textarea').inputValue(), '旧会话草稿不清空')
    await project(page, '真实登记测试').click()
    await page.getByText('在线', { exact: true }).waitFor()
    assert.equal(await page.locator('textarea').inputValue(), '新会话自己的草稿')
    const count = posts('projects').length
    await button(page, '添加项目').click(); await modal(page).waitFor()
    await page.keyboard.press('Escape'); await modal(page).waitFor({ state: 'hidden' })
    assert.equal(posts('projects').length, count)
    assert.equal(await button(page, '添加项目').evaluate(element => element === document.activeElement), true)
    pass('typed project/session creation reconciles node IDs and preserves old/new drafts; cancel is side-effect-free')

    fixture.controls.failProject = true
    await fill(page, '拒绝后可修正', '/fixture/rejected')
    await submit(page)
    await modal(page).getByRole('alert').filter({ hasText: 'rejected' }).waitFor()
    assert.equal(await modal(page).getByLabel('名称', { exact: true }).inputValue(), '拒绝后可修正')
    assert.equal(await modal(page).getByRole('button', { name: '添加项目', exact: true }).isEnabled(), true)
    assert.equal(await page.locator('textarea').inputValue(), '新会话自己的草稿')
    fixture.controls.failProject = false
    await submit(page); await modal(page).waitFor({ state: 'hidden' })
    await project(page, '拒绝后可修正').waitFor()
    await page.locator('textarea').fill('拒绝不清空项目草稿')
    await page.route('**/api/v1/sessions', route => route.request().method() === 'POST' ? route.fulfill({ status: 403, json: { error: 'fixture denied session creation' } }) : route.continue())
    await button(page, '新对话').click()
    await page.getByRole('alert').filter({ hasText: 'denied session' }).waitFor()
    assert.equal(await page.locator('textarea').inputValue(), '拒绝不清空项目草稿')
    assert.equal(await button(page, '新对话').isEnabled(), true)
    await page.unroute('**/api/v1/sessions')
    await fill(page, '另一个空项目', '/fixture/second-empty')
    await submit(page); await modal(page).waitFor({ state: 'hidden' })
    assert.equal(await page.locator('textarea').inputValue(), '', 'Empty projects have distinct draft scopes')
    await project(page, '拒绝后可修正').click()
    assert.equal(await page.locator('textarea').inputValue(), '拒绝不清空项目草稿')
    pass('explicit rejection permits correction, keeps input/draft, and empty projects do not share drafts')

    const unknown = await open()
    fixture.controls.failSession = true
    const beforeUnknown = posts('sessions').length
    await button(unknown, '新对话').click()
    await unknown.getByRole('alert').filter({ hasText: '结果未确认' }).waitFor()
    assert.equal(await button(unknown, '新对话').isDisabled(), true)
    await button(unknown, '核对创建结果').click()
    await unknown.getByRole('alert').filter({ hasText: '仍未确认' }).waitFor()
    assert.equal(posts('sessions').length, beforeUnknown + 1, 'Read-only reconciliation never re-posts an unknown create')
    assert.equal(await button(unknown, '新对话').isDisabled(), true)
    fixture.controls.failSession = false
    await unknown.close()
    pass('500/unknown outcome remains locked; reconciliation is GET-only and does not infer success')

    const recovery = await open()
    let failReadback = false, recoveringPosts = 0
    await recovery.route('**/api/v1/projects', async route => {
      if (route.request().method() === 'POST') { recoveringPosts++; const response = await route.fetch(); failReadback = true; await route.fulfill({ response }) }
      else if (failReadback) await route.fulfill({ status: 503, json: { error: 'fixture readback unavailable' } })
      else await route.continue()
    })
    const oldListSize = await recovery.locator('.wb-project').count()
    await fill(recovery, '已提交待核对', '/fixture/readback')
    await submit(recovery)
    await modal(recovery).getByRole('alert').filter({ hasText: '尚未核对' }).waitFor()
    assert.equal(await recovery.locator('.wb-project').count(), oldListSize, 'No optimistic insertion')
    assert.equal((await reads('projects')).some(item => item.name === '已提交待核对'), true, 'Node really committed')
    assert.equal(await modal(recovery).getByRole('button', { name: '添加项目', exact: true }).isDisabled(), true)
    failReadback = false
    await modal(recovery).getByRole('button', { name: '核对创建结果', exact: true }).click()
    await modal(recovery).waitFor({ state: 'hidden' })
    await project(recovery, '已提交待核对').waitFor()
    assert.equal(recoveringPosts, 1)
    await recovery.close()
    pass('committed ID plus failed readback stays locked until exact-ID read-only reconciliation')

    const late = await open()
    const held = gate(), entered = gate(), finished = gate(); gates.push(held)
    await late.route('**/api/v1/projects', async route => {
      if (route.request().method() !== 'POST') return route.continue()
      const response = await route.fetch(); entered.release(); await held.promise
      await route.fulfill({ response }); finished.release()
    })
    await fill(late, '迟到但不抢会话', '/fixture/late')
    await submit(late); await entered.promise
    await modal(late).getByRole('button', { name: '取消', exact: true }).click()
    await button(late, '梳理 Agent 架构').click(); await late.getByText('在线', { exact: true }).waitFor()
    await late.locator('textarea').fill('切换后自己的草稿')
    held.release(); await finished.promise
    await project(late, '迟到但不抢会话').waitFor()
    await late.waitForFunction(() => !document.querySelector('.wb-creation-message'))
    assert.equal(await late.locator('.wb-thread[aria-current="true"]').innerText(), '梳理 Agent 架构')
    assert.equal(await late.locator('textarea').inputValue(), '切换后自己的草稿')
    await late.close()
    pass('late successful create refreshes only same-node facts, never steals a changed session or draft')

    const rejectedLate = await open()
    const rejectReply = gate(), rejectEntered = gate(), rejectFinished = gate(); gates.push(rejectReply)
    await rejectedLate.route('**/api/v1/sessions', async route => {
      if (route.request().method() !== 'POST') return route.continue()
      rejectEntered.release(); await rejectReply.promise
      await route.fulfill({ status: 403, json: { error: 'obsolete selected-session rejection' } }); rejectFinished.release()
    })
    await button(rejectedLate, '新对话').click(); await rejectEntered.promise
    await button(rejectedLate, '梳理 Agent 架构').click(); await rejectedLate.getByText('在线', { exact: true }).waitFor()
    await rejectedLate.locator('textarea').fill('拒绝回调也不改新会话')
    rejectReply.release(); await rejectFinished.promise
    await rejectedLate.waitForFunction(() => !document.querySelector('.wb-creation-message'))
    assert.equal(await rejectedLate.locator('.wb-thread[aria-current="true"]').innerText(), '梳理 Agent 架构')
    assert.equal(await rejectedLate.locator('textarea').inputValue(), '拒绝回调也不改新会话')
    assert.equal(await button(rejectedLate, '新对话').isEnabled(), true)
    await rejectedLate.close()
    pass('late explicit rejection is discarded after the selection generation changes')

    const memory = await open()
    await memory.addInitScript(() => { Storage.prototype.setItem = () => { throw new DOMException('Fixture storage denied', 'QuotaExceededError') } })
    await memory.reload(); await memory.getByText('在线', { exact: true }).waitFor()
    await memory.locator('textarea').fill('禁止存储也保留 A')
    await button(memory, '梳理 Agent 架构').click(); await memory.getByText('在线', { exact: true }).waitFor()
    await memory.locator('textarea').fill('禁止存储也保留 B')
    await button(memory, '为工作台添加深色模式').click(); await memory.getByText('在线', { exact: true }).waitFor()
    assert.equal(await memory.locator('textarea').inputValue(), '禁止存储也保留 A')
    await button(memory, '梳理 Agent 架构').click(); await memory.getByText('在线', { exact: true }).waitFor()
    assert.equal(await memory.locator('textarea').inputValue(), '禁止存储也保留 B')
    await memory.close()
    pass('blocked storage keeps per-scope in-memory drafts without a stale render or data loss')

    const reconnect = await open()
    const oldReply = gate(), oldEntered = gate(), oldFinished = gate(); gates.push(oldReply)
    await reconnect.route('**/api/v1/sessions', async route => {
      if (route.request().method() !== 'POST') return route.continue()
      oldEntered.release(); await oldReply.promise
      await route.fulfill({ status: 500, json: { error: 'obsolete connection result' } }); oldFinished.release()
    })
    await button(reconnect, '新对话').click(); await oldEntered.promise
    await button(reconnect, '断开节点').click()
    await button(reconnect, '连接节点').click()
    await reconnect.getByText('在线', { exact: true }).waitFor()
    await reconnect.locator('textarea').fill('同地址新连接不收旧错误')
    oldReply.release(); await oldFinished.promise
    // The controlled old response has been released; yield render frames so a
    // stale React update would become visible, not an arbitrary server timeout.
    await reconnect.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))))
    assert.equal(await reconnect.locator('.wb-creation-message').count(), 0)
    assert.equal(await button(reconnect, '新对话').isEnabled(), true)
    assert.equal(await reconnect.locator('textarea').inputValue(), '同地址新连接不收旧错误')
    await reconnect.close()
    pass('same-origin reconnect invalidates old creation callbacks and locks by connection epoch')

    const stale = await open()
    const historyReply = gate(), historyEntered = gate(), historyFinished = gate(); gates.push(historyReply)
    await stale.route('**/api/v1/sessions/architecture/**', async route => {
      if (!/message-(summaries|window)/.test(route.request().url())) return route.continue()
      const response = await route.fetch(); historyEntered.release(); await historyReply.promise
      await route.fulfill({ response }); historyFinished.release()
    })
    await button(stale, '梳理 Agent 架构').click(); await historyEntered.promise
    await button(stale, '验证 RPC 事件协议').click(); await stale.getByText('在线', { exact: true }).waitFor()
    await stale.locator('textarea').fill('最新选中的会话草稿')
    historyReply.release(); await historyFinished.promise
    await stale.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))))
    assert.equal(await stale.locator('.wb-thread[aria-current="true"]').innerText(), '验证 RPC 事件协议')
    assert.equal(await stale.locator('textarea').inputValue(), '最新选中的会话草稿')
    await stale.close()
    pass('late history cannot revive an old session after selecting another project/session')

    const shots = path.resolve(root, '../../artifacts/confirmed-workbench'); fs.mkdirSync(shots, { recursive: true })
    for (const width of [1440, 390, 320]) for (const scheme of ['light', 'dark']) {
      const shot = await open(width)
      if (scheme === 'dark') await button(shot, '切换主题').click()
      if (width <= 700) await button(shot, '打开侧栏').click()
      await fill(shot, '已有项目', '/node/projects/existing')
      const box = await modal(shot).boundingBox()
      // The confirmed native dialog also has Chromium's calc(100% - 38px) max-width.
      assert.equal(box.width, Math.min(450, width - 38)); assert(box.x >= 12)
      assert.equal(await modal(shot).evaluate(element => element.scrollWidth > element.clientWidth), false)
      assert.equal(await shot.evaluate(() => document.documentElement.scrollWidth > innerWidth + 1), false)
      await shot.screenshot({ path: path.join(shots, `creation-${width}-${scheme}.png`), animations: 'disabled' })
      await shot.close()
    }
    assert.deepEqual(errors, [])
    pass('project dialog geometry, keyboard dismissal, honest capabilities and light/dark mobile layouts')
  } finally {
    for (const held of gates) held.release()
    await browser?.close()
    if (preview.exitCode === null) await new Promise(resolve => { preview.once('exit', resolve); preview.kill('SIGTERM') })
    await fixture.close()
  }
})().catch(error => { console.error(error); process.exitCode = 1 })
