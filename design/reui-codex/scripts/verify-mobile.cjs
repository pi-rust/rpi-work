// Mobile acceptance: 390x844 and 320x740 stay usable.
//
// Self-contained: starts `vite preview` (fixture mode needs no node) and checks
// geometry plus the drawer/panel behaviour.
//
// Usage: node scripts/verify-mobile.cjs
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

;(async () => {
  const port = await freePort()
  const preview = spawn('npm', ['run', 'preview', '--', '--port', String(port), '--strictPort'], {
    cwd: appRoot,
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
  try {
    await waitForServer(`${origin}/`)
    for (const [width, height] of [
      [390, 844],
      [320, 740],
    ]) {
      const page = await browser.newPage({ viewport: { width, height }, deviceScaleFactor: 2 })
      page.setDefaultTimeout(15000)
      page.on('pageerror', (error) => errors.push(`${width}: ${error.message}`))
      await page.goto(`${origin}/`)
      await page.getByText('主题切换已经准备好').waitFor()

      const overflow = await page.evaluate(() => ({
        scrollWidth: document.documentElement.scrollWidth,
        innerWidth: window.innerWidth,
      }))
      assert(
        overflow.scrollWidth <= overflow.innerWidth + 1,
        `${width}: horizontal overflow ${overflow.scrollWidth} > ${overflow.innerWidth}`,
      )

      // The sidebar must start hidden and open as a drawer.
      const brand = page.getByRole('button', { name: /rpiwork/ })
      assert.equal(await brand.count(), 0, `${width}: sidebar hidden`)
      await page.getByRole('button', { name: '打开侧栏' }).click()
      await brand.waitFor()
      await page.getByRole('button', { name: '检查手机端布局', exact: true }).click()
      await brand.waitFor({ state: 'detached' })
      pass(`${width}px: sidebar is a drawer that closes after picking a session`)

      // Composer stays reachable with a usable send target.
      const send = page.getByRole('button', { name: '发送' })
      const box = await send.boundingBox()
      assert(box && box.width >= 24 && box.height >= 24, `${width}: send target too small`)
      assert(box.y + box.height <= height + 1, `${width}: send button below the fold`)
      pass(`${width}px: composer and send button stay inside the viewport`)

      // Run panel opens as a full-height overlay with its own close button.
      await page.getByRole('button', { name: '运行面板' }).click()
      await page.getByTestId('run-panel').waitFor()
      await page.getByRole('button', { name: '关闭面板' }).click()
      // The inspector stays mounted to preserve embedded pages; closing hides it.
      await page.getByTestId('run-panel').waitFor({ state: 'hidden' })
      pass(`${width}px: run panel opens and closes as an overlay`)

      overflow.scrollWidth <= overflow.innerWidth + 1
      await page.screenshot({ path: `/tmp/reui-mobile-${width}.png` })
      await page.close()
    }

    assert.deepEqual(errors, [], 'page errors: ' + errors.join(' | '))
    pass('no page errors at either width')
    console.log('RESULT', passed.length, 'checks passed')
  } catch (error) {
    console.error('FAIL', error.message.split('\n')[0])
    try {
      const page = browser.contexts()[0]?.pages()[0]
      if (page) console.error('main:', (await page.locator('main').innerText()).slice(0, 300).replace(/\n/g, ' | '))
    } catch {}
    console.error('page errors:', errors.slice(0, 3))
    process.exitCode = 1
  } finally {
    await browser.close()
    preview.kill()
  }
})()
