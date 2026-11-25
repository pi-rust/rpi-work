// Real product, isolated node protocol. No workspace writes or host clipboard.
const assert = require('node:assert/strict')
const path = require('node:path'), net = require('node:net'), fs = require('node:fs'), http = require('node:http')
const { spawn } = require('node:child_process')
const { chromium } = require(require.resolve('playwright', { paths: [path.resolve(__dirname, '../../../tools')] }))
const { startFixture } = require('../../../tests/fixtures/ui-fixture.cjs')
const root = path.resolve(__dirname, '..'), artifacts = path.resolve(root, '../../artifacts/tree')
const freePort = () => new Promise(resolve => { const server = net.createServer(); server.listen(0, '127.0.0.1', () => { const port = server.address().port; server.close(() => resolve(port)) }) })
const bounded = async promise => { let timer; try { return await Promise.race([promise, new Promise((_, reject) => { timer = setTimeout(() => reject(Error('HTTP gate timeout')), 10000) })]) } finally { clearTimeout(timer) } }
const entry = (name, kind = 'directory', parent = '') => ({ name, path: parent + name, kind, size: 0 })
const roots = [entry('src'), entry('empty'), entry('README.md', 'file')]
;(async () => {
  const fixture = await startFixture(0, { nativeQueries: true, includeHistoryAssistant: true })
  fixture.controls.fileEntriesByPath = { '': roots, src: [entry('theme.rs', 'file', 'src/')], empty: [] }
  let loads = 0, browser
  const site = http.createServer((request, response) => { loads++; response.writeHead(200, { 'content-type': 'text/html; charset=utf-8' }); response.end('<!doctype html><label>Tree preview<input aria-label="Tree preview"></label>') })
  await new Promise(resolve => site.listen(0, '127.0.0.1', resolve))
  const siteUrl = `http://127.0.0.1:${site.address().port}/`
  const port = await freePort(), origin = `http://127.0.0.1:${port}`
  const preview = spawn(process.execPath, [path.join(root, 'node_modules/vite/bin/vite.js'), 'preview', '--host', '127.0.0.1', '--port', String(port), '--strictPort'], { cwd: root, env: { ...process.env, RPI_NODE_URL: fixture.url }, stdio: 'ignore' })
  const passed = [], failures = []
  try {
    let ready = false
    for (let i = 0; i < 100; i++) { try { if ((await fetch(origin)).ok) { ready = true; break } } catch {} await new Promise(resolve => setTimeout(resolve, 100)) }
    assert(ready); fs.mkdirSync(artifacts, { recursive: true }); browser = await chromium.launch()
    const check = async (name, run, prepare, hasWorkspace = true) => {
      const context = await browser.newContext({ viewport: { width: 1440, height: 940 } }), page = await context.newPage()
      page.setDefaultTimeout(7000)
      const errors = [], gates = [], requests = []
      page.on('pageerror', error => errors.push(error.message))
      page.on('request', request => { if (new URL(request.url()).pathname.endsWith('/files')) requests.push({ path: new URL(request.url()).searchParams.get('path') || '', url: request.url(), method: request.method() }) })
      const panel = page.getByTestId('run-panel'), group = path => panel.getByRole('group', { name: path ? `目录 ${path}` : '项目根目录', exact: true })
      const rendered = () => page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))))
      const gate = () => {
        let enter, release, finish, request
        const entered = new Promise(resolve => { enter = resolve }), held = new Promise(resolve => { release = resolve }), done = new Promise(resolve => { finish = resolve })
        const value = { hold: async (route, response) => { request = route.request(); enter(); try { await held; await route.fulfill(response) } finally { finish() } }, entered: () => bounded(entered), release, done: () => request ? bounded(done) : Promise.resolve(), flush: async () => { const received = page.waitForResponse(response => response.request() === request); release(); await bounded(done); await (await received).finished(); await rendered() } }
        gates.push(value); return value
      }
      const expand = async name => { await panel.getByRole('treeitem', { name, exact: true }).getByText(name, { exact: true }).click() }
      try {
        if (prepare) await prepare({ page, gate })
        await page.goto(origin + '/?live=1')
        if (hasWorkspace) await page.getByText('帮我为工作台增加一个深色主题，并保持手机端可用。', { exact: true }).waitFor()
        else await page.getByText('在线', { exact: true }).waitFor()
        await page.getByRole('button', { name: '文件面板', exact: true }).click()
        if (hasWorkspace) await panel.getByTestId('tab-tree').click()
        await run({ page, panel, group, expand, requests, gate, rendered })
        assert.deepEqual(errors, [], 'uncaught page errors')
        assert(requests.every(request => request.method === 'GET'), 'directory reads stay GET-only')
        passed.push(name); console.log('PASS', name)
      } catch (error) { failures.push(name); console.error('FAIL', name, error.stack, 'page errors:', JSON.stringify(errors)) }
      finally { for (const value of gates) value.release(); for (const value of gates) await value.done(); await context.close() }
    }
    await check('root and lazy child entries use node paths; keyboard opens folders and the changed-file Diff', async ({ panel, group, requests }) => {
      await panel.getByRole('treeitem', { name: 'src', exact: true }).waitFor()
      assert.deepEqual(requests.map(request => request.path), [''])
      await panel.getByRole('treeitem', { name: 'src', exact: true }).press('ArrowRight'); await panel.getByRole('treeitem', { name: 'theme.rs', exact: true }).waitFor()
      assert.deepEqual(requests.map(request => request.path), ['', 'src'])
      await panel.getByRole('treeitem', { name: 'theme.rs', exact: true }).press('Enter')
      await panel.getByText(/let theme = "dark"/).waitFor()
      assert.equal(await group('src').count(), 0)
    })
    await check('connected node without a selected project is unavailable, not an observed empty root', async ({ panel, requests }) => {
      await panel.getByText('文件与 Diff 需要连接真实节点；当前没有工作区。', { exact: true }).waitFor()
      assert.equal(await panel.getByTestId('tab-tree').count(), 0)
      assert.equal(await panel.getByText('项目根目录为空。', { exact: true }).count(), 0)
      assert.deepEqual(requests, [])
    }, async ({ page }) => {
      await page.route(url => url.pathname === '/api/v1/projects', route => route.fulfill({ json: { projects: [] } }))
      await page.route(url => url.pathname === '/api/v1/sessions', route => route.fulfill({ json: { sessions: [] } }))
    }, false)
    await check('held root request is loading, not an empty success or stale files', async ({ panel, group, rendered }) => {
      await group('').getByText('正在读取项目根目录…', { exact: true }).waitFor()
      assert.equal(await panel.getByRole('treeitem').count(), 0)
      assert.equal(await panel.getByText('项目根目录为空。', { exact: true }).count(), 0)
      await rootGate.flush(); await panel.getByRole('treeitem', { name: 'src', exact: true }).waitFor(); await rendered()
      assert.equal(await group('').count(), 0)
    }, async ({ page, gate }) => { rootGate = gate(); await page.route(url => url.pathname.endsWith('/files'), route => rootGate.hold(route, { json: { path: '', entries: roots } })) })
    await check('root failure is local and explicit retry repeats the identical GET only once', async ({ page, panel, group, requests, gate, rendered }) => {
      const error = group('').getByRole('alert'); await error.waitFor()
      assert.match(await error.textContent(), /directory unavailable/)
      assert.equal(await panel.getByText('项目根目录为空。', { exact: true }).count(), 0)
      assert.equal(await page.locator('textarea').locator('..').getByText(/directory unavailable/).count(), 0)
      const held = gate(); await page.route(url => url.pathname.endsWith('/files'), route => held.hold(route, { json: { path: '', entries: roots } }))
      await group('').getByRole('button', { name: '重试读取项目根目录', exact: true }).press('Enter'); await held.entered()
      await group('').getByRole('status').waitFor(); await rendered()
      assert.equal(await group('').getByRole('button').count(), 0); assert.equal(requests.length, 2)
      assert.equal(requests[0].url, requests[1].url)
      await held.flush(); await panel.getByRole('treeitem', { name: 'src', exact: true }).waitFor()
      assert.equal(await group('').count(), 0)
    }, async ({ page }) => { await page.route(url => url.pathname.endsWith('/files'), route => route.fulfill({ status: 503, json: { error: 'directory unavailable' } })) })
    await check('successful empty root is explicit and does not offer a failed-read retry', async ({ panel, group }) => {
      await group('').getByText('项目根目录为空。', { exact: true }).waitFor()
      assert.equal(await panel.getByRole('treeitem').count(), 0); assert.equal(await group('').getByRole('alert').count(), 0); assert.equal(await group('').getByRole('button').count(), 0)
    }, async ({ page }) => { await page.route(url => url.pathname.endsWith('/files'), route => route.fulfill({ json: { path: '', entries: [] } })) })
    await check('child feedback is independent and preserves source; only an explicit retry rereads the failed child', async ({ page, panel, group, expand, requests, rendered }) => {
      await panel.getByRole('treeitem', { name: 'README.md', exact: true }).press('Enter')
      await panel.getByRole('region', { name: '只读文件内容', exact: true }).waitFor()
      await page.evaluate(() => { window.treeSource = document.querySelector('.wb-source-code') })
      await page.route(url => url.pathname.endsWith('/files') && url.searchParams.get('path') === 'src', route => route.fulfill({ status: 503, json: { error: 'child unavailable' } }))
      await expand('src'); await group('src').getByRole('alert').waitFor()
      await expand('empty'); await group('empty').getByRole('status').waitFor()
      await group('src').getByRole('alert').waitFor()
      await panel.getByRole('treeitem', { name: 'README.md', exact: true }).waitFor()
      await expand('src'); assert.equal(await group('src').count(), 0)
      await expand('src'); await group('src').getByRole('alert').waitFor(); await rendered(); assert.equal(requests.length, 3)
      await page.route(url => url.pathname.endsWith('/files') && url.searchParams.get('path') === 'src', route => route.fulfill({ json: { path: 'src', entries: fixture.controls.fileEntriesByPath.src } }))
      await group('src').getByRole('button', { name: '重试读取目录 src', exact: true }).click()
      await panel.getByRole('treeitem', { name: 'theme.rs', exact: true }).waitFor()
      assert.deepEqual(requests.map(request => request.path), ['', 'src', 'empty', 'src']); assert.equal(requests[1].url, requests[3].url)
      assert.equal(await page.evaluate(() => treeSource === document.querySelector('.wb-source-code')), true)
      await group('empty').getByRole('status').waitFor()
    })
    await check('directory toolbar refresh is distinct from Git and rereads the root and open directories only', async ({ page, panel, group, expand, requests, gate, rendered }) => {
      await panel.getByRole('treeitem', { name: 'src', exact: true }).waitFor()
      const refresh = panel.getByRole('button', { name: '刷新目录', exact: true })
      assert.equal(await refresh.count(), 1, 'directory view needs its own refresh action')
      assert.equal(await panel.getByRole('button', { name: '刷新改动', exact: true }).count(), 0)
      const root = gate(), child = gate(), nested = gate(), observed = []
      const nestedDir = entry('nested', 'directory', 'src/')
      const oldFile = entry('before.txt', 'file', 'src/nested/')
      let refreshing = false
      await page.route(url => url.pathname.endsWith('/files'), route => {
        const pathname = new URL(route.request().url()).searchParams.get('path') || ''
        if (!refreshing && pathname === 'src') return route.fulfill({ json: { path: 'src', entries: [entry('theme.rs', 'file', 'src/'), nestedDir] } })
        if (!refreshing && pathname === 'src/nested') return route.fulfill({ json: { path: 'src/nested', entries: [oldFile] } })
        if (refreshing && pathname === '') return root.hold(route, { json: { path: '', entries: [entry('src'), entry('new.txt', 'file'), entry('new-dir'), entry('empty')] } })
        if (refreshing && pathname === 'src') return child.hold(route, { json: { path: 'src', entries: [entry('new.rs', 'file', 'src/'), nestedDir] } })
        if (refreshing && pathname === 'src/nested') return nested.hold(route, { json: { path: 'src/nested', entries: [entry('after.txt', 'file', 'src/nested/')] } })
        return route.continue()
      })
      await expand('empty'); await group('empty').getByRole('status').waitFor(); await expand('empty')
      await expand('src'); await expand('nested')
      await panel.getByRole('treeitem', { name: 'before.txt', exact: true }).waitFor()
      const initialPaths = ['', 'empty', 'src', 'src/nested']
      assert.deepEqual(requests.map(request => request.path), initialPaths)
      page.on('request', request => { if (new URL(request.url()).pathname.includes('/git/')) observed.push(request.url()) })
      refreshing = true
      await refresh.press('Enter'); await root.entered()
      await group('').getByRole('status').filter({ hasText: '正在刷新' }).waitFor()
      await panel.getByRole('treeitem', { name: 'README.md', exact: true }).waitFor()
      await refresh.press('Space'); await rendered()
      assert.deepEqual(requests.map(request => request.path), [...initialPaths, ''])
      await root.flush(); await child.entered()
      await panel.getByRole('treeitem', { name: 'new.txt', exact: true }).waitFor()
      await panel.getByRole('treeitem', { name: 'theme.rs', exact: true }).waitFor()
      assert.deepEqual(requests.map(request => request.path), [...initialPaths, '', 'src'])
      await child.flush(); await nested.entered()
      await panel.getByRole('treeitem', { name: 'new.rs', exact: true }).waitFor()
      await panel.getByRole('treeitem', { name: 'before.txt', exact: true }).waitFor()
      await nested.flush(); await panel.getByRole('treeitem', { name: 'after.txt', exact: true }).waitFor()
      for (const name of ['src', 'nested']) assert.equal(await panel.getByRole('treeitem', { name, exact: true }).getAttribute('aria-expanded'), 'true')
      for (const name of ['README.md', 'theme.rs', 'before.txt']) assert.equal(await panel.getByRole('treeitem', { name, exact: true }).count(), 0)
      await expand('empty'); await group('empty').getByRole('status').waitFor(); await rendered()
      assert.deepEqual(requests.map(request => request.path), [...initialPaths, '', 'src', 'src/nested'])
      assert.deepEqual(requests.slice(4).map(request => request.url), [requests[0].url, requests[2].url, requests[3].url])
      assert.deepEqual(observed, [], 'refreshing the directory never asks Git')
      await panel.getByTestId('tab-changes').click()
      await panel.getByRole('button', { name: '刷新改动', exact: true }).waitFor()
      assert.equal(await refresh.count(), 0)
    })
    await check('refresh failures retain the last listing and source; local retry only reads its own directory', async ({ page, panel, group, expand, requests, rendered }) => {
      const src = panel.getByRole('treeitem', { name: 'src', exact: true })
      await src.waitFor(); await expand('src')
      await panel.getByRole('treeitem', { name: 'theme.rs', exact: true }).waitFor()
      await panel.getByRole('treeitem', { name: 'README.md', exact: true }).press('Enter')
      const source = panel.getByRole('region', { name: '只读文件内容', exact: true }); await source.waitFor()
      await page.evaluate(() => { window.treeSource = document.querySelector('.wb-source-code') })
      const refresh = panel.getByRole('button', { name: '刷新目录', exact: true })
      const listing = url => url.pathname.endsWith('/files')
      const failure = { status: 503, json: { error: 'refresh unavailable' } }
      let rootFails = true, childFails = true
      await page.route(listing, route => {
        const pathname = new URL(route.request().url()).searchParams.get('path') || ''
        if (pathname === '' && rootFails || pathname === 'src' && childFails) return route.fulfill(failure)
        if (pathname === 'src') return route.fulfill({ json: { path: 'src', entries: [] } })
        return route.continue()
      })
      await refresh.click(); await group('').getByRole('alert').filter({ hasText: '保留上次读取结果' }).waitFor(); await rendered()
      await panel.getByRole('treeitem', { name: 'theme.rs', exact: true }).waitFor()
      assert.deepEqual(requests.map(request => request.path), ['', 'src', ''])
      rootFails = false
      await refresh.click(); await group('src').getByRole('alert').filter({ hasText: '保留上次读取结果' }).waitFor()
      assert.equal(await group('').count(), 0)
      await expand('src'); assert.equal(await group('src').count(), 0)
      await expand('src'); await group('src').getByRole('alert').waitFor(); await rendered()
      assert.deepEqual(requests.map(request => request.path), ['', 'src', '', '', 'src'])
      await panel.getByRole('treeitem', { name: 'theme.rs', exact: true }).waitFor()
      childFails = false
      await group('src').getByRole('button', { name: '重试读取目录 src', exact: true }).click()
      await group('src').getByText('目录 src 为空。', { exact: true }).waitFor()
      assert.equal(await panel.getByRole('treeitem', { name: 'theme.rs', exact: true }).count(), 0)
      assert.deepEqual(requests.map(request => request.path), ['', 'src', '', '', 'src', 'src'])
      assert.equal(await page.evaluate(() => treeSource === document.querySelector('.wb-source-code')), true)
    })
    await check('removed directories lose their expansion and cache even if their old GET settles after the path returns', async ({ page, panel, group, expand, requests, gate, rendered }) => {
      const src = panel.getByRole('treeitem', { name: 'src', exact: true })
      await src.waitFor()
      const old = gate(), fresh = gate()
      let rootEntries = [entry('README.md', 'file')], childReads = 0
      await page.route(url => url.pathname.endsWith('/files'), route => {
        const pathname = new URL(route.request().url()).searchParams.get('path') || ''
        if (pathname === '') return route.fulfill({ json: { path: '', entries: rootEntries } })
        return ++childReads === 1
          ? old.hold(route, { json: { path: 'src', entries: [entry('old.txt', 'file', 'src/')] } })
          : fresh.hold(route, { json: { path: 'src', entries: [entry('fresh.txt', 'file', 'src/')] } })
      })
      await expand('src'); await old.entered()
      const refresh = panel.getByRole('button', { name: '刷新目录', exact: true })
      await refresh.click(); await src.waitFor({ state: 'detached' }); await rendered()
      assert.equal(await group('src').count(), 0)
      rootEntries = roots
      await refresh.click(); await src.waitFor()
      assert.equal(await src.getAttribute('aria-expanded'), 'false')
      assert.deepEqual(requests.map(request => request.path), ['', 'src', '', ''])
      await src.press('ArrowRight'); await fresh.entered()
      await old.flush(); await group('src').getByRole('status').waitFor()
      assert.equal(await panel.getByRole('treeitem', { name: 'old.txt', exact: true }).count(), 0)
      await fresh.flush(); await panel.getByRole('treeitem', { name: 'fresh.txt', exact: true }).waitFor()
      assert.deepEqual(requests.map(request => request.path), ['', 'src', '', '', 'src'])
    })
    await check('late refresh from a previous session cannot publish entries or continue refreshing its children', async ({ page, panel, group, expand, requests, gate, rendered }) => {
      await panel.getByRole('treeitem', { name: 'src', exact: true }).waitFor()
      await expand('src'); await panel.getByRole('treeitem', { name: 'theme.rs', exact: true }).waitFor()
      const old = gate(), current = gate()
      let rootsRequested = 0
      await page.route(url => url.pathname.endsWith('/files') && !url.searchParams.get('path'), route => ++rootsRequested === 1
        ? old.hold(route, { json: { path: '', entries: [...roots, entry('OLD.txt', 'file')] } })
        : current.hold(route, { json: { path: '', entries: [entry('CURRENT.txt', 'file')] } }))
      await panel.getByRole('button', { name: '刷新目录', exact: true }).click(); await old.entered()
      await page.getByRole('button', { name: '梳理 Agent 架构', exact: true }).click()
      await panel.getByTestId('tab-tree').click(); await current.entered()
      await old.flush(); await group('').getByText('正在读取项目根目录…', { exact: true }).waitFor()
      await current.flush(); await panel.getByRole('treeitem', { name: 'CURRENT.txt', exact: true }).waitFor(); await rendered()
      assert.equal(await panel.getByRole('treeitem', { name: 'OLD.txt', exact: true }).count(), 0)
      assert.deepEqual(requests.map(request => request.path), ['', 'src', '', ''])
    })
    await check('empty child feedback follows expansion and reuses the successful cached listing', async ({ group, expand, requests, rendered }) => {
      await expand('empty'); await group('empty').getByText('目录 empty 为空。', { exact: true }).waitFor()
      await expand('empty'); assert.equal(await group('empty').count(), 0)
      await expand('empty'); await group('empty').getByRole('status').waitFor(); await rendered()
      assert.deepEqual(requests.map(request => request.path), ['', 'empty'])
    })
    for (const rejected of [false, true]) await check(`late same-path root ${rejected ? 'failure' : 'success'} cannot clear the new session loading or enter its tree`, async ({ page, panel, group, rendered }) => {
      await scopeGate.entered()
      await page.getByRole('button', { name: '梳理 Agent 架构', exact: true }).click()
      await panel.getByTestId('tab-tree').click(); await currentGate.entered()
      await group('').getByRole('status').waitFor()
      await scopeGate.flush(); await rendered()
      await group('').getByRole('status').waitFor()
      assert.equal(await panel.getByRole('treeitem').count(), 0); assert.equal(await panel.getByRole('alert').count(), 0)
      await currentGate.flush(); await panel.getByRole('treeitem', { name: 'CURRENT.txt', exact: true }).waitFor()
      assert.equal(await panel.getByRole('treeitem', { name: 'OLD.txt', exact: true }).count(), 0)
      assert.equal(await group('').count(), 0)
    }, async ({ page, gate }) => { scopeGate = gate(); currentGate = gate(); let calls = 0; await page.route(url => url.pathname.endsWith('/files'), route => ++calls === 1 ? scopeGate.hold(route, rejected ? { status: 503, json: { error: 'old scope failure' } } : { json: { path: '', entries: [entry('OLD.txt', 'file')] } }) : currentGate.hold(route, { json: { path: '', entries: [entry('CURRENT.txt', 'file')] } })) })
    await check('child retry and directory refresh preserve draft selection and the real iframe across themes and widths', async ({ page, panel, group, expand, requests, rendered, gate }) => {
      await page.getByRole('button', { name: '浏览器', exact: true }).click()
      await page.getByRole('textbox', { name: '预览地址', exact: true }).fill(siteUrl); await page.getByRole('button', { name: '打开', exact: true }).click()
      const input = page.frameLocator('.real-browser iframe').getByLabel('Tree preview'); await input.fill('keep directory preview')
      const initialLoads = loads; await page.evaluate(() => { window.treeFrame = document.querySelector('.real-browser iframe') })
      const draft = page.locator('textarea'); await draft.fill('目录读取草稿 selection'); await draft.evaluate(node => node.setSelectionRange(2, 6))
      for (const dark of [false, true]) {
        await page.setViewportSize({ width: 1440, height: 940 }); if (dark) await page.getByRole('button', { name: '切换主题', exact: true }).click()
        for (const width of [1440, 320]) {
          const wasOverlay = await page.evaluate(() => matchMedia('(max-width: 900px)').matches)
          await page.setViewportSize({ width, height: 940 }); if (width <= 900 && !wasOverlay) await panel.waitFor({ state: 'hidden' })
          if (await panel.isVisible() && width <= 900) await panel.getByRole('button', { name: '关闭浏览器面板', exact: true }).click()
          await page.getByRole('button', { name: '文件面板', exact: true }).click()
          await panel.getByTestId('tab-tree').click()
          const src = panel.getByRole('treeitem', { name: 'src', exact: true }); await src.waitFor()
          const firstVisit = !dark && width === 1440
          assert.equal(await src.getAttribute('aria-expanded'), firstVisit ? 'false' : 'true')
          if (firstVisit) await expand('src')
          await group('src').getByRole('alert').waitFor()
          const button = group('src').getByRole('button', { name: '重试读取目录 src', exact: true })
          await button.focus(); await page.keyboard.press('Tab'); await page.keyboard.press('Shift+Tab'); await rendered()
          assert.equal(await button.evaluate(node => node === document.activeElement), true)
          await page.waitForFunction(() => getComputedStyle(document.activeElement).outlineOffset === '-2px')
          assert.deepEqual(await button.evaluate(node => { const css = getComputedStyle(node); return [css.outlineStyle, css.outlineOffset] }), ['solid', '-2px'])
          if (width === 320) { const box = await button.boundingBox(); assert(box.height >= 44 && box.width >= 44) }
          assert.equal(await page.evaluate(() => document.documentElement.scrollWidth), width)
          await page.screenshot({ path: path.join(artifacts, `${dark ? 'dark' : 'light'}-${width}.png`) })
          const pending = gate(); await page.route(url => url.pathname.endsWith('/files') && url.searchParams.get('path') === 'src', route => pending.hold(route, { status: 503, json: { error: 'child unavailable' } }))
          await button.press('Space'); await pending.entered(); await group('src').getByRole('status').waitFor()
          await panel.getByRole('treeitem', { name: 'README.md', exact: true }).waitFor()
          await pending.flush(); await group('src').getByRole('alert').waitFor()
          await page.unroute(url => url.pathname.endsWith('/files') && url.searchParams.get('path') === 'src')
          await page.route(url => url.pathname.endsWith('/files') && url.searchParams.get('path') === 'src', route => route.fulfill({ status: 503, json: { error: 'child unavailable' } }))
          const refresh = panel.getByRole('button', { name: '刷新目录', exact: true })
          await refresh.focus(); await page.keyboard.press('Tab'); await page.keyboard.press('Shift+Tab')
          assert.equal(await refresh.evaluate(node => node === document.activeElement), true)
          await page.waitForFunction(() => getComputedStyle(document.activeElement).outlineOffset === '-2px')
          assert.equal(await refresh.evaluate(node => getComputedStyle(node).outlineStyle), 'solid')
          if (width === 320) { const box = await refresh.boundingBox(); assert(box.height >= 44 && box.width >= 44) }
          const refreshRoot = gate()
          const rootListing = url => url.pathname.endsWith('/files') && !url.searchParams.get('path')
          await page.route(rootListing, route => refreshRoot.hold(route, { json: { path: '', entries: roots } }))
          const before = requests.length
          await refresh.press('Space'); await refreshRoot.entered()
          await group('').getByRole('status').filter({ hasText: '正在刷新' }).waitFor()
          assert.equal(await refresh.getAttribute('aria-disabled'), 'true')
          assert.equal(await refresh.evaluate(node => node === document.activeElement), true)
          assert.equal(await page.evaluate(() => document.documentElement.scrollWidth), width)
          await page.screenshot({ path: path.join(artifacts, `refresh-${dark ? 'dark' : 'light'}-${width}.png`) })
          await refreshRoot.flush(); await group('src').getByRole('alert').waitFor(); await rendered()
          assert.deepEqual(requests.slice(before).map(request => request.path), ['', 'src'])
          assert.equal(await refresh.getAttribute('aria-disabled'), null)
          assert.equal(await refresh.evaluate(node => node === document.activeElement), true)
          await page.unroute(rootListing)
          // Returning to browser unmounts only files, never its iframe.
          if (width === 320) await panel.getByRole('button', { name: '关闭面板', exact: true }).click()
          await page.getByRole('button', { name: '浏览器', exact: true }).click()
        }
      }
      assert.equal(await draft.inputValue(), '目录读取草稿 selection'); assert.deepEqual(await draft.evaluate(node => [node.selectionStart, node.selectionEnd]), [2, 6])
      assert.equal(await page.evaluate(() => treeFrame === document.querySelector('.real-browser iframe')), true); assert.equal(await input.inputValue(), 'keep directory preview'); assert.equal(loads, initialLoads)
      assert(requests.some(request => request.path === 'src'))
    }, async ({ page }) => { await page.route(url => url.pathname.endsWith('/files') && url.searchParams.get('path') === 'src', route => route.fulfill({ status: 503, json: { error: 'child unavailable' } })) })
    await check('reload restores only visible expanded branches after their parent listings arrive', async ({ page, panel, group, expand, requests, gate, rendered }) => {
      const nested = entry('nested', 'directory', 'src/'), leaf = entry('leaf.txt', 'file', 'src/nested/')
      const root = gate(), child = gate(), grandchild = gate()
      let restoring = false
      const listing = url => url.pathname.endsWith('/files')
      await page.route(listing, route => {
        const dir = new URL(route.request().url()).searchParams.get('path') || ''
        if (dir === '' && restoring) return root.hold(route, { json: { path: '', entries: roots } })
        if (dir === 'src') return restoring ? child.hold(route, { json: { path: dir, entries: [nested] } }) : route.fulfill({ json: { path: dir, entries: [nested] } })
        if (dir === 'src/nested') return restoring ? grandchild.hold(route, { json: { path: dir, entries: [leaf] } }) : route.fulfill({ json: { path: dir, entries: [leaf] } })
        return route.continue()
      })
      await expand('src'); await expand('nested')
      await panel.getByRole('treeitem', { name: 'leaf.txt', exact: true }).waitFor()
      await expand('empty'); await group('empty').getByRole('status').waitFor(); await expand('empty')
      assert.deepEqual(requests.map(request => request.path), ['', 'src', 'src/nested', 'empty'])
      restoring = true
      const before = requests.length
      await page.reload()
      await page.getByText('帮我为工作台增加一个深色主题，并保持手机端可用。', { exact: true }).waitFor()
      await page.getByRole('button', { name: '文件面板', exact: true }).click(); await rendered()
      assert.equal(requests.length, before)
      await panel.getByTestId('tab-tree').click(); await root.entered()
      await group('').getByRole('status').waitFor()
      await panel.getByTestId('tab-changes').click(); await root.flush()
      assert.deepEqual(requests.slice(before).map(request => request.path), [''])
      await panel.getByTestId('tab-tree').click()
      const src = panel.getByRole('treeitem', { name: 'src', exact: true }); await src.waitFor()
      assert.equal(await src.getAttribute('aria-expanded'), 'true', 'directory expansion survives a browser reload')
      await child.entered(); await group('src').getByRole('status').waitFor()
      await page.setViewportSize({ width: 320, height: 940 }); await panel.waitFor({ state: 'hidden' })
      await child.flush()
      assert.deepEqual(requests.slice(before).map(request => request.path), ['', 'src'])
      await page.getByRole('button', { name: '文件面板', exact: true }).click(); await grandchild.entered()
      await group('src/nested').getByRole('status').waitFor(); await grandchild.flush()
      await panel.getByRole('treeitem', { name: 'leaf.txt', exact: true }).waitFor()
      assert.equal(await panel.getByRole('treeitem', { name: 'empty', exact: true }).getAttribute('aria-expanded'), 'false')
      assert.deepEqual(requests.slice(before).map(request => request.url), requests.slice(0, 3).map(request => request.url))
      assert.equal(await page.evaluate(() => document.documentElement.scrollWidth), 320)
      await page.screenshot({ path: path.join(artifacts, 'restored-light-320.png') })
    })
    await check('collapsed ancestors defer descendant reads and a failed parent keeps the saved expansion for explicit retry', async ({ page, panel, group, expand, requests, rendered }) => {
      const nested = entry('nested', 'directory', 'src/'), leaf = entry('leaf.txt', 'file', 'src/nested/')
      let failParent = false
      await page.route(url => url.pathname.endsWith('/files') && Boolean(url.searchParams.get('path')), route => {
        const dir = new URL(route.request().url()).searchParams.get('path')
        if (dir === 'src' && failParent) return route.fulfill({ status: 503, json: { error: 'saved parent unavailable' } })
        return route.fulfill({ json: { path: dir, entries: dir === 'src' ? [nested] : [leaf] } })
      })
      await expand('src'); await expand('nested')
      await panel.getByRole('treeitem', { name: 'leaf.txt', exact: true }).waitFor()
      await expand('src'); await panel.getByRole('treeitem', { name: 'nested', exact: true }).waitFor({ state: 'detached' })
      const before = requests.length
      await page.reload()
      await page.getByText('帮我为工作台增加一个深色主题，并保持手机端可用。', { exact: true }).waitFor()
      await page.getByRole('button', { name: '文件面板', exact: true }).click()
      await panel.getByTestId('tab-tree').click()
      const src = panel.getByRole('treeitem', { name: 'src', exact: true }); await src.waitFor(); await rendered()
      assert.equal(await src.getAttribute('aria-expanded'), 'false')
      assert.deepEqual(requests.slice(before).map(request => request.path), [''])
      failParent = true
      await src.press('ArrowRight'); await group('src').getByRole('alert').filter({ hasText: 'saved parent unavailable' }).waitFor()
      await expand('src'); await expand('src'); await group('src').getByRole('alert').waitFor(); await rendered()
      assert.deepEqual(requests.slice(before).map(request => request.path), ['', 'src'])
      failParent = false
      await group('src').getByRole('button', { name: '重试读取目录 src', exact: true }).press('Enter')
      await panel.getByRole('treeitem', { name: 'leaf.txt', exact: true }).waitFor()
      assert.deepEqual(requests.slice(before).map(request => request.path), ['', 'src', 'src', 'src/nested'])
      assert.equal(requests[before + 1].url, requests[before + 2].url)
    })
    await check('a directory retyped during reload drops its saved descendants before the path returns', async ({ page, panel, expand, requests, rendered }) => {
      const nested = entry('nested', 'directory', 'src/')
      let retyped = false
      await page.route(url => url.pathname.endsWith('/files'), route => {
        const dir = new URL(route.request().url()).searchParams.get('path') || ''
        const entries = dir === '' ? (retyped ? [entry('src', 'file'), entry('README.md', 'file')] : roots)
          : dir === 'src' ? [nested] : [entry('leaf.txt', 'file', 'src/nested/')]
        return route.fulfill({ json: { path: dir, entries } })
      })
      await expand('src'); await expand('nested')
      await panel.getByRole('treeitem', { name: 'leaf.txt', exact: true }).waitFor()
      await expand('src')
      retyped = true
      const before = requests.length
      await page.reload()
      await page.getByText('帮我为工作台增加一个深色主题，并保持手机端可用。', { exact: true }).waitFor()
      await page.getByRole('button', { name: '文件面板', exact: true }).click(); await panel.getByTestId('tab-tree').click()
      const src = panel.getByRole('treeitem', { name: 'src', exact: true }); await src.waitFor()
      assert.equal(await src.getAttribute('aria-expanded'), null)
      assert.deepEqual(requests.slice(before).map(request => request.path), [''])
      retyped = false
      await page.reload()
      await page.getByText('帮我为工作台增加一个深色主题，并保持手机端可用。', { exact: true }).waitFor()
      await page.getByRole('button', { name: '文件面板', exact: true }).click(); await panel.getByTestId('tab-tree').click()
      await src.waitFor()
      assert.equal(await src.getAttribute('aria-expanded'), 'false')
      await expand('src')
      const nestedRow = panel.getByRole('treeitem', { name: 'nested', exact: true }); await nestedRow.waitFor(); await rendered()
      assert.equal(await nestedRow.getAttribute('aria-expanded'), 'false')
      assert.deepEqual(requests.slice(before).map(request => request.path), ['', '', 'src'])
    })
    await check('identical directory paths across sessions restore their own expansion when storage is unavailable', async ({ page, panel, group, expand, requests }) => {
      await expand('src'); await panel.getByRole('treeitem', { name: 'theme.rs', exact: true }).waitFor()
      const architecture = page.getByRole('button', { name: '梳理 Agent 架构', exact: true })
      const dark = page.getByRole('button', { name: '为工作台添加深色模式', exact: true })
      const treeTab = panel.getByTestId('tab-tree')
      const src = panel.getByRole('treeitem', { name: 'src', exact: true })
      await architecture.click(); await treeTab.click(); await src.waitFor()
      assert.equal(await src.getAttribute('aria-expanded'), 'false')
      await expand('empty'); await group('empty').getByRole('status').waitFor()
      await dark.click(); await treeTab.click(); await panel.getByRole('treeitem', { name: 'theme.rs', exact: true }).waitFor()
      const empty = panel.getByRole('treeitem', { name: 'empty', exact: true })
      assert.equal(await empty.getAttribute('aria-expanded'), 'false')
      await architecture.click(); await treeTab.click(); await group('empty').getByRole('status').waitFor()
      assert.equal(await src.getAttribute('aria-expanded'), 'false')
      assert.deepEqual(requests.map(request => request.path), ['', 'src', '', 'empty', '', 'src', '', 'empty'])
      assert.equal(await page.evaluate(() => localStorage.length), 0)
    }, async ({ page }) => { await page.addInitScript(() => { Storage.prototype.setItem = () => { throw Error('storage unavailable') } }) })
    await check('different nodes with the same project, session and paths restore separate expansions after reload', async ({ page, panel, group, expand, requests }) => {
      const other = await startFixture(0, { nativeQueries: true, includeHistoryAssistant: true })
      other.controls.fileEntriesByPath = { '': roots, src: [entry('other.rs', 'file', 'src/')], empty: [] }
      const darkPrompt = page.getByText('帮我为工作台增加一个深色主题，并保持手机端可用。', { exact: true })
      const openTree = async () => {
        await page.getByRole('button', { name: '文件面板', exact: true }).click()
        await panel.getByTestId('tab-tree').click()
      }
      const connectOther = async () => {
        await page.getByRole('button', { name: '断开节点', exact: true }).click()
        await page.getByRole('textbox', { name: '节点地址', exact: true }).fill(other.url)
        await page.getByRole('button', { name: '连接节点', exact: true }).click()
        await darkPrompt.waitFor()
        await openTree()
      }
      const src = panel.getByRole('treeitem', { name: 'src', exact: true })
      const empty = panel.getByRole('treeitem', { name: 'empty', exact: true })
      try {
        await expand('src'); await panel.getByRole('treeitem', { name: 'theme.rs', exact: true }).waitFor()
        await connectOther(); await src.waitFor()
        assert.equal(await src.getAttribute('aria-expanded'), 'false')
        await expand('empty'); await group('empty').getByRole('status').waitFor()
        await page.reload(); await darkPrompt.waitFor(); await openTree()
        await panel.getByRole('treeitem', { name: 'theme.rs', exact: true }).waitFor()
        assert.equal(await empty.getAttribute('aria-expanded'), 'false')
        await connectOther(); await group('empty').getByRole('status').waitFor()
        assert.equal(await src.getAttribute('aria-expanded'), 'false')
        assert.deepEqual(requests.map(request => request.path), ['', 'src', '', 'empty', '', 'src', '', 'empty'])
      } finally { await other.close() }
    })
    await check('directory continuation keeps loaded rows, retries the same cursor, and refreshes its loaded range', async ({ page, panel, group, requests, gate, rendered }) => {
      const row = name => panel.getByRole('treeitem', { name, exact: true })
      const held = gate(), failed = gate()
      const root = group('')
      const loadMore = root.getByRole('button', { name: '加载更多项目根目录文件', exact: true })
      const refresh = panel.getByRole('button', { name: '刷新目录', exact: true })
      const response = (entries, nextCursor) => ({ json: { path: '', entries, pagination: { complete: !nextCursor, nextCursor } } })
      let revision = 'original', attempts = 0
      await row('initial.txt').waitFor()
      await page.route(url => url.pathname.endsWith('/files'), route => {
        const cursor = new URL(route.request().url()).searchParams.get('cursor')
        if (!cursor) {
          const name = revision === 'original' ? 'initial.txt' : 'refreshed.txt'
          return route.fulfill(response([entry(name, 'file')], revision))
        }
        if (cursor === 'original') return ++attempts === 1
          ? failed.hold(route, { status: 503, json: { error: 'temporarily unavailable' } })
          : held.hold(route, response([entry('last.txt', 'file')]))
        return route.fulfill(response([entry('replacement.txt', 'file')]))
      })
      await refresh.click()
      await loadMore.waitFor()
      await loadMore.press('Enter'); await failed.entered()
      await root.getByRole('status').filter({ hasText: '正在加载' }).waitFor()
      await row('initial.txt').waitFor()
      const count = requests.length
      await loadMore.evaluate(button => button.click()); await rendered()
      assert.equal(requests.length, count)
      await failed.flush()
      await root.getByRole('alert').filter({ hasText: '后续页读取失败' }).waitFor()
      const retry = root.getByRole('button', { name: '重试加载项目根目录文件', exact: true })
      await retry.press('Enter'); await held.entered()
      assert.equal(requests.at(-1).url, requests.at(-2).url)
      await held.flush(); await row('last.txt').waitFor()
      assert.equal(await root.count(), 0)
      await page.waitForFunction(() => document.activeElement?.getAttribute('aria-label') === '刷新目录')
      await row('initial.txt').waitFor()
      revision = 'updated'
      await refresh.press('Enter'); await row('replacement.txt').waitFor()
      assert.equal(await row('last.txt').count(), 0)
      assert.deepEqual(requests.slice(-2).map(request => new URL(request.url).searchParams.get('cursor')), [null, 'updated'])
      assert(requests.every(request => new URL(request.url).searchParams.get('page') === '1'))
    }, async ({ page }) => {
      await page.route(url => url.pathname.endsWith('/files'), route => route.fulfill({ json: { path: '', entries: [entry('initial.txt', 'file')] } }))
    })
    await check('partial parent listing restores a saved expanded folder from a later page', async ({ page, panel, requests, expand }) => {
      await panel.getByRole('treeitem', { name: 'src', exact: true }).waitFor()
      await expand('src')
      await panel.getByRole('treeitem', { name: 'theme.rs', exact: true }).waitFor()
      await page.route(url => url.pathname.endsWith('/files') && !url.searchParams.get('path'), route => {
        const cursor = new URL(route.request().url()).searchParams.get('cursor')
        return route.fulfill({ json: { path: '', entries: cursor ? [entry('src')] : [entry('alpha')],
          pagination: cursor ? { complete: true } : { complete: false, nextCursor: 'later' } } })
      })
      await page.reload()
      await page.getByText('帮我为工作台增加一个深色主题，并保持手机端可用。', { exact: true }).waitFor()
      await page.getByRole('button', { name: '文件面板', exact: true }).click()
      await panel.getByTestId('tab-tree').click()
      await panel.getByRole('treeitem', { name: 'theme.rs', exact: true }).waitFor()
      assert.equal(await panel.getByRole('treeitem', { name: 'src', exact: true }).getAttribute('aria-expanded'), 'true')
      assert.deepEqual(requests.slice(-3).map(request => [request.path, new URL(request.url).searchParams.get('cursor')]), [['', null], ['', 'later'], ['src', null]])
    })
    await check('large directory windows retain the full keyboard order and exact file paths', async ({ page, panel, requests, rendered }) => {
      const row = name => panel.getByRole('treeitem', { name, exact: true })
      const first = row('large-0000.txt'), last = row('large-1999.txt')
      await first.waitFor()
      assert(await panel.getByRole('treeitem').count() < 40, 'a large directory renders only a bounded window of rows')
      assert.deepEqual([await first.getAttribute('aria-setsize'), await first.getAttribute('aria-posinset')], ['2001', '2'])
      await first.focus(); await page.keyboard.press('End'); await last.waitFor()
      await page.waitForFunction(() => document.activeElement?.getAttribute('aria-label') === 'large-1999.txt')
      assert.deepEqual([await last.getAttribute('aria-setsize'), await last.getAttribute('aria-posinset')], ['2001', '2001'])
      await page.keyboard.press('ArrowUp')
      await page.waitForFunction(() => document.activeElement?.getAttribute('aria-label') === 'large-1998.txt')
      const fileRequest = page.waitForRequest(request => {
        const url = new URL(request.url())
        return request.method() === 'GET' && url.pathname.endsWith('/file') && url.searchParams.get('path') === 'large-1998.txt'
      })
      await page.keyboard.press('Enter'); await fileRequest
      await panel.getByRole('region', { name: '只读文件内容', exact: true }).waitFor()
      await page.keyboard.press('Shift+Tab')
      await page.waitForFunction(() => document.activeElement?.getAttribute('aria-label') === '刷新目录')
      await page.keyboard.press('Tab')
      await page.waitForFunction(() => document.activeElement?.getAttribute('aria-label') === 'large-1998.txt')
      await page.keyboard.press('Home'); await row('src').waitFor()
      await page.waitForFunction(() => document.activeElement?.getAttribute('aria-label') === 'src')
      await page.keyboard.press('ArrowRight')
      await page.waitForFunction(() => {
        const viewport = document.querySelector('.wb-directory-viewport')
        const folder = document.querySelector('[role="treeitem"][aria-label="src"]')
        return folder?.getAttribute('aria-expanded') === 'true' &&
          folder.getBoundingClientRect().top < viewport.getBoundingClientRect().bottom
      })
      await row('theme.rs').waitFor()
      await page.keyboard.press('ArrowDown')
      await page.waitForFunction(() => document.activeElement?.getAttribute('aria-label') === 'theme.rs')
      assert.deepEqual([await row('theme.rs').getAttribute('aria-level'), await row('theme.rs').getAttribute('aria-setsize')], ['2', '1'])
      await page.keyboard.press('ArrowLeft')
      await page.waitForFunction(() => document.activeElement?.getAttribute('aria-label') === 'src')
      await page.keyboard.press('ArrowLeft'); await row('theme.rs').waitFor({ state: 'detached' })
      await rendered(); assert.deepEqual(requests.map(request => request.path), ['', 'src'])
    }, async ({ page }) => {
      const contents = [entry('src'), ...Array.from({ length: 2000 }, (_, i) => entry(`large-${String(i).padStart(4, '0')}.txt`, 'file'))]
      await page.route(url => url.pathname.endsWith('/files') && !url.searchParams.get('path'), route => route.fulfill({ json: { path: '', entries: contents } }))
    })
    await check('mobile scrolling windows long names and keeps directory feedback and source independent', async ({ page, panel, group, requests, gate, rendered }) => {
      const row = name => panel.getByRole('treeitem', { name, exact: true })
      const scroll = panel.locator('.wb-directory-viewport')
      const largeFolder = row('src')
      const lastName = `${'很长的目录文件名'.repeat(10)}.txt`
      const large = [entry('src'), ...Array.from({ length: 900 }, (_, i) => entry(`record-${i}.txt`, 'file')), entry(lastName, 'file')]
      const child = gate()
      let largeRoot = true
      await page.route(url => url.pathname.endsWith('/files'), route => {
        const dir = new URL(route.request().url()).searchParams.get('path') || ''
        if (dir === 'src') {
          const response = { status: 503, json: { error: 'nested listing unavailable' } }
          return largeRoot ? child.hold(route, response) : route.fulfill(response)
        }
        return route.fulfill({ json: { path: '', entries: largeRoot ? large : roots } })
      })
      await page.getByRole('button', { name: '浏览器', exact: true }).click()
      await page.getByRole('button', { name: '切换主题', exact: true }).click()
      await page.setViewportSize({ width: 320, height: 940 }); await panel.waitFor({ state: 'hidden' })
      await page.getByRole('button', { name: '文件面板', exact: true }).click(); await panel.getByTestId('tab-tree').click()
      await largeFolder.press('ArrowRight'); await child.entered(); await group('src').getByRole('status').waitFor()
      await scroll.hover(); await page.mouse.wheel(0, 18000)
      await page.waitForFunction(() => document.querySelector('.wb-directory-viewport').scrollTop > 10000)
      await page.waitForFunction(() => [...document.querySelectorAll('.wb-directory-viewport [role="treeitem"]')].some(row => Number(row.getAttribute('aria-posinset')) > 200))
      assert.equal(await page.evaluate(() => document.activeElement.getAttribute('aria-label')), 'src')
      assert(await panel.getByRole('treeitem').count() < 40)
      await child.flush(); await page.keyboard.press('End')
      const last = row(lastName); await last.waitFor()
      await page.waitForFunction(name => document.activeElement?.getAttribute('aria-label') === name, lastName)
      const geometry = await last.evaluate(row => {
        const parent = row.closest('.wb-directory-viewport'), bounds = row.getBoundingClientRect(), clip = parent.getBoundingClientRect()
        const label = row.querySelector('[title]')
        return { height: bounds.height, inView: bounds.top >= clip.top - 1 && bounds.bottom <= clip.bottom + 1,
          pageWidth: document.documentElement.scrollWidth, paneOverflow: parent.scrollWidth - parent.clientWidth,
          textClips: label.scrollWidth > label.clientWidth, outline: getComputedStyle(row).outlineOffset,
          innerRing: getComputedStyle(label.parentElement).boxShadow }
      })
      assert.deepEqual(geometry, { height: 46, inView: true, pageWidth: 320, paneOverflow: 0, textClips: true, outline: '-2px', innerRing: 'none' })
      assert.deepEqual(requests.map(request => request.path), ['', '', 'src'])
      await page.screenshot({ path: path.join(artifacts, 'windowed-dark-320.png') })
      await page.keyboard.press('Enter')
      const source = panel.getByRole('region', { name: '只读文件内容', exact: true }); await source.waitFor()
      await page.evaluate(() => { window.windowedSource = document.querySelector('.wb-source-code') })
      largeRoot = false
      const refresh = panel.getByRole('button', { name: '刷新目录', exact: true })
      await refresh.click(); await row('README.md').waitFor()
      await page.waitForFunction(() => document.querySelector('[aria-label="刷新目录"]')?.getAttribute('aria-disabled') === null)
      await group('src').getByRole('alert').waitFor(); await rendered()
      assert.equal(await panel.getByRole('treeitem').count(), 3)
      assert.equal(await panel.locator('.wb-directory-window').count(), 0)
      assert.equal(await page.evaluate(() => windowedSource === document.querySelector('.wb-source-code')), true)
      assert.deepEqual(requests.map(request => request.path), ['', '', 'src', '', 'src'])
    })
    console.log(`RESULT ${passed.length} checks passed${failures.length ? `, ${failures.length} failed` : ''}`)
    if (failures.length) process.exitCode = 1
  } finally { if (browser) await browser.close(); preview.kill(); await fixture.close(); await new Promise(resolve => site.close(resolve)) }
})().catch(error => { console.error(error); process.exitCode = 1 })
let rootGate, scopeGate, currentGate
