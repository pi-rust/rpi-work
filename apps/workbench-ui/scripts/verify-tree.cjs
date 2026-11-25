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
    await check('local child loading and retry preserve draft selection and the real iframe in both modes and widths', async ({ page, panel, group, expand, requests, rendered, gate }) => {
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
          await panel.getByTestId('tab-tree').click(); await expand('src')
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
          // Returning to browser unmounts only files, never its iframe.
          if (width === 320) await panel.getByRole('button', { name: '关闭面板', exact: true }).click()
          await page.getByRole('button', { name: '浏览器', exact: true }).click()
        }
      }
      assert.equal(await draft.inputValue(), '目录读取草稿 selection'); assert.deepEqual(await draft.evaluate(node => [node.selectionStart, node.selectionEnd]), [2, 6])
      assert.equal(await page.evaluate(() => treeFrame === document.querySelector('.real-browser iframe')), true); assert.equal(await input.inputValue(), 'keep directory preview'); assert.equal(loads, initialLoads)
      assert(requests.some(request => request.path === 'src'))
    }, async ({ page }) => { await page.route(url => url.pathname.endsWith('/files') && url.searchParams.get('path') === 'src', route => route.fulfill({ status: 503, json: { error: 'child unavailable' } })) })
    console.log(`RESULT ${passed.length} checks passed${failures.length ? `, ${failures.length} failed` : ''}`)
    if (failures.length) process.exitCode = 1
  } finally { if (browser) await browser.close(); preview.kill(); await fixture.close(); await new Promise(resolve => site.close(resolve)) }
})().catch(error => { console.error(error); process.exitCode = 1 })
let rootGate, scopeGate, currentGate
