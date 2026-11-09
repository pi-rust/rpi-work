// Formal Web + isolated HTTP/WebSocket only. Never reads/writes a real conversation.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');
const WEB = process.env.WORK_UI_URL || 'http://127.0.0.1:14180/';
const OLD = '主题切换已经准备好，接下来运行组件测试。';
const FIRST = '全新一轮输出：不得出现在旧回复上方。';
const SECOND = '先流式、后完整的第二条回复。';
(async () => {
  fs.mkdirSync('artifacts', { recursive: true });
  const fixture = await startFixture(0, { pagedHistory: true });
  const older = Array.from({ length: 60 }, (_, i) => ({ entryId: `stream-old-${i}`, role: 'user', content: `既有历史问题 ${i}` }));
  fixture.setMessages(older);
  const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
  const page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
  page.setDefaultTimeout(12000);
  const errors = [], results = [];
  page.on('pageerror', e => errors.push(e.message));
  await page.route('**/*', async route => {
    const u = new URL(route.request().url());
    if (u.origin === new URL(fixture.url).origin) return route.continue();
    if (u.origin === new URL(WEB).origin) {
      if (u.pathname.startsWith('/api/v1/')) return route.fulfill({ response: await route.fetch({ url: new URL(u.pathname + u.search, fixture.url).href }) });
      return route.continue();
    }
    if (u.hostname === 'api.github.com') return route.fulfill({ status: 404, body: '{}' });
    return route.abort('blockedbyclient');
  });
  const pass = name => { results.push(name); console.log('PASS', name); };
  const history = id => page.locator(`[data-history-id="${id}"]`);
  const text = value => page.locator('.transcript').getByText(value, { exact: true });
  const before = async (a, b) => {
    await text(a).waitFor(); await text(b).waitFor();
    const first = await text(a).elementHandle(), second = await text(b).elementHandle();
    assert(await first.evaluate((el, next) => Boolean(el.compareDocumentPosition(next) & Node.DOCUMENT_POSITION_FOLLOWING), second), `${a} must precede ${b}`);
    // Sample both positions in the same frame: follow-scroll may advance between
    // two separate boundingBox RPCs even though the visual order never changes.
    assert(await first.evaluate((el, next) => el.getBoundingClientRect().y < next.getBoundingClientRect().y, second), 'DOM order must match visual order');
  };
  const end = message => fixture.emit({ type: 'message_end', message });
  const select = async id => {
    const entry = page.locator(`[data-session-id="${id}"] .session-item`);
    if (!await entry.isVisible()) await page.locator('.project-row').filter({ hasText: 'rpi-work' }).click();
    await entry.click();
  };
  try {
    await page.goto(WEB);
    await page.getByRole('button', { name: '打开设置', exact: true }).click();
    await page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '执行节点', exact: true }).click();
    await page.getByRole('button', { name: '连接其他节点', exact: true }).click();
    await page.getByLabel('节点地址', { exact: true }).fill(fixture.url);
    await page.getByRole('button', { name: '连接节点', exact: true }).click();
    await page.getByRole('status').filter({ hasText: '在线' }).first().waitFor();
    await page.getByRole('button', { name: '返回会话', exact: true }).click();
    await select('dark'); await text(OLD).waitFor();
    await page.waitForFunction(() => !document.querySelector('.composer textarea')?.disabled);
    await page.locator('.composer textarea').fill('顺序回归中的本地草稿');

    const question = { entryId: 'stream-question', role: 'user', content: '新一轮的问题不得挤到旧回复前面。' };
    end(question); fixture.emit({ type: 'agent_start', runId: 'stream-order' });
    const first = { entryId: 'stream-first', role: 'assistant', timestamp: 30001, content: [{ type: 'text', text: FIRST }] };
    fixture.emit({ type: 'message_start', message: { ...first, content: [] } });
    fixture.emit({ type: 'message_update', message: first, eventType: 'text_delta', delta: FIRST });
    await before(OLD, FIRST); await before(OLD, question.content); await before(question.content, FIRST);
    for (const name of ['read', 'bash']) {
      const tool = page.locator('.transcript .call-name > strong').getByText(name, { exact: true });
      const toolNode = await tool.elementHandle(), newNode = await text(FIRST).elementHandle();
      assert(await toolNode.evaluate((el, next) => Boolean(el.compareDocumentPosition(next) & Node.DOCUMENT_POSITION_FOLLOWING), newNode), 'old tools precede new text');
    }
    end(first);
    pass('new full-body stream stays below old projection-only text/tools and its user message');

    fixture.emit({ type: 'message_start', message: { entryId: 'stream-second', role: 'assistant', content: [] } });
    fixture.emit({ type: 'message_update', message: { entryId: 'stream-second', role: 'assistant' }, eventType: 'text_delta', delta: SECOND });
    await before(FIRST, SECOND);
    await text(SECOND).evaluate(el => { window.__streamRow = el.closest('[data-message-row]'); });
    const second = { entryId: 'stream-second', role: 'assistant', timestamp: 30002, content: [{ type: 'text', text: SECOND }] };
    end(second); await history('stream-second').waitFor();
    assert(await history('stream-second').evaluate(el => el === window.__streamRow), 'partial-to-history promotion preserves the same row DOM');
    await before(FIRST, SECOND); assert.equal(await text(SECOND).count(), 1);
    pass('partial-to-complete promotion keeps its stable DOM position without duplication');

    fixture.emit({ type: 'tool_execution_start', toolCallId: 'stream-call', toolName: 'stream_order_read', args: { path: 'fixture-only.txt' } });
    await page.locator('.call-name > strong').getByText('stream_order_read', { exact: true }).waitFor();
    const call = { entryId: 'stream-call-owner', role: 'assistant', content: [{ type: 'toolCall', id: 'stream-call', name: 'stream_order_read', arguments: { path: 'fixture-only.txt' } }] };
    end(call);
    fixture.emit({ type: 'tool_execution_end', toolCallId: 'stream-call', toolName: 'stream_order_read', result: { content: [{ type: 'text', text: 'stream-order-result' }] } });
    const result = { entryId: 'stream-call-result', role: 'toolResult', toolCallId: 'stream-call', toolName: 'stream_order_read', content: [{ type: 'text', text: 'stream-order-result' }] };
    end(result);
    await history('stream-call-owner').locator('.inline-call').evaluate(el => { el.open = true; });
    await history('stream-call-owner').locator('.call-output').filter({ hasText: 'stream-order-result' }).waitFor();
    assert.equal(await page.locator('.call-name > strong').getByText('stream_order_read', { exact: true }).count(), 1);
    await before(FIRST, SECOND);
    pass('standalone tool projection upgrades to its historical owner without duplicate result cards');

    // Canonical suffix now includes the previously event-only historical answer.
    // More than 48 messages exercises settlement with a bounded snapshot.
    fixture.setMessages([...older, { entryId: 'answer', role: 'assistant', content: OLD }, question, first, second, call, result]);
    fixture.emit({ type: 'agent_end', runId: 'stream-order' });
    fixture.emit({ type: 'agent_settled', runId: 'stream-order', outcome: 'completed' });
    await page.getByRole('button', { name: '发送消息', exact: true }).waitFor();
    await history('answer').waitFor();
    await before(OLD, question.content); await before(question.content, FIRST); await before(FIRST, SECOND);
    assert.equal(await text(OLD).count(), 1); assert.equal(await text(SECOND).count(), 1);
    pass('bounded settlement and old projection-to-history reconciliation preserve chronological positions');

    await page.locator('.transcript').hover(); await page.mouse.wheel(0, -100000);
    await history('stream-old-0').waitFor();
    await before('既有历史问题 0', OLD); await before(OLD, FIRST); await before(FIRST, SECOND);
    pass('older-page prepend leaves all previously displayed live rows in their relative order');

    fixture.emit({ type: 'agent_start', runId: 'stream-anonymous' });
    const anonymousText = '匿名流式回复取得正式 ID 后也不搬家。';
    fixture.emit({ type: 'message_start', message: { role: 'assistant', timestamp: 30003, content: [] } });
    fixture.emit({ type: 'message_update', message: { role: 'assistant', timestamp: 30003 }, eventType: 'text_delta', delta: anonymousText });
    await before(SECOND, anonymousText);
    await text(anonymousText).evaluate(el => { window.__anonymousStreamRow = el.closest('[data-message-row]'); });
    const canonical = { entryId: 'stream-canonical', role: 'assistant', timestamp: 30003, content: anonymousText };
    end(canonical);
    fixture.setMessages([...older, { entryId: 'answer', role: 'assistant', content: OLD }, question, first, second, call, result, canonical]);
    fixture.emit({ type: 'agent_settled', runId: 'stream-anonymous', outcome: 'completed' });
    await history('stream-canonical').waitFor();
    assert(await history('stream-canonical').evaluate(el => el === window.__anonymousStreamRow), 'canonical ID reconciliation keeps the live DOM slot');
    await before(SECOND, anonymousText); assert.equal(await text(anonymousText).count(), 1);
    fixture.emit({ type: 'agent_start', runId: 'stream-next' });
    end({ entryId: 'stream-next-question', role: 'user', content: '再开一轮，保持旧位置。' });
    fixture.emit({ type: 'message_start', message: { entryId: 'stream-next-reply', role: 'assistant', content: [] } });
    fixture.emit({ type: 'message_update', message: { entryId: 'stream-next-reply', role: 'assistant', content: '下一轮仍然追加在末尾。' }, eventType: 'text_delta', delta: '下一轮仍然追加在末尾。' });
    await before(anonymousText, '再开一轮，保持旧位置。'); await before('再开一轮，保持旧位置。', '下一轮仍然追加在末尾。');
    end({ entryId: 'stream-next-reply', role: 'assistant', content: '下一轮仍然追加在末尾。' });
    fixture.emit({ type: 'agent_settled', runId: 'stream-next', outcome: 'completed' });
    await page.getByRole('button', { name: '发送消息', exact: true }).waitFor();
    pass('anonymous-to-canonical ID promotion and a subsequent round retain their original chronological slots');

    await select('architecture'); await text('这是 梳理 Agent 架构 的独立历史。').waitFor();
    assert.equal(await text(FIRST).count(), 0);
    await select('dark'); await text(FIRST).waitFor();
    await before(OLD, FIRST); await before(FIRST, SECOND);
    const restored = page.waitForResponse(r => r.request().method() === 'GET' && new URL(r.url()).pathname.endsWith('/sessions/dark/snapshot'));
    const subscribed = page.waitForEvent('websocket', socket => socket.url().includes('/api/v1/realtime'));
    fixture.disconnect('dark');
    await (await restored).finished(); await subscribed;
    await page.locator('.connection-badge.online').waitFor();
    await before(OLD, FIRST); await before(FIRST, SECOND); assert.equal(await text(SECOND).count(), 1);
    assert.equal(await page.locator('.composer textarea').inputValue(), '顺序回归中的本地草稿');
    assert.deepEqual(errors, []);
    assert.equal(fixture.requests.filter(r => ['prompt', 'queue_message', 'abort', 'compact'].includes(r.body.type)).length, 0);
    pass('session switching/reconnect isolate order state, retain drafts and make zero send/model requests');
    await page.screenshot({ path: 'artifacts/message-stream-order.png', animations: 'disabled' });

    // Keep replay-overlap cases in an unopened session, after all dark-session
    // assertions. emit builds real multi-seq snapshot.events; setMessages then
    // replaces its message_end side effects with authoritative persisted history.
    const emitMobile = event => fixture.emit(event, 'mobile');
    const overlapText = '快照正式历史与匿名完成事件只能显示一份。';
    const distinctText = '不同明确 ID 即使正文与时间相同也必须都保留。';
    const pendingText = '快照中的未完成流仍须保留。';
    const userText = '无 ID 用户消息：相同正文的两个回合。';
    const overlapReply = {
      role: 'assistant', timestamp: 40001,
      content: [{ type: 'text', text: overlapText }],
      model: 'fixture-only-model', stopReason: 'stop', usage: { input: 3, output: 4 },
    };
    const canonicalOverlap = {
      ...overlapReply, entryId: 'snapshot-overlap-reply',
      webTiming: { elapsedMs: 1200, running: false },
    };
    const explicitA = { entryId: 'snapshot-explicit-a', role: 'assistant', timestamp: 40002, content: [{ type: 'text', text: distinctText }] };
    const explicitB = { ...explicitA, entryId: 'snapshot-explicit-b' };
    const anonymousUser = { role: 'user', content: userText };
    const userSeqs = [];
    const canonicalUsers = [0, 1].map(round => ({ ...anonymousUser, entryId: `snapshot-user-${round}` }));
    emitMobile({ type: 'agent_start', runId: 'snapshot-overlap' });
    emitMobile({ type: 'message_start', message: { ...overlapReply, content: [] } });
    emitMobile({ type: 'message_update', message: overlapReply, eventType: 'text_delta', delta: overlapText });
    emitMobile({ type: 'message_end', message: overlapReply });
    for (const reply of [explicitA, explicitB]) {
      emitMobile({ type: 'message_start', message: { ...reply, content: [] } });
      emitMobile({ type: 'message_update', message: reply, eventType: 'text_delta', delta: distinctText });
      emitMobile({ type: 'message_end', message: reply });
    }
    emitMobile({ type: 'agent_settled', runId: 'snapshot-overlap', outcome: 'completed' });
    emitMobile({ type: 'agent_start', runId: 'snapshot-unfinished' });
    const pendingReply = { role: 'assistant', timestamp: 40003, content: [{ type: 'text', text: pendingText }] };
    emitMobile({ type: 'message_start', message: { ...pendingReply, content: [] } });
    emitMobile({ type: 'message_update', message: pendingReply, eventType: 'text_delta', delta: pendingText });
    // Explicit B exists only in events; the unfinished stream has no
    // message_end. Both must survive alongside the persisted canonical rows.

    const assertSnapshotReplay = async phase => {
      await history('snapshot-overlap-reply').waitFor();
      await history('snapshot-explicit-a').waitFor();
      // B is intentionally event-only: it owns a projection row, not a
      // data-history-id. Both replies must nevertheless remain visible.
      await text(distinctText).nth(1).waitFor();
      await text(pendingText).waitFor();
      assert.equal(await text(overlapText).count(), 1, `${phase}: completed anonymous replay is covered by canonical history (including history-only webTiming)`);
      assert.equal(await history('snapshot-overlap-reply').getByText(overlapText, { exact: true }).count(), 1, `${phase}: canonical reply remains`);
      assert.equal(await text(pendingText).count(), 1, `${phase}: unfinished stream is not discarded`);
      assert.equal(await text(distinctText).count(), 2, `${phase}: different explicit IDs cannot be merged by body/timestamp`);
      assert.equal(await text(userText).count(), canonicalUsers.length, `${phase}: identical user rounds remain separate`);
      pass(`snapshot overlap ${phase}: canonical once, unfinished retained, distinct IDs and anonymous user rounds preserved`);
    };
    // Persisted users are present in the initial snapshot. User event identity
    // is exercised live below; projection restores assistant content, not users.
    fixture.setMessages([canonicalOverlap, explicitA, ...canonicalUsers], 'mobile');
    await select('mobile');
    await assertSnapshotReplay('initial open');
    const liveUserText = '实时匿名用户两轮同文不会按事件重复。';
    for (let round = 0; round < 2; round++) {
      for (const type of ['message_start', 'message_update', 'message_end']) {
        const message = { role: 'user', content: liveUserText };
        if (type === 'message_end') message.entryId = `live-anonymous-user-${round}`;
        userSeqs.push(emitMobile({ type, message }));
      }
      await text(liveUserText).nth(round).waitFor();
      assert.equal(await text(liveUserText).count(), round + 1, 'one live user row per message segment');
    }
    assert.equal(new Set(userSeqs).size, 6, 'anonymous user starts/updates and promoted ends span six distinct seqs');
    pass('live anonymous user segments promote without duplication and preserve identical rounds');
    await select('architecture'); await text('这是 梳理 Agent 架构 的独立历史。').waitFor();
    assert.equal(await text(overlapText).count(), 0);
    await select('mobile');
    await assertSnapshotReplay('switch back');
    assert.equal(await text(liveUserText).count(), 2, 'canonical user echoes remain unique on switch back');
    const mobileRestored = page.waitForResponse(r => r.request().method() === 'GET' && new URL(r.url()).pathname.endsWith('/sessions/mobile/snapshot'));
    const mobileSubscribed = page.waitForEvent('websocket', socket => socket.url().includes('/api/v1/realtime'));
    fixture.disconnect('mobile');
    await (await mobileRestored).finished(); await mobileSubscribed;
    await page.locator('.connection-badge.online').waitFor();
    await assertSnapshotReplay('reconnect');
    assert.equal(await text(liveUserText).count(), 2, 'canonical user echoes remain unique on reconnect');

    // The first host ID promotes an open segment even if the host revises its
    // timestamp on persistence. Assert before settlement as well as on restore.
    const driftPartial = 'ID 提升前的正文不能残留为第二条消息。';
    const driftText = '同一消息的时间戳变化后仍然只有一条。';
    const drift = { entryId: 'timestamp-drift-formal', role: 'assistant', timestamp: 51001, content: driftText };
    emitMobile({ type: 'agent_start', runId: 'timestamp-drift' });
    emitMobile({ type: 'message_start', message: { role: 'assistant', timestamp: 51000, content: driftPartial } });
    await text(driftPartial).waitFor();
    await text(driftPartial).evaluate(element => { window.__driftRow = element.closest('[data-message-row]'); });
    emitMobile({ type: 'message_end', message: drift });
    await text(driftText).waitFor();
    assert.equal(await text(driftPartial).count(), 0, 'timestamp changes cannot split the lifecycle into two messages');
    assert.equal(await text(driftText).count(), 1);
    assert(await text(driftText).evaluate(element => element.closest('[data-message-row]') === window.__driftRow), 'ID promotion keeps the existing DOM slot');
    fixture.setMessages([drift], 'mobile');
    emitMobile({ type: 'agent_settled', runId: 'timestamp-drift', outcome: 'completed' });
    await history(drift.entryId).waitFor();
    pass('timestamp-revised first canonical ID promotes one lifecycle without a second message before or after settlement');

    await select('architecture');
    const restoredText = '已存在的正式历史接管匿名流后不得保留重复投影。';
    const restoredOwner = { entryId: 'restored-canonical-owner', role: 'assistant', timestamp: 52000, content: restoredText };
    emitMobile({ type: 'agent_start', runId: 'restore-promotion' });
    emitMobile({ type: 'message_start', message: { role: 'assistant', timestamp: 52000, content: restoredText } });
    fixture.setMessages([drift, restoredOwner], 'mobile');
    await select('mobile');
    await history(restoredOwner.entryId).waitFor();
    await text(restoredText).nth(1).waitFor(); // No canonical evidence yet: do not guess by body.
    emitMobile({ type: 'message_update', message: { role: 'assistant', timestamp: 52000, content: restoredText }, eventType: 'text_delta', delta: restoredText });
    emitMobile({ type: 'message_update', message: restoredOwner, eventType: 'text_delta', delta: '' });
    emitMobile({ type: 'message_update', message: {}, eventType: 'text_delta', delta: '正式身份确认后的增量不得另起匿名行' });
    emitMobile({ type: 'message_end', message: restoredOwner });
    await page.waitForFunction(value => [...document.querySelectorAll('.transcript .conversation-markdown')].filter(element => element.textContent === value).length === 1, restoredText);
    assert.equal(await text(restoredText).count(), 1, 'formal end coalesces both the live row and anonymous projection into its existing history owner');
    emitMobile({ type: 'agent_settled', runId: 'restore-promotion', outcome: 'completed' });
    const restoredSnapshot = page.waitForResponse(response => response.request().method() === 'GET' && new URL(response.url()).pathname.endsWith('/sessions/mobile/snapshot'));
    fixture.disconnect('mobile');
    await (await restoredSnapshot).finished();
    await page.locator('.connection-badge.online').waitFor();
    await history(restoredOwner.entryId).waitFor();
    assert.equal(await text(restoredText).count(), 1);
    assert.equal(await text(driftText).count(), 1);
    assert.equal(await text(driftPartial).count(), 0, 'snapshot replay covers the entire timestamp-revised segment');
    pass('restored canonical history and an unfinished anonymous stream converge on formal ID and stay single on reconnect');

    await select('architecture');
    const unfinishedText = '快照内已取得正式身份但尚未结束的回复只能显示一次。';
    const unfinished = { entryId: 'snapshot-promoted-unfinished', role: 'assistant', content: unfinishedText };
    const unfinishedSuffix = ' 无角色字段增量';
    const unfinishedLatest = unfinishedText + unfinishedSuffix;
    emitMobile({ type: 'agent_start', runId: 'snapshot-promoted-unfinished' });
    emitMobile({ type: 'message_start', message: { role: 'assistant', content: [] } });
    emitMobile({ type: 'message_update', message: unfinished, eventType: 'text_delta', delta: unfinishedText });
    fixture.setMessages([drift, restoredOwner, { ...unfinished, content: '快照读取时的旧正文必须被更新。' }], 'mobile');
    await select('mobile');
    await history(unfinished.entryId).waitFor();
    await text(unfinishedText).waitFor();
    assert.equal(await text(unfinishedText).count(), 1, 'snapshot promotion must retain the newest body before message_end');
    await history(unfinished.entryId).evaluate(row => { window.__unfinishedOwner = row; });
    emitMobile({ type: 'message_update', message: {}, eventType: 'text_delta', delta: unfinishedSuffix });
    await text(unfinishedLatest).waitFor();
    assert(await history(unfinished.entryId).evaluate(row => row === window.__unfinishedOwner), 'a roleless delta updates the existing canonical DOM owner');
    const unfinishedRestored = page.waitForResponse(response => response.request().method() === 'GET' && new URL(response.url()).pathname.endsWith('/sessions/mobile/snapshot'));
    fixture.disconnect('mobile');
    await (await unfinishedRestored).finished();
    await page.locator('.connection-badge.online').waitFor();
    await history(unfinished.entryId).waitFor();
    await text(unfinishedLatest).waitFor();
    assert.equal(await text(unfinishedLatest).count(), 1, 'roleless delta replay must retain the newest visible text without an anonymous row');
    assert.equal(await page.locator('.projection-message-row').filter({ hasText: '无角色字段增量' }).count(), 0);
    emitMobile({ type: 'message_end', message: { ...unfinished, content: unfinishedLatest } });
    await history(unfinished.entryId).waitFor();
    assert.equal(await text(unfinishedLatest).count(), 1);
    pass('unfinished snapshot promotion and roleless deltas retain one canonical owner before end and across reconnect');

    await select('architecture');
    const pagedPersisted = '分页中同一正式消息的旧正文不得再次出现。';
    const pagedLive = '分页中应保留已证明身份的最新流式正文。';
    const pagedCanonical = { entryId: 'paged-canonical-owner', role: 'assistant', content: pagedPersisted };
    const gap = Array.from({ length: 60 }, (_, i) => ({ entryId: `alias-gap-${i}`, role: 'user', content: `分页别名间隔 ${i}` }));
    fixture.setMessages([pagedCanonical, ...gap], 'mobile');
    emitMobile({ type: 'agent_start', runId: 'paged-alias' });
    emitMobile({ type: 'message_start', message: { role: 'assistant', content: [] } });
    await select('mobile');
    await history('alias-gap-59').waitFor();
    emitMobile({ type: 'message_update', message: { ...pagedCanonical, content: pagedLive }, eventType: 'text_delta', delta: pagedLive });
    await text(pagedLive).waitFor();
    await page.locator('.transcript').hover();
    await page.mouse.wheel(0, -100000);
    await history('alias-gap-0').waitFor();
    assert.equal(await text(pagedPersisted).count(), 0, 'older canonical page cannot add a second row beside its proven live alias');
    assert.equal(await text(pagedLive).count(), 1, 'live content wins without renaming the projection DOM owner');
    pass('older-page overlap recognizes proven canonical aliases and preserves the newest live content');
    assert.deepEqual(errors, []);
    assert.equal(fixture.requests.filter(r => ['prompt', 'queue_message', 'abort', 'compact'].includes(r.body.type)).length, 0);
  } catch (error) {
    results.push({ failure: error.message });
    fs.writeFileSync('artifacts/message-stream-order-failure-dom.json', JSON.stringify(await page.locator('.transcript [data-message-row]').evaluateAll(rows => rows.map(row => ({ id: row.dataset.historyId, key: row.dataset.messageRow, text: row.innerText, markdown: [...row.querySelectorAll('.conversation-markdown')].map(element => ({ text: element.textContent, html: element.innerHTML })) }))), null, 2));
    await page.screenshot({ path: 'artifacts/message-stream-order-failure.png', animations: 'disabled' }).catch(() => {});
    throw error;
  } finally {
    fs.writeFileSync('artifacts/message-stream-order-validation.json', JSON.stringify({ results, errors, scope: 'formal Web + isolated fixture, not native desktop acceptance' }, null, 2));
    await browser.close(); await fixture.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
