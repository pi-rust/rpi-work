// Formal WASM + isolated HTTP/SSE; no real model or tool execution.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');
const WEB = process.env.WORK_UI_URL || 'http://127.0.0.1:14180/';
const prefix = 'tool-newline-';
const output = 'first line\nsecond line\n\nfourth line';
const draft = '工具空行回归草稿，不发送。';
const call = (id, whitespace = '') => ({ entryId: prefix + id, role: 'assistant', content: [
  { type: 'text', text: whitespace },
  { type: 'thinking', text: 'private fixture thought' },
  { type: 'toolCall', id: prefix + id, name: 'newline_fixture', arguments: {} },
] });
const result = id => ({ entryId: prefix + id + '-result', role: 'toolResult', toolCallId: prefix + id, toolName: 'newline_fixture', content: output });
const tool = id => ({ type: 'toolCall', id: prefix + id, name: 'newline_fixture', arguments: {} });
const mixed = [
  { entryId: prefix + 'mixed-a', role: 'assistant', content: [{ type: 'text', text: '正文第一行\n正文第二行' }, tool('a')] },
  result('a'),
  { entryId: prefix + 'mixed-bc', role: 'assistant', content: [tool('b'), { type: 'text', text: '\n \t' }, tool('c')] },
  result('b'), result('c'),
  { entryId: prefix + 'ordered', role: 'assistant', content: [{ type: 'text', text: 'ORDER before' }, tool('ordered'), { type: 'thinking', text: 'private fixture thought' }, { type: 'text', text: 'ORDER after\nnext line' }] },
  result('ordered'),
];
const messages = [...mixed, call('one'), result('one'), call('two', '\r\n\n \t'), result('two'),
  { entryId: prefix + 'blank', role: 'assistant', content: '\n\n' }, call('three'), result('three')];
(async () => {
  fs.mkdirSync('artifacts', { recursive: true });
  const report = { results: [], evidence: {}, errors: [] };
  let fixture, browser, page;
  let baselineProjectionKeys = [];
  const pass = label => { report.results.push(label); console.log('PASS', label); };
  const card = id => page.locator(`[data-tool-id="${prefix}${id}"]`);
  const row = id => page.locator(`[data-history-id="${prefix}${id}"]`);
  const checkMixed = async (stage, compact = true) => {
    const evidence = await page.evaluate(prefix => {
      const card = id => document.querySelector(`[data-tool-id="${prefix}${id}"]`);
      const boxes = ['a', 'b', 'c'].map(id => card(id).getBoundingClientRect());
      const ordered = document.querySelector(`[data-history-id="${prefix}ordered"]`);
      const regions = [...ordered.querySelectorAll(':scope > .timeline-content-region')];
      const prose = document.querySelector(`[data-history-id="${prefix}mixed-a"] article`);
      return {
        gaps: boxes.slice(1).map((box, i) => box.top - boxes[i].bottom),
        order: regions.map(el => el.querySelector('[data-tool-id]') ? 'tool' : el.textContent.includes('ORDER before') ? 'before' : 'after'),
        nested: ['a', 'b', 'c', 'ordered'].some(id => !!card(id).closest('article')),
        readingGap: boxes[0].top - prose.getBoundingClientRect().bottom,
        prose: prose.querySelector('.conversation-markdown > p').innerText,
      };
    }, prefix);
    report.evidence[stage] = evidence;
    assert.deepEqual(evidence.order, ['before', 'tool', 'after'], `${stage}: original content order`);
    assert.equal(evidence.nested, false, `${stage}: tools are not inside prose articles`);
    assert(evidence.readingGap >= 20, `${stage}: prose retains reading space`);
    assert.equal(evidence.prose, '正文第一行\n正文第二行');
    if (compact) for (const gap of evidence.gaps) assert(Math.abs(gap) <= 1, `${stage}: A→B/C compact gap ${gap}`);
    for (const id of ['a', 'b', 'c', 'ordered']) assert.equal(await card(id).count(), 1);
  };
  const inspect = () => page.evaluate(({ prefix, baselineProjectionKeys }) => {
    const rows = ['one', 'two', 'blank', 'three'].map(id => {
      const el = document.querySelector(`[data-history-id="${prefix}${id}"]`);
      return { id, height: el.getBoundingClientRect().height, headings: el.querySelectorAll('.assistant-message-heading').length };
    });
    const boxes = ['one', 'two', 'three'].map(id => document.querySelector(`[data-tool-id="${prefix}${id}"]`).getBoundingClientRect());
    const projection = [...document.querySelectorAll('.projection-message-row')]
      .filter(el => !baselineProjectionKeys.includes(el.dataset.messageRow))
      .map(el => ({ height: el.getBoundingClientRect().height, headings: el.querySelectorAll('.assistant-message-heading').length }));
    return { rows, gaps: boxes.slice(1).map((box, i) => box.top - boxes[i].bottom), projection };
  }, { prefix, baselineProjectionKeys });
  try {
    fixture = await startFixture(0, { pagedHistory: true }); fixture.setMessages(messages);
    // Same IDs in another session must start with native defaults, not inherit
    // this session's disclosure choices (including failed-tool manual closes).
    fixture.setMessages([call('merge-b'), result('merge-b'), call('failed-b'),
      { ...result('failed-b'), isError: true }], 'mobile');
    browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
    const context = await browser.newContext({ viewport: { width: 1280, height: 900 }, permissions: ['clipboard-read', 'clipboard-write'] });
    page = await context.newPage(); page.setDefaultTimeout(12000);
    page.on('pageerror', error => report.errors.push(error.message));
    const origins = new Set([new URL(WEB).origin, new URL(fixture.url).origin]);
    await page.route('**/*', route => {
      const u = new URL(route.request().url());
      if (origins.has(u.origin) || !['http:', 'https:'].includes(u.protocol)) return route.continue();
      if (u.hostname === 'api.github.com') return route.fulfill({ status: 404, body: '{}' });
      return route.abort('blockedbyclient');
    });
    await page.addInitScript(() => localStorage.setItem('rpi-work-appearance-v1', JSON.stringify({ theme: 'codex', mode: 'light', density: 'standard', toolRendering: 'simple', customThemes: [] })));
    await page.goto(WEB);
    await page.getByRole('button', { name: '打开设置', exact: true }).click();
    await page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '工作协同', exact: true }).click();
    await page.getByRole('button', { name: '连接其他节点', exact: true }).click();
    await page.getByLabel('节点地址', { exact: true }).fill(fixture.url);
    await page.getByRole('button', { name: '连接节点', exact: true }).click();
    await page.locator('.server-details [role=status]').filter({ hasText: '在线' }).waitFor();
    await page.getByRole('button', { name: '返回会话', exact: true }).click();
    if (!await page.locator('[data-session-id="dark"] .session-item').isVisible()) await page.locator('.project-row').filter({ hasText: 'rpi-work' }).click();
    await page.locator('[data-session-id="dark"] .session-item').click();
    await card('three').waitFor();
    await checkMixed('history-mixed');
    await page.waitForFunction(() => !document.querySelector('.composer textarea')?.disabled);
    await page.locator('.composer textarea').fill(draft);
    // The fixture seeds unrelated real prose/tools; assert only this run's slots.
    baselineProjectionKeys = await page.locator('.projection-message-row').evaluateAll(rows => rows.map(el => el.dataset.messageRow));
    fixture.emit({ type: 'agent_start', runId: prefix + 'run' });
    fixture.emit({ type: 'message_start', message: { entryId: prefix + 'live', role: 'assistant', content: [] } });
    // A later tool proves the preceding empty snapshot was processed, not just sent.
    fixture.emit({ type: 'tool_execution_start', toolCallId: prefix + 'live-tool', toolName: 'newline_fixture', args: {} });
    await card('live-tool').waitFor();
    await card('live-tool').locator(':scope > summary').click();
    const liveDisclosure = await card('live-tool').elementHandle();
    assert(await liveDisclosure.evaluate(el => el.open), 'user opens the running tool');
    report.evidence.initial = await inspect();
    console.log('INITIAL EVIDENCE', JSON.stringify(report.evidence.initial));
    const red = report.evidence.initial.rows.some(r => r.headings || r.id === 'blank' && r.height > 0) || report.evidence.initial.projection.some(r => r.headings);
    if (red && !fs.existsSync('artifacts/tool-newlines-red-validation.json')) {
      await page.screenshot({ path: 'artifacts/tool-newlines-red.png', animations: 'disabled' });
      fs.writeFileSync('artifacts/tool-newlines-red-validation.json', JSON.stringify(report, null, 2));
    }
    for (const r of report.evidence.initial.rows) {
      assert.equal(r.headings, 0, `${r.id}: whitespace is not public prose and must not add assistant chrome`);
      if (r.id === 'blank') assert.equal(r.height, 0, 'whitespace-only history row is zero-height');
    }
    for (const r of report.evidence.initial.projection) assert.equal(r.headings, 0, 'empty streaming text must not add an assistant heading');
    for (const gap of report.evidence.initial.gaps) assert(Math.abs(gap) <= 1, `no phantom line between tools: ${gap}`);
    pass('history whitespace/tool-only messages and empty live snapshot add no avatar or blank lines');

    // Incremental whitespace must remain in the data; hide only the empty visual block.
    const live = { entryId: prefix + 'live', role: 'assistant' };
    fixture.emit({ type: 'message_update', eventType: 'text_delta', contentIndex: 0, delta: '\n', message: live });
    fixture.emit({ type: 'tool_execution_end', toolCallId: prefix + 'live-tool', toolName: 'newline_fixture', result: 'live complete' });
    await card('live-tool').locator('.call-status[aria-label="已完成"]').waitFor();
    assert(await liveDisclosure.evaluate(el => el.isConnected && el.open), 'output updates preserve the open disclosure and its DOM');
    report.evidence.whitespaceDelta = await inspect();
    assert(report.evidence.whitespaceDelta.projection.every(r => !r.headings), 'newline-only delta stays visually empty');
    fixture.emit({ type: 'message_update', eventType: 'text_delta', contentIndex: 0, delta: '流式第一行\n流式第二行', message: live });
    const paragraph = page.locator('.projection-message-row .conversation-markdown > p').filter({ hasText: '流式第一行' });
    await paragraph.waitFor();
    assert.equal(await paragraph.innerText(), '流式第一行\n流式第二行');
    const liveSlot = await paragraph.evaluateHandle(el => el.closest('[data-message-row]'));
    fixture.emit({ type: 'message_update', message: { ...live, content: [] } });
    await page.waitForFunction(() => ![...document.querySelectorAll('.projection-message-row p')].some(el => el.textContent.includes('流式第一行')));
    assert(await liveSlot.evaluate(el => el.isConnected && el.getBoundingClientRect().height === 0), 'cleared text retains its stable zero-height slot');
    pass('whitespace deltas stay hidden; real prose appears normally; empty authoritative snapshot clears it without deleting the slot');

    for (const rendering of ['simple', 'detailed']) {
      await page.setViewportSize({ width: 1280, height: 900 });
      await page.getByRole('button', { name: '外观与主题', exact: true }).click();
      const dialog = page.getByRole('dialog', { name: '外观与主题', exact: true });
      await dialog.getByLabel('工具展示', { exact: true }).selectOption(rendering);
      await dialog.getByRole('button', { name: '保存', exact: true }).click(); await dialog.waitFor({ state: 'hidden' });
      for (const width of [1280, 390, 320]) {
        await page.setViewportSize({ width, height: 900 });
        const evidence = await inspect(); report.evidence[`${rendering}-${width}`] = evidence;
        await checkMixed(`mixed-${rendering}-${width}`, rendering === 'simple');
        assert(evidence.rows.every(r => !r.headings), 'both presentation modes omit phantom headings');
        assert(evidence.projection.every(r => !r.headings), 'empty projection stays hidden');
        if (rendering === 'simple') assert(evidence.gaps.every(gap => Math.abs(gap) <= 1), 'compact tools stay contiguous at every width');
        assert.equal(await page.locator('.composer textarea').inputValue(), draft);
      }
    }
    await card('two').locator(':scope > summary').click();
    assert.equal(await card('two').locator('.call-output').textContent(), output, 'real output newlines and blank lines stay verbatim');
    await card('two').getByRole('button', { name: '复制工具输出', exact: true }).click();
    await card('two').locator('button[data-copied="true"]').waitFor();
    assert.equal((await page.evaluate(() => navigator.clipboard.readText())).replace(/\r\n/g, '\n'), output);
    assert(!(await page.locator('.transcript').innerText()).includes('private fixture thought'));
    pass('simple/detailed × desktop/mobile retain real output whitespace, raw copy, privacy and draft');

    fixture.emit({ type: 'message_end', message: { ...live, content: [] } });
    // A real live tool projection acquires a mixed canonical owner. Adding
    // public siblings must not replace the keyed card or reset the user's open.
    const liveOwner = { entryId: prefix + 'live-owner', role: 'assistant', content: [
      { type: 'text', text: 'LIVE before\nsecond line' }, tool('live-tool'),
    ] };
    fixture.emit({ type: 'message_end', message: liveOwner });
    fixture.emit({ type: 'message_end', message: { ...result('live-tool'), content: 'live complete' } });
    await row('live-owner').locator('[data-tool-id]').waitFor();
    assert(await liveDisclosure.evaluate(el => el.isConnected && el.open), 'projection→mixed history retains the same open card');
    assert.equal(await card('live-tool').count(), 1);
    assert.equal(await card('live-tool').locator('.call-output').textContent(), 'live complete');
    assert.equal(await card('live-tool').evaluate(el => !!el.closest('article')), false);
    const liveBC = { entryId: prefix + 'live-bc', role: 'assistant', content: [tool('live-b'), tool('live-c')] };
    fixture.emit({ type: 'message_end', message: liveBC });
    fixture.emit({ type: 'message_end', message: result('live-b') });
    fixture.emit({ type: 'message_end', message: result('live-c') });
    const liveOrdered = { entryId: prefix + 'live-ordered', role: 'assistant', content: [
      { type: 'text', text: 'LIVE ordered before' }, tool('live-ordered'), { type: 'text', text: 'LIVE ordered after' },
    ] };
    fixture.emit({ type: 'message_end', message: liveOrdered });
    fixture.emit({ type: 'message_end', message: result('live-ordered') });
    await row('live-ordered').locator('[data-tool-id]').waitFor();
    const checkLiveOrder = async stage => {
      const evidence = await row('live-ordered').locator(':scope > .timeline-content-region').evaluateAll(regions => regions.map(el =>
        el.querySelector('[data-tool-id]') ? 'tool' : el.textContent.includes('LIVE ordered before') ? 'before' : 'after'));
      assert.deepEqual(evidence, ['before', 'tool', 'after'], `${stage}: live text→tool→text`);
      const gaps = await page.evaluate(prefix => {
        const boxes = ['live-tool', 'live-b', 'live-c'].map(id => document.querySelector(`[data-tool-id="${prefix}${id}"]`).getBoundingClientRect());
        return boxes.slice(1).map((box, i) => box.top - boxes[i].bottom);
      }, prefix);
      report.evidence[stage] = { order: evidence, gaps };
      assert.equal(await page.locator('html').getAttribute('data-tool-rendering'), 'simple', `${stage}: compact mode is active`);
      assert(gaps.every(gap => Math.abs(gap) <= 1), `${stage}: live A→B/C compact gaps ${gaps}`);
    };
    // Return to compact mode for live and restored A→B/C measurements.
    await page.setViewportSize({ width: 1280, height: 900 });
    await page.getByRole('button', { name: '外观与主题', exact: true }).click();
    const appearance = page.getByRole('dialog', { name: '外观与主题', exact: true });
    await appearance.getByLabel('工具展示', { exact: true }).selectOption('simple');
    await appearance.getByRole('button', { name: '保存', exact: true }).click();
    await appearance.waitFor({ state: 'hidden' });
    await checkLiveOrder('live-mixed-order');
    pass('live mixed ownership preserves user expansion, result merging, content order and A→B/C compact spacing');

    // Unlike the single-card case above, B loses its outer projection row when
    // A and B acquire one canonical owner. Assert state, not DOM identity.
    for (const id of ['merge-a', 'merge-b', 'failed-a', 'failed-b']) {
      fixture.emit({ type: 'tool_execution_start', toolCallId: prefix + id, toolName: 'newline_fixture', args: {} });
      await card(id).waitFor();
    }
    await card('merge-b').locator(':scope > summary').click();
    await page.waitForFunction(prefix => document.querySelector(`[data-tool-id="${prefix}merge-b"]`).open, prefix);
    fixture.emit({ type: 'tool_execution_end', toolCallId: prefix + 'failed-b', toolName: 'newline_fixture', isError: true, result: 'expected failure' });
    await card('failed-b').locator('.call-status[aria-label="失败"]').waitFor();
    await page.waitForFunction(prefix => document.querySelector(`[data-tool-id="${prefix}failed-b"]`).open, prefix);
    await card('failed-b').locator(':scope > summary').click();
    // Native toggle is asynchronous; wait for the in-memory close before the
    // parent row is removed, rather than racing a synthetic property write.
    await page.waitForFunction(prefix => window.__rpiToolDisclosures?.tools.get(prefix + 'failed-b')?.open === false, prefix);
    for (const pair of ['merge', 'failed']) {
      const owner = { entryId: prefix + pair + '-owner', role: 'assistant', content: [
        { type: 'text', text: `${pair} merged projection tools` }, tool(pair + '-a'), tool(pair + '-b'),
      ] };
      fixture.emit({ type: 'message_end', message: owner });
      fixture.emit({ type: 'message_end', message: result(pair + '-a') });
      fixture.emit({ type: 'message_end', message: pair === 'failed'
        ? { ...result(pair + '-b'), content: 'expected failure', isError: true } : result(pair + '-b') });
      await row(pair + '-owner').locator(`[data-tool-id="${prefix}${pair}-b"]`).waitFor();
    }
    await page.waitForFunction(prefix => document.querySelector(`[data-tool-id="${prefix}merge-b"]`).open, prefix);
    assert.equal(await card('merge-a').evaluate(el => el.open), false, 'untouched A remains closed');
    assert.equal(await card('merge-b').evaluate(el => el.open), true, 'B restores user expansion after outer-row reparent/remount');
    assert.equal(await card('failed-b').evaluate(el => el.open), false, 'manually closed failure stays closed after remount');
    assert(await liveDisclosure.evaluate(el => el.isConnected && el.open), 'unrelated open card is unaffected');
    for (const id of ['merge-a', 'merge-b', 'failed-a', 'failed-b']) assert.equal(await card(id).count(), 1);
    // A subsequent failed output refresh must not reopen a manually closed row.
    fixture.emit({ type: 'message_end', message: { ...result('failed-b'), content: 'expected failure updated', isError: true } });
    await page.waitForFunction(prefix => document.querySelector(`[data-tool-id="${prefix}failed-b"] .call-output`).textContent === 'expected failure updated', prefix);
    assert.equal(await card('failed-b').evaluate(el => el.open), false);
    pass('multi-projection A/B merge preserves B expansion, untouched A and manual failed-tool close');
    fixture.emit({ type: 'agent_end', runId: prefix + 'run' });
    fixture.emit({ type: 'agent_settled', runId: prefix + 'run', outcome: 'completed' });
    await page.getByRole('button', { name: '发送消息', exact: true }).waitFor();
    // Reopen the session to load history afresh without relying on startup's
    // device-dependent node/session selection policy.
    await page.setViewportSize({ width: 1280, height: 900 });
    await page.locator('[data-session-id="mobile"] .session-item').click();
    await page.waitForFunction(() => !document.querySelector('[data-tool-id="tool-newline-three"]'));
    await card('merge-b').waitFor();
    assert.equal(await card('merge-b').evaluate(el => el.open), false, 'same tool ID in another session does not inherit open');
    await page.waitForFunction(prefix => document.querySelector(`[data-tool-id="${prefix}failed-b"]`)?.open, prefix);
    assert.equal(await card('failed-b').evaluate(el => el.open), true, 'new scope first failure opens despite prior scope manual close');
    pass('session scopes isolate identical tool IDs and first-failure defaults');
    await page.locator('[data-session-id="dark"] .session-item').click();
    await card('three').waitFor();
    await card('merge-b').waitFor();
    assert.equal(await card('merge-b').evaluate(el => el.open), false, 'returning to an old scope does not retain its discarded open table');
    await page.waitForFunction(prefix => document.querySelector(`[data-tool-id="${prefix}failed-b"]`)?.open, prefix);
    await checkMixed('reopened-mixed');
    await checkLiveOrder('reopened-live-mixed');
    assert.equal(await card('live-tool').count(), 1);
    assert.equal(await card('live-tool').locator('.call-output').textContent(), 'live complete');
    assert.equal(await row('two').locator('.assistant-message-heading').count(), 0);
    assert.equal(await row('blank').evaluate(el => el.getBoundingClientRect().height), 0);
    assert.equal(await card('two').count(), 1);
    assert.deepEqual(report.errors, []);
    assert(!fixture.requests.some(r => ['prompt', 'queue_message'].includes(r.body?.type)));
    await page.screenshot({ path: 'artifacts/tool-newlines.png', animations: 'disabled' });
    pass('settlement and restored history remain blank-line free with one card per tool and no model sends');
  } catch (error) {
    report.failure = error.stack; process.exitCode = 1; console.error(error);
    await page?.screenshot({ path: 'artifacts/tool-newlines-failure.png', animations: 'disabled' }).catch(() => {});
  } finally {
    fs.writeFileSync('artifacts/tool-newlines-validation.json', JSON.stringify(report, null, 2));
    await browser?.close(); await fixture?.close();
  }
})();
