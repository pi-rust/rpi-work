const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

async function checkMessageTypography(browser, url) {
  const page = await browser.newPage({ viewport: { width: 1440, height: 940 }, deviceScaleFactor: 1, reducedMotion: 'reduce', colorScheme: 'light' });
  const errors = [], measurements = [];
  page.on('pageerror', error => errors.push(error.message));
  const button = name => page.getByRole('button', { name, exact: true });
  const draft = page.locator('#draft');
  const reply = page.locator('.demo-turn > .turn-response').last();
  const setAppearance = async ({ size, density, layout, theme }) => {
    await button('设置').click();
    await button('外观').click();
    await page.getByLabel('消息字号', { exact: true }).selectOption(String(size));
    await page.getByLabel('阅读密度', { exact: true }).selectOption(density);
    await page.getByLabel('消息布局', { exact: true }).selectOption(layout);
    await page.locator('#theme').selectOption(theme);
    await button('关闭设置').click();
  };
  const shot = async name => {
    if (process.env.RPI_WORKBENCH_SHOTS) await page.screenshot({ path: path.join(process.env.RPI_WORKBENCH_SHOTS, name + '.png') });
  };
  try {
    await page.clock.install();
    await page.goto(url);
    await draft.fill('检查正文密度');
    await draft.press('Enter');
    await page.clock.runFor(1800);
    assert.equal(await page.locator('.turn-execution').count(), 1);
    await page.locator('details[data-activity="initial"] > summary').click();
    await page.locator('.turn-execution > summary').click();
    await page.locator('.demo-turn details.activity > summary').click();
    await draft.fill('保留排版设置时的草稿');
    const content = await page.locator('#transcript').textContent();
    const original = await reply.elementHandle();
    const auxiliaryFonts = () => page.evaluate(() => ['.message-code pre', '.activity pre'].map(selector => {
      const style = getComputedStyle(document.querySelector(selector));
      return { size: style.fontSize, leading: style.lineHeight };
    }));
    const codeFonts = await auxiliaryFonts();
    for (const size of [16, 18]) for (const density of ['comfortable', 'compact']) {
      for (const layout of ['reading', 'classic']) for (const theme of ['light', 'dark']) {
        const settings = { size, density, layout, theme };
        await setAppearance(settings);
        const styles = await page.evaluate(() => [...document.querySelectorAll('.assistant > p, .reply > p, .reply > ul, .turn-response')].map(node => {
          const style = getComputedStyle(node);
          return { selector: node.className || node.tagName, size: parseFloat(style.fontSize), leading: parseFloat(style.lineHeight) };
        }));
        const leading = size * (density === 'compact' ? 1.55 : 1.65);
        for (const style of styles) {
          assert.equal(style.size, size);
          assert.ok(Math.abs(style.leading - leading) < .05, `${JSON.stringify(settings)} ${style.selector}: ${style.leading} != ${leading}`);
        }
        assert.deepEqual(await auxiliaryFonts(), codeFonts, 'Body density/font must not resize code or tool logs');
        assert.equal(await original.evaluate(node => node.isConnected), true, 'Appearance must not rebuild the reply');
        assert.equal(await page.locator('#transcript').textContent(), content);
        assert.equal(await draft.inputValue(), '保留排版设置时的草稿');
        for (const selector of ['details[data-activity="initial"]', '.turn-execution', '.demo-turn details.activity']) {
          assert.equal(await page.locator(selector).evaluate(node => node.open), true);
        }
        measurements.push({ ...settings, bodyLeading: leading, code: codeFonts });
        if ((size === 16 && layout === 'reading' && theme === 'light') || (size === 18 && layout === 'classic' && theme === 'dark')) {
          await reply.scrollIntoViewIfNeeded();
          await shot(`typography-${size}-${density}-${layout}-${theme}`);
        }
      }
    }

    await setAppearance({ size: 16, density: 'comfortable', layout: 'reading', theme: 'light' });
    const heading = page.locator('.reply h2').first();
    await heading.evaluate(node => {
      const conversation = document.querySelector('.conversation');
      conversation.scrollTop += node.getBoundingClientRect().top - conversation.getBoundingClientRect().top - 24;
    });
    const anchorTop = await heading.evaluate(node => node.getBoundingClientRect().top);
    await setAppearance({ size: 18, density: 'compact', layout: 'reading', theme: 'light' });
    const readingAnchor = { before: anchorTop, after: await heading.evaluate(node => node.getBoundingClientRect().top) };
    assert.ok(Math.abs(readingAnchor.after - readingAnchor.before) < 2, 'Density/font changes must retain the visible reading anchor');
    await page.locator('.conversation').evaluate(node => { node.scrollTop = node.scrollHeight; });
    let bottomGap;
    for (const [size, density] of [[16, 'comfortable'], [18, 'compact']]) {
      await setAppearance({ size, density, layout: 'reading', theme: 'light' });
      bottomGap = await page.locator('.conversation').evaluate(node => node.scrollHeight - node.scrollTop - node.clientHeight);
      assert.ok(bottomGap < 2, 'An answer already at the bottom must stay at the bottom after appearance changes');
    }

    // Test-only stress content. Do not extend the demo response generator or
    // persist synthetic long text as protocol/history data.
    const line = page.locator('.message-code .code-line').first();
    const codeText = await line.textContent();
    const responseText = await reply.textContent();
    try {
      await line.evaluate(node => node.textContent = 'const longIdentifier = "' + 'unbroken_identifier_'.repeat(32) + '";');
      await reply.evaluate(node => node.textContent = '正文应按阅读列换行，不跟随代码横向滚动。'.repeat(14) + 'https://example.invalid/' + 'unbroken_path_'.repeat(32));
      for (const width of [1440, 1280, 1024]) {
        await page.setViewportSize({ width, height: 940 });
        const geometry = await page.evaluate(() => {
          const pre = document.querySelector('.message-code pre');
          const response = document.querySelector('.turn-response');
          const reading = document.querySelector('#transcript').getBoundingClientRect();
          const answer = response.getBoundingClientRect();
          pre.scrollLeft = 100;
          return {
            pageOverflow: document.documentElement.scrollWidth > innerWidth,
            codeOverflow: pre.scrollWidth > pre.clientWidth, localScroll: pre.scrollLeft,
            bodyOverflow: response.scrollWidth > response.clientWidth,
            lines: answer.height / parseFloat(getComputedStyle(response).lineHeight),
            aligned: answer.x === reading.x && answer.width === reading.width,
          };
        });
        assert.equal(geometry.pageOverflow, false);
        assert.equal(geometry.bodyOverflow, false);
        assert.equal(geometry.codeOverflow, true);
        assert.ok(geometry.localScroll > 0);
        assert.ok(geometry.lines > 10);
        assert.equal(geometry.aligned, true);
        await page.locator('.message-code').scrollIntoViewIfNeeded();
        await shot('typography-local-code-scroll-' + width);
      }
    } finally {
      await line.evaluate((node, text) => node.textContent = text, codeText);
      await reply.evaluate((node, text) => node.textContent = text, responseText);
    }
    await page.setViewportSize({ width: 1440, height: 940 });
    await page.reload();
    assert.equal(await page.locator('html').getAttribute('data-density'), 'compact');
    assert.equal(await reply.evaluate(node => getComputedStyle(node).fontSize), '18px');
    assert.ok(Math.abs(await reply.evaluate(node => parseFloat(getComputedStyle(node).lineHeight)) - 27.9) < .05);
    assert.equal(await draft.inputValue(), '保留排版设置时的草稿');
    assert.deepEqual(errors, []);
    if (process.env.RPI_WORKBENCH_SHOTS) fs.writeFileSync(path.join(process.env.RPI_WORKBENCH_SHOTS, 'typography-metrics.json'), JSON.stringify({ cases: measurements, readingAnchor, bottomGap }, null, 2) + '\n');
    console.log('Consistent body density, 16/18px, themes/layouts, independent code/log fonts, reading anchor, DOM/draft/details retention, persistence and local code overflow passed.');
  } finally {
    await page.close();
  }
}

module.exports = { checkMessageTypography };
