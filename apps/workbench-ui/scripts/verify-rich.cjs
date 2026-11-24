// Product UI, real Rust client, isolated protocol/HTTP/clipboard fixtures.
const assert = require('node:assert/strict')
const path = require('node:path')
const net = require('node:net')
const http = require('node:http')
const fs = require('node:fs/promises')
const { spawn } = require('node:child_process')
const { chromium } = require(require.resolve('playwright', { paths: [path.resolve(__dirname, '../../../tools')] }))
const { startFixture } = require('../../../tests/fixtures/ui-fixture.cjs')
const root = path.resolve(__dirname, '..')
const freePort = () => new Promise(resolve => { const server = net.createServer(); server.listen(0, '127.0.0.1', () => { const port = server.address().port; server.close(() => resolve(port)) }) })
const wideCell = 'wide-table-cell-'.repeat(45)
const code = 'const ready = true;\n' + '// long canonical code '.repeat(75) + '\n'
const source = '## 富内容验收\n\n正文 **重要**、*强调*、~~删除~~ 与 `inline`。\n\n- 第一项\n- 第二项\n\n> 引用正文\n\n3. 第三步\n4. 第四步\n\n- [x] 已勾选\n- [ ] 未勾选\n\n| 名称 | 说明 |\n| --- | --- |\n| canonical | ' + wideCell + ' |\n\n```typescript\n' + code + '```\n\n结束正文。'
const readable = '富内容验收\n\n正文 重要、强调、删除 与 inline。\n\n第一项\n第二项\n\n引用正文\n\n第三步\n第四步\n\n[x] 已勾选\n[ ] 未勾选\n\n名称\t说明\ncanonical\t' + wideCell + '\n\n' + code.replace(/\n$/, '') + '\n\n结束正文。'
function codeContrasts(section) {
  const canvas = document.createElement('canvas'), ctx = canvas.getContext('2d')
  const rgb = color => { ctx.clearRect(0, 0, 1, 1); ctx.fillStyle = color; ctx.fillRect(0, 0, 1, 1); return [...ctx.getImageData(0, 0, 1, 1).data].slice(0, 3) }
  const luminance = values => values.map(value => { const c = value / 255; return c <= .04045 ? c / 12.92 : ((c + .055) / 1.055) ** 2.4 }).reduce((sum, value, index) => sum + value * [.2126, .7152, .0722][index], 0)
  const background = luminance(rgb(getComputedStyle(section).backgroundColor))
  return [...section.querySelectorAll('pre span[class], .wb-message-code-header > span')].map(node => {
    const foreground = luminance(rgb(getComputedStyle(node).color))
    return { class: node.className, ratio: (Math.max(background, foreground) + .05) / (Math.min(background, foreground) + .05) }
  })
}
;(async () => {
  const fixture = await startFixture(0, { nativeQueries: true, includeHistoryAssistant: true })
  const emit = (id, content) => fixture.emit({ type: 'message_end', message: { entryId: id, role: 'assistant', content } })
  let previewLoads = 0, trackLoads = 0
  const site = http.createServer((req, res) => {
    if (req.url === '/') previewLoads++; else trackLoads++
    res.setHeader('Content-Type', 'text/html')
    res.end('<!doctype html><html><body><label>Rich preview<input aria-label="Rich preview"></label></body></html>')
  })
  const artifacts = path.resolve(root, '../../artifacts/rich-answer')
  const failures = [], passed = []
  let browser, preview
  try {
    await fs.mkdir(artifacts, { recursive: true })
    await new Promise(resolve => site.listen(0, '127.0.0.1', resolve))
    const siteUrl = `http://127.0.0.1:${site.address().port}/`
    const port = await freePort(), origin = `http://127.0.0.1:${port}`
    preview = spawn(process.execPath, [path.join(root, 'node_modules/vite/bin/vite.js'), 'preview', '--host', '127.0.0.1', '--port', String(port), '--strictPort'], { cwd: root, env: { ...process.env, RPI_NODE_URL: fixture.url }, stdio: 'ignore' })
    let ready = false
    for (let i = 0; i < 120; i++) { try { if ((await fetch(origin)).ok) { ready = true; break } } catch {} await new Promise(resolve => setTimeout(resolve, 100)) }
    assert(ready, 'owned product preview started')
    browser = await chromium.launch()
    const check = async (name, run) => {
      emit('rich-history', source)
      const context = await browser.newContext({ viewport: { width: 1440, height: 940 }, deviceScaleFactor: 1, reducedMotion: 'reduce' })
      await context.addInitScript(() => {
        const probe = window.clipboardProbe = { mode: 'resolve', calls: [], pending: [], legacyCalls: 0 }
        const clipboard = { writeText(text) {
          probe.calls.push(text)
          if (probe.mode === 'reject') return Promise.reject(new DOMException('Denied', 'NotAllowedError'))
          if (probe.mode === 'throw') throw Error('Synchronous clipboard failure')
          if (probe.mode === 'pending') return new Promise((resolve, reject) => probe.pending.push({ resolve, reject }))
          return Promise.resolve()
        } }
        Object.defineProperty(navigator, 'clipboard', { configurable: true, get: () => probe.mode === 'missing' ? undefined : clipboard })
        document.execCommand = () => { probe.legacyCalls++; throw Error('Host clipboard is forbidden') }
      })
      const unexpected = []
      await context.route('**/*', route => {
        const url = route.request().url()
        if (!/^https?:/.test(url) || url.startsWith(origin + '/') || url.startsWith(siteUrl)) return route.continue()
        unexpected.push(url)
        return route.abort()
      })
      const page = await context.newPage(), errors = []
      page.setDefaultTimeout(10000)
      page.on('pageerror', error => errors.push(error.message))
      page.on('console', message => { if (message.type() === 'error' && !message.text().startsWith('Failed to load resource:')) errors.push(message.text()) })
      const heading = page.getByRole('heading', { name: '富内容验收', exact: true })
      const answer = page.locator('.wb-answer').filter({ has: heading })
      const rendered = () => page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))))
      const mode = value => page.evaluate(value => { clipboardProbe.mode = value }, value)
      const state = (button, value) => button.and(page.locator(`[data-copy-state="${value}"]`)).waitFor()
      const copy = async (button, expected) => {
        const count = await page.evaluate(() => clipboardProbe.calls.length)
        await button.click()
        await page.waitForFunction(count => clipboardProbe.calls.length === count + 1, count)
        assert.equal(await page.evaluate(() => clipboardProbe.calls.at(-1)), expected)
        await state(button, 'success')
      }
      const choose = async layout => {
        await page.getByRole('button', { name: '打开显示选项', exact: true }).click()
        await page.getByRole('combobox', { name: '消息布局', exact: true }).selectOption(layout)
        await page.keyboard.press('Escape')
        await rendered()
      }
      try {
        await page.goto(origin + '/?live=1')
        await page.getByText('帮我为工作台增加一个深色主题，并保持手机端可用。', { exact: true }).waitFor()
        await heading.waitFor()
        await run({ page, answer, rendered, mode, state, copy, choose, siteUrl })
        assert.deepEqual(errors, [], 'page/React errors')
        assert.deepEqual(unexpected, [], 'no external requests, including images')
        assert.equal(await page.evaluate(() => clipboardProbe.legacyCalls), 0)
        passed.push(name)
        console.log('PASS', name)
      } catch (error) {
        failures.push(name)
        console.error('FAIL', name, error.stack)
        console.error('page/React errors:', errors)
        await page.screenshot({ path: path.join(artifacts, `failed-${failures.length}.png`) }).catch(() => {})
      } finally { await context.close() }
    }
    await check('Rust Markdown DTO renders structured history and Rust tokens, not raw delimiters', async ({ answer }) => {
      assert.equal(await answer.locator('strong').innerText(), '重要')
      assert.equal(await answer.locator('em').innerText(), '强调')
      assert.equal(await answer.locator('del').innerText(), '删除')
      assert.equal(await answer.locator('blockquote').innerText(), '引用正文')
      assert.equal(await answer.locator('ol').getAttribute('start'), '3')
      assert.equal(await answer.locator('thead th').count(), 2)
      assert.equal(await answer.locator('tbody tr').count(), 1)
      for (const checkbox of await answer.getByRole('checkbox').all()) assert.equal(await checkbox.isDisabled(), true)
      assert.equal(await answer.getByRole('checkbox', { name: '已勾选的清单项（只读）', exact: true }).isChecked(), true)
      assert.equal(await answer.getByRole('checkbox', { name: '未勾选的清单项（只读）', exact: true }).isChecked(), false)
      assert.equal(await answer.locator('.wb-message-code pre code').textContent(), code)
      assert.equal(await answer.locator('.code-token-keyword').first().textContent(), 'const')
      assert.equal(await answer.locator('.wb-message-code + p').evaluate(node => getComputedStyle(node).marginTop), '16px')
    })
    await check('HTML stays literal, images stay inert and only explicit credential-free HTTP links navigate', async ({ page, siteUrl }) => {
      const before = trackLoads
      emit('rich-security', '## 安全正文\n\n<script>window.richAttack=1</script>\n\n<img src="' + siteUrl + 'track" onerror="window.richAttack=2">\n\n<iframe src="' + siteUrl + 'frame"></iframe>\n\n![像素 **说明**](' + siteUrl + 'track-image)\n\n[危险](javascript:alert%281%29) [编码](jav&#97;script:alert%281%29) [文件](file:///tmp/a) [相对](src/main.rs) [平台](rpi://execute) [凭据](https://user@example.org) [文档](https://example.org/docs)')
      const target = page.locator('.wb-answer').filter({ has: page.getByRole('heading', { name: '安全正文', exact: true }) })
      await target.waitFor()
      assert.equal(await target.locator('.wb-prose').locator('script,img,iframe,svg,style,form,video,audio').count(), 0)
      assert.equal(await page.evaluate(() => typeof window.richAttack), 'undefined')
      assert.equal(await target.locator('.wb-image-placeholder').innerText(), '图片（未加载）：像素 说明')
      assert.equal(await target.locator('a').count(), 1)
      const link = target.getByRole('link', { name: /文档.*新窗口/ })
      assert.equal(await link.getAttribute('href'), 'https://example.org/docs')
      assert.equal(await link.getAttribute('target'), '_blank')
      assert.equal(await link.getAttribute('rel'), 'noopener noreferrer')
      assert.equal(await link.getAttribute('referrerpolicy'), 'no-referrer')
      assert((await target.locator('.wb-prose').textContent()).includes('<script>window.richAttack=1</script>'))
      assert.equal(trackLoads, before)
    })
    await check('answer/code copy payloads are exact and rejected, missing or throwing APIs clear success', async ({ page, answer, mode, state, copy }) => {
      const answerCopy = answer.getByRole('button', { name: '复制回答', exact: true })
      const codeCopy = answer.getByRole('button', { name: '复制代码', exact: true })
      await copy(codeCopy, code)
      await copy(answerCopy, readable)
      for (const forbidden of ['复制代码', '复制回答', 'rpi', '工具结果']) assert.equal(readable.includes(forbidden), false)
      for (const button of [codeCopy, answerCopy]) {
        for (const failure of ['reject', 'missing', 'throw']) {
          await mode(failure)
          await button.click()
          await state(button, 'error')
          assert.equal(await button.locator('.lucide-check').count(), 0)
          assert.equal(await button.locator('..').getByRole('status').innerText(), '复制失败，请选择文字后手动复制')
        }
        await mode('resolve')
        await copy(button, button === codeCopy ? code : readable)
      }
      assert.equal(await page.evaluate(() => clipboardProbe.legacyCalls), 0)
    })
    await check('delayed clipboard completion keeps focus, draft selection, reading position and the real iframe', async ({ page, answer, mode, state, choose, siteUrl }) => {
      await page.getByRole('textbox', { name: '预览地址', exact: true }).fill(siteUrl)
      await page.getByRole('button', { name: '打开', exact: true }).click()
      const previewDraft = page.frameLocator('.real-browser iframe').getByLabel('Rich preview')
      await previewDraft.fill('preserve iframe form')
      const loads = previewLoads
      const draft = page.locator('textarea')
      await draft.fill('复制期间保留草稿与选区')
      const button = answer.getByRole('button', { name: '复制代码', exact: true })
      await mode('pending')
      await button.click()
      await state(button, 'pending')
      assert.equal(await button.isDisabled(), true)
      await draft.focus()
      await draft.evaluate(node => node.setSelectionRange(2, 7))
      const top = await page.locator('.wb-message-scroll').evaluate(node => node.scrollTop)
      await answer.evaluate(node => {
        window.richNodes = { prose: node.querySelector('.wb-prose'), code: node.querySelector('pre'), frame: document.querySelector('.real-browser iframe') }
        clipboardProbe.pending.shift().resolve()
      })
      await state(button, 'success')
      assert.equal(await draft.evaluate(node => node === document.activeElement), true)
      assert.deepEqual(await draft.evaluate(node => [node.value, node.selectionStart, node.selectionEnd]), ['复制期间保留草稿与选区', 2, 7])
      assert(Math.abs(await page.locator('.wb-message-scroll').evaluate(node => node.scrollTop) - top) < 2)
      await choose('classic')
      await page.getByRole('button', { name: '切换主题', exact: true }).click()
      await page.getByRole('button', { name: '工作台设置', exact: true }).click()
      await page.getByRole('button', { name: '关闭设置', exact: true }).click()
      assert.equal(await answer.evaluate(node => richNodes.prose === node.querySelector('.wb-prose') && richNodes.code === node.querySelector('pre') && richNodes.frame === document.querySelector('.real-browser iframe')), true)
      assert.equal(await previewDraft.inputValue(), 'preserve iframe form')
      assert.equal(previewLoads, loads)
      assert.deepEqual(await draft.evaluate(node => [node.selectionStart, node.selectionEnd]), [2, 7])
    })
    await check('A to B to A payload changes and session switches cannot adopt old clipboard feedback', async ({ page, answer, mode, state, copy }) => {
      const button = answer.getByRole('button', { name: '复制回答', exact: true })
      await copy(button, readable)
      const changeAndBack = async () => {
        emit('rich-history', source + '\n\n新载荷B')
        await answer.getByText('新载荷B', { exact: true }).waitFor()
        await state(button, 'idle')
        emit('rich-history', source)
        await answer.getByText('新载荷B', { exact: true }).waitFor({ state: 'detached' })
        await state(button, 'idle')
      }
      await changeAndBack()
      await mode('pending')
      await button.click()
      await state(button, 'pending')
      await changeAndBack()
      await page.evaluate(() => clipboardProbe.pending.shift().resolve())
      await state(button, 'idle')
      await button.click()
      await state(button, 'pending')
      const calls = await page.evaluate(() => clipboardProbe.calls.length)
      await page.getByRole('button', { name: '梳理 Agent 架构', exact: true }).click()
      await answer.waitFor({ state: 'detached' })
      await page.evaluate(() => clipboardProbe.pending.shift().resolve())
      await page.getByRole('button', { name: '为工作台添加深色模式', exact: true }).click()
      await answer.waitFor()
      await state(button, 'idle')
      assert.equal(await page.evaluate(() => clipboardProbe.calls.length), calls, 'never automatically repeat clipboard writes')
    })
    await check('unclosed streamed fences keep canonical DOM and local scrolling; reflow does not steal the reader', async ({ page, rendered }) => {
      const initial = '## 流式富内容\n\n```rust\n// ' + 'stream-code-'.repeat(110) + '\nfn main() {'
      fixture.emit({ type: 'agent_start' })
      fixture.emit({ type: 'message_update', message: { entryId: 'rich-live' }, eventType: 'text_delta', delta: initial })
      const target = page.locator('.wb-answer').filter({ has: page.getByRole('heading', { name: '流式富内容', exact: true }) })
      const pre = target.locator('pre')
      await pre.waitFor()
      await pre.evaluate(node => { node.scrollLeft = 120; window.streamNodes = { pre: node, prose: node.closest('.wb-prose'), anchor: node.closest('[data-conversation-anchor]') } })
      const tail = '\n    println!("ok");\n}\n```\n\n流式结尾'
      fixture.emit({ type: 'message_update', message: { entryId: 'rich-live' }, eventType: 'text_delta', delta: tail })
      await target.getByText('流式结尾', { exact: true }).waitFor()
      emit('rich-live', initial + tail + '\n\n最终历史')
      fixture.emit({ type: 'agent_settled', outcome: 'completed', pendingResume: false })
      await target.getByText('最终历史', { exact: true }).waitFor()
      assert.equal(await pre.evaluate(node => node === streamNodes.pre && node.closest('.wb-prose') === streamNodes.prose && node.closest('[data-conversation-anchor]') === streamNodes.anchor), true)
      assert((await pre.evaluate(node => node.scrollLeft)) >= 119)
      for (let index = 0; index < 24; index++) emit(`rich-scroll-${index}`, `## 阅读段 ${index}\n\n${'canonical paragraph '.repeat(25)}`)
      await page.getByRole('heading', { name: '阅读段 23', exact: true }).waitFor()
      const anchor = await page.locator('.wb-message-scroll').evaluate(log => {
        log.scrollTop = Math.round((log.scrollHeight - log.clientHeight) * .4)
        log.dispatchEvent(new Event('scroll'))
        const top = log.getBoundingClientRect().top
        const node = [...log.querySelectorAll('[data-conversation-anchor]')].find(item => item.getBoundingClientRect().bottom > top + 1)
        return { key: node.dataset.conversationAnchor, top: node.getBoundingClientRect().top - top }
      })
      emit('rich-history', source + '\n\n## 上方富内容更新\n\n' + 'changed above the reader '.repeat(75))
      await page.getByRole('heading', { name: '上方富内容更新', exact: true }).waitFor()
      await rendered()
      await page.waitForFunction(({ key, top }) => {
        const log = document.querySelector('.wb-message-scroll')
        const node = [...log.querySelectorAll('[data-conversation-anchor]')].find(item => item.dataset.conversationAnchor === key)
        return node && Math.abs(node.getBoundingClientRect().top - log.getBoundingClientRect().top - top) < 2
      }, anchor)
    })
    await check('both layouts contain long code/tables in light/dark 1440, 1024, 390 and 320px with keyboard focus and contrast', async ({ page, answer, rendered, choose }) => {
      const pre = answer.locator('.wb-message-code pre')
      for (const dark of [false, true]) {
        if (dark) await page.getByRole('button', { name: '切换主题', exact: true }).click()
        for (const width of [1440, 1024, 390, 320]) {
          await page.setViewportSize({ width, height: 940 })
          for (const layout of ['reading', 'classic']) {
            await choose(layout)
            const geometry = await answer.evaluate(node => {
              const pre = node.querySelector('pre'), table = node.querySelector('.wb-answer-table')
              return { overflow: document.documentElement.scrollWidth - innerWidth, proseFont: getComputedStyle(node.querySelector('.wb-prose')).fontSize, codeOverflow: pre.scrollWidth - pre.clientWidth, tableOverflow: table.scrollWidth - table.clientWidth }
            })
            assert(geometry.overflow <= 1 && geometry.codeOverflow > 100 && geometry.tableOverflow > 100, JSON.stringify({ dark, width, layout, geometry }))
            assert.equal(geometry.proseFont, '16px')
          }
          await answer.getByRole('button', { name: '复制代码', exact: true }).focus()
          await page.keyboard.press('Tab')
          assert.equal(await pre.evaluate(node => document.activeElement === node && node.matches(':focus-visible')), true)
          await pre.evaluate(node => { node.scrollLeft = 0 })
          // Native ArrowRight routing after resize is an opt-in platform
          // diagnostic, as in the confirmed prototype's verify-code-focus.
          if (process.env.RPI_WORKBENCH_NATIVE_SCROLL_PROBE === '1') {
            await page.keyboard.press('ArrowRight')
            await page.waitForFunction(node => node.scrollLeft > 0, await pre.elementHandle(), { timeout: 3000 })
          }
          await pre.evaluate(node => { node.scrollLeft = 120 })
          assert((await pre.evaluate(node => node.scrollLeft)) >= 119)
          assert.equal(await page.locator('.wb-message-scroll').evaluate(node => node.scrollLeft), 0)
          await page.keyboard.press('Tab')
          assert.equal(await pre.evaluate(node => document.activeElement === node), false)
          await page.keyboard.press('Shift+Tab')
          assert.equal(await pre.evaluate(node => document.activeElement === node), true)
          assert.equal(await pre.evaluate(node => getComputedStyle(node).outlineOffset), '-3px')
          assert.equal(await pre.evaluate(node => getComputedStyle(node).outlineStyle), 'solid')
          if ([1440, 320].includes(width)) await page.screenshot({ path: path.join(artifacts, `${dark ? 'dark' : 'light'}-${width}.png`) })
          if (width <= 390) assert((await answer.getByRole('button', { name: '复制代码', exact: true }).boundingBox()).height >= 44)
        }
        const contrasts = await answer.locator('.wb-message-code').evaluate(codeContrasts)
        for (const contrast of contrasts) assert(contrast.ratio >= 4.5, JSON.stringify({ dark, contrast }))
      }
      await rendered()
    })
    await check('every selectable Rust palette keeps code tokens and header text at 4.5:1 in both modes', async ({ page, answer, rendered }) => {
      for (const dark of [false, true]) {
        if (dark) await page.getByRole('button', { name: '切换主题', exact: true }).click()
        await page.getByRole('button', { name: '打开显示选项', exact: true }).click()
        const select = page.getByRole('combobox', { name: '选择主题', exact: true })
        const presets = await select.locator('option').evaluateAll(nodes => nodes.map(node => node.value))
        assert(presets.length >= 5)
        for (const preset of presets) {
          await select.selectOption(preset)
          await rendered()
          for (const contrast of await answer.locator('.wb-message-code').evaluate(codeContrasts)) assert(contrast.ratio >= 4.5, JSON.stringify({ preset, dark, contrast }))
        }
        await page.keyboard.press('Escape')
      }
    })
    await check('excess nesting falls back to full escaped source and keeps an exact copy without truncation', async ({ page, copy }) => {
      const deep = '> '.repeat(64) + '复杂内容原文 <img src="https://example.org/never-load">'
      emit('rich-fallback', deep)
      const target = page.locator('.wb-answer').filter({ has: page.locator('.wb-prose[data-plain="true"]') })
      await target.waitFor()
      assert.equal(await target.locator('.wb-prose').textContent(), deep)
      assert.equal(await target.locator('img').count(), 0)
      await target.getByText('内容较复杂，已按原文显示。', { exact: true }).waitFor()
      await copy(target.getByRole('button', { name: '复制回答', exact: true }), deep)
    })
    assert.equal(failures.length, 0, failures.join('\n'))
    console.log(`RESULT ${passed.length} checks passed`)
  } finally {
    if (browser) await browser.close()
    if (preview && preview.exitCode === null && preview.signalCode === null) await new Promise(resolve => { preview.once('exit', resolve); preview.kill('SIGTERM') })
    if (site.listening) await new Promise(resolve => site.close(resolve))
    await fixture.close()
  }
})().catch(error => { console.error(error.stack); process.exitCode = 1 })
