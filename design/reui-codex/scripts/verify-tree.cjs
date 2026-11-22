// Directory tree acceptance (ReUI Tree on @headless-tree).
//
// Usage: node scripts/verify-tree.cjs
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
const { startFixture } = require(path.resolve(__dirname, '../../../tests/fixtures/ui-fixture.cjs'))
const appRoot = path.resolve(__dirname, '..')
const freePort = () => new Promise(r => { const s = net.createServer(); s.listen(0,'127.0.0.1',()=>{const p=s.address().port;s.close(()=>r(p));}); });
const wait = async u => { for (let i=0;i<100;i++){ try{ if((await fetch(u)).ok) return; }catch{} await new Promise(r=>setTimeout(r,200)); } throw new Error('no server'); };
(async () => {
  const fixture = await startFixture(0, { nativeQueries: true, includeHistoryAssistant: true });
  fixture.controls.fileEntriesByPath = { src: [{ name: 'theme.rs', path: 'src/theme.rs', kind: 'file', size: 12 }] };
  const port = await freePort();
  const preview = spawn('npm',['run','preview','--','--port',String(port),'--strictPort'],{cwd:appRoot,env:{...process.env,RPI_NODE_URL:fixture.url},stdio:'ignore'});
  await wait(`http://127.0.0.1:${port}/`);
  const errors = [];
  const passed = [];
  const pass = (name) => { passed.push(name); console.log('PASS', name) };
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
  page.on('pageerror', e => errors.push(e.message));
  await page.goto(`http://127.0.0.1:${port}/?live=1`);
  await page.getByText('在线', { exact: true }).waitFor();
  await page.getByRole('button', { name: '文件面板' }).click();
  const panel = page.getByTestId('run-panel');
  await panel.getByTestId('tab-tree').click();
  await page.waitForTimeout(800);
  const before = await panel.innerText();
  assert.match(before, /src/, 'root listing renders');
  assert.match(before, /README\.md/, 'root listing includes files');
  pass('ReUI tree renders the project root from the node');
  await panel.getByText('src', { exact: true }).first().click();
  await page.waitForTimeout(1200);
  const after = await panel.innerText();
  assert.match(after, /theme\.rs/, 'expanded folder shows its children');
  pass('expanding a directory loads its children lazily');

  // The fixture's worktree lists src/theme.rs as modified, so the tree must
  // route it to the diff instead of the saved source.
  await panel.getByText('theme.rs', { exact: true }).click();
  await panel.getByText(/let theme = "dark"/).waitFor();
  pass('clicking a changed file opens its diff from the tree');

  assert.deepEqual(errors, [], 'page errors: ' + errors.join(' | '));
  pass('no page errors');
  console.log('RESULT', passed.length, 'checks passed');
  await browser.close(); preview.kill(); await fixture.close();
})();
