// Formal WASM + isolated HTTP/SSE fixture; presentation only, no model/tool execution.
// Run ONLY after the main agent has built the web UI:
//   node tests/e2e/verify-web-tool-display.cjs
// WORK_UI_URL defaults to the same preview as verify-tool-rendering.cjs.
// CHROME_PATH is optional. No saved profile, credentials, prompts or real nodes are used.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');

const url = process.env.WORK_UI_URL || 'http://127.0.0.1:14182/app/';
const draft = '工具展示回归草稿：只展示，不提交、不执行、不覆盖。';
const longPath = 'D:/fixture-only/' + 'very-long-directory-中文/'.repeat(12) + 'source-完整末尾.rs';
const namespacedPath = longPath.replace('source-完整末尾.rs', 'namespaced-read-完整末尾.md');
const source = '# Source heading must remain literal\n<html><body><script>window.__webToolXss=1</script></body></html>\nconst literal = "<img src=x onerror=window.__webToolXss=2>";\n';
const command = Array.from({ length: 9 }, (_, i) => `echo 'fixture-only ${i} <script>window.__webToolXss=3</script>'`).join('\n') + '\n';
const markdown = '# Fixture Markdown heading\n\nA **bold** result with `inline code`.\n\n- first\n- second\n\n<script>window.__webToolXss=4</script>\n<img src=x onerror="window.__webToolXss=5">\n';
const rawHtml = '<section><h1>Not Markdown</h1><script>window.__webToolXss=6</script><img src=x onerror="window.__webToolXss=7"></section>';
const edits = [
  { oldText: 'const before = "<old>";\n  keep_old();', newText: 'const after = "<script>window.__webToolXss=8</script>";\n  keep_new();' },
  { oldText: 'second_old();', newText: 'second_new();' },
];
const editOutput = 'Fixture edit result: permission denied; no file changed. This is the real result, not the preview.';
const history = [
  { id: 'web-display-read', name: 'read', args: { path: longPath, offset: 1, limit: 20 }, icon: 'file', target: longPath, output: source },
  { id: 'web-display-shell', name: 'shell', args: { command, timeout: 120 }, icon: 'terminal', target: command.replace(/\s+/g, ' ').trim(), output: 'Fixture shell result only; nothing was executed.\n<exit status="0">' },
  { id: 'web-display-search', name: 'search', args: { query: 'fixture needle', path: longPath }, icon: 'search', target: `fixture needle · ${longPath}`, output: markdown },
  { id: 'web-display-functions-read', name: 'functions.read', args: { path: namespacedPath }, icon: 'file', target: namespacedPath, output: '# This is source, not a rendered heading\n\n**literal source**\n' },
  { id: 'web-display-html', name: 'fixture_html', args: { description: '<img src=x onerror=window.__webToolXss=9>' }, icon: 'tool', target: '<img src=x onerror=window.__webToolXss=9>', output: rawHtml },
  { id: 'web-display-edit', name: 'edit', args: { path: longPath, edits }, icon: 'edit', target: longPath, output: editOutput },
  { id: 'web-display-history-failed', name: 'shell', args: { command: 'fixture-only failed command' }, target: 'fixture-only failed command', failed: true, output: '# Failure remains raw\nFixture historical failure: no execution.' },
];
// Deliberately split calls into 2/3/2 protocol messages, with suppressed result
// rows between batches. Message boundaries must not create visual tool groups.
const batches = [history.slice(0, 2), history.slice(2, 5), history.slice(5)];
const messages = [
  { entryId: 'web-display-user', role: 'user', content: 'Fixture-only tool display history; never send this as a prompt.' },
  { entryId: 'web-display-before-tools', role: 'assistant', content: '先检查这些文件，再核对工具结果。' },
  ...batches.flatMap((batch, index) => [
    { entryId: `web-display-history-${index}`, role: 'assistant', content: batch.map(item => ({ type: 'toolCall', id: item.id, name: item.name, arguments: item.args })) },
    ...batch.map(item => ({ entryId: `result-${item.id}`, role: 'toolResult', toolCallId: item.id, toolName: item.name, isError: Boolean(item.failed), content: [{ type: 'text', text: item.output }] })),
    { entryId: `private-only-${index}`, role: 'assistant', content: [{ type: 'thinking', thinking: 'Fixture-only private content must not occupy visible space.' }] },
  ]),
  { entryId: 'web-display-after-tools', role: 'assistant', content: [
    { type: 'text', text: '工具检查已完成，这是带头像的回复。' },
    { type: 'toolCall', id: 'web-display-mixed-tool', name: 'read', arguments: { path: 'fixture-only/mixed.rs' } },
  ] },
  { entryId: 'web-display-mixed-result', role: 'toolResult', toolCallId: 'web-display-mixed-tool', toolName: 'read', content: [{ type: 'text', text: 'Mixed tool fixture result' }] },
];

(async () => {
  fs.mkdirSync('artifacts', { recursive: true });
  const results = [], errors = [], geometry = [], screenshots = [], blockedRequests = [];
  let fixture, browser, page, fatal;
  const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
  const until = async (label, predicate, timeout = 12000) => {
    const deadline = Date.now() + timeout;
    while (Date.now() < deadline) {
      if (await predicate()) return;
      await sleep(100);
    }
    assert.fail(`Timed out: ${label}`);
  };
  const test = async (name, body) => {
    console.log(`TEST ${results.length + 1}: ${name}`);
    await body(); results.push(name); console.log(`PASS ${results.length}: ${name}`);
  };
  // IDs are intentionally disjoint from the fixture's seeded read-1/shell-1 SSE.
  const card = id => page.locator(`.inline-call[data-tool-id="${id}"]`);
  const summary = id => card(id).locator(':scope > summary');
  const isOpen = id => card(id).evaluate(element => element.open);
  const setOpen = async (id, open) => {
    if (await isOpen(id) !== open) await summary(id).click();
    await until(`${id} open=${open}`, async () => await isOpen(id) === open);
  };
  const shot = async name => {
    const path = `artifacts/web-tool-display-${name}.png`;
    await page.screenshot({ path, animations: 'disabled' }); screenshots.push(path);
  };
  const emit = event => fixture.emit(event, 'dark');
  const outputIs = (id, expected) => until(`${id} output snapshot`, async () => await card(id).locator('.call-output').textContent() === expected);
  const assertStatus = async (id, label, icon) => {
    const marker = summary(id).locator('.call-icon.call-status[role="img"]');
    await until(`${id}: ${label}`, async () => await marker.getAttribute('aria-label') === label);
    assert.equal(await marker.count(), 1);
    assert.equal(await summary(id).locator('.call-status').count(), 1, 'one left tool/state indicator');
    const success = summary(id).locator('.tool-success-mark');
    assert.equal(await success.count(), label === '已完成' ? 1 : 0, 'only completed tools have a separate success mark');
    if (label === '已完成') {
      assert.equal(await success.locator('svg').getAttribute('data-icon'), 'check');
      assert.equal(await success.getAttribute('aria-hidden'), 'true');
      assert(await success.evaluate(element => element.nextElementSibling === element.parentElement.lastElementChild), 'success precedes the trailing chevron');
    }
    assert(await marker.evaluate(element => element.parentElement.firstElementChild === element), 'tool/state icon is the leftmost summary child');
    assert(await marker.isVisible());
    assert.equal(await marker.getAttribute('title'), label);
    if (label === '运行中') {
      assert.equal(await marker.locator('.tool-running-spinner').count(), 1);
      assert.equal(await marker.locator('svg').count(), 0, 'running has a spinner, not a misleading completed icon');
    } else {
      assert.equal(await marker.locator('.tool-running-spinner').count(), 0);
      const svg = marker.locator('svg');
      assert.equal(await svg.count(), 1);
      assert.equal(await svg.getAttribute('data-icon'), label === '失败' ? 'info' : icon);
      assert.equal(await svg.getAttribute('aria-hidden'), 'true');
    }
    const chevron = summary(id).locator(':scope > svg:last-child');
    assert.equal(await chevron.count(), 1);
    assert.equal(await chevron.getAttribute('data-icon'), 'chevron-right');
    assert(await chevron.evaluate(element => element.parentElement.lastElementChild === element), 'chevron is the trailing direct child');
  };
  const copy = async (id, label, expected) => {
    const button = card(id).getByRole('button', { name: label, exact: true });
    await button.click();
    await until(`${id} copy ack`, async () => await button.getAttribute('data-copied') === 'true');
    assert.equal((await page.evaluate(() => navigator.clipboard.readText())).replace(/\r\n/g, '\n'), expected.replace(/\r\n/g, '\n'), `${label} copies the complete raw payload`);
  };
  const assertSafe = async () => {
    assert.equal(await page.evaluate(() => window.__webToolXss), undefined, 'fixture HTML must never execute');
    for (const item of history) {
      assert.equal(await card(item.id).locator('script, iframe, object, embed, img, [onerror], [onload]').count(), 0, `${item.id}: no executable injected DOM`);
    }
  };
  const assertDraft = async () => assert.equal(await page.locator('.composer textarea').inputValue(), draft);
  const assertNoWrites = () => {
    const commands = fixture.requests.filter(request => /\/commands$/.test(request.path));
    assert.deepEqual(commands.filter(request => !String(request.body.type).startsWith('get_')), [], 'only read-only fixture commands, never prompt/tool execution/model/credential writes');
    assert.deepEqual(fixture.requests.filter(request => !['GET', 'HEAD', 'OPTIONS'].includes(request.method) && !/\/(commands|open)$/.test(request.path)), [], 'no other mutating fixture API calls');
  };

  try {
    fixture = await startFixture(0);
    fixture.setMessages(messages);
    browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
    const context = await browser.newContext({ viewport: { width: 1280, height: 900 }, permissions: ['clipboard-read', 'clipboard-write'] });
    page = await context.newPage();
    page.setDefaultTimeout(10000);
    page.on('pageerror', error => errors.push(error.message));
    // No real model/network fallback: only the preview and ephemeral local fixture.
    // GitHub release checks are unrelated UI startup traffic; satisfy them locally.
    const allowedOrigins = new Set([new URL(url).origin, new URL(fixture.url).origin]);
    await page.route('**/*', route => {
      const requestUrl = new URL(route.request().url());
      if (!['http:', 'https:'].includes(requestUrl.protocol) || allowedOrigins.has(requestUrl.origin)) return route.continue();
      if (requestUrl.origin === 'https://api.github.com') return route.fulfill({ status: 404, contentType: 'application/json', body: '{}' });
      blockedRequests.push(requestUrl.origin + requestUrl.pathname);
      return route.abort('blockedbyclient');
    });
    // Fresh context and legacy appearance without toolRendering prove simple is the default.
    await page.addInitScript(() => localStorage.setItem('rpi-work-appearance-v1', JSON.stringify({ theme: 'codex', mode: 'light', density: 'standard', customThemes: [] })));
    await page.goto(url);
    await page.waitForFunction(() => document.documentElement.dataset.toolRendering === 'simple');
    await page.getByRole('button', { name: '打开设置', exact: true }).click();
    await page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '执行节点', exact: true }).click();
    await page.getByRole('button', { name: '连接其他节点', exact: true }).click();
    await page.getByLabel('节点地址', { exact: true }).fill(fixture.url);
    await page.getByRole('button', { name: '连接节点', exact: true }).click();
    await page.locator('.server-details [role=status]').filter({ hasText: '在线' }).waitFor();
    await page.getByRole('button', { name: '返回会话', exact: true }).click();
    await page.locator('.project-row').filter({ hasText: 'rpi-work' }).click();
    await page.locator('[data-session-id="dark"] .session-item').click();
    await card(history[0].id).waitFor();
    await until('fixture SSE subscription', async () => fixture.requests.some(request => request.path === '/api/v1/sessions/dark/events'));
    await page.locator('.composer textarea').fill(draft);

    await test('completed tools retain type icons with a separate success mark, unchanged failure status, raw names and full targets', async () => {
      for (let index = 0; index < batches.length; index++) {
        const row = page.locator(`[data-history-id="web-display-history-${index}"]`);
        assert.equal(await row.locator(':scope > .timeline-tool-region').count(), batches[index].length, 'each tool is an independent region rather than a spaced protocol batch');
        const group = row.locator(':scope > .timeline-tool-region[role="group"]');
        assert.equal(await group.count(), 1, 'the first tool region carries the accessible group summary');
        assert.equal(await group.getAttribute('aria-label'), `工具执行组，共 ${batches[index].length} 项`);
        assert.equal(await row.locator('article, .assistant-avatar, .assistant-message-heading').count(), 0, 'tool batches have no prose container or heading');
      }
      assert.equal(await page.locator('.message-fork').count(), 0, 'message-level fork shortcut stays removed');
      assert.equal(await page.locator('.user-message .assistant-avatar, .tool-only-message .assistant-avatar').count(), 0, 'users and pure tool batches do not receive assistant avatars');
      for (const id of ['web-display-before-tools', 'web-display-after-tools']) {
        const avatar = page.locator(`[data-history-id="${id}"] .assistant-avatar`);
        assert.equal(await avatar.count(), 1, 'public assistant reply has one local avatar');
        assert.equal(await avatar.getAttribute('aria-hidden'), 'true', 'avatar is decorative, not a duplicate spoken role');
        assert.equal(await avatar.locator('svg').getAttribute('data-icon'), 'code');
      }
      for (const item of history) {
        assert.equal(await card(item.id).count(), 1, 'select by stable tool ID, not ambiguous raw tool names');
        assert.equal(await summary(item.id).locator('.call-name > strong').textContent(), item.name);
        assert.equal(await summary(item.id).locator('.call-name > strong').getAttribute('title'), item.name);
        const target = summary(item.id).locator('.call-name > code');
        assert.equal(await target.textContent(), item.target);
        assert.equal(await target.getAttribute('title'), item.target, 'full title must retain the tail past 96 characters');
        assert(!item.target.startsWith('path='));
        await assertStatus(item.id, item.failed ? '失败' : '已完成', item.icon);
        const resultHint = summary(item.id).locator('.tool-result-hint[role="status"]');
        assert.equal(await resultHint.count(), 1);
        assert.equal(await resultHint.getAttribute('aria-live'), 'polite');
        assert.equal(await resultHint.getAttribute('aria-label'), item.failed ? '结果状态：查看错误' : '结果状态：有结果');
        if (!item.failed) assert.equal(await isOpen(item.id), false, 'completed history starts collapsed');
      }
      for (const id of ['web-display-read', 'web-display-shell', 'web-display-search', 'web-display-functions-read']) {
        assert((await summary(id).locator('.call-name > code').getAttribute('title')).length > 96);
      }
      assert.equal(await isOpen('web-display-history-failed'), true, 'historical failures open by default');
      assert.equal(await card('web-display-history-failed').locator('pre.call-output').textContent(), history.at(-1).output);
      await card('web-display-history-failed').locator('.inline-call-warning').waitFor();
      await setOpen('web-display-history-failed', false);
    });

    await test('edit counts are visible while collapsed and represent argument previews only', async () => {
      const counts = summary('web-display-edit').locator('.edit-count');
      assert(await counts.isVisible());
      assert.equal(await isOpen('web-display-edit'), false);
      assert.equal(await counts.locator('.add').textContent(), '+3');
      assert.equal(await counts.locator('.remove').textContent(), '−3');
      assert.match(await counts.getAttribute('title'), /不代表实际落盘/);
      for (const item of history.filter(item => item.name !== 'edit')) {
        assert.equal(await summary(item.id).locator('.edit-count').count(), 0);
      }
    });

    await test('arguments, read source and raw HTML remain escaped plain text with visible section labels', async () => {
      for (const item of history.filter(item => item.name !== 'shell')) {
        await setOpen(item.id, true);
        assert.deepEqual(JSON.parse(await card(item.id).locator('.call-arguments').textContent()), item.args);
        const labels = card(item.id).locator('.tool-label');
        assert(await labels.count() >= 2);
        for (const label of await labels.all()) {
          const text = label.locator('.tool-label-text');
          assert(await text.isVisible(), 'ToolLabel has visible text, not only a title or aria-label');
          assert.equal(await text.textContent(), await label.getAttribute('aria-label'));
          assert.equal(await label.getAttribute('title'), await text.textContent());
        }
      }
      for (const id of ['web-display-read', 'web-display-functions-read']) {
        const item = history.find(item => item.id === id);
        assert.equal(await card(id).locator('.tool-read-path').textContent(), item.target);
        assert.equal(await card(id).locator('pre.call-output').textContent(), item.output);
        assert.equal(await card(id).locator('.tool-markdown-output, .call-output h1').count(), 0, 'read source never becomes Markdown');
        await copy(id, '复制工具输出', item.output);
      }
      assert.equal(await card('web-display-html').locator('pre.call-output').textContent(), rawHtml);
      await assertSafe();
    });

    await test('Markdown output renders headings but copying returns the raw original Markdown', async () => {
      const output = card('web-display-search').locator('div.call-output.tool-markdown-output');
      await output.getByRole('heading', { name: 'Fixture Markdown heading', exact: true }).waitFor();
      assert.equal(await output.locator('strong').filter({ hasText: /^bold$/ }).textContent(), 'bold');
      assert.equal(await output.locator('li').count(), 2);
      assert.equal(await card('web-display-search').locator('pre.call-output').count(), 0);
      await copy('web-display-search', '复制工具输出', markdown);
      await assertSafe();
    });

    await test('explicit edit pairs preview removals/additions without replacing the truthful tool result', async () => {
      const preview = card('web-display-edit').locator('.tool-edit-preview');
      await preview.waitFor();
      assert.equal(await preview.locator('.tool-read-path').textContent(), longPath);
      assert.deepEqual(await preview.locator('.tool-edit-line.remove code').allTextContents(), edits.flatMap(edit => edit.oldText.split('\n')));
      assert.deepEqual(await preview.locator('.tool-edit-line.add code').allTextContents(), edits.flatMap(edit => edit.newText.split('\n')));
      assert.equal(await card('web-display-edit').locator('pre.call-output').textContent(), editOutput);
      await copy('web-display-edit', '复制工具输出', editOutput);
      assertNoWrites(); await assertSafe();
    });

    await test('shell commands keep a four-line preview, full expansion, raw arguments and full copy', async () => {
      const id = 'web-display-shell';
      await setOpen(id, true);
      const pre = card(id).locator('pre.command-text');
      const expand = card(id).getByRole('button', { name: '展开完整命令', exact: true });
      await expand.waitFor();
      assert.equal(await pre.textContent(), command);
      assert(await pre.evaluate(element => {
        const style = getComputedStyle(element);
        return element.classList.contains('collapsed') && element.scrollHeight > element.clientHeight && element.clientHeight <= parseFloat(style.lineHeight) * 4 + 1;
      }), 'collapsed command has a measured four-line viewport, not a truncated payload');
      await copy(id, '复制完整命令', command);
      await expand.focus(); await page.keyboard.press('Enter');
      const collapse = card(id).getByRole('button', { name: '收起命令', exact: true });
      assert.equal(await collapse.getAttribute('aria-expanded'), 'true');
      assert.equal(await pre.textContent(), command);
      assert.equal(await pre.evaluate(element => element.classList.contains('collapsed')), false);
      const raw = card(id).locator('.command-raw-arguments');
      await raw.locator(':scope > summary').click();
      assert.deepEqual(JSON.parse(await raw.locator('.call-arguments').textContent()), history[1].args);
      await raw.locator(':scope > summary').click();
      await collapse.click();
      assert.equal(await expand.getAttribute('aria-expanded'), 'false');
      await assertSafe();
    });

    await test('live failure opens once; sibling/output/timer updates keep independent stable disclosures and draft focus', async () => {
      const runId = 'web-display-live-run', failedId = 'web-display-live-failure', siblingId = 'web-display-live-sibling';
      emit({ type: 'agent_start', runId });
      emit({ type: 'tool_execution_start', runId, toolCallId: failedId, toolName: 'read', args: { path: longPath } });
      await assertStatus(failedId, '运行中');
      assert.equal(await isOpen(failedId), false);
      const failedNode = await card(failedId).elementHandle();
      emit({ type: 'tool_execution_start', runId, toolCallId: siblingId, toolName: 'read', args: { path: 'fixture-only/sibling.rs' } });
      await assertStatus(siblingId, '运行中');
      assert.equal(await isOpen(siblingId), false, 'another instance does not inherit open state');
      const siblingNode = await card(siblingId).elementHandle();
      await setOpen(siblingId, true);
      emit({ type: 'tool_execution_update', runId, toolCallId: failedId, partialResult: { content: [{ type: 'text', text: 'Fixture running snapshot' }] } });
      await outputIs(failedId, 'Fixture running snapshot');
      assert.equal(await isOpen(failedId), false, 'output alone does not force expansion');
      const end = { type: 'tool_execution_end', runId, toolCallId: failedId, toolName: 'read', isError: true, result: { content: [{ type: 'text', text: 'Fixture live failure' }] } };
      emit(end);
      await assertStatus(failedId, '失败');
      await until('failure automatically expands', () => isOpen(failedId));
      await outputIs(failedId, 'Fixture live failure');
      await setOpen(failedId, false);
      const input = page.locator('.composer textarea');
      await input.focus();
      const focusedInput = await input.elementHandle();
      const elapsed = summary(siblingId).locator('.tool-elapsed');
      const beforeTick = await elapsed.textContent();
      emit({ type: 'tool_execution_update', runId, toolCallId: failedId, partialResult: { content: [{ type: 'text', text: 'Failure late output snapshot' }] } });
      await outputIs(failedId, 'Failure late output snapshot');
      assert.equal(await isOpen(failedId), false, 'late output must not reopen a manually folded failure');
      emit({ ...end, result: { content: [{ type: 'text', text: 'Failure duplicate terminal snapshot' }] } });
      await outputIs(failedId, 'Failure duplicate terminal snapshot');
      await until('a real local timing tick while sibling runs', async () => {
        const value = await elapsed.textContent();
        return value !== beforeTick && /\d/.test(value) && !value.includes('—');
      });
      await sleep(1100); // Cross another tick, not just the immediate event render.
      assert.equal(await isOpen(failedId), false, 'same failure does not reopen after output/terminal/timing rerenders');
      assert.equal(await isOpen(siblingId), true, 'sibling keeps its independently chosen state');
      for (const [node, id] of [[failedNode, failedId], [siblingNode, siblingId]]) {
        assert(await node.evaluate((element, id) => element.isConnected && element === document.querySelector(`.inline-call[data-tool-id="${id}"]`), id), 'keyed component retains the same native details node');
      }
      assert(await focusedInput.evaluate(element => element.isConnected && document.activeElement === element), 'background updates preserve composer focus');
      assert.equal(await isOpen('web-display-history-failed'), false, 'unrelated rerenders do not reopen historical failures either');
      await assertDraft();
      emit({ type: 'tool_execution_end', runId, toolCallId: siblingId, toolName: 'read', result: { content: [{ type: 'text', text: 'Sibling completed fixture result' }] } });
      await assertStatus(siblingId, '已完成', 'file');
      assert.equal(await isOpen(siblingId), true, 'completion does not reset a manually opened component');
      emit({ type: 'agent_settled', runId, outcome: 'failed' });
      await until('both settled tool states stay terminal', async () =>
        await summary(failedId).locator('.call-status').getAttribute('aria-label') === '失败'
        && await summary(siblingId).locator('.call-status').getAttribute('aria-label') === '已完成');
    });

    await test('simple mode light/dark at 1280/390/320 stays bounded with mobile 44px summary targets', async () => {
      for (const mode of ['light', 'dark']) {
        await page.setViewportSize({ width: 1280, height: 900 });
        await page.locator('.app-header .header-more').click();
    await page.getByRole('menuitem', { name: '外观与主题', exact: true }).click();
        const dialog = page.getByRole('dialog', { name: '外观与主题', exact: true });
        await dialog.getByLabel('明暗模式', { exact: true }).selectOption(mode);
        await dialog.getByRole('button', { name: '保存', exact: true }).click();
        await dialog.waitFor({ state: 'hidden' });
        await page.waitForFunction(value => document.documentElement.dataset.mode === value, mode);
        for (const width of [1280, 390, 320]) {
          await page.setViewportSize({ width, height: 900 });
          assert.equal(await page.evaluate(() => document.documentElement.dataset.toolRendering), 'simple');
          for (const item of history) {
            await setOpen(item.id, true);
            await summary(item.id).scrollIntoViewIfNeeded();
            await card(item.id).evaluate(async element => {
              await Promise.all(element.getAnimations({ subtree: true }).filter(animation => animation.effect?.getTiming().iterations !== Infinity).map(animation => animation.finished.catch(() => {})));
            });
            const bounds = await card(item.id).evaluate(element => {
              const header = element.querySelector(':scope > summary'), box = header.getBoundingClientRect();
              return { client: element.clientWidth, scroll: element.scrollWidth, headerClient: header.clientWidth, headerScroll: header.scrollWidth, summaryHeight: box.height, left: box.left, right: box.right, pageClient: innerWidth, pageScroll: document.documentElement.scrollWidth };
            });
            geometry.push({ mode, width, id: item.id, ...bounds });
            assert(bounds.scroll <= bounds.client + 1, `${mode}/${width}/${item.id}: card overflow ${JSON.stringify(bounds)}`);
            assert(bounds.headerScroll <= bounds.headerClient + 1, 'long titles must truncate visually within the header');
            assert(bounds.left >= -1 && bounds.right <= width + 1, 'summary fits the visible viewport');
            assert(bounds.pageScroll <= bounds.pageClient + 1, 'no page-level horizontal overflow');
            if (width <= 767) assert(Math.abs(bounds.summaryHeight - 36) <= 1, 'mobile simple summary is compact');
            else assert(Math.abs(bounds.summaryHeight - 34) <= 1, 'desktop simple summary remains unchanged');
            await assertStatus(item.id, item.failed ? '失败' : '已完成', item.icon);
            const rotation = await summary(item.id).locator(':scope > svg').evaluate(e => {
              const matrix = new DOMMatrixReadOnly(getComputedStyle(e).transform);
              return Math.round(Math.atan2(matrix.b, matrix.a) * 180 / Math.PI);
            });
            assert.equal(rotation, 90, 'right-pointing chevron rotates down, never left, when expanded');
            assert.equal(await summary(item.id).locator('.call-name > code').getAttribute('title'), item.target);
            for (const label of await card(item.id).locator('.tool-label-text').all()) assert(await label.isVisible());
            const path = `artifacts/web-tool-display-${mode}-${width}-${item.id}.png`;
            await card(item.id).screenshot({ path, animations: 'disabled' }); screenshots.push(path);
          }
          for (const item of history) await setOpen(item.id, false);
          const gaps = await page.evaluate(ids => {
            const boxes = ids.map(id => document.querySelector(`.inline-call[data-tool-id="${id}"]`).getBoundingClientRect());
            return boxes.slice(1).map((box, index) => box.top - boxes[index].bottom);
          }, history.map(item => item.id));
          geometry.push({ mode, width, collapsedToolGaps: gaps });
          for (const gap of gaps) assert(Math.abs(gap) <= 1, `2/3/2 batches and empty result/private rows must not add tool spacing: ${gaps}`);
          const boundaries = await page.evaluate(() => {
            const box = selector => document.querySelector(selector).getBoundingClientRect();
            const before = box('[data-history-id="web-display-before-tools"] article');
            const after = box('[data-history-id="web-display-after-tools"] article');
            const first = box('[data-tool-id="web-display-read"]');
            const last = box('[data-tool-id="web-display-history-failed"]');
            const text = box('[data-history-id="web-display-after-tools"] article');
            const mixed = box('[data-tool-id="web-display-mixed-tool"]');
            return { beforeTools: first.top - before.bottom, afterTools: after.top - last.bottom, mixed: mixed.top - text.bottom };
          });
          geometry.push({ mode, width, textToolBoundaries: boundaries });
          assert(Math.abs(boundaries.beforeTools - 20) <= 1, `public region to tool has one 20px gap: ${JSON.stringify(boundaries)}`);
          assert(Math.abs(boundaries.afterTools - 24) <= 1, `separate reply owns 24px even across suppressed result rows: ${JSON.stringify(boundaries)}`);
          assert(Math.abs(boundaries.mixed - 20) <= 1, 'inline text/tool boundary has 20px breathing space');
          await shot(`${mode}-${width}-continuous-tools`);
          await setOpen('web-display-read', true);
          await summary('web-display-read').scrollIntoViewIfNeeded();
          await shot(`${mode}-${width}`);
          await assertDraft(); await assertSafe();
        }
      }
    });

    assertNoWrites();
    assert.deepEqual(blockedRequests, [], 'no attempted external model/resource requests');
    assert.deepEqual(errors, [], 'no application JavaScript errors');
    await assertDraft();
    results.push('zero prompt/model/tool execution/credential writes; draft preserved; no script execution or page errors');
    console.log('PASS safety and draft invariants');
  } catch (error) {
    fatal = error.stack || String(error);
    if (page) await shot('failure').catch(() => {});
    throw error;
  } finally {
    fs.writeFileSync('artifacts/web-tool-display-validation.json', JSON.stringify({ results, errors, fatal, geometry, screenshots, blockedRequests, requests: fixture?.requests }, null, 2));
    if (browser) await browser.close();
    if (fixture) await fixture.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
