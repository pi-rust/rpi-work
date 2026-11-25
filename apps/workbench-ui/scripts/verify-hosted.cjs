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
    const sessionResponse = action => page.waitForResponse(response => new URL(response.url()).pathname.endsWith(`/sessions/${session.sessionId}/${action}`))
    await buttons('会话操作').click()
    await archive.getByLabel('会话名称', { exact: true }).fill('  同源会话名称  ')
    const renamed = sessionResponse('rename')
    await archive.getByRole('button', { name: '保存', exact: true }).click()
    assert.equal((await (await renamed).json()).title, '同源会话名称')
    await archive.waitFor({ state: 'hidden' })
    await buttons('会话操作').click()
    const pinned = sessionResponse('metadata')
    await archive.getByRole('button', { name: '置顶会话', exact: true }).click()
    assert.equal((await (await pinned).json()).pinned, true)
    await archive.waitFor({ state: 'hidden' })
    await page.getByRole('region', { name: '置顶会话', exact: true }).getByRole('button', { name: '同源会话名称', exact: true }).waitFor()
    pass('browser renames and pins the same session in the real owner and its sidebar')
    await buttons('会话操作').click()
    const archived = sessionResponse('metadata')
    await archive.getByRole('button', { name: '归档会话', exact: true }).click()
    const archiveReceipt = await archived
    assert(archiveReceipt.ok(), `Real owner archive ${archiveReceipt.status()}: ${await archiveReceipt.text()}`)
    assert.equal((await archiveReceipt.json()).archived, true)
    await archive.waitFor({ state: 'hidden' })
    const archivedSession = currentPreferences()
    assert.equal(archivedSession.archived, true)
    assert.equal(archivedSession.pinned, true)
    assert.equal(archivedSession.title, '同源会话名称')
    await buttons('已归档会话').click()
    const restored = sessionResponse('metadata')
    await archiveList.getByRole('button', { name: /^恢复 / }).click()
    const restoreReceipt = await restored
    assert(restoreReceipt.ok(), `Real owner restore ${restoreReceipt.status()}: ${await restoreReceipt.text()}`)
    const restoredPreferences = await restoreReceipt.json()
    assert.equal(restoredPreferences.archived, false)
    assert.equal(restoredPreferences.pinned, true)
    await archiveList.waitFor({ state: 'hidden' })
    await page.getByText('在线', { exact: true }).waitFor()
    await buttons('会话操作').click()
    await archive.getByRole('button', { name: '取消置顶', exact: true }).click()
    await archive.waitFor({ state: 'hidden' })
    const unpinnedSession = currentPreferences()
    assert.equal(unpinnedSession.pinned, false)
    assert.equal(unpinnedSession.archived, false)
    assert.equal(await page.locator('textarea').inputValue(), 'real owner: preserve the seeded draft')
    pass('browser archives and restores the real owner session and recovers its draft')
    await page.getByText('托管历史后续', { exact: true }).waitFor()
    await buttons('会话操作').click()
    const fullForked = sessionResponse('fork')
    await archive.getByRole('button', { name: '从历史分叉', exact: true }).click()
    const fullForkResponse = await fullForked
    assert(fullForkResponse.ok(), `Real owner full fork ${fullForkResponse.status()}: ${await fullForkResponse.text()}`)
    const fullFork = await fullForkResponse.json()
    assert.equal(fullFork.sourceSessionId, session.sessionId)
    assert.equal(fullFork.entryId ?? null, null)
    await page.locator('.wb-window-title').getByText('同源会话名称（分叉）', { exact: true }).waitFor()
    await page.getByText('托管历史后续', { exact: true }).waitFor()
    assert(requests.some(url => url.includes(`/sessions/${fullFork.sessionId}/snapshot`)), 'full fork opens the owner-returned exact destination')
    await buttons('上一个会话').click()
    await page.getByText('在线', { exact: true }).waitFor()
    assert.equal(await page.locator('textarea').inputValue(), 'real owner: preserve the seeded draft')

    const forked = sessionResponse('fork')
    await buttons('从此处开始新对话').click()
    const forkResponse = await forked
    assert(forkResponse.ok(), `Real owner fork ${forkResponse.status()}: ${await forkResponse.text()}`)
    const fork = await forkResponse.json()
    assert.equal(fork.sourceSessionId, session.sessionId)
    assert.equal(fork.entryId, 'fixture-fork-answer')
    assert.notEqual(fork.sessionId, session.sessionId)
    await page.locator('.wb-window-title').getByText('同源会话名称（分叉）', { exact: true }).waitFor()
    await page.getByText('在线', { exact: true }).waitFor()
    await page.getByText('托管历史回答', { exact: true }).waitFor()
    assert.equal(await page.getByText('托管历史后续', { exact: true }).count(), 0)
    assert(requests.some(url => url.includes(`/sessions/${fork.sessionId}/snapshot`)), 'fork opens the owner-returned exact destination')
    assert.equal(await page.locator('textarea').inputValue(), '')
    await buttons('上一个会话').click()
    await page.getByText('在线', { exact: true }).waitFor()
    await page.getByText('托管历史后续', { exact: true }).waitFor()
    assert.equal(await page.locator('textarea').inputValue(), 'real owner: preserve the seeded draft')
    pass('browser forks full history and a canonical answer in the real owner and keeps source history and draft')
    const filter = async mode => {
      await buttons('筛选会话').click()
      await page.getByLabel('显示会话', { exact: true }).selectOption(mode)
    }
    const original = '同源会话名称'
    const branch = page.getByRole('region', { name: '最近会话', exact: true }).getByRole('button', { name: `${original}（分叉）`, exact: true })
    const title = page.locator('.wb-window-title')
    await page.locator('textarea').fill('hosted-filter-permission')
    await buttons('发送').click()
    await page.getByText('托管筛选待审批', { exact: true }).waitFor()
    await branch.first().click()
    await title.getByText(`${original}（分叉）`, { exact: true }).waitFor()
    await page.getByText('在线', { exact: true }).waitFor()
    const approvalRead = page.waitForResponse(response => new URL(response.url()).pathname.endsWith('/api/v1/sessions') && response.request().method() === 'GET')
    await filter('approval')
    const listResponse = await approvalRead
    assert.equal(listResponse.headers()['cache-control'], 'no-store')
    assert.equal((await listResponse.json()).sessions.find(entry => entry.sessionId === session.sessionId).activity, 'awaitingApproval')
    await buttons(original).waitFor()
    assert.equal(await branch.count(), 0)
    await title.getByText(`${original}（分叉）`, { exact: true }).waitFor()
    await filter('running')
    await buttons(original).waitFor()
    await buttons(original).click()
    await page.getByText('托管筛选待审批', { exact: true }).waitFor()
    await buttons('中止').click()
    await page.getByText('没有正在执行的会话', { exact: true }).waitFor()
    await filter('all')
    await branch.first().waitFor()
    pass('browser filters an unselected owner session with pending approval and removes it after settlement')
    // Make one isolated Git diff and re-read it from the real owner before
    // attaching feedback to the existing session draft.
    const runGit = args => execFileSync('git', ['-C', projectPath, ...args], { stdio: 'pipe' })
    runGit(['init', '--quiet', '--template='])
    const reviewPath = 'review-draft.txt'
    fs.writeFileSync(path.join(projectPath, reviewPath), 'line for review\n')
    await page.locator('textarea').fill('真实会话草稿')
    await buttons('文件面板').click()
    const filePanel = page.getByTestId('run-panel')
    await filePanel.getByRole('button', { name: `查看 ${reviewPath} 的 Diff`, exact: true }).click()
    await filePanel.getByRole('button', { name: `在 ${reviewPath} · 新行 1 添加反馈`, exact: true }).click()
    const feedback = page.getByRole('dialog', { name: '行内反馈', exact: true })
    await feedback.getByLabel('反馈内容').fill('复核新增内容')
    const checkedDiff = page.waitForResponse(response => {
      const url = new URL(response.url())
      return url.pathname.endsWith('/git/diff') && url.searchParams.get('path') === reviewPath
    })
    await feedback.getByRole('button', { name: '添加到草稿', exact: true }).click()
    const diffReceipt = await checkedDiff
    assert.equal(diffReceipt.headers()['cache-control'], 'no-store')
    assert.equal((await diffReceipt.json()).staged, false)
    await filePanel.getByRole('status').filter({ hasText: '已添加到当前会话草稿' }).waitFor()
    assert.equal(await page.locator('main textarea').first().inputValue(), '真实会话草稿\n\n行内反馈 · review-draft.txt · 工作区 Diff · 新行 1\n  +line for review\n复核新增内容')
    fs.unlinkSync(path.join(projectPath, reviewPath))
    pass('a real worktree diff is verified through the owner and attached to the existing draft')

    // Build a directory beyond extcore's whole-list limit only after the
    // session workflows, so Git/status checks elsewhere stay small.
    const expectedFiles = Array.from({ length: 2005 }, (_, index) => `file-${String(index).padStart(4, '0')}.txt`)
    for (const name of expectedFiles) fs.writeFileSync(path.join(projectPath, name), name)
    fs.mkdirSync(path.join(projectPath, 'folder-first'))
    fs.writeFileSync(path.join(projectPath, '.env.pagination'), 'hidden fixture')
    if (process.platform !== 'win32') {
      fs.symlinkSync(path.join(projectPath, 'folder-first'), path.join(projectPath, 'symlink'))
      fs.symlinkSync(data, path.join(projectPath, 'outside'))
    }
    const tree = page.getByTestId('run-panel')
    const listingPath = `/api/v1/projects/${projectId}/files`
    const nextPage = cursor => page.waitForResponse(response => {
      const url = new URL(response.url())
      return url.pathname === listingPath && url.searchParams.get('page') === '1' && url.searchParams.get('cursor') === cursor
    })
    const firstPage = nextPage(null)
    await tree.getByTestId('tab-tree').click()
    const firstListing = await firstPage
    assert.equal(firstListing.headers()['cache-control'], 'no-store')
    const first = await firstListing.json()
    assert.equal(first.entries.length, 200)
    assert.equal(first.entries[0].path, 'folder-first')
    assert.equal(first.pagination.complete, false)
    assert(first.pagination.nextCursor)
    const more = tree.getByRole('button', { name: '加载更多项目根目录文件', exact: true })
    const secondPage = nextPage(first.pagination.nextCursor)
    await more.press('Enter')
    const second = await (await secondPage).json()
    assert.equal(second.entries[0].path, expectedFiles[199])
    await tree.getByRole('treeitem', { name: expectedFiles[0], exact: true }).waitFor()
    assert(await tree.getByRole('treeitem').count() < 40)
    const headers = { 'x-rpi-web': '1' }
    const authorization = await firstListing.request().headerValue('authorization')
    if (authorization) headers.authorization = authorization
    const readList = async params => await page.request.get(firstListing.url().split('?')[0], { params, headers })
    const allEntries = [...first.entries, ...second.entries]
    let pagination = second.pagination
    let pages = 2
    while (pagination.nextCursor && pages < 12) {
      const response = await readList({ page: '1', cursor: pagination.nextCursor })
      assert(response.ok(), 'real continuation: ' + await response.text())
      const listing = await response.json()
      assert(listing.entries.length > 0 && listing.entries.length <= 200)
      allEntries.push(...listing.entries)
      pagination = listing.pagination
      pages++
    }
    assert.equal(pages, 11)
    assert.deepEqual(pagination, { complete: true })
    assert.deepEqual(allEntries.map(entry => entry.path), ['folder-first', ...expectedFiles])
    assert.equal((await readList({})).status(), 413, 'legacy listing retains its full-list limit')
    assert.equal((await readList({ page: '1', cursor: 'malformed' })).status(), 400)
    assert.equal((await readList({ page: '1', path: 'folder-first', cursor: first.pagination.nextCursor })).status(), 409)
    assert.equal((await readList({ page: '1', path: '..' })).status(), 403)
    if (process.platform !== 'win32') {
      assert.equal((await readList({ page: '1', path: 'outside' })).status(), 403)
    }
    fs.writeFileSync(path.join(projectPath, 'added.txt'), '')
    await more.press('Enter')
    await tree.getByRole('alert').filter({ hasText: '项目根目录内容已变化' }).waitFor()
    await tree.getByRole('button', { name: '刷新项目根目录', exact: true }).click()
    await tree.getByRole('treeitem', { name: 'added.txt', exact: true }).waitFor()
    pass('real owner pages beyond the legacy limit, enforces project boundaries, and rejects changed listings')
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
