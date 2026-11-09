// Real integrated WASM must be built/served separately. Authoring validation: node --check only.
// No product/fixture edits, injected footer DOM, fake timers, provider calls or setter commands.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');
const UI = process.env.WORK_UI_URL || 'http://127.0.0.1:14180/';
const FILTER = process.env.FILTER; // Optional isolated old-build layout proof, e.g. FILTER=12.
const KEY = 'rpi-work-footer-v1';
const DRAFT = 'Footer settings must preserve this unsent draft 😀';
const MODEL = 'deepseek/deepseek-v4-pro';
// registry.rs: declarations, not guessed legacy metric IDs.
const PLUGINS = ['session-identity', 'session-usage', 'context-usage', 'tool-activity'];
const TITLES = ['会话身份', '会话用量', '上下文用量', '已加载工具活动'];
const DEFAULT_PLUGINS = PLUGINS.filter(id => id !== 'session-identity');
const BASE_ITEMS = ['input-output', 'cache', 'cost', 'context', 'tools'];
const ITEM_LABELS = { identity: '供应商/模型', 'input-output': '累计输入/输出', cache: '缓存命中率', cost: '累计费用', context: '上下文', tools: '已加载工具/失败', speed: '生成速度', 'requests-retries': '请求/重试' };
const defaults = () => ({ version: 1, plugins: PLUGINS.map(id => ({ id, enabled: id !== 'session-identity', hidden_items: [] })) });
const results = [], errors = [], geometry = [];
let browser;
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
async function until(check, label) {
  const deadline = Date.now() + 7000;
  while (Date.now() < deadline) { if (await check()) return; await delay(35); }
  throw new Error(`Timed out: ${label}`);
}
function report() {
  fs.writeFileSync('artifacts/footer-plugins-validation.json', JSON.stringify({ results, errors, geometry,
    assumptions: [
      'Runs against the separately built integrated App/use_footer_preferences provider, RuntimeFooter/FooterHost and SettingsPage footer section; result entries below record actual execution.',
      'FooterSettings and registry/preferences DOM, Chinese labels, IDs and schema are taken from footer_plugins/{mod.rs,registry.rs,preferences.rs,footer_plugins.css}.',
      'Fixture snapshot contains sessionId/messages/events/latestSettlement, NOT runtime state. RuntimeState is read through POST get_state response.data. Tests inspect both actual HTTP responses and modify only the real get_state envelope.',
      'Default cachedTokens 24000 + uncachedTokens 8000 = input 32000; output derives from totalTokens 36800 minus input. Context is 38000/100000. Loaded projection has read-1 and shell-1, not activeTools.length.',
      'Preferences have plugin order and hidden_items, not item ordering. Reorder coverage moves plugins; item coverage hides/shows declared items.',
      'Only corrupt/quota cases install storage init scripts. All ordinary preferences use physical checkbox/button interactions. Drafts use textarea.fill; no sends or setters.',
      'Session and node scope use per-origin/per-session get_state HTTP overrides, not set_model. Same-address late-readback/connection-epoch races belong to verify-composer-model-picker.cjs cases 12/16 and are not claimed here.',
      'Disconnect closes the real fixture WebSocket and blocks subsequent reconnect get_state reads; it does not synthesize new stats. Existing global 30-second health checks are not treated as footer polling.',
      'Plugin provider panic/error/descriptor isolation is covered by registry.rs pure Rust unit tests, not a browser-import/plugin execution API. Escaping is checked through actual runtime identity HTTP text.',
    ], limitations: 'Formal WASM with isolated HTTP/WebSocket fixtures, simulated insets and typography; not a native Android or real-device acceptance test. Each isolated case has a 45-second wall bound; the six screenshot/layout combinations have a 75-second bound. Passed/failed entries and screenshots describe only this run.'
  }, null, 2));
}
async function test(name, configure, verify, boundMs = 45000) {
  if (FILTER && !name.includes(FILTER)) return;
  const started = Date.now(), fixtures = [], commands = [], snapshots = [], states = [], external = [], failures = [];
  let context, page, timer;
  const proof = { name, status: 'FAIL', commands, snapshots, states, external, errors: failures };
  const policy = { overrides: new Map(), disconnected: false };
  const work = async () => {
    const fixture = await startFixture(0); fixtures.push(fixture);
    context = await browser.newContext({ viewport: { width: 1280, height: 900 } });
    page = await context.newPage(); page.setDefaultTimeout(7000);
    page.on('pageerror', e => failures.push(e.message));
    page.on('websocket', socket => {
      const socketUrl = new URL(socket.url());
      const node = fixtures.find(candidate => {
        const nodeUrl = new URL(candidate.url);
        return nodeUrl.hostname === socketUrl.hostname && nodeUrl.port === socketUrl.port;
      });
      if (node && policy.disconnected) socket.close();
    });
    const h = {
      page, context, fixture, fixtures, policy, commands, snapshots, states,
      footer: () => page.locator('footer.runtime-footer.plugin-footer[aria-label="Agent 运行统计"]'),
      item: id => h.footer().locator(`.footer-item[data-item-id="${id}"]`),
      settings: () => page.locator('section.footer-plugin-settings[aria-label="底部状态栏"]'),
      field: () => page.locator('.composer-main textarea'),
      send: () => page.getByRole('button', { name: '发送消息', exact: true }),
      trigger: () => page.getByRole('button', { name: '选择供应商与模型', exact: true }),
      session: id => page.locator(`[data-session-id="${id}"] .session-item`),
      async addNode() { const node = await startFixture(0); fixtures.push(node); return node; },
      override(node, id, patch) { policy.overrides.set(`${new URL(node.url).origin}/${id}`, patch); },
      async ready() {
        await h.send().waitFor();
        await page.waitForFunction(() => { const e = document.querySelector('.composer-main textarea'); return e && !e.disabled && e.dataset.imeReady === 'true'; });
      },
      async chooseSession(id = 'dark') {
        if (!await h.session(id).isVisible()) await page.locator('.project-row').filter({ hasText: 'rpi-work' }).click();
        await h.session(id).click(); await h.ready();
      },
      async connect(node = fixture) {
        await page.getByRole('button', { name: '打开设置', exact: true }).click();
        await page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '执行节点', exact: true }).click();
        if (!await page.getByLabel('节点地址', { exact: true }).isVisible()) await page.getByRole('button', { name: '连接其他节点', exact: true }).click();
        await page.getByLabel('节点地址', { exact: true }).fill(node.url);
        await page.getByRole('button', { name: '连接节点', exact: true }).click();
        await page.locator('.server-details [role=status]').filter({ hasText: '在线' }).waitFor();
        await h.back(); await h.chooseSession();
      },
      async openSettings() {
        await page.getByRole('button', { name: '打开设置', exact: true }).click();
        // General card also says 底部状态栏: scope this selector to navigation.
        await page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '底部状态栏', exact: true }).click();
        await h.settings().getByRole('heading', { name: '底部状态栏', exact: true }).waitFor();
        await until(async () => !await h.settings().getByRole('button', { name: '恢复默认状态栏', exact: true }).isDisabled(), 'local preferences loaded');
      },
      async back() { await page.getByRole('button', { name: '返回会话', exact: true }).click(); },
      check(label) { return h.settings().getByRole('checkbox', { name: label, exact: true }); },
      async reset() { await h.settings().getByRole('button', { name: '恢复默认状态栏', exact: true }).click(); },
      async stored() { return page.evaluate(key => localStorage.getItem(key), KEY); },
      async persisted(expected) { await until(async () => await h.stored() === JSON.stringify(expected), 'exact persisted preferences'); },
      async enableIdentity() {
        await h.openSettings(); await h.check('会话身份 插件').check(); await h.back();
      },
      async text(id, expected) {
        const prefix = `${ITEM_LABELS[id]}：${expected}。`;
        await until(async () => {
          if (await h.item(id).count() !== 1) return false;
          const name = await h.item(id).getAttribute('aria-label');
          return name?.startsWith(prefix) && name.length > prefix.length;
        }, `${id} full accessible value: ${expected}`);
        await h.visual(id, expected);
      },
      async visual(id, full) {
        const value = h.item(id).locator('.footer-value');
        let expected = full;
        if (id === 'input-output') {
          const [, input, output] = full.match(/^会话累计 ↑(.+) ↓(.+)$/);
          assert.deepEqual(await value.locator('span').allTextContents(), [`↑ ${input}`, `↓ ${output}`]);
          const icon = value.locator('svg[data-icon="diff"]');
          assert.equal(await icon.count(), 1);
          const iconSize = await icon.boundingBox();
          if (await h.item(id).isVisible()) assert(iconSize && Math.abs(iconSize.width - 12) <= 0.1 && Math.abs(iconSize.height - 12) <= 0.1, 'diff icon is 12px');
          expected = `↑ ${input}↓ ${output}`;
        } else if (id === 'cache') {
          const amount = full.slice('缓存 '.length), bold = value.locator('b.footer-cache-value');
          assert.equal(await bold.count(), 1); assert.equal(await bold.textContent(), amount);
          if (amount !== '—') {
            const colors = await bold.evaluate(b => ({ number: getComputedStyle(b).color,
              label: getComputedStyle(b.parentElement).color, root: getComputedStyle(b.closest('footer')).color }));
            assert.equal(colors.label, colors.root, 'cache label remains muted');
            assert.notEqual(colors.number, colors.label, 'only the known cache number is highlighted');
            assert(await bold.evaluate(b => {
              const style = getComputedStyle(b), good = style.getPropertyValue('--success').trim();
              const hex = good.match(/^#([\da-f]{6})$/i);
              return hex ? style.color === `rgb(${[0, 2, 4].map(i => parseInt(hex[1].slice(i, i + 2), 16)).join(', ')})` : style.color === good;
            }), 'cache number uses the good/success color');
          } else {
            assert(await bold.evaluate(b => getComputedStyle(b).color === getComputedStyle(b.parentElement).color), 'unknown cache is not highlighted as good');
          }
        } else if (id === 'context') {
          const percent = full === '上下文 —' ? '—' : full.split(' ·')[1];
          expected = `上下文 ${percent}`;
          const ring = value.locator('.context-ring');
          assert.equal(await ring.count(), 1); assert.equal(await ring.getAttribute('aria-hidden'), 'true');
          assert.equal(await ring.getAttribute('data-known'), String(percent !== '—'));
          const used = await ring.evaluate(e => e.style.getPropertyValue('--used').trim());
          if (percent === '—') assert.equal(used, '', 'unknown context never becomes a zero ring');
          else { assert.match(used, /^\d+(?:\.\d+)?%$/); assert.equal(Number.parseFloat(used), Number.parseFloat(percent)); }
          if (await h.item(id).isVisible()) {
            const box = await ring.boundingBox();
            assert(box && Math.abs(box.width - 12) <= 0.1 && Math.abs(box.height - 12) <= 0.1, 'context ring is 12px');
          }
        } else if (id === 'cost') expected = full.replace(/^累计/, '');
        else if (id === 'tools') {
          const [, calls, failures] = full.match(/^已加载工具 (.+) ·失败(.+)$/);
          expected = `${calls} tools${failures !== '—' && failures !== '0' ? ` ·失败${failures}` : ''}`;
        }
        // textContent also checks mobile cost/speed, retained in DOM with display:none.
        if (id === 'input-output') {
          assert.equal((await value.textContent()).replace(/\s/g, ''), expected.replace(/\s/g, ''), `${id} compact visual text`);
        } else assert.equal((await value.textContent()).trim(), expected, `${id} complete visual text`);
      },
      async detail() {
        const visibleUsage = await h.footer().locator('.footer-plugin[data-plugin-id="session-usage"] .footer-item:visible').count();
        const detail = h.footer().locator('.footer-detail');
        assert.equal(await detail.count(), visibleUsage ? 1 : 0, 'detail exists only with visible usage metrics');
        if (visibleUsage) {
          assert.equal(await detail.evaluate(e => e.tagName), 'SPAN');
          assert.equal(await detail.textContent(), '会话累计');
          const wide = (await page.viewportSize()).width > 1200;
          assert.equal(await detail.isVisible(), wide, 'original narrow-screen footer hides the trailing caption');
          assert.equal(await detail.getAttribute('tabindex'), null);
          assert.equal(await detail.getAttribute('role'), null);
          assert.equal(await detail.locator('button,a,input,[tabindex],[role=button],[role=link]').count(), 0);
          if (wide) assert(await detail.evaluate(d => {
            const right = Math.max(...[...d.closest('footer').querySelectorAll('.footer-item')]
              .filter(i => getComputedStyle(i).display !== 'none').map(i => i.getBoundingClientRect().right));
            return d.getBoundingClientRect().left >= right - 1;
          }), 'session total detail is on the right of the metrics');
        }
      },
      async preserved() {
        assert.equal(await h.field().inputValue(), DRAFT);
        assert.equal(await h.trigger().getAttribute('title'), `deepseek / ${MODEL}`);
        assert(await h.send().isEnabled(), 'settings must not poison send readiness');
      },
      async compact(label, { maxHeight = 32, safeInset = false } = {}) {
        const footer = h.footer();
        const resetScroll = async () => {
          await footer.evaluate(e => e.scrollTo({ left: 0, behavior: 'instant' }));
          await until(async () => await footer.evaluate(e => Math.abs(e.scrollLeft) <= 1), 'footer scroll restored to zero');
        };
        await resetScroll();
        const sizes = await footer.evaluate(e => {
          const r = e.getBoundingClientRect(), s = getComputedStyle(e);
          const box = n => { const b = n.getBoundingClientRect(); return { left: b.left, right: b.right, top: b.top, bottom: b.bottom, height: b.height }; };
          return { ...box(e), scroll: e.scrollWidth, client: e.clientWidth, viewportLeft: r.left + e.clientLeft,
            viewportRight: r.left + e.clientLeft + e.clientWidth, viewportWidth: innerWidth, viewportHeight: innerHeight,
            documentWidth: Math.max(document.documentElement.scrollWidth, document.body.scrollWidth),
            flexWrap: s.flexWrap, direction: s.flexDirection, overflowX: s.overflowX,
            fontFamily: s.fontFamily, font: parseFloat(s.fontSize), scale: Number(s.getPropertyValue('--type-scale').trim() || 1),
            scrollbarHeight: parseFloat(getComputedStyle(e, '::-webkit-scrollbar').height),
            items: [...e.querySelectorAll('.footer-item')].map(i => {
              const style = getComputedStyle(i), v = i.querySelector('.footer-value'), vs = getComputedStyle(v);
              const range = document.createRange(); range.selectNodeContents(v);
              const text = range.getBoundingClientRect(), rects = [];
              const walker = document.createTreeWalker(v, NodeFilter.SHOW_TEXT);
              while (walker.nextNode()) {
                if (!walker.currentNode.textContent.trim()) continue;
                const part = document.createRange(); part.selectNodeContents(walker.currentNode);
                rects.push(...[...part.getClientRects()].map(r => ({ top: r.top, bottom: r.bottom })));
              }
              return { id: i.dataset.itemId, ...box(i), font: parseFloat(style.fontSize), whitespace: style.whiteSpace,
                display: style.display, gap: parseFloat(style.columnGap), valueGap: parseFloat(vs.columnGap),
                shrink: style.flexShrink, visible: style.display !== 'none' && style.visibility !== 'hidden',
                value: { ...box(v), whitespace: vs.whiteSpace, shrink: vs.flexShrink, overflow: vs.textOverflow,
                  scroll: v.scrollWidth, client: v.clientWidth, scrollHeight: v.scrollHeight, clientHeight: v.clientHeight,
                  textLeft: text.left, textRight: text.right, textTop: text.top, textBottom: text.bottom,
                  textRects: rects } };
            }), warnings: [...e.querySelectorAll('.footer-warning')].map(box) };
        });
        sizes.hiddenItems = sizes.items.filter(i => !i.visible);
        sizes.items = sizes.items.filter(i => i.visible);
        geometry.push(Object.assign(sizes, { label }));
        // Capture BEFORE any compact assertion: an old wrapped WASM must leave visual proof too.
        await page.screenshot({ path: `artifacts/footer-${label}.png`, animations: 'disabled' });
        try {
          assert(sizes.height > 0 && sizes.height <= maxHeight, `compact footer <=${maxHeight}px: ${JSON.stringify(sizes)}`);
          assert(sizes.left >= -1 && sizes.right <= sizes.viewportWidth + 1, 'footer stays within viewport');
          assert(sizes.documentWidth <= sizes.viewportWidth, 'no document horizontal overflow');
          assert.equal(sizes.flexWrap, 'nowrap'); assert.equal(sizes.direction, 'row'); assert.equal(sizes.overflowX, 'auto');
          assert.match(await footer.getAttribute('class'), /\bfooter-metrics\b/);
          assert.match(sizes.fontFamily, /mono|consolas|courier/i, 'restored monospace metrics');
          assert(Math.abs(sizes.font - (sizes.viewportWidth <= 700 ? 11 : 12) * sizes.scale) <= 0.1, 'desktop 12px/mobile 11px typography scales');
          for (const item of sizes.hiddenItems) {
            assert(sizes.viewportWidth <= 700 && ['cost', 'speed'].includes(item.id), `${item.id}: only legacy narrow-screen metrics are hidden`);
            assert.equal(item.display, 'none');
          }
          for (const id of ['cost', 'speed']) {
            if (await h.item(id).count()) assert.equal(await h.item(id).isVisible(), sizes.viewportWidth > 700, `${id} retains original mobile display:none rule`);
          }
          await h.detail();
          assert(sizes.scrollbarHeight > 0 && sizes.scrollbarHeight <= 3, 'custom horizontal scrollbar is at most 3px');
          assert.equal(await footer.getAttribute('tabindex'), '0', 'only the scroll container is keyboard focusable');
          assert.equal(await footer.locator('[title],[role=tooltip]').count(), 0, 'full values do not depend on tooltips');
          assert.match(await footer.getAttribute('aria-description') || '', /横向|水平|horizontal/i);
          assert(sizes.items.length > 0);
          const line = sizes.items[0];
          for (const item of sizes.items) {
            assert(item.visible && item.height > 0 && item.font >= 11, `${item.id} readable`);
            assert(Math.abs(item.top - line.top) <= 1 && Math.abs(item.bottom - line.bottom) <= 1, `${item.id} shares one row`);
            // Flex items are blockified: inline-flex computes to flex here.
            assert.equal(item.display, 'flex');
            assert.equal(item.gap, sizes.viewportWidth <= 350 ? 1 : 5, `${item.id} internal gap`);
            if (item.id === 'input-output') assert.equal(item.valueGap, sizes.viewportWidth <= 350 ? 3 : 7, 'token value gap');
            assert.equal(item.whitespace, 'nowrap'); assert.equal(item.shrink, '0');
            const v = item.value;
            assert.equal(v.whitespace, 'nowrap'); assert.equal(v.shrink, '0');
            assert.notEqual(v.overflow, 'ellipsis', `${item.id} never truncates text`);
            assert(v.textRects.length > 0 && v.textRects.every(r => Math.abs(r.top - v.textRects[0].top) <= 1 && Math.abs(r.bottom - v.textRects[0].bottom) <= 1), `${item.id} text rects share one visual line`);
            assert(v.scroll <= v.client + 1 && v.scrollHeight <= v.clientHeight + 1, `${item.id} has no internal wrapping/clipping`);
            assert(v.textLeft >= v.left - 1 && v.textRight <= v.right + 1, `${item.id} preserves full text width`);
            assert(v.textTop >= sizes.top - 1 && v.textBottom <= sizes.bottom + 1 && v.top >= sizes.top - 1 && v.bottom <= sizes.bottom + 1, `${item.id} has no vertical clipping`);
          }
          for (const warning of sizes.warnings) {
            assert(Math.abs(warning.top - line.top) <= 1 && Math.abs(warning.bottom - line.bottom) <= 1, 'offline warning stays inline');
          }
          if (safeInset) assert(sizes.top >= 24 && sizes.bottom <= sizes.viewportHeight - 23, 'footer respects safe insets');
          await footer.focus();
          assert(await footer.evaluate(e => document.activeElement === e), 'footer root accepts keyboard focus');
          if (sizes.scroll > sizes.client + 1) {
            await page.keyboard.press('ArrowRight');
            await until(async () => await footer.evaluate(e => e.scrollLeft > 0), 'ArrowRight actually scrolls the focused footer');
            sizes.keyboardScrollLeft = await footer.evaluate(e => e.scrollLeft);
            // Arrow-key scrolling is animated by the browser, not CSS. Wait for
            // its native motion to settle before testing programmatic restoration.
            let last = -1, stableSince = Date.now();
            await until(async () => {
              const left = await footer.evaluate(e => e.scrollLeft);
              if (Math.abs(left - last) > 0.1) { last = left; stableSince = Date.now(); }
              return Date.now() - stableSince >= 200;
            }, 'native keyboard scroll animation settled');
          }
          // Reveal the end of long identity text through the container, not truncation/tooltips.
          const identity = sizes.items.find(i => i.id === 'identity');
          if (identity) {
            // Identity can follow any reordered plugin; reveal its start rather than
            // assuming it occupies the initial viewport.
            const start = Math.min(sizes.scroll - sizes.client, Math.max(0, identity.value.textLeft - sizes.viewportLeft));
            await footer.evaluate((e, left) => e.scrollTo({ left, behavior: 'instant' }), start);
            await until(async () => await footer.evaluate((e, left) => Math.abs(e.scrollLeft - left) <= 1, start), 'identity beginning reached');
            assert(await h.item('identity').locator('.footer-value').evaluate(v => {
              const b = v.getBoundingClientRect(), e = v.closest('footer'), r = e.getBoundingClientRect();
              return b.left >= r.left + e.clientLeft - 1 && b.left < r.left + e.clientLeft + e.clientWidth;
            }), 'identity beginning is reachable in any plugin order');
            const target = Math.min(sizes.scroll - sizes.client, Math.max(0, identity.value.textRight - sizes.viewportRight));
            await footer.evaluate((e, left) => e.scrollTo({ left, behavior: 'instant' }), target);
            await until(async () => await footer.evaluate((e, left) => Math.abs(e.scrollLeft - left) <= 1, target), 'long identity end reached');
            sizes.identityEndScrollLeft = await footer.evaluate(e => e.scrollLeft);
            assert(await h.item('identity').locator('.footer-value').evaluate(v => {
              const range = document.createRange(); range.selectNodeContents(v);
              const text = range.getBoundingClientRect(), e = v.closest('footer'), r = e.getBoundingClientRect();
              return text.right <= r.left + e.clientLeft + e.clientWidth + 1 && text.right > r.left + e.clientLeft;
            }), 'full identity ending is reachable horizontally');
          }
          const last = sizes.items.at(-1);
          const target = Math.min(sizes.scroll - sizes.client, Math.max(0, last.right - sizes.viewportRight));
          await footer.evaluate((e, left) => e.scrollTo({ left, behavior: 'instant' }), target);
          await until(async () => await footer.evaluate((e, left) => Math.abs(e.scrollLeft - left) <= 1, target), 'last metric scroll reached');
          sizes.lastMetricScrollLeft = await footer.evaluate(e => e.scrollLeft);
          assert(await h.item(last.id).evaluate(i => {
            const b = i.getBoundingClientRect(), e = i.closest('footer'), r = e.getBoundingClientRect();
            return b.left >= r.left + e.clientLeft - 1 && b.right <= r.left + e.clientLeft + e.clientWidth + 1;
          }), 'last metric is fully reachable without document scrolling');
        } finally {
          await resetScroll();
          sizes.restoredScrollLeft = await footer.evaluate(e => e.scrollLeft);
        }
        return sizes;
      },
      async readonly() {
        const footer = h.footer(); await footer.waitFor();
        const originalScroll = await footer.evaluate(e => e.scrollLeft);
        assert.equal(await footer.locator('button,a,input,select,textarea,[tabindex],[contenteditable=true],[role=button],[role=link]').count(), 0);
        const items = footer.locator('.footer-item');
        assert(await items.count() > 0);
        for (let i = 0; i < await items.count(); i++) {
          const item = items.nth(i);
          assert.equal(await item.evaluate(e => e.tagName), 'SPAN');
          assert.equal(await item.getAttribute('title'), null, 'no browser-native tooltip');
          assert.match(await item.getAttribute('aria-label'), /。/);
          assert.equal(await item.getAttribute('role'), 'group');
          if (!await item.isVisible()) {
            assert(['cost', 'speed'].includes(await item.getAttribute('data-item-id')), 'only legacy mobile metrics are hidden');
            continue; // Retained DOM semantics/actions were checked above; do not click display:none.
          }
          assert.equal(await footer.getByRole('group', { name: await item.getAttribute('aria-label'), exact: true }).count(), 1);
          await item.click(); await item.dblclick();
          assert.equal(await page.getByRole('dialog').count(), 0, 'footer cannot open any runtime/model/tools dialog');
        }
        await page.mouse.move(0, 0);
        // Double-click probes may select browser text; clear selection for clean screenshots.
        await page.evaluate(() => getSelection()?.removeAllRanges());
        await footer.evaluate((e, left) => e.scrollTo({ left, behavior: 'instant' }), originalScroll);
        await until(async () => await footer.evaluate((e, left) => Math.abs(e.scrollLeft - left) <= 1, originalScroll), 'read-only probes restore footer scroll');
      },
    };
    if (configure) await configure(h);
    const uiOrigin = new URL(UI).origin;
    await context.route('**/*', async route => {
      const req = route.request(), url = new URL(req.url());
      if (!['http:', 'https:'].includes(url.protocol)) return route.continue();
      if (url.origin === 'https://api.github.com') return route.fulfill({ status: 404, json: {} });
      const node = fixtures.find(n => new URL(n.url).origin === url.origin);
      if (!node && url.origin !== uiOrigin) { external.push(req.url()); return route.abort('blockedbyclient'); }
      if (!node) return route.continue();
      const session = url.pathname.match(/^\/api\/v1\/sessions\/([^/]+)\/(snapshot|commands)$/);
      if (req.method() === 'GET' && session?.[2] === 'snapshot') {
        const response = await route.fetch(), data = await response.json();
        assert.equal(data.sessionId, session[1]); assert(Array.isArray(data.events)); assert(Array.isArray(data.messages));
        assert.equal(Object.hasOwn(data, 'state'), false, 'actual fixture has no runtime snapshot state field');
        snapshots.push({ origin: url.origin, sessionId: data.sessionId, lastSeq: data.lastSeq, hasState: false });
        return route.fulfill({ response, json: data });
      }
      if (req.method() === 'POST' && session?.[2] === 'commands') {
        const body = req.postDataJSON(); commands.push({ origin: url.origin, sessionId: session[1], body });
        if (!body.type.startsWith('get_')) {
          failures.push(`Forbidden command ${body.type}`); return route.abort('blockedbyclient');
        }
        if (body.type === 'get_state') {
          if (policy.disconnected) return route.abort('connectionfailed');
          const response = await route.fetch(), data = await response.json();
          assert.equal(data.command, 'get_state'); assert.equal(data.success, true); assert(data.data);
          const patch = policy.overrides.get(`${url.origin}/${session[1]}`);
          if (patch) Object.assign(data.data, structuredClone(patch));
          states.push({ origin: url.origin, sessionId: session[1], data: data.data });
          return route.fulfill({ response, json: data });
        }
        return route.continue();
      }
      // Session open is read-like navigation; realtime uses the WebSocket listener above. Auth/metadata/projects/etc writes are forbidden.
      if (!['GET', 'HEAD', 'OPTIONS'].includes(req.method()) && !url.pathname.endsWith('/open')) {
        failures.push(`Forbidden HTTP write ${req.method()} ${url.pathname}`); return route.abort('blockedbyclient');
      }
      return route.continue();
    });
    await page.goto(UI);
    await page.getByRole('button', { name: '打开设置', exact: true }).waitFor();
    // With a fresh browser context there is no confirmed node/session. The host
    // may be absent on the landing view; if mounted, it must expose only unknowns.
    if (await h.footer().count()) {
      assert.equal(await h.item('identity').count(), 0, 'fresh defaults omit optional identity');
      await h.text('input-output', '会话累计 ↑— ↓—');
      await h.text('context', '上下文 —');
    }
    await h.connect();
    await page.locator('.transcript').getByText('主题切换已经准备好，接下来运行组件测试。', { exact: true }).waitFor();
    await until(() => states.some(s => s.sessionId === 'dark'), 'real get_state source observed');
    await h.footer().waitFor(); await h.field().fill(DRAFT);
    await verify(h);
    assert.deepEqual(external, []); assert.deepEqual(failures, []);
    assert(commands.every(c => c.body.type.startsWith('get_')), 'zero prompt/queue/model/thinking/tools/auth writes');
    for (const node of fixtures) {
      assert(node.requests.filter(r => r.path.endsWith('/commands')).every(r => r.body.type.startsWith('get_')));
      assert.equal(node.requests.filter(r => /\/(auth|metadata)$/.test(r.path) && !['GET', 'OPTIONS'].includes(r.method)).length, 0);
    }
    proof.status = 'PASS'; console.log('PASS', name);
  };
  try {
    await Promise.race([work(), new Promise((_, reject) => { timer = setTimeout(() => reject(new Error(`Case exceeded ${boundMs}ms`)), boundMs); })]);
  } catch (e) {
    proof.error = e.stack || String(e); errors.push({ name, error: proof.error }); console.error('FAIL', name, e.message);
  } finally {
    clearTimeout(timer);
    // Closing context interrupts pending browser operations on wall-bound failure.
    if (context) await context.close().catch(() => {});
    for (const node of fixtures) await node.close();
    proof.durationMs = Date.now() - started; results.push(proof); report();
  }
}
(async () => {
  fs.mkdirSync('artifacts', { recursive: true });
  try {
    browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
    await test('01 real default totals, cache, cost, context, optional identity and loaded calls', null, async h => {
      assert.deepEqual(await h.footer().locator('.footer-plugin').evaluateAll(es => es.map(e => e.dataset.pluginId)), DEFAULT_PLUGINS);
      assert.deepEqual(await h.footer().locator('.footer-item').evaluateAll(es => es.map(e => e.dataset.itemId)), BASE_ITEMS);
      assert.equal(await h.item('identity').count(), 0, 'identity is opt-in, not a default metric');
      await h.text('input-output', '会话累计 ↑32k ↓4.8k'); await h.text('cache', '缓存 75%');
      await h.text('cost', '累计$0.0184'); await h.text('context', '上下文 38k/100k ·38%');
      await h.text('tools', '已加载工具 2 ·失败0');
      assert.equal(await h.item('speed').count(), 0); assert.equal(await h.item('requests-retries').count(), 0);
      for (const [id, cls] of [['cache', 'cache-metric'], ['cost', 'cost-metric'], ['context', 'context-meter'], ['tools', 'tool-metric']]) {
        assert((await h.item(id).getAttribute('class')).split(/\s+/).includes(cls));
      }
      await h.detail();
      assert.equal(await h.stored(), null, 'loading defaults must not write'); await h.preserved();
    });
    await test('02 explicit advanced counters stay read-only; per-item clicks never refresh or open', h => {
      h.fixture.controls.runtimeStats = { cachedTokens: 24000, uncachedTokens: 8000, totalTokens: 36800, costTotal: 0.0184, cacheHitRatio: 0.75, tokensPerSecond: 42, requests: 7, retries: 0 };
    }, async h => {
      await h.openSettings();
      await h.check('生成速度').uncheck(); await h.check('请求/重试').uncheck();
      await h.check('生成速度').check(); await h.check('请求/重试').check(); await h.back();
      await h.text('speed', '42 tok/s'); await h.text('requests-retries', '请求 7 ·重试 0');
      const before = h.commands.length; await h.readonly();
      assert.equal(h.commands.length, before, 'clicking passive metrics must issue no runtime commands'); await h.preserved();
    });
    await test('03 general card/nav, native enable/item/reorder, persisted reload and reset', null, async h => {
      await h.page.getByRole('button', { name: '打开设置', exact: true }).click();
      await h.page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '通用', exact: true }).click();
      await h.page.locator('.settings-cards .settings-card').filter({ has: h.page.locator('strong', { hasText: /^底部状态栏$/ }) }).click();
      await h.settings().getByRole('heading', { name: '底部状态栏', exact: true }).waitFor();
      await until(async () => !await h.check('会话身份 插件').isDisabled(), 'settings ready');
      assert.equal(await h.check('会话身份 插件').isChecked(), false);
      // Save the historical v1 all-enabled shape through real settings: no migration
      // may silently turn identity off when that explicit preference is reloaded.
      await h.check('会话身份 插件').check();
      const allEnabled = defaults(); allEnabled.plugins.forEach(p => { p.enabled = true; });
      await h.persisted(allEnabled); await h.back();
      await h.text('identity', `deepseek / ${MODEL} · high`);
      await h.page.reload(); await h.connect();
      // Reload persistence here covers preferences, not unsent-draft persistence.
      await h.field().fill(DRAFT); await h.openSettings();
      assert(await h.check('会话身份 插件').isChecked()); await h.persisted(allEnabled);
      await h.back(); await h.text('identity', `deepseek / ${MODEL} · high`); await h.openSettings();
      await h.check('会话身份 插件').uncheck(); await h.check('累计费用').uncheck();
      await h.settings().getByRole('button', { name: '上移 已加载工具活动', exact: true }).click();
      const expected = defaults(); expected.plugins[0].enabled = false; expected.plugins[1].hidden_items = ['cost'];
      [expected.plugins[2], expected.plugins[3]] = [expected.plugins[3], expected.plugins[2]];
      await h.persisted(expected); await h.back();
      assert.equal(await h.item('identity').count(), 0); assert.equal(await h.item('cost').count(), 0);
      assert.deepEqual(await h.footer().locator('.footer-plugin').evaluateAll(es => es.map(e => e.dataset.pluginId)), ['session-usage', 'tool-activity', 'context-usage']);
      await h.preserved();
      await h.page.reload(); await h.connect(); await h.openSettings();
      assert.equal(await h.check('会话身份 插件').isChecked(), false); assert.equal(await h.check('累计费用').isChecked(), false);
      assert.deepEqual(await h.settings().locator('legend').allTextContents(), ['会话身份', '会话用量', '已加载工具活动', '上下文用量']);
      await h.check('会话身份 插件').check(); await h.check('累计费用').check();
      await h.reset(); await h.persisted(defaults());
      assert.equal(await h.check('会话身份 插件').isChecked(), false, 'reset restores identity off');
      await h.back();
      await h.text('cost', '累计$0.0184'); assert.equal(await h.item('identity').count(), 0);
    });
    await test('04 all-off removes footer without losing draft/model/send; reset restores', null, async h => {
      await h.openSettings(); for (const title of TITLES) await h.check(`${title} 插件`).uncheck();
      const expected = defaults(); expected.plugins.forEach(p => { p.enabled = false; }); await h.persisted(expected);
      await h.back(); assert.equal(await h.footer().count(), 0); assert.equal(await h.page.locator('footer.runtime-footer').count(), 0);
      await h.preserved(); await h.openSettings(); await h.reset(); await h.persisted(defaults()); await h.back();
      await h.footer().waitFor(); assert.equal(await h.item('identity').count(), 0); await h.preserved();
      // Detail is absent when usage is disabled even while other plugins remain.
      await h.openSettings(); await h.check('会话用量 插件').uncheck(); await h.back();
      await h.detail(); await h.text('context', '上下文 38k/100k ·38%');
      await h.openSettings(); await h.reset(); await h.back(); await h.detail();
    });
    await test('05 corrupted storage remains byte-identical through memory edits until explicit reset', async h => {
      await h.context.addInitScript(({ key }) => {
        if (!sessionStorage.getItem('footer-corrupt-seeded')) { localStorage.setItem(key, '{broken footer JSON'); sessionStorage.setItem('footer-corrupt-seeded', '1'); }
      }, { key: KEY });
    }, async h => {
      await h.openSettings(); await h.settings().getByRole('alert').filter({ hasText: 'JSON 无效' }).waitFor();
      assert.equal(await h.check('会话身份 插件').isChecked(), false);
      await h.check('会话身份 插件').check(); await h.check('累计费用').uncheck();
      assert.equal(await h.stored(), '{broken footer JSON'); await h.back();
      await h.text('identity', `deepseek / ${MODEL} · high`); assert.equal(await h.item('cost').count(), 0); await h.preserved();
      await h.page.reload(); await h.connect();
      assert.equal(await h.item('identity').count(), 0, 'corrupt storage reload uses identity-off defaults');
      await h.openSettings();
      await h.settings().getByRole('alert').waitFor(); assert.equal(await h.check('会话身份 插件').isChecked(), false);
      assert.equal(await h.stored(), '{broken footer JSON');
      await h.reset(); await h.persisted(defaults()); assert.equal(await h.settings().getByRole('alert').count(), 0);
    });
    await test('06 quota rejection leaves memory settings usable and reports error', async h => {
      await h.context.addInitScript(({ key }) => {
        const original = Storage.prototype.setItem;
        Storage.prototype.setItem = function(k, value) {
          if (this === localStorage && k === key) throw new DOMException('Fixture quota', 'QuotaExceededError');
          return original.call(this, k, value);
        };
      }, { key: KEY });
    }, async h => {
      await h.openSettings(); await h.check('会话身份 插件').check();
      await h.settings().getByRole('alert').filter({ hasText: '仅保留在内存' }).waitFor();
      assert.equal(await h.stored(), null); assert.equal(await h.check('会话身份 插件').isChecked(), true);
      await h.back(); await h.text('identity', `deepseek / ${MODEL} · high`);
      await h.openSettings(); await h.check('会话身份 插件').uncheck(); await h.back();
      assert.equal(await h.item('identity').count(), 0); await h.preserved();
      await h.openSettings(); await h.check('会话身份 插件').check(); await h.back();
      await h.text('identity', `deepseek / ${MODEL} · high`); await h.preserved();
    });
    await test('07 session/node scope derives from each real get_state, unknown identity never leaks', async h => {
      h.override(h.fixture, 'architecture', { provider: 'scope-b', modelId: 'scope-b/architecture', thinkingLevel: 'low', stats: { cachedTokens: 100, uncachedTokens: 900, outputTokens: 20 }, context: { tokens: 200, window: 1000 } });
      h.second = await h.addNode();
      h.override(h.second, 'dark', { provider: null, modelId: null, thinkingLevel: null, stats: {}, context: {} });
    }, async h => {
      await h.enableIdentity(); await h.text('identity', `deepseek / ${MODEL} · high`);
      await h.chooseSession('architecture'); await h.text('identity', 'scope-b / scope-b/architecture · low');
      await h.text('input-output', '会话累计 ↑1k ↓20'); await h.text('context', '上下文 200/1k ·20%');
      await h.chooseSession('dark'); await h.text('identity', `deepseek / ${MODEL} · high`);
      assert.equal(await h.field().inputValue(), DRAFT);
      await h.connect(h.second); await h.text('identity', '— / —');
      await h.text('input-output', '会话累计 ↑— ↓—'); await h.text('context', '上下文 —');
      assert.doesNotMatch(await h.footer().innerText(), /scope-b|deepseek|32k/);
      assert(h.states.some(s => s.origin === new URL(h.second.url).origin && s.data.provider === null));
    });
    await test('08 genuine WebSocket loss shows unconfirmed placeholders without synthetic stats or footer actions', null, async h => {
      await h.enableIdentity(); await h.text('identity', `deepseek / ${MODEL} · high`);
      await h.text('input-output', '会话累计 ↑32k ↓4.8k');
      h.policy.disconnected = true; h.fixture.disconnect('dark', true);
      await h.footer().getByRole('status').filter({ hasText: '节点未连接，统计未确认' }).waitFor();
      await h.text('identity', '— / —'); await h.text('input-output', '会话累计 ↑— ↓—');
      await h.text('context', '上下文 —'); await h.text('tools', '已加载工具 — ·失败—');
      const before = h.states.length; await h.readonly(); assert.equal(h.states.length, before);
      await h.compact('plugins-offline-inline');
      assert.equal(await h.field().inputValue(), DRAFT);
    });
    await test('09 missing fields do not invent totals, tokens or advanced metrics from percentages', h => {
      h.fixture.controls.runtimeStats = {}; h.fixture.controls.runtimeContext = { percent: 38 };
    }, async h => {
      await h.text('input-output', '会话累计 ↑— ↓—'); await h.text('cache', '缓存 —');
      await h.text('cost', '累计—'); await h.text('context', '上下文 —');
      assert.equal(await h.item('speed').count(), 0); assert.equal(await h.item('requests-retries').count(), 0);
      assert.deepEqual(h.states.find(s => s.sessionId === 'dark').data.stats, {});
      assert(h.snapshots.some(s => !s.hasState));
    });
    await test('10 real zero counts/cost/speed differ from unavailable values and zero window', h => {
      h.fixture.controls.runtimeStats = { cachedTokens: 0, uncachedTokens: 0, outputTokens: 0, costTotal: 0, cacheHitRatio: 0, tokensPerSecond: 0, requests: 0, retries: 0 };
      h.fixture.controls.runtimeContext = { tokens: 0, window: 0, percent: 0 };
    }, async h => {
      await h.text('input-output', '会话累计 ↑0 ↓0'); await h.text('cost', '累计$0.0000');
      await h.text('cache', '缓存 0%'); await h.text('speed', '0 tok/s'); await h.text('requests-retries', '请求 0 ·重试 0');
      await h.text('context', '上下文 —'); await h.readonly();
    });
    await test('11 negative/overflow inputs are bounded; loaded calls ignore tool config/stats counts', h => {
      h.override(h.fixture, 'dark', { stats: { cachedTokens: -1, uncachedTokens: 10, outputTokens: 1e16, costTotal: 1e13, tokensPerSecond: -1, requests: 1e16, retries: -2, tools: 999, toolCalls: 999 }, context: { tokens: -1, window: 100000 }, activeTools: ['read'] });
      h.override(h.fixture, 'architecture', { stats: { cachedTokens: 1e16, uncachedTokens: 1e16, outputTokens: -1, costTotal: -1, tokensPerSecond: 1e16, requests: -1, retries: 1e16, toolCalls: 555 }, context: { tokens: 1e16, window: 1e17 }, activeTools: [] });
    }, async h => {
      async function invalid() {
        await h.text('input-output', '会话累计 ↑— ↓—'); await h.text('cost', '累计—'); await h.text('context', '上下文 —');
        assert.equal(await h.item('speed').count(), 0); assert.equal(await h.item('requests-retries').count(), 0);
        assert.doesNotMatch(await h.footer().innerText(), /NaN|Infinity|undefined|999|555/);
      }
      await invalid(); await h.text('tools', '已加载工具 2 ·失败0');
      // Append a true tool-end event for an existing call: deduplicate call/result and mark failure.
      h.fixture.emit({ type: 'tool_execution_end', toolCallId: 'shell-1', toolName: 'bash', isError: true, result: { content: [{ type: 'text', text: 'Fixture tool failure' }], isError: true } });
      await h.text('tools', '已加载工具 2 ·失败1');
      await h.chooseSession('architecture'); await invalid(); await h.text('tools', '已加载工具 2 ·失败0');
      await h.chooseSession('dark'); await h.text('tools', '已加载工具 2 ·失败1');
    });
    await test('12 default light/dark 1280/390/320 screenshots, compact scrolling row and full escaped identity', null, async h => {
      assert.equal(await h.item('identity').count(), 0, 'default screenshots must not include identity');
      for (const mode of ['dark', 'light']) {
        await h.page.setViewportSize({ width: 1280, height: 900 });
        await h.page.getByRole('button', { name: '外观与主题', exact: true }).click();
        const theme = h.page.getByRole('dialog', { name: '外观与主题', exact: true });
        await theme.getByLabel('明暗模式', { exact: true }).selectOption(mode);
        await theme.getByRole('button', { name: '保存', exact: true }).click(); await theme.waitFor({ state: 'hidden' });
        for (const width of [1280, 390, 320]) {
          await h.page.setViewportSize({ width, height: 900 });
          await h.text('input-output', '会话累计 ↑32k ↓4.8k'); await h.text('cache', '缓存 75%');
          await h.text('cost', '累计$0.0184'); await h.text('context', '上下文 38k/100k ·38%');
          await h.text('tools', '已加载工具 2 ·失败0');
          const sizes = await h.compact(`plugins-${mode}-${width}`);
          assert.equal(await h.item('identity').count(), 0);
          assert.deepEqual(sizes.items.map(i => i.id), BASE_ITEMS.filter(id => width > 700 || id !== 'cost'));
          assert.deepEqual(sizes.hiddenItems.map(i => i.id), width <= 700 ? ['cost'] : []);
          assert(sizes.scroll <= sizes.client + 1, 'restored defaults fit without footer horizontal scrolling, including 390/320px');
          await h.readonly(); await h.preserved();
        }
      }
      // Force hostile *HTTP runtime text*, never DOM. Registry's 512-byte item value limit remains respected.
      const hostile = '<img src=x onerror="globalThis.__footerXss=1">';
      const long = `scope/${'long-model-'.repeat(16)}`;
      h.override(h.fixture, 'architecture', { provider: hostile, modelId: long, thinkingLevel: 'high' });
      await h.page.setViewportSize({ width: 1280, height: 900 });
      await h.enableIdentity(); await h.chooseSession('architecture');
      await h.text('identity', `${hostile} / ${long} · high`);
      const identity = h.item('identity');
      assert.equal(await identity.getAttribute('title'), null);
      assert((await identity.getAttribute('class')).split(/\s+/).includes('identity-metric'));
      assert((await identity.getAttribute('aria-label')).startsWith(`供应商/模型：${hostile} / ${long} · high。`));
      assert.equal(await identity.locator('img,script,svg').count(), 0);
      assert.equal(await h.page.evaluate(() => globalThis.__footerXss), undefined);
      for (const width of [1280, 390, 320]) {
        await h.page.setViewportSize({ width, height: 900 });
        await h.compact(`plugins-hostile-identity-${width}`);
        assert.equal(await identity.innerText(), `${hostile} / ${long} · high`, 'full model remains inline, not hidden behind a tooltip');
        await identity.hover();
        assert.equal(await identity.evaluate(e => getComputedStyle(e, '::after').content), 'none');
        assert.deepEqual(await h.page.locator('dialog[open],[role=tooltip]:visible').evaluateAll(es => es.map(e => ({ tag: e.tagName, id: e.id, role: e.getAttribute('role'), text: e.textContent }))), [], 'footer hover never opens a dialog or visible tooltip; unrelated hidden timeline placeholders are inert');
      }
      await h.page.setViewportSize({ width: 1280, height: 900 });
      await h.page.mouse.move(0, 0);
      // Native dialogs remain reachable from non-footer contexts, not by metrics.
      await h.page.getByRole('button', { name: '查看任务上下文', exact: true }).click();
      await h.page.getByRole('button', { name: '管理当前上下文', exact: true }).click();
      const dialog = h.page.getByRole('dialog', { name: '上下文与压缩', exact: true }); await dialog.waitFor();
      await h.page.keyboard.press('Escape'); await dialog.waitFor({ state: 'hidden' });
    }, 75000);
    await test('13 larger typography, keyboard viewport and safe insets keep advanced metrics inline without native hints', h => {
      h.fixture.controls.runtimeStats = { cachedTokens: 24000, uncachedTokens: 8000, outputTokens: 4800, costTotal: 0.0184, tokensPerSecond: 42, requests: 9, retries: 0, cacheHitRatio: 0.75 };
    }, async h => {
      for (const scenario of [{ width: 390, height: 430, scale: 1 }, { width: 320, height: 844, scale: 1.5 }, { width: 1280, height: 900, scale: 1.5 }]) {
        await h.page.setViewportSize(scenario);
        await h.page.evaluate(({ scale }) => {
          document.querySelector('.app-shell').style.setProperty('--type-scale', String(scale));
          document.documentElement.style.setProperty('--app-safe-bottom', '24px');
          document.documentElement.style.setProperty('--app-safe-top', '24px');
        }, scenario);
        await h.text('speed', '42 tok/s'); await h.text('requests-retries', '请求 9 ·重试 0');
        await h.compact(`style-${scenario.width}-${scenario.height}-${scenario.scale}`, {
          maxHeight: scenario.scale === 1.5 ? 44 : 32, safeInset: true,
        });
        assert.equal(await h.footer().locator('[title]').count(), 0);
        const composer = await h.page.locator('.composer-main .composer').boundingBox();
        assert(composer.y >= 24 && composer.y + composer.height <= scenario.height - 23);
        await h.readonly(); await h.preserved();
      }
    });
  } catch (e) { errors.push({ name: 'suite setup', error: e.stack || String(e) }); console.error(e); }
  finally { if (browser) await browser.close(); report(); }
  if (errors.length) process.exitCode = 1;
  else console.log(`PASS footer plugins: ${results.length} isolated cases; zero runtime/auth writes`);
})();
