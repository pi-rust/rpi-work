// Presentation-only regression: formal WASM + local HTTP/SSE fixture; never submit a prompt.
// Run after the web UI is built: node tests/e2e/verify-message-readability.cjs
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');

const url = process.env.WORK_UI_URL || 'http://127.0.0.1:14180/';
const draft = '阅读排版回归草稿：保留，不发送，不执行。';
const prose = [
  '## 舒适的消息阅读 / Comfortable reading',
  '这是第一段较长的中文说明。消息应该保留自然的呼吸空间，让读者轻松区分正文、列表和工具记录，而不是挤在狭窄的一列。 Comfortable typography makes a long bilingual response easier to scan without losing the conversation context.',
  '第二段继续解释阅读节奏。头像和“助手”标签放在正文上方，不应占用手机屏幕宝贵的横向空间。 Paragraph spacing should remain generous on desktop and on a narrow phone, while the text measure stays comfortable rather than stretching across the entire workspace.',
  '第三段用于观察连续段落之间的真实几何距离。更大的主题字号也需要得到尊重，不要为了默认样式而缩小用户选择。 Preserve larger theme settings, readable line spacing, and a separate compact visual rhythm for tool calls.',
  '- 中文列表：清楚分隔每一项，并允许长句自然换行。\n- English list: keep the reading measure bounded and the composer draft unchanged.\n- 工具记录保持紧凑，不需要重复的头像或角色标题。',
  '### 示例代码 / Code sample',
  '```js\nconst message = "fixture only; no model call";\nconsole.log(message);\n```',
].join('\n\n');
const tools = [
  { id: 'readability-read', name: 'read', arguments: { path: 'fixture-only/readability.md' }, output: 'Local fixture read result; no file was read.' },
  { id: 'readability-search', name: 'search', arguments: { query: 'comfortable typography', path: 'fixture-only' }, output: 'Local fixture search result; no search was executed.' },
];
const lineBreakCases = [
  { id: 'newline-lf', content: '第一行\n第二行', visible: '第一行\n第二行' },
  { id: 'newline-crlf', content: '第一行\r\n第二行', visible: '第一行\n第二行' },
  { id: 'newline-format', content: '**第一行**\n[第二行](https://example.invalid/read)', visible: '第一行\n第二行' },
  { id: 'newline-parts', content: [{ type: 'text', text: '第一行\n' }, { type: 'text', text: '第二行' }], visible: '第一行\n第二行' },
];
const messages = [
  { entryId: 'readability-user', role: 'user', content: '请保留头像，并让中英文长回复舒适易读。 Keep tool records compact and preserve this fixture-only conversation. '.repeat(5) },
  { entryId: 'readability-first', role: 'assistant', content: prose },
  ...tools.flatMap((tool, index) => [
    { entryId: `readability-tools-${index}`, role: 'assistant', content: [{ type: 'toolCall', id: tool.id, name: tool.name, arguments: tool.arguments }] },
    { entryId: `readability-result-${index}`, role: 'toolResult', toolCallId: tool.id, toolName: tool.name, content: [{ type: 'text', text: tool.output }] },
  ]),
  { entryId: 'readability-second', role: 'assistant', content: '检查已完成，这是第二条正常回复。\n\nThe next public reply keeps its heading, avatar and comfortable paragraph spacing.' },
  { entryId: 'readability-mixed', role: 'assistant', content: [
    { type: 'text', text: '这条混合回复先给出说明，再展示紧凑工具。 Text and the inline tool need their own breathing space.' },
    { type: 'toolCall', id: 'readability-mixed-read', name: 'read', arguments: { path: 'fixture-only/mixed.md' } },
  ] },
  { entryId: 'readability-mixed-result', role: 'toolResult', toolCallId: 'readability-mixed-read', toolName: 'read', content: [{ type: 'text', text: 'Linked mixed fixture result; nothing executed.' }] },
  ...lineBreakCases.map(item => ({ entryId: item.id, role: 'assistant', content: item.content })),
  { entryId: 'newline-paragraphs', role: 'assistant', content: '第一段\n\n第二段' },
  { entryId: 'newline-code', role: 'assistant', content: '```text\n第一行\n\n第三行\n```' },
  { entryId: 'newline-user', role: 'user', content: '第一行\n第二行' },
];

(async () => {
  fs.mkdirSync('artifacts', { recursive: true });
  const results = [], errors = [], geometry = [], screenshots = [], blockedRequests = [], blockedWrites = [];
  let fixture, browser, page, fatal;
  const row = id => page.locator(`[data-history-id="${id}"]`);
  const card = id => page.locator(`.inline-call[data-tool-id="${id}"]`);
  const pass = text => { results.push(text); console.log('PASS', text); };
  const writes = requests => requests.filter(request => !['GET', 'HEAD', 'OPTIONS'].includes(request.method)
    && !(request.method === 'POST' && (/\/sessions\/[^/]+\/open$/.test(request.path)
      || (/\/commands$/.test(request.path) && String(request.body?.type).startsWith('get_')))));
  const assertSafe = async () => {
    assert.equal(await page.locator('.composer textarea').inputValue(), draft, 'draft must not change');
    assert.deepEqual(writes(fixture.requests), [], 'only read-only fixture commands and session open are allowed');
    assert.deepEqual(blockedWrites, [], 'no attempted prompt/configuration/credential or other writes');
    assert.deepEqual(blockedRequests, [], 'no attempted real node or external network access');
    assert.deepEqual(errors, [], 'no application JavaScript errors');
  };
  const assertSingleBreak = async (paragraph, expected) => {
    assert.equal(await paragraph.count(), 1, 'a single authored newline is not a new paragraph');
    assert.equal(await paragraph.locator('br').count(), 1, 'exactly one explicit break per source newline');
    const layout = await paragraph.evaluate(element => {
      const style = getComputedStyle(element);
      return { height: element.getBoundingClientRect().height, line: parseFloat(style.lineHeight), whiteSpace: style.whiteSpace };
    });
    assert.equal(layout.whiteSpace, 'normal', 'CSS must not preserve a second newline alongside br');
    assert(Math.abs(layout.height - 2 * layout.line) <= 1, `two source lines occupy exactly two lines, not an extra blank line: ${JSON.stringify(layout)}`);
    assert.equal(await paragraph.innerText(), expected);
  };
  const checkLineBreaks = async () => {
    for (const item of lineBreakCases) await assertSingleBreak(row(item.id).locator('.conversation-markdown > p'), item.visible);
    const paragraphs = row('newline-paragraphs').locator('.conversation-markdown > p');
    assert.equal(await paragraphs.count(), 2, 'authored blank line still separates paragraphs');
    assert.equal(await paragraphs.locator('br').count(), 0, 'paragraph gap is not duplicated with br');
    assert.equal(await row('newline-code').locator('pre code').textContent(), '第一行\n\n第三行', 'code whitespace is verbatim');
    assert.equal(await row('newline-user').locator('.conversation-user-text').innerText(), '第一行\n第二行', 'plain user line breaks remain intact');
    await assertSingleBreak(row('newline-live').locator('.conversation-markdown > p'), '流式第一行\n流式第二行');
  };
  // Scope every transcript query by fixture ID: snapshot SSE also seeds unrelated replies/tools.
  const measure = () => page.evaluate(() => {
    const box = element => {
      const r = element.getBoundingClientRect();
      return { top: r.top, bottom: r.bottom, left: r.left, right: r.right, width: r.width, height: r.height };
    };
    const row = id => document.querySelector(`[data-history-id="${id}"]`);
    const article = id => row(id).querySelector('article');
    const call = id => document.querySelector(`.inline-call[data-tool-id="${id}"]`);
    const replies = ['readability-first', 'readability-second', 'readability-mixed'].map(id => {
      const a = article(id), body = a.querySelector('.message-content'), heading = a.querySelector('.assistant-message-heading');
      const prose = [...body.querySelectorAll('.message-text-part .conversation-markdown')];
      return { id, article: box(a), body: box(body), heading: heading && box(heading),
        marginTop: parseFloat(getComputedStyle(a).marginTop), marginBottom: parseFloat(getComputedStyle(a).marginBottom),
        prose: prose.map(element => ({ ...box(element), scroll: element.scrollWidth, client: element.clientWidth })),
        text: prose.flatMap(element => [...element.querySelectorAll('p, li')].map(node => {
          const style = getComputedStyle(node);
          return { font: parseFloat(style.fontSize), line: parseFloat(style.lineHeight) };
        })),
      };
    });
    const first = article('readability-first'), markdown = first.querySelector('.conversation-markdown');
    const paragraphs = [...markdown.querySelectorAll(':scope > p')];
    const list = markdown.querySelector(':scope > ul');
    const user = article('readability-user').querySelector('.message-content'), style = getComputedStyle(user);
    const pure = tools => tools.map(id => {
      const a = row(id).querySelector('.timeline-tool-region');
      return { ...box(a), topMargin: parseFloat(getComputedStyle(a).marginTop), bottomMargin: parseFloat(getComputedStyle(a).marginBottom) };
    });
    return { viewport: innerWidth, pageWidth: document.documentElement.scrollWidth,
      cap: getComputedStyle(document.querySelector('.app-shell')).getPropertyValue('--conversation-content-max').trim(),
      row: box(row('readability-first')), composer: box(document.querySelector('.composer-main > .composer')),
      replies, pure: pure(['readability-tools-0', 'readability-tools-1']),
      user: { ...box(user), padding: [style.paddingTop, style.paddingRight, style.paddingBottom, style.paddingLeft].map(parseFloat) },
      paragraphCount: paragraphs.length,
      paragraphGaps: paragraphs.slice(1).map((node, i) => box(node).top - box(paragraphs[i]).bottom),
      listGap: box(list).top - box(paragraphs.at(-1)).bottom,
      toolGap: box(call('readability-search')).top - box(call('readability-read')).bottom,
      beforeTools: box(call('readability-read')).top - box(first).bottom,
      afterTools: box(article('readability-second')).top - box(call('readability-search')).bottom,
      inlineGap: box(call('readability-mixed-read')).top - box(article('readability-mixed')).bottom,
      summaries: ['readability-read', 'readability-search', 'readability-mixed-read'].map(id => box(call(id).querySelector(':scope > summary'))),
    };
  });
  const check = async (mode, width, scale = 1) => {
    const g = await measure();
    geometry.push({ mode, width, scale, ...g });
    const bounded = (b, label) => assert(b.width > 0 && b.left >= -1 && b.right <= width + 1, `${label} fits viewport: ${JSON.stringify(b)}`);
    assert(g.pageWidth <= width + 1, 'no page-level horizontal overflow');
    assert.equal(g.cap, '1000px', 'global row/composer cap remains 1000px, not prose measure');
    for (const [label, b] of [['row', g.row], ['composer', g.composer]]) {
      bounded(b, label); assert(b.width <= 1001, `${label} retains the global cap`);
    }
    for (const reply of g.replies) {
      const a = row(reply.id).locator('article'), heading = a.locator('.assistant-message-heading');
      assert.equal(await heading.count(), 1, `${reply.id}: one role heading`);
      assert.equal((await heading.textContent()).trim(), 'rpi', 'assistant identity matches the Web message heading');
      assert.equal(await a.locator('.assistant-avatar').count(), 1);
      const avatar = heading.locator('.assistant-avatar');
      assert.equal(await avatar.count(), 1, 'avatar belongs to heading, never a side column');
      assert.equal(await avatar.getAttribute('aria-hidden'), 'true');
      assert.equal(await avatar.locator('svg').getAttribute('data-icon'), 'code');
      const mark = await avatar.evaluate(element => {
        const style = getComputedStyle(element), icon = element.querySelector('svg');
        return { width: element.getBoundingClientRect().width, height: element.getBoundingClientRect().height,
          radius: parseFloat(style.borderRadius), icon: icon.getBoundingClientRect().width, stroke: getComputedStyle(icon).strokeWidth };
      });
      assert.deepEqual(mark, { width: 21, height: 21, radius: 6, icon: 19, stroke: '2px' }, 'compact Web code mark, not a circular sparkle');
      assert(await avatar.isVisible());
      assert(Math.abs(reply.body.top - reply.heading.bottom - 8) <= 1, 'Web heading/body gap is 8px');
      assert(reply.heading.bottom <= reply.body.top + 1, 'role heading is above the entire body');
      assert(Math.abs(reply.heading.left - reply.body.left) <= 1, 'body uses full width below heading');
      assert(Math.abs(reply.marginTop - 24) <= 1, 'each separate public reply has one 24px leading gap owner');
      assert.equal(reply.marginBottom, 0, 'no trailing margin that doubles the next public region gap');
      assert(reply.prose.length && reply.text.length, 'public prose is present');
      for (const b of reply.prose) {
        bounded(b, 'prose'); assert(b.width <= 761 && b.scroll <= b.client + 1, 'only prose has a bounded 760px reading measure');
      }
      for (const text of reply.text) {
        assert(text.font >= 15, `public prose font >=15px: ${text.font}`);
        assert(text.line / text.font >= 1.75 - 0.01, `public prose line-height >=1.75: ${JSON.stringify(text)}`);
      }
    }
    assert.equal(await row('readability-user').locator('.assistant-avatar, .assistant-message-heading').count(), 0);
    assert.equal(await row('readability-user').locator('.user-message-heading').textContent(), '你');
    const userHeading = await row('readability-user').evaluate(element => {
      const heading = element.querySelector('.user-message-heading').getBoundingClientRect(), body = element.querySelector('.message-content').getBoundingClientRect();
      return { leftDelta: heading.left - body.left, rightDelta: heading.right - body.right, gap: body.top - heading.bottom,
        align: getComputedStyle(element.querySelector('.user-message-heading')).justifyContent };
    });
    assert(Math.abs(userHeading.leftDelta) <= 1 && Math.abs(userHeading.rightDelta) <= 1, 'user title shares the bubble width');
    assert.equal(userHeading.align, 'flex-end');
    assert(Math.abs(userHeading.gap - 8) <= 1, 'user heading/body gap matches assistant');
    bounded(g.user, 'user bubble'); assert(g.user.width <= 651, 'user bubble max 650px');
    if (scale === 1) g.user.padding.forEach((value, i) => assert(Math.abs(value - [11, 15, 11, 15][i]) <= 1, 'Web user bubble padding 11px 15px'));
    for (const [tag, size] of [['h2', 21], ['h3', 18]]) {
      const font = await row('readability-first').locator(`.conversation-markdown ${tag}`).evaluate(element => parseFloat(getComputedStyle(element).fontSize));
      assert(Math.abs(font - size * scale) <= 0.1, `Web ${tag} size respects type scale`);
    }
    assert.equal(g.paragraphCount, 3, 'three long fixture paragraphs rendered');
    for (const gap of [...g.paragraphGaps, g.listGap]) assert(gap >= 12 && gap <= 32 * scale, `reasonable paragraph/list spacing, not cramped: ${gap}`);
    for (const id of ['readability-tools-0', 'readability-tools-1']) {
      assert.equal(await row(id).locator(':scope > .timeline-tool-region').count(), 1);
      assert.equal(await row(id).locator('article').count(), 0, 'pure tools are not wrapped in a prose message');
      assert.equal(await row(id).locator('.assistant-avatar, .assistant-message-heading').count(), 0, 'pure tools have no heading/avatar');
    }
    assert(Math.abs(g.toolGap) <= 1, `neighboring tool messages stay continuous: ${g.toolGap}`);
    for (const b of g.pure) assert(Math.abs(b.topMargin) <= 1 && Math.abs(b.bottomMargin) <= 1, 'tool regions have no turn margins');
    assert(Math.abs(g.beforeTools - 20) <= 1 && Math.abs(g.afterTools - 24) <= 1, '20px before tools; a separate public reply owns 24px after tools even across empty results');
    assert(Math.abs(g.inlineGap - 20) <= 1, 'mixed text/tool boundary has one 20px owner');
    for (const b of g.summaries) {
      bounded(b, 'tool summary');
      assert(b.height <= (width >= 768 ? 44 : 72) * scale, 'tools remain compact rather than inheriting prose spacing');
    }
    assert.equal(await row('readability-first').locator('.conversation-markdown h2').count(), 1);
    assert.equal(await row('readability-first').locator('.conversation-markdown h3').count(), 1);
    assert.equal(await row('readability-first').locator('.conversation-code pre code').count(), 1);
    await checkLineBreaks();
    await assertSafe();
    pass(`${mode} ${width}px scale ${scale}: readable prose, single line breaks, heading/avatar, compact tools, bounded layout and retained draft`);
    return g.replies[0].text[0].font;
  };

  try {
    fixture = await startFixture(0);
    fixture.setMessages(messages);
    browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
    const context = await browser.newContext({ viewport: { width: 1280, height: 900 } });
    page = await context.newPage();
    page.setDefaultTimeout(10000);
    page.on('pageerror', error => errors.push(error.message));
    const previewOrigin = new URL(url).origin, fixtureOrigin = new URL(fixture.url).origin;
    await context.route('**/*', async route => {
      const request = route.request(), target = new URL(request.url());
      if (!['http:', 'https:'].includes(target.protocol)) return route.continue();
      if (target.origin === 'https://api.github.com') return route.fulfill({ status: 404, contentType: 'application/json', body: '{}' });
      let body = {};
      try { body = request.postDataJSON() || {}; } catch {}
      const record = { method: request.method(), path: target.pathname, body };
      if (target.origin === fixtureOrigin) {
        if (!writes([record]).length) return route.continue();
        blockedWrites.push(record);
      } else if (target.origin === previewOrigin && target.pathname === '/api/v1/agent-backends' && record.method === 'GET') {
        // The initial default-node catalog probe is read-only. Send it only to
        // our isolated fixture, never to an installed or external node.
        return route.fulfill({ response: await route.fetch({ url: new URL(target.pathname, fixture.url).href }) });
      } else if (target.origin === previewOrigin && !target.pathname.startsWith('/api/') && ['GET', 'HEAD', 'OPTIONS'].includes(record.method)) {
        return route.continue(); // Static preview assets only; never a real node API.
      } else {
        blockedRequests.push({ origin: target.origin, ...record });
        if (!['GET', 'HEAD', 'OPTIONS'].includes(record.method)) blockedWrites.push(record);
      }
      return route.abort('blockedbyclient');
    });
    await page.addInitScript(() => localStorage.setItem('rpi-work-appearance-v1', JSON.stringify({ theme: 'codex', mode: 'light', density: 'standard', toolRendering: 'simple', customThemes: [] })));
    await page.goto(url);
    await page.getByRole('button', { name: '打开设置', exact: true }).click();
    await page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '执行节点', exact: true }).click();
    await page.getByRole('button', { name: '连接其他节点', exact: true }).click();
    await page.getByLabel('节点地址', { exact: true }).fill(fixture.url);
    await page.getByRole('button', { name: '连接节点', exact: true }).click();
    await page.locator('.server-details [role=status]').filter({ hasText: '在线' }).waitFor();
    await page.getByRole('button', { name: '返回会话', exact: true }).click();
    await page.locator('.project-row').filter({ hasText: 'rpi-work' }).click();
    await page.locator('[data-session-id="dark"] .session-item').click();
    await card('readability-mixed-read').waitFor();
    await page.locator('.composer textarea').fill(draft);
    await page.waitForFunction(() => Boolean(window.__rpiScrollFollow?.instances?.size));
    fixture.emit({ type: 'agent_start', runId: 'newline-live-run' });
    fixture.emit({ type: 'message_start', message: { entryId: 'newline-live', role: 'assistant', content: [] } });
    fixture.emit({ type: 'message_update', message: { entryId: 'newline-live', role: 'assistant' }, eventType: 'text_delta', delta: '流式第一行\n' });
    await page.locator('.agent-message .conversation-markdown > p').filter({ hasText: '流式第一行' }).waitFor();
    fixture.emit({ type: 'message_update', message: { entryId: 'newline-live', role: 'assistant' }, eventType: 'text_delta', delta: '流式第二行' });
    const streamed = page.locator('.agent-message .conversation-markdown > p').filter({ hasText: '流式第二行' });
    await streamed.waitFor();
    await assertSingleBreak(streamed, '流式第一行\n流式第二行');
    fixture.emit({ type: 'message_end', message: { entryId: 'newline-live', role: 'assistant', content: '流式第一行\n流式第二行' } });
    await row('newline-live').waitFor();
    fixture.emit({ type: 'agent_end', runId: 'newline-live-run' });
    fixture.emit({ type: 'agent_settled', runId: 'newline-live-run', outcome: 'completed' });
    // Confirm linked results through the fixture cards, not seeded SSE tools.
    for (const [id, output] of [...tools.map(tool => [tool.id, tool.output]), ['readability-mixed-read', 'Linked mixed fixture result; nothing executed.']]) {
      assert.equal(await card(id).locator('.call-output').textContent(), output);
      assert.equal(await card(id).evaluate(element => element.open), false, 'successful fixture tools start collapsed');
    }
    for (const mode of ['light', 'dark']) {
      await page.setViewportSize({ width: 1280, height: 900 });
      await page.getByRole('button', { name: '外观与主题', exact: true }).click();
      const appearance = page.getByRole('dialog', { name: '外观与主题', exact: true });
      await appearance.getByLabel('明暗模式', { exact: true }).selectOption(mode);
      await appearance.getByRole('button', { name: '保存', exact: true }).click();
      for (const width of [1280, 390, 320]) {
        await page.setViewportSize({ width, height: 900 });
        await row('readability-first').locator('.assistant-message-heading').evaluate(element => element.scrollIntoView({ block: 'start', behavior: 'instant' }));
        const path = `artifacts/message-readability-${mode}-${width}.png`;
        await page.screenshot({ path, animations: 'disabled' }); screenshots.push(path);
        const baseFont = await check(mode, width);
        if (width === 320) {
          await page.locator('.app-shell').evaluate(element => element.style.setProperty('--type-scale', '1.5'));
          try {
            const largerFont = await check(mode, width, 1.5);
            assert(largerFont >= baseFont * 1.4, 'type-scale 1.5 must grow prose, not freeze at the minimum');
          } finally {
            await page.locator('.app-shell').evaluate(element => element.style.removeProperty('--type-scale'));
          }
        }
      }
    }
    await assertSafe();
  } catch (error) {
    fatal = error.stack || String(error);
    throw error;
  } finally {
    // Keep safety evidence even if startup, a geometry assertion, or screenshot fails.
    fs.writeFileSync('artifacts/message-readability-validation.json', JSON.stringify({ results, errors, fatal, geometry, screenshots, blockedRequests, blockedWrites, unexpectedWrites: fixture ? writes(fixture.requests) : [], requests: fixture?.requests }, null, 2));
    try { if (browser) await browser.close(); } finally { if (fixture) await fixture.close(); }
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
