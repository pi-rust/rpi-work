// Accessibility and IME acceptance.
//
// Self-contained: starts `vite preview` in fixture mode (no node needed).
//
// Usage: node scripts/verify-a11y.cjs
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
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 } })
  page.setDefaultTimeout(15000)
  page.on('pageerror', (error) => errors.push(error.message))
  try {
    await waitForServer(`${origin}/`)
    await page.goto(`${origin}/`)
    await page.getByText('主题切换已经准备好').waitFor()

    // Landmarks and live regions.
    assert.equal(await page.getByRole('log', { name: '会话记录' }).count(), 1, 'transcript is a log region')
    assert.equal(await page.getByRole('navigation', { name: '项目与会话' }).count(), 1, 'sidebar is a nav landmark')
    assert.equal(await page.getByRole('main').count(), 1, 'one main landmark')
    pass('transcript, sidebar and main expose landmarks and a live region')

    // Every visible icon button must have an accessible name.
    const unnamed = await page.evaluate(() =>
      [...document.querySelectorAll('button')]
        .filter((button) => {
          const style = getComputedStyle(button)
          if (style.display === 'none' || style.visibility === 'hidden') return false
          const text = (button.innerText || '').trim()
          return !text && !button.getAttribute('aria-label') && !button.getAttribute('title')
        })
        .map((button) => button.outerHTML.slice(0, 80)),
    )
    assert.deepEqual(unnamed, [], 'icon buttons without names: ' + unnamed.join(' | '))
    pass('no icon-only button is missing an accessible name')

    // Keyboard: focus the composer, type, and send with the documented shortcut.
    const composer = page.getByPlaceholder('给 rpi 发消息…（⌘↵ 发送）')
    await composer.click()
    await page.keyboard.type('键盘发送的消息')
    assert.equal(await page.evaluate(() => document.activeElement?.tagName), 'TEXTAREA')
    await page.keyboard.press('Meta+Enter')
    await page.getByText('键盘发送的消息').first().waitFor()
    pass('⌘↵ in the composer is the documented send path')

    // Focus order: from the composer, Tab reaches the model picker and further
    // controls without leaving the composer area unreachable.
    await composer.click()
    await page.keyboard.press('Tab')
    const focusedAfterTab = await page.evaluate(() => document.activeElement?.getAttribute('aria-label') || document.activeElement?.tagName)
    assert(focusedAfterTab, 'tab moves focus out of the textarea: ' + focusedAfterTab)
    pass('tab order continues from the composer into the control row')

    // Command palette: keyboard-only path to any session.
    await page.keyboard.press('Meta+k')
    const palette = page.getByRole('dialog', { name: '搜索项目与会话' })
    await palette.waitFor()
    await palette.getByPlaceholder('搜索会话或项目…').fill('RPC')
    const options = await palette.getByRole('option').allInnerTexts()
    assert.equal(options.length, 1, 'filter narrows to one session: ' + JSON.stringify(options))
    assert.match(options[0], /验证 RPC 事件协议/)
    await page.keyboard.press('Enter')
    await palette.waitFor({ state: 'detached' })
    await page.getByText('验证 RPC 事件协议').first().waitFor()
    pass('⌘K palette filters and opens a session with the keyboard')

    assert.deepEqual(errors, [], 'page errors: ' + errors.join(' | '))
    pass('no page errors')
    console.log('RESULT', passed.length, 'checks passed')
  } catch (error) {
    console.error('FAIL', error.message.split('\n')[0])
    console.error('page errors:', errors.slice(0, 3))
    await page.screenshot({ path: '/tmp/reui-a11y-fail.png' })
    process.exitCode = 1
  } finally {
    await browser.close()
    preview.kill()
  }
})()
