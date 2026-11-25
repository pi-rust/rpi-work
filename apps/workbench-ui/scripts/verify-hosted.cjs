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
const suffix = process.platform === 'win32' ? '.exe' : ''
const cli = path.join(repoRoot, `target/debug/work-cli${suffix}`)
const rpc = path.join(repoRoot, `target/debug/work-server-rpc-fixture${suffix}`)
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
  if (!fs.existsSync(rpc)) throw new Error('先 cargo build -p work-node-service --bin work-server-rpc-fixture --locked')
  const data = fs.realpathSync(fs.mkdtempSync(path.join(require('node:os').tmpdir(), 'rpi-work-hosted-')))
  // The existing credential-free RPC fixture opts into native queries by argv.
  // A copied executable name supplies that flag without a platform shell wrapper.
  const isolatedRpc = path.join(data, `creation-native-queries-fixture${suffix}`)
  fs.copyFileSync(rpc, isolatedRpc)
  fs.mkdirSync(path.join(data, 'agent'), { recursive: true })
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

  const server = spawn(cli, ['serve', '--shutdown-stdin'], {
    env: {
      ...process.env,
      RPI_WORK_DATA_DIR: data,
      RPI_CODING_AGENT_DIR: path.join(data, 'agent'),
      RPI_WORK_RPI_BIN: isolatedRpc,
      RPI_WORK_HTTP_BIND: '127.0.0.1',
      RPI_WORK_HTTP_PORT: String(apiPort),
      RPI_WORK_WEB_LISTEN_PORT: String(webPort),
      RPI_WORK_HTTP_PASSWORD: password,
      RPI_WORK_HTTP_ASSETS: path.join(appRoot, 'dist'),
    },
    stdio: ['pipe', 'pipe', 'pipe'],
  })
  const exited = new Promise(resolve => server.once('exit', () => resolve(true)))
  let log = ''
  server.stdout.on('data', (chunk) => (log += chunk))
  server.stderr.on('data', (chunk) => (log += chunk))

  const api = `http://127.0.0.1:${apiPort}`
  const web = `http://127.0.0.1:${webPort}`
  let browser
  try {
    browser = await chromium.launch({ headless: true })
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

    // Real browser writes -> existing Rust client -> real isolated owner. Its
    // agent protocol is synthetic and credential-free, never an installed rpi.
    await page.locator('textarea').fill('real owner: preserve the seeded draft')
    const browserProjectPath = path.join(data, 'browser-project')
    fs.mkdirSync(browserProjectPath)
    const requests = [], snapshots = []
    page.on('request', request => requests.push(request.url()))
    page.on('response', response => { if (new URL(response.url()).pathname.endsWith('/snapshot')) snapshots.push(response) })
    await page.getByRole('button', { name: '添加项目', exact: true }).click()
    const modal = page.getByRole('dialog', { name: '添加项目', exact: true })
    await modal.getByLabel('名称', { exact: true }).fill('browser-created')
    await modal.getByLabel('项目路径', { exact: true }).fill(browserProjectPath)
    await modal.getByRole('button', { name: '添加项目', exact: true }).click()
    await modal.waitFor({ state: 'hidden' })
    const created = JSON.parse(run(['projects'])).find(item => item.path === browserProjectPath)
    assert(created, 'Browser-created project is in the real owner list')
    await page.locator('.wb-project').filter({ hasText: created.name }).waitFor()
    assert.equal(await page.locator('textarea').inputValue(), '')
    pass('browser registers an existing directory and reconciles the real owner project ID')
    const sessionCount = JSON.parse(run(['sessions'])).length
    await page.getByRole('button', { name: '新对话', exact: true }).click()
    await page.getByText('在线', { exact: true }).waitFor()
    await page.locator('.wb-thread[aria-current="true"]').waitFor()
    const realSessions = JSON.parse(run(['sessions']))
    assert.equal(realSessions.length, sessionCount + 1)
    const createdSession = realSessions.find(item => item.projectId === created.id)
    assert(createdSession, 'Real node session has the returned project binding')
    // Sidebar selection precedes the SDK read. Observe the exact successful
    // snapshot response (including one already received), not scheduler timing.
    const targetSnapshot = `/sessions/${createdSession.sessionId}/snapshot`
    const snapshot = snapshots.find(response => new URL(response.url()).pathname.endsWith(targetSnapshot))
      ?? await page.waitForResponse(response => new URL(response.url()).pathname.endsWith(targetSnapshot))
    await snapshot.finished()
    assert(snapshot.ok(), 'the exact returned session snapshot succeeds')
    assert(requests.some(url => url.includes(`/sessions/${createdSession.sessionId}/snapshot`)), 'the canonical SDK opens the exact returned session snapshot')
    assert.equal(await page.locator('textarea').inputValue(), '')
    pass('browser creates and opens one real owner session without a prompt or model call')
    await page.locator('.wb-project').filter({ hasText: 'hosted-fixture' }).click()
    await page.getByText('在线', { exact: true }).waitFor()
    assert.equal(await page.locator('textarea').inputValue(), 'real owner: preserve the seeded draft')
    pass('returning to the original real owner session restores its draft')
    const buttons = name => page.getByRole('button', { name, exact: true })
    const archive = page.getByRole('dialog', { name: '会话操作', exact: true })
    const archiveList = page.getByRole('dialog', { name: '已归档会话', exact: true })
    const currentPreferences = () => JSON.parse(run(['sessions'])).find(item => item.sessionId === session.sessionId)
    const archiveResponse = () => page.waitForResponse(response => new URL(response.url()).pathname.endsWith(`/sessions/${session.sessionId}/metadata`))
    await buttons('会话操作').click()
    const archived = archiveResponse()
    await archive.getByRole('button', { name: '归档会话', exact: true }).click()
    const archiveReceipt = await archived
    assert(archiveReceipt.ok(), `Real owner archive ${archiveReceipt.status()}: ${await archiveReceipt.text()}`)
    assert.equal((await archiveReceipt.json()).archived, true)
    await archive.waitFor({ state: 'hidden' })
    assert.equal(currentPreferences().archived, true)
    await buttons('已归档会话').click()
    const restored = archiveResponse()
    await archiveList.getByRole('button', { name: /^恢复 / }).click()
    const restoreReceipt = await restored
    assert(restoreReceipt.ok(), `Real owner restore ${restoreReceipt.status()}: ${await restoreReceipt.text()}`)
    assert.equal((await restoreReceipt.json()).archived, false)
    await archiveList.waitFor({ state: 'hidden' })
    await page.getByText('在线', { exact: true }).waitFor()
    assert.equal(currentPreferences().archived, false)
    assert.equal(await page.locator('textarea').inputValue(), 'real owner: preserve the seeded draft')
    pass('browser archives and restores the real owner session and recovers its draft')
    const shots = path.join(repoRoot, 'artifacts/confirmed-workbench')
    fs.mkdirSync(shots, { recursive: true })
    await page.screenshot({ path: path.join(shots, 'creation-hosted.png'), animations: 'disabled' })
    assert.deepEqual(errors, [], 'page errors: ' + errors.join(' | '))
    pass('no page errors while running from the node')
    console.log('RESULT', passed.length, 'checks passed')
  } catch (error) {
    console.error('FAIL', error.message.split('\n')[0])
    if (error.stderr) console.error('isolated CLI diagnostic:', error.stderr.toString().slice(0, 1500))
    console.error('page errors:', errors.slice(0, 3))
    console.error('server log tail:', log.split('\n').slice(-6).join(' | '))
    process.exitCode = 1
  } finally {
    await browser?.close()
    server.stdin.end()
    if (!await Promise.race([exited, new Promise(resolve => { const timer = setTimeout(() => resolve(false), 10000); timer.unref() })])) {
      server.kill('SIGTERM')
      await exited
    }
    fs.rmSync(data, { recursive: true, force: true })
  }
})()
