// Theme acceptance: palette, preset catalogue, density and persistence all come
// from `work-viewmodel::theme` through the wasm bridge.
//
// Usage: npm run preview -- --port 5179  &&  REUI_URL=http://127.0.0.1:5179/ node scripts/verify-theme.cjs
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
(async () => {
  const errors = [], passed = [];
  const pass = n => { passed.push(n); console.log('PASS', n); };
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 2 });
  page.setDefaultTimeout(15000);
  page.on('pageerror', e => errors.push(e.message));
  const cssVar = name => page.evaluate(n => getComputedStyle(document.querySelector('.h-dvh')).getPropertyValue(n).trim(), name);
  try {
    await page.goto(process.env.REUI_URL || 'http://127.0.0.1:5179/');
    await page.getByText('主题切换已经准备好').waitFor();
    const codexBg = await cssVar('--background');
    assert.equal(codexBg, '#ffffff', 'codex light bg token: ' + codexBg);
    pass('codex theme CSS variables come from the Rust token map');

    const options = await page.getByLabel('选择主题').locator('option').allTextContents();
    assert(options.length >= 5, 'preset catalogue loaded: ' + options.join(','));
    pass(`theme catalogue exposes ${options.length} presets from theme_contract`);

    // pick a different preset and confirm the variables change
    const target = options.find(o => o !== options[0]) || options[1];
    const targetId = await page.getByLabel('选择主题').locator('option').nth(options.indexOf(target)).getAttribute('value');
    await page.getByLabel('选择主题').selectOption(targetId);
    await page.waitForFunction(prev => getComputedStyle(document.querySelector('.h-dvh')).getPropertyValue('--background').trim() !== prev, codexBg);
    pass('switching preset replaces the whole variable set');

    // dark toggle recomputes from the same contract
    const beforeDark = await cssVar('--background');
    await page.getByRole('button', { name: '切换主题' }).click();
    await page.waitForFunction(prev => getComputedStyle(document.querySelector('.h-dvh')).getPropertyValue('--background').trim() !== prev, beforeDark);
    pass('dark mode recomputes the tokens in Rust');

    // density scales the root font size through density-factor
    const baseSize = await page.evaluate(() => getComputedStyle(document.querySelector('.h-dvh')).fontSize);
    await page.getByRole('button', { name: '切换密度' }).click();
    await page.waitForFunction(prev => getComputedStyle(document.querySelector('.h-dvh')).fontSize !== prev, baseSize);
    const compactSize = await page.evaluate(() => getComputedStyle(document.querySelector('.h-dvh')).fontSize);
    assert(parseFloat(compactSize) < parseFloat(baseSize), `compact ${compactSize} < standard ${baseSize}`);
    pass('density from the contract scales the layout');

    // choice survives a reload
    const persisted = await page.evaluate(() => localStorage.getItem('rpi-work-reui-theme-v1'));
    assert(persisted && persisted.includes('"compact":true'), 'persisted: ' + persisted);
    await page.reload();
    await page.getByText('主题切换已经准备好').waitFor();
    assert.equal(await page.getByRole('button', { name: '切换密度' }).innerText(), '紧凑');
    pass('theme choice persists across reloads');

    // Syntax colours come from the same palette through the css-variables theme.
    const keywordVar = () => page.evaluate(() => getComputedStyle(document.querySelector('.h-dvh')).getPropertyValue('--code-token-keyword').trim());
    const codexKeyword = await keywordVar();
    assert(codexKeyword.length > 0, 'keyword variable present');
    await page.getByRole('button', { name: '切换密度' }).click();
    await page.getByLabel('选择主题').selectOption('codex');
    await page.getByRole('button', { name: '切换主题' }).click();
    await page.waitForTimeout(600);
    assert.notEqual(await keywordVar(), codexKeyword, 'code token colour follows the theme');
    pass('shiki code tokens resolve through --code-token-* from the same contract');

    await page.screenshot({ path: '/tmp/reui-theme.png' });
    assert.deepEqual(errors, []);
    pass('no page errors');
    console.log('RESULT', passed.length, 'checks passed');
  } catch (error) {
    console.error('FAIL', error.message.split('\n')[0]);
    console.error('errors:', errors.slice(0, 3));
    await page.screenshot({ path: '/tmp/reui-theme-fail.png' });
    process.exitCode = 1;
  } finally { await browser.close(); }
})();
