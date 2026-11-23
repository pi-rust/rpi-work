const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { withQuery } = require('./verify-url.cjs');

async function checkTurnHierarchy(browser, url) {
  const page = await browser.newPage({ viewport: { width: 1440, height: 940 }, deviceScaleFactor: 1, colorScheme: 'light', reducedMotion: 'reduce' });
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  const button = name => page.getByRole('button', { name, exact: true });
  const draft = page.locator('#draft');
  const turns = page.locator('.demo-turn');
  const captures = [];
  const shot = async name => {
    const directory = process.env.RPI_WORKBENCH_SHOTS;
    if (!directory) return;
    await page.frameLocator('iframe[title="页面预览"]').getByRole('heading', { name: '项目概览', exact: true }).waitFor();
    await page.mouse.move(0, 0);
    await page.screenshot({ path: path.join(directory, name + '.png') });
    const geometry = await turns.first().locator(':scope > .turn-response').evaluate(node => {
      const reading = document.querySelector('#transcript').getBoundingClientRect();
      const answer = node.getBoundingClientRect();
      return {
        viewport: { width: innerWidth, height: innerHeight, dpr: devicePixelRatio },
        theme: document.documentElement.dataset.theme, layout: document.documentElement.dataset.messageLayout,
        density: document.documentElement.dataset.density, font: getComputedStyle(node).font,
        reading: { x: reading.x, width: reading.width }, answer: { x: answer.x, width: answer.width },
      };
    });
    captures.push({ name, ...geometry });
    fs.writeFileSync(path.join(directory, 'turn-hierarchy-metrics.json'), JSON.stringify({ browser: browser.version(), platform: process.platform, captures }, null, 2) + '\n');
  };
  const checkAnswer = async () => {
    const bounds = await turns.first().locator(':scope > .turn-response').evaluate(node => {
      const answer = node.getBoundingClientRect();
      const reading = document.querySelector('#transcript').getBoundingClientRect();
      return { x: answer.x, width: answer.width, readingX: reading.x, readingWidth: reading.width };
    });
    assert.equal(bounds.x, bounds.readingX);
    assert.equal(bounds.width, bounds.readingWidth);
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false);
  };
  try {
    await page.clock.install();
    await page.goto(withQuery(url, { view: 'new' }));
    await draft.fill('整理消息区的视觉层次');
    await draft.press('Enter');
    const threadId = await page.locator('.thread-button[aria-current="true"]').getAttribute('data-session');
    await page.clock.runFor(700);
    assert.equal(await turns.first().locator('.turn-plan').isVisible(), true);
    assert.equal(await turns.first().locator('.turn-execution').count(), 0);
    await checkAnswer();
    await shot('turn-running-light');
    await page.clock.runFor(1100);
    const execution = turns.first().locator('.turn-execution');
    assert.equal(await execution.evaluate(node => node.open), false);
    assert.equal(await turns.first().locator('.turn-plan').isVisible(), false);
    assert.equal(await turns.first().locator(':scope > .turn-response').isVisible(), true);
    await checkAnswer();
    await shot('turn-complete-light');

    for (const [theme, layout] of [['dark', 'reading'], ['dark', 'classic'], ['light', 'classic']]) {
      await button('设置').click();
      await button('外观').click();
      await page.locator('#theme').selectOption(theme);
      await page.getByLabel('消息布局', { exact: true }).selectOption(layout);
      await button('关闭设置').click();
      await checkAnswer();
      assert.equal(await execution.evaluate(node => node.open), false);
      await shot('turn-complete-' + theme + '-' + layout);
    }
    for (const width of [1440, 1280, 1024]) {
      await page.setViewportSize({ width, height: 940 });
      await checkAnswer();
    }
    await page.setViewportSize({ width: 1440, height: 940 });

    // Native disclosure keyboard interaction must not hide the final answer.
    await execution.locator(':scope > summary').focus();
    await page.keyboard.press('Enter');
    assert.equal(await turns.first().locator('.turn-plan').isVisible(), true);
    await turns.first().locator('details.activity summary').click();
    await execution.locator(':scope > summary').focus();
    await page.keyboard.press('Enter');
    await page.locator('[data-session="connection"]').click();
    await page.locator('[data-session="' + threadId + '"]').click();
    assert.equal(await execution.evaluate(node => node.open), false);
    assert.equal(await turns.first().locator(':scope > .turn-response').isVisible(), true);
    await execution.locator(':scope > summary').click();
    assert.equal(await turns.first().locator('details.activity').evaluate(node => node.open), true);
    await shot('turn-complete-expanded');

    // Finishing in the background preserves a tool record the reader opened.
    await draft.fill('完成时保留正在查看的工具记录');
    await draft.press('Enter');
    await page.clock.runFor(700);
    await turns.last().locator('details.activity summary').click();
    await page.locator('[data-session="connection"]').click();
    await page.clock.runFor(1100);
    await page.locator('[data-session="' + threadId + '"]').click();
    assert.equal(await turns.last().locator('.turn-execution').evaluate(node => node.open), true);
    assert.equal(await turns.last().locator('details.activity').evaluate(node => node.open), true);

    await page.getByLabel('工作模式', { exact: true }).selectOption('plan');
    await draft.fill('整理验证计划');
    await draft.press('Enter');
    await page.clock.runFor(1800);
    assert.equal(await turns.last().locator('.turn-execution').evaluate(node => node.open), false);
    assert.equal(await button('开始实施').isVisible(), true);
    assert.equal(await button('开始实施').evaluate(node => node.parentElement.classList.contains('demo-turn')), true);
    await button('开始实施').click();
    await page.clock.runFor(1800);

    await button('设置').click();
    await button('常规').click();
    await page.getByLabel('下一轮演示', { exact: true }).selectOption('failure');
    await button('关闭设置').click();
    await draft.fill('保留失败后的重试入口');
    await draft.press('Enter');
    await page.clock.runFor(700);
    assert.equal(await turns.last().locator('.turn-execution').count(), 0);
    assert.equal(await button('重试').isVisible(), true);
    await shot('turn-failure');
    assert.deepEqual(errors, []);
    console.log('Execution/answer hierarchy, collapsed completion, keyboard disclosure, desktop geometry, themes/layouts, open-tool retention and visible plan/retry actions passed.');
  } finally {
    await page.close();
  }
}

module.exports = { checkTurnHierarchy };
