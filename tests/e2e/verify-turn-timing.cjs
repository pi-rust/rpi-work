// Formal regression: real WASM preview + isolated HTTP/SSE fixture, never a model.
// Run ONLY after the timing WASM build is ready:
//   node tests/e2e/verify-turn-timing.cjs
// WORK_UI_URL defaults to the actual preview on :14180; CHROME_PATH is optional.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');

(async () => {
  fs.mkdirSync('artifacts', { recursive: true });
  const results = [], samples = [], geometry = [], errors = [], screenshots = [];
  let fixture, browser, page, fatal;
  const draft = 'Timing regression draft — must never be submitted';
  const historyId = 'timing-history-only'; // Deliberately NOT read-1/shell-1 from seeded SSE.
  const historyName = 'timing_history_unknown';
  const messages = [{ entryId: 'timing-user', role: 'user', content: 'Fixture-only timing history' }];
  const historyPair = (id, name, output) => [
    { entryId: `assistant-${id}`, role: 'assistant', timestamp: 1000,
      content: [{ type: 'toolCall', id, name, arguments: { fixture: true } }] },
    { entryId: `result-${id}`, role: 'toolResult', toolCallId: id, toolName: name,
      timestamp: 9000000000000, content: [{ type: 'text', text: output }] },
  ];
  messages.push(...historyPair(historyId, historyName, 'History output with deliberately fake timestamps'));
  const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
  // Poll rendered state, not elapsed wall time alone; hidden details use textContent below.
  const until = async (label, predicate, timeout = 12000) => {
    const deadline = Date.now() + timeout;
    while (Date.now() < deadline) {
      if (await predicate()) return;
      await sleep(100);
    }
    assert.fail(`Timed out waiting for ${label}`);
  };
  const text = async locator => (await locator.textContent()).trim();
  const milliseconds = value => {
    if (value === '<1ms') return 0.5;
    const match = /^(\d+(?:\.\d+)?)(ms|s|m)$/.exec(value);
    assert(match, `Expected a measured numeric duration, got ${JSON.stringify(value)}`);
    return Number(match[1]) * ({ ms: 1, s: 1000, m: 60000 })[match[2]];
  };
  const run = id => page.locator(`.turn-timing-run[data-run-id="${id}"]`);
  const duration = id => run(id).locator('.turn-timing-summary .turn-timing-duration');
  const span = (id, tool) => run(id).locator(`.turn-timing-span[data-span-id="tool:${tool}"]`);
  const spanDuration = (id, tool) => span(id, tool).locator('.turn-timing-span-duration');
  const card = name => page.locator('.inline-call').filter({
    has: page.locator('.call-name > strong').filter({ hasText: new RegExp(`^${name}$`) }),
  });
  const badge = name => card(name).locator('summary .tool-elapsed');
  const positive = async locator => {
    await until('rendered positive elapsed', async () => {
      const value = await text(locator);
      return !value.includes('—') && milliseconds(value) > 0;
    });
    return milliseconds(await text(locator));
  };
  const grows = async (locator, label) => {
    const before = await positive(locator);
    await until(`${label} grows across a local tick`, async () => {
      const value = await text(locator);
      return !value.includes('—') && milliseconds(value) > before + 400;
    });
    const after = milliseconds(await text(locator));
    samples.push({ label, before, after });
    return after;
  };
  const frozen = async (locators, label) => {
    // Capture only AFTER the terminal event rendered and the bridge returned a duration.
    for (const locator of locators) await positive(locator);
    const before = await Promise.all(locators.map(text));
    await sleep(2300); // More than two component ticks.
    assert.deepEqual(await Promise.all(locators.map(text)), before, `${label} must freeze`);
    samples.push({ label, frozen: before });
  };
  const status = async (id, expected) => until(`${id}: ${expected}`, async () =>
    await run(id).getAttribute('data-status') === expected);
  const emit = event => fixture.emit(event, 'dark');
  const start = async id => {
    emit({ type: 'agent_start', runId: id });
    await run(id).waitFor({ state: 'attached' });
    await status(id, 'running');
    assert.equal(await run(id).getAttribute('open'), null, 'new runs start collapsed');
  };
  const toolStart = async (id, tool, name) => {
    if (await run(id).getAttribute('open') === null) await run(id).locator('summary').click();
    emit({ type: 'tool_execution_start', runId: id, toolCallId: tool, toolName: name,
      args: { fixture: true }, timestamp: -9000000000000 });
    await card(name).waitFor();
    await span(id, tool).waitFor({ state: 'attached' });
  };
  const toolEnd = async (id, tool, name, extra = {}) => {
    emit({ type: 'tool_execution_end', runId: id, toolCallId: tool, toolName: name,
      result: { content: [{ type: 'text', text: `Live output ${name}` }] }, ...extra });
    await until(`${tool} terminal render`, async () =>
      /已完成|失败/.test(await text(span(id, tool).locator('.turn-timing-span-status'))));
  };
  const settle = async (id, outcome = 'completed') => {
    emit({ type: 'agent_settled', runId: id, outcome });
    await status(id, outcome);
  };
  const shot = async name => {
    const path = `artifacts/turn-timing-${name}.png`;
    await page.screenshot({ path, animations: 'disabled' });
    screenshots.push(path);
  };
  const test = async (name, body) => {
    console.log(`TEST ${results.length + 1}: ${name}`);
    await body();
    results.push(name);
    console.log(`PASS ${results.length}: ${name}`);
  };
  const selectSession = async id => {
    const before = fixture.requests.filter(r => r.path === `/api/v1/sessions/${id}/snapshot`).length;
    await page.locator(`[data-session-id="${id}"] .session-item`).click();
    await until(`${id} authoritative history snapshot`, async () =>
      fixture.requests.filter(r => r.path === `/api/v1/sessions/${id}/snapshot`).length > before);
    const inspector = page.locator('.inspector');
    if (!await inspector.isVisible()) {
      await page.getByRole('button', { name: '查看任务上下文', exact: true }).click();
    }
    await inspector.waitFor({ state: 'visible' });
    const timing = inspector.getByRole('button', { name: '回合耗时', exact: true });
    if (await timing.getAttribute('aria-expanded') !== 'true') await timing.click();
    await inspector.locator('.turn-timeline').waitFor();
  };
  try {
    fixture = await startFixture(0);
    fixture.setMessages(messages);
    browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
    page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
    page.setDefaultTimeout(10000);
    page.on('pageerror', error => errors.push(error.message));
    await page.route('https://api.github.com/**', route => route.fulfill({ status: 404, body: '{}' }));
    await page.goto(process.env.WORK_UI_URL || 'http://127.0.0.1:14180/');
    await page.getByRole('button', { name: '打开设置', exact: true }).click();
    await page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '执行节点', exact: true }).click();
    await page.getByRole('button', { name: '连接其他节点', exact: true }).click();
    await page.getByLabel('节点地址', { exact: true }).fill(fixture.url);
    await page.getByRole('button', { name: '连接节点', exact: true }).click();
    await page.locator('.server-details [role=status]').filter({ hasText: '在线' }).waitFor();
    await page.getByRole('button', { name: '返回会话', exact: true }).click();
    await page.locator('.project-row').filter({ hasText: 'rpi-work' }).click();
    await selectSession('dark');
    await page.locator('.composer textarea').fill(draft);

    await test('history is unknown, never reconstructed from seeded SSE or fake timestamps', async () => {
      const timeline = page.locator('section.turn-timeline').and(
        page.getByRole('region', { name: /^回合(?:时间线|耗时记录)$/ }),
      );
      await timeline.waitFor();
      const history = timeline.locator('.turn-timing-run[data-status="history"]');
      assert.equal(await history.count(), 1);
      assert.match(await text(history.locator('summary.turn-timing-summary')), /历史记录.*未记录/);
      assert.equal(await history.locator('.turn-timing-span').count(), 0, 'collapsed history does not mount hidden step nodes');
      assert.equal(await text(history.locator('.turn-timing-summary .turn-timing-duration')), '— 未记录');
      assert.equal(await text(badge(historyName)), '—');
      assert.equal(await badge(historyName).getAttribute('role'), 'img');
      for (const attr of ['title', 'aria-label']) assert.match(await badge(historyName).getAttribute(attr), /客户端接收计时/);
      await sleep(1200);
      assert.equal(await text(badge(historyName)), '—');
      await shot('history');
    });

    await test('parallel tools tick locally without disturbing the draft or focus', async () => {
      await start('timing-parallel');
      await toolStart('timing-parallel', 'timing-a', 'timing_parallel_a');
      await toolStart('timing-parallel', 'timing-b', 'timing_parallel_b');
      const input = page.locator('.composer textarea');
      await input.focus();
      const original = await input.elementHandle();
      await grows(badge('timing_parallel_a'), 'parallel A');
      await grows(badge('timing_parallel_b'), 'parallel B overlap');
      assert(await original.evaluate(e => e.isConnected && document.activeElement === e));
      assert.equal(await input.inputValue(), draft);
      assert.match(await text(run('timing-parallel').locator('.turn-timing-summary')), /回合 \d+\s*·\s*进行中/);
      for (const tool of ['timing-a', 'timing-b']) {
        const row = span('timing-parallel', tool);
        assert.match(await text(row.locator('.turn-timing-offset')), /^\+/);
        assert.match(await text(row.locator('.turn-timing-span-name')), /timing_parallel_/);
        assert.match(await text(row.locator('.turn-timing-span-status')), /进行中/);
      }
    });

    await test('B ends before A; its card and span freeze while A continues growing', async () => {
      await toolEnd('timing-parallel', 'timing-b', 'timing_parallel_b');
      await frozen([badge('timing_parallel_b'), spanDuration('timing-parallel', 'timing-b')], 'B ended');
      await grows(badge('timing_parallel_a'), 'A still running');
      await toolEnd('timing-parallel', 'timing-a', 'timing_parallel_a');
      await positive(badge('timing_parallel_a'));
      await positive(spanDuration('timing-parallel', 'timing-a'));
      // Settle immediately in the next case: a deliberate idle gap here would
      // legitimately make whole-run time exceed the sum of the tool times.
    });

    await test('settlement freezes the whole run; elapsed is not a fake sum of overlapping tools', async () => {
      await settle('timing-parallel');
      await frozen([duration('timing-parallel'), badge('timing_parallel_a'), badge('timing_parallel_b')], 'completed parallel run');
      const total = milliseconds(await text(duration('timing-parallel')));
      const a = milliseconds(await text(spanDuration('timing-parallel', 'timing-a')));
      const b = milliseconds(await text(spanDuration('timing-parallel', 'timing-b')));
      assert(total >= Math.max(a, b) - 100, 'run covers each individual tool');
      assert(total < a + b - 300, `overlap must not be double-counted: ${JSON.stringify({ total, a, b })}`);
      samples.push({ label: 'overlap is not a sum', total, a, b });
    });

    await test('agent_end remains pending and growing until actual agent_settled', async () => {
      await start('timing-pending');
      await positive(duration('timing-pending'));
      emit({ type: 'agent_end', runId: 'timing-pending' });
      await status('timing-pending', 'awaiting_settlement');
      assert.match(await text(run('timing-pending').locator('summary')), /等待结算/);
      await grows(duration('timing-pending'), 'pending settlement');
      await settle('timing-pending');
      await frozen([duration('timing-pending')], 'settlement, not agent_end');
    });

    await test('failed tools freeze; tool ends without starts and isolated settlements stay unknown', async () => {
      await start('timing-failed');
      await toolStart('timing-failed', 'timing-fail-tool', 'timing_failed_tool');
      await positive(badge('timing_failed_tool'));
      await toolEnd('timing-failed', 'timing-fail-tool', 'timing_failed_tool', { isError: true, error: 'Fixture-only failure' });
      assert.match(await text(span('timing-failed', 'timing-fail-tool').locator('.turn-timing-span-status')), /失败/);
      await toolEnd('timing-failed', 'timing-no-start', 'timing_missing_start', { isError: true });
      assert.equal(await text(spanDuration('timing-failed', 'timing-no-start')), '— 未记录');
      await settle('timing-failed', 'failed');
      await frozen([duration('timing-failed'), badge('timing_failed_tool')], 'failed run');
      // A terminal boundary alone has no start; do not derive one from another run.
      emit({ type: 'agent_settled', runId: 'timing-orphan-settlement', outcome: 'failed' });
      await status('timing-orphan-settlement', 'failed');
      assert.equal(await text(duration('timing-orphan-settlement')), '— 未记录');
    });

    await test('interruption freezes the run without fabricating a missing tool end time', async () => {
      await start('timing-interrupted');
      await toolStart('timing-interrupted', 'timing-unfinished', 'timing_interrupted_tool');
      await grows(badge('timing_interrupted_tool'), 'tool before interruption');
      await settle('timing-interrupted', 'interrupted');
      await frozen([duration('timing-interrupted')], 'interrupted settlement');
      assert.equal(await text(spanDuration('timing-interrupted', 'timing-unfinished')), '— 未记录');
      assert.equal(await text(badge('timing_interrupted_tool')), '—');
      assert.match(await text(span('timing-interrupted', 'timing-unfinished').locator('.turn-timing-span-status')), /已停止/);
    });

    await test('multiple rounds keep separate run IDs and old tools cannot restart a frozen run', async () => {
      const old = await text(duration('timing-parallel'));
      await start('timing-next-round');
      await toolStart('timing-next-round', 'timing-next', 'timing_next_round_tool');
      await grows(badge('timing_next_round_tool'), 'next independent tool');
      emit({ type: 'tool_execution_end', runId: 'timing-parallel', toolCallId: 'timing-a', toolName: 'timing_parallel_a', result: 'Late duplicate' });
      await toolEnd('timing-next-round', 'timing-next', 'timing_next_round_tool');
      await settle('timing-next-round');
      assert.equal(await text(duration('timing-parallel')), old);
      assert.equal(await run('timing-parallel').count(), 1);
      assert.equal(await run('timing-next-round').count(), 1);
      assert.notEqual(await text(run('timing-next-round').locator('.turn-timing-title')), '回合 1');
      await frozen([duration('timing-next-round')], 'independent next round');
    });

    await test('session isolation and reconciled history preserve live timing by matching tool ID', async () => {
      const before = await text(badge('timing_parallel_a'));
      const beforeRun = await text(duration('timing-parallel'));
      messages.push(...historyPair('timing-a', 'timing_parallel_a', 'Reconciled history output for timing A'));
      fixture.setMessages(messages);
      await selectSession('mobile');
      await until('dark runs removed from mobile view', async () => await run('timing-parallel').count() === 0);
      assert.equal(await card('timing_parallel_a').count(), 0);
      emit({ type: 'agent_start', runId: 'timing-background-only' });
      emit({ type: 'agent_settled', runId: 'timing-background-only', outcome: 'completed' });
      assert.equal(await run('timing-background-only').count(), 0);
      await selectSession('dark');
      await until('reconciled card', async () => await card('timing_parallel_a').count() === 1);
      await card('timing_parallel_a').locator('summary').click();
      await card('timing_parallel_a').getByText('Reconciled history output for timing A', { exact: true }).waitFor();
      assert.equal(await text(badge('timing_parallel_a')), before);
      assert.equal(await text(duration('timing-parallel')), beforeRun);
      assert.equal(await text(badge(historyName)), '—');
      assert.equal(await page.locator('.composer textarea').inputValue(), draft);
      await shot('reconciled-history');
    });

    await test('native details keyboard toggles and light/dark 1280/390/320 stay accessible and bounded', async () => {
      await page.setViewportSize({ width: 1280, height: 900 });
      const details = run('timing-parallel'), summary = details.locator('summary.turn-timing-summary');
      if (await details.getAttribute('open') !== null) await summary.click();
      await summary.focus();
      await page.keyboard.press('Enter');
      await until('Enter opens native details', async () => await details.getAttribute('open') !== null);
      await page.keyboard.press('Space');
      await until('Space closes native details', async () => await details.getAttribute('open') === null);
      for (const mode of ['light', 'dark']) {
        await page.setViewportSize({ width: 1280, height: 900 });
        await page.getByRole('button', { name: '外观与主题', exact: true }).click();
        const appearance = page.getByRole('dialog', { name: '外观与主题', exact: true });
        await appearance.getByLabel('明暗模式', { exact: true }).selectOption(mode);
        await appearance.getByRole('button', { name: '保存', exact: true }).click();
        await appearance.waitFor({ state: 'hidden' });
        for (const width of [1280, 390, 320]) {
          await page.setViewportSize({ width, height: 844 });
          await summary.scrollIntoViewIfNeeded();
          if (await details.getAttribute('open') === null) await summary.click();
          const proof = await page.locator('.turn-timeline').evaluate(timeline => {
            const bounds = e => { const r = e.getBoundingClientRect(); return { left: r.left, right: r.right, height: r.height }; };
            return { viewport: innerWidth, scrollWidth: document.documentElement.scrollWidth,
              timeline: bounds(timeline), summaries: [...timeline.querySelectorAll('.turn-timing-summary')].map(bounds),
              spans: [...timeline.querySelectorAll('details[open] .turn-timing-span')].map(bounds) };
          });
          assert(proof.scrollWidth <= width + 1, JSON.stringify(proof));
          for (const rect of [proof.timeline, ...proof.summaries, ...proof.spans]) {
            assert(rect.left >= -1 && rect.right <= width + 1, JSON.stringify({ mode, width, rect }));
          }
          assert(proof.summaries.every(rect => rect.height >= 44), 'run summaries have >=44px touch targets');
          geometry.push({ mode, width, ...proof });
          await shot(`${mode}-${width}`);
          assert.equal(await page.locator('.composer textarea').inputValue(), draft);
        }
      }
    });

    await test('browser clock bridge failure yields unknown, never a fabricated elapsed', async () => {
      // Fault the real clock capability, not the DOM, WASM reducer, or component.
      await page.evaluate(() => {
        window.__timingOriginalNowDescriptor = Object.getOwnPropertyDescriptor(performance, 'now');
        // Invalid bridge samples fail timing closed without throwing inside
        // unrelated browser/scroll consumers of the shared clock capability.
        Object.defineProperty(performance, 'now', { configurable: true, value() { return Number.NaN; } });
      });
      try {
        await start('timing-clock-failure');
        await toolStart('timing-clock-failure', 'timing-clock-tool', 'timing_clock_failure_tool');
        await toolEnd('timing-clock-failure', 'timing-clock-tool', 'timing_clock_failure_tool');
        await settle('timing-clock-failure');
        await sleep(1200);
        assert.equal(await text(duration('timing-clock-failure')), '— 未记录');
        assert.equal(await text(badge('timing_clock_failure_tool')), '—');
        assert.equal(await text(spanDuration('timing-clock-failure', 'timing-clock-tool')), '— 未记录');
        await shot('clock-unknown');
      } finally {
        await page.evaluate(() => {
          const descriptor = window.__timingOriginalNowDescriptor;
          if (descriptor) Object.defineProperty(performance, 'now', descriptor);
          else delete performance.now;
          delete window.__timingOriginalNowDescriptor;
        });
      }
    });

    await test('no application JavaScript errors, model commands, implicit responses, or lost draft', async () => {
      const forbidden = fixture.requests.filter(r => r.path.endsWith('/commands') &&
        !/^get_/.test(r.body.type || ''));
      assert.deepEqual(forbidden, [], 'only read-only discovery commands are allowed');
      assert.deepEqual(errors, []);
      assert.equal(await page.locator('.composer textarea').inputValue(), draft);
    });
  } catch (error) {
    fatal = error.stack || error.message;
    if (page && !page.isClosed()) await shot('failure').catch(() => {});
    throw error;
  } finally {
    fs.writeFileSync('artifacts/turn-timing-validation.json', JSON.stringify({
      preview: process.env.WORK_UI_URL || 'http://127.0.0.1:14180/',
      results, samples, geometry, errors, screenshots, fatal: fatal || null,
      commands: fixture ? fixture.requests.filter(r => r.path.endsWith('/commands')).map(r => ({ path: r.path, type: r.body.type })) : [],
      passed: !fatal && results.length === 12,
    }, null, 2));
    try { if (browser) await browser.close(); }
    finally { if (fixture) await fixture.close(); }
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
