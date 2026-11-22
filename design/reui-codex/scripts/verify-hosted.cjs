// Hosted acceptance: the built prototype served by a real node.
//
// Starts an isolated owner (own data dir, own ports, own password), seeds one
// project and one session through the CLI, serves `dist/` as the node's Web
// assets, then checks that a browser loads the app same-origin, connects
// automatically and can open the seeded session.
//
// Usage: node scripts/verify-hosted.cjs
const path = require('node:path')
const fs = require('node:fs')
const net = require('node:net')
const { spawn, execFileSync } = require('node:child_process')

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
const repoRoot = path.resolve(appRoot, '../..')
const cli = path.join(repoRoot, 'target/debug/work-cli')
const password = 'hosted-verify-token'

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

async function waitForServer(url, timeoutMs = 60000) {
  const deadline = Date.now() + timeoutMs
  while (Date.now() < deadline) {
    try {
      if ((await fetch(url)).ok) return
    } catch {
      /* not listening yet */
    }
    await new Promise((resolve) => setTimeout(resolve, 250))
  }
  throw new Error(`server did not start: ${url}`)
}

;(async () => {
  if (!fs.existsSync(cli)) throw new Error(`缺少 ${cli}；先 cargo build -p work-cli`)
  const data = fs.mkdtempSync(path.join(require('node:os').tmpdir(), 'rpi-work-hosted-'))
  fs.writeFileSync(path.join(data, 'sharing.json'), '{"allow_sharing":true}')
  fs.writeFileSync(path.join(data, 'web-sharing.json'), '{"allow_sharing":true}')
  const apiPort = await freePort()
  const webPort = await freePort()
  const errors = []
  const passed = []
  const pass = (name) => {
    passed.push(name)
    console.log('PASS', name)
  }

  const server = spawn(cli, ['serve'], {
    env: {
      ...process.env,
      RPI_WORK_DATA_DIR: data,
      RPI_WORK_HTTP_BIND: '127.0.0.1',
      RPI_WORK_HTTP_PORT: String(apiPort),
      RPI_WORK_WEB_LISTEN_PORT: String(webPort),
      RPI_WORK_HTTP_PASSWORD: password,
      RPI_WORK_HTTP_ASSETS: path.join(appRoot, 'dist'),
    },
    stdio: ['ignore', 'pipe', 'pipe'],
  })
  let log = ''
  server.stdout.on('data', (chunk) => (log += chunk))
  server.stderr.on('data', (chunk) => (log += chunk))

  const api = `http://127.0.0.1:${apiPort}`
  const web = `http://127.0.0.1:${webPort}`
  const browser = await chromium.launch({ headless: true })
  try {
    await waitForServer(`${web}/`)
    pass('the node serves the built prototype same-origin')

    // Seed one project and one session through the CLI (HTTP mode).
    const run = (args) =>
      execFileSync(cli, ['--url', api, ...args], {
        env: { ...process.env, RPI_WORK_PASSWORD: password },
        stdio: 'pipe',
      }).toString()
    const projectPath = path.join(data, 'hosted-project')
    fs.mkdirSync(projectPath, { recursive: true })
    const project = JSON.parse(run(['project-create', 'hosted-fixture', projectPath]))
    const projectId = project.project?.id ?? project.id
    assert(projectId, 'project created: ' + JSON.stringify(project))
    const session = JSON.parse(run(['session-create', projectId]))
    assert(session.sessionId, 'session created: ' + JSON.stringify(session))
    pass('seeded one project and one session on the isolated node')

    const page = await browser.newPage({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 2 })
    page.setDefaultTimeout(20000)
    page.on('pageerror', (error) => errors.push(error.message))
    await page.goto(`${web}/`)
    // The hosted app auto-connects and asks for the node password.
    await page.getByLabel('节点密码').waitFor()
    await page.getByLabel('节点密码').fill(password)
    await page.getByRole('button', { name: '登录' }).click()

    await page.getByText(projectId, { exact: false }).first().waitFor()
    await page.getByText('在线', { exact: true }).waitFor()
    pass('same-origin auto-connect plus password login reach the workspace')

    const body = await page.locator('main').innerText()
    assert.match(body, new RegExp(projectId), 'project renders: ' + body.slice(0, 120))
    pass('node-hosted bundle lists the seeded project')

    await page.screenshot({ path: '/tmp/reui-hosted.png' })
    assert.deepEqual(errors, [], 'page errors: ' + errors.join(' | '))
    pass('no page errors while running from the node')
    console.log('RESULT', passed.length, 'checks passed')
  } catch (error) {
    console.error('FAIL', error.message.split('\n')[0])
    console.error('page errors:', errors.slice(0, 3))
    console.error('server log tail:', log.split('\n').slice(-6).join(' | '))
    process.exitCode = 1
  } finally {
    await browser.close()
    server.kill()
  }
})()
