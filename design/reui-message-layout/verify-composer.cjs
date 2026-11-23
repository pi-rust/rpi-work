const assert = require('node:assert/strict');

async function checkComposerCaret(browser, url) {
  const page = await browser.newPage({ viewport: { width: 1440, height: 940 } });
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  const draft = page.locator('#draft');
  const menu = page.locator('#composer-menu');
  try {
    await page.goto(url);
    await draft.fill('保留前文 @tran');
    assert.equal(await menu.isVisible(), true);
    await draft.press('Home');
    assert.equal(await menu.isVisible(), false);
    assert.equal(await draft.inputValue(), '保留前文 @tran');
    await draft.press('End');
    assert.equal(await menu.isVisible(), true);
    await draft.press('Shift+Home');
    assert.equal(await menu.isVisible(), false);
    for (const key of ['Shift+ArrowUp', 'Shift+ArrowDown']) {
      await draft.fill('保留前文\n@tran');
      if (key === 'Shift+ArrowDown') await draft.evaluate(node => node.setSelectionRange(node.value.length - 2, node.value.length - 2));
      await draft.press(key);
      assert.equal(await draft.evaluate(node => node.selectionStart !== node.selectionEnd), true);
      assert.equal(await menu.isVisible(), false);
      assert.equal(await draft.inputValue(), '保留前文\n@tran');
    }
    await draft.fill('/');
    await draft.press('ArrowDown');
    assert.equal(await menu.getByRole('option', { selected: true }).textContent(), '/plan切换计划模式');
    await draft.press('ArrowUp');
    assert.equal(await menu.getByRole('option', { selected: true }).textContent(), '/goal设定目标');

    // A pointer selection may race caret changes; never apply a stale range.
    await draft.fill('保留前文 @tran');
    await draft.evaluate(node => node.setSelectionRange(0, 0));
    await menu.getByRole('option').first().click();
    assert.equal(await menu.isVisible(), false);
    assert.equal(await draft.inputValue(), '保留前文 @tran');
    assert.equal(await page.locator('#attachments').textContent(), '');
    await draft.fill('保留前文 @tran');
    await draft.press('Tab');
    assert.equal(await draft.inputValue(), '保留前文 ');
    assert.match(await page.locator('#attachments').textContent(), /src\/components\/transcript\.tsx/);

    await draft.fill('/plan');
    assert.equal(await menu.isVisible(), true);
    await draft.dispatchEvent('compositionstart', { data: '' });
    assert.equal(await menu.isVisible(), false);
    await draft.dispatchEvent('keydown', { key: 'Enter', code: 'Enter', isComposing: true, keyCode: 229 });
    assert.equal(await page.locator('.demo-turn').count(), 0);
    assert.equal(await draft.inputValue(), '/plan');
    await draft.dispatchEvent('compositionend', { data: '' });
    assert.equal(await menu.isVisible(), true);
    await draft.press('Enter');
    assert.equal(await page.getByLabel('工作模式', { exact: true }).inputValue(), 'plan');
    assert.equal(await draft.inputValue(), '');
    assert.equal(await page.locator('.demo-turn').count(), 0);
    assert.deepEqual(errors, []);
    console.log('Composer caret navigation, stale mention ranges, prefix retention and IME/menu isolation passed.');
  } finally {
    await page.close();
  }
}

module.exports = { checkComposerCaret };
