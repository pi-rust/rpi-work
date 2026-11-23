const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

async function checkDiffReading(browser, url) {
  const page = await browser.newPage({ viewport: { width: 1440, height: 940 }, deviceScaleFactor: 1, reducedMotion: 'reduce' });
  const errors = [], measurements = [];
  page.on('pageerror', error => errors.push(error.message));
  const button = name => page.getByRole('button', { name, exact: true });
  const lines = page.locator('.diff-lines');
  // Stress only presentation in this page; keep file paths, line locations and
  // review state from the existing fixture. No new simulated backend behavior.
  const stress = async () => {
    await page.locator('.diff-line code').first().evaluate(node => node.textContent = '  const path = "' + 'long_component_name_'.repeat(24) + '";');
    await page.locator('.diff-source').evaluate(node => node.textContent = 'long readable filename '.repeat(20) + '.tsx');
  };
  const geometry = async label => {
    await page.mouse.move(0, 0);
    const result = await lines.evaluate(node => {
      const rect = node.getBoundingClientRect();
      const clip = node.closest('.inspector-content').getBoundingClientRect();
      return {
        left: rect.left, right: rect.right, width: node.clientWidth,
        scroll: node.scrollLeft, scrollWidth: node.scrollWidth,
        pageOverflow: document.documentElement.scrollWidth > innerWidth,
        rows: [...node.querySelectorAll('.diff-line')].map(row => {
          const oldLine = row.children[0], newLine = row.children[1], action = row.lastElementChild;
          const box = action.getBoundingClientRect();
          const center = document.elementFromPoint(box.x + box.width / 2, box.y + box.height / 2);
          return {
            oldLeft: oldLine.getBoundingClientRect().left, newLeft: newLine.getBoundingClientRect().left,
            actionLeft: box.left, actionRight: box.right,
            hitDetails: { top: box.top, bottom: box.bottom, covering: center?.outerHTML.slice(0, 140) },
            // Only hit-test centers inside the vertical review viewport; a
            // partially clipped row can have its center below the screenshot.
            hit: box.y + box.height / 2 >= Math.max(0, clip.top) && box.y + box.height / 2 < Math.min(innerHeight, clip.bottom)
              ? center?.closest('.line-comment') === action : true,
            rowColor: row.matches('.add,.remove') ? getComputedStyle(row).backgroundColor : getComputedStyle(document.body).backgroundColor,
            gutterColor: getComputedStyle(oldLine).backgroundColor, actionColor: getComputedStyle(action).backgroundColor,
          };
        }),
        notes: [...node.querySelectorAll('.inline-feedback,.inline-finding')].map(note => {
          const box = note.getBoundingClientRect();
          return { left: box.left, right: box.right, width: box.width, overflow: note.scrollWidth > note.clientWidth };
        }),
      };
    });
    assert.ok(result.scrollWidth > result.width, 'The long code fixture must overflow locally');
    assert.equal(result.pageOverflow, false);
    for (const row of result.rows) {
      assert.ok(Math.abs(row.oldLeft - result.left) < 1);
      assert.ok(Math.abs(row.newLeft - result.left - 23) < 1);
      assert.ok(Math.abs(row.actionRight - result.right) < 1, 'Every row action must remain at the viewport edge');
      assert.ok(row.actionLeft >= result.left + 46);
      assert.equal(row.hit, true, label + ': Code must not cover the sticky action hit target: ' + JSON.stringify(row.hitDetails));
      assert.equal(row.gutterColor, row.rowColor);
      assert.equal(row.actionColor, row.rowColor);
    }
    for (const note of result.notes) {
      assert.ok(Math.abs(note.left - result.left) < 1);
      assert.ok(Math.abs(note.width - result.width) < 1);
      assert.ok(note.right <= result.right + 1);
      assert.equal(note.overflow, false, 'Review notes must wrap at the visible width, not the full code width');
    }
    const header = await page.locator('.diff-caption').boundingBox();
    assert.equal(header.height, 38);
    assert.equal(await page.locator('.diff-source').evaluate(node => getComputedStyle(node).whiteSpace), 'nowrap');
    measurements.push({ label, width: result.width, scroll: result.scroll, actionRight: result.rows[0].actionRight, notes: result.notes });
  };
  const positions = async label => {
    for (const fraction of [0, .5, 1]) {
      await lines.evaluate((node, value) => { node.scrollLeft = (node.scrollWidth - node.clientWidth) * value; }, fraction);
      await geometry(label + '-' + fraction);
    }
  };
  const shot = async name => {
    if (process.env.RPI_WORKBENCH_SHOTS) await page.screenshot({ path: path.join(process.env.RPI_WORKBENCH_SHOTS, name + '.png') });
  };
  try {
    await page.clock.install();
    await page.goto(url);
    await page.getByRole('tab', { name: '变更', exact: true }).click();
    await page.locator('#draft').fill('保留差异阅读时的草稿');
    await stress();
    await positions('initial-light');
    await lines.evaluate(node => { node.scrollLeft = 0; });
    const firstAction = page.locator('.diff-line .line-comment').first();
    await firstAction.hover();
    assert.equal(await firstAction.locator('.icon').evaluate(node => getComputedStyle(node).opacity), '1');
    await firstAction.click();
    assert.equal(await page.locator('#comment-location').textContent(), 'src/components/transcript.tsx:83');
    await button('关闭行内反馈').click();

    await lines.focus();
    await page.keyboard.press('ArrowRight');
    await page.waitForFunction(() => document.querySelector('.diff-lines').scrollLeft > 0);
    const added = page.locator('.diff-line.add .line-comment').first();
    await added.focus();
    await page.keyboard.press('Enter');
    assert.equal(await page.locator('#comment-location').textContent(), 'src/components/transcript.tsx:84');
    await page.keyboard.press('Escape');
    assert.equal(await page.locator('#comment-dialog').getAttribute('open'), null);
    const removed = page.locator('.diff-line.remove .line-comment').first();
    await removed.focus();
    assert.equal(await removed.locator('.icon').evaluate(node => getComputedStyle(node).opacity), '1');
    await page.keyboard.press('Enter');
    assert.equal(await page.locator('#comment-location').textContent(), 'src/components/transcript.tsx:84（旧）');
    const feedback = '这段删除行需要核对。'.repeat(16) + 'unbroken_feedback_path_'.repeat(32);
    await page.locator('#review-comment').fill(feedback);
    await button('加入下一条消息').click();
    assert.equal(await page.locator('.inline-feedback').textContent(), feedback);
    assert.equal(await page.locator('#draft').inputValue(), '保留差异阅读时的草稿');
    await button('运行审阅').click();
    await page.clock.runFor(1800);
    assert.equal(await page.locator('.inline-finding').count(), 1);
    await stress();
    for (const theme of ['light', 'dark']) {
      await button('设置').click();
      await button('外观').click();
      await page.locator('#theme').selectOption(theme);
      await button('关闭设置').click();
      for (const width of [1440, 1280, 1024]) {
        await page.setViewportSize({ width, height: 940 });
        await positions(theme + '-' + width);
        await lines.evaluate(node => { node.scrollLeft = (node.scrollWidth - node.clientWidth) / 2; });
        await firstAction.hover();
        await shot('diff-sticky-' + theme + '-' + width);
      }
    }
    await page.setViewportSize({ width: 1440, height: 940 });
    const divider = page.locator('.inspector-divider');
    await divider.focus();
    await divider.press('Home');
    assert.equal(await divider.getAttribute('aria-valuenow'), '320');
    await positions('minimum-panel');
    await shot('diff-sticky-minimum-panel');
    await page.locator('.diff-line').last().scrollIntoViewIfNeeded();
    await positions('minimum-panel-bottom');
    assert.equal(await page.locator('.diff-source').getAttribute('title'), 'src/components/transcript.tsx');
    await button('关闭发现').click();
    assert.equal(await page.locator('.inline-finding').count(), 0);
    assert.equal(await page.locator('.inline-feedback').textContent(), feedback);
    assert.equal(await page.locator('#draft').inputValue(), '保留差异阅读时的草稿');
    assert.deepEqual(errors, []);
    if (process.env.RPI_WORKBENCH_SHOTS) fs.writeFileSync(path.join(process.env.RPI_WORKBENCH_SHOTS, 'diff-reading-metrics.json'), JSON.stringify(measurements, null, 2) + '\n');
    console.log('Diff sticky line numbers/actions, long filename ellipsis, independent horizontal scroll, wrapped feedback/findings, themes, desktop/minimum widths and mouse/keyboard line locations passed.');
  } finally {
    await page.close();
  }
}

module.exports = { checkDiffReading };
