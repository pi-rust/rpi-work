const assert = require('node:assert/strict');
const path = require('node:path');
const { pathToFileURL } = require('node:url');
const { withQuery } = require('./verify-url.cjs');

async function checkCollaboration(browser, url) {
  const page = await browser.newPage({ viewport: { width: 1440, height: 940 }, reducedMotion: 'reduce' });
  const errors = [], requests = [];
  page.on('pageerror', error => errors.push(error.message));
  page.on('request', request => { if (!request.url().startsWith('file:')) requests.push(request.url()); });
  const button = name => page.getByRole('button', { name, exact: true });
  const row = id => page.locator(`#connection-list [data-collab-node="${id}"]`);
  const modal = page.locator('#collab-dialog');
  const saved = () => page.evaluate(() => Object.entries(localStorage).map(([key, value]) => { try { return [key, JSON.parse(value)]; } catch { return []; } }).find(([, value]) => value?.collaboration)?.[1]);
  const scenario = async value => { await page.locator('.collab-demo').evaluate(node => { node.open = true; }); await page.locator('#collab-scenario').selectOption(value); };
  const close = async () => { await button('关闭协同弹窗').click(); await page.clock.runFor(30); await page.waitForFunction(() => !document.querySelector('#collab-dialog-body').children.length); };
  const show = async () => { await button('设置').click(); await button('工作协同').click(); };
  const manual = async address => { await button('输入地址').click(); await page.getByLabel('节点地址', { exact: true }).fill(address); await button('预览节点').click(); };
  try {
    await page.clock.install();
    await page.goto(withQuery(url, { runtime: 'ready' }));
    await page.clock.runFor(800);
    await page.locator('#draft').fill('协同设置不改变草稿');
    await page.locator('#draft').evaluate(input => input.setSelectionRange(2, 5));
    const ids = await page.locator('#recent-threads [data-session]').evaluateAll(nodes => nodes.map(node => node.dataset.session));
    await page.evaluate(() => { window.collabTranscript = document.querySelector('#transcript'); window.collabFrame = document.querySelector('iframe'); document.querySelector('.conversation').scrollTop = 240; window.collabScroll = document.querySelector('.conversation').scrollTop; });
    await show();
    assert.equal(await page.locator('#collab-current-name').textContent(), '此电脑');
    assert.equal(await button('一次性授权').isDisabled(), true);
    assert.equal(await page.locator('#collab-disconnect').isVisible(), false);
    for (const label of ['允许远程协作', '允许 Web 访问', '允许局域网发现']) assert.equal(await page.getByRole('switch', { name: label, exact: true }).isChecked(), false);
    await page.getByRole('switch', { name: '允许 Web 访问', exact: true }).check();
    assert.equal(await page.getByRole('switch', { name: '允许远程协作', exact: true }).isChecked(), false);
    assert.equal(await button('一次性授权').isDisabled(), true);
    await page.getByRole('switch', { name: '允许远程协作', exact: true }).check();
    await button('一次性授权').click();
    assert.match(await modal.innerText(), /120 秒/);
    await page.clock.runFor(120100);
    assert.match(await modal.innerText(), /已过期/);
    assert.equal(await modal.locator('.collab-code-placeholder').isVisible(), false);
    await button('重新生成演示').click();
    assert.equal(await modal.locator('.collab-code-placeholder').isVisible(), true);
    await close();
    assert.equal(await button('一次性授权').evaluate(node => node === document.activeElement), true);
    await button('地址二维码').click();
    assert.match(await modal.innerText(), /不可扫码/);
    assert.equal(await modal.locator('canvas, img, svg').count(), 1, 'Only the close icon; no misleading QR');
    await page.keyboard.press('Escape'); await page.clock.runFor(30);
    assert.equal(await page.locator('#settings-dialog').evaluate(node => node.open), true);
    await page.evaluate(() => { window.collabCopies = []; Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText: async text => { window.collabCopies.push(text); } } }); });
    await button('复制地址').click();
    assert.deepEqual(await page.evaluate(() => window.collabCopies), ['http://192.0.2.10:13031/']);
    await page.evaluate(() => { Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText() { throw Error('denied'); } } }); });
    await button('复制地址').click(); assert.match(await page.locator('#collab-feedback').textContent(), /无法复制/);

    await button('刷新发现').click();
    assert.equal(await button('刷新发现').isDisabled(), true);
    await page.clock.runFor(600);
    assert.equal(await page.locator('#collab-discovered .connection-row').count(), 2);
    assert.equal(await page.getByRole('switch', { name: '允许局域网发现', exact: true }).isChecked(), false);
    await scenario('discovery-empty'); await button('刷新发现').click(); await page.clock.runFor(600);
    assert.match(await page.locator('#collab-discovered').textContent(), /没有发现/);
    await scenario('service-unavailable');
    assert.equal(await page.getByRole('switch', { name: '允许远程协作', exact: true }).isDisabled(), true);
    assert.equal(await page.getByRole('switch', { name: '允许 Web 访问', exact: true }).isDisabled(), true);
    assert.equal((await saved()).collaboration.sharing, true);
    await scenario('connect-error');
    await row('devbox').getByRole('button', { name: '模拟连接', exact: true }).click(); await page.clock.runFor(600);
    assert.match(await row('devbox').locator('.connection-status').textContent(), /连接失败/);
    assert.notEqual((await saved()).connections.devbox, true);
    await manual('https://worker.example:13031/');
    await button('确认模拟连接').click(); await page.clock.runFor(600);
    assert.match(await modal.innerText(), /连接失败/);
    assert.equal((await saved()).collaboration.nodes.length, 0);
    await close(); await scenario('success');
    for (const address of ['javascript:alert(1)', 'https://user:sample@worker.example/', 'https://worker.example/?ticket=sample', 'https://worker.example/#sample']) {
      await manual(address); assert.equal(await modal.locator('[data-collab-action="confirm"]').count(), 0); assert.match(await modal.innerText(), /请输入/); await close();
    }
    await manual('https://cancel.example:13031/'); await button('确认模拟连接').click();
    await page.evaluate(() => document.querySelector('#collab-dialog').close()); await page.clock.runFor(700);
    assert.equal((await saved()).collaboration.nodes.length, 0);
    await page.waitForFunction(() => !document.querySelector('#collab-dialog-body').children.length);
    await manual('https://stale.example:13031/'); await button('确认模拟连接').click();
    await page.getByLabel('节点地址', { exact: true }).fill('https://changed.example/'); await page.clock.runFor(700);
    assert.equal((await saved()).collaboration.nodes.length, 0); await close();

    await row('devbox').getByRole('button', { name: '模拟连接', exact: true }).click(); await page.clock.runFor(600);
    assert.equal(await page.locator('#collab-current-name').textContent(), '此电脑');
    await row('devbox').getByRole('button', { name: '切换', exact: true }).click();
    assert.equal(await page.locator('#collab-current-name').textContent(), '开发主机');
    assert.equal(await page.locator('#collab-local-access').isVisible(), false);
    assert.equal(await page.locator('#collab-remote-note').isVisible(), true);
    assert.equal(await button('一次性授权').isDisabled(), true);
    await row('devbox').getByRole('button', { name: '设置', exact: true }).click(); await modal.getByRole('button', { name: '模拟断开', exact: true }).click(); await page.clock.runFor(30);
    assert.equal(await page.locator('#collab-current-name').textContent(), '此电脑');
    assert.match(await page.locator('#collab-recent').innerText(), /开发主机/);
    await scenario('visitors'); assert.match(await page.locator('#collab-visitor-list').textContent(), /Android 手机/);
    await button('撤销演示授权').click(); assert.match(await page.locator('#collab-visitor-list').textContent(), /暂无访问记录/);
    await scenario('success');
    await manual('https://worker.example:13031/'); await button('确认模拟连接').click(); await page.clock.runFor(600);
    await page.waitForFunction(() => !document.querySelector('#collab-dialog-body').children.length);
    assert.equal(await modal.evaluate(node => node.open), false);
    assert.equal(await page.locator('#collab-current-name').textContent(), '此电脑');
    const custom = (await saved()).collaboration.nodes[0]; assert.equal(custom.address, 'https://worker.example:13031/');
    assert.equal(await page.locator(`#project-node option[value="${custom.id}"]`).count(), 1);
    await row(custom.id).getByRole('button', { name: '切换', exact: true }).click();
    await button('关闭设置').click();
    assert.equal(await page.locator('#draft').inputValue(), '协同设置不改变草稿');
    assert.deepEqual(await page.locator('#draft').evaluate(input => [input.selectionStart, input.selectionEnd]), [2, 5]);
    assert.equal(await page.evaluate(() => window.collabTranscript === document.querySelector('#transcript') && window.collabFrame === document.querySelector('iframe') && window.collabScroll === document.querySelector('.conversation').scrollTop), true);
    assert.deepEqual(await page.locator('#recent-threads [data-session]').evaluateAll(nodes => nodes.map(node => node.dataset.session)), ids);
    await page.reload(); await page.clock.runFor(800); await show();
    assert.equal(await page.locator('#collab-current-name').textContent(), 'worker.example');
    assert.equal(await page.locator(`#project-node option[value="${custom.id}"]`).count(), 1);
    await row('local').getByRole('button', { name: '切换', exact: true }).click();
    assert.equal(await page.getByRole('switch', { name: '允许 Web 访问', exact: true }).isChecked(), true);
    await page.getByRole('switch', { name: '允许远程协作', exact: true }).uncheck();
    assert.equal(await page.getByRole('switch', { name: '允许 Web 访问', exact: true }).isChecked(), true);
    await button('关闭设置').click(); await button('设置').click();
    await page.getByLabel('搜索设置', { exact: true }).fill('允许远程协作'); await page.getByLabel('搜索设置', { exact: true }).press('Enter');
    assert.equal(await page.locator('#collab-local-access').isVisible(), true);
    for (const theme of ['light', 'dark']) {
      await page.emulateMedia({ colorScheme: theme });
      for (const width of [1440, 1024, 700, 390, 320]) {
        await page.setViewportSize({ width, height: 940 });
        assert.equal(await page.locator('#settings-dialog').evaluate(node => node.scrollWidth > node.clientWidth + 1), false, `settings overflow at ${width}`);
        assert.equal(await page.locator('.settings-content').evaluate(node => node.scrollWidth > node.clientWidth + 1), false, `content overflow at ${width}`);
        assert.equal(await page.locator('.collab-visitors').isVisible(), width > 700);
        assert.equal(await page.locator('.collab-scan').isVisible(), width <= 700);
      }
    }
    await button('扫码连接').click(); assert.match(await modal.innerText(), /不启动摄像头/); await close();
    await page.setViewportSize({ width: 1440, height: 940 });
    await button('关闭设置').click();
    await page.evaluate(() => {
      const entry = Object.entries(localStorage).find(([, value]) => { try { return JSON.parse(value)?.collaboration; } catch { return false; } });
      const data = JSON.parse(entry[1]); data.connections.local = false;
      data.collaboration = { current: 'missing', sharing: 'yes', web: [], discoverable: {}, nodes: [{ id: 'node-invalid', address: 'https://user:sample@private.example/' }] };
      localStorage.setItem(entry[0], JSON.stringify(data));
    });
    await page.reload(); await page.clock.runFor(800); await show();
    assert.equal(await page.locator('#collab-current-name').textContent(), '此电脑');
    assert.equal(await page.getByRole('switch', { name: '允许远程协作', exact: true }).isChecked(), false);
    assert.equal(await row('node-invalid').count(), 0);
    assert.deepEqual(errors, []); assert.deepEqual(requests, [], 'No network scans, downloads, camera or authentication requests');
  } finally { await page.close(); }
  console.log('Collaboration node selection, independent access, discovery, safe address previews, failure/cancel/stale guards, expiring demo authorization, clipboard stubs, persistence, responsive themes and conversation retention passed.');
}
module.exports = { checkCollaboration };
if (require.main === module) {
  const { chromium } = require(process.env.RPI_LAYOUT_PLAYWRIGHT ?? '../../tools/node_modules/playwright');
  (async () => { const browser = await chromium.launch(); try { await checkCollaboration(browser, withQuery(pathToFileURL(path.join(__dirname, 'index.html')).href, { runtime: 'ready' })); } finally { await browser.close(); } })().catch(error => { console.error(error); process.exitCode = 1; });
}
