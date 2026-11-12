// Formal WASM + isolated fixture. Search and disclosure never mutate session configuration.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');
(async () => {
  fs.mkdirSync('artifacts', { recursive: true });
  const fixture = await startFixture(0);
  fixture.controls.models = [
    { id: 'deepseek/deepseek-v4-pro', name: '当前模型', provider: 'deepseek', reasoning: true },
    { id: 'shared', name: 'Ambiguous OpenAI', provider: 'openai', reasoning: true },
    { id: 'shared', name: 'Ambiguous Beta', provider: 'beta', reasoning: false },
    { id: 'alpha/model-v1', name: '工作台模型', provider: 'alpha', reasoning: false },
    { id: `alpha/${'long-model-'.repeat(24)}`, name: '长名称模型 · '.repeat(12), provider: 'alpha', reasoning: true },
  ];
  fixture.setMessages([
    { entryId: 'polish-user', role: 'user', content: '参考 Web 的交互，保留 provider、tools 和消息的清晰层级。' },
    { entryId: 'polish-agent', role: 'assistant', content: [
      { type: 'text', text: '消息正文应当易读。\n\n工具参数与结果仍然完整保留。' },
      { type: 'toolCall', id: 'polish-read', name: 'read_file', arguments: { path: 'src/features/conversation/very-long-file-name.rs', lines: Array.from({ length: 40 }, (_, i) => `argument ${i}`) } },
    ] },
    { entryId: 'polish-result', role: 'toolResult', toolCallId: 'polish-read', content: [{ type: 'text', text: Array.from({ length: 80 }, (_, i) => `Output line ${i} · 中文工具结果`).join('\n') }] },
  ]);
  const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
  const context = await browser.newContext({ viewport: { width: 1280, height: 900 } });
  const page = await context.newPage(), errors = [], results = [];
  page.setDefaultTimeout(10000);
  page.on('pageerror', error => errors.push(error.message));
  await context.route('https://api.github.com/**', route => route.fulfill({ status: 404, body: '{}' }));
  await page.addInitScript(() => {
    if (!localStorage.getItem('rpi-work-appearance-v1')) localStorage.setItem('rpi-work-appearance-v1', JSON.stringify({ theme: 'codex', mode: 'light', density: 'standard', toolRendering: 'simple', customThemes: [] }));
  });
  const pass = text => { results.push(text); console.log('PASS', text); };
  const trigger = () => page.getByRole('button', { name: '选择供应商与模型', exact: true });
  const dialog = () => page.getByRole('dialog', { name: '选择供应商与模型', exact: true });
  const search = () => dialog().getByRole('searchbox', { name: '搜索供应商或模型', exact: true });
  const options = () => dialog().locator('.composer-model-option');
  const close = async () => {
    await page.keyboard.press('Escape');
    await dialog().waitFor({ state: 'hidden' });
    await page.waitForFunction(() => document.querySelector('.composer-model-picker')?.getAttribute('aria-expanded') === 'false');
  };
  const card = () => page.locator('.inline-call').filter({ has: page.locator('.call-name > strong').filter({ hasText: /^read_file$/ }) });
  try {
    await page.goto(process.env.WORK_UI_URL || 'http://127.0.0.1:14180/');
    await page.getByRole('button', { name: '打开设置', exact: true }).click();
    await page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '工作协同', exact: true }).click();
    await page.getByRole('button', { name: '连接其他节点', exact: true }).click();
    await page.getByLabel('节点地址', { exact: true }).fill(fixture.url);
    await page.getByRole('button', { name: '连接节点', exact: true }).click();
    await page.getByRole('status').filter({ hasText: '在线' }).first().waitFor();
    await page.getByRole('button', { name: '返回会话', exact: true }).click();
    await page.locator('.project-row').filter({ hasText: 'rpi-work' }).click();
    await page.locator('[data-session-id="dark"] .session-item').click();
    await card().waitFor();
    await page.locator('.composer textarea').fill('样式调整保留草稿');
    await trigger().click();
    await page.waitForFunction(() => document.querySelectorAll('.composer-model-option').length === 5);
    assert.equal(await dialog().locator('[aria-pressed="true"]').count(), 1);
    await search().fill('  OPENAI  ');
    await page.waitForFunction(() => document.querySelectorAll('.composer-model-option').length === 1);
    assert(await options().isVisible());
    assert(await options().isDisabled(), 'filtered duplicate ID remains unselectable');
    assert.equal(await search().evaluate(e => e === document.activeElement), true);
    pass('provider search is case-insensitive, trims input, opens matches and preserves ambiguity protection');
    await search().fill('工作台');
    await page.waitForFunction(() => document.querySelectorAll('.composer-model-option').length === 1);
    assert.equal(await options().getAttribute('data-model-id'), 'alpha/model-v1');
    await search().fill('MODEL-V1');
    await page.waitForFunction(() => document.querySelectorAll('.composer-model-option').length === 1);
    assert(await options().isVisible());
    await search().fill('no-such-model');
    await dialog().getByText('没有匹配的模型，请尝试其他名称或清除搜索。', { exact: true }).waitFor();
    assert.equal(await options().count(), 0);
    await dialog().getByRole('button', { name: '清除模型搜索', exact: true }).click();
    await page.waitForFunction(() => document.querySelectorAll('.composer-model-option').length === 5);
    assert.equal(await search().inputValue(), '');
    pass('name and raw ID search, empty result and clear restore the full catalog');
    // A native disclosure toggle must survive a query update within search mode.
    await search().fill('alpha');
    const alpha = dialog().locator('[data-provider="alpha"].composer-provider-group');
    await alpha.locator('summary').click();
    assert.equal(await alpha.evaluate(e => e.open), false);
    await search().fill('alpha/');
    assert.equal(await alpha.evaluate(e => e.open), false);
    await close();
    await trigger().click();
    assert.equal(await search().inputValue(), '', 'reopening starts with a clean catalog');
    await close();
    pass('native expansion survives filtering updates; Escape and reopening work');

    for (const mode of ['light', 'dark']) {
      await page.getByRole('button', { name: '外观与主题', exact: true }).click();
      const appearance = page.getByRole('dialog', { name: '外观与主题', exact: true });
      await appearance.getByLabel('明暗模式', { exact: true }).selectOption(mode);
      await appearance.getByRole('button', { name: '保存', exact: true }).click();
      for (const width of [1280, 390, 320]) {
        await page.setViewportSize({ width, height: 900 });
        if (await card().evaluate(e => e.open)) await card().locator(':scope > summary').click();
        await page.locator('.user-message').getByText('参考 Web 的交互，保留 provider、tools 和消息的清晰层级。', { exact: true }).scrollIntoViewIfNeeded();
        await page.screenshot({ path: `artifacts/provider-tools-polish-${mode}-${width}-messages.png`, animations: 'disabled' });
        await card().locator(':scope > summary').click();
        await card().scrollIntoViewIfNeeded();
        const tool = await card().evaluate(e => {
          const args = e.querySelector('.call-arguments'), output = e.querySelector('.call-output');
          return { width: e.clientWidth, scroll: e.scrollWidth, args: args.clientHeight, argsTotal: args.scrollHeight, output: output.clientHeight, outputTotal: output.scrollHeight };
        });
        assert(tool.scroll <= tool.width + 1, JSON.stringify(tool));
        assert(tool.args <= 140 && tool.argsTotal > tool.args);
        assert(tool.output <= 360 && tool.outputTotal > tool.output);
        await card().locator(':scope > summary').scrollIntoViewIfNeeded();
        await page.screenshot({ path: `artifacts/provider-tools-polish-${mode}-${width}-tools.png`, animations: 'disabled' });
        await trigger().click();
        await search().fill('alpha');
        await page.waitForFunction(() => document.querySelectorAll('.composer-model-option').length === 2);
        assert(await options().first().isVisible());
        const geometry = await dialog().evaluate(e => ({ width: e.clientWidth, scroll: e.scrollWidth, right: e.getBoundingClientRect().right, left: e.getBoundingClientRect().left, view: innerWidth }));
        assert(geometry.scroll <= geometry.width + 1 && geometry.left >= 0 && geometry.right <= geometry.view, JSON.stringify(geometry));
        if (width <= 700) assert.equal(await search().evaluate(e => getComputedStyle(e).fontSize), '16px');
        await page.screenshot({ path: `artifacts/provider-tools-polish-${mode}-${width}-picker.png`, animations: 'disabled' });
        await close();
        assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
      }
      pass(`${mode}: picker, long IDs and bounded tool output fit 1280/390/320px`);
      await page.setViewportSize({ width: 1280, height: 900 });
    }
    assert.equal(await page.locator('.composer textarea').inputValue(), '样式调整保留草稿');
    assert.equal(fixture.requests.filter(r => ['prompt', 'steer', 'follow_up', 'set_model', 'set_thinking_level', 'set_active_tools', 'set_api_key'].includes(r.body?.type)).length, 0);
    assert.deepEqual(errors, []);
    pass('draft retained; zero model/configuration/credential writes and no application JS errors');
  } catch (error) {
    await page.screenshot({ path: 'artifacts/provider-tools-polish-failure.png', animations: 'disabled' });
    throw error;
  } finally {
    fs.writeFileSync('artifacts/provider-tools-polish-validation.json', JSON.stringify({ results, errors }, null, 2));
    await browser.close();
    await fixture.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
