// Formal WASM, local project files and inert source snippets.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');
const sources = {
  'sample.rs': '/* first\n第二行 */\npub fn main() {\n    let name = "<script>window.__sourceExecuted=true</script>";\n    let count = 42;\n}\n',
  'sample.ts': 'export const enabled = true;\nconst name = "work"; // safe\n',
  'sample.py': 'def main():\n    return True, "work", 42 # safe\n',
  'sample.json': '{ "enabled": true, "count": 42 }\n',
  'sample.html': '<!-- safe -->\n<div data-label="work">源代码</div>\n<script>window.__sourceExecuted=true</script>\n',
  'sample.css': '/* safe */ .view { margin-top: 12px; content: "work"; }\n',
  'sample.toml': 'name = "work"\nenabled = true # safe\n',
  'sample.yaml': 'name: "work"\nenabled: true # safe\n',
  'large.rs': Array.from({ length: 900 }, (_, index) => `const label_${index} = "工作区"; // ${'long source '.repeat(8)}`).join('\n'),
  'sample.txt': '<script>window.__sourceExecuted=true</script>\n原始文本。\n',
};
(async () => {
  assert(process.env.WORK_UI_URL, 'Run with tests/support/run-static-ui.cjs');
  fs.mkdirSync('artifacts/code-highlighting', { recursive: true });
  const fixture = await startFixture();
  fixture.controls.fileEntriesByPath = {
    '': Object.entries(sources).map(([name, content]) => ({ name, path: name, kind: 'file', size: Buffer.byteLength(content) })),
  };
  fixture.controls.fileContentByPath = sources;
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
  page.setDefaultTimeout(12000);
  const errors = [], results = [];
  page.on('pageerror', error => errors.push(error.message));
  const files = page.locator('#project-files');
  const button = (name, scope = page) => scope.getByRole('button', { name, exact: true });
  const draft = '代码高亮检查保留草稿，不发送。';
  try {
    await page.addInitScript(address => {
      localStorage.setItem('rpi-work-nodes-v1', JSON.stringify({ version: 1, recent: [address], selected: address }));
      localStorage.setItem('rpi-work-settings-v1', JSON.stringify({ version: 1, appearance: {
        theme: 'codex', mode: 'system', density: 'standard', toolRendering: 'simple', customThemes: [],
      } }));
    }, fixture.url);
    await page.route('https://api.github.com/**', route => route.fulfill({ status: 404, body: '{}' }));
    await page.goto(process.env.WORK_UI_URL);
    await page.locator('[data-session-id=dark] .session-item').first().click();
    await page.locator('.composer textarea').fill(draft);
    await button('打开项目文件').click();
    for (const [name, source] of Object.entries(sources)) {
      await files.locator('.project-file-tree .project-file-row').filter({ hasText: name }).click();
      await files.locator('.project-preview-title small').filter({ hasText: name }).waitFor();
      await button('源码', files).click();
      const code = files.locator('.project-code');
      await code.waitFor();
      const text = await code.locator('.project-code-line > code').allTextContents();
      const expected = source.split('\n');
      if (source.endsWith('\n')) expected.pop();
      assert.deepEqual(text, expected, `${name} preserves source rows`);
      if (name.endsWith('.txt')) assert.equal(await code.locator('[class*="code-token-"]').count(), 0);
      else {
        assert(await code.locator('.code-token-keyword, .code-token-literal').count() > 0, `${name} is highlighted`);
        assert(await code.locator('.code-token-string').count() > 0, `${name} strings are highlighted`);
        assert(await code.locator('[class*="code-token-"]').first().evaluate(element => getComputedStyle(element).color !== getComputedStyle(element.closest('pre')).color));
      }
      if (name === 'sample.rs') assert.equal(await code.locator('[data-line="2"] .code-token-comment').textContent(), '第二行 */');
      assert.equal(await page.evaluate(() => window.__sourceExecuted), undefined);
      results.push(name);
      await button('返回文件列表', files).click();
    }
    console.log('PASS file types, multiline comments, original text and escaped source', results.join(', '));
    for (const mode of ['light', 'dark']) {
      await page.emulateMedia({ colorScheme: mode });
      await page.waitForFunction(mode => document.documentElement.dataset.mode === mode, mode);
      for (const width of [1440, 390, 320]) {
        await page.setViewportSize({ width, height: width < 768 ? 740 : 1000 });
        await files.locator('.project-file-tree .project-file-row').filter({ hasText: 'sample.rs' }).click();
        const code = files.locator('.project-code');
        await code.waitFor();
        for (const wrap of [false, true]) {
          const toggler = button(wrap ? '自动换行' : '关闭换行', files);
          if (await toggler.count()) await toggler.click();
          assert(await code.locator('.code-token-keyword').count() > 0);
          assert(await files.evaluate(element => element.scrollWidth <= element.clientWidth + 1));
        }
        await page.screenshot({ path: `artifacts/code-highlighting/${mode}-${width}.png`, animations: 'disabled' });
        await button('返回文件列表', files).click();
      }
      console.log('PASS source highlighting, wrapping and narrow file viewer', mode);
    }
    await files.locator('.project-change-row').first().click();
    await files.locator('.diff-lines .code-token-keyword').first().waitFor();
    assert.equal(await files.locator('.diff-line.add .code-token-string').first().textContent(), '"dark"');
    assert.equal(await files.locator('.diff-line.del .code-token-string').first().textContent(), '"light"');
    await page.keyboard.press('Escape');
    await files.waitFor({ state: 'hidden' });
    assert.equal(await page.locator('.composer textarea').inputValue(), draft);
    assert.deepEqual(errors, []);
    console.log('PASS diff syntax, addition/deletion rows, safe source and retained draft');
    fs.writeFileSync('artifacts/code-highlighting/validation.json', JSON.stringify({ results, errors }, null, 2));
  } finally {
    await browser.close();
    await fixture.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
