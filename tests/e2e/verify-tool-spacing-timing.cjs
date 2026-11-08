// Formal prebuilt WASM + isolated HTTP/SSE fixture. Commands below are inert data.
// Run: node tests/support/run-static-ui.cjs tests/e2e/verify-tool-spacing-timing.cjs
// No build, saved profile, real credentials, model sends or actual tools.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');
const WEB = process.env.WORK_UI_URL || 'http://127.0.0.1:14182/app/';
const prefix = 'spacing-timing-';
const commands = [
  'cargo test -p work-client --locked && cargo test -p work-ui --locked',
  "rg -n 'verify-message-stream-order|message-stream-order|消息.*顺序|stream order' tests/README.md && git diff --check -- crates/client/work-client/src/lib.rs && node --check tests/e2e/verify-cross-turn-message-identity.cjs",
];
const draft = '工具间距与节点计时回归草稿：不得发送或执行。';
const terminal = elapsedMs => ({ elapsedMs, running: false });
const result = (id, timing, command = 'fixture-only command; never execute') => ({
  entryId: `${prefix}${id}-result`, role: 'toolResult', toolCallId: prefix + id,
  name: 'bash', toolName: 'bash', arguments: { command },
  ...(timing === undefined ? {} : { webTiming: timing }),
  content: [{ type: 'text', text: `Fixture output for ${id}; nothing was executed.` }],
});
const call = (id, timing) => ({
  entryId: `${prefix}${id}-call`, role: 'assistant', content: [{
    type: 'toolCall', id: prefix + id, name: 'bash', arguments: { command: 'fixture-only linked command' },
    ...(timing === undefined ? {} : { webTiming: timing }),
  }],
});
const messages = [
  result('orphan-1', terminal(1234), commands[0]),
  result('orphan-2', terminal(5678), commands[1]),
  call('linked', terminal(9000)), result('linked', terminal(3456)),
  { entryId: `${prefix}private`, role: 'assistant', content: [{ type: 'thinking', thinking: 'Hidden fixture thought, not public text.' }] },
  result('orphan-3', terminal(789)),
  call('fallback', terminal(4321)), result('fallback'),
  { ...result('unknown'), timestamp: 1, startedAt: 100, endedAt: 999999, durationMs: 888888 },
  result('negative', terminal(-1)), result('malformed', terminal('1234')),
  result('running-snapshot', { elapsedMs: 1200, running: true }), result('zero', terminal(0)),
];
const expected = { 'orphan-1': '1.23s', 'orphan-2': '5.68s', linked: '3.46s', 'orphan-3': '789ms', fallback: '4.32s', unknown: '—', negative: '—', malformed: '—', 'running-snapshot': '—', zero: '<1ms' };
const sequence = ['orphan-1', 'orphan-2', 'linked', 'orphan-3'];
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));

(async () => {
  fs.mkdirSync('artifacts', { recursive: true });
  const report = { scope: 'formal prebuilt WASM + isolated HTTP/SSE fixture only', results: [], initialEvidence: null, geometry: [], screenshots: [], errors: [], blockedRequests: [] };
  let fixture, browser, page;
  const card = id => page.locator(`.inline-call[data-tool-id="${prefix}${id}"]`);
  const badge = id => card(id).locator(':scope > summary .tool-elapsed');
  const until = async (label, predicate, timeout = 12000) => {
    const end = Date.now() + timeout;
    while (Date.now() < end) { if (await predicate()) return; await sleep(100); }
    assert.fail(`Timed out: ${label}`);
  };
  const pass = text => { report.results.push(text); console.log('PASS', text); };
  const shot = async name => {
    const path = `artifacts/tool-spacing-timing-${name}.png`;
    await page.screenshot({ path, animations: 'disabled' }); report.screenshots.push(path);
  };
  const measure = async () => page.evaluate(({ prefix, sequence }) => {
    const boxes = sequence.map(id => {
      const element = document.querySelector(`.inline-call[data-tool-id="${prefix}${id}"]`);
      const box = element.getBoundingClientRect();
      return { id, top: box.top, bottom: box.bottom, height: box.height };
    });
    const badges = Object.fromEntries(sequence.map(id => {
      const el = document.querySelector(`[data-tool-id="${prefix}${id}"] .tool-elapsed`);
      return [id, el ? { text: el.textContent.trim(), title: el.title, aria: el.getAttribute('aria-label') } : null];
    }));
    return { boxes, gaps: boxes.slice(1).map((box, i) => box.top - boxes[i].bottom), badges };
  }, { prefix, sequence });
  const timingIs = async (id, text, node = false) => {
    assert.equal(await card(id).count(), 1, `${id}: exactly one stable tool card`);
    assert.equal(await badge(id).count(), 1, `${id}: exactly one duration badge`);
    assert.equal((await badge(id).textContent()).trim(), text, `${id}: recorded metadata only, no timestamp inference`);
    if (node) {
      for (const attribute of ['title', 'aria-label']) {
        const label = await badge(id).getAttribute(attribute);
        assert.match(label || '', /节点执行耗时/, `${id}: source explicitly names node execution`);
        assert.doesNotMatch(label || '', /客户端接收计时/, 'node execution is not client receive timing');
      }
    }
  };
  const containers = async () => page.evaluate(prefix => {
    const ids = ['private', 'linked-result', 'fallback-result'];
    const hidden = ids.map(id => {
      const el = document.querySelector(`[data-history-id="${prefix}${id}"]`);
      if (!el) return { id, height: 0, absent: true };
      const style = getComputedStyle(el);
      return { id, height: el.getBoundingClientRect().height, paddingTop: parseFloat(style.paddingTop), paddingBottom: parseFloat(style.paddingBottom), marginTop: parseFloat(style.marginTop), marginBottom: parseFloat(style.marginBottom) };
    });
    const rows = [...document.querySelectorAll(`[data-history-id^="${prefix}"]`)].filter(el => el.querySelector('.inline-call')).map(el => {
      const style = getComputedStyle(el);
      return { id: el.dataset.historyId, paddingTop: parseFloat(style.paddingTop), paddingBottom: parseFloat(style.paddingBottom) };
    });
    return { hidden, rows };
  }, prefix);
  const zeroContainers = data => {
    for (const row of data.hidden) {
      assert.equal(row.height, 0, `${row.id}: hidden/suppressed container consumes zero height`);
      if (!row.absent) for (const key of ['paddingTop', 'paddingBottom', 'marginTop', 'marginBottom']) assert.equal(row[key], 0, `${row.id}: ${key}`);
    }
    assert(data.rows.length > 0, 'measured real history tool containers');
    for (const row of data.rows) for (const key of ['paddingTop', 'paddingBottom']) assert.equal(row[key], 0, `${row.id}: no extra outer container ${key}`);
  };
  const appearance = async (mode, rendering) => {
    await page.setViewportSize({ width: 1280, height: 900 });
    await page.locator('.app-header .header-more').click();
    await page.getByRole('menuitem', { name: '外观与主题', exact: true }).click();
    const dialog = page.getByRole('dialog', { name: '外观与主题', exact: true });
    await dialog.getByLabel('明暗模式', { exact: true }).selectOption(mode);
    await dialog.getByLabel('工具展示', { exact: true }).selectOption(rendering);
    await dialog.getByRole('button', { name: '保存', exact: true }).click();
    await dialog.waitFor({ state: 'hidden' });
    await page.waitForFunction(({ mode, rendering }) => document.documentElement.dataset.mode === mode && document.documentElement.dataset.toolRendering === rendering, { mode, rendering });
  };
  const session = async id => {
    await page.setViewportSize({ width: 1280, height: 900 });
    await page.locator(`[data-session-id="${id}"] .session-item`).click();
    await until(`${id} SSE subscription`, async () => fixture.requests.some(r => r.path === `/api/v1/sessions/${id}/events`));
  };
  try {
    fixture = await startFixture(0, { pagedHistory: true });
    fixture.setMessages(messages);
    // Same IDs without timing in another scope: never borrow dark's metadata.
    fixture.setMessages([result('orphan-1'), call('live'), result('live')], 'mobile');
    browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
    const context = await browser.newContext({ viewport: { width: 1280, height: 900 }, permissions: ['clipboard-read', 'clipboard-write'] });
    page = await context.newPage(); page.setDefaultTimeout(12000);
    page.on('pageerror', error => report.errors.push(error.message));
    const origins = new Set([new URL(WEB).origin, new URL(fixture.url).origin]);
    await page.route('**/*', route => {
      const u = new URL(route.request().url());
      if (origins.has(u.origin) || !['http:', 'https:'].includes(u.protocol)) return route.continue();
      if (u.origin === 'https://api.github.com') return route.fulfill({ status: 404, body: '{}' });
      report.blockedRequests.push(u.origin + u.pathname); return route.abort('blockedbyclient');
    });
    await page.addInitScript(() => localStorage.setItem('rpi-work-appearance-v1', JSON.stringify({ theme: 'codex', mode: 'light', density: 'standard', toolRendering: 'simple', customThemes: [] })));
    await page.goto(WEB);
    await page.getByRole('button', { name: '打开设置', exact: true }).click();
    await page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '执行节点', exact: true }).click();
    await page.getByRole('button', { name: '连接其他节点', exact: true }).click();
    await page.getByLabel('节点地址', { exact: true }).fill(fixture.url);
    await page.getByRole('button', { name: '连接节点', exact: true }).click();
    await page.locator('.server-details [role=status]').filter({ hasText: '在线' }).waitFor();
    await page.getByRole('button', { name: '返回会话', exact: true }).click();
    if (!await page.locator('[data-session-id="dark"] .session-item').isVisible()) await page.locator('.project-row').filter({ hasText: 'rpi-work' }).click();
    await session('dark');
    await card('orphan-1').waitFor(); await card('orphan-3').waitFor();
    await page.waitForFunction(() => !document.querySelector('.composer textarea')?.disabled);
    await page.locator('.composer textarea').fill(draft);
    await page.waitForFunction(() => document.documentElement.dataset.toolRendering === 'simple');
    // IMPORTANT: collect BOTH independent regressions before any assertions.
    report.initialEvidence = await measure();
    report.initialEvidence.containers = await containers();
    report.initialEvidence.expectedBadges = { 'orphan-1': '1.23s', 'orphan-2': '5.68s' };
    report.initialEvidence.spacingRegression = report.initialEvidence.gaps.some(gap => Math.abs(gap) > 1);
    report.initialEvidence.timingRegression = ['orphan-1', 'orphan-2'].some(id => report.initialEvidence.badges[id]?.text !== expected[id]);
    console.log('INITIAL EVIDENCE', JSON.stringify(report.initialEvidence));
    fs.writeFileSync('artifacts/tool-spacing-timing-validation.json', JSON.stringify(report, null, 2));
    if (report.initialEvidence.spacingRegression || report.initialEvidence.timingRegression) {
      const red = 'artifacts/tool-spacing-timing-red-validation.json';
      // Preserve the first pre-fix evidence permanently across final reruns.
      if (!fs.existsSync(red)) {
        await shot('red-initial');
        fs.writeFileSync(red, JSON.stringify(report, null, 2));
      }
    }
    assert(report.initialEvidence.gaps.every(gap => Math.abs(gap) <= 1), `orphan/linked tool gaps must be <=1px: ${report.initialEvidence.gaps}; badge evidence: ${JSON.stringify(report.initialEvidence.badges)}`);
    for (const [id, text] of Object.entries(expected)) await timingIs(id, text, !['unknown', 'negative', 'malformed', 'running-snapshot'].includes(id));
    zeroContainers(report.initialEvidence.containers);
    pass('adjacent orphan/linked/orphan tools have no gaps; result metadata wins; terminal call metadata fallback; invalid/unknown snapshots stay unknown');

    for (const mode of ['light', 'dark']) for (const rendering of ['simple', 'detailed']) {
      await appearance(mode, rendering);
      for (const width of [1280, 390, 320]) {
        await page.setViewportSize({ width, height: 900 });
        const geometry = { mode, rendering, width, ...await measure(), containers: await containers() };
        report.geometry.push(geometry);
        // Detailed cards intentionally retain their own margins; only outer
        // history-container padding and hidden rows are required to be zero.
        zeroContainers(geometry.containers);
        if (rendering === 'simple') {
          for (const gap of geometry.gaps) assert(Math.abs(gap) <= 1, `${mode}/${width}: gap ${gap}`);
          for (const box of geometry.boxes) {
            if (width > 767) assert(Math.abs(box.height - 34) <= 1, `${mode}/${width}: desktop tool row remains unchanged ${box.height}`);
            else assert(Math.abs(box.height - 36) <= 1, `${mode}/${width}: compact mobile tool row ${box.height}`);
          }
        }
        for (const [id, text] of Object.entries(expected)) await timingIs(id, text);
        assert.equal(await page.locator('.composer textarea').inputValue(), draft);
        await card('orphan-1').scrollIntoViewIfNeeded();
        await shot(`${mode}-${rendering}-${width}`);
      }
    }
    pass('light/dark × 1280/390/320 simple gaps and detailed outer containers; draft preserved');
    await appearance('light', 'simple');
    await card('orphan-1').locator(':scope > summary').click();
    const bodySpace = await card('orphan-1').locator(':scope > .call-body').evaluate(el => { const s = getComputedStyle(el); return [parseFloat(s.marginBottom), parseFloat(s.paddingBottom)]; });
    assert.deepEqual(bodySpace, [12, 10], 'desktop expanded tool spacing remains unchanged');
    const copy = card('orphan-1').getByRole('button', { name: '复制完整命令', exact: true });
    if (await copy.count()) {
      await copy.click();
      await until('raw command clipboard', async () => await page.evaluate(() => navigator.clipboard.readText()) === commands[0]);
    }
    await card('orphan-1').locator(':scope > summary').click();
    const oldCard = await card('orphan-1').elementHandle();
    const liveId = prefix + 'live', runId = prefix + 'run';
    const emit = event => fixture.emit(event, 'dark');
    emit({ type: 'agent_start', runId });
    emit({ type: 'tool_execution_start', runId, toolCallId: liveId, toolName: 'bash', args: { command: 'fixture-only live command' }, webTiming: { elapsedMs: 1000, running: true } });
    await card('live').waitFor();
    const durationMs = text => text.endsWith('ms') ? parseFloat(text) : text.endsWith('s') ? parseFloat(text) * 1000 : NaN;
    await until('live node duration grows past 1000ms', async () => durationMs((await badge('live').textContent()).trim()) > 1000, 10000);
    report.liveGrowingBadge = (await badge('live').textContent()).trim();
    assert.match(await badge('live').getAttribute('title'), /节点执行耗时/);
    emit({ type: 'tool_execution_end', runId, toolCallId: liveId, toolName: 'bash', webTiming: terminal(2600), result: { content: [{ type: 'text', text: 'Fixture live output; nothing executed.' }] } });
    await until('terminal node duration', async () => (await badge('live').textContent()).trim() === '2.60s');
    await sleep(2200); await timingIs('live', '2.60s', true);
    // TranscriptOrder promises a stable outer display slot. Promotion changes
    // the renderer from projection to history, so inner disclosure nodes may
    // remount; asserting their identity would impose an unrelated new contract.
    const liveRow = await card('live').evaluateHandle(el => el.closest('[data-message-row]'));
    // This projection has no node event timing. Its historical call (without
    // a result row) is the sole source of node duration; the matching-block
    // renderer must not discard it in favour of client receive timing.
    emit({ type: 'tool_execution_start', runId, toolCallId: prefix + 'call-only', toolName: 'bash', args: { command: 'fixture-only call-only command' } });
    await card('call-only').waitFor();
    emit({ type: 'tool_execution_end', runId, toolCallId: prefix + 'call-only', toolName: 'bash', result: { content: [{ type: 'text', text: 'Fixture call-only projection' }] } });
    await until('call-only completed', async () => await card('call-only').locator('.call-status').getAttribute('aria-label') === '已完成');
    const canonical = [...messages, call('live', terminal(9999)), result('live', terminal(2600)), call('call-only', terminal(4321))];
    fixture.setMessages(canonical);
    const refreshed = page.waitForResponse(response => new URL(response.url()).pathname.endsWith('/sessions/dark/snapshot') || (new URL(response.url()).pathname.endsWith('/sessions/dark/commands') && response.request().postDataJSON()?.type === 'get_messages'));
    emit({ type: 'agent_end', runId }); emit({ type: 'agent_settled', runId, outcome: 'completed' });
    await (await refreshed).finished();
    await until('canonical live call row', async () => await page.locator(`[data-history-id="${prefix}live-call"]`).count() === 1);
    await timingIs('live', '2.60s', true);
    await timingIs('call-only', '4.32s', true);
    assert(await oldCard.evaluate(el => el.isConnected), 'unrelated existing orphan DOM remains connected');
    assert(await card('orphan-1').evaluate((el, original) => el === original, oldCard), 'unrelated history card retains DOM identity');
    assert(await liveRow.evaluate(el => el.isConnected), 'live display slot stays connected during history projection');
    assert(await card('live').evaluate((el, original) => el.closest('[data-message-row]') === original, liveRow), 'canonical history retains the same live display slot without duplication');
    await sleep(1500); await timingIs('live', '2.60s', true);
    assert.equal(await page.locator('.composer textarea').inputValue(), draft);
    pass('live node baseline ticks, end freezes 2.60s, settlement keeps stable DOM and one card');
    await session('mobile'); await card('orphan-1').waitFor();
    await timingIs('orphan-1', '—'); await timingIs('live', '—');
    await sleep(1200); await timingIs('live', '—');
    pass('mobile scope with matching IDs and no metadata does not borrow dark timing');
    await session('dark'); await card('live').waitFor();
    await timingIs('orphan-1', '1.23s', true); await timingIs('orphan-2', '5.68s', true); await timingIs('live', '2.60s', true);
    await sleep(2200); await timingIs('live', '2.60s', true);
    pass('session reopen restores persisted metadata without ticking or duplicates');
    await shot('persisted');
    assert.deepEqual(report.errors, [], 'no browser runtime errors');
    assert.deepEqual(report.blockedRequests, [], 'no external model/network requests');
    pass('fresh browser, restricted network, no real credentials or model sends');
  } catch (error) {
    report.fatal = error.stack || String(error);
    if (page) await shot('failure').catch(() => {});
    throw error;
  } finally {
    report.requests = fixture?.requests;
    report.forbiddenRequests = fixture?.requests.filter(r => (/\/commands$/.test(r.path) && !String(r.body.type).startsWith('get_')) || (!['GET', 'HEAD', 'OPTIONS'].includes(r.method) && !/\/(commands|open)$/.test(r.path))) || [];
    fs.writeFileSync('artifacts/tool-spacing-timing-validation.json', JSON.stringify(report, null, 2));
    try { assert.deepEqual(report.forbiddenRequests, [], 'no prompt/tool/model/credential/mutation writes'); }
    finally { try { if (browser) await browser.close(); } finally { if (fixture) await fixture.close(); } }
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
