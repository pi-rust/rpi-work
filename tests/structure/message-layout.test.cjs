// CSS-only contract: source styles + synthetic DOM, no WASM, server or session.
// This does not validate the production Rust renderer or Android presentation.
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { chromium } = require('../support/browser.cjs');
const root = path.resolve(__dirname, '../../apps/work-ui/src');
const css = [
  'shared/theme/themes.css', 'shared/theme/codex.css', 'shared/theme/styles.css',
  'shared/theme/theme-design.css', 'shared/theme/codex-components.css',
  'features/conversation/conversation.css', 'shared/theme/tool-presentation.css',
  'features/conversation/user_timeline.css', 'features/projects/management.css',
].map(file => fs.readFileSync(path.join(root, file), 'utf8')).join('\n');
const copy = className => `<span class="conversation-copy"><button class="button tool-icon-button ${className}" data-copied="true">Copy</button></span>`;
const prose = `<article class="conversation-widget message timeline-message agent-message"><div class="message-heading">Assistant</div><div class="message-content"><div class="conversation-markdown"><p>Readable prose</p><div class="conversation-code"><div class="code-language"><span>rust</span>${copy('markdown-copy')}</div><pre><code><span class="code-token-keyword">let</span> value = <span class="code-token-number">42</span>;</code></pre></div><table><tr><td>Cell</td></tr></table></div><div class="message-actions">${copy('message-copy-markdown')}</div></div></article>`;
const tool = '<div class="timeline-content-region timeline-tool-region timeline-message"><div class="conversation-widget conversation-tool"><details class="inline-call"><summary>Tool call</summary></details></div></div>';
const row = (key, body) => `<div data-message-row="${key}" class="message-turn">${body}</div>`;

test('conversation source CSS keeps a local reading axis and one visual gap owner', async () => {
  const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
  try {
    for (const mode of ['light', 'dark']) {
      for (const width of [320, 390, 740, 1440]) {
        const page = await browser.newPage({ viewport: { width, height: 1600 } });
        try {
          const user = '<article class="conversation-widget message timeline-message user-message"><div class="message-heading user-message-heading">You</div><div class="message-content"><div class="conversation-user-text">Question</div></div></article>';
          await page.setContent(`<html data-theme="codex" data-mode="${mode}"><style>${css}</style><div class="conversation-panel" style="height:1600px"><div class="transcript-frame"><div class="conversation-timeline"><button class="timeline-node">·</button></div><div class="transcript">${row('user', user)}${row('answer', `<div class="timeline-content-region">${prose}</div>${tool}${tool}<div class="timeline-content-region">${prose}</div>`)}${row('next', prose)}</div></div></div></html>`);
          const m = await page.evaluate(() => {
            const q = selector => document.querySelector(selector);
            const rect = element => { const r = element.getBoundingClientRect(); return { x: r.x, width: r.width, top: r.top, bottom: r.bottom, height: r.height }; };
            const p = [...document.querySelectorAll('.agent-message')].map(rect);
            const t = [...document.querySelectorAll('.inline-call')].map(rect);
            const u = rect(q('.user-message'));
            return {
              userWidth: u.width, x: p[0].x, proseWidth: p[0].width, toolWidth: t[0].width,
              bubbleWidth: rect(q('.user-message .message-content')).width,
              gaps: [p[0].top - u.bottom, t[0].top - p[0].bottom, t[1].top - t[0].bottom, p[1].top - t[1].bottom, p[2].top - p[1].bottom],
              rail: rect(q('.conversation-timeline')),
              body: getComputedStyle(q('.conversation-markdown')).fontSize,
              table: getComputedStyle(q('td')).fontSize,
              codeHeader: rect(q('.code-language')).height,
              copy: rect(q('.markdown-copy')),
              actionOpacity: getComputedStyle(q('.message-actions')).opacity,
              workspaceMax: getComputedStyle(document.documentElement).getPropertyValue('--conversation-content-max').trim(),
            };
          });
          assert.deepEqual(m.gaps, [24, 20, 0, 20, 24], `${mode}/${width}`);
          assert.equal(m.workspaceMax, '1000px');
          assert.equal(m.body, '15px');
          assert.equal(m.table, '13px');
          assert.equal(m.codeHeader, width < 768 ? 44 : 30);
          assert.equal(m.copy.width, width < 768 ? 44 : 28);
          assert.equal(m.copy.height, width < 768 ? 44 : 28);
          if (width < 768) {
            assert.equal(m.x, 36);
            assert.equal(m.proseWidth, width - 60);
            assert.equal(m.actionOpacity, '1');
            assert.ok(m.rail.x + m.rail.width <= m.x, 'rail must not steal prose taps');
          } else {
            assert.equal(m.userWidth, 780);
            assert.equal(m.proseWidth, 760);
            assert.equal(m.toolWidth, 760);
            assert.equal(m.actionOpacity, '0');
            await page.locator('.message-copy-markdown').first().focus();
            assert.equal(await page.locator('.message-actions').first().evaluate(e => getComputedStyle(e).opacity), '1');
          }
          if (width === 390) assert.ok(Math.abs(m.bubbleWidth - 310.2) < 1);
          // Real history retains zero-height wrappers for linked tool results.
          // They must not change the next public message's visual spacing.
          for (const emptyRows of [0, 1, 3]) {
            await page.setContent(`<html data-theme="codex" data-mode="${mode}"><style>${css}</style><div class="conversation-panel"><div class="transcript">${row('tool', tool)}${Array.from({ length: emptyRows }, (_, i) => row(`result-${i}`, '')).join('')}${row('public', `<div class="timeline-content-region">${prose}</div>`)}</div></div></html>`);
            const gap = await page.evaluate(() => document.querySelector('.agent-message').getBoundingClientRect().top - document.querySelector('.inline-call').getBoundingClientRect().bottom);
            assert.equal(gap, 24, `${mode}/${width}: empty result wrappers do not change separate reply spacing`);
          }
        } finally { await page.close(); }
      }
    }
  } finally { await browser.close(); }
});
