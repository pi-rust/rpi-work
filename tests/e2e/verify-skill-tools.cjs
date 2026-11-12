// Formal WASM + isolated fixture; no real skills, model calls or credential writes.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');
const draft = '技能卡片不得清除这份草稿。';
(async () => {
  fs.mkdirSync('artifacts', { recursive: true });
  const fixture = await startFixture(0), results = [], errors = [];
  const history = [
    { id: 'skill-history', name: 'read', arguments: { path: '/fixture/skills/demo/SKILL.md' }, skill: true },
    { id: 'skill-windows', name: 'functions.READ', arguments: { filePath: 'C:\\fixture\\skills\\demo\\SKILL.md' }, skill: true },
    { id: 'skill-error', name: 'read_file', arguments: { file_path: '/fixture/skills/missing/SKILL.md' }, skill: true, failed: true },
    { id: 'skill-pending', name: 'read', arguments: { path: '/fixture/skills/pending/SKILL.md' }, skill: true, pending: true },
    { id: 'skill-ordinary', name: 'read', arguments: { path: '/fixture/README.md' } },
    { id: 'skill-backup', name: 'read', arguments: { path: '/fixture/SKILL.md.bak' } },
    { id: 'skill-directory', name: 'read', arguments: { directory: '/fixture/SKILL.md' } },
    { id: 'skill-unknown', name: 'skills', arguments: { path: '/fixture/SKILL.md' } },
    { id: 'skill-shell', name: 'bash', arguments: { command: 'cat /fixture/SKILL.md' } },
  ];
  fixture.setMessages([
    { entryId: 'skill-audit-user', role: 'user', content: '技能读取展示回归（fixture）' },
    { entryId: 'skill-audit-calls', role: 'assistant', content: history.map(({ id, name, arguments: args }) => ({ type: 'toolCall', id, name, arguments: args })) },
    ...history.filter(test => !test.pending).map(test => ({ entryId: `result-${test.id}`, role: 'toolResult', toolCallId: test.id, toolName: test.name, isError: Boolean(test.failed), content: [{ type: 'text', text: `Fixture output ${test.id} <script>window.__skillXss=1</script>` }] })),
  ]);
  const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
  const context = await browser.newContext({ viewport: { width: 1280, height: 950 }, permissions: ['clipboard-read', 'clipboard-write'] });
  const page = await context.newPage();
  page.setDefaultTimeout(10000); page.on('pageerror', error => errors.push(error.message));
  await page.addInitScript(() => localStorage.setItem('rpi-work-appearance-v1', JSON.stringify({ theme: 'codex', mode: 'light', density: 'standard', customThemes: [] })));
  const card = id => page.locator(`.inline-call[data-tool-id="${id}"]`);
  const pass = text => { results.push(text); console.log('PASS', text); };
  const shot = name => page.screenshot({ path: `artifacts/skill-tools-${name}.png`, animations: 'disabled' });
  const check = async (test, action, icon) => {
    const row = card(test.id); await row.waitFor();
    assert.equal(await row.count(), 1, 'one row per real tool call');
    assert.equal(await row.locator('.tool-action').innerText(), action);
    assert.equal(await row.locator('.tool-action').getAttribute('title'), test.name, 'native identity retained');
    assert.equal(await row.evaluate(e => e.classList.contains('skill-card')), Boolean(test.skill));
    if (icon) assert.equal(await row.locator('.call-icon svg').getAttribute('data-icon'), icon);
    if (test.pending) {
      await row.locator('.tool-running-spinner').waitFor();
      return;
    }
    await row.scrollIntoViewIfNeeded();
    if (!await row.evaluate(e => e.open)) await row.locator(':scope > summary').click();
    assert.deepEqual(JSON.parse(await row.locator('.call-arguments').textContent()), test.arguments);
    const output = `Fixture output ${test.id} <script>window.__skillXss=1</script>`;
    assert.equal(await row.locator('.call-output').innerText(), output);
    if (test.failed) await row.locator('.inline-call-warning').waitFor();
    await row.getByRole('button', { name: '复制工具输出', exact: true }).click();
    await row.locator('.conversation-copy button[data-copied="true"]').waitFor();
    assert.equal(await page.evaluate(() => navigator.clipboard.readText()), output);
  };
  try {
    await page.route('https://api.github.com/**', route => route.fulfill({ status: 404, body: '{}' }));
    await page.goto(process.env.WORK_UI_URL || 'http://127.0.0.1:14182/app/');
    await page.getByRole('button', { name: '打开设置', exact: true }).click();
    await page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '工作协同', exact: true }).click();
    await page.getByRole('button', { name: '连接其他节点', exact: true }).click();
    await page.getByLabel('节点地址', { exact: true }).fill(fixture.url);
    await page.getByRole('button', { name: '连接节点', exact: true }).click();
    await page.locator('.server-details [role=status]').filter({ hasText: '在线' }).waitFor();
    await page.getByRole('button', { name: '返回会话', exact: true }).click();
    await page.locator('.project-row').filter({ hasText: 'rpi-work' }).click();
    await page.locator('[data-session-id="dark"] .session-item').click();
    await page.locator('.transcript').getByText('技能读取展示回归（fixture）', { exact: true }).waitFor();
    const field = page.getByRole('textbox', { name: '发送给 agent 的消息', exact: true }); await field.fill(draft);
    for (const test of history) await check(test, test.skill ? (test.failed ? '读取技能失败' : test.pending ? '正在读取技能' : 'skills') : test.name, test.pending ? null : test.failed ? 'info' : 'check');
    pass('history: explicit POSIX/Windows skill reads, pending and failure are distinct; normal/backup/directory/unknown/shell calls unchanged; raw input/output/copy preserved');

    fixture.emit({ type: 'agent_start', runId: 'skill-regression' });
    const live = { id: 'skill-live', name: 'read', arguments: { path: '/fixture/skills/live/SKILL.md', limit: 30 }, skill: true };
    fixture.emit({ type: 'tool_execution_start', toolCallId: live.id, toolName: live.name, args: live.arguments });
    await card(live.id).locator('.tool-action').filter({ hasText: '正在读取技能' }).waitFor();
    await check({ ...live, pending: true }, '正在读取技能');
    fixture.emit({ type: 'tool_execution_update', toolCallId: live.id, toolName: live.name, partialResult: { content: [{ type: 'text', text: 'partial skill body' }] } });
    await card(live.id).locator('.call-output').filter({ hasText: 'partial skill body' }).waitFor({ state: 'attached' });
    assert.equal(await card(live.id).locator('.tool-action').innerText(), '正在读取技能', 'partial output is not success');
    fixture.emit({ type: 'tool_execution_end', toolCallId: live.id, toolName: live.name, result: { content: [{ type: 'text', text: `Fixture output ${live.id} <script>window.__skillXss=1</script>` }] } });
    await card(live.id).locator('.call-status[title="已完成"]').waitFor();
    await check(live, 'skills', 'check');
    await card(live.id).locator(':scope > summary').click();
    const failed = { id: 'skill-live-error', name: 'read', arguments: { path: '/fixture/skills/missing/SKILL.md' }, skill: true, failed: true };
    fixture.emit({ type: 'tool_execution_start', toolCallId: failed.id, toolName: failed.name, args: failed.arguments });
    await card(failed.id).locator('.tool-action').filter({ hasText: '正在读取技能' }).waitFor();
    fixture.emit({ type: 'tool_execution_end', toolCallId: failed.id, toolName: failed.name, isError: true, result: { content: [{ type: 'text', text: `Fixture output ${failed.id} <script>window.__skillXss=1</script>` }] } });
    await card(failed.id).locator('.call-status[title="失败"]').waitFor();
    assert(await card(failed.id).evaluate(e => e.open), 'real failure auto-opens');
    await check(failed, '读取技能失败', 'info');
    assert.equal(await card(live.id).evaluate(e => e.open), false, 'unrelated failure does not undo reader collapse');
    fixture.emit({ type: 'agent_settled', runId: 'skill-regression', outcome: 'completed' });
    pass('live start/partial/success/failure follow real events, preserve a single row and user expansion, and do not turn partial output into success');

    const appearance = () => page.getByRole('dialog', { name: '外观与主题', exact: true });
    for (const rendering of ['simple', 'detailed']) {
      for (const theme of ['light', 'dark']) {
        await page.getByRole('button', { name: '外观与主题', exact: true }).click();
        await appearance().getByLabel('工具展示', { exact: true }).selectOption(rendering);
        await appearance().getByLabel('明暗模式', { exact: true }).selectOption(theme);
        await appearance().getByRole('button', { name: '保存', exact: true }).click();
        await appearance().waitFor({ state: 'hidden' });
        for (const width of [1280, 390, 320]) {
          await page.setViewportSize({ width, height: 950 });
          await check(history[0], 'skills', 'check');
          await card(history[0].id).scrollIntoViewIfNeeded();
          assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
          assert(await card(history[0].id).evaluate(e => e.scrollWidth <= e.clientWidth + 1), 'skill card does not overflow');
          if (width < 700) assert((await card(history[0].id).locator(':scope > summary').boundingBox()).height >= 44);
          assert.equal(await field.inputValue(), draft);
          await shot(`${rendering}-${theme}-${width}`);
        }
        pass(`${rendering}/${theme}: skill card fits 1280/390/320, mobile disclosure touch target and draft preserved`);
      }
    }
    assert.equal(await page.evaluate(() => window.__skillXss), undefined);
    assert.equal(fixture.requests.filter(request => request.method !== 'GET' && !(request.method === 'POST' && (request.path.endsWith('/open') || request.body.type?.startsWith('get_')))).length, 0, 'only context open and reads allowed');
    assert.deepEqual(errors, []);
    pass('no prompts, tool execution, configuration writes, implicit answers or script execution');
  } catch (error) {
    await shot('failure'); throw error;
  } finally {
    fs.writeFileSync('artifacts/skill-tools-validation.json', JSON.stringify({ scope: 'Formal WASM + isolated synthetic fixture, not native/model skill execution', results, errors }, null, 2));
    await browser.close(); await fixture.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
