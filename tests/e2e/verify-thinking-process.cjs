// Production WASM with isolated history and streaming fixtures.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');

const thought = '先确认消息工作区的布局，再检查思考内容的层级。\n\n## 核对步骤\n\n- 保留已有草稿和工具记录。\n- 思考内容使用次级文字，正文继续清晰展示。\n\n```text\n' + 'fixture/path/'.repeat(80) + '\n```';
const answer = '消息工作区已整理，可以继续工作。';
const hidden = 'REDACTED-THINKING-FIXTURE';
const draft = '保留消息工作区草稿，不发送。';
const output = path.resolve(process.env.WORK_THINKING_REPORT || 'artifacts/thinking-process');

(async () => {
  assert(process.env.WORK_UI_URL, 'Serve a formal WASM bundle with run-static-ui.cjs');
  fs.mkdirSync(output, { recursive: true });
  const fixture = await startFixture();
  fixture.setMessages([
    { entryId: 'thinking-user', role: 'user', content: '核对思考过程的展示。' },
    { entryId: 'thinking-history', role: 'assistant', content: [
      { type: 'thinking', thinking: thought, signature: 'OPAQUE-THINKING-SIGNATURE' },
      { type: 'thinking', thinking: hidden, redacted: true },
      { type: 'text', text: answer },
    ] },
  ]);
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
  page.setDefaultTimeout(12000);
  const errors = [], results = [];
  page.on('pageerror', error => errors.push(error.message));
  const pass = message => { results.push(message); console.log('PASS', message); };
  try {
    await page.addInitScript(address => {
      localStorage.setItem('rpi-work-nodes-v1', JSON.stringify({ version: 1, recent: [address], selected: address }));
      localStorage.setItem('rpi-work-settings-v1', JSON.stringify({ version: 1, appearance: {
        theme: 'codex', mode: 'system', density: 'standard', toolRendering: 'simple', customThemes: [],
      } }));
    }, fixture.url);
    await page.route('https://api.github.com/**', route => route.fulfill({ status: 404, body: '{}' }));
    await page.goto(process.env.WORK_UI_URL);
    await page.locator('[data-session-id=dark] .session-item').first().click();
    const row = page.locator('[data-history-id=thinking-history]');
    const thinking = row.locator('.message-thinking');
    const summary = thinking.locator('summary');
    const body = thinking.locator('.message-thinking-body');
    await thinking.waitFor();
    await page.locator('.composer textarea').fill(draft);
    assert.equal(await thinking.count(), 1);
    assert.equal(await thinking.getAttribute('open'), null);
    assert.equal(await body.isVisible(), false);
    assert.equal(await row.locator('.agent-message .conversation-markdown').innerText(), answer);
    assert(!(await page.content()).includes(hidden));
    assert(!(await page.content()).includes('OPAQUE-THINKING-SIGNATURE'));
    pass('history thinking is collapsed, public text stays separate and redacted payloads remain absent');

    await summary.click();
    await body.waitFor({ state: 'visible' });
    await page.screenshot({ path: path.join(output, 'expanded-initial.png'), animations: 'disabled' });
    await thinking.screenshot({ path: path.join(output, 'thinking-expanded.png'), animations: 'disabled' });
    assert.equal(await summary.locator('.message-thinking-chevron').count(), 1, 'the disclosure has a visible arrow');
    assert.equal(await body.locator('h2').innerText(), '核对步骤');
    assert.equal(await body.locator('li').count(), 2);
    for (const mode of ['light', 'dark']) {
      await page.emulateMedia({ colorScheme: mode });
      await page.waitForFunction(mode => document.documentElement.dataset.mode === mode, mode);
      for (const [width, height] of [[1440, 1000], [768, 768], [390, 844], [320, 420]]) {
        await page.setViewportSize({ width, height });
        await summary.scrollIntoViewIfNeeded();
        const metrics = await thinking.evaluate(element => {
          const style = getComputedStyle(element);
          const summary = element.querySelector('summary'), body = element.querySelector('.message-thinking-body');
          const paragraph = body.querySelector('p'), markdown = body.querySelector('.conversation-markdown');
          const code = body.querySelector('pre');
          return {
            border: style.borderTopWidth, background: style.backgroundColor,
            summaryHeight: summary.getBoundingClientRect().height,
            summaryWidth: summary.getBoundingClientRect().width,
            labelColor: getComputedStyle(summary).color,
            bodyColor: getComputedStyle(body).color,
            proseColor: getComputedStyle(paragraph).color,
            font: parseFloat(getComputedStyle(markdown).fontSize),
            bodyHeight: body.getBoundingClientRect().height,
            codeScrolls: code.scrollWidth > code.clientWidth,
            width: innerWidth, documentWidth: document.documentElement.scrollWidth,
            contentWidth: element.clientWidth, scrollWidth: element.scrollWidth,
          };
        });
        assert.equal(metrics.border, '0px');
        assert.equal(metrics.background, 'rgba(0, 0, 0, 0)');
        assert(metrics.summaryHeight >= 44 && metrics.summaryWidth >= 44);
        assert.equal(metrics.proseColor, metrics.bodyColor, 'Markdown inherits the secondary thought color');
        assert(metrics.font >= 14, 'thought text remains readable');
        assert(metrics.bodyHeight <= Math.min(360, height * 0.45) + 1);
        assert(metrics.codeScrolls, 'long code scrolls inside the thought body');
        assert(metrics.documentWidth <= width + 1);
        assert(metrics.scrollWidth <= metrics.contentWidth + 1);
        await summary.focus();
        await page.keyboard.press('Space');
        await body.waitFor({ state: 'hidden' });
        assert.equal(await summary.evaluate(element => document.activeElement === element), true);
        await page.keyboard.press('Enter');
        await body.waitFor({ state: 'visible' });
        assert.equal(await page.locator('.composer textarea').inputValue(), draft);
        await page.screenshot({ path: path.join(output, `${mode}-${width}.png`), animations: 'disabled' });
      }
      pass(`${mode} thinking uses a compact disclosure, secondary text and bounded scrolling at 320–1440px`);
    }

    fixture.emit({ type: 'agent_start', runId: 'thinking-stream-run' });
    const live = { entryId: 'thinking-live', role: 'assistant', content: [{ type: 'thinking', thinking: '开始核对流式样式。' }] };
    fixture.emit({ type: 'message_start', message: live });
    const streamed = page.locator('[data-message-row]').filter({ hasText: '开始核对流式样式。' }).locator('.message-thinking').first();
    await streamed.waitFor();
    assert.equal(await streamed.getAttribute('open'), null);
    await streamed.locator('summary').click();
    fixture.emit({ type: 'message_update', message: { ...live, content: [{ type: 'thinking', thinking: '开始核对流式样式。\n\n新增一段内容，保持展开。' }] } });
    await streamed.getByText('新增一段内容，保持展开。', { exact: true }).waitFor();
    assert.notEqual(await streamed.getAttribute('open'), null);
    await streamed.locator('summary').click();
    fixture.emit({ type: 'message_update', message: { ...live, content: [{ type: 'thinking', thinking: '开始核对流式样式。\n\n新增一段内容，保持展开。\n\n再次更新。' }] } });
    await page.waitForFunction(() => [...document.querySelectorAll('.message-thinking-body')].some(element => element.textContent.includes('再次更新。')));
    assert.equal(await streamed.getAttribute('open'), null);
    fixture.emit({ type: 'message_end', message: live });
    fixture.emit({ type: 'agent_end', runId: 'thinking-stream-run' });
    fixture.emit({ type: 'agent_settled', runId: 'thinking-stream-run', outcome: 'completed' });
    assert.equal(await page.locator('.composer textarea').inputValue(), draft);
    pass('streamed updates use the same disclosure and preserve the user’s expanded or collapsed choice');
    assert.deepEqual(errors, []);
    const writes = fixture.requests.filter(request => request.method === 'POST' && [
      'prompt', 'abort', 'queue_message', 'set_model', 'set_thinking_level', 'set_api_key',
    ].includes(request.body?.type));
    assert.deepEqual(writes, []);
    fs.writeFileSync(path.join(output, 'validation.json'), JSON.stringify({ results, errors }, null, 2));
  } finally {
    await browser.close();
    await fixture.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
