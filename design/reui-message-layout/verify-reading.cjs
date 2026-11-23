const assert = require('node:assert/strict');
const { withQuery } = require('./verify-url.cjs');

async function checkReadingRestoration(browser, url) {
  const page = await browser.newPage({ viewport: { width: 1440, height: 940 } });
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  const conversation = page.locator('.conversation');
  const top = () => conversation.evaluate(node => node.scrollTop);
  const scroll = value => conversation.evaluate((node, value) => { node.scrollTop = value; }, value);
  const thread = id => page.locator('[data-session="' + id + '"]');
  const initial = page.locator('details[data-activity="initial"]');
  const button = name => page.getByRole('button', { name, exact: true });
  const nearly = (actual, expected) => assert.ok(Math.abs(actual - expected) <= 1, `${actual} should match ${expected}`);
  try {
    await page.clock.install();
    await page.goto(url);
    await initial.locator('summary').click();
    await scroll(55);
    await thread('connection').click();
    await thread('layout').click();
    nearly(await top(), 55);
    assert.equal(await initial.evaluate(node => node.open), true);
    await button('上一个会话').click();
    await button('下一个会话').click();
    nearly(await top(), 55);
    assert.equal(await initial.evaluate(node => node.open), true);

    await initial.locator('summary').click();
    await scroll(75);
    await page.locator('.rail-button[data-page="resources"]').click();
    await page.locator('.rail-button[data-page="chat"]').click();
    nearly(await top(), 75);
    await thread('connection').click();
    await thread('layout').click();
    nearly(await top(), 75);
    assert.equal(await initial.evaluate(node => node.open), false);

    const draft = page.locator('#draft');
    await draft.fill('验证上翻时保留阅读位置');
    await draft.press('Enter');
    await page.clock.runFor(700);
    await page.locator('.demo-turn details.activity summary').click();
    await scroll(0);
    await page.clock.runFor(1100);
    nearly(await top(), 0);
    assert.equal(await page.locator('.demo-turn details.activity').evaluate(node => node.open), true);
    await thread('connection').click();
    await thread('layout').click();
    nearly(await top(), 0);
    assert.equal(await page.locator('.demo-turn details.activity').evaluate(node => node.open), true);

    await draft.fill('后台执行第二轮');
    await draft.press('Enter');
    await draft.fill('排队执行第三轮');
    await draft.press('Enter');
    await conversation.evaluate(node => { node.scrollTop = node.scrollHeight; });
    await thread('connection').click();
    await page.clock.runFor(3600);
    await thread('layout').click();
    assert.equal(await page.locator('.demo-turn').count(), 3);
    nearly(await conversation.evaluate(node => node.scrollHeight - node.clientHeight - node.scrollTop), 0);

    // Width/font/layout changes while away restore a semantic anchor, not only px.
    await scroll(55);
    const offset = await page.locator('.prompt').first().evaluate(node => node.getBoundingClientRect().top);
    await thread('connection').click();
    await button('设置').click();
    await button('外观').click();
    await page.getByLabel('消息布局', { exact: true }).selectOption('classic');
    await page.getByLabel('消息字号', { exact: true }).selectOption('18');
    await button('关闭设置').click();
    await thread('layout').click();
    nearly(await page.locator('.prompt').first().evaluate(node => node.getBoundingClientRect().top), offset);

    await page.goto(withQuery(url, { history: 'loading' }));
    await page.clock.runFor(800);
    await initial.locator('summary').click();
    await scroll(70);
    await thread('connection').click();
    assert.equal(await conversation.getAttribute('data-history'), 'loading');
    await thread('layout').click();
    nearly(await top(), 70);
    assert.equal(await initial.evaluate(node => node.open), true);
    await page.clock.runFor(800);
    nearly(await top(), 70);
    assert.equal(await conversation.getAttribute('data-history'), 'ready');
    assert.deepEqual(errors, []);
    console.log('Per-chat reading anchors, tool expansion, history navigation, resources, background streaming and bottom-follow restoration passed.');
  } finally {
    await page.close();
  }
}

module.exports = { checkReadingRestoration };
