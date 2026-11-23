// Own the fixture preview and the suite process group; never touch other servers.
const { spawn } = require('node:child_process')
const path = require('node:path')
const net = require('node:net')
const root = path.resolve(__dirname, '..')
const scripts = ['a11y', 'fixture', 'theme', 'mobile', 'settings', 'tree', 'live', 'hosted', 'browser', 'shell', 'creation', 'entry']
;(async () => {
  const port = await new Promise(resolve => { const server = net.createServer(); server.listen(0, '127.0.0.1', () => { const port = server.address().port; server.close(() => resolve(port)) }) })
  const url = `http://127.0.0.1:${port}/`
  const preview = spawn(process.execPath, [path.join(root, 'node_modules/vite/bin/vite.js'), 'preview', '--mode', 'fixture', '--host', '127.0.0.1', '--port', String(port), '--strictPort'], { cwd: root, stdio: 'ignore', env: { ...process.env, RPI_NODE_URL: '' } })
  let suite
  const stopGroup = () => {
    if (!suite) return
    try { if (process.platform === 'win32') suite.kill('SIGTERM'); else process.kill(-suite.pid, 'SIGTERM') }
    catch (error) { if (error.code !== 'ESRCH') throw error }
  }
  let interrupted = false
  const interrupt = () => {
    interrupted = true
    stopGroup()
    if (preview.exitCode === null && preview.signalCode === null) preview.kill('SIGTERM')
  }
  process.once('SIGINT', interrupt)
  process.once('SIGTERM', interrupt)
  try {
    let ready = false
    for (let i = 0; i < 150 && !interrupted; i++) { try { if ((await fetch(url)).ok) { ready = true; break } } catch {} await new Promise(resolve => setTimeout(resolve, 100)) }
    if (!ready) throw Error('Owned fixture preview failed to start; build:fixture is required')
    for (const script of scripts) {
      if (interrupted) throw Error('Acceptance cancelled')
      suite = spawn(process.execPath, [path.join(__dirname, `verify-${script}.cjs`)], { cwd: root, detached: process.platform !== 'win32', stdio: 'inherit', env: { ...process.env, REUI_URL: url } })
      const code = await new Promise(resolve => suite.once('exit', code => resolve(code ?? 1)))
      stopGroup()
      suite = null
      if (code !== 0) throw Error(`verify:${script} failed (${code})`)
    }
  } finally {
    process.removeListener('SIGINT', interrupt)
    process.removeListener('SIGTERM', interrupt)
    stopGroup()
    if (preview.exitCode === null && preview.signalCode === null) await new Promise(resolve => { preview.once('exit', resolve); preview.kill('SIGTERM') })
  }
})().catch(error => { console.error(error); process.exitCode = 1 })
