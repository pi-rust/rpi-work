// Acceptance: formal, already-built WASM app + isolated HTTP/SSE fixtures only.
// Run after main wiring/build: node tests/e2e/verify-user-timeline.cjs
// No component/prototype fallback, app-instance navigation calls, or model writes.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { performance } = require('node:perf_hooks');
const { setTimeout: delay } = require('node:timers/promises');
const { chromium } = require('../support/browser.cjs');
const { startFixture, fixtureCommand } = require('../fixtures/ui-fixture.cjs');

const URL_APP = process.env.WORK_UI_URL || 'http://127.0.0.1:14180/';
const PANEL = '#conversation-panel.transcript[data-session-scope]';
const ROOT = '.transcript-frame > .conversation-timeline';
const NODE = `${ROOT} .timeline-node[data-timeline-target][data-preview]`;
const LIST = `${ROOT} .timeline-list`;
const cases = [];
const liveApps = new Set();
const test = (name, run) => cases.push({ name, run });
const user = (id, content = `问题 ${id}`) => ({ entryId: id, role: 'user', content });
const answer = (id, lines = 24) => ({ entryId: `answer-${id}`, role: 'assistant', content: [{ type: 'text', text: Array.from({ length: lines }, (_, n) => `答复 ${id} / ${n}：真实内容撑开阅读区域。`).join('\n\n') }] });
const turns = (count = 4) => Array.from({ length: count }, (_, i) => [user(`u-${i}`), answer(i)]).flat();
const history = (count = 150) => Array.from({ length: count }, (_, i) => user(`h-${String(i).padStart(3, '0')}`, `历史 ${i}：保留真实游标与阅读锚点。\n${'内容段落。'.repeat(12)}`));
const deferred = () => { let resolve; const promise = new Promise(r => { resolve = r; }); return { promise, resolve }; };
async function poll(fn, message = 'condition', timeout = 10000) {
  const end = performance.now() + timeout;
  while (performance.now() < end) { if (await fn()) return; await delay(25); }
  throw new Error(`Timed out: ${message}`);
}
const closeSafely = async resource => { if (resource) await resource.close().catch(() => {}); };

// CDP delivers genuine browser touch input, including pointer capture/cancellation.
async function withNativeTouch(s, run) {
  const cdp = await s.context.newCDPSession(s.page);
  try {
    await cdp.send('Emulation.setTouchEmulationEnabled', { enabled: true, maxTouchPoints: 1 });
    const touch = (type, point) => cdp.send('Input.dispatchTouchEvent', { type, touchPoints: point ? [{ ...point, id: 1, radiusX: 2, radiusY: 2, force: 1 }] : [] });
    await run(touch);
  } finally { await cdp.detach(); }
}
async function touchDrag(touch, point, distance) {
  await touch('touchStart', point);
  for (let i = 1; i <= 8; i++) {
    await touch('touchMove', { x: point.x, y: point.y + distance * i / 8 });
    await delay(30);
  }
  await delay(100); // Avoid relying on a fling after touchEnd to reach the edge.
  await touch('touchEnd');
}

async function setup(browser, config = {}) {
  const fixture = await startFixture(0, { pagedHistory: true, ...config.fixture });
  const context = await browser.newContext({ viewport: { width: 1440, height: 1000 }, hasTouch: !!config.touch, isMobile: !!config.touch });
  const page = await context.newPage();
  page.setDefaultTimeout(8000); page.setDefaultNavigationTimeout(12000);
  const errors = [], blocked = [], gates = [], nodes = [fixture];
  page.on('pageerror', e => errors.push(e.message));
  const s = { fixture, context, page, errors, blocked, gates, nodes,
    panel: page.locator(PANEL), root: page.locator(ROOT), ticks: page.locator(NODE), list: page.locator(LIST),
    composer: page.locator('.composer textarea'),
    toggle: page.locator('.app-header').getByRole('button', { name: /^(显示|隐藏)时间线$/ }),
    requests: (node = fixture) => node.requests.filter(r => r.body.type === 'get_messages'),
  };
  let closing;
  s.close = () => closing ||= (async () => {
    for (const gate of gates) gate.resolve();
    await closeSafely(context); for (const node of nodes) await closeSafely(node);
    liveApps.delete(s);
  })();
  liveApps.add(s);
  try {
    await context.addInitScript(mode => localStorage.setItem('rpi-work-appearance-v1', JSON.stringify({ theme: 'codex', mode, density: 'comfortable' })), config.mode || 'light');
    await context.route('**/*', async route => {
      const req = route.request(), url = new URL(req.url());
      if (url.origin === 'https://api.github.com') return route.fulfill({ status: 404, body: '{}' });
      const node = nodes.find(n => url.origin === new URL(n.url).origin);
      if (node && req.method() !== 'GET' && req.method() !== 'OPTIONS') {
        const body = req.postDataJSON();
        // Opening an existing session is read-only; all command mutations fail closed.
        if (!(url.pathname.endsWith('/open') || (url.pathname.endsWith('/commands') && /^get_/.test(body?.type || '')))) {
          blocked.push(`${req.method()} ${url.pathname} ${body?.type || ''}`);
          return route.abort('blockedbyclient');
        }
      }
      if (url.origin !== new URL(URL_APP).origin && !node) {
        blocked.push(req.url()); return route.abort('blockedbyclient');
      }
      return route.continue();
    });
    // Start with actual fixture messagePage(limit=48), not a component-only DOM.
    // Drop unrelated default projection events unless a case explicitly tests them.
    await context.route('**/api/v1/sessions/*/snapshot?*', async route => {
      const response = await route.fetch();
      const body = await response.json();
      if (!config.projection) { body.events = []; delete body.latestSettlement; }
      await route.fulfill({ response, json: body });
    });
    fixture.setMessages(config.messages || turns());
    fixture.setMessages([user('architecture-only'), answer('architecture')], 'architecture');
    fixture.setMessages([user('mobile-only'), answer('mobile')], 'mobile');
    if (config.prepare) await config.prepare(fixture);
    s.connect = async node => {
      await page.getByRole('button', { name: '打开设置', exact: true }).click();
      const settings = page.locator('.settings-page');
      await settings.getByRole('navigation', { name: '设置分类', exact: true }).getByRole('button', { name: '执行节点', exact: true }).click();
      if (!await settings.getByLabel('节点地址', { exact: true }).isVisible())
        await settings.getByRole('button', { name: '连接其他节点', exact: true }).click();
      await settings.getByLabel('节点地址', { exact: true }).fill(node.url);
      await settings.getByRole('button', { name: '连接节点', exact: true }).click();
      await settings.locator('.server-overview .server-details').getByRole('status').filter({ hasText: '在线' }).waitFor();
      await settings.getByRole('button', { name: '返回会话', exact: true }).click();
      const toggle = page.locator('[data-project-id="rpi-work"] .project-toggle');
      if (await toggle.getAttribute('aria-expanded') !== 'true') await toggle.click();
      await page.locator('[data-session-id="dark"] .session-item').click();
      await page.waitForFunction(origin => document.querySelector('#conversation-panel')?.dataset.sessionScope === `${origin}::session::dark`, node.url.replace(/\/$/, ''));
      await poll(() => node.requests.some(r => r.path.endsWith('/dark/snapshot')), 'real snapshot');
    };
    s.visibility = async visible => {
      await poll(async () => await s.toggle.getAttribute('aria-label') === (visible ? '隐藏时间线' : '显示时间线')
        && await s.toggle.getAttribute('aria-pressed') === String(visible)
        && await s.toggle.getAttribute('aria-expanded') === String(visible), `header timeline state ${visible}`);
      await page.waitForFunction(visible => visible
        ? window.__rpiUserTimelines instanceof Map && window.__rpiUserTimelines.size === 1
        : !window.__rpiUserTimelines || window.__rpiUserTimelines.size === 0, visible);
      if (!visible) assert.equal(await s.root.count(), 0, 'hidden timeline is unmounted, not CSS-hidden');
    };
    s.count = n => page.waitForFunction(({ selector, n }) => document.querySelectorAll(selector).length === n, { selector: NODE, n });
    s.keys = () => s.ticks.evaluateAll(es => es.map(e => e.dataset.timelineTarget));
    s.load = () => s.root.getByRole('button', { name: '在时间线中显示更早的消息', exact: true });
    // Opaque keys are compared as data, never interpolated into CSS.
    s.geometry = key => s.panel.evaluate((el, key) => {
      const row = [...el.querySelectorAll('.message-turn[data-user-message-key]')].find(e => e.dataset.userMessageKey === key);
      if (!row) throw new Error(`Missing real user target ${key}`);
      const offset = row.getBoundingClientRect().top - el.getBoundingClientRect().top;
      const max = el.scrollHeight - el.clientHeight;
      const desired = Math.max(0, Math.min(max, el.scrollTop + offset - 12));
      return { offset, top: el.scrollTop, max, desired };
    }, key);
    s.jump = async index => {
      const tick = s.ticks.nth(index), key = await tick.getAttribute('data-timeline-target');
      await tick.click();
      await poll(async () => { const g = await s.geometry(key); return Math.abs(g.top - g.desired) <= 3; }, `clamped 12px navigation ${key}`);
      const landed = await s.geometry(key);
      // At the bottom, the active viewport anchor may precede the clicked row.
      if (Math.abs(landed.offset - 12) <= 3)
        await poll(async () => await tick.getAttribute('aria-current') === 'location', 'active tick');
      return key;
    };
    s.outside = () => page.evaluate(() => {
      const rect = selector => { const r = document.querySelector(selector)?.getBoundingClientRect(); return r && { x: r.x, y: r.y, width: r.width, height: r.height }; };
      return { x: scrollX, y: scrollY, header: rect('header'), composer: rect('.composer'), top: document.querySelector('#conversation-panel').scrollTop };
    });
    s.frames = () => page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
    s.railAnchor = () => s.list.evaluate(list => {
      const top = list.getBoundingClientRect().top;
      const node = [...list.querySelectorAll('.timeline-node')].find(e => e.getBoundingClientRect().bottom > top + .5);
      if (!node) throw new Error('No visible rail node');
      return { key: node.dataset.timelineTarget, offset: node.getBoundingClientRect().top - top };
    });
    s.railAnchored = anchor => poll(async () => {
      const actual = await s.railAnchor();
      return actual.key === anchor.key && Math.abs(actual.offset - anchor.offset) <= 2;
    }, 'preserved first visible rail key and pixel offset');
    s.anchor = () => s.panel.evaluate(el => {
      const box = el.getBoundingClientRect();
      const row = [...el.querySelectorAll('.message-turn[data-history-id]')].find(e => e.getBoundingClientRect().bottom > box.top && e.getBoundingClientRect().top < box.bottom);
      if (!row) throw new Error('No visible actual history row');
      return { id: row.dataset.historyId, offset: row.getBoundingClientRect().top - box.top };
    });
    s.anchored = anchor => poll(() => s.panel.evaluate((el, a) => {
      const row = [...el.querySelectorAll('[data-history-id]')].find(e => e.dataset.historyId === a.id);
      return row && Math.abs(row.getBoundingClientRect().top - el.getBoundingClientRect().top - a.offset) <= 3;
    }, anchor), 'preserved reading anchor');
    s.holdPage = async (node = fixture) => {
      const gate = deferred(), arrived = deferred(); gates.push(gate, arrived);
      const pattern = `${node.url}api/v1/sessions/dark/commands`;
      const handler = async route => {
        if (route.request().postDataJSON()?.type !== 'get_messages') return route.fallback();
        // Fetch real server response first so request/cursor accounting stays real.
        try { const response = await route.fetch(); arrived.resolve(); await gate.promise; await route.fulfill({ response }); }
        catch (error) { if (!page.isClosed()) throw error; }
      };
      await context.route(pattern, handler);
      return { arrived: arrived.promise, release: async () => { gate.resolve(); await context.unroute(pattern, handler); } };
    };
    await page.goto(URL_APP); await s.connect(fixture);
    // Each fresh App starts hidden. Existing rail regressions opt in here;
    // visible:false exercises the real default without mounting the component.
    await s.visibility(false);
    if (config.visible !== false) { await s.toggle.click(); await s.visibility(true); }
    await poll(() => fixture.requests.some(r => r.body.type === 'get_state'), 'runtime state from actual fixture API');
    assert(fixture.requests.some(r => r.path.endsWith('/dark/snapshot') && new URLSearchParams(r.query).get('limit') === '48'));
    assert.equal(await page.locator('.transcript-frame > .transcript[data-session-scope]').count(), 1);
    assert.equal(await s.panel.locator('.turn-timeline').count(), 0, 'execution timing is not the input rail');
    assert.equal(await s.root.locator('.turn-timeline').count(), 0);
    return s;
  } catch (error) { await s.close(); throw error; }
}

async function withApp(browser, config, run) {
  const s = await setup(browser, config);
  try {
    await run(s);
    assert.deepEqual(s.errors, [], 'application errors');
    assert.deepEqual(s.blocked, [], 'unexpected network or mutation attempted');
    for (const node of s.nodes) {
      assert.equal(node.requests.filter(r => r.method !== 'GET' && r.method !== 'OPTIONS' && !r.path.endsWith('/open') && !(r.path.endsWith('/commands') && /^get_/.test(r.body.type))).length, 0, 'fixture is read-only');
    }
    return { requests: s.nodes.map(n => ({ commands: n.requests.filter(r => r.body.type).map(r => r.body.type), pages: s.requests(n).map(r => r.body) })) };
  } catch (error) {
    await s.page.screenshot({ path: 'artifacts/user-timeline-failure.png', animations: 'disabled', timeout: 3000 }).catch(() => {});
    throw error;
  } finally { await s.close(); }
}

test('single real user and mixed identity precedence, no eager history', browser => withApp(browser, {
  messages: [user('one'), answer('one'), { role: 'user', messageId: 'message-id', content: '第二条' }, { role: 'user', id: 'id-only', content: '第三条' }, { role: 'user', content: '无游标但可以定位' }, answer('tail')],
}, async s => {
  await s.count(4);
  assert.deepEqual(await s.keys(), ['entry:one', 'entry:message-id', 'entry:id-only', 'loaded:4']);
  assert.equal(s.requests().length, 0);
  assert.equal(await s.load().count(), 0);
  await s.jump(0); assert(Math.abs((await s.geometry('entry:one')).offset - 12) <= 3);
  await s.jump(3);
  assert.equal(await s.root.getByRole('navigation', { name: '用户输入时间线', exact: true }).count(), 1);
}));

test('one input stays visible; assistant/tool-only complete history hides the rail', async browser => {
  await withApp(browser, { messages: [user('single'), answer('single')] }, async s => { await s.count(1); assert(await s.root.isVisible()); await s.jump(0); });
  return withApp(browser, { messages: [answer('only'), { role: 'toolResult', toolCallId: 'orphan', toolName: 'read', content: '工具结果' }] }, async s => {
    await s.count(0); assert.equal(await s.root.isVisible(), false); assert.equal(s.requests().length, 0);
  });
});

test('snapshot + projection public user de-duplication, private/settlement/queue excluded', browser => withApp(browser, {
  projection: true,
  messages: [user('same', [{ type: 'text', text: '图文输入' }, { type: 'image', mimeType: 'image/gif', data: 'R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7' }]), answer('same'), { role: 'toolResult', toolName: 'read', toolCallId: 'orphan', content: '不是问题' }, { role: 'assistant', content: [{ type: 'thinking', text: 'PRIVATE_THOUGHT' }] }, { role: 'system', content: 'INTERNAL_SYSTEM' }],
  prepare: node => {
    node.setQueue([{ entryId: 'queued-only', text: 'QUEUED_NOT_PUBLIC', queue: 'followUp', status: 'queued' }]);
    node.emit({ type: 'message_end', message: user('same', '图文输入') });
    // Restore image-bearing authoritative snapshot after the projection event.
    node.setMessages([user('same', [{ type: 'text', text: '图文输入' }, { type: 'image', mimeType: 'image/gif', data: 'R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7' }]), answer('same'), { role: 'assistant', content: [{ type: 'thinking', text: 'PRIVATE_THOUGHT' }] }, { role: 'toolResult', toolCallId: 'orphan', toolName: 'read', content: '工具结果' }]);
  },
}, async s => {
  await s.count(1); assert.deepEqual(await s.keys(), ['entry:same']);
  assert.equal(await s.panel.locator('.message-turn[data-user-message-key]').count(), 1);
  assert.equal(await s.panel.locator('.conversation-message-image').count(), 1);
  const all = await s.ticks.evaluateAll(es => es.map(e => e.outerHTML).join(''));
  assert(!/PRIVATE_THOUGHT|QUEUED_NOT_PUBLIC|工具结果|settled/.test(all));
  s.fixture.emit({ type: 'agent_start', runId: 'timeline-settlement' });
  await s.page.locator('.composer-actions .send-button.is-loading').waitFor();
  s.fixture.emit({ type: 'agent_settled', runId: 'timeline-settlement', outcome: 'completed', summary: '结算不是用户输入' });
  // Settlement changes the real run controls; its summary is not a chat message.
  await s.page.getByRole('button', { name: '发送消息', exact: true }).waitFor();
  assert.equal(await s.page.locator('.composer-actions .send-button.is-loading').count(), 0);
  await s.count(1); assert.deepEqual(await s.keys(), ['entry:same']);
  assert.equal(await s.panel.locator('.message-turn[data-user-message-key]').count(), 1);
  assert.equal(await s.panel.locator('.message-turn[data-user-message-key]').filter({ hasText: '结算不是用户输入' }).count(), 0);
  assert.equal(await s.panel.getByText('结算不是用户输入', { exact: true }).count(), 0, 'no fabricated summary chat row');
  assert(!/PRIVATE_THOUGHT|QUEUED_NOT_PUBLIC|工具结果|settled|结算不是用户输入/.test(await s.ticks.evaluateAll(es => es.map(e => e.outerHTML).join(''))));
}));

test('opaque hostile IDs and Unicode-140 preview use inert data, never HTML/secret/image bytes', browser => withApp(browser, {
  messages: [user('x"]#<img src=x onerror=alert(1)>/\\:..', ` 甲\n\t乙\u202e\u2066\u0001 ${'😀界'.repeat(90)}<script>BAD()</script>`), user('image-only', [{ type: 'image', mimeType: 'image/gif', data: 'R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7' }, { type: 'thinking', text: 'SECRET_PREVIEW' }]), answer('tail')],
}, async s => {
  await s.count(2);
  const previews = await s.ticks.evaluateAll(es => es.map(e => ({ text: e.dataset.preview, label: e.getAttribute('aria-label'), title: e.getAttribute('title'), html: e.innerHTML })));
  assert.equal([...previews[0].text].length, 140); assert(previews[0].text.startsWith('甲 乙 '));
  assert(!/[\u0000-\u001f\u202a-\u202e\u2060-\u2069]/u.test(previews[0].text));
  assert.equal(previews[1].text, '图片消息');
  assert(!/SECRET_PREVIEW|base64|R0lGOD|<script|<img/.test(JSON.stringify(previews)));
  assert(previews.every(p => p.title === null));
  await s.jump(0); await s.jump(1);
}));

test('keyboard roves without moving transcript/window/composer until Enter or Space', browser => withApp(browser, {}, async s => {
  await s.count(4); await s.jump(0);
  await s.ticks.first().focus(); await s.frames(); const before = await s.outside();
  for (const [key, index] of [['ArrowDown', 1], ['End', 3], ['ArrowUp', 2], ['Home', 0]]) {
    await s.page.keyboard.press(key);
    assert(await s.ticks.nth(index).evaluate(e => e === document.activeElement));
    assert.equal(await s.root.locator('.timeline-node[tabindex="0"]').count(), 1);
    assert.deepEqual(await s.outside(), before);
    assert.equal(await s.ticks.first().getAttribute('aria-current'), 'location');
  }
  await s.page.keyboard.press('ArrowDown'); await s.page.keyboard.press('Enter');
  await poll(async () => Math.abs((await s.geometry('entry:u-1')).offset - 12) <= 3);
  await s.page.keyboard.press('ArrowDown'); await s.page.keyboard.press('Space');
  await poll(async () => Math.abs((await s.geometry('entry:u-2')).offset - 12) <= 3);
  await s.page.keyboard.press('Escape'); assert.equal(await s.root.locator('.timeline-preview:visible').count(), 0);
  assert.equal(await s.page.locator('.message-turn:not([data-user-message-key]) [tabindex="0"].timeline-node').count(), 0);
}));

test('pointer clicks traverse whole rail and clamp actual bottom range', browser => withApp(browser, { messages: [...turns(5), user('bottom')] }, async s => {
  await s.count(6); await s.composer.fill('保留草稿'); await s.composer.focus();
  const before = await s.outside();
  for (const i of [0, 2, 4, 1, 5, 3, 0]) {
    const key = await s.jump(i), g = await s.geometry(key);
    if (i === 5) assert(Math.abs(g.top - g.max) <= 3, 'bottom target clamps to actual scroll range');
    else assert(Math.abs(g.offset - 12) <= 3);
    assert(await s.composer.evaluate(e => e === document.activeElement));
    const after = await s.outside(); assert.deepEqual({ ...after, top: before.top }, before);
  }
  assert.equal(await s.composer.inputValue(), '保留草稿');
}));

test('hover preview is bounded, hoverable, escaped/left cleanly without stealing focus', browser => withApp(browser, {}, async s => {
  await s.count(4); await s.composer.fill('悬停保留焦点'); await s.composer.focus();
  const before = await s.outside(); await s.ticks.first().hover();
  const tooltip = s.root.getByRole('tooltip', { exact: true }); await tooltip.waitFor();
  const id = await tooltip.getAttribute('id'); assert.equal(await s.ticks.first().getAttribute('aria-describedby'), id);
  assert.equal(await s.ticks.first().getAttribute('title'), null);
  await tooltip.hover(); assert(await tooltip.isVisible());
  assert(await s.composer.evaluate(e => e === document.activeElement));
  assert.deepEqual(await s.outside(), before);
  await s.page.keyboard.press('Escape'); await tooltip.waitFor({ state: 'hidden' });
  assert.equal(await s.ticks.first().getAttribute('aria-describedby'), null);
  await s.ticks.nth(1).hover(); await tooltip.waitFor(); await s.composer.hover(); await tooltip.waitFor({ state: 'hidden' });
}));

test('manual transcript scroll updates active; independent rail wheel/focus resists reveal', browser => withApp(browser, { fixture: { pagedHistory: false }, messages: Array.from({ length: 90 }, (_, i) => user(`many-${i}`, `输入 ${i}：${'足够内容。'.repeat(12)}`)) }, async s => {
  // Non-paged snapshot for this case is still served by the real fixture.
  await s.count(90);
  await s.panel.hover(); await s.page.mouse.wheel(0, -500);
  await poll(async () => (await s.panel.evaluate(e => e.scrollTop)) < (await s.panel.evaluate(e => e.scrollHeight - e.clientHeight)) - 100);
  const g = await s.outside();
  await s.list.hover(); await s.page.mouse.wheel(0, -100000); await s.frames();
  assert.equal(await s.list.evaluate(e => e.scrollTop), 0);
  assert.deepEqual(await s.outside(), g);
  await s.ticks.first().focus();
  await s.panel.evaluate(el => { el.scrollTop = el.scrollHeight / 2; el.dispatchEvent(new Event('scroll')); });
  await s.frames(); assert.equal(await s.list.evaluate(e => e.scrollTop), 0);
  const active = await s.root.locator('.timeline-node[aria-current="location"]').getAttribute('data-timeline-target');
  assert.notEqual(active, await s.ticks.first().getAttribute('data-timeline-target'));
  assert(await s.ticks.first().evaluate(e => e === document.activeElement));
}));

test('90 inputs, primary drag threshold, bounded edge scrub and synthetic click dedup', browser => withApp(browser, { fixture: { pagedHistory: false }, messages: Array.from({ length: 90 }, (_, i) => user(`drag-${i}`, `输入 ${i} ${'长内容。'.repeat(18)}`)) }, async s => {
  await s.count(90); assert(await s.list.evaluate(e => e.scrollHeight > e.clientHeight));
  // Enter the rail before selecting a drag origin: an initial transcript-follow
  // frame may otherwise reveal the active tail after we capture the first tick.
  await s.list.hover(); await s.frames();
  await s.list.evaluate(e => { e.scrollTop = 0; }); await s.frames();
  const box = await s.ticks.first().boundingBox(), listBox = await s.list.boundingBox();
  const x = box.x + box.width / 2, y = box.y + box.height / 2;
  // Observe the real compatibility click: it must not dispatch another host
  // navigation after pointerup. This does not wrap or invoke application methods.
  await s.list.evaluate(list => {
    const pager = list.closest('.transcript-frame').querySelector('.history-pager');
    let navigations = 0;
    pager?.addEventListener('rpi-history-navigation', () => { navigations++; });
    list.addEventListener('click', () => {
      const before = navigations;
      setTimeout(() => { list.dataset.testClickExtraNavigations = String(navigations - before); }, 0);
    }, true);
  });
  await s.page.mouse.move(x, y); await s.page.mouse.down();
  await s.page.mouse.move(x, y + 2); await s.frames(); assert.equal(await s.list.getAttribute('data-scrubbing'), null);
  await s.page.mouse.move(x, y + 8); await poll(async () => await s.list.getAttribute('data-scrubbing') === 'true');
  await s.page.mouse.move(x, listBox.y + listBox.height - 2);
  await poll(async () => await s.list.evaluate(e => e.scrollTop > 40), 'edge autoscroll');
  assert(await s.list.evaluate(e => e.scrollTop <= e.scrollHeight - e.clientHeight));
  await s.page.mouse.up(); await s.frames(); assert.equal(await s.list.getAttribute('data-scrubbing'), null);
  await poll(async () => await s.list.getAttribute('data-test-click-extra-navigations') !== null, 'real compatibility click observed');
  assert.equal(await s.list.getAttribute('data-test-click-extra-navigations'), '0', 'real browser compatibility click is deduplicated');
  const before = await s.outside(); await s.frames();
  assert.deepEqual(await s.outside(), before);
}));

test('pointercancel/lost capture/document blur cancel real held gestures and edge frames', browser => withApp(browser, { fixture: { pagedHistory: false }, messages: turns(16) }, async s => {
  await s.count(16);
  for (const event of ['pointercancel', 'lostpointercapture', 'blur']) {
    await s.list.evaluate(e => { e.scrollTop = 0; });
    const box = await s.ticks.first().boundingBox();
    // Record the genuine browser pointer id, never call component methods.
    await s.list.evaluate(e => { e.addEventListener('pointerdown', ev => { e.dataset.testPointer = ev.pointerId; }, { once: true }); });
    await s.page.mouse.move(box.x + box.width / 2, box.y + box.height / 2); await s.page.mouse.down();
    await s.page.mouse.move(box.x + box.width / 2, box.y + box.height / 2 + 8);
    await poll(async () => await s.list.getAttribute('data-scrubbing') === 'true');
    if (event === 'blur') await s.page.evaluate(() => window.dispatchEvent(new Event('blur')));
    else await s.list.evaluate((e, type) => e.dispatchEvent(new PointerEvent(type, { pointerId: Number(e.dataset.testPointer), bubbles: true, isPrimary: true })), event);
    await s.frames(); assert.equal(await s.list.getAttribute('data-scrubbing'), null);
    const position = await s.outside(), railTop = await s.list.evaluate(e => e.scrollTop);
    await s.frames(); assert.deepEqual(await s.outside(), position); assert.equal(await s.list.evaluate(e => e.scrollTop), railTop);
    await s.page.mouse.up();
  }
}));

test('real CDP phone touch hold/drag previews on the compact rail, tap navigates and cancellation cleans up', browser => withApp(browser, { touch: true, fixture: { pagedHistory: false }, messages: turns(12) }, async s => {
  await s.count(12);
  for (const width of [390, 320]) {
    await s.page.setViewportSize({ width, height: 844 });
    assert(await s.ticks.evaluateAll(es => es.every(e => { const r = e.getBoundingClientRect(); return r.width >= 32 && Math.abs(r.height - 15) <= 1 && !e.hasAttribute('title'); })));
    assert.equal(await s.list.evaluate(e => getComputedStyle(e).touchAction), 'none');
    assert.notEqual(await s.panel.evaluate(e => getComputedStyle(e).touchAction), 'none', 'only the rail intercepts touch');
    await s.composer.fill('触屏浏览保留草稿'); await s.composer.focus();
    await withNativeTouch(s, async touch => {
      await s.list.evaluate(e => { e.scrollTop = 0; }); await s.frames();
      const box = await s.ticks.nth(3).boundingBox(), point = { x: box.x + box.width / 2, y: box.y + box.height / 2 };
      const before = await s.outside();
      await touch('touchStart', point);
      const tooltip = s.root.getByRole('tooltip'); await tooltip.waitFor();
      assert.equal(await tooltip.locator('.timeline-label').textContent(), '问题 u-3');
      assert.equal(await tooltip.locator('.timeline-preview-index').textContent(), '4 / 12');
      assert.deepEqual(await s.outside(), before, 'holding previews without navigating');
      await touch('touchMove', { x: point.x, y: point.y + 30 });
      await poll(async () => await s.list.getAttribute('data-scrubbing') === 'true');
      await poll(async () => await tooltip.locator('.timeline-label').textContent() === '问题 u-5');
      assert.equal(await tooltip.locator('.timeline-preview-index').textContent(), '6 / 12');
      const bounds = await tooltip.boundingBox();
      assert(bounds.x >= 0 && bounds.x + bounds.width <= width && bounds.y >= 0 && bounds.y + bounds.height <= 844);
      await s.page.screenshot({ path: `artifacts/user-timeline-touch-preview-${width}.png`, animations: 'disabled' });
      await touch('touchEnd');
      await tooltip.waitFor({ state: 'hidden' });
      assert.equal(await s.list.getAttribute('data-scrubbing'), null);
      assert(await s.composer.evaluate(e => e === document.activeElement), 'touch keeps composer focus');
      assert.equal(await s.composer.inputValue(), '触屏浏览保留草稿');
      await touch('touchStart', point); await tooltip.waitFor();
      await touch('touchCancel'); await tooltip.waitFor({ state: 'hidden' });
      assert.equal(await s.list.getAttribute('data-scrubbing'), null);
      await touch('touchStart', point); await touch('touchEnd');
      await poll(async () => Math.abs((await s.geometry('entry:u-3')).offset - 12) <= 3);
    });
  }
}));

test('jump pauses streaming follow; real media resize preserves anchor; latest explicitly resumes', browser => withApp(browser, {}, async s => {
  await s.count(4); await s.jump(1); const anchor = await s.anchor();
  s.fixture.emit({ type: 'message_update', message: { entryId: 'stream-after-jump', role: 'assistant' }, eventType: 'text_delta', delta: 'STREAM_AFTER_JUMP_REAL_APPEND' });
  await s.panel.getByText('STREAM_AFTER_JUMP_REAL_APPEND', { exact: true }).waitFor(); await s.anchored(anchor);
  // Generate a valid tall raster, then introduce it through the actual live renderer.
  const data = await s.page.evaluate(() => { const canvas = document.createElement('canvas'); canvas.width = 240; canvas.height = 1200; canvas.getContext('2d').fillRect(0, 0, 240, 1200); return canvas.toDataURL('image/png').split(',')[1]; });
  const message = { entryId: 'real-tall-media', role: 'assistant', content: [{ type: 'image', mimeType: 'image/png', data }] };
  s.fixture.emit({ type: 'message_start', message }); s.fixture.emit({ type: 'message_end', message });
  await poll(() => s.panel.locator('.conversation-message-image img').evaluateAll(es => es.some(img => img.complete && img.naturalHeight === 1200)), 'real tall media decode');
  await s.anchored(anchor);
  await s.page.getByRole('button', { name: '回到最新消息', exact: true }).click();
  await poll(() => s.panel.evaluate(e => Math.abs(e.scrollHeight - e.clientHeight - e.scrollTop) <= 3), 'explicit latest');
  s.fixture.emit({ type: 'message_update', message: { entryId: 'stream-resumed', role: 'assistant' }, eventType: 'text_delta', delta: 'STREAM_RESUMED_REAL_APPEND' });
  await s.panel.getByText('STREAM_RESUMED_REAL_APPEND', { exact: true }).waitFor();
  await poll(() => s.panel.evaluate(e => Math.abs(e.scrollHeight - e.clientHeight - e.scrollTop) <= 3));
}));

test('tool-only last page automatically fills the rail and preserves the current reading anchor', browser => withApp(browser, {
  messages: [...history(48), ...Array.from({ length: 48 }, (_, i) => ({ entryId: `tool-${i}`, role: 'assistant', content: [{ type: 'toolCall', id: `call-${i}`, name: 'read', arguments: { path: `file-${i}` } }] }))],
  prepare: fixture => { fixture.controls.historyDelayMs = 1000; },
}, async s => {
  await s.count(0); assert(await s.root.isVisible());
  await poll(() => s.requests().length === 1, 'default fill request');
  const anchor = await s.anchor();
  assert.equal(await s.load().isDisabled(), true); assert.equal(await s.load().getAttribute('aria-busy'), 'true');
  const box = await s.load().boundingBox();
  for (let i = 0; i < 3; i++) await s.page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
  assert.equal(s.requests().length, 1, 'busy initial fill cannot double-fetch');
  assert.equal(s.requests()[0].body.beforeId, 'tool-0'); assert.equal(s.requests()[0].body.limit, 48);
  await s.count(48); await s.anchored(anchor); assert.equal(s.requests().length, 1); assert.equal(await s.load().count(), 0);
}));

test('pending history keeps rail/targets live; anchor navigation and failure retry retain real cursor', browser => withApp(browser, { messages: history() }, async s => {
  await s.count(48); const before = await s.keys(), gate = await s.holdPage();
  try {
    await s.load().click(); await gate.arrived; assert.deepEqual(await s.keys(), before);
    await s.jump(12); const anchor = await s.anchor(); await gate.release(); await s.count(96); await s.anchored(anchor);
  } finally { await gate.release(); }
  s.fixture.controls.failHistoryCount = 1;
  await s.load().click(); await poll(() => s.requests().length === 2);
  await s.page.getByRole('button', { name: '重试加载更早消息', exact: true }).waitFor();
  assert.equal(await s.ticks.count(), 96);
  await s.load().click(); await s.count(144);
  assert.equal(s.requests().length, 3); assert.equal(s.requests()[1].body.beforeId, s.requests()[2].body.beforeId);
  assert(s.requests().every(r => r.body.limit === 48));
}));

test('unsupported capability and missing real cursor are truthful disabled history, not false end', async browser => {
  await withApp(browser, { messages: history(), fixture: { noMessagePaging: true } }, async s => {
    await s.count(48); assert(await s.load().isDisabled()); assert.equal(s.requests().length, 0);
    assert.match(await s.root.textContent(), /不支持历史分页/);
    assert.equal(await s.page.getByText('已加载到会话开头', { exact: true }).count(), 0);
  });
  // The real messagePage reports hasMore but firstId=null when the leading
  // loaded entry lacks an entryId. Never invent a cursor from loaded:{index}.
  return withApp(browser, { messages: Array.from({ length: 90 }, (_, i) => ({ role: 'user', content: `无后端游标 ${i}` })) }, async s => {
    await s.count(48); assert(await s.load().isDisabled()); assert.equal(s.requests().length, 0);
    assert.match(await s.root.textContent(), /缺少.*游标/); await s.jump(10);
  });
});

test('late session/node/reconnect pages cannot revive stale scopes; tab teardown empties diagnostic Map', browser => withApp(browser, { messages: history() }, async s => {
  await s.count(48);
  s.fixture.controls.failHistoryCount = 1; // Deliver an old error, not just an old successful page.
  const gate = await s.holdPage();
  try {
    await s.load().click(); await gate.arrived;
    await s.page.locator('[data-session-id="architecture"] .session-item').click();
    await s.count(1); assert.deepEqual(await s.keys(), ['entry:architecture-only']);
    await gate.release(); await s.frames(); assert.deepEqual(await s.keys(), ['entry:architecture-only']);
  } finally { await gate.release(); }
  await s.page.locator('#changes-tab').click();
  await s.page.waitForFunction(() => window.__rpiUserTimelines?.size === 0);
  await s.page.locator('#conversation-tab').click(); await s.count(1);
  await s.page.waitForFunction(() => window.__rpiUserTimelines?.size === 1);
  await s.page.locator('[data-session-id="dark"] .session-item').click(); await s.count(48);
  const reconnect = await s.holdPage();
  try {
    await s.load().click(); await reconnect.arrived;
    const snapshots = () => s.fixture.requests.filter(r => r.path.endsWith('/dark/snapshot')).length;
    const n = snapshots(); s.fixture.disconnect(); await poll(() => snapshots() > n, 'real reconnect snapshot');
    await reconnect.release(); await s.count(48);
    assert.equal(await s.page.getByRole('button', { name: '重试加载更早消息', exact: true }).count(), 0);
  } finally { await reconnect.release(); }
  const other = await startFixture(0, { pagedHistory: true }); s.nodes.push(other); other.setMessages([user('new-node-only'), answer('node')]);
  const late = await s.holdPage();
  try {
    await s.load().click(); await late.arrived; await s.connect(other); await s.count(1);
    await late.release(); await s.frames(); assert.deepEqual(await s.keys(), ['entry:new-node-only']);
    assert.equal(await s.page.evaluate(() => window.__rpiUserTimelines.size), 1);
  } finally { await late.release(); }
}));

test('IME draft and ask-card editing never page; focus survives hover/pointer navigation without layout shifts', browser => withApp(browser, { messages: history() }, async s => {
  await s.count(48); await s.composer.fill('已有草稿 中文🙂'); await s.composer.focus();
  await s.composer.dispatchEvent('compositionstart', { data: '拼' });
  await s.composer.dispatchEvent('compositionupdate', { data: '拼音' });
  const before = await s.outside();
  await s.ticks.nth(1).hover(); await s.jump(1);
  assert(await s.composer.evaluate(e => e === document.activeElement));
  await s.composer.dispatchEvent('compositionend', { data: '拼音' });
  assert.equal(await s.composer.inputValue(), '已有草稿 中文🙂');
  const after = await s.outside(); assert.deepEqual({ ...after, top: before.top }, before);
  s.fixture.emit({ type: 'ui_dialog_request', requestId: 'timeline-edit', toolCallId: 'timeline-ask', ui: { purpose: 'question', kind: 'input', question: '保留输入光标，不加载历史' } });
  const reply = s.panel.getByLabel('timeline-edit · 自由回答', { exact: true });
  await reply.waitFor(); await reply.fill('保留中文回答');
  // Keep the genuine ask control focused while exposing the transcript's older
  // edge. Without the editable guard, Home immediately consumes the real cursor.
  await reply.evaluate(el => { el.focus({ preventScroll: true }); el.setSelectionRange(2, 2); });
  await s.panel.evaluate(el => { el.scrollTop = 0; el.dispatchEvent(new Event('scroll')); });
  await s.frames(); assert.equal(s.requests().length, 0);
  for (const key of ['Home', 'ArrowUp', 'PageUp', 'Shift+Home', 'Control+Home']) await s.page.keyboard.press(key);
  await s.page.keyboard.insertText('光标');
  assert.equal(await reply.inputValue(), '光标保留中文回答', 'Home edits the real answer rather than paging');
  assert(await reply.evaluate(el => el === document.activeElement));
  await delay(900); assert.equal(s.requests().length, 0, 'editing never arms delayed scroll intent');
  assert.equal(await s.load().isEnabled(), true, 'real older history remains available');
  assert.equal(await s.composer.inputValue(), '已有草稿 中文🙂');
  assert.equal(s.fixture.requests.filter(r => /prompt|queue_message|set_api_key|ui_dialog_response/.test(fixtureCommand(r) || '')).length, 0);
}));

const sparseMessages = () => [...history(12), ...Array.from({ length: 94 }, (_, i) => ({
  entryId: `sparse-tool-${i}`, role: 'assistant',
  content: [{ type: 'toolCall', id: `sparse-call-${i}`, name: 'read', arguments: { path: `file-${i}` } }],
})), user('sparse-latest')];
test('visibility default hidden never fills sparse history; explicit ordinary paging still works', browser => withApp(browser, {
  visible: false, messages: sparseMessages(),
}, async s => {
  await s.visibility(false); await delay(900);
  assert.equal(s.requests().length, 0, 'hidden rail must not preload toward ten inputs');
  assert.equal(await s.panel.locator('[data-history-id]').count(), 48, 'bounded initial snapshot');
  const older = s.page.locator('.history-pager').getByRole('button', { name: '加载更早消息', exact: true });
  assert(await older.isEnabled(), 'ordinary pager remains available while timeline is hidden');
  await older.click();
  await poll(async () => await s.panel.locator('[data-history-id]').count() === 96, 'explicit hidden-history page');
  await delay(900);
  assert.deepEqual(s.requests().map(r => [r.body.beforeId, r.body.limit]), [['sparse-tool-47', 48]], 'manual load must not start a hidden ten-tick fill chain');
  await s.visibility(false);
}));

test('visibility header Enter/Space toggles cleanly and remount preserves draft and reading anchor', browser => withApp(browser, {
  visible: false, messages: history(),
}, async s => {
  await s.composer.fill('显示隐藏后保留草稿 中文🙂');
  await s.panel.evaluate(el => { el.scrollTop = (el.scrollHeight - el.clientHeight) * .45; el.dispatchEvent(new Event('scroll')); });
  await s.frames(); const anchor = await s.anchor();
  await s.toggle.focus(); await s.page.keyboard.press('Enter');
  await s.visibility(true); await s.count(48); await s.anchored(anchor);
  assert(await s.toggle.evaluate(el => el === document.activeElement), 'keyboard toggle retains header focus');
  await s.ticks.nth(20).focus();
  await s.root.locator('.timeline-preview:visible').waitFor();
  await s.toggle.focus(); await s.page.keyboard.press('Space'); await s.visibility(false);
  assert.equal(await s.page.locator('.timeline-preview:visible').count(), 0, 'unmount removes any open preview');
  await s.frames(); await s.anchored(anchor);
  await s.page.keyboard.press('Enter'); await s.visibility(true); await s.count(48);
  await s.anchored(anchor);
  assert.equal(await s.composer.inputValue(), '显示隐藏后保留草稿 中文🙂');
  assert.equal(s.requests().length, 0, 'toggle never acts as older-history intent');
}));

test('visibility hiding an in-flight initial fill cleans registry and stops hidden continuation', browser => withApp(browser, {
  visible: false, messages: sparseMessages(),
}, async s => {
  const gate = await s.holdPage();
  try {
    await s.toggle.click(); await s.visibility(true); await gate.arrived;
    assert.equal(s.requests().length, 1);
    await s.toggle.click(); await s.visibility(false);
    await gate.release();
    await poll(async () => await s.panel.locator('[data-history-id]').count() === 96, 'already requested page may finish while hidden');
    await delay(900); await s.visibility(false);
    assert.equal(s.requests().length, 1, 'late completion cannot restart hidden sparse fill or revive registry');
    await s.toggle.click(); await s.visibility(true); await s.count(13);
    assert.deepEqual(s.requests().map(r => r.body.beforeId), ['sparse-tool-47', 'h-011'], 'remount resumes from actual loaded cursor');
  } finally { await gate.release(); }
}));

test('visibility state survives session switches but refresh starts hidden', browser => withApp(browser, {
  visible: false,
}, async s => {
  const select = async id => {
    await s.page.locator(`[data-session-id="${id}"] .session-item`).click();
    await poll(async () => (await s.panel.getAttribute('data-session-scope'))?.endsWith(`::session::${id}`), `selected ${id}`);
  };
  await select('architecture'); await s.visibility(false);
  await s.toggle.click(); await s.visibility(true); await s.count(1);
  await select('dark'); await s.visibility(true); await s.count(4);
  await s.toggle.click(); await s.visibility(false);
  await select('architecture'); await s.visibility(false);
  await s.toggle.click(); await s.visibility(true); await s.count(1);
  await s.page.reload(); await s.panel.waitFor(); await s.visibility(false);
  await delay(900); assert.equal(s.requests().length, 0, 'refresh never restores visibility from storage');
}));

test('visibility 390/320 header toggle has touch-sized bounds and no horizontal overflow', browser => withApp(browser, {
  visible: false, touch: true,
}, async s => {
  for (const width of [390, 320]) {
    await s.page.setViewportSize({ width, height: 844 });
    for (const visible of [false, true]) {
      await s.visibility(visible); await s.frames();
      const g = await s.toggle.evaluate(el => {
        const r = el.getBoundingClientRect(), h = el.closest('.app-header').getBoundingClientRect();
        return { x: r.x, y: r.y, right: r.right, bottom: r.bottom, width: r.width, height: r.height,
          headerTop: h.top, headerBottom: h.bottom, headerRight: h.right,
          viewport: document.documentElement.clientWidth, documentWidth: document.documentElement.scrollWidth, bodyWidth: document.body.scrollWidth };
      });
      assert(g.width >= 44 && g.height >= 44, `touch-sized header button at ${width}: ${JSON.stringify(g)}`);
      assert(g.x >= 0 && g.right <= g.viewport + 1 && g.right <= g.headerRight + 1 && g.y >= g.headerTop - 1 && g.bottom <= g.headerBottom + 1, `header button stays on-screen: ${JSON.stringify(g)}`);
      assert(g.documentWidth <= g.viewport + 1 && g.bodyWidth <= g.viewport + 1, `no horizontal page overflow at ${width}: ${JSON.stringify(g)}`);
      await s.toggle.click();
    }
    await s.visibility(false);
  }
}));

test('default ten inputs automatically fill sparse 48-message pages without clicking the rail', browser => withApp(browser, {
  // Initial suffix: one input plus 47 tools. The first older page still has
  // only one input; a second real cursor is needed to reach ten user inputs.
  messages: sparseMessages(),
}, async s => {
  await s.count(13);
  assert.equal(s.requests().length, 2, 'initial fill follows successive real pages without clicks or wheel input');
  assert.deepEqual(s.requests().map(r => [r.body.beforeId, r.body.limit]), [['sparse-tool-47', 48], ['h-011', 48]]);
  assert(await s.root.isVisible());
  assert.equal(await s.root.locator('.timeline-preview:visible').count(), 0, 'previews remain hover/focus only');
  for (const [width, height, step] of [[1440, 1000, 15], [390, 844, 15]]) {
    await s.page.setViewportSize({ width, height }); await s.frames();
    const g = await s.list.evaluate(list => {
      const box = list.getBoundingClientRect();
      const rows = [...list.querySelectorAll('.timeline-node')].map(node => {
        const r = node.getBoundingClientRect(); return { top: r.top - box.top, bottom: r.bottom - box.top };
      });
      // Native scrollTop rounds to device pixels; tolerate a one-pixel edge.
      const visible = rows.filter(r => r.top >= -1 && r.bottom <= box.height + 1);
      return { height: box.height, count: visible.length, rows, scrollTop: list.scrollTop };
    });
    assert.equal(g.count, 10, `ten real ticks are expanded before any timeline interaction: ${JSON.stringify(g)}`);
    assert.equal(g.height, 10 * step);
    await s.page.screenshot({ path: `artifacts/user-timeline-default-ten-${width}.png`, animations: 'disabled' });
  }
  await delay(500); assert.equal(s.requests().length, 2, 'does not eagerly fetch after sufficient real inputs');
}));

test('default ten inputs stop on errors without retry loops and manual retry keeps the real cursor', browser => withApp(browser, {
  messages: sparseMessages(), prepare: fixture => { fixture.controls.failHistoryCount = 1; },
}, async s => {
  await s.page.getByRole('button', { name: '重试加载更早消息', exact: true }).waitFor();
  await s.count(1);
  const cursor = s.requests()[0].body.beforeId;
  await s.composer.fill('自动补齐失败后保留草稿'); await s.composer.focus();
  await delay(900); assert.equal(s.requests().length, 1, 'failure blocks automatic retry');
  await s.load().click(); await s.count(13);
  assert.deepEqual(s.requests().map(r => r.body.beforeId), [cursor, cursor, 'h-011']);
  assert.equal(await s.composer.inputValue(), '自动补齐失败后保留草稿');
}));

test('default ten inputs respect unsupported paging, absent cursors and nonadvancing pages', async browser => {
  const configurations = [
    { messages: sparseMessages(), fixture: { noMessagePaging: true }, expected: 0 },
    { messages: [...history(12), { role: 'assistant', content: 'No backend cursor' }, ...sparseMessages().slice(-47)], expected: 0 },
    { messages: sparseMessages(), prepare: fixture => { fixture.controls.ignoreHistoryCursor = true; }, expected: 1 },
  ];
  for (const config of configurations) await withApp(browser, config, async s => {
    await s.count(1);
    if (config.expected) {
      await s.page.locator('.history-pager [role=status]').filter({ hasText: '读取更早消息失败' }).waitFor();
      assert(await s.load().isDisabled(), 'a host ignoring its cursor loses pagination capability');
    } else assert(await s.load().isDisabled());
    await delay(700); assert.equal(s.requests().length, config.expected, 'no invented cursors, unsupported calls or infinite page loops');
  });
});

test('default ten inputs stop at history start when fewer exist and ignore late pages in another session', async browser => {
  await withApp(browser, { messages: [user('only-one'), ...sparseMessages().slice(12)] }, async s => {
    await s.count(2); await delay(400);
    assert.equal(s.requests().length, 1, 'history exhausted before ten: stop, do not fabricate ticks');
    assert.equal(await s.load().count(), 0);
  });
  return withApp(browser, { messages: sparseMessages(), prepare: fixture => { fixture.controls.historyDelayMs = 1200; } }, async s => {
    await poll(() => s.requests().length === 1, 'initial background fill');
    await s.page.locator('[data-session-id="architecture"] .session-item').click(); await s.count(1);
    await delay(1500);
    assert.deepEqual(await s.keys(), ['entry:architecture-only'], 'old automatic page cannot revive previous session inputs');
    assert.equal(s.requests().length, 1, 'no continuation of the old fill after switching sessions');
  });
});

test('ten-node always-visible 10/11/48 actual inputs, desktop/phone caps and short-frame bounds', async browser => {
  const details = [];
  for (const count of [10, 11, 48]) {
    details.push(await withApp(browser, { messages: history(count) }, async s => {
      await s.count(count);
      const expected = Array.from({ length: count }, (_, i) => `entry:h-${String(i).padStart(3, '0')}`);
      for (const [width, height] of [[1440, 1000], [390, 844], [1440, 480], [390, 480]]) {
        await s.page.setViewportSize({ width, height }); await s.frames();
        assert(await s.root.isVisible(), 'left input rail is visible without any click');
        assert.deepEqual(await s.keys(), expected, 'every real node remains mounted, not a ten-node slice');
        const g = await s.root.evaluate(root => {
          const rect = e => { const r = e.getBoundingClientRect(); return { left: r.left, right: r.right, top: r.top, bottom: r.bottom, height: r.height }; };
          const list = root.querySelector('.timeline-list');
          return { rail: rect(root), frame: rect(root.parentElement), list: rect(list), message: rect(root.parentElement.querySelector('.message-turn')), composer: rect(document.querySelector('.composer')), rows: [...list.querySelectorAll('.timeline-node')].map(rect), max: list.scrollHeight - list.clientHeight, overflow: getComputedStyle(list).overflowY };
        });
        const step = 15;
        assert(g.rows.every(row => Math.abs(row.height - step) <= 1), `${step}px real input rows`);
        assert(g.list.height > 0 && g.list.height <= 10 * step + 1, 'at most ten rows, fewer on short screens');
        assert(Math.abs(g.list.height - Math.min(10 * step, g.frame.height - 24)) <= 1, 'default list fills ten rows or all available short-frame space');
        if (g.list.height >= 10 * step - .5) {
          const visible = g.rows.filter(row => row.top >= g.list.top - 1 && row.bottom <= g.list.bottom + 1);
          assert.equal(visible.length, 10, `ten ticks actually visible: ${JSON.stringify({ count, width, height, list: g.list, rows: g.rows.map(r => ({ top: r.top - g.list.top, bottom: r.bottom - g.list.top })) })}`);
        }
        assert(Math.abs(g.rail.left - g.frame.left - (width <= 767 ? 0 : 5)) < 1, 'left frame anchor');
        assert(g.message.left >= g.rail.right + 2, 'input rail retains reserved message space');
        assert(g.list.top >= g.rail.top - 1 && g.list.bottom <= g.rail.bottom + 1);
        assert(g.rail.top >= g.frame.top - 1 && g.rail.bottom <= g.frame.bottom + 1 && g.rail.bottom <= g.composer.top + 1);
        if (count === 11 && height > 480) {
          await s.page.screenshot({ path: `artifacts/user-timeline-ten-${width}.png`, fullPage: true, animations: 'disabled' });
        }
        if (count > 10) {
          assert(g.max > 0 && /auto|scroll/.test(g.overflow), 'eleven and 48 inputs scroll internally');
          const outside = await s.outside();
          await s.list.evaluate(e => { e.scrollTop = e.scrollHeight; }); await s.frames();
          assert(await s.list.evaluate(e => e.scrollTop > 0));
          assert.deepEqual(await s.keys(), expected, 'scrolling never drops offscreen nodes');
          assert.deepEqual(await s.outside(), outside, 'programmatic rail browsing cannot move the transcript');
        }
        assert.equal(s.requests().length, 0, 'layout, initial render and programmatic rail scroll do not fetch');
      }
    }));
  }
  return { scenarios: details };
});

test('ten-node real upward wheel pages once, preserves both anchors and uses successive real 48 cursors', browser => withApp(browser, { messages: history() }, async s => {
  await s.count(48); await delay(200); assert.equal(s.requests().length, 0, 'no eager initial fetch');
  await s.jump(0); await s.jump(12);
  await s.panel.evaluate(e => { e.scrollTop = 0; e.dispatchEvent(new Event('scroll')); }); await s.frames();
  for (const top of [64, 0]) {
    await s.list.evaluate((e, top) => { e.scrollTop = top; e.dispatchEvent(new Event('scroll')); }, top); await s.frames();
  }
  await s.list.hover(); await s.page.mouse.wheel(0, 96); await s.frames();
  await delay(200); assert.equal(s.requests().length, 0, 'navigation, programmatic scroll and newer-end wheel are not older intent');
  await s.jump(12);
  await s.list.hover(); await s.list.evaluate(e => { e.scrollTop = 0; }); await s.frames();
  const before = await s.keys(), rail = await s.railAnchor(), transcript = await s.anchor(), gate = await s.holdPage();
  try {
    await s.page.mouse.wheel(0, -120); await gate.arrived;
    for (let i = 0; i < 6; i++) await s.page.mouse.wheel(0, -80);
    await s.frames(); await delay(150);
    assert.equal(s.requests().length, 1, 'held page coalesces repeated real wheel intent');
    assert.deepEqual(await s.keys(), before, 'pending request retains all navigable nodes');
    assert.equal(s.requests()[0].body.beforeId, 'h-102'); assert.equal(s.requests()[0].body.limit, 48);
    await s.railAnchored(rail); await s.anchored(transcript);
  } finally { await gate.release(); }
  await s.count(96); await s.railAnchored(rail); await s.anchored(transcript);
  assert.deepEqual(await s.keys(), Array.from({ length: 96 }, (_, i) => `entry:h-${String(i + 54).padStart(3, '0')}`));
  await delay(900); assert.equal(s.requests().length, 1, 'settlement never replays held intent');
  await s.list.evaluate(e => { e.scrollTop = 0; e.dispatchEvent(new Event('scroll')); }); await s.frames();
  await delay(900); assert.equal(s.requests().length, 1, 'idling at a programmatically exposed older edge cannot page again');
  await s.list.hover(); await s.page.mouse.wheel(0, -120); await s.count(144);
  assert.equal(s.requests().length, 2); assert.equal(s.requests()[1].body.beforeId, 'h-054');
  assert(s.requests().every(r => r.body.limit === 48));
}));

test('ten-node keyboard Home is older intent without transcript navigation or idle repeat', browser => withApp(browser, { messages: history() }, async s => {
  await s.count(48); await s.jump(12); await s.ticks.nth(12).focus();
  const outside = await s.outside(), transcript = await s.anchor(), gate = await s.holdPage();
  let rail;
  try {
    await s.page.keyboard.press('End'); await s.frames();
    assert(await s.ticks.last().evaluate(e => e === document.activeElement));
    assert.equal(s.requests().length, 0, 'End is newer-end browsing only');
    await s.page.keyboard.press('Home'); await gate.arrived;
    assert(await s.ticks.first().evaluate(e => e === document.activeElement));
    assert.equal(await s.list.evaluate(e => e.scrollTop), 0);
    rail = await s.railAnchor();
    assert.deepEqual(await s.outside(), outside, 'Home moves rail focus only, not transcript/window/composer');
    for (let i = 0; i < 3; i++) await s.page.keyboard.press('Home');
    assert.equal(s.requests().length, 1); assert.equal(s.requests()[0].body.beforeId, 'h-102');
  } finally { await gate.release(); }
  await s.count(96); await s.railAnchored(rail); await s.anchored(transcript);
  await delay(900); assert.equal(s.requests().length, 1, 'holding/releasing Home does not create an idle request loop');
}));

test('ten-node real CDP older-edge touch scrub pages once and preserves the released reading anchor', browser => withApp(browser, { touch: true, messages: history() }, async s => {
  await s.page.setViewportSize({ width: 390, height: 844 }); await s.count(48);
  await s.jump(12);
  await s.list.evaluate(e => { e.scrollTop = 64; }); await s.frames();
  await delay(150); assert.equal(s.requests().length, 0, 'programmatic touch starting position cannot page');
  const outside = await s.outside(), gate = await s.holdPage();
  let transcript = await s.anchor();
  let rail;
  try {
    await withNativeTouch(s, async touch => {
      const box = await s.list.boundingBox();
      await touchDrag(touch, { x: box.x + box.width / 2, y: box.y + 40 }, -70);
    });
    await gate.arrived;
    await poll(() => s.list.evaluate(e => e.scrollTop <= 1), 'held touch at the older edge scrolls the rail');
    rail = await s.railAnchor();
    assert.equal(await s.list.getAttribute('data-scrubbing'), null, 'release ends touch scrubbing');
    const released = await s.outside();
    assert.deepEqual(released.composer, outside.composer, 'touch scrub does not move composer/window');
    transcript = await s.anchor();
    assert.equal(s.requests().length, 1); assert.equal(s.requests()[0].body.beforeId, 'h-102');
    assert.equal(s.requests()[0].body.limit, 48);
  } finally { await gate.release(); }
  await s.count(96); await s.railAnchored(rail); await s.anchored(transcript);
  await delay(900); assert.equal(s.requests().length, 1, 'touch settlement is not fresh older-end intent');
}));

test('ten-node automatic history error gates fresh wheel/Home intent but manual retry keeps cursor', browser => withApp(browser, { messages: history() }, async s => {
  await s.count(48); s.fixture.controls.failHistoryCount = 1;
  await s.list.hover(); await s.list.evaluate(e => { e.scrollTop = 0; }); await s.frames();
  await s.page.mouse.wheel(0, -120);
  await s.page.getByRole('button', { name: '重试加载更早消息', exact: true }).waitFor();
  assert.equal(s.requests().length, 1); assert.equal(s.requests()[0].body.beforeId, 'h-102');
  const keys = await s.keys();
  for (let i = 0; i < 3; i++) await s.page.mouse.wheel(0, -120);
  await s.ticks.first().focus(); await s.page.keyboard.press('Home');
  await delay(900);
  assert.equal(s.requests().length, 1, 'error blocks automatic retry even after new older-end intent');
  assert.deepEqual(await s.keys(), keys); assert(await s.load().isEnabled(), 'manual retry remains available');
  await s.load().click(); await s.count(96);
  assert.equal(s.requests().length, 2); assert.equal(s.requests()[1].body.beforeId, s.requests()[0].body.beforeId);
  assert(s.requests().every(r => r.body.limit === 48));
  await delay(900); assert.equal(s.requests().length, 2, 'manual success does not replay failed automatic intent');
}));

test('ten-node unsupported capability and missing backend cursor reject real automatic intent', async browser => {
  const details = [];
  for (const config of [
    { messages: history(), fixture: { noMessagePaging: true } },
    { messages: Array.from({ length: 90 }, (_, i) => ({ role: 'user', content: `无游标输入 ${i}` })) },
  ]) {
    details.push(await withApp(browser, config, async s => {
      await s.count(48); assert(await s.root.isVisible()); assert(await s.load().isDisabled());
      assert.match(await s.root.textContent(), config.fixture ? /不支持历史分页/ : /缺少.*游标/);
      await s.list.hover(); await s.list.evaluate(e => { e.scrollTop = 0; }); await s.frames();
      for (let i = 0; i < 3; i++) await s.page.mouse.wheel(0, -120);
      await s.ticks.last().focus(); await s.page.keyboard.press('Home');
      await delay(900); assert.equal(s.requests().length, 0, 'never invent a cursor or bypass capability with automatic paging');
      assert.equal(await s.page.getByText('已加载到会话开头', { exact: true }).count(), 0);
      assert.equal(await s.ticks.count(), 48);
    }));
  }
  return { scenarios: details };
});

test('light/dark 1440/390/320 screenshots, short viewport, bounded tooltip and real media layout', async browser => {
  for (const mode of ['light', 'dark']) {
    await withApp(browser, { mode, messages: [user('screenshot-image', [{ type: 'text', text: '真实图文输入，预览只显示文字。' }, { type: 'image', mimeType: 'image/gif', data: 'R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7' }]), ...turns(4)] }, async s => {
      await s.count(5);
      await poll(() => s.panel.locator('.conversation-message-image img').evaluateAll(es => es.length > 0 && es.every(img => img.complete && img.naturalHeight > 0)), 'screenshot real image decode');
      for (const width of [1440, 390, 320]) {
        await s.page.setViewportSize({ width, height: width === 1440 ? 1000 : 844 });
        // A click release deliberately suppresses preview at the same pointer
        // coordinates; leave the rail before testing a fresh hover gesture.
        await s.jump(0); await s.composer.hover(); await s.ticks.first().hover();
        const tooltip = s.root.getByRole('tooltip', { exact: true }); await tooltip.waitFor();
        const assertBounds = async () => {
          const bounds = await s.root.evaluate(root => {
            const rect = e => { const r = e.getBoundingClientRect(); return { left: r.left, right: r.right, top: r.top, bottom: r.bottom, width: r.width, height: r.height }; };
            return { rail: rect(root), list: rect(root.querySelector('.timeline-list')), nodeHeight: root.querySelector('.timeline-node').getBoundingClientRect().height, frame: rect(root.parentElement), message: rect(root.parentElement.querySelector('.message-turn')), tooltip: rect(root.querySelector('.timeline-preview')), composer: rect(document.querySelector('.composer')), width: innerWidth, height: innerHeight, scrollWidth: document.documentElement.scrollWidth };
          });
          const { rail: r, frame: f, message: m, tooltip: t, composer: c } = bounds;
          assert(Math.abs(r.left - f.left - (width <= 767 ? 0 : 5)) < 1, 'input timeline is anchored to the left of the conversation frame');
          assert(m.left >= r.right + 2, 'message content reserves room for the left rail');
          assert(Math.abs(t.left - r.right - 8) < 1, 'preview opens inward to the right of the left rail');
          assert(bounds.list.height <= 10 * bounds.nodeHeight + 1, 'list viewport shows at most ten input rows');
          assert(bounds.list.top >= r.top - 1 && bounds.list.bottom <= r.bottom + 1, 'list stays inside rail');
          assert(r.top >= f.top - 1 && r.bottom <= f.bottom + 1, 'rail stays inside transcript frame');
          assert(r.bottom <= c.top + 1, 'rail must not cover input/footer');
          assert(t.left >= 0 && t.right <= bounds.width && t.top >= 0 && t.bottom <= bounds.height, 'tooltip real rectangle fits viewport');
          assert(t.top >= r.top - 1 && t.bottom <= r.bottom + 1);
          assert(bounds.scrollWidth <= bounds.width);
          if (width <= 390) assert(await s.ticks.evaluateAll(es => es.every(e => Math.abs(e.getBoundingClientRect().height - 15) <= 1 && e.getBoundingClientRect().width >= 32)));
        };
        await assertBounds();
        assert.equal(await s.page.evaluate(() => document.documentElement.dataset.mode), mode);
        await s.page.screenshot({ path: `artifacts/user-timeline-${mode}-${width}.png`, fullPage: true, animations: 'disabled' });
        await s.page.setViewportSize({ width, height: 480 }); await s.ticks.first().hover(); await assertBounds();
      }
    });
  }
});

(async () => {
  fs.mkdirSync('artifacts', { recursive: true });
  const filter = process.env.FILTER ? new RegExp(process.env.FILTER, 'i') : null;
  const selected = cases.filter(entry => !filter || filter.test(entry.name));
  const report = { scope: 'formal built WASM main integration; isolated read-only HTTP/SSE; no model', filter: process.env.FILTER || null, available: cases.length, startedAt: new Date().toISOString(), results: [] };
  let browser;
  try {
    assert(selected.length, 'FILTER must select at least one case');
    browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
    for (const entry of selected) {
      const started = performance.now(); let timer;
      try {
        // Hard per-case cutoff, including setup/multiple scenarios. Closing the
        // browser on timeout interrupts actions/fetches; fixture finally releases gates.
        const detail = await Promise.race([entry.run(browser), new Promise((_, reject) => {
          timer = setTimeout(() => {
            for (const app of liveApps) void app.close();
            void browser.close(); reject(new Error('Case exceeded 60s cutoff'));
          }, 57000);
        })]);
        report.results.push({ name: entry.name, status: 'passed', durationMs: Math.round(performance.now() - started), ...detail });
        console.log('PASS', entry.name);
      } catch (error) {
        report.results.push({ name: entry.name, status: 'failed', durationMs: Math.round(performance.now() - started), error: error.stack || String(error) });
        throw error;
      } finally {
        clearTimeout(timer);
        // Only executed cases appear; never claim authored-only coverage passed.
        fs.writeFileSync('artifacts/user-timeline-validation.json', JSON.stringify(report, null, 2));
      }
    }
  } catch (error) { console.error(error); report.error = error.stack || String(error); process.exitCode = 1; }
  finally {
    report.finishedAt = new Date().toISOString(); report.executed = report.results.length; report.planned = selected.length;
    report.passed = report.results.filter(r => r.status === 'passed').length;
    fs.writeFileSync('artifacts/user-timeline-validation.json', JSON.stringify(report, null, 2));
    await Promise.all([...liveApps].map(app => app.close()));
    await closeSafely(browser);
  }
})();
