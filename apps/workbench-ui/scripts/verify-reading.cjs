// Real Rust-client conversation: display geometry, canonical phase and history recovery.
const assert = require('node:assert/strict')
const path = require('node:path')
const net = require('node:net')
const http = require('node:http')
const fs = require('node:fs/promises')
const { spawn } = require('node:child_process')
const { chromium } = require(require.resolve('playwright', { paths: [path.resolve(__dirname, '../../../tools')] }))
const { startFixture } = require('../../../tests/fixtures/ui-fixture.cjs')
const root = path.resolve(__dirname, '..')
const KEY = 'rpi-workbench-message-layout-v1'
const freePort = () => new Promise(resolve => { const server = net.createServer(); server.listen(0, '127.0.0.1', () => { const port = server.address().port; server.close(() => resolve(port)) }) })
;(async () => {
  const fixture = await startFixture(0, { nativeQueries: true, includeHistoryAssistant: true })
  let previewLoads = 0
  const site = http.createServer((req, res) => { if (req.url === '/') previewLoads++; res.setHeader('Content-Type', 'text/html'); res.end('<!doctype html><html><body><label>Preview draft<input aria-label="Preview draft"></label></body></html>') })
  await new Promise(resolve => site.listen(0, '127.0.0.1', resolve))
  const siteUrl = `http://127.0.0.1:${site.address().port}/`
  const port = await freePort(), origin = `http://127.0.0.1:${port}`
  const preview = spawn(process.execPath, [path.join(root, 'node_modules/vite/bin/vite.js'), 'preview', '--host', '127.0.0.1', '--port', String(port), '--strictPort'], { cwd: root, env: { ...process.env, RPI_NODE_URL: fixture.url }, stdio: 'ignore' })
  let browser
  const failures = []
  try {
    let ready = false
    for (let i = 0; i < 120; i++) { try { if ((await fetch(origin)).ok) { ready = true; break } } catch {} await new Promise(resolve => setTimeout(resolve, 100)) }
    assert(ready, 'owned product preview started')
    browser = await chromium.launch()
    const check = async (name, run) => {
      const context = await browser.newContext({ viewport: { width: 1440, height: 940 }, deviceScaleFactor: 1, reducedMotion: 'reduce' })
      const page = await context.newPage(), errors = []
      page.setDefaultTimeout(10000)
      page.on('pageerror', error => errors.push(error.message))
      const rendered = () => page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))))
      const open = async () => { await page.goto(origin + '/?live=1'); await page.getByText('帮我为工作台增加一个深色主题，并保持手机端可用。', { exact: true }).waitFor() }
      const choose = async value => { await page.getByRole('button', { name: '打开显示选项', exact: true }).click(); await page.getByRole('combobox', { name: '消息布局', exact: true }).selectOption(value); await page.keyboard.press('Escape'); await rendered() }
      const geometry = () => page.evaluate(() => {
        const column = document.querySelector('.wb-transcript').getBoundingClientRect()
        const prompt = document.querySelector('.wb-prompt'), prose = document.querySelector('.wb-prose')
        const p = prompt.getBoundingClientRect(), a = prose.getBoundingClientRect(), style = getComputedStyle(prompt), answer = getComputedStyle(prose)
        return { column: { x: column.x, width: column.width, right: column.right }, prompt: { x: p.x, width: p.width, right: p.right }, answer: { x: a.x, width: a.width }, background: style.backgroundColor, radius: style.borderRadius, font: answer.fontSize, lineHeight: answer.lineHeight, overflow: document.documentElement.scrollWidth - innerWidth }
      })
      try { await run({ context, page, open, choose, rendered, geometry }); assert.deepEqual(errors, []); console.log('PASS', name) }
      catch (error) { failures.push(name); console.error('FAIL', name, error.stack) }
      finally { await context.close() }
    }
    await check('reading/classic geometry and persistence preserve message/tool/preview DOM and draft selection', async ({ page, open, choose, geometry }) => {
      await open()
      await page.getByRole('button', { name: '打开显示选项', exact: true }).click()
      assert.equal(await page.getByRole('combobox', { name: '消息布局', exact: true }).inputValue(), 'reading')
      await page.keyboard.press('Escape')
      const reading = await geometry()
      assert(reading.prompt.width < reading.column.width * 0.91, JSON.stringify(reading))
      assert(Math.abs(reading.prompt.right - reading.column.right) < 1)
      assert.equal(reading.font, '16px')
      assert.equal(reading.lineHeight, '26.4px')
      for (const summary of await page.locator('.wb-activity:not([open]) > summary').all()) await summary.click()
      await page.getByText('bash', { exact: true }).first().click()
      await page.getByText(/12 passed; 0 failed/).waitFor()
      await page.getByRole('textbox', { name: '预览地址', exact: true }).fill(siteUrl)
      await page.getByRole('button', { name: '打开', exact: true }).click()
      const previewDraft = page.frameLocator('.real-browser iframe').getByLabel('Preview draft')
      await previewDraft.fill('keep the real page form')
      await page.locator('textarea').fill('保留布局切换前的草稿与选区')
      await page.locator('textarea').evaluate(node => node.setSelectionRange(2, 7))
      await page.evaluate(() => { window.readingNodes = { message: document.querySelector('.wb-prompt'), tool: document.querySelectorAll('.wb-tool-card')[1], frame: document.querySelector('.real-browser iframe') } })
      const loads = previewLoads
      await choose('classic')
      const classic = await geometry()
      assert(Math.abs(classic.prompt.x - classic.answer.x) < 1, JSON.stringify(classic))
      assert(Math.abs(classic.prompt.width - classic.column.width) < 1)
      assert(Math.abs(classic.answer.width - classic.column.width) < 1)
      assert.equal(classic.background, 'rgba(0, 0, 0, 0)')
      assert.equal(classic.radius, '0px')
      assert.equal(await page.evaluate(() => readingNodes.message === document.querySelector('.wb-prompt') && readingNodes.tool === document.querySelectorAll('.wb-tool-card')[1] && readingNodes.frame === document.querySelector('.real-browser iframe')), true)
      assert.equal(await previewDraft.inputValue(), 'keep the real page form')
      assert.equal(previewLoads, loads)
      assert.deepEqual(await page.locator('textarea').evaluate(node => [node.value, node.selectionStart, node.selectionEnd]), ['保留布局切换前的草稿与选区', 2, 7])
      assert.equal(await page.getByText(/12 passed; 0 failed/).isVisible(), true)
      const artifacts = path.resolve(root, '../../artifacts/reading')
      await fs.mkdir(artifacts, { recursive: true })
      await page.screenshot({ path: path.join(artifacts, 'classic-light.png') })
      await choose('reading')
      await page.screenshot({ path: path.join(artifacts, 'reading-light.png') })
      await choose('classic')
      await page.reload()
      await page.getByText('帮我为工作台增加一个深色主题，并保持手机端可用。', { exact: true }).waitFor()
      assert.equal(await page.locator('[data-message-layout]').getAttribute('data-message-layout'), 'classic')
    })
    await check('both layouts keep the reading edge and avoid horizontal overflow in dark desktop and 390px', async ({ page, open, choose, rendered, geometry }) => {
      await open()
      await page.getByRole('button', { name: '切换主题', exact: true }).click()
      for (const summary of await page.locator('.wb-activity:not([open]) > summary').all()) await summary.click()
      await page.getByRole('button', { name: '打开显示选项', exact: true }).click()
      await page.getByRole('button', { name: '切换密度', exact: true }).click()
      await page.keyboard.press('Escape')
      await rendered()
      assert.equal((await geometry()).lineHeight, '24.8px')
      for (const width of [1440, 390]) {
        await page.setViewportSize({ width, height: 940 })
        await rendered()
        for (const layout of ['reading', 'classic']) {
          await choose(layout)
          const value = await geometry()
          assert(value.overflow <= 1, JSON.stringify(value))
          assert(Math.abs(value.answer.x - value.column.x) < 1)
          if (layout === 'classic') assert(Math.abs(value.prompt.width - value.answer.width) < 1)
        }
      }
    })
    await check('invalid preferences fall back to reading and rejected storage keeps the selected layout in memory', async ({ context, page, open, choose }) => {
      await context.addInitScript(key => {
        localStorage.setItem(key, 'unsupported-layout')
        Object.defineProperty(Storage.prototype, 'setItem', { value() { throw new DOMException('fixture storage rejected', 'SecurityError') } })
      }, KEY)
      await open()
      assert.equal(await page.locator('[data-message-layout]').getAttribute('data-message-layout'), 'reading')
      await choose('classic')
      assert.equal(await page.locator('[data-message-layout]').getAttribute('data-message-layout'), 'classic')
    })
    await check('completed activity defaults closed, prose is outside it, and summary is keyboard operable', async ({ page, open, choose }) => {
      await open()
      const group = page.locator('.wb-activity').first()
      assert.equal(await group.getAttribute('open'), null)
      assert.equal(await page.locator('.wb-prose').first().evaluate(node => node.closest('.wb-activity') === null), true)
      await group.locator(':scope > summary').focus()
      await page.keyboard.press('Enter')
      assert.notEqual(await group.getAttribute('open'), null)
      await choose('classic')
      assert.notEqual(await group.getAttribute('open'), null)
      await group.locator(':scope > summary').focus()
      await page.keyboard.press('Space')
      assert.equal(await group.getAttribute('open'), null)
    })
    await check('tool completion is not settlement; canonical settlement folds untouched activity without replacing streamed prose', async ({ page, open, rendered }) => {
      await open()
      fixture.emit({ type: 'agent_start' })
      fixture.emit({ type: 'message_end', message: { entryId: 'layout-auto-user', role: 'user', content: '真实工具分组与结算场景' } })
      await page.getByText('真实工具分组与结算场景', { exact: true }).waitFor()
      fixture.emit({ type: 'tool_execution_start', toolCallId: 'layout-auto', toolName: 'bash', args: { command: 'pwd' } })
      const tool = page.locator('[data-tool-id="layout-auto"]'), group = page.locator('[data-activity-key="activity/tool/layout-auto"]')
      await tool.waitFor()
      fixture.emit({ type: 'tool_execution_end', toolCallId: 'layout-auto', result: { content: [{ type: 'text', text: 'observed output' }] } })
      await tool.getByText('已完成', { exact: true }).waitFor({ state: 'attached' })
      assert.notEqual(await group.getAttribute('open'), null, 'completed tool does not fold active agent')
      fixture.emit({ type: 'message_update', message: { entryId: 'layout-stream' }, eventType: 'text_delta', delta: '真实流式正文' })
      await page.getByText('真实流式正文', { exact: true }).waitFor()
      await page.getByText('真实流式正文', { exact: true }).evaluate(node => { window.streamedProse = node })
      fixture.emit({ type: 'message_end', message: { entryId: 'layout-stream', role: 'assistant', content: [{ type: 'text', text: '真实流式正文及最终记录' }] } })
      await page.getByText('真实流式正文及最终记录', { exact: true }).waitFor()
      assert.equal(await page.getByText('真实流式正文及最终记录', { exact: true }).evaluate(node => node === window.streamedProse && node.closest('.wb-activity') === null), true)
      fixture.emit({ type: 'agent_settled', outcome: 'completed', pendingResume: false })
      await page.waitForFunction(() => document.querySelector('[data-activity-key="activity/tool/layout-auto"]')?.open === false)
      await rendered()
      assert.equal(await page.getByRole('status', { name: '会话运行观察' }).textContent(), '会话观察：已完成')
      assert.equal(await page.getByRole('status', { name: '会话运行观察' }).getAttribute('data-tone'), 'success')
      assert.equal(await page.getByText('真实流式正文及最终记录', { exact: true }).isVisible(), true)
    })
    await check('explicit child and parent expansion intent survives settlement and layout switches', async ({ page, open, choose }) => {
      await open()
      fixture.emit({ type: 'agent_start' })
      fixture.emit({ type: 'message_end', message: { entryId: 'layout-intent-user', role: 'user', content: '保留真实工具展开意图' } })
      await page.getByText('保留真实工具展开意图', { exact: true }).waitFor()
      fixture.emit({ type: 'tool_execution_start', toolCallId: 'layout-intent', toolName: 'bash', args: { command: 'pwd' } })
      const tool = page.locator('[data-tool-id="layout-intent"]'), group = page.locator('[data-activity-key="activity/tool/layout-intent"]')
      await tool.getByRole('button').click()
      fixture.emit({ type: 'tool_execution_end', toolCallId: 'layout-intent', result: { content: [{ type: 'text', text: 'keep explicit output open' }] } })
      await page.getByText('keep explicit output open', { exact: true }).waitFor()
      fixture.emit({ type: 'agent_settled', outcome: 'completed', pendingResume: false })
      await page.waitForFunction(() => document.querySelector('[aria-label="会话运行观察"]')?.textContent === '会话观察：已完成')
      await choose('classic')
      assert.notEqual(await group.getAttribute('open'), null)
      assert.equal(await page.getByText('keep explicit output open', { exact: true }).isVisible(), true)
      await group.locator(':scope > summary').click()
      await choose('reading')
      assert.equal(await group.getAttribute('open'), null)
      assert.equal(await page.getByText('keep explicit output open', { exact: true }).isVisible(), false)
    })
    await check('failed tool and failed run retain visible diagnostics without a success icon', async ({ page, open }) => {
      await open()
      fixture.emit({ type: 'agent_start' })
      fixture.emit({ type: 'message_end', message: { entryId: 'layout-failure-user', role: 'user', content: '真实失败层次场景' } })
      await page.getByText('真实失败层次场景', { exact: true }).waitFor()
      fixture.emit({ type: 'tool_execution_start', toolCallId: 'layout-failure', toolName: 'bash', args: { command: 'false' } })
      const tool = page.locator('[data-tool-id="layout-failure"]'), group = page.locator('[data-activity-key="activity/tool/layout-failure"]')
      await tool.waitFor()
      fixture.emit({ type: 'tool_execution_end', toolCallId: 'layout-failure', isError: true, result: { content: [{ type: 'text', text: 'observed tool failure' }] } })
      fixture.emit({ type: 'agent_settled', outcome: 'failed', pendingResume: false })
      await page.waitForFunction(() => document.querySelector('[aria-label="会话运行观察"]')?.textContent === '会话观察：运行失败')
      assert.notEqual(await group.getAttribute('open'), null)
      assert.equal(await page.getByRole('status', { name: '会话运行观察' }).getAttribute('data-tone'), 'danger')
      assert.equal(await tool.locator('.text-emerald-600').count(), 0)
      await tool.getByRole('button').click()
      await page.getByText('observed tool failure', { exact: true }).waitFor()
    })
    await check('reader position is anchored through streamed history and only explicit jump follows latest', async ({ page, open, rendered }) => {
      fixture.emit({ type: 'message_end', message: { entryId: 'scroll-history-tool', role: 'assistant', content: [{ type: 'toolCall', id: 'layout-intent', name: 'bash', arguments: { command: 'pwd' } }] } })
      fixture.emit({ type: 'message_end', message: { entryId: 'scroll-history-result', role: 'toolResult', toolCallId: 'layout-intent', isError: false, content: [{ type: 'text', text: 'keep explicit output open' }] } })
      for (let index = 0; index < 32; index++) {
        fixture.emit({ type: 'message_end', message: { entryId: `scroll-base-${index}`, role: 'assistant', content: `scroll-base-${index} ${'long canonical response content. '.repeat(24)}` } })
      }
      await open()
      await page.emulateMedia({ reducedMotion: 'no-preference' })
      await page.getByText('scroll-base-0', { exact: false }).first().waitFor()
      const log = page.locator('.wb-message-scroll')
      await log.evaluate(node => { node.scrollTop = Math.round((node.scrollHeight - node.clientHeight) * 0.43); node.dispatchEvent(new Event('scroll')) })
      const anchor = await page.evaluate(() => {
        const log = document.querySelector('.wb-message-scroll'), logTop = log.getBoundingClientRect().top
        const node = [...log.querySelectorAll('[data-conversation-anchor]')].find(item => item.getBoundingClientRect().bottom > logTop + 1)
        return { key: node?.dataset.conversationAnchor, top: node?.getBoundingClientRect().top - logTop, scrollTop: log.scrollTop }
      })
      assert(anchor.key && anchor.top !== undefined)
      fixture.emit({ type: 'message_update', message: { entryId: 'scroll-new-history' }, eventType: 'text_delta', delta: 'scroll-new-history streaming' })
      await page.getByText('scroll-new-history streaming', { exact: true }).waitFor()
      await page.getByText('scroll-new-history streaming', { exact: true }).evaluate(node => { window.scrollStream = node })
      fixture.emit({ type: 'message_end', message: { entryId: 'scroll-new-history', role: 'assistant', content: `scroll-new-history ${'new message without stealing the reader. '.repeat(40)}` } })
      await page.getByText('scroll-new-history', { exact: false }).waitFor()
      assert.equal(await page.getByText('scroll-new-history', { exact: false }).evaluate(node => node === window.scrollStream), true)
      await page.getByRole('button', { name: '有新消息 · 回到底部', exact: true }).waitFor()
      await rendered()
      const after = await page.evaluate(key => {
        const log = document.querySelector('.wb-message-scroll'), node = [...log.querySelectorAll('[data-conversation-anchor]')].find(item => item.dataset.conversationAnchor === key)
        return { top: node.getBoundingClientRect().top - log.getBoundingClientRect().top, scrollTop: log.scrollTop, max: log.scrollHeight - log.clientHeight }
      }, anchor.key)
      assert(Math.abs(after.top - anchor.top) < 2, JSON.stringify({ anchor, after }))
      assert(Math.abs(after.scrollTop - after.max) > 48)
      const buttonBox = await page.locator('.wb-latest-button').boundingBox(), logBox = await log.boundingBox()
      assert(buttonBox.y >= logBox.y && buttonBox.y + buttonBox.height <= logBox.y + logBox.height, JSON.stringify({ buttonBox, logBox }))
      await page.getByRole('button', { name: '有新消息 · 回到底部', exact: true }).click()
      await page.waitForFunction(() => { const node = document.querySelector('.wb-message-scroll'); return node.scrollHeight - node.scrollTop - node.clientHeight <= 2 })
      assert.equal(await page.getByRole('button', { name: '有新消息 · 回到底部', exact: true }).count(), 0)
      fixture.emit({ type: 'message_end', message: { entryId: 'scroll-bottom-follow', role: 'assistant', content: '底部跟随只在读者已经贴底时继续' } })
      await page.getByText('底部跟随只在读者已经贴底时继续', { exact: true }).waitFor()
      await page.waitForFunction(() => { const node = document.querySelector('.wb-message-scroll'); return node.scrollHeight - node.scrollTop - node.clientHeight <= 2 })
      assert.equal(await page.getByRole('button', { name: '有新消息 · 回到底部', exact: true }).count(), 0)
      await log.evaluate(node => { node.scrollTop *= 0.5; node.dispatchEvent(new Event('scroll')) })
      const rewritten = '等' + '底部跟随只在读者已经贴底时继续'.slice(1)
      fixture.emit({ type: 'message_end', message: { entryId: 'scroll-bottom-follow', role: 'assistant', content: rewritten } })
      await page.getByText(rewritten, { exact: true }).waitFor()
      await page.getByRole('button', { name: '有新消息 · 回到底部', exact: true }).waitFor()
    })
    await check('reading anchor and follow intent are isolated per real session when switching away and back', async ({ page, open, rendered }) => {
      for (let index = 0; index < 32; index++) {
        fixture.emit({ type: 'message_end', message: { entryId: `architecture-scroll-${index}`, role: 'assistant', content: `architecture-scroll-${index} independent session history` } }, 'architecture')
      }
      await open()
      const log = page.locator('.wb-message-scroll')
      await page.getByText('scroll-base-0', { exact: false }).first().waitFor()
      await log.evaluate(node => { node.scrollTop = Math.round((node.scrollHeight - node.clientHeight) * 0.31); node.dispatchEvent(new Event('scroll')) })
      const darkAnchor = await page.evaluate(() => {
        const log = document.querySelector('.wb-message-scroll'), top = log.getBoundingClientRect().top
        const node = [...log.querySelectorAll('[data-conversation-anchor]')].find(item => item.getBoundingClientRect().bottom > top + 1)
        return { key: node?.dataset.conversationAnchor, top: node?.getBoundingClientRect().top - top }
      })
      await page.getByRole('button', { name: '梳理 Agent 架构', exact: true }).click()
      await page.getByText('architecture-scroll-0', { exact: false }).first().waitFor()
      await rendered()
      await log.evaluate(node => { node.scrollTop = Math.round((node.scrollHeight - node.clientHeight) * 0.62); node.dispatchEvent(new Event('scroll')) })
      const architectureTop = await log.evaluate(node => node.scrollTop)
      assert(architectureTop > 0)
      const architectureAnchor = await log.evaluate(log => {
        const top = log.getBoundingClientRect().top
        const node = [...log.querySelectorAll('[data-conversation-anchor]')].find(item => item.getBoundingClientRect().bottom > top + 1)
        return { key: node.dataset.conversationAnchor, top: node.getBoundingClientRect().top - top }
      })
      await page.getByRole('button', { name: '为工作台添加深色模式', exact: true }).click()
      await page.getByText('scroll-new-history', { exact: false }).first().waitFor()
      await rendered()
      const restored = await page.evaluate(key => {
        const log = document.querySelector('.wb-message-scroll'), top = log.getBoundingClientRect().top
        const node = [...log.querySelectorAll('[data-conversation-anchor]')].find(item => item.dataset.conversationAnchor === key)
        return { top: node?.getBoundingClientRect().top - top, scrollTop: log.scrollTop }
      }, darkAnchor.key)
      assert(Math.abs(restored.top - darkAnchor.top) < 2, JSON.stringify({ darkAnchor, restored }))
      assert.equal(await page.getByRole('button', { name: '有新消息 · 回到底部', exact: true }).count(), 0)
      await page.getByRole('button', { name: '回到底部', exact: true }).waitFor()
      await page.getByRole('button', { name: '打开显示选项', exact: true }).click()
      await page.getByRole('combobox', { name: '消息布局', exact: true }).selectOption('classic')
      await page.keyboard.press('Escape')
      await page.setViewportSize({ width: 390, height: 844 })
      await page.waitForFunction(({ key, expected }) => {
        const log = document.querySelector('.wb-message-scroll'), top = log.getBoundingClientRect().top
        const node = [...log.querySelectorAll('[data-conversation-anchor]')].find(item => item.dataset.conversationAnchor === key)
        return node && Math.abs(node.getBoundingClientRect().top - top - expected) < 2
      }, { key: darkAnchor.key, expected: darkAnchor.top }, { timeout: 5000 })
      await rendered()
      const reflowed = await page.evaluate(key => {
        const log = document.querySelector('.wb-message-scroll'), top = log.getBoundingClientRect().top
        const node = [...log.querySelectorAll('[data-conversation-anchor]')].find(item => item.dataset.conversationAnchor === key)
        return node.getBoundingClientRect().top - top
      }, darkAnchor.key)
      assert(Math.abs(reflowed - darkAnchor.top) < 2, JSON.stringify({ darkAnchor, reflowed }))
      assert.equal(await page.getByRole('button', { name: '有新消息 · 回到底部', exact: true }).count(), 0)
      await page.getByRole('button', { name: '回到底部', exact: true }).click()
      await page.waitForFunction(() => { const node = document.querySelector('.wb-message-scroll'); return node.scrollHeight - node.scrollTop - node.clientHeight <= 2 })
      await page.setViewportSize({ width: 1440, height: 940 })
      await page.getByRole('button', { name: '梳理 Agent 架构', exact: true }).click()
      await page.getByText('architecture-scroll-0', { exact: false }).waitFor()
      await page.waitForFunction(({ key, top }) => {
        const log = document.querySelector('.wb-message-scroll')
        const node = [...log.querySelectorAll('[data-conversation-anchor]')].find(item => item.dataset.conversationAnchor === key)
        return node && Math.abs(node.getBoundingClientRect().top - log.getBoundingClientRect().top - top) < 2
      }, architectureAnchor)
      await page.getByRole('button', { name: '为工作台添加深色模式', exact: true }).click()
      await page.getByText('scroll-new-history', { exact: false }).waitFor()
      await page.waitForFunction(() => { const node = document.querySelector('.wb-message-scroll'); return node.scrollHeight - node.scrollTop - node.clientHeight <= 2 })
    })
    await check('process expansion, page navigation and session restore preserve the reading anchor, draft and real iframe', async ({ page, open, rendered }) => {
      await open()
      const log = page.locator('.wb-message-scroll')
      const group = page.locator('[data-activity-key="activity/tool/layout-intent"]')
      const tool = page.locator('[data-tool-id="layout-intent"]')
      await page.getByRole('textbox', { name: '预览地址', exact: true }).fill(siteUrl)
      await page.getByRole('button', { name: '打开', exact: true }).click()
      const previewDraft = page.frameLocator('.real-browser iframe').getByLabel('Preview draft')
      await previewDraft.fill('scroll must not reload the page')
      const loads = previewLoads
      await page.locator('textarea').fill('滚动与页面切换保留草稿选区')
      await page.locator('textarea').evaluate(node => node.setSelectionRange(2, 6))
      await log.evaluate(node => { node.scrollTop = Math.round((node.scrollHeight - node.clientHeight) * 0.5); node.dispatchEvent(new Event('scroll')) })
      const anchor = await log.evaluate(node => {
        const top = node.getBoundingClientRect().top
        const item = [...node.querySelectorAll('[data-conversation-anchor]')].find(item => item.getBoundingClientRect().bottom > top + 1)
        return { key: item.dataset.conversationAnchor, top: item.getBoundingClientRect().top - top }
      })
      // DOM activation avoids Playwright scrolling an offscreen summary into
      // view; we are measuring the layout effect, not a second reader gesture.
      await group.locator(':scope > summary').evaluate(node => { if (node.parentElement.open) node.click() })
      await rendered()
      await group.locator(':scope > summary').evaluate(node => node.click())
      await tool.locator('[data-slot="collapsible-trigger"]').evaluate(node => node.click())
      await rendered()
      const assertAnchor = async () => {
        await page.waitForFunction(({ key, top }) => {
          const log = document.querySelector('.wb-message-scroll')
          const item = [...log.querySelectorAll('[data-conversation-anchor]')].find(item => item.dataset.conversationAnchor === key)
          return item && Math.abs(item.getBoundingClientRect().top - log.getBoundingClientRect().top - top) < 2
        }, anchor)
      }
      await assertAnchor()
      assert.equal(await tool.locator('[data-slot="collapsible-trigger"]').getAttribute('aria-expanded'), 'true')
      await page.getByRole('button', { name: '工作台设置', exact: true }).click()
      await page.locator('.wb-settings-content').evaluate(node => { node.scrollTop = 80 })
      await page.getByRole('button', { name: '关闭设置', exact: true }).click()
      await page.getByRole('button', { name: '会话', exact: true }).click()
      await assertAnchor()
      assert.equal(await tool.locator('[data-slot="collapsible-trigger"]').getAttribute('aria-expanded'), 'true')
      await page.getByRole('button', { name: '技能与插件', exact: true }).click()
      await page.getByRole('button', { name: '会话', exact: true }).click()
      await assertAnchor()
      await page.getByRole('button', { name: '梳理 Agent 架构', exact: true }).click()
      await page.getByText('architecture-scroll-0', { exact: false }).waitFor()
      await page.getByRole('button', { name: '为工作台添加深色模式', exact: true }).click()
      await assertAnchor()
      assert.notEqual(await group.getAttribute('open'), null)
      assert.equal(await tool.locator('[data-slot="collapsible-trigger"]').getAttribute('aria-expanded'), 'true')
      assert.equal(await page.getByRole('button', { name: '有新消息 · 回到底部', exact: true }).count(), 0)
      assert.equal(await previewDraft.inputValue(), 'scroll must not reload the page')
      assert.equal(previewLoads, loads)
      assert.deepEqual(await page.locator('textarea').evaluate(node => [node.value, node.selectionStart, node.selectionEnd]), ['滚动与页面切换保留草稿选区', 2, 6])
    })
    await check('identical session and message IDs on different nodes cannot share reading position or follow intent', async ({ page, open, rendered }) => {
      const other = await startFixture(0, { nativeQueries: true, includeHistoryAssistant: true })
      try {
        other.setMessages([
          { entryId: 'user-dark', role: 'user', content: '另一个节点相同会话ID的独立正文' },
          ...Array.from({ length: 40 }, (_, index) => ({ entryId: `scroll-base-${index}`, role: 'assistant', content: `other-node-${index} independent reading` })),
        ])
        await open()
        const log = page.locator('.wb-message-scroll')
        await log.evaluate(node => { node.scrollTop = Math.round((node.scrollHeight - node.clientHeight) * 0.36); node.dispatchEvent(new Event('scroll')) })
        const saved = await log.evaluate(node => node.scrollTop)
        const connect = async target => {
          await page.getByRole('button', { name: '断开节点', exact: true }).click()
          await page.getByRole('textbox', { name: '节点地址', exact: true }).fill(target)
          await page.getByRole('button', { name: '连接节点', exact: true }).click()
          await page.getByText('在线', { exact: true }).waitFor()
        }
        await connect(other.url)
        await page.getByText('other-node-39 independent reading', { exact: true }).waitFor()
        await page.waitForFunction(() => { const log = document.querySelector('.wb-message-scroll'); return log.scrollHeight - log.scrollTop - log.clientHeight < 2 })
        await log.evaluate(node => { node.scrollTop = Math.round((node.scrollHeight - node.clientHeight) * 0.72); node.dispatchEvent(new Event('scroll')) })
        await connect(origin)
        await page.getByText('scroll-new-history', { exact: false }).waitFor()
        await rendered()
        await page.waitForFunction(top => Math.abs(document.querySelector('.wb-message-scroll').scrollTop - top) < 2, saved)
        assert.equal(await page.getByRole('button', { name: '有新消息 · 回到底部', exact: true }).count(), 0)
      } finally { await other.close() }
    })
    await check('history-only tools recover real results once; redacted or opaque content never becomes an answer', async ({ page, open }) => {
      await open()
      fixture.emit({ type: 'message_end', message: { entryId: 'history-tools', role: 'assistant', content: [{ type: 'toolCall', id: 'history-call', name: 'bash', arguments: { command: 'pwd' } }] } })
      const tool = page.locator('[data-tool-id="history-call"]')
      await tool.getByText('结果未观察', { exact: true }).waitFor({ state: 'attached' })
      assert.equal(await tool.locator('.text-emerald-600').count(), 0)
      fixture.emit({ type: 'message_end', message: { entryId: 'history-result', role: 'toolResult', toolCallId: 'history-call', isError: false, content: [{ type: 'text', text: 'restored historical result' }] } })
      await tool.getByText('已完成', { exact: true }).waitFor({ state: 'attached' })
      await tool.getByRole('button').click()
      await page.getByText('restored historical result', { exact: true }).waitFor()
      fixture.emit({ type: 'message_end', message: { entryId: 'history-public', role: 'assistant', content: [{ type: 'thinking', redacted: true, thinking: 'do-not-show-reasoning', text: 'do-not-show-reasoning', signature: 'do-not-show-signature' }, { type: 'opaque', text: 'do-not-show-opaque' }, { type: 'text', text: '权威历史中的公开正文' }] } })
      await page.getByText('权威历史中的公开正文', { exact: true }).waitFor()
      assert.doesNotMatch(await page.locator('.wb-transcript').textContent(), /do-not-show-/)
      await page.reload()
      await page.getByText('权威历史中的公开正文', { exact: true }).waitFor()
      assert.equal(await page.locator('[data-tool-id="history-call"]').count(), 1)
      assert.doesNotMatch(await page.locator('.wb-transcript').textContent(), /do-not-show-/)
      fixture.controls.accessStatus = 404
      try {
        await page.reload()
        await page.getByText('权威历史中的公开正文', { exact: true }).waitFor()
        assert.equal(await page.locator('[data-tool-id="history-call"]').count(), 1)
        assert.doesNotMatch(await page.locator('.wb-transcript').textContent(), /do-not-show-/)
        assert.equal(await page.getByRole('status', { name: '会话运行观察' }).textContent(), '会话观察：尚无运行阶段观察')
        assert.equal(await page.getByRole('button', { name: '发送', exact: true }).isDisabled(), true)
      } finally { delete fixture.controls.accessStatus }
    })
    assert.deepEqual(failures, [], 'reading layout regressions remain')
  } finally {
    if (browser) await browser.close()
    if (preview.exitCode === null && preview.signalCode === null) await new Promise(resolve => { preview.once('exit', resolve); preview.kill('SIGTERM') })
    await fixture.close()
    await new Promise(resolve => site.close(resolve))
  }
})().catch(error => { console.error(error); process.exitCode = 1 })
