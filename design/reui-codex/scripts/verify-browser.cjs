// Isolated real HTTP preview, not the prototype's generated srcdoc/project data.
const assert = require('node:assert/strict')
const http = require('node:http')
const net = require('node:net')
const path = require('node:path')
const fs = require('node:fs')
const { spawn } = require('node:child_process')
const { chromium } = require(require.resolve('playwright', { paths: [path.resolve(__dirname, '../../../tools')] }))
const root = path.resolve(__dirname, '..')
const freePort = () => new Promise(resolve => { const server = net.createServer(); server.listen(0, '127.0.0.1', () => { const port = server.address().port; server.close(() => resolve(port)) }) })
async function wait(url) {
  for (let i = 0; i < 150; i++) { try { if ((await fetch(url)).ok) return } catch {} await new Promise(resolve => setTimeout(resolve, 100)) }
  throw Error('Preview did not start')
}
;(async () => {
  const requests = []
  const content = http.createServer((request, response) => {
    requests.push({ path: request.url, authorization: request.headers.authorization, referer: request.headers.referer })
    response.writeHead(200, { 'content-type': 'text/html; charset=utf-8' })
    response.end(`<!doctype html><html><head><title>Actual preview</title></head><body><h1>Real page ${request.url}</h1><label>Page draft<input aria-label="Page draft"></label></body></html>`)
  })
  await new Promise(resolve => content.listen(0, '127.0.0.1', resolve))
  const target = `http://127.0.0.1:${content.address().port}`
  const port = await freePort()
  const origin = `http://127.0.0.1:${port}`
  const preview = spawn(process.execPath, [path.join(root, 'node_modules/vite/bin/vite.js'), 'preview', '--host', '127.0.0.1', '--port', String(port), '--strictPort'], { cwd: root, stdio: 'ignore' })
  let browser
  const errors = []
  try {
    browser = await chromium.launch()
    await wait(origin)
    const page = await browser.newPage({ viewport: { width: 1440, height: 940 }, deviceScaleFactor: 1 })
    page.on('pageerror', error => errors.push(error.message))
    const button = name => page.getByRole('button', { name, exact: true })
    const address = page.getByLabel('预览地址', { exact: true })
    const enter = async url => { await address.fill(url); await address.press('Enter') }
    const frame = () => page.frameLocator('iframe[title="页面预览 1"]')
    await page.goto(origin + '/?fixture=1')
    await page.getByText('主题切换已经准备好').waitFor()
    assert.equal(await page.getByRole('region', { name: '浏览器预览' }).isVisible(), true)
    assert.equal(await page.locator('.real-browser iframe').count(), 0, 'No fake homepage or unsolicited site request')
    await page.locator('textarea').fill('真实浏览器迁移保留草稿')
    await page.locator('textarea').evaluate(node => node.setSelectionRange(2, 5))
    for (const value of ['javascript:alert(1)', 'data:text/html,unsafe', 'file:///tmp/local', 'https://user:demo@invalid.example/']) {
      await enter(value)
      assert.equal(await address.getAttribute('aria-invalid'), 'true')
      assert.equal(await page.locator('.real-browser iframe').count(), 0)
    }
    await enter(target + '/one')
    await frame().getByRole('heading', { name: 'Real page /one', exact: true }).waitFor()
    await frame().getByLabel('Page draft').fill('unsaved page form')
    await page.evaluate(() => { window.originalPreview = document.querySelector('.real-browser iframe') })
    const oneRequests = () => requests.filter(request => request.path === '/one').length
    const initialRequests = oneRequests()
    assert.ok(initialRequests > 0, 'Real target was actually requested')
    assert.equal(await page.locator('.real-browser iframe').getAttribute('sandbox'), 'allow-scripts')
    assert.equal(await page.locator('.real-browser iframe').evaluate(node => node.contentDocument === null), true)
    assert.equal(await frame().locator('body').evaluate(() => { try { void parent.document.body; return false } catch { return true } }), true)
    await button('切换主题').click()
    await page.getByLabel('预览视口').selectOption('mobile')
    await button('文件面板').click(); await button('浏览器').click()
    await button('切换会话侧栏').click(); await button('切换会话侧栏').click()
    await button('置顶项目 pi-rust').click()
    await button('置顶项目 pi-rust').click()
    await button('关闭浏览器面板').click(); await button('浏览器').click()
    assert.equal(await page.evaluate(() => window.originalPreview === document.querySelector('.real-browser iframe')), true)
    assert.equal(await frame().getByLabel('Page draft').inputValue(), 'unsaved page form')
    assert.equal(oneRequests(), initialRequests, 'Theme/viewport/rail changes do not reload the page')
    assert.equal(await page.locator('textarea').inputValue(), '真实浏览器迁移保留草稿')
    assert.deepEqual(await page.locator('textarea').evaluate(node => [node.selectionStart, node.selectionEnd]), [2, 5])
    await button('新建浏览器标签页').click()
    assert.equal(await page.getByRole('tabpanel').getByText('预览项目页面').isVisible(), true)
    await enter(target + '/two')
    await page.frameLocator('iframe[title="页面预览 2"]').getByRole('heading', { name: 'Real page /two', exact: true }).waitFor()
    await page.getByRole('tab', { name: '127.0.0.1', exact: true }).first().click()
    assert.equal(await frame().getByLabel('Page draft').inputValue(), 'unsaved page form')
    await enter(target + '/three')
    await frame().getByRole('heading', { name: 'Real page /three', exact: true }).waitFor()
    await button('预览后退').click(); await frame().getByRole('heading', { name: 'Real page /one', exact: true }).waitFor()
    await button('预览前进').click(); await frame().getByRole('heading', { name: 'Real page /three', exact: true }).waitFor()
    await page.getByLabel('预览页面历史').selectOption('0'); await frame().getByRole('heading', { name: 'Real page /one', exact: true }).waitFor()
    await button('刷新预览').click(); await frame().getByRole('heading', { name: 'Real page /one', exact: true }).waitFor()
    const link = page.getByRole('link', { name: '在新窗口打开预览' })
    assert.equal(await link.getAttribute('href'), target + '/one')
    assert.equal(await link.getAttribute('rel'), 'noopener noreferrer')
    await button('关闭预览标签页 2').click()
    assert.equal(await page.getByRole('tab', { name: '127.0.0.1', exact: true }).count(), 1)
    const shots = path.resolve(root, '../../artifacts/confirmed-workbench')
    fs.mkdirSync(shots, { recursive: true })
    const beforeResize = oneRequests()
    for (const width of [1440, 1024, 390, 320]) {
      await page.setViewportSize({ width, height: 940 })
      await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))))
      if (!await page.getByRole('region', { name: '浏览器预览' }).isVisible()) await button('浏览器').click()
      assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth + 1), false, `document overflow ${width}`)
      assert.equal(await page.locator('.real-browser').evaluate(node => node.scrollWidth > node.clientWidth + 1), false, `browser overflow ${width}`)
      assert.equal(await frame().getByRole('heading', { name: 'Real page /one', exact: true }).isVisible(), true)
    }
    assert.equal(oneRequests(), beforeResize, 'Resize preserves the page instead of masking compositor issues with reloads')
    // Capture fresh contexts, not a theme/desktop→mobile compositor diagnostic.
    for (const width of [1440, 1024, 390, 320]) for (const scheme of ['light', 'dark']) {
      const shot = await browser.newPage({ viewport: { width, height: 940 }, deviceScaleFactor: 1 })
      shot.on('pageerror', error => errors.push(error.message))
      await shot.goto(origin + '/?fixture=1')
      await shot.getByText('主题切换已经准备好').waitFor()
      if ((await shot.locator('html').getAttribute('class') ?? '').includes('dark') !== (scheme === 'dark')) await shot.getByRole('button', { name: '切换主题', exact: true }).click()
      if (!await shot.getByRole('region', { name: '浏览器预览' }).isVisible()) await shot.getByRole('button', { name: '浏览器', exact: true }).click()
      await shot.getByLabel('预览地址', { exact: true }).fill(target + '/one')
      await shot.getByLabel('预览地址', { exact: true }).press('Enter')
      await shot.frameLocator('iframe[title="页面预览 1"]').getByRole('heading', { name: 'Real page /one', exact: true }).waitFor()
      await shot.screenshot({ path: path.join(shots, `browser-${width}-${scheme}.png`) })
      await shot.close()
    }
    for (const request of requests) { assert.equal(request.authorization, undefined); assert.equal(request.referer, undefined) }
    assert.deepEqual(errors, [])
    console.log('PASS real HTTP preview, URL validation, opaque sandbox, tabs/history/reload, unchanged iframe/form/draft, themes and responsive viewports')
  } finally {
    await browser?.close()
    content.closeAllConnections(); await new Promise(resolve => content.close(resolve))
    if (preview.exitCode === null) await new Promise(resolve => { preview.once('exit', resolve); preview.kill('SIGTERM') })
  }
})().catch(error => { console.error(error); process.exitCode = 1 })
