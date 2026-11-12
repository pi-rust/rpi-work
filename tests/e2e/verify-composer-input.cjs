// Formal WASM + isolated HTTP/SSE. Synthetic IME checks are not physical-keyboard acceptance.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
async function until(test) {
  for (let i = 0; i < 160; i++) { if (test()) return; await delay(25); }
  throw new Error('Expected fixture state did not arrive');
}
(async () => {
  fs.mkdirSync('artifacts', { recursive: true });
  const fixture = await startFixture(0);
  fixture.setMessages(Array.from({ length: 70 }, (_, i) => ({
    entryId: `input-probe-${i}`, role: i % 2 ? 'assistant' : 'user',
    content: i % 2 ? `## 内容 ${i}\n\n${'用于复现长对话中的输入延迟。 **测试文字** `source.rs`\n\n'.repeat(12)}` : `输入回归 ${i}`,
  })));
  const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
  const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
  const errors = [], results = [], measurements = [];
  // Installed before App boots; production pages do not define this object.
  await page.addInitScript(() => {
    window.__rpiAppRenderProbe = { enabled: true, ready: false, appRenders: 0 };
  });
  const appRenders = () => page.evaluate(() => window.__rpiAppRenderProbe.appRenders);
  const sendButton = () => page.getByRole('button', { name: '发送消息', exact: true });
  page.setDefaultTimeout(10000); page.on('pageerror', error => errors.push(error.message));
  const field = () => page.locator('.composer textarea');
  const session = id => page.locator(`[data-session-id="${id}"] .session-item`);
  const commands = () => fixture.requests.filter(r => ['prompt', 'queue_message'].includes(r.body.type));
  const pass = text => { results.push(text); console.log('PASS', text); };
  const value = async text => {
    await page.waitForFunction(expected => document.querySelector('.composer textarea')?.value === expected, text);
  };
  const guardReady = (installed = true) => page.waitForFunction(expected => {
    const field = document.querySelector('.composer textarea');
    return field && !field.disabled && field.dataset.imeReady === String(expected) && (!expected || window.__rpiComposerInputs?.has(field.id));
  }, installed);
  const installCounters = () => page.evaluate(() => {
    if (!window.__inputCounters) {
      window.__inputCounters = { writes: 0, longTasks: [], inputLatencyMs: [] };
      // Event-to-next-animation-frame samples are observational, not a timing SLA.
      document.addEventListener('input', event => {
        if (!event.target.matches('.composer textarea')) return;
        const started = performance.now();
        requestAnimationFrame(() => window.__inputCounters.inputLatencyMs.push(performance.now() - started));
      }, true);
      const original = Storage.prototype.setItem;
      Storage.prototype.setItem = function(key, text) {
        if (key === 'rpi-work-drafts-v1') window.__inputCounters.writes++;
        return original.call(this, key, text);
      };
      new PerformanceObserver(list => window.__inputCounters.longTasks.push(...list.getEntries().map(e => e.duration))).observe({ type: 'longtask', buffered: false });
    }
    window.__inputCounters.writes = 0; window.__inputCounters.longTasks = []; window.__inputCounters.inputLatencyMs = [];
  });
  const reconnect = async (guardInstalled = true) => {
    await page.setViewportSize({ width: 1280, height: 900 });
    await page.getByRole('button', { name: '打开设置', exact: true }).click();
    await page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '工作协同', exact: true }).click();
    await page.getByRole('button', { name: '连接其他节点', exact: true }).click();
    await page.getByLabel('节点地址', { exact: true }).fill(fixture.url);
    await page.getByRole('button', { name: '连接节点', exact: true }).click();
    await page.locator('.server-details [role=status]').filter({ hasText: '在线' }).waitFor();
    await page.getByRole('button', { name: '返回会话', exact: true }).click();
    if (!await session('dark').isVisible()) await page.locator('.project-row').filter({ hasText: 'rpi-work' }).click();
    await session('dark').click(); await guardReady(guardInstalled);
    await page.setViewportSize({ width: 390, height: 844 });
  };
  try {
    await page.route('https://api.github.com/**', route => route.fulfill({ status: 404, body: '{}' }));
    await page.goto(process.env.WORK_UI_URL || 'http://127.0.0.1:14180/');
    await reconnect();
    await page.waitForFunction(() => window.__rpiAppRenderProbe.ready);
    await field().fill('');
    await page.waitForFunction(() => [...document.querySelectorAll('.composer button')].some(e => e.getAttribute('aria-label') === '发送消息' && e.disabled));
    assert.equal(await sendButton().isDisabled(), true, 'empty draft disables send');
    await delay(250);
    const emptyRenders = await appRenders();
    await field().fill('baseline ');
    await page.waitForFunction(() => [...document.querySelectorAll('.composer button')].some(e => e.getAttribute('aria-label') === '发送消息' && !e.disabled));
    await page.waitForFunction(before => window.__rpiAppRenderProbe.appRenders > before, emptyRenders);
    assert.equal(await sendButton().isEnabled(), true, 'nonempty draft enables send');
    await delay(250); await installCounters();
    const typed = 'abcdefghijklmnopqrstuvwxyz1234567890';
    const rendersBefore = await appRenders();
    const started = Date.now(); await field().pressSequentially(typed, { delay: 0 });
    const elapsedMs = Date.now() - started; await delay(300);
    const renderDelta = await appRenders() - rendersBefore;
    const counters = await page.evaluate(() => window.__inputCounters);
    measurements.push({ scenario: '70 Markdown messages, 390px, 36 sequential nonempty characters', elapsedMs, appRenderDelta: renderDelta, ...counters });
    // Counts executions of App itself, including VDOM work with no DOM changes.
    // A small fixed allowance tolerates unrelated asynchronous updates, not one
    // render per character. Positive control above prevents a silent dead probe.
    assert(renderDelta <= 2, `App must not recompute per character: ${renderDelta} renders / ${typed.length} characters`);
    assert(counters.inputLatencyMs.length > 0, 'input latency samples must be recorded');
    assert.equal(await field().inputValue(), `baseline ${typed}`);
    assert(counters.writes <= 4, `draft writes must be batched, not per character: ${JSON.stringify(counters)}`);
    assert.equal(await page.locator('.message-turn').count(), 70);
    pass(`long history typing retains all characters with ${renderDelta} App renders and ${counters.writes} batched storage writes (${elapsedMs}ms in this environment)`);
    await field().fill('');
    await page.waitForFunction(() => [...document.querySelectorAll('.composer button')].some(e => e.getAttribute('aria-label') === '发送消息' && e.disabled));
    assert.equal(await sendButton().isDisabled(), true, 'clearing draft disables send again');
    pass('empty/nonempty/empty button semantics update; opt-in App execution probe has a positive control');

    // Selection indices are UTF-16, just like textarea APIs and keyboard editing.
    await field().fill('前面😀后面');
    await field().evaluate(e => { e.focus(); e.setSelectionRange(4, 4); });
    await page.keyboard.insertText('插入'); await delay(80);
    assert.equal(await field().inputValue(), '前面😀插入后面');
    assert.equal(await field().evaluate(e => e.selectionStart), 6);
    await page.keyboard.press('Backspace'); await delay(80);
    assert.equal(await field().inputValue(), '前面😀插后面');
    assert.equal(await field().evaluate(e => e.selectionStart), 5);
    await field().press('Control+z'); await delay(80);
    assert.equal(await field().inputValue(), '前面😀插入后面');
    pass('Chinese/emoji mid-text insertion, deletion, undo and caret survive reactive updates');

    const beforeIme = commands().length;
    await field().fill('候选词确认不能发送');
    await field().evaluate(e => {
      e.dispatchEvent(new CompositionEvent('compositionstart', { data: '', bubbles: true }));
      e.value = '候选词确认不能发送中文';
      e.dispatchEvent(new InputEvent('input', { data: '中文', inputType: 'insertCompositionText', isComposing: true, bubbles: true }));
      e.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', code: 'Enter', isComposing: false, bubbles: true, cancelable: true }));
      e.dispatchEvent(new CompositionEvent('compositionend', { data: '中文', bubbles: true }));
      e.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', code: 'Enter', keyCode: 229, which: 229, isComposing: false, bubbles: true, cancelable: true }));
      e.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', code: 'Enter', isComposing: true, bubbles: true, cancelable: true }));
    });
    await delay(200); assert.equal(commands().length, beforeIme);
    assert.equal(await field().inputValue(), '候选词确认不能发送中文');
    await field().press('Shift+Enter'); await delay(80);
    assert.equal(await field().inputValue(), '候选词确认不能发送中文\n');
    assert.equal(commands().length, beforeIme);
    pass('composition-active Enter, composing Enter and post-composition keyCode229 do not send; Shift+Enter inserts newline');

    // Native Chromium composition plus a late snapshot delivered to the real
    // WASM handler models delayed edits, not a physical system IME acceptance test.
    const cdp = await page.context().newCDPSession(page);
    await field().fill(''); await delay(80); await field().focus();
    await field().evaluate(e => {
      const descriptor = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value');
      const proof = window.__imeLateProof = { active: false, hold: true, snapshots: [], setters: [] };
      Object.defineProperty(e, 'value', { configurable: true, get() { return descriptor.get.call(this); }, set(text) {
        proof.setters.push({ active: proof.active, text }); descriptor.set.call(this, text);
      } });
      proof.start = () => { proof.active = true; };
      proof.end = () => { proof.active = false; };
      proof.capture = event => {
        if (proof.hold && !event.replayedSnapshot) {
          proof.snapshots.push({ text: e.value, data: event.data }); event.stopImmediatePropagation();
        }
      };
      e.addEventListener('compositionstart', proof.start);
      e.addEventListener('compositionend', proof.end);
      e.addEventListener('input', proof.capture, true);
    });
    try {
      for (const text of ['ni', 'nihao']) await cdp.send('Input.imeSetComposition', { text, selectionStart: text.length, selectionEnd: text.length });
      await field().evaluate(e => {
        const proof = window.__imeLateProof, old = proof.snapshots[0];
        const descriptor = Object.getOwnPropertyDescriptor(e, 'value');
        Object.defineProperty(e, 'value', { ...descriptor, get() { return old.text; } });
        try {
          const event = new InputEvent('input', { bubbles: true, data: old.data, inputType: 'insertCompositionText', isComposing: true });
          event.replayedSnapshot = true; e.dispatchEvent(event);
        } finally { Object.defineProperty(e, 'value', descriptor); }
      });
      await delay(100);
      assert.equal(await field().inputValue(), 'nihao', 'a late input snapshot must not rewrite the active composition');
      assert.equal(await page.evaluate(() => window.__imeLateProof.setters.filter(entry => entry.active).length), 0);
      await page.evaluate(() => { window.__imeLateProof.hold = false; });
      await cdp.send('Input.insertText', { text: '你好' });
      await value('你好');
      assert.equal(commands().length, beforeIme, 'composition must never submit a message');
    } finally {
      await field().evaluate(e => {
        const proof = window.__imeLateProof;
        e.removeEventListener('compositionstart', proof.start); e.removeEventListener('compositionend', proof.end);
        e.removeEventListener('input', proof.capture, true); delete e.value;
      });
      await cdp.detach();
    }
    pass('native Chromium composition survives a delayed input snapshot without active value writes or pinyin residue');

    // Flush a real input in the same JS turn as pagehide, before the 180ms writer.
    const scope = await page.locator('.transcript').getAttribute('data-session-scope');
    await field().evaluate((e, scope) => {
      e.value = '退出前最后一字🦀'; e.dispatchEvent(new Event('input', { bubbles: true }));
      window.dispatchEvent(new Event('pagehide'));
      window.__inputFlushProof = JSON.parse(localStorage.getItem('rpi-work-drafts-v1'))[scope].text;
    }, scope);
    assert.equal(await page.evaluate(() => window.__inputFlushProof), '退出前最后一字🦀');
    await page.reload(); await reconnect(); await value('退出前最后一字🦀');
    pass('pagehide before debounce preserves the final edit and reload restores the scope-keyed draft');

    await page.setViewportSize({ width: 1280, height: 900 });
    await session('architecture').click(); await guardReady(); await field().fill('另一个会话的独立草稿');
    await session('dark').click(); await guardReady(); await value('退出前最后一字🦀');
    assert.equal(await page.evaluate(() => window.__rpiComposerInputs.size), 1, 'unmounted IME listeners are disposed');
    await session('architecture').click(); await value('另一个会话的独立草稿');
    await session('dark').click(); await value('退出前最后一字🦀');
    pass('session switches restore independent drafts and clean the old field IME guard');

    // Programmatic quotations must still synchronize the DOM-owned input.
    await page.getByLabel('选择本地附件，可多选').setInputFiles({ name: 'input-quote.txt', mimeType: 'text/plain', buffer: Buffer.from('引用内容不会丢字') });
    await page.getByRole('button', { name: '预览 input-quote.txt', exact: true }).click();
    await page.getByRole('button', { name: '引用文本到草稿', exact: true }).click();
    await page.waitForFunction(() => document.querySelector('.composer textarea')?.value.includes('引用内容不会丢字'));
    await field().fill('组合前草稿');
    await field().evaluate(e => e.dispatchEvent(new CompositionEvent('compositionstart', { bubbles: true })));
    for (let quote = 0; quote < 2; quote++) {
      await page.getByRole('button', { name: '预览 input-quote.txt', exact: true }).evaluate(e => e.click());
      await page.getByRole('button', { name: '引用文本到草稿', exact: true }).evaluate(e => e.click());
      await delay(100);
      assert.equal(await field().inputValue(), quote === 0 ? '组合前草稿' : '组合前草稿中', 'external quotes must wait for compositionend');
      if (quote === 0) await field().evaluate(e => {
        e.value = '组合前草稿中';
        e.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertCompositionText', isComposing: true }));
      });
    }
    await field().evaluate(e => {
      e.value = '组合前草稿中你好';
      e.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertCompositionText', isComposing: true }));
      e.dispatchEvent(new CompositionEvent('compositionend', { bubbles: true, data: '你好' }));
      e.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertText' }));
    });
    await page.waitForFunction(() => {
      const text = document.querySelector('.composer textarea').value;
      return text.startsWith('组合前草稿中你好') && text.split('引用内容不会丢字').length === 3;
    });
    await page.getByRole('button', { name: '移除 input-quote.txt', exact: true }).click();
    pass('external quotes synchronize normally, defer during composition and merge multiple additions without overwriting new input');

    // Acknowledgments may clear only their own revision, never new typing.
    fixture.controls.promptDelayMs = 300;
    await field().fill('正常回车发送完整中文😀');
    const promptBefore = commands().length; await field().press('Enter');
    await until(() => commands().length === promptBefore + 1);
    assert.equal(commands().at(-1).body.message, '正常回车发送完整中文😀');
    await field().fill('确认返回前的新草稿');
    await field().evaluate(e => e.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', code: 'Enter', keyCode: 229, bubbles: true, cancelable: true })));
    await delay(450);
    assert.equal(commands().length, promptBefore + 1, 'IME cannot create a pending Enter replay during submission');
    assert.equal(await field().inputValue(), '确认返回前的新草稿');
    pass('normal Enter sends the complete latest value; delayed acknowledgment preserves a newer draft revision');

    fixture.emit({ type: 'agent_start', runId: 'input-stream' });
    await page.getByRole('button', { name: '停止运行', exact: true }).waitFor();
    fixture.controls.queueDelayMs = 300;
    await field().fill('组合前待投递');
    const beforeComposingClear = commands().length;
    await field().press('Enter'); await until(() => commands().length === beforeComposingClear + 1);
    await field().evaluate(e => {
      e.dispatchEvent(new CompositionEvent('compositionstart', { bubbles: true }));
      // Browser composition is ahead of its input snapshot when acceptance clears.
      e.value = '组合前待投递你好';
    });
    await delay(450);
    assert.equal(await field().inputValue(), '组合前待投递你好', 'acceptance must not clear an active composition');
    await field().evaluate(e => {
      e.dispatchEvent(new CompositionEvent('compositionend', { bubbles: true, data: '你好' }));
      e.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertText' }));
    });
    await delay(100);
    assert.equal(await field().inputValue(), '组合前待投递你好', 'deferred clear must not overwrite newly committed input');
    assert.equal(commands().length, beforeComposingClear + 1);
    fixture.controls.queueDelayMs = 0;
    pass('acceptance clearing defers during composition and yields to a newer committed draft');
    await page.setViewportSize({ width: 390, height: 844 });
    await field().fill('流式输出中编辑：');
    let n = 0;
    const timer = setInterval(() => fixture.emit({ type: 'message_update', message: { entryId: 'input-stream-answer', role: 'assistant' }, eventType: 'text_delta', delta: `增量${n++} ` }), 20);
    try { await field().pressSequentially('abcdef1234567890', { delay: 3 }); } finally { clearInterval(timer); }
    await delay(100); assert.equal(await field().inputValue(), '流式输出中编辑：abcdef1234567890');
    const beforeQueue = commands().length;
    await field().evaluate(e => e.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', code: 'Enter', keyCode: 229, bubbles: true, cancelable: true })));
    await delay(100); assert.equal(commands().length, beforeQueue, 'IME cannot enqueue either');
    await field().press('Enter'); await until(() => commands().length === beforeQueue + 1);
    assert.equal(commands().at(-1).body.type, 'queue_message');
    assert.equal(commands().at(-1).body.message, '流式输出中编辑：abcdef1234567890');
    await value('');
    await page.waitForFunction(() => !/正在投递消息|消息已被节点接受并加入/.test(document.querySelector('.composer-foot [role=status]')?.textContent || ''));

    fixture.controls.failQueue = true;
    await field().fill('入队失败保留草稿'); await field().press('Enter');
    await page.locator('.composer-foot [role=status]').filter({ hasText: '投递失败：' }).waitFor();
    assert.equal(await field().inputValue(), '入队失败保留草稿');
    fixture.controls.failQueue = false; fixture.controls.uncertainQueue = true;
    await field().fill('未确认入队保留草稿'); await field().press('Enter');
    await page.locator('.composer-foot [role=status]').filter({ hasText: '节点未确认消息已入队' }).waitFor();
    assert.equal(await field().inputValue(), '未确认入队保留草稿');
    fixture.controls.uncertainQueue = false;
    await field().fill('重试成功不显示入队提示'); await field().press('Enter'); await value('');
    await page.waitForFunction(() => !/正在投递消息|消息已被节点接受并加入|节点未确认消息已入队|投递失败/.test(document.querySelector('.composer-foot [role=status]')?.textContent || ''));
    pass('queue acceptance is silent, pending text clears, and rejection/uncertain acceptance retain visible errors and drafts');
    fixture.emit({ type: 'agent_settled', runId: 'input-stream', outcome: 'completed' });
    pass('streaming output preserves edits; IME confirmation does not queue and explicit Enter queues the complete text');

    // Many other drafts do not become synchronous per-keystroke storage writes.
    await field().fill('保留其他草稿');
    await page.waitForFunction(scope => JSON.parse(localStorage.getItem('rpi-work-drafts-v1'))[scope]?.text === '保留其他草稿', scope);
    // Seed at the next document's boot, after the previous Rust persistence task
    // is gone. Editing disk under a live writer races even after exit flush is
    // disposed, and a fixed sleep does not prove the debounce task has finished.
    await page.addInitScript(() => {
      if (sessionStorage.getItem('rpi-input-seed-many-once') !== 'pending') return;
      sessionStorage.removeItem('rpi-input-seed-many-once');
      const store = JSON.parse(localStorage.getItem('rpi-work-drafts-v1'));
      for (let i = 0; i < 100; i++) store[`http://127.0.0.1:1/::session::stored-${i}`] = { text: '不相关草稿'.repeat(400), revision: 1 };
      localStorage.setItem('rpi-work-drafts-v1', JSON.stringify(store));
    });
    await page.evaluate(() => {
      window.__rpiDraftFlush.dispose();
      sessionStorage.setItem('rpi-input-seed-many-once', 'pending');
    });
    await page.reload(); await reconnect(); await value('保留其他草稿');
    await field().focus(); await field().press('Control+End');
    await installCounters(); await field().pressSequentially('12345678901234567890', { delay: 0 });
    assert.equal(await field().inputValue(), '保留其他草稿12345678901234567890');
    await page.waitForFunction(scope => JSON.parse(localStorage.getItem('rpi-work-drafts-v1'))[scope]?.text === '保留其他草稿12345678901234567890', scope);
    const many = await page.evaluate(() => ({ writes: window.__inputCounters.writes, count: Object.keys(JSON.parse(localStorage.getItem('rpi-work-drafts-v1'))).length }));
    assert(many.writes <= 4, JSON.stringify(many)); assert(many.count >= 102, JSON.stringify(many));
    measurements.push({ scenario: '100 unrelated stored drafts, 20 characters', ...many });
    await page.locator('.composer-main').screenshot({ path: 'artifacts/composer-input-mobile.png', animations: 'disabled' });
    pass('many persisted drafts remain intact with batched writes');

    const validStore = await page.evaluate(() => localStorage.getItem('rpi-work-drafts-v1'));
    await page.evaluate(() => {
      window.__inputOriginalSetItem = Storage.prototype.setItem;
      Storage.prototype.setItem = function(key, text) {
        if (key === 'rpi-work-drafts-v1') throw new Error('fixture denies storage');
        return window.__inputOriginalSetItem.call(this, key, text);
      };
    });
    await field().fill('存储失败也不能丢输入');
    await page.locator('.composer-foot [role=status]').filter({ hasText: '浏览器拒绝保存本地草稿' }).waitFor();
    assert.equal(await field().inputValue(), '存储失败也不能丢输入');
    await page.evaluate(() => { Storage.prototype.setItem = window.__inputOriginalSetItem; });
    pass('storage rejection reports a visible persistence error while retaining the in-memory input');

    await page.evaluate(() => {
      window.__rpiDraftFlush.dispose();
      localStorage.setItem('rpi-work-drafts-v1', '{broken fixture draft');
    });
    await page.reload(); await reconnect();
    await field().fill('损坏存储时继续编辑'); await delay(300);
    assert.equal(await page.evaluate(() => localStorage.getItem('rpi-work-drafts-v1')), '{broken fixture draft');
    assert.equal(await page.evaluate(() => Boolean(window.__rpiDraftFlush)), false);
    await page.evaluate(() => window.dispatchEvent(new Event('pagehide')));
    assert.equal(await page.evaluate(() => localStorage.getItem('rpi-work-drafts-v1')), '{broken fixture draft');
    assert.equal(await field().inputValue(), '损坏存储时继续编辑');
    await page.evaluate(saved => localStorage.setItem('rpi-work-drafts-v1', saved), validStore);
    assert.deepEqual(errors, []);
    pass('malformed storage stays recoverable: editing and pagehide never overwrite it; no application errors');

    // Simulate a missing mount target across the native/eval handshake. Keyboard
    // submission must fail closed; typing and the explicit send button still work.
    await page.addInitScript(() => {
      const original = Document.prototype.getElementById;
      Document.prototype.getElementById = function(id) {
        return String(id).startsWith('rpi-composer-') ? null : original.call(this, id);
      };
    });
    await page.reload(); await reconnect(false);
    await page.getByRole('status').filter({ hasText: '输入法保护未就绪' }).waitFor();
    await field().fill('保护失败时保留输入');
    const noGuardBefore = commands().length;
    await field().press('Enter'); await delay(80);
    assert.equal(commands().length, noGuardBefore);
    assert.equal(await field().inputValue(), '保护失败时保留输入\n');
    await page.getByRole('button', { name: '发送消息', exact: true }).click();
    await until(() => commands().length === noGuardBefore + 1);
    assert.equal(commands().at(-1).body.message, '保护失败时保留输入\n');
    assert.deepEqual(errors, []);
    pass('failed IME installation is visible and cannot send by Enter; explicit button preserves normal submission');
  } catch (error) {
    console.error('INPUT STATE', await page.evaluate(() => {
      const e = document.querySelector('.composer textarea');
      return { value: e?.value, initial: e?.defaultValue, content: e?.textContent, id: e?.id, scope: document.querySelector('.transcript')?.dataset.sessionScope, drafts: localStorage.getItem('rpi-work-drafts-v1'), ready: e?.dataset.imeReady };
    }));
    await page.screenshot({ path: 'artifacts/composer-input-failure.png', animations: 'disabled' }); throw error;
  } finally {
    fs.writeFileSync('artifacts/composer-input-validation.json', JSON.stringify({ results, measurements, errors, baseline: { characters: 36, storageWrites: 37, elapsedMs: 1474, ime229UnexpectedPrompts: 1 }, limitations: 'Synthetic browser IME tests; not physical Android keyboard verification. Timing is local debug WASM, not a device benchmark.' }, null, 2));
    await browser.close(); await fixture.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
