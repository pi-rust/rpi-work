// Formal WASM Web + isolated HTTP/SSE fixture only; never sends a real prompt.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');
const WEB = process.env.WORK_UI_URL || 'http://127.0.0.1:14180/';
const OLD = 'IDENTITY OLD: the previous assistant reply must never change.';
const FIRST = 'IDENTITY FIRST: a different turn with the same timestamp.';
const C1 = 'IDENTITY C1: first reply without message_start.';
const C2 = 'IDENTITY C2: a different explicit ID, not a continuation.';
const D = 'IDENTITY D: anonymous first delta after a user boundary.';
const user = (entryId, content) => ({ entryId, role: 'user', content });
const assistant = (entryId, text, timestamp) => ({
  entryId, role: 'assistant', ...(timestamp === undefined ? {} : { timestamp }),
  content: [{ type: 'text', text }],
});

(async () => {
  fs.mkdirSync('artifacts', { recursive: true });
  const results = [], errors = [];
  let browser, page, oldRow, oldText, oldHTML;
  const fixture = await startFixture(0, { pagedHistory: true });
  const aUser = user('identity-a-user', 'IDENTITY user A');
  const a = assistant('identity-a', OLD, 42);
  const bUser = user('identity-b-user', 'IDENTITY user B');
  const b = assistant('identity-b', FIRST, 42);
  const cUser = user('identity-c-user', 'IDENTITY user C');
  const c1 = assistant('identity-c1', C1);
  const c2 = assistant('identity-c2', C2);
  const dUser = user('identity-d-user', 'IDENTITY user D');
  const d = assistant('identity-d', D);
  fixture.setMessages([aUser, a]);
  const pass = name => { results.push(name); console.log('PASS', name); };
  try {
    browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
    page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
    page.setDefaultTimeout(12000);
    page.on('pageerror', error => errors.push(error.message));
    await page.route('**/*', async route => {
      const u = new URL(route.request().url());
      if (u.origin === new URL(fixture.url).origin) return route.continue();
      if (u.origin === new URL(WEB).origin) {
        if (u.pathname.startsWith('/api/v1/')) {
          return route.fulfill({ response: await route.fetch({ url: new URL(u.pathname + u.search, fixture.url).href }) });
        }
        return route.continue();
      }
      if (u.hostname === 'api.github.com') return route.fulfill({ status: 404, body: '{}' });
      return route.abort('blockedbyclient');
    });
    const history = id => page.locator(`[data-history-id="${id}"]`);
    // The fixture also seeds an unrelated answer/read/bash projection. Never use
    // assistant row counts or that seed answer's text to identify our messages.
    const text = value => page.locator('.transcript').getByText(value, { exact: true });
    const immutableA = async stage => {
      assert.equal(await history('identity-a').count(), 1, `${stage}: A retains its unique canonical ID`);
      assert(await oldRow.evaluate(el => el.isConnected), `${stage}: original A row remains connected`);
      assert(await history('identity-a').evaluate((el, original) => el === original, oldRow), `${stage}: A row DOM is untouched`);
      assert(await oldText.evaluate(el => el.isConnected), `${stage}: original A text element remains connected`);
      assert.equal(await oldText.textContent(), OLD, `${stage}: previous reply must not be overwritten`);
      assert.equal(await oldText.evaluate(el => el.innerHTML), oldHTML, `${stage}: A text DOM is unchanged`);
      assert.equal(await history('identity-a').getByText(OLD, { exact: true }).count(), 1, `${stage}: A still contains OLD`);
      assert.equal(await text(OLD).count(), 1, `${stage}: OLD is not duplicated`);
    };
    const before = async (first, second) => {
      await text(first).waitFor(); await text(second).waitFor();
      assert.equal(await text(first).count(), 1, `unique text: ${first}`);
      assert.equal(await text(second).count(), 1, `unique text: ${second}`);
      const left = await text(first).elementHandle(), right = await text(second).elementHandle();
      assert(await left.evaluate((el, next) => Boolean(el.compareDocumentPosition(next) & Node.DOCUMENT_POSITION_FOLLOWING), right), `${first} must precede ${second}`);
      // Follow-scroll can move between two RPC samples; compare in one frame.
      assert(await left.evaluate((el, next) => el.getBoundingClientRect().y < next.getBoundingClientRect().y, right), 'visual order must match DOM order');
    };
    const distinct = async (first, second) => {
      const left = await text(first).elementHandle(), right = await text(second).elementHandle();
      assert(await left.evaluate((el, next) => {
        const a = el.closest('[data-message-row]'), b = next.closest('[data-message-row]');
        return Boolean(a && b && a !== b);
      }, right), `${first} and ${second} must occupy distinct reply rows`);
    };
    const end = message => fixture.emit({ type: 'message_end', message });
    const delta = (entryId, value) => fixture.emit({
      type: 'message_update', message: { role: 'assistant', ...(entryId ? { entryId } : {}) },
      eventType: 'text_delta', delta: value,
    });
    const settle = async (messages, runId) => {
      // Emitted partial updates do not populate the canonical snapshot. Install
      // the entire expected transcript before EVERY settlement, including C1.
      fixture.setMessages(messages);
      const refreshed = page.waitForResponse(response => {
        const request = response.request(), pathname = new URL(response.url()).pathname;
        return (request.method() === 'GET' && pathname.endsWith('/sessions/dark/snapshot')) ||
          (request.method() === 'POST' && pathname.endsWith('/sessions/dark/commands') && request.postDataJSON()?.type === 'get_messages');
      });
      fixture.emit({ type: 'agent_end', runId });
      fixture.emit({ type: 'agent_settled', runId, outcome: 'completed' });
      await (await refreshed).finished();
      await page.getByRole('button', { name: '发送消息', exact: true }).waitFor();
      for (const message of messages) await history(message.entryId).waitFor();
      await immutableA(`${runId} settled`);
    };

    await page.goto(WEB);
    await page.getByRole('button', { name: '打开设置', exact: true }).click();
    await page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '工作协同', exact: true }).click();
    await page.getByRole('button', { name: '连接其他节点', exact: true }).click();
    await page.getByLabel('节点地址', { exact: true }).fill(fixture.url);
    await page.getByRole('button', { name: '连接节点', exact: true }).click();
    await page.getByRole('status').filter({ hasText: '在线' }).first().waitFor();
    await page.getByRole('button', { name: '返回会话', exact: true }).click();
    const session = page.locator('[data-session-id="dark"] .session-item');
    if (!await session.isVisible()) await page.locator('.project-row').filter({ hasText: 'rpi-work' }).click();
    await session.click();
    await history('identity-a').getByText(OLD, { exact: true }).waitFor();
    await page.waitForFunction(() => !document.querySelector('.composer textarea')?.disabled);
    oldRow = await history('identity-a').elementHandle();
    oldText = await history('identity-a').getByText(OLD, { exact: true }).elementHandle();
    oldHTML = await oldText.evaluate(el => el.innerHTML);
    await immutableA('initial history');

    end(bUser);
    fixture.emit({ type: 'agent_start', runId: 'identity-b-run' });
    fixture.emit({ type: 'message_start', message: { ...b, content: [] } });
    await text(bUser.content).waitFor();
    await immutableA('B empty message_start');
    fixture.emit({ type: 'message_update', message: b, eventType: 'text_delta', delta: FIRST });
    // Do not require contentless deltas after a full body: retained history
    // currently freezes those updates, a separate regression from identity.
    // Wait for arrival even on a broken bundle that renders FIRST twice;
    // immutableA below must diagnose the old-row overwrite, not strict mode.
    await text(FIRST).first().waitFor();
    await immutableA('B first full-body update with timestamp 42');
    await before(OLD, bUser.content); await before(bUser.content, FIRST);
    await distinct(OLD, FIRST);
    await page.screenshot({ path: 'artifacts/cross-turn-message-identity-streaming.png', animations: 'disabled' });
    pass('same-timestamp first full-body B never overwrites A or appears before user B; original A DOM is untouched');
    end(b);
    await history('identity-b').waitFor();
    await immutableA('B message_end');
    await settle([aUser, a, bUser, b], 'identity-b-run');
    await before(OLD, bUser.content); await before(bUser.content, FIRST);
    await distinct(OLD, FIRST);
    assert.equal(await history('identity-b').getByText(FIRST, { exact: true }).count(), 1);
    pass('canonical settlement preserves distinct identity-a/identity-b replies despite identical timestamps');

    end(cUser);
    fixture.emit({ type: 'agent_start', runId: 'identity-c-run' });
    delta('identity-c1', C1);
    await before(cUser.content, C1);
    await immutableA('C1 first partial without message_start');
    // Deliberately no start/end for C1 before the ID changes to C2.
    delta('identity-c2', C2);
    await before(FIRST, cUser.content); await before(cUser.content, C1); await before(C1, C2);
    await distinct(C1, C2);
    await immutableA('C2 explicit ID boundary without message_start');
    assert.equal(await text(C1 + C2).count(), 0, 'different explicit IDs must never concatenate');
    end(c2);
    await history('identity-c2').waitFor();
    await immutableA('C2 message_end');
    // Do not late-end C1: late terminal ordering is a separate concern.
    await settle([aUser, a, bUser, b, cUser, c1, c2], 'identity-c-run');
    await before(cUser.content, C1); await before(C1, C2); await distinct(C1, C2);
    assert.equal(await history('identity-c1').getByText(C1, { exact: true }).count(), 1);
    assert.equal(await history('identity-c2').getByText(C2, { exact: true }).count(), 1);
    pass('missing message_start plus a new explicit ID creates two ordered replies without concatenation, including settlement');

    // A user boundary alone must reset the assistant slot: no agent_start or
    // message_start occurs between settled C2 and this anonymous first delta.
    end(dUser);
    delta(null, D);
    await before(C2, dUser.content); await before(dUser.content, D);
    await distinct(C2, D);
    await immutableA('anonymous first delta after user D');
    assert.equal(await history('identity-c2').getByText(C2, { exact: true }).count(), 1);
    end(d);
    await settle([aUser, a, bUser, b, cUser, c1, c2, dUser, d], 'identity-d-run');
    await before(C2, dUser.content); await before(dUser.content, D);
    await distinct(C2, D);
    pass('user boundary isolates an anonymous first delta without agent_start from the previous settled reply');

    assert.deepEqual(errors, [], 'no browser runtime errors');
    assert.equal(fixture.requests.filter(request => ['prompt', 'queue_message', 'abort', 'compact', 'set_api_key', 'set_model', 'set_thinking_level', 'set_active_tools'].includes(request.body.type)).length, 0, 'no send/model/credential configuration requests');
    pass('isolated fixture only; zero prompt/model/credential writes');
    await page.screenshot({ path: 'artifacts/cross-turn-message-identity.png', animations: 'disabled' });
  } catch (error) {
    results.push({ failure: error.message, stack: error.stack });
    if (page) await page.screenshot({ path: 'artifacts/cross-turn-message-identity-failure.png', animations: 'disabled' }).catch(() => {});
    throw error;
  } finally {
    fs.writeFileSync('artifacts/cross-turn-message-identity-validation.json', JSON.stringify({
      results, errors,
      expectedIds: ['identity-a-user', 'identity-a', 'identity-b-user', 'identity-b', 'identity-c-user', 'identity-c1', 'identity-c2', 'identity-d-user', 'identity-d'],
      scope: 'formal WASM Web + isolated HTTP/SSE fixture, not native desktop acceptance',
      forbiddenRequests: fixture.requests.filter(request => ['prompt', 'queue_message', 'abort', 'compact', 'set_api_key', 'set_model', 'set_thinking_level', 'set_active_tools'].includes(request.body.type)),
    }, null, 2));
    try { if (browser) await browser.close(); } finally { await fixture.close(); }
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
