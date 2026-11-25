// Product preview + isolated node protocol/clipboard. No host clipboard or file writes.
const assert = require('node:assert/strict')
const path = require('node:path'), net = require('node:net'), fs = require('node:fs'), http = require('node:http')
const { spawn } = require('node:child_process')
const { chromium } = require(require.resolve('playwright', { paths: [path.resolve(__dirname, '../../../tools')] }))
const { startFixture } = require('../../../tests/fixtures/ui-fixture.cjs')
const root = path.resolve(__dirname, '..'), artifacts = path.resolve(root, '../../artifacts/files')
const freePort = () => new Promise(resolve => { const server = net.createServer(); server.listen(0, '127.0.0.1', () => { const port = server.address().port; server.close(() => resolve(port)) }) })
const bounded = async promise => { let timer; try { return await Promise.race([promise, new Promise((_, reject) => { timer = setTimeout(() => reject(Error('HTTP gate timeout')), 10000) })]) } finally { clearTimeout(timer) } }
const source = 'pub fn main() {\r\n    let ready = true;\r\n    // ' + 'long source 界 '.repeat(90) + '\r\n}\r\n'
const patch = 'diff --git a/src/theme.rs b/src/theme.rs\n--- a/src/theme.rs\n+++ b/src/theme.rs\n@@ -1 +1 @@\n-let theme = "light";\n+let theme = "dark"; // ' + 'long diff '.repeat(120) + '\n\\ No newline at end of file\n'
async function addFeedback(viewer, side, text) {
  await viewer.getByRole('button', { name: `在 src/theme.rs · ${side}行 1 添加反馈`, exact: true }).click()
  const dialog = viewer.page().getByRole('dialog', { name: '行内反馈', exact: true })
  await dialog.getByLabel('反馈内容').fill(text)
  await dialog.getByRole('button', { name: '添加到草稿', exact: true }).click()
  await dialog.waitFor({ state: 'hidden' })
}
function contrastRows(viewer) {
  const ctx = document.createElement('canvas').getContext('2d')
  const luminance = color => {
    ctx.clearRect(0, 0, 1, 1); ctx.fillStyle = color; ctx.fillRect(0, 0, 1, 1)
    return [...ctx.getImageData(0, 0, 1, 1).data].slice(0, 3).map(v => { const c = v / 255; return c <= .04045 ? c / 12.92 : ((c + .055) / 1.055) ** 2.4 }).reduce((sum, v, i) => sum + v * [.2126, .7152, .0722][i], 0)
  }
  return [...viewer.querySelectorAll('.wb-source-number, .wb-diff-number, code > span[class], [data-kind="meta"] code, [data-kind="hunk"] code, [data-kind="note"] code, [data-kind="add"] code, [data-kind="remove"] code')].map(node => {
    const backgroundNode = node.matches('.wb-diff-number') ? node : node.closest('.wb-source-line')
    const foreground = luminance(getComputedStyle(node).color), background = luminance(getComputedStyle(backgroundNode).backgroundColor)
    return { role: node.className || node.parentElement.dataset.kind, ratio: (Math.max(foreground, background) + .05) / (Math.min(foreground, background) + .05) }
  })
}
;(async () => {
  const fixture = await startFixture(0, { nativeQueries: true, includeHistoryAssistant: true })
  fixture.controls.fileEntriesByPath = { '': ['source.rs', 'other.rs', 'empty.txt', 'evil.html'].map(name => ({ name, path: name, kind: 'file', size: 0 })) }
  fixture.controls.fileContentByPath = { 'source.rs': source, 'other.rs': 'other source', 'empty.txt': '', 'evil.html': '<script>window.fileAttack=1</script><img src="https://example.org/must-not-load">' }
  let previewLoads = 0
  const site = http.createServer((request, response) => { previewLoads++; response.writeHead(200, { 'content-type': 'text/html; charset=utf-8' }); response.end('<!doctype html><label>File preview<input aria-label="File preview"></label>') })
  await new Promise(resolve => site.listen(0, '127.0.0.1', resolve))
  const siteUrl = `http://127.0.0.1:${site.address().port}/`
  const port = await freePort(), origin = `http://127.0.0.1:${port}`
  const preview = spawn(process.execPath, [path.join(root, 'node_modules/vite/bin/vite.js'), 'preview', '--host', '127.0.0.1', '--port', String(port), '--strictPort'], { cwd: root, env: { ...process.env, RPI_NODE_URL: fixture.url }, stdio: 'ignore' })
  let browser
  const failures = [], passed = []
  try {
    let ready = false
    for (let i = 0; i < 120; i++) { try { if ((await fetch(origin)).ok) { ready = true; break } } catch {} await new Promise(resolve => setTimeout(resolve, 100)) }
    assert(ready)
    fs.mkdirSync(artifacts, { recursive: true })
    browser = await chromium.launch()
    const check = async (name, run, { prepare, waitHistory = true } = {}) => {
      const context = await browser.newContext({ viewport: { width: 1440, height: 940 } })
      await context.addInitScript(() => {
        window.clip = { calls: [], pending: [], mode: 'resolve', legacy: 0 }
        Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText(text) { clip.calls.push(text); if (clip.mode === 'reject') return Promise.reject(Error('denied')); if (clip.mode === 'pending') return new Promise(resolve => clip.pending.push(resolve)); return Promise.resolve() } } })
        document.execCommand = () => { clip.legacy++; throw Error('legacy clipboard forbidden') }
      })
      const page = await context.newPage(), errors = [], gates = [], external = []
      page.setDefaultTimeout(10000)
      page.on('pageerror', error => errors.push(error.message))
      page.on('request', request => { if (/^https?:/.test(request.url()) && !request.url().startsWith(origin + '/') && !request.url().startsWith(siteUrl)) external.push(request.url()) })
      const rendered = () => page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))))
      const panel = page.getByTestId('run-panel'), viewer = panel.getByRole('region', { name: '只读文件内容', exact: true })
      const gate = () => {
        let enter, release, finish, request
        const entered = new Promise(resolve => { enter = resolve }), held = new Promise(resolve => { release = resolve }), done = new Promise(resolve => { finish = resolve })
        const value = { hold: async (route, response) => { request = route.request(); enter(); try { await held; await route.fulfill(response) } finally { finish() } }, entered: () => bounded(entered), release, done: () => request ? bounded(done) : Promise.resolve(), flush: async () => { const received = page.waitForResponse(response => response.request() === request); release(); await bounded(done); await (await received).finished(); await rendered() } }
        gates.push(value); return value
      }
      const tree = async () => { await panel.getByTestId('tab-tree').click(); await panel.getByRole('treeitem', { name: 'source.rs', exact: true }).waitFor() }
      const select = async name => { await tree(); await panel.getByRole('treeitem', { name, exact: true }).getByText(name, { exact: true }).click() }
      const diff = async () => { await panel.getByTestId('tab-changes').click(); await panel.getByRole('button', { name: '查看 src/theme.rs 的 Diff', exact: true }).click() }
      try {
        await page.route(url => url.pathname.endsWith('/git/diff'), route => route.fulfill({ json: { path: 'src/theme.rs', patch } }))
        if (prepare) await prepare({ page, gate })
        await page.goto(origin + '/?live=1')
        if (waitHistory) await page.getByText('帮我为工作台增加一个深色主题，并保持手机端可用。', { exact: true }).waitFor()
        else await page.getByRole('button', { name: '为工作台添加深色模式', exact: true }).waitFor()
        await page.getByRole('button', { name: '文件面板', exact: true }).click()
        await run({ page, panel, viewer, tree, select, diff, gate, rendered })
        assert.deepEqual(errors, [])
        assert.deepEqual(external, [])
        assert.equal(await page.evaluate(() => clip.legacy), 0)
        passed.push(name); console.log('PASS', name)
      } catch (error) { failures.push(name); console.error('FAIL', name, error.stack) }
      finally { for (const value of gates) value.release(); for (const value of gates) await value.done(); await context.close() }
    }
    await check('source uses shared Rust rows and tokens; exact copy excludes line numbers and preserves CRLF', async ({ page, panel, viewer, tree }) => {
      await tree(); await panel.getByRole('treeitem', { name: 'source.rs', exact: true }).press('Enter'); await viewer.waitFor()
      assert.equal(await viewer.locator('.wb-source-line').count(), 4)
      assert.equal(await viewer.locator('.code-token-keyword').first().textContent(), 'pub')
      await panel.getByRole('button', { name: '复制源码', exact: true }).click()
      await panel.locator('[data-copy-state="success"]').waitFor()
      assert.deepEqual(await page.evaluate(() => clip.calls), [source])
      assert.equal(await panel.getByRole('button', { name: '保存', exact: true }).count(), 0)
    })
    await check('raw Diff keeps every header/sign and copy payload without invented numbers or feedback', async ({ page, panel, viewer, diff }) => {
      const raw = patch.replace('@@ -1 +1 @@', '@@ -1,2 +1,2 @@')
      await page.route(url => url.pathname.endsWith('/git/diff'), route => route.fulfill({ json: { path: 'src/theme.rs', patch: raw } }))
      await diff(); await viewer.waitFor()
      await panel.getByText('此 Diff 未通过统一差异校验，按完整原文显示；不提供行号。', { exact: true }).waitFor()
      assert.equal(await viewer.locator('[data-kind="add"]').count(), 1)
      assert.equal(await viewer.locator('[data-kind="remove"]').count(), 1)
      assert.equal(await viewer.locator('.wb-source-number, .wb-diff-number').count(), 0)
      await panel.getByRole('button', { name: '复制 Diff', exact: true }).click()
      await panel.locator('[data-copy-state="success"]').waitFor()
      assert.deepEqual(await page.evaluate(() => clip.calls), [raw])
      assert.equal(await viewer.getByRole('button').count(), 0)
    })
    await check('validated unified hunks show real old/new numbers and treat header-like source as changes', async ({ page, panel, viewer, diff }) => {
      const text = 'diff --git a/src/theme.rs b/src/theme.rs\r\n--- a/src/theme.rs\r\n+++ b/src/theme.rs\r\n@@ -10,2 +20,2 @@ function\r\n--- source content\r\n+++ source content <script>window.diffAttack=1</script>\r\n context\r\n\\ No newline at end of file\r\n'
      await page.route(url => url.pathname.endsWith('/git/diff'), route => route.fulfill({ json: { path: 'src/theme.rs', patch: text } }))
      await diff(); await viewer.waitFor()
      assert.equal(await viewer.locator('[data-kind="remove"]').count(), 1)
      assert.equal(await viewer.locator('[data-kind="add"]').count(), 1)
      const changes = viewer.locator('[data-kind="remove"], [data-kind="add"], [data-kind="context"]')
      assert.deepEqual(await changes.locator('.wb-diff-old').allTextContents(), ['10', '', '11'])
      assert.deepEqual(await changes.locator('.wb-diff-new').allTextContents(), ['', '20', '21'])
      assert.equal(await viewer.locator('script').count(), 0)
      assert.equal(await page.evaluate(() => window.diffAttack), undefined)
      await panel.getByRole('button', { name: '复制 Diff', exact: true }).click()
      await panel.locator('[data-copy-state="success"]').waitFor()
      assert.deepEqual(await page.evaluate(() => clip.calls), [text])
    })
    await check('zero ranges and multiple files reset counters; unsupported or malformed patches clear every number', async ({ page, panel, viewer, diff }) => {
      const valid = '--- /dev/null\n+++ b/new\n@@ -0,0 +1 @@\n+created\ndiff --git a/old b/old\n--- a/old\n+++ /dev/null\n@@ -10 +0,0 @@\n-removed\n'
      const cases = [valid, 'diff --cc file\n@@@ -1 -1 +1 @@@\n', 'diff --git a/file b/file\nGIT binary patch\nliteral 0\n', '--- a/x\n+++ b/x\n@@ -1,2 +1,2 @@\n-old\n+new\n', '--- a/x\n+++ b/x\n@@ -4294967296 +1 @@\n-old\n+new\n', 'diff --git a/old b/new\nrename from old\nrename to new\n', valid]
      for (const text of cases) {
        await page.route(url => url.pathname.endsWith('/git/diff'), route => route.fulfill({ json: { path: 'src/theme.rs', patch: text } }))
        await diff(); await viewer.waitFor()
        if (text === valid) {
          assert.deepEqual(await viewer.locator('.wb-diff-old').allTextContents(), ['', '10'])
          assert.deepEqual(await viewer.locator('.wb-diff-new').allTextContents(), ['1', ''])
          await panel.getByText('统一 Diff · 工作区', { exact: true }).waitFor()
          assert.equal(await viewer.getByRole('button', { name: /添加反馈/ }).count(), 0)
        } else {
          assert.equal(await viewer.locator('.wb-diff-number').count(), 0)
          await panel.getByText('原始 Diff · 工作区', { exact: true }).waitFor()
          assert.match(await panel.locator('.wb-file-status').textContent(), /按完整原文显示/)
        }
        await panel.getByRole('button', { name: '复制 Diff', exact: true }).click(); await panel.locator('[data-copy-state="success"]').waitFor()
        assert.equal(await page.evaluate(() => clip.calls.at(-1)), text)
      }
    })
    await check('line feedback re-reads the exact Diff and creates a scoped feedback chip without changing the input', async ({ page, viewer, diff, gate, rendered }) => {
      await diff(); await viewer.waitFor()
      const draft = page.locator('main textarea').first()
      await draft.fill('existing draft')
      await draft.evaluate(node => node.setSelectionRange(2, 6))
      const action = viewer.getByRole('button', { name: '在 src/theme.rs · 旧行 1 添加反馈', exact: true })
      await action.press('Enter')
      const form = page.getByRole('dialog', { name: '行内反馈', exact: true })
      assert.equal(await form.getByLabel('反馈内容').evaluate(node => node === document.activeElement), true)
      await form.getByLabel('反馈内容').fill('保留兼容路径')
      const verification = gate()
      const requests = []
      page.on('request', request => { if (new URL(request.url()).pathname.startsWith('/api/v1/')) requests.push([request.method(), new URL(request.url())]) })
      await page.route(url => url.pathname.endsWith('/git/diff'), route => verification.hold(route, { json: { path: 'src/theme.rs', patch } }))
      await form.getByRole('button', { name: '添加到草稿', exact: true }).press('Enter'); await verification.entered()
      assert.equal(await draft.inputValue(), 'existing draft')
      await form.getByRole('button', { name: '正在核对…', exact: true }).waitFor()
      await verification.flush(); await form.waitFor({ state: 'hidden' }); await rendered()
      await page.getByRole('button', { name: '移除 src/theme.rs · 旧行 1 · 工作区 的反馈', exact: true }).waitFor()
      await viewer.locator('[data-kind="remove"] .wb-inline-feedback').getByText('保留兼容路径', { exact: true }).waitFor()
      assert.equal(await draft.inputValue(), 'existing draft')
      assert.deepEqual(await draft.evaluate(node => [node.selectionStart, node.selectionEnd]), [2, 6])
      assert.equal(await action.evaluate(node => node === document.activeElement), true)
      assert.deepEqual(requests.map(([method, url]) => [method, url.searchParams.get('path'), url.searchParams.get('staged')]), [['GET', 'src/theme.rs', 'false']])
    })
    await check('changed and cancelled Diff confirmations retain the untouched draft', async ({ page, viewer, diff, gate }) => {
      await diff(); await viewer.waitFor()
      const draft = page.locator('main textarea').first()
      await draft.fill('keep draft')
      const action = viewer.getByRole('button', { name: '在 src/theme.rs · 新行 1 添加反馈', exact: true })
      const form = page.getByRole('dialog', { name: '行内反馈', exact: true })
      const late = gate()
      let attempts = 0
      await page.route(url => url.pathname.endsWith('/git/diff'), route => ++attempts === 1
        ? route.fulfill({ json: { path: 'src/theme.rs', patch: patch.replace('"dark"', '"new"') } })
        : late.hold(route, { json: { path: 'src/theme.rs', patch } }))
      await action.click()
      await form.getByLabel('反馈内容').fill('保留反馈文本')
      const submit = form.getByRole('button', { name: '添加到草稿', exact: true })
      await submit.click()
      await form.getByRole('alert').filter({ hasText: 'Diff 已变化' }).waitFor()
      assert.equal(await form.getByLabel('反馈内容').inputValue(), '保留反馈文本')
      assert.equal(await draft.inputValue(), 'keep draft')
      await submit.click(); await late.entered()
      await form.getByRole('button', { name: '取消', exact: true }).click(); await late.flush()
      await form.waitFor({ state: 'hidden' })
      assert.equal(await draft.inputValue(), 'keep draft')
      assert.equal(await page.getByRole('list', { name: '行内反馈草稿', exact: true }).count(), 0)
    })
    await check('feedback restores after reload, stays session scoped, and only annotates the exact Diff revision', async ({ page, panel, viewer, diff, rendered }) => {
      const draft = page.locator('main textarea').first()
      const chip = page.getByRole('button', { name: '移除 src/theme.rs · 旧行 1 · 工作区 的反馈', exact: true })
      const row = viewer.locator('.wb-inline-feedback')
      let currentPatch = patch
      await page.route(url => url.pathname.endsWith('/git/diff'), route => route.fulfill({ json: { path: 'src/theme.rs', staged: new URL(route.request().url()).searchParams.get('staged') === 'true', patch: currentPatch } }))
      await diff(); await addFeedback(viewer, '旧', '仅保存在当前会话')
      await chip.waitFor()
      await page.getByRole('button', { name: '梳理 Agent 架构', exact: true }).click()
      await chip.waitFor({ state: 'hidden' })
      await page.getByRole('button', { name: '为工作台添加深色模式', exact: true }).click()
      await chip.waitFor()
      await page.reload()
      await chip.waitFor()
      assert.equal(await draft.inputValue(), '')
      assert.equal(await page.getByRole('button', { name: '发送', exact: true }).isDisabled(), true)
      const files = page.getByRole('button', { name: '文件面板', exact: true })
      if (!await panel.getByTestId('tab-changes').isVisible()) await files.click()
      await diff(); await row.waitFor()
      await panel.getByTestId('staged-toggle').check()
      await panel.getByText('统一 Diff · 已暂存', { exact: true }).waitFor()
      await viewer.waitFor(); await rendered()
      assert.equal(await row.count(), 0)
      currentPatch = patch.replace('light', 'earlier')
      await panel.getByTestId('staged-toggle').uncheck()
      await viewer.getByText('-let theme = "earlier";', { exact: true }).waitFor()
      assert.equal(await row.count(), 0)
      await chip.press('Enter')
      await chip.waitFor({ state: 'hidden' })
      assert.equal(await draft.evaluate(node => node === document.activeElement), true)
      await page.reload()
      await page.getByText('帮我为工作台增加一个深色主题，并保持手机端可用。', { exact: true }).waitFor()
      assert.equal(await chip.count(), 0)
    })
    await check('send and queue deliver exact feedback text; a late ACK clears only submitted draft parts', async ({ page, viewer, diff }) => {
      const draft = page.locator('main textarea').first()
      const chips = page.getByRole('list', { name: '行内反馈草稿', exact: true })
      await diff()
      await draft.fill('检查更改')
      await addFeedback(viewer, '旧', '第一条反馈')
      await page.evaluate(() => {
        const originalSend = WebSocket.prototype.send
        WebSocket.prototype.send = function (data) {
          const frame = typeof data === 'string' ? JSON.parse(data) : null
          if (frame?.type === 'execute' && frame.payload?.type === 'prompt') {
            WebSocket.prototype.send = originalSend
            window.releaseFeedbackPrompt = () => originalSend.call(this, data)
            return
          }
          return originalSend.call(this, data)
        }
      })
      const calls = fixture.requests.length
      try {
        await draft.press('Meta+Enter')
        await page.waitForFunction(() => typeof window.releaseFeedbackPrompt === 'function')
        assert.equal(await chips.getByRole('listitem').count(), 1)
        await draft.fill('后续修改')
        await addFeedback(viewer, '新', '回执期间新增反馈')
        await page.evaluate(() => window.releaseFeedbackPrompt())
        await chips.getByRole('button', { name: /旧行 1/ }).waitFor({ state: 'hidden' })
        assert.equal(await draft.inputValue(), '后续修改')
        assert.equal(await chips.getByRole('listitem').count(), 1)
        const writes = command => fixture.requests.slice(calls).filter(({ body }) => body.type === 'execute' && body.payload?.type === command).map(({ body }) => body.payload.message)
        assert.deepEqual(writes('prompt'), ['检查更改\n\n行内反馈 · src/theme.rs · 工作区 Diff · 旧行 1\n  -let theme = "light";\n第一条反馈'])
        fixture.emit({ type: 'agent_start' })
        const queue = page.getByRole('button', { name: '加入队列', exact: true })
        await queue.click()
        await chips.waitFor({ state: 'hidden' })
        assert.equal(await draft.inputValue(), '')
        assert.deepEqual(writes('queue_message'), [`后续修改\n\n行内反馈 · src/theme.rs · 工作区 Diff · 新行 1\n  ${patch.split('\n')[5].slice(0, 240)}…\n回执期间新增反馈`])
      } finally {
        fixture.setQueue([])
        fixture.emit({ type: 'agent_settled', outcome: 'completed', pendingResume: false })
      }
    })
    await check('maximum valid line numbers fit both sticky columns without clipping or leaking page width', async ({ page, panel, viewer, diff }) => {
      const text = '--- a/file\n+++ b/file\n@@ -4294967295 +4294967295 @@\n-old\n+new ' + 'long '.repeat(200) + '\n'
      await page.route(url => url.pathname.endsWith('/git/diff'), route => route.fulfill({ json: { path: 'src/theme.rs', patch: text } }))
      await diff(); await viewer.waitFor()
      assert.deepEqual(await viewer.locator('.wb-diff-old').allTextContents(), ['4294967295', ''])
      assert.deepEqual(await viewer.locator('.wb-diff-new').allTextContents(), ['', '4294967295'])
      await viewer.evaluate(node => { node.scrollLeft = node.clientWidth + 100 })
      const left = (await viewer.boundingBox()).x
      const before = await viewer.locator('.wb-diff-old').first().boundingBox(), after = await viewer.locator('.wb-diff-new').first().boundingBox()
      assert(Math.abs(before.x - left) < 1); assert(Math.abs(after.x - before.x - before.width) < 1); assert(before.width >= 80)
      for (const overflow of await viewer.locator('.wb-diff-number').evaluateAll(nodes => nodes.map(node => node.scrollWidth > node.clientWidth))) assert.equal(overflow, false)
      assert.equal(await page.evaluate(() => document.documentElement.scrollWidth), 1440)
      await panel.getByRole('button', { name: '复制 Diff', exact: true }).click(); await panel.locator('[data-copy-state="success"]').waitFor()
      assert.equal(await page.evaluate(() => clip.calls.at(-1)), text)
    })
    await check('pending selection clears old content; late A cannot replace B or clear its loading state', async ({ page, panel, viewer, select, gate }) => {
      await select('other.rs'); await viewer.waitFor()
      const old = gate(), current = gate()
      await page.route(url => url.pathname.endsWith('/file'), route => {
        const name = new URL(route.request().url()).searchParams.get('path')
        return (name === 'source.rs' ? old : current).hold(route, { json: { path: name, content: name === 'source.rs' ? 'OLD CONTENT' : 'CURRENT CONTENT' } })
      })
      await select('source.rs'); await old.entered()
      await panel.getByText('正在读取 source.rs…', { exact: true }).waitFor()
      assert.equal(await viewer.count(), 0)
      await select('other.rs'); await current.entered(); await old.flush()
      await panel.getByText('正在读取 other.rs…', { exact: true }).waitFor()
      assert.doesNotMatch(await panel.innerText(), /OLD CONTENT/)
      await current.flush(); await viewer.getByText('CURRENT CONTENT', { exact: true }).waitFor()
    })
    await check('empty source and empty staged Diff are distinct; source filter changes do not issue a Diff read', async ({ page, panel, viewer, select, diff }) => {
      await select('empty.txt'); await panel.getByText('空文件', { exact: true }).waitFor()
      let calls = 0
      await page.route(url => url.pathname.endsWith('/git/diff'), route => { calls++; assert.equal(new URL(route.request().url()).searchParams.get('staged'), 'true'); return route.fulfill({ json: { path: 'src/theme.rs', patch: '' } }) })
      await panel.getByTestId('staged-toggle').check()
      await page.evaluate(() => new Promise(resolve => requestAnimationFrame(resolve)))
      assert.equal(calls, 0)
      await diff(); await panel.getByText('当前范围没有 Diff。', { exact: true }).waitFor()
      await panel.getByText('原始 Diff · 已暂存', { exact: true }).waitFor()
      assert.equal(calls, 1); assert.equal(await viewer.count(), 0)
      await page.getByRole('button', { name: '浏览器', exact: true }).click()
      await page.getByRole('button', { name: '文件面板', exact: true }).click()
      assert.equal(await panel.getByTestId('staged-toggle').isChecked(), true)
      await panel.getByText('原始 Diff · 已暂存', { exact: true }).waitFor()
      assert.equal(calls, 1, 'remount restores the observed filter without automatically reading again')
    })
    await check('read failure is local and retry repeats only the exact GET; source HTML stays inert', async ({ page, panel, viewer, select }) => {
      let calls = 0
      await page.route(url => url.pathname.endsWith('/file'), route => { assert.equal(route.request().method(), 'GET'); return ++calls === 1 ? route.fulfill({ status: 503, json: { error: 'file temporarily unavailable' } }) : route.fulfill({ json: { path: 'evil.html', content: fixture.controls.fileContentByPath['evil.html'] } }) })
      await select('evil.html'); await panel.getByRole('alert').waitFor()
      assert.equal(await viewer.count(), 0)
      await panel.getByRole('button', { name: '重试读取文件', exact: true }).click(); await viewer.waitFor()
      assert.equal(calls, 2)
      assert.equal(await viewer.locator('img,script,iframe,svg').count(), 0)
      assert.equal(await page.evaluate(() => window.fileAttack), undefined)
      assert.match(await viewer.textContent(), /window.fileAttack=1/)
    })
    await check('failed copy clears old success and pending callbacks cannot follow a different file', async ({ page, panel, viewer, select }) => {
      await select('source.rs'); await viewer.waitFor()
      const copy = panel.getByRole('button', { name: '复制源码', exact: true })
      await copy.click(); await panel.locator('[data-copy-state="success"]').waitFor()
      await page.evaluate(() => { clip.mode = 'reject' }); await copy.click(); await panel.locator('[data-copy-state="error"]').waitFor()
      await page.evaluate(() => { clip.mode = 'pending' }); await copy.click(); await panel.locator('[data-copy-state="pending"]').waitFor()
      await select('other.rs'); await viewer.getByText('other source', { exact: true }).waitFor()
      await page.evaluate(() => clip.pending.shift()()); await panel.locator('[data-copy-state="idle"]').waitFor()
      assert.equal(await page.evaluate(() => clip.calls.length), 3)
    })
    await check('session change rejects a late same-path read and retains the new scope result', async ({ page, panel, viewer, select, gate }) => {
      const old = gate(); let calls = 0
      await page.route(url => url.pathname.endsWith('/file'), route => ++calls === 1 ? old.hold(route, { json: { path: 'source.rs', content: 'OLD SESSION' } }) : route.fulfill({ json: { path: 'source.rs', content: 'NEW SESSION' } }))
      await select('source.rs'); await old.entered()
      await page.getByRole('button', { name: '梳理 Agent 架构', exact: true }).click()
      await select('source.rs'); await viewer.getByText('NEW SESSION', { exact: true }).waitFor()
      await old.flush(); assert.doesNotMatch(await panel.innerText(), /OLD SESSION/)
    })
    await check('source/Diff keep local scrolling, visible focus and draft/iframe across both modes and four widths', async ({ page, panel, viewer, select, diff, rendered }) => {
      await page.getByRole('button', { name: '浏览器', exact: true }).click()
      await page.getByRole('textbox', { name: '预览地址', exact: true }).fill(siteUrl)
      await page.getByRole('button', { name: '打开', exact: true }).click()
      const previewDraft = page.frameLocator('.real-browser iframe').getByLabel('File preview')
      await previewDraft.fill('preserve file preview')
      const loads = previewLoads
      await page.evaluate(() => { window.fileFrame = document.querySelector('.real-browser iframe') })
      await page.getByRole('button', { name: '文件面板', exact: true }).click()
      const draft = page.locator('textarea')
      await draft.fill('文件阅读草稿 selection'); await draft.evaluate(node => node.setSelectionRange(2, 7))
      await diff(); await addFeedback(viewer, '新', '调整后的行需要核对窄屏布局和水平滚动。'.repeat(3))
      for (const dark of [false, true]) {
        await page.setViewportSize({ width: 1440, height: 940 })
        if (dark) await page.getByRole('button', { name: '切换主题', exact: true }).click()
        for (const width of [1440, 1024, 390, 320]) {
          const wasOverlay = await page.evaluate(() => matchMedia('(max-width: 900px)').matches)
          await page.setViewportSize({ width, height: 940 })
          // The existing shell deliberately closes the rail upon entering the
          // overlay breakpoint. Wait for that observable effect, then reopen.
          if (width <= 900 && !wasOverlay) await panel.waitFor({ state: 'hidden' })
          if (!await panel.isVisible()) await page.getByRole('button', { name: '文件面板', exact: true }).click()
          for (const mode of ['source', 'diff']) {
            if (mode === 'source') await select('source.rs'); else await diff()
            await viewer.waitFor(); await panel.getByRole('button', { name: mode === 'source' ? '复制源码' : '复制 Diff', exact: true }).focus(); await page.keyboard.press('Tab'); await rendered()
            assert.equal(await viewer.evaluate(node => document.activeElement === node), true)
            const box = await viewer.evaluate(node => { node.scrollLeft = node.clientWidth + 120; const style = getComputedStyle(node); return { width: node.clientWidth, scroll: node.scrollWidth, left: node.scrollLeft, offset: style.outlineOffset, outline: style.outlineStyle, pageWidth: document.documentElement.scrollWidth, viewport: innerWidth } })
            assert(box.scroll > box.width && box.left > 0, JSON.stringify(box)); assert.equal(box.pageWidth, box.viewport); assert.equal(box.offset, '-3px'); assert.equal(box.outline, 'solid')
            if (mode === 'source') {
              const left = (await viewer.boundingBox()).x
              for (const rect of await viewer.locator('.wb-source-number').evaluateAll(nodes => nodes.map(node => ({ x: node.getBoundingClientRect().x, width: node.getBoundingClientRect().width })))) { assert(Math.abs(rect.x - left) < 1, JSON.stringify({ rect, left, width, dark })); assert.equal(rect.width, 40) }
            } else {
              const left = (await viewer.boundingBox()).x
              const annotation = await viewer.locator('.wb-inline-feedback').boundingBox()
              assert(annotation && annotation.x >= left + 80 && annotation.x + annotation.width <= left + box.width, JSON.stringify({ width, annotation, box, left }))
              for (const [selector, offset] of [['.wb-diff-old', 0], ['.wb-diff-new', 40]]) {
                const numbers = await viewer.locator(selector).evaluateAll(nodes => nodes.map(node => ({ x: node.getBoundingClientRect().x, width: node.getBoundingClientRect().width })))
                assert(numbers.length > 0)
                for (const rect of numbers) { assert(Math.abs(rect.x - left - offset) < 1, JSON.stringify({ selector, rect, left, width, dark })); assert.equal(rect.width, 40) }
              }
            }
            await page.keyboard.press('Tab'); assert.equal(await viewer.evaluate(node => document.activeElement === node), false)
            await page.keyboard.press('Shift+Tab'); assert.equal(await viewer.evaluate(node => document.activeElement === node), true)
            if (width <= 390) {
              const copyBox = await panel.getByRole('button', { name: mode === 'source' ? '复制源码' : '复制 Diff', exact: true }).boundingBox()
              assert(copyBox.height >= 44 && copyBox.width >= 44)
              const target = mode === 'source' ? panel.getByRole('treeitem', { name: 'source.rs', exact: true }).getByText('source.rs', { exact: true }) : panel.getByRole('button', { name: '查看 src/theme.rs 的 Diff', exact: true })
              assert((await target.boundingBox()).height >= 44)
            }
            if ([1440, 320].includes(width)) {
              await viewer.evaluate(node => { node.scrollLeft = 0 })
              await page.screenshot({ path: path.join(artifacts, `${mode}-${dark ? 'dark' : 'light'}-${width}.png`) })
            }
          }
        }
      }
      assert.equal(await draft.inputValue(), '文件阅读草稿 selection')
      assert.deepEqual(await draft.evaluate(node => [node.selectionStart, node.selectionEnd]), [2, 7])
      assert.equal(await page.evaluate(() => fileFrame === document.querySelector('.real-browser iframe')), true)
      assert.equal(await previewDraft.inputValue(), 'preserve file preview')
      assert.equal(previewLoads, loads)
    })
    await check('all five palettes keep source tokens, line numbers and raw Diff headers/signs above 4.5:1', async ({ page, viewer, select, diff, rendered }) => {
      for (const dark of [false, true]) {
        if (dark) await page.getByRole('button', { name: '切换主题', exact: true }).click()
        for (const mode of ['source', 'diff']) {
          if (mode === 'source') await select('source.rs'); else await diff()
          await viewer.waitFor()
          await page.getByRole('button', { name: '打开显示选项', exact: true }).click()
          const palettes = page.getByRole('combobox', { name: '选择主题', exact: true })
          const ids = await palettes.locator('option').evaluateAll(nodes => nodes.map(node => node.value))
          assert.equal(ids.length, 5)
          for (const id of ids) {
            await palettes.selectOption(id); await rendered()
            const rows = await viewer.evaluate(contrastRows); assert(rows.length > 0)
            for (const row of rows) assert(row.ratio >= 4.5, JSON.stringify({ id, mode, dark, row }))
          }
          await page.keyboard.press('Escape')
        }
      }
    })
    await check('many-line input falls back to one full literal node and exact copy without claiming one source line', async ({ page, panel, viewer, select }) => {
      const text = '\n'.repeat(10010) + '<script>literal tail 界</script>'
      await page.route(url => url.pathname.endsWith('/file'), route => route.fulfill({ json: { path: 'source.rs', content: text } }))
      await select('source.rs'); await viewer.waitFor()
      assert.equal(await viewer.locator('.wb-source-line').count(), 1)
      assert.equal(await viewer.locator('[data-kind="plain"]').textContent(), text)
      assert.equal(await viewer.locator('script, .wb-source-number').count(), 0)
      await panel.getByText('内容较大，按完整原文显示；行号与高亮已停用。', { exact: true }).waitFor()
      await panel.getByRole('button', { name: '复制源码', exact: true }).click()
      await panel.locator('[data-copy-state="success"]').waitFor()
      assert.deepEqual(await page.evaluate(() => clip.calls), [text])
    })
    let delayedVM
    await check('late presentation WASM keeps the file pane loading instead of invoking an uninitialized export', async ({ panel, viewer, select }) => {
      await delayedVM.entered()
      await panel.getByText('正在加载文件展示…', { exact: true }).waitFor()
      assert.equal(await viewer.count(), 0)
      await delayedVM.flush()
      await select('source.rs'); await viewer.waitFor()
      assert.equal(await viewer.locator('.wb-source-line').count(), 4)
    }, { waitHistory: false, prepare: async ({ page, gate }) => {
      delayedVM = gate()
      await page.route(url => url.pathname.includes('work_viewmodel') && url.pathname.endsWith('.wasm'), async route => { const response = await route.fetch(); await delayedVM.hold(route, { response }) })
    } })
    assert.deepEqual(failures, [])
    console.log(`RESULT ${passed.length} checks passed`)
  } finally {
    if (browser) await browser.close()
    if (preview.exitCode === null && preview.signalCode === null) await new Promise(resolve => { preview.once('exit', resolve); preview.kill('SIGTERM') })
    await fixture.close()
    await new Promise(resolve => site.close(resolve))
  }
})().catch(error => { console.error(error.stack); process.exitCode = 1 })
