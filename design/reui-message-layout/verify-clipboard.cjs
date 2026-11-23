const assert = require('node:assert/strict');
const path = require('node:path');

async function checkClipboardFeedback(browser, url) {
  const page = await browser.newPage({ viewport: { width: 1440, height: 940 }, reducedMotion: 'reduce' });
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  await page.addInitScript(() => {
    const probe = window.clipboardProbe = { mode: 'resolve', calls: [], pending: [], legacyCalls: 0 };
    const clipboard = { writeText(text) {
      probe.calls.push(text);
      if (probe.mode === 'reject') return Promise.reject(new DOMException('Denied', 'NotAllowedError'));
      if (probe.mode === 'throw') throw new Error('Synchronous clipboard error');
      if (probe.mode === 'pending') return new Promise((resolve, reject) => probe.pending.push({ resolve, reject }));
      return Promise.resolve();
    } };
    // Never fall through to the host clipboard, including legacy copy paths.
    Object.defineProperty(navigator, 'clipboard', { configurable: true, get: () => probe.mode === 'missing' ? undefined : clipboard });
    document.execCommand = () => { probe.legacyCalls++; throw new Error('Host clipboard access is forbidden in this regression'); };
  });
  const button = name => page.getByRole('button', { name, exact: true });
  const codeCopy = page.locator('[data-action="copy-code"]');
  const diffCopy = page.locator('.diff-actions [data-action="copy"]');
  const draft = page.locator('#draft');
  const mode = value => page.evaluate(value => { clipboardProbe.mode = value; }, value);
  const lastPayload = () => page.evaluate(() => clipboardProbe.calls.at(-1));
  const copy = async (target, expected) => {
    const count = await page.evaluate(() => clipboardProbe.calls.length);
    await target.click();
    await page.waitForFunction(count => clipboardProbe.calls.length === count + 1, count);
    assert.equal(await lastPayload(), expected);
  };
  const toast = async text => {
    await page.waitForFunction(text => !document.querySelector('#toast').hidden && document.querySelector('#toast').textContent === text, text);
  };
  const diffPayload = () => page.evaluate(() => [document.querySelector('.diff-range').textContent,
    ...[...document.querySelectorAll('.diff-line code')].map(node => node.textContent[0] + node.textContent.slice(2))].join('\n'));
  const shot = async name => {
    if (process.env.RPI_WORKBENCH_SHOTS) await page.screenshot({ path: path.join(process.env.RPI_WORKBENCH_SHOTS, name + '.png') });
  };
  try {
    await page.clock.install();
    await page.goto(url);
    const code = (await page.locator('.message-code .code-line').allTextContents()).join('\n');
    const answer = await page.locator('.reply').first().evaluate(node => [node.querySelector('h2').innerText,
      node.querySelector(':scope > p').innerText, node.querySelector(':scope > ul').innerText].join('\n\n')) + '\n\n' + code;
    await draft.fill('复制时保留的草稿');
    await draft.evaluate(node => node.setSelectionRange(2, 5));
    await copy(codeCopy, code);
    await toast('已复制');
    await copy(button('复制回答').first(), answer);
    for (const label of ['复制代码', '本次修改 ·', '在浏览器中打开']) assert.equal((await lastPayload()).includes(label), false);

    await button('设置').click();
    await button('外观').click();
    await page.getByRole('switch', { name: '代码行号', exact: true }).uncheck();
    await page.getByLabel('消息布局', { exact: true }).selectOption('classic');
    await page.locator('#theme').selectOption('dark');
    await button('关闭设置').click();
    await copy(codeCopy, code);
    await copy(button('复制回答').first(), answer);
    assert.equal(await draft.inputValue(), '复制时保留的草稿');
    assert.deepEqual(await draft.evaluate(node => [node.selectionStart, node.selectionEnd]), [2, 5]);

    for (const value of ['reject', 'missing', 'throw']) {
      await mode(value);
      await codeCopy.click();
      await toast('复制失败，请选择文字后手动复制');
    }
    await shot('copy-code-failure');
    await mode('resolve');
    await copy(codeCopy, code);
    await toast('已复制');

    await page.getByRole('tab', { name: '变更', exact: true }).click();
    const initialIcon = await diffCopy.innerHTML();
    const expectedDiff = await diffPayload();
    for (const theme of ['dark', 'light']) {
      if (theme === 'light') {
        await button('设置').click();
        await button('外观').click();
        await page.locator('#theme').selectOption(theme);
        await button('关闭设置').click();
      }
      await mode('resolve');
      await copy(diffCopy, expectedDiff);
      await page.waitForFunction(() => document.querySelector('.diff-actions [data-action="copy"]').title === '已复制差异片段');
      assert.notEqual(await diffCopy.innerHTML(), initialIcon);
      for (const value of ['reject', 'missing', 'throw']) {
        await mode(value);
        await diffCopy.click();
        await page.waitForFunction(() => document.querySelector('.diff-actions [data-action="copy"]').title === '无法复制，可手动选择差异文本');
        assert.equal(await diffCopy.innerHTML(), initialIcon, 'A failed retry must not retain the success checkmark');
      }
      await diffCopy.hover();
      await shot('copy-diff-retry-failure-' + theme);
      await mode('resolve');
      await copy(diffCopy, expectedDiff);
      await page.waitForFunction(() => document.querySelector('.diff-actions [data-action="copy"]').title === '已复制差异片段');
    }

    await page.clock.runFor(3100);
    await mode('pending');
    await codeCopy.click();
    assert.equal(await page.locator('#toast').getAttribute('hidden'), '');
    await draft.focus();
    await draft.evaluate(node => node.setSelectionRange(2, 5));
    const scroll = await page.locator('.conversation').evaluate(node => node.scrollTop);
    await page.evaluate(() => clipboardProbe.pending.shift().resolve());
    await toast('已复制');
    assert.equal(await draft.evaluate(node => document.activeElement === node), true);
    assert.equal(await draft.inputValue(), '复制时保留的草稿');
    assert.deepEqual(await draft.evaluate(node => [node.selectionStart, node.selectionEnd]), [2, 5]);
    assert.equal(await page.locator('.conversation').evaluate(node => node.scrollTop), scroll);

    await diffCopy.click();
    const oldButton = await diffCopy.elementHandle();
    const originalDiff = await lastPayload();
    await page.locator('.review-file[data-file="1"]').click();
    assert.equal(await oldButton.evaluate(node => node.isConnected), false);
    await page.evaluate(() => clipboardProbe.pending.shift().resolve());
    assert.equal(await diffCopy.getAttribute('aria-label'), '复制差异片段', 'A completed old-file request must not mark the new file as copied');
    assert.equal(originalDiff, expectedDiff);
    await mode('resolve');
    await copy(diffCopy, await diffPayload());

    await page.locator('.diff-source').click();
    const source = (await page.locator('.source-code .source-line code').allTextContents()).join('\n');
    assert.ok(source.length > 0);
    await copy(page.locator('[data-action="copy-source"]'), source);
    await toast('已复制');

    await draft.fill('核对新回答的复制');
    await draft.press('Enter');
    await page.clock.runFor(1800);
    const response = await page.locator('.demo-turn > .turn-response').last().innerText();
    await draft.fill('复制后仍然保留的草稿');
    await copy(page.locator('.demo-turn [data-action="copy-reply"]').last(), response);
    assert.equal(await draft.inputValue(), '复制后仍然保留的草稿');
    assert.equal(await page.evaluate(() => clipboardProbe.legacyCalls), 0);
    assert.deepEqual(errors, []);
    console.log('Clipboard stub payloads, answer/control separation, diff success-to-failure recovery, rejected/missing/synchronous APIs, delayed focus/draft/scroll retention and file-switch ownership passed without host clipboard writes.');
  } finally {
    await page.close();
  }
}

module.exports = { checkClipboardFeedback };
