// Live-mode acceptance: a real node through the wasm work-client binding.
// Usage: node scripts/live-fixture.cjs   (prints a port)
//        RPI_NODE_URL=http://127.0.0.1:<port> npm run preview -- --port 5180
//        REUI_URL='http://127.0.0.1:5180/?live=1' node scripts/verify-live.cjs
const path = require('node:path')

// Resolve the repository's browser test dependency without a second install.
function resolvePlaywright() {
  try {
    return require.resolve('playwright', { paths: [path.resolve(__dirname, '../../../tools')] })
  } catch {
    throw new Error('先运行 npm ci --prefix tools（仓库根目录），或在 tools/ 下安装 playwright')
  }
}
const assert = require('node:assert/strict');
const { chromium } = require(resolvePlaywright());
const URL = process.env.REUI_URL || 'http://127.0.0.1:5180/?live=1';
(async () => {
  const errors = [], passed = [];
  const pass = n => { passed.push(n); console.log('PASS', n); };
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 2 });
  page.setDefaultTimeout(20000);
  page.on('pageerror', e => errors.push(e.message));
  try {
    await page.goto(URL);
    // sessions come from the node through work-client (wasm)
    await page.getByRole('button', { name: '为工作台添加深色模式', exact: true }).waitFor();
    for (const title of ['梳理 Agent 架构', '检查手机端布局', '验证 RPC 事件协议']) {
      await page.getByRole('button', { name: title, exact: true }).waitFor();
    }
    pass('live node projects and sessions rendered through work-client wasm');

    await page.getByText('在线', { exact: true }).waitFor();
    const header = page.locator('header');
    assert.match(await header.innerText(), /在线/);
    pass('header reports the live connection');

    // switching sessions loads that session's canonical messages
    await page.getByRole('button', { name: '梳理 Agent 架构', exact: true }).click();
    await page.getByText('这是 梳理 Agent 架构 的独立历史。').waitFor();
    pass('selecting a session loads its messages and orders them in Rust');

    await page.getByRole('button', { name: '为工作台添加深色模式', exact: true }).click();
    await page.getByText('帮我为工作台增加一个深色主题，并保持手机端可用。').waitFor();
    pass('switching back restores the first session through the same presenter');

    // P3: live wire events replay through the Rust projection.
    await page.getByText('标题占位', { exact: true }).count();
    await page.getByText('src/theme.rs', { exact: true }).first().waitFor();
    await page.getByText('cargo test -p work-ui --lib', { exact: true }).first().waitFor();
    pass('live WS events produce the read/bash tool cards');

    const transcript = await page.locator('main').innerText();
    assert.match(transcript, /pub fn theme\(\) \{ \/\* test fixture \*\/ \}/, 'read output comes from the event stream');
    await page.getByText('bash', { exact: true }).first().click();
    await page.getByText(/12 passed; 0 failed/).waitFor();
    pass('expanding a live tool card shows the streamed result');

    await page.screenshot({ path: '/tmp/reui-live.png' });
    await page.getByRole('button', { name: '断开节点' }).click();
    await page.getByText('rpiwork').first().waitFor();
    assert.match(await page.locator('header').innerText(), /fixture/);
    pass('disconnect falls back to the fixture without a reload');

    assert.deepEqual(errors, [], 'page errors: ' + errors.join(' | '));
    pass('no page errors');
    console.log('RESULT', passed.length, 'checks passed');
  } catch (error) {
    console.error('FAIL', error.message.split('\n')[0]);
    console.error('page errors:', errors.slice(0, 3));
    await page.screenshot({ path: '/tmp/reui-live-fail.png' });
    process.exitCode = 1;
  } finally { await browser.close(); }
})();
