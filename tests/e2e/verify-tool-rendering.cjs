// Formal WASM + local node fixture. Presentation never submits prompts or tool commands.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { chromium } = require('../support/browser.cjs');
const { startFixture, fixtureCommand } = require('../fixtures/ui-fixture.cjs');
const url = process.env.WORK_UI_URL || 'http://127.0.0.1:14182/app/';
const draft = '工具展示只改变视觉，保留当前草稿与执行状态。';
const iconCommand = Array.from({ length: 9 }, (_, i) => `echo '完整命令 ${i} <script>window.__toolIconXss=1</script>'`).join('\n') + '\n';
(async () => {
  fs.mkdirSync('artifacts', { recursive: true });
  const fixture = await startFixture(0);
  // History is independent of the fixture's seeded read/bash SSE replay.
  const argumentCases = [
    { key: 'missing', empty: true },
    { key: 'empty', args: '', empty: true },
    { key: 'whitespace', args: ' \n\t ', empty: true },
    { key: 'null', args: null, empty: true },
    { key: 'json_null', args: 'null', empty: true },
    { key: 'object_empty', args: {}, empty: true },
    { key: 'array_empty', args: [], empty: true },
    { key: 'string_empty', args: '""', empty: true },
    { key: 'false', args: false, expected: 'false' },
    { key: 'zero', args: 0, expected: '0' },
    { key: 'object', args: { path: 'src/icon-regression.rs', limit: 0 }, expected: 'src/icon-regression.rs' },
    { key: 'text', args: 'meaningful fixture arguments', expected: 'meaningful fixture arguments' },
    { key: 'partial', args: '{"path":', expected: '{"path":' },
  ];
  const semantics = [
    ['read', 'file'], ['read_file', 'file'], ['bash', 'terminal'],
    ['write', 'edit'], ['edit', 'edit'], ['apply_patch', 'edit'],
    ['grep', 'search'], ['glob', 'search'], ['search', 'search'],
    ['websearch', 'search'], ['webfetch', 'globe'],
    ['todo', 'plan'], ['update_plan', 'plan'], ['unrecognized_fixture_tool', 'tool'],
  ];
  const historicalCalls = [
    { id: 'icon-command', name: 'icon_command', arguments: { command: iconCommand, timeout: 120 } },
    ...argumentCases.map(test => ({ id: `history-${test.key}`, name: `history_args_${test.key}`, ...(Object.hasOwn(test, 'args') ? { arguments: test.args } : {}) })),
    // Leave read/bash exclusively to the existing seeded events and assertions.
    ...semantics.filter(([name]) => !['read', 'bash'].includes(name)).map(([name]) => ({ id: `history-icon-${name}`, name, arguments: {} })),
  ];
  fixture.setMessages([
    { entryId: 'user-dark', role: 'user', content: '帮我为工作台增加一个深色主题，并保持手机端可用。' },
    { entryId: 'history-tools', role: 'assistant', content: historicalCalls.map(call => ({ type: 'toolCall', ...call })) },
    ...historicalCalls.map(call => ({ entryId: `result-${call.id}`, role: 'toolResult', toolCallId: call.id, toolName: call.name, content: [{ type: 'text', text: `Historical output retained: ${call.name}` }] })),
  ]);
  const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
  const context = await browser.newContext({ viewport: { width: 1440, height: 1000 }, permissions: ['clipboard-read', 'clipboard-write'] });
  const page = await context.newPage(), results = [], errors = [];
  page.setDefaultTimeout(10000); page.on('pageerror', error => errors.push(error.message));
  // Exercise migration from the old saved preference, not just an empty device.
  await page.addInitScript(() => {
    if (!localStorage.getItem('rpi-work-appearance-v1')) localStorage.setItem('rpi-work-appearance-v1', JSON.stringify({ theme: 'codex', mode: 'light', density: 'standard', customThemes: [] }));
  });
  const pass = text => { results.push(text); console.log('PASS', text); };
  const dialog = () => page.getByRole('dialog', { name: '外观与主题', exact: true });
  const open = () => page.getByRole('button', { name: '外观与主题', exact: true }).click();
  const active = mode => page.waitForFunction(value => document.documentElement.dataset.toolRendering === value, mode);
  const save = async () => { await dialog().getByRole('button', { name: '保存', exact: true }).click(); await dialog().waitFor({ state: 'hidden' }); };
  const choose = async mode => { await open(); await dialog().getByLabel('工具展示', { exact: true }).selectOption(mode); await active(mode); await save(); };
  const upload = value => dialog().locator('#appearance-import-file').setInputFiles({ name: 'tools.json', mimeType: 'application/json', buffer: Buffer.from(JSON.stringify(value)) });
  const exported = async label => {
    const [download] = await Promise.all([page.waitForEvent('download'), dialog().getByRole('button', { name: label, exact: true }).click()]);
    return JSON.parse(fs.readFileSync(await download.path(), 'utf8'));
  };
  const shot = name => page.screenshot({ path: `artifacts/tool-rendering-${name}.png`, animations: 'disabled' });
  const call = name => page.locator('.inline-call').filter({ has: page.locator('.call-name > strong').filter({ hasText: new RegExp(`^${name}$`) }) });
  const read = () => call('read').first();
  const assertIcon = async (icon, semantic) => {
    await icon.scrollIntoViewIfNeeded();
    assert(await icon.isVisible(), `${semantic} icon must be visible before measuring`);
    const proof = await icon.evaluate(svg => {
      const style = getComputedStyle(svg), box = svg.getBoundingClientRect(), grid = svg.getBBox();
      return {
        attrs: Object.fromEntries(['viewBox', 'fill', 'stroke', 'stroke-width', 'stroke-linecap', 'stroke-linejoin', 'aria-hidden', 'focusable', 'data-icon'].map(key => [key, svg.getAttribute(key)])),
        stroke: style.stroke, color: style.color, width: style.strokeWidth,
        size: [box.width, box.height], grid: [grid.x, grid.y, grid.width, grid.height], geometry: svg.innerHTML,
      };
    });
    assert.deepEqual(proof.attrs, { viewBox: '0 0 24 24', fill: 'none', stroke: 'currentColor', 'stroke-width': '2', 'stroke-linecap': 'round', 'stroke-linejoin': 'round', 'aria-hidden': 'true', focusable: 'false', 'data-icon': semantic });
    assert.equal(proof.stroke, proof.color); assert.equal(proof.width, '2px');
    assert(proof.size.every(value => value > 0)); assert(proof.geometry.length > 0);
    const [x, y, width, height] = proof.grid;
    assert(x >= 0 && y >= 0 && width > 0 && height > 0 && x + width <= 24 && y + height <= 24, `${semantic} path stays on its 24px grid`);
  };
  const assertSemanticIcons = async () => {
    for (const [name, semantic] of semantics) {
      await assertIcon(call(name).first().locator('.call-icon > svg.icon[data-icon]'), semantic);
      await assertIcon(call(name).first().locator('.tool-success-mark > svg.icon[data-icon]'), 'check');
    }
    await assertIcon(page.locator('.extras-add > svg.icon[data-icon]'), 'paperclip');
  };
  const assertArguments = async (source, test) => {
    const name = `${source}_args_${test.key}`, card = call(name);
    assert.equal(await card.count(), 1, `${name} is selected by exact tool name`);
    await card.scrollIntoViewIfNeeded();
    if (!await card.evaluate(e => e.open)) await card.locator('summary').click();
    if (test.empty) {
      assert.equal(await card.locator('.call-name > code').count(), 0, `${name} does not repeat the tool name as an empty summary`);
      assert.equal(await card.locator('.call-arguments').count(), 0, `${name} omits empty arguments entirely`);
      assert.equal(await card.getByText(/^(完整)?调用参数$/, { exact: true }).count(), 0, `${name} omits the parameter heading`);
    } else {
      await card.locator('.call-arguments').waitFor();
      assert((await card.locator('.call-arguments').innerText()).includes(test.expected), `${name} preserves meaningful/partial arguments`);
      assert.equal(await card.getByRole('img', { name: '调用参数', exact: true }).count(), 1);
    }
    const expected = `${source === 'history' ? 'Historical' : 'Live'} output retained: ${name}`;
    assert.equal(await card.locator('.call-output').innerText(), expected);
    assert(await card.getByRole('img', { name: '工具结果', exact: true }).isVisible());
    await card.getByRole('button', { name: '复制工具输出', exact: true }).click();
    await card.locator('.conversation-copy button[data-copied="true"]').waitFor();
    assert.equal(await page.evaluate(() => navigator.clipboard.readText()), expected);
    await card.locator('summary').click();
  };
  const targetedEvidence = async label => {
    for (const [kind, name] of [['empty-args', 'history_args_object_empty'], ['meaningful-args', 'live_args_object']]) {
      const card = call(name);
      if (!await card.evaluate(e => e.open)) await card.locator('summary').click();
      await card.scrollIntoViewIfNeeded();
      // Wait for the chevron's rotation before measuring; its intermediate
      // diagonal bounds can extend a couple of pixels beyond the flex row.
      await card.evaluate(async element => {
        await Promise.all(element.getAnimations({ subtree: true }).map(animation => animation.finished.catch(() => {})));
      });
      const bounds = await card.evaluate(e => ({ width: e.clientWidth, scroll: e.scrollWidth, children: [...e.children].map(child => ({ tag: child.className, width: child.getBoundingClientRect().width, scroll: child.scrollWidth })) }));
      assert(bounds.scroll <= bounds.width + 1, `${label}/${kind} fits the viewport: ${JSON.stringify(bounds)}`);
      const chrome = await card.evaluate(e => {
        const copy = e.querySelector('.conversation-copy button');
        const icon = e.querySelector('.call-status svg');
        const buttonBox = copy.getBoundingClientRect(), iconBox = icon.getBoundingClientRect();
        return { text: copy.innerText, title: copy.title, label: copy.getAttribute('aria-label'), button: [buttonBox.width, buttonBox.height], icon: [iconBox.width, iconBox.height], status: e.querySelector('.call-status[role="img"]').getAttribute('aria-label'), namedMarkers: [...e.querySelectorAll('.tool-label')].every(marker => marker.title && marker.getAttribute('aria-label')) };
      });
      assert.equal(chrome.text, '', 'copy action is icon-only, not a large text button');
      assert.equal(chrome.title, '复制工具输出'); assert.equal(chrome.label, chrome.title);
      assert.equal(chrome.status, '已完成'); assert(chrome.namedMarkers);
      assert.deepEqual(chrome.icon, [14, 14], 'web status uses a single left 14px outline');
      assert(chrome.button.every(size => size >= (page.viewportSize().width <= 700 ? 44 : 28)), 'small icons retain usable touch targets');
      await card.screenshot({ path: `artifacts/tool-rendering-${label}-${kind}.png`, animations: 'disabled' });
      await card.locator('summary').click();
    }
    // Capture compact named headers, not a giant page containing all history.
    const captured = new Set();
    for (const [name, semantic] of semantics) {
      if (captured.has(semantic)) continue;
      captured.add(semantic);
      const header = call(name).first().locator(':scope > summary');
      await header.scrollIntoViewIfNeeded();
      await header.screenshot({ path: `artifacts/tool-rendering-${label}-semantic-${semantic}.png`, animations: 'disabled' });
    }
    const commandCard = call('icon_command');
    await commandCard.locator(':scope > summary').click();
    const expand = commandCard.getByRole('button', { name: '展开完整命令', exact: true });
    await expand.waitFor();
    assert.equal(await expand.innerText(), '');
    assert.equal(await expand.getAttribute('title'), '展开完整命令');
    assert.deepEqual(await expand.locator('svg').evaluate(e => [e.getBoundingClientRect().width, e.getBoundingClientRect().height]), [12, 12]);
    if (page.viewportSize().width <= 700) assert((await expand.boundingBox()).height >= 44);
    const preview = commandCard.locator('.command-preview');
    await preview.getByRole('button', { name: '复制完整命令', exact: true }).click();
    await preview.locator('button[data-copied="true"]').waitFor();
    assert.equal((await page.evaluate(() => navigator.clipboard.readText())).replace(/\r\n/g, '\n'), iconCommand);
    await expand.focus(); await page.keyboard.press('Enter');
    const collapse = commandCard.getByRole('button', { name: '收起命令', exact: true });
    assert.equal(await collapse.getAttribute('aria-expanded'), 'true');
    assert.equal(await commandCard.locator('.command-text').textContent(), iconCommand);
    const raw = commandCard.locator('.command-raw-arguments');
    assert.equal(await raw.locator('summary').getAttribute('aria-label'), '完整调用参数');
    await raw.locator('summary').click();
    assert.deepEqual(JSON.parse(await raw.locator('.call-arguments').innerText()), { command: iconCommand, timeout: 120 });
    await raw.locator('summary').click();
    await collapse.click();
    assert.equal(await expand.getAttribute('aria-expanded'), 'false');
    assert.equal(await page.evaluate(() => window.__toolIconXss), undefined);
    await commandCard.screenshot({ path: `artifacts/tool-rendering-${label}-command-icons.png`, animations: 'disabled' });
    await commandCard.locator(':scope > summary').click();
  };
  const css = (locator, property) => locator.evaluate((e, p) => getComputedStyle(e).getPropertyValue(p), property);
  const selectSession = async () => {
    await page.locator('.project-row').filter({ hasText: 'rpi-work' }).click();
    await page.locator('.sidebar-sessions').getByRole('button', { name: '为工作台添加深色模式', exact: true }).click();
    await read().waitFor();
  };
  try {
    await page.goto(url); await active('simple');
    await page.getByRole('button', { name: '打开设置', exact: true }).click();
    await page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '工作协同', exact: true }).click();
    await page.getByRole('button', { name: '连接其他节点', exact: true }).click();
    await page.getByLabel('节点地址', { exact: true }).fill(fixture.url);
    await page.getByRole('button', { name: '连接节点', exact: true }).click();
    await page.getByRole('status').filter({ hasText: '在线' }).waitFor();
    await page.getByRole('button', { name: '返回会话', exact: true }).click(); await selectSession();
    await page.locator('.composer textarea').fill(draft);
    assert.equal(await css(read(), 'border-top-width'), '0px');
    assert.equal(await css(read(), 'background-color'), 'rgba(0, 0, 0, 0)');
    assert.equal(await css(read().locator('.call-name'), 'display'), 'flex');
    assert((await read().locator('summary').boundingBox()).height <= 44);
    await shot('simple-light');
    await read().locator('summary').focus(); await page.keyboard.press('Enter');
    await read().locator('.call-arguments').waitFor();
    assert.match(await read().locator('.call-arguments').innerText(), /src\/theme.rs/);
    const output = await read().locator('.call-output').innerText();
    await read().getByRole('button', { name: '复制工具输出', exact: true }).click();
    await read().locator('.conversation-copy button[data-copied="true"]').waitFor();
    assert.equal(await page.evaluate(() => navigator.clipboard.readText()), output);
    await shot('simple-expanded');
    pass('old preferences default to a compact row with keyboard expansion and complete copyable results');
    for (const test of argumentCases) await assertArguments('history', test);
    await assertSemanticIcons();
    pass('historical missing/null/empty arguments omit the complete parameter section; false/0/object/text/partial JSON retain arguments, output and copy');
    pass('all seeded and historical successful tools retain semantic icons and a separate success check; timeline and composer icons retain stroke, 24px grid and decorative accessibility');

    await choose('detailed');
    assert(await read().evaluate(e => e.open), 'mode changes must preserve the native details state');
    assert.equal(await css(read(), 'border-top-width'), '1px');
    assert.equal(await css(read().locator('.call-name'), 'display'), 'grid');
    assert.equal(await read().locator('.call-output').innerText(), output);
    await read().locator('summary').click(); await shot('detailed-light');
    await open(); await dialog().getByLabel('工具展示', { exact: true }).selectOption('simple'); await active('simple');
    await page.keyboard.press('Escape'); await dialog().waitFor({ state: 'hidden' }); await active('detailed');
    assert.equal(await page.locator('.composer textarea').inputValue(), draft);
    await page.reload(); await active('detailed'); await selectSession();
    await page.locator('.composer textarea').fill(draft);
    pass('detailed retains the existing cards; preview cancellation, saved reload and draft are independent');

    await open();
    const full = await exported('导出主题 JSON'), preference = await exported('导出偏好 JSON');
    assert.equal(full.preferredToolRendering, 'detailed'); assert.equal(preference.toolRendering, 'detailed');
    const legacy = { ...preference }; delete legacy.toolRendering;
    for (const bad of [null, true, 'compact', 'simple;display:none']) {
      await upload({ ...full, preferredToolRendering: bad });
      await dialog().locator('.theme-error').filter({ hasText: /工具展示/ }).waitFor(); await active('detailed');
      await upload({ ...preference, toolRendering: bad });
      await dialog().locator('.theme-error').filter({ hasText: /工具展示/ }).waitFor(); await active('detailed');
    }
    await upload(legacy); await active('simple');
    assert.equal((await exported('导出主题 JSON')).preferredToolRendering, undefined);
    await upload(full); await active('detailed'); await save();
    pass('theme and preference exports preserve explicit overrides; old imports follow the theme and invalid enums reject atomically');

    const themed = JSON.parse(fs.readFileSync('examples/themes/codex-reading.json', 'utf8'));
    themed.theme.id = 'tool-rendering-proof'; themed.theme.name = '工具展示主题验证';
    themed.theme.design.toolRendering = 'detailed';
    await open(); await upload(themed); await active('detailed');
    await page.waitForFunction(() => document.documentElement.dataset.theme === 'tool-rendering-proof');
    assert.equal(await dialog().getByLabel('工具展示', { exact: true }).inputValue(), 'theme');
    assert.deepEqual(await exported('导出主题 JSON'), themed);
    await dialog().getByLabel('工具展示', { exact: true }).selectOption('simple'); await active('simple');
    const withOverride = await exported('导出主题 JSON');
    assert.equal(withOverride.theme.design.toolRendering, 'detailed'); assert.equal(withOverride.preferredToolRendering, 'simple');
    for (const bad of ['card', null, { css: 'display:none' }]) {
      const invalid = structuredClone(themed); invalid.theme.id = 'invalid-tool-theme'; invalid.theme.design.toolRendering = bad;
      await upload(invalid); await dialog().locator('.theme-error').filter({ hasText: /视觉规格/ }).waitFor(); await active('simple');
      assert.equal(await dialog().locator('.theme-preset').filter({ hasText: '工具展示主题验证' }).count(), 1);
    }
    await dialog().locator('.theme-preset').filter({ hasText: 'Reddit 橙' }).click(); await active('simple');
    await dialog().locator('.theme-preset').filter({ hasText: 'Codex 黑白' }).click();
    await dialog().getByLabel('工具展示', { exact: true }).selectOption('theme'); await active('simple'); await save();
    pass('the theme supplies its own rendering default; device override wins across palettes without mutating theme identity');

    fixture.emit({ type: 'agent_start', runId: 'tool-rendering-status' });
    for (const test of argumentCases) {
      const name = `live_args_${test.key}`, toolCallId = `live-${test.key}`;
      fixture.emit({ type: 'tool_execution_start', toolCallId, toolName: name, ...(Object.hasOwn(test, 'args') ? { args: test.args } : {}) });
      await call(name).locator('.call-status[title="运行中"]').waitFor();
      fixture.emit({ type: 'tool_execution_end', toolCallId, toolName: name, result: { content: [{ type: 'text', text: `Live output retained: ${name}` }] } });
      await call(name).locator('.call-status[title="已完成"]').waitFor();
      await assertArguments('live', test);
    }
    await targetedEvidence('simple-light-desktop');
    pass('live start/end preserves running/completed status, output and clipboard while hiding only empty parameter sections, including missing/null/{}/[]/empty strings');
    fixture.emit({ type: 'tool_execution_start', toolCallId: 'failed-visual-tool', toolName: 'write', args: { path: 'src/failure.rs' } });
    const failed = call('write').filter({ hasText: 'src/failure.rs' });
    await failed.locator('.call-status[title="运行中"]').waitFor();
    fixture.emit({ type: 'tool_execution_end', toolCallId: 'failed-visual-tool', isError: true, result: { content: [{ type: 'text', text: 'Fixture tool failed; no file was written.' }] } });
    await failed.locator('.call-status[title="失败"]').waitFor();
    fixture.emit({ type: 'ui_dialog_request', requestId: 'tool-visual-ask', toolCallId: 'visual-ask', ui: { purpose: 'question', kind: 'selector', question: '工具模式不应替用户确认', options: ['允许一次', '拒绝'] } });
    const ask = page.locator('[data-request-id="tool-visual-ask"]'); await ask.waitFor();
    for (const mode of ['simple', 'detailed']) {
      await choose(mode);
      assert(await ask.isVisible()); assert.equal(await ask.getByRole('radio').count(), 2);
      assert.equal(await failed.locator('.call-status').getAttribute('title'), '失败');
      if (!await failed.evaluate(e => e.open)) await failed.locator(':scope > summary').click();
      await failed.locator('.inline-call-warning').waitFor();
      assert.match(await failed.locator('.call-output').innerText(), /no file was written/);
      await failed.locator('summary').click();
      for (const theme of ['light', 'dark']) {
        await open(); await dialog().getByLabel('明暗模式', { exact: true }).selectOption(theme); await save();
        await targetedEvidence(`${mode}-${theme}-desktop`);
        for (const width of [390, 320]) {
          await page.setViewportSize({ width, height: 844 });
          assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
          assert(await read().evaluate(e => e.scrollWidth <= e.clientWidth + 1));
          assert((await read().locator('summary').boundingBox()).height >= 44);
          for (const source of ['history', 'live']) for (const test of argumentCases) await assertArguments(source, test);
          await assertSemanticIcons();
          await targetedEvidence(`${mode}-${theme}-${width}`);
          await read().scrollIntoViewIfNeeded();
          await shot(theme === 'light' ? `${mode}-${width}` : `${mode}-${theme}-${width}`);
          assert(await ask.isVisible());
        }
        await page.setViewportSize({ width: 1440, height: 1000 });
      }
    }
    assert.equal(fixture.requests.filter(r => fixtureCommand(r) === 'ui_dialog_response').length, 0);
    fixture.emit({ type: 'ui_dialog_closed', requestId: 'tool-visual-ask' });
    fixture.emit({ type: 'agent_settled', runId: 'tool-rendering-status', outcome: 'completed' });
    await choose('simple');
    await open(); await dialog().getByLabel('明暗模式', { exact: true }).selectOption('dark'); await save(); await shot('simple-dark');
    await choose('detailed'); await shot('detailed-dark');
    assert.equal(await page.locator('.composer textarea').inputValue(), draft);
    pass('running/failure, full results and unanswered Ask remain truthful in both modes at 390/320 and in dark mode');
    pass('simple/detailed × light/dark × desktop/390/320 targeted open empty/meaningful argument and semantic icon screenshots; responsive history/live copy and visible SVG contracts remain intact');
    assert.equal(fixture.requests.filter(r => r.body.type === 'prompt').length, 0);
    assert.deepEqual(errors, []); pass('no model prompts, implicit confirmations or application JavaScript errors');
  } catch (error) {
    await shot('failure'); throw error;
  } finally {
    fs.writeFileSync('artifacts/tool-rendering-validation.json', JSON.stringify({ results, errors }, null, 2));
    await browser.close(); await fixture.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
