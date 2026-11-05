// Formal WASM + local HTTP/SSE only. Never calls a model or a remote renderer.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');
const flow = 'flowchart TD\n  A[收到消息] --> B{需要工具？}\n  B -->|是| C[执行工具]\n  B -->|否| D[回复用户]\n  C --> D';
const sequence = 'sequenceDiagram\n  participant U as 用户\n  participant A as 助手\n  U->>A: 请画流程图\n  A-->>U: Mermaid 图表';
const fenced = source => `\`\`\`mermaid\n${source}\n\`\`\``;
(async () => {
  fs.mkdirSync('artifacts', { recursive: true });
  const fixture = await startFixture(0);
  const emit = (id, text) => fixture.emit({ type: 'message_end', message: { entryId: id, role: 'assistant', content: [{ type: 'text', text }] } });
  // History is present before connection, not just injected into the DOM.
  fixture.setMessages([{ entryId: 'mermaid-history', role: 'assistant', content: [{ type: 'text', text: `# 消息中的流程图\n${fenced(flow)}\n\n下面是普通代码：\n\`\`\`rust\nfn main() {}\n\`\`\`` }] }]);
  const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
  const context = await browser.newContext({ viewport: { width: 1440, height: 1000 }, permissions: ['clipboard-read', 'clipboard-write'] });
  const page = await context.newPage(), results = [], errors = [], external = [];
  page.setDefaultTimeout(20000); page.on('pageerror', error => errors.push(error.message));
  page.on('request', request => {
    const url = request.url();
    // Existing application update checks are unrelated to Mermaid rendering.
    if (url === 'https://api.github.com/repos/pi-rust/rpi-work/releases/latest') return;
    if (/^https?:/.test(url) && !/^https?:\/\/(127\.0\.0\.1|localhost)(:|\/)/.test(url)) external.push(url);
  });
  const pass = text => { results.push(text); console.log('PASS', text); };
  const diagrams = () => page.locator('#conversation-panel .conversation-mermaid');
  const diagram = source => diagrams().filter({ has: page.locator('pre code').filter({ hasText: source }) }).first();
  const ready = locator => locator.locator('.mermaid-image:not([hidden]) img').waitFor();
  const imageSource = locator => locator.locator('img').getAttribute('src');
  const shot = name => page.screenshot({ path: `artifacts/mermaid-${name}.png`, animations: 'disabled' });
  const bottom = () => page.waitForFunction(() => { const e = document.querySelector('#conversation-panel'); return e.scrollHeight - e.clientHeight - e.scrollTop < 3; });
  const mode = async value => {
    await page.getByRole('button', { name: '外观与主题', exact: true }).click();
    const dialog = page.getByRole('dialog', { name: '外观与主题', exact: true });
    await dialog.getByLabel('明暗模式', { exact: true }).selectOption(value);
    await dialog.getByRole('button', { name: '保存', exact: true }).click();
    await dialog.waitFor({ state: 'hidden' });
  };
  try {
    await page.goto(process.env.WORK_UI_URL || 'http://127.0.0.1:14182/app/');
    await page.getByRole('button', { name: '打开设置', exact: true }).click();
    await page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '执行节点', exact: true }).click();
    await page.getByRole('button', { name: '连接其他节点', exact: true }).click();
    await page.getByLabel('节点地址', { exact: true }).fill(fixture.url);
    await page.getByRole('button', { name: '连接节点', exact: true }).click();
    await page.getByRole('status').filter({ hasText: '在线' }).first().waitFor();
    await page.getByRole('button', { name: '返回会话', exact: true }).click();
    await page.locator('.project-row').filter({ hasText: 'rpi-work' }).click();
    await page.locator('.sidebar-sessions').getByRole('button', { name: '为工作台添加深色模式', exact: true }).click();
    await ready(diagram(flow));
    const svg = await diagram(flow).locator('img').evaluate(async image => (await fetch(image.src)).text());
    fs.writeFileSync('artifacts/mermaid-flow.svg', svg);
    assert.match(svg, /收到消息/); assert.match(svg, /执行工具/); assert.match(svg, /<svg/);
    assert.equal(await page.locator('#conversation-panel .conversation-code').filter({ hasText: 'fn main() {}' }).count(), 1);
    assert.equal(await diagrams().locator('svg,script,iframe').count(), 0, 'Mermaid SVG must never become active host markup');
    assert.equal(await page.locator('iframe[title="隔离的 Mermaid 渲染器"]').getAttribute('sandbox'), 'allow-scripts');
    await bottom(); await shot('flow-light');
    pass('history Mermaid renders a real Chinese SVG image offline; ordinary code is unchanged');

    await diagram(flow).getByRole('button', { name: '复制 Mermaid 源码' }).click();
    await diagram(flow).getByText('已复制', { exact: true }).waitFor();
    assert.equal((await page.evaluate(() => navigator.clipboard.readText())).replace(/\r\n/g, '\n'), flow);
    await diagram(flow).locator('summary').focus(); await page.keyboard.press('Enter');
    assert(await diagram(flow).locator('details').evaluate(e => e.open));
    const light = await imageSource(diagram(flow));
    await mode('dark'); await ready(diagram(flow));
    await page.waitForFunction(old => document.querySelector('.conversation-mermaid img').src !== old, light);
    assert(await diagram(flow).locator('details').evaluate(e => e.open), 'theme changes preserve the source expansion');
    const darkSvg = await diagram(flow).locator('img').evaluate(async image => (await fetch(image.src)).text());
    assert.notEqual(svg, darkSvg);
    await diagram(flow).locator('summary').click(); await bottom(); await shot('flow-dark');
    await mode('light'); await ready(diagram(flow));
    pass('source copying, keyboard expansion and light/dark re-render preserve message state');

    // Role locators exclude a closed dialog before its queued close cleanup;
    // DOM detachment, not invisibility, is the lifecycle boundary under test.
    const viewer = page.locator('dialog.mermaid-viewer');
    const openViewer = async locator => {
      const button = locator.getByRole('button', { name: '放大查看 Mermaid 图表', exact: true });
      await page.waitForFunction(id => {
        const host = document.getElementById(id), image = host?.querySelector('.mermaid-image img');
        return host && !host.querySelector('button[aria-label="放大查看 Mermaid 图表"]').disabled && image?.complete && image.naturalWidth > 0;
      }, await locator.getAttribute('id'));
      await button.focus(); await page.keyboard.press('Enter');
      await viewer.waitFor();
      await page.waitForFunction(() => {
        const image = document.querySelector('.mermaid-viewer img');
        return image?.complete && image.naturalWidth > 0 && parseFloat(image.style.width) > 0;
      });
      assert(await viewer.evaluate(e => e.matches(':modal')), 'viewer uses native modal semantics');
      assert.equal(await page.getByRole('dialog', { name: 'Mermaid 图表', exact: true }).count(), 1);
      return button;
    };
    const opener = await openViewer(diagram(flow));
    assert.equal(await viewer.locator('img').getAttribute('src'), await imageSource(diagram(flow)), 'viewer reuses the inert preview Blob');
    assert(await viewer.getByRole('button', { name: '关闭图表查看器' }).evaluate(e => e === document.activeElement));
    for (let i = 0; i < 9; i++) {
      await page.keyboard.press('Tab');
      assert(await viewer.evaluate(e => e.contains(document.activeElement)), 'native modal contains keyboard focus');
    }
    const fittedWidth = await viewer.locator('img').evaluate(e => e.getBoundingClientRect().width);
    await viewer.getByRole('button', { name: '放大图表', exact: true }).click();
    assert(await viewer.locator('img').evaluate(e => e.getBoundingClientRect().width) > fittedWidth);
    await viewer.getByRole('button', { name: '缩小图表', exact: true }).click();
    assert(Math.abs(await viewer.locator('img').evaluate(e => e.getBoundingClientRect().width) - fittedWidth) < 1);
    await viewer.getByRole('button', { name: '适应窗口', exact: true }).click();
    assert(Math.abs(await viewer.locator('img').evaluate(e => e.getBoundingClientRect().width) - fittedWidth) < 1);
    assert.equal(await viewer.locator('svg,script,iframe,object,embed').count(), 0);
    const oldViewerUrl = await viewer.locator('img').getAttribute('src');
    // The native modal intentionally makes the appearance UI inert. Change the
    // observed mode attribute directly to exercise an external theme update.
    const originalMode = await page.evaluate(() => {
      const root = document.documentElement, previous = root.getAttribute('data-mode');
      root.dataset.mode = previous === 'dark' ? 'light' : 'dark'; return previous;
    });
    await page.waitForFunction(old => {
      const image = document.querySelector('.mermaid-viewer img');
      return image?.src !== old && image?.complete && image.naturalWidth > 0;
    }, oldViewerUrl);
    await ready(diagram(flow));
    assert.equal(await viewer.locator('img').getAttribute('src'), await imageSource(diagram(flow)));
    assert(await page.evaluate(async url => { try { await fetch(url); return false; } catch (_) { return true; } }, oldViewerUrl), 'replaced viewer Blob is revoked');
    const themedViewerUrl = await viewer.locator('img').getAttribute('src');
    await page.evaluate(previous => {
      if (previous === null) document.documentElement.removeAttribute('data-mode');
      else document.documentElement.setAttribute('data-mode', previous);
    }, originalMode);
    await page.waitForFunction(old => {
      const image = document.querySelector('.mermaid-viewer img');
      return image?.src !== old && image?.complete && image.naturalWidth > 0;
    }, themedViewerUrl);
    await ready(diagram(flow));
    await page.keyboard.press('Escape'); await viewer.waitFor({ state: 'detached' });
    assert(await opener.evaluate(e => e === document.activeElement), 'Escape restores focus to the opener');
    await openViewer(diagram(flow));
    await viewer.getByRole('button', { name: '关闭图表查看器' }).click();
    await viewer.waitFor({ state: 'detached' });
    assert(await opener.evaluate(e => e === document.activeElement), 'close restores focus to the opener');
    await openViewer(diagram(flow));
    await viewer.evaluate((dialog, id) => {
      dialog.close();
      // Reopen in the same task, before the native queued close event can run.
      window.__rpiMermaid.openViewer(id);
    }, await diagram(flow).getAttribute('id'));
    await viewer.waitFor();
    assert.equal(await page.locator('.mermaid-viewer').count(), 1);
    assert(await viewer.evaluate(e => e.matches(':modal')));
    await page.keyboard.press('Escape'); await viewer.waitFor({ state: 'detached' });

    await openViewer(diagram(flow));
    const pendingUrl = await imageSource(diagram(flow));
    const pendingMode = await page.evaluate(() => {
      const hold = event => {
        const frame = document.querySelector('iframe[title="隔离的 Mermaid 渲染器"]');
        if (frame && event.source === frame.contentWindow && event.data?.type === 'rendered') {
          event.stopImmediatePropagation();
          window.__viewerHeldRender = { source: event.source, data: event.data };
          removeEventListener('message', hold, true);
        }
      };
      addEventListener('message', hold, true);
      const root = document.documentElement, previous = root.getAttribute('data-mode');
      root.dataset.mode = previous === 'dark' ? 'light' : 'dark';
      return previous;
    });
    await page.waitForFunction(() => !!window.__viewerHeldRender);
    // Native close is queued. Inspect the opener in that same close task,
    // rather than predicting its disabled state before another render can finish.
    await viewer.evaluate((dialog, id) => {
      dialog.addEventListener('close', () => {
        const host = document.getElementById(id);
        const button = host.querySelector('button[aria-label="放大查看 Mermaid 图表"]');
        window.__viewerExpectedFocus = button.disabled ? host : button;
      }, { capture: true, once: true });
    }, await diagram(flow).getAttribute('id'));
    await page.keyboard.press('Escape'); await viewer.waitFor({ state: 'detached' });
    assert(await page.evaluate(() => document.activeElement === window.__viewerExpectedFocus), 'closing during a pending render restores usable focus at close time');
    await page.evaluate(() => delete window.__viewerExpectedFocus);
    await page.evaluate(() => {
      const held = window.__viewerHeldRender; delete window.__viewerHeldRender;
      dispatchEvent(new MessageEvent('message', held));
    });
    await page.waitForFunction(old => document.querySelector('.conversation-mermaid img').src !== old, pendingUrl);
    await ready(diagram(flow));
    const completedUrl = await imageSource(diagram(flow));
    await page.evaluate(previous => {
      if (previous === null) document.documentElement.removeAttribute('data-mode');
      else document.documentElement.setAttribute('data-mode', previous);
    }, pendingMode);
    await page.waitForFunction(old => document.querySelector('.conversation-mermaid img').src !== old, completedUrl);
    await ready(diagram(flow));
    pass('native viewer supports keyboard focus, zoom/fit, rapid reopen, pending-render close fallback and live theme Blob replacement');

    await page.locator('.composer textarea').fill('图表渲染保留草稿');
    await page.locator('#conversation-panel').hover(); await page.mouse.wheel(0, 100000); await bottom();
    fixture.emit({ type: 'message_update', message: { entryId: 'mermaid-stream', role: 'assistant' }, eventType: 'text_delta', delta: `流式绘制中\n\`\`\`MERMAID\n${sequence}` });
    await diagram(sequence).getByText('等待完整 Mermaid 代码…', { exact: true }).waitFor();
    assert.equal(await diagram(sequence).locator('img').getAttribute('src'), null);
    assert(await diagram(sequence).getByRole('button', { name: '放大查看 Mermaid 图表' }).isDisabled());
    fixture.emit({ type: 'message_update', message: { entryId: 'mermaid-stream', role: 'assistant' }, eventType: 'text_delta', delta: '\n```\n流程说明仍然正常显示。' });
    await ready(diagram(sequence)); await bottom(); await shot('sequence');
    assert.equal(await diagrams().count(), 2);
    assert.equal(await page.locator('iframe[title="隔离的 Mermaid 渲染器"]').count(), 1);
    pass('incomplete streaming fences wait for closure; sequence diagrams share one isolated renderer and follow output');

    for (const [id, source, message] of [
      ['bad', 'flowchart TD\nA -->', 'Mermaid 渲染失败'],
      ['config', '%%{init: {"securityLevel":"loose"}}%%\nflowchart TD\nA-->B', '不支持图表内配置指令'],
      ['frontmatter', '---\nconfig:\n  securityLevel: loose\n---\nflowchart TD\nA-->B', '不支持图表内配置指令'],
      ['large', `flowchart TD\n%% ${'x'.repeat(20001)}`, '超过 20,000 字符']
    ]) {
      emit(`mermaid-${id}`, fenced(source));
      await diagram(source).locator('.mermaid-status').filter({ hasText: message }).waitFor();
      await diagram(source).locator('summary').click();
      assert.equal(await diagram(source).locator('code').innerText(), source);
    }
    emit('mermaid-recovery', fenced('flowchart LR\n恢复 --> 正常'));
    await ready(diagram('恢复 --> 正常'));
    pass('invalid syntax/config/frontmatter/oversize fall back to intact source; the next diagram still renders');

    const hostile = 'flowchart TD\n A["<img src=https://mermaid-invalid.example/steal onerror=parent.__mermaidXss=1>"] --> B["</script><script>parent.__mermaidXss=1</script>"]\n click A "https://mermaid-invalid.example/click"';
    emit('mermaid-hostile', fenced(hostile));
    await diagram(hostile).waitFor();
    await page.waitForFunction(() => {
      const e = [...document.querySelectorAll('.conversation-mermaid')].find(e => e.textContent.includes('mermaid-invalid.example'));
      return e && !e.querySelector('.mermaid-status').textContent.includes('正在绘制');
    });
    assert.equal(await page.evaluate(() => window.__mermaidXss), undefined);
    assert.equal(await diagrams().locator('script,svg,a,iframe').count(), 0);
    assert.deepEqual(external, []);
    pass('HTML/script labels, external images and clickable links cannot execute in the host or request a remote renderer');

    // Hold one real renderer response to exercise disposal while work is pending.
    // Session navigation must cancel it, not block the next diagram for 12 seconds.
    fixture.setMessages([{ entryId: 'mermaid-other-session', role: 'assistant', content: [{ type: 'text', text: fenced('flowchart LR\n切换会话 --> 仍可绘图') }] }], 'architecture');
    const disposedUrl = await imageSource(diagram(flow));
    await page.evaluate(() => {
      window.__mermaidHold = true;
      addEventListener('message', event => {
        const renderer = document.querySelector('iframe[title="隔离的 Mermaid 渲染器"]');
        if (window.__mermaidHold && renderer && event.source === renderer.contentWindow && event.data?.type === 'rendered') {
          event.stopImmediatePropagation(); window.__mermaidHold = false; window.__mermaidHeld = true;
        }
      }, true);
    });
    emit('mermaid-dispose-pending', fenced('flowchart LR\n准备卸载 --> 等待结果'));
    await page.waitForFunction(() => window.__mermaidHeld);
    await openViewer(diagram(flow));
    // Simulate navigation initiated outside the inert page (e.g. host/session event).
    await page.locator('.sidebar-sessions').getByRole('button', { name: '梳理 Agent 架构', exact: true }).evaluate(e => e.click());
    await viewer.waitFor({ state: 'detached' });
    await diagram('切换会话 --> 仍可绘图').locator('.mermaid-image:not([hidden]) img').waitFor({ timeout: 8000 });
    assert.equal(await page.locator('iframe[title="隔离的 Mermaid 渲染器"]').count(), 1);
    assert(await page.evaluate(async url => { try { await fetch(url); return false; } catch (_) { return true; } }, disposedUrl), 'unmounted image URLs are revoked');
    await page.locator('.sidebar-sessions').getByRole('button', { name: '为工作台添加深色模式', exact: true }).click();
    await ready(diagram(flow));
    pass('switching sessions cancels pending renders without a timeout stall, revokes old images and remounts history');

    // A final valid graph is used for mobile layout and async scroll-follow checks.
    const mobile = 'flowchart TD\n开始 --> 判断{处理成功？}\n判断 -->|是| 完成\n判断 -->|否| 重试\n重试 --> 判断';
    await page.locator('#conversation-panel').hover(); await page.mouse.wheel(0, 100000); await bottom();
    emit('mermaid-mobile', fenced(mobile)); await ready(diagram(mobile)); await bottom();
    // Let the preceding wheel gesture expire before measuring passive resize following.
    // Otherwise the follow policy correctly treats its still-active intent window as user reading.
    await page.waitForTimeout(750);
    for (const width of [390, 320]) {
      await page.setViewportSize({ width, height: 844 }); await bottom();
      const bounds = await diagram(mobile).boundingBox();
      assert(bounds.x >= 0 && bounds.x + bounds.width <= width + 1);
      assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
      const copy = await diagram(mobile).getByRole('button', { name: '复制 Mermaid 源码' }).boundingBox();
      assert(copy.height >= 44);
      await shot(`mobile-${width}`);
      const mobileOpener = await openViewer(diagram(mobile));
      const viewerBounds = await viewer.boundingBox();
      assert(viewerBounds.x >= 0 && viewerBounds.x + viewerBounds.width <= width + 1);
      assert(viewerBounds.y >= 0 && viewerBounds.y + viewerBounds.height <= 845);
      assert(await viewer.locator('.mermaid-viewer-viewport').evaluate(e => {
        const image = e.querySelector('img');
        return image.width <= e.clientWidth && image.height <= e.clientHeight;
      }), 'fit keeps the complete diagram in the mobile viewport');
      for (const control of await viewer.getByRole('button').all()) {
        const bounds = await control.boundingBox();
        assert(bounds.width >= 44 && bounds.height >= 44, 'viewer controls have touch-sized targets');
      }
      await shot(`viewer-mobile-${width}`);
      await page.keyboard.press('Escape'); await viewer.waitFor({ state: 'detached' });
      assert(await mobileOpener.evaluate(e => e === document.activeElement));
      // Large focus displacement is a reading position: resume through the real
      // Latest control. A passive displacement inside the old 48px tolerance must
      // instead re-pin; hiding Latest while leaving a ~42px gap is not following.
      const latest = page.getByRole('button', { name: '回到最新消息', exact: true });
      if (await latest.isVisible()) await latest.click();
      await bottom();
      await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
      assert(!(await latest.isVisible()), 'a pinned follower does not offer Latest');

      // The same near-bottom distance produced by real reader input must pause,
      // not be mistaken for native focus restoration and snapped back down.
      await page.locator('#conversation-panel').hover(); await page.mouse.wheel(0, -42);
      await page.waitForFunction(() => {
        const e = document.querySelector('#conversation-panel'), gap = e.scrollHeight - e.clientHeight - e.scrollTop;
        return gap >= 3 && gap <= 48;
      });
      await latest.waitFor({ state: 'visible' });
      await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
      assert(await page.locator('#conversation-panel').evaluate(e => {
        const gap = e.scrollHeight - e.clientHeight - e.scrollTop; return gap >= 3 && gap <= 48;
      }), 'deliberate near-bottom reading remains paused after follow updates');
      await latest.click(); await bottom();
    }
    assert.equal(await page.locator('.composer textarea').inputValue(), '图表渲染保留草稿');
    assert.equal(fixture.requests.filter(r => r.body.type === 'prompt').length, 0);
    assert.deepEqual(errors, []); assert.deepEqual(external, []);
    pass('390/320px remain within viewport, keep touch targets/draft/following, with zero model calls or application JS errors');
  } catch (error) {
    console.error('Viewer diagnostics:', await page.evaluate(() => ({ active: document.activeElement?.outerHTML, dialogs: [...document.querySelectorAll('dialog')].map(e => e.outerHTML.slice(0, 500)) })));
    await shot('failure');
    results.push({ failure: error.message, statuses: await diagrams().locator('.mermaid-status').allTextContents(), scroll: await page.locator('#conversation-panel').evaluate(e => ({ top: e.scrollTop, height: e.scrollHeight, visible: e.clientHeight, records: [...window.__rpiScrollFollow.records] })) });
    throw error;
  } finally {
    fs.writeFileSync('artifacts/mermaid-validation.json', JSON.stringify({ results, errors, external }, null, 2));
    await browser.close(); await fixture.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
