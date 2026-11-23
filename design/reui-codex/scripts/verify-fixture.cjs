// Fixture-mode acceptance: rows, tool targets, metrics and theme tokens come from wasm.
// Usage: npm run preview -- --port 5179  &&  REUI_URL=http://127.0.0.1:5179/ node scripts/verify-fixture.cjs
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
const URL = process.env.REUI_URL || 'http://127.0.0.1:5178/';
(async () => {
  const errors = [], passed = [];
  const pass = n => { passed.push(n); console.log('PASS', n); };
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 2 });
  page.setDefaultTimeout(15000);
  page.on('pageerror', e => errors.push(e.message));
  try {
    await page.goto(URL);
    await page.getByText('主题切换已经准备好').waitFor();
    pass('rows rendered from the wasm presenter');

    // tool target/labels come from work_viewmodel::conversation::tool_display
    const readRow = page.locator('button', { hasText: 'src/theme.rs' }).first();
    await readRow.waitFor();
    assert.match(await page.locator('body').innerText(), /cargo test -p work-ui --lib/);
    pass('tool targets and shell command come from Rust');

    // footer numbers come from work_viewmodel::observability::metrics
    const footer = page.locator('footer');
    const footerText = await footer.innerText();
    for (const expected of [/↑ 32k/, /↓ 4\.8k/, /缓存 75%/, /上下文 38%/, /\$0\.0184/]) {
      assert.match(footerText, expected, 'footer: ' + footerText);
    }
    pass('metrics formatted by Rust (32k / 4.8k / 75% / 38% / $0.0184)');

    // theme token panel shows the generated contract
    // The confirmed workbench defaults to its browser; runtime diagnostics stay available.
    await page.getByRole('button', { name: '运行面板', exact: true }).click();
    await page.getByText('Rust 主题 token').click();
    await page.getByText('#ffffff').first().waitFor();
    pass('theme tokens read from theme_contract through wasm');

    await page.screenshot({ path: '/tmp/reui-light.png' });
    await page.getByRole('button', { name: '切换主题' }).click();
    await page.waitForTimeout(400);
    await page.screenshot({ path: '/tmp/reui-dark.png' });
    pass('light and dark screenshots captured');

    assert.deepEqual(errors, [], 'page errors: ' + errors.join(' | '));
    pass('no page errors');
    console.log('RESULT', passed.length, 'checks passed');
  } catch (error) {
    console.error('FAIL', error.stack ?? error.message);
    console.error('page errors:', errors.slice(0, 3));
    process.exitCode = 1;
  } finally { await browser.close(); }
})();
