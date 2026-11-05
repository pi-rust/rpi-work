// Replay a complete installed-native RPC turn, including its repeated turn_end body.
// Synthetic captured fixture + formal WASM only; no user session or real model.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');
const captured = require('../fixtures/native-short-reply.json');
(async () => {
  const fixture = await startFixture(0, { pagedHistory: true });
  fixture.setMessages(captured.messages.slice(0,1));
  const browser = await chromium.launch({headless:true,executablePath:process.env.CHROME_PATH});
  const page = await browser.newPage({viewport:{width:1440,height:1000}});
  page.setDefaultTimeout(12000);
  const results = [], errors = [];
  page.on('pageerror',e=>errors.push(e.message));
  await page.route('https://api.github.com/**',r=>r.fulfill({status:404,body:'{}'}));
  const reply = 'Hi! What can I help you with?';
  const exact = () => page.locator('.transcript').getByText(reply,{exact:true});
  const pass = text => {results.push(text);console.log('PASS',text);};
  try {
    await page.goto(process.env.WORK_UI_URL || 'http://127.0.0.1:14180/');
    await page.getByRole('button',{name:'打开设置',exact:true}).click();
    await page.getByRole('navigation',{name:'设置分类'}).getByRole('button',{name:'执行节点',exact:true}).click();
    await page.getByRole('button',{name:'连接其他节点',exact:true}).click();
    await page.getByLabel('节点地址',{exact:true}).fill(fixture.url);
    await page.getByRole('button',{name:'连接节点',exact:true}).click();
    await page.getByRole('status').filter({hasText:'在线'}).first().waitFor();
    await page.getByRole('button',{name:'返回会话',exact:true}).click();
    const selected = page.locator('[data-session-id="dark"] .session-item');
    if (!await selected.isVisible()) await page.locator('[data-project-id="rpi-work"] .project-row').click();
    await selected.click();
    await page.locator('[data-history-id="native-short-user"]').waitFor();
    await page.setViewportSize({width:390,height:844});
    for (const event of captured.events) {
      let refreshed;
      if (event.type === 'agent_settled') refreshed = page.waitForResponse(r=>r.request().method()==='GET'&&new URL(r.url()).pathname.endsWith('/dark/snapshot'));
      fixture.emit(event);
      if (event.type === 'agent_start') {
        await page.waitForFunction(()=>document.querySelector('.transcript'));
      }
      if (event.type === 'message_end') {
        fixture.setMessages(captured.messages);
        await page.locator('[data-history-id="native-short-assistant"]').waitFor();
        assert.equal(await exact().count(),1,'message_end has one reply before turn_end');
      }
      if (refreshed) {
        await (await refreshed).finished();
        await page.getByRole('button',{name:'发送消息',exact:true}).waitFor();
        // Status and body must have reached the completed event, not just HTTP response arrival.
        await page.waitForFunction(()=>document.querySelector('.run-status.completed, .task-status.completed, .status-badge.completed') || document.body.innerText.includes('已完成'));
      }
    }
    assert.equal(await exact().count(),1,'turn_end summary must not create a second live assistant row after settlement');
    assert.equal(await page.locator('[data-history-id="native-short-assistant"]').count(),1);
    assert.equal(await page.locator('[data-history-id^="live-assistant-"]').count(),0,'no fabricated summary message identity');
    pass('complete native turn including turn_end stays at one reply after settlement');
    await page.screenshot({path:'artifacts/turn-summary-mobile-fixed.png',animations:'disabled'});
    await page.getByRole('button',{name:'打开项目导航',exact:true}).click();
    await page.locator('[data-session-id="architecture"] .session-item').click();
    await page.getByRole('button',{name:'打开项目导航',exact:true}).click();
    await page.locator('[data-session-id="dark"] .session-item').click();
    await page.locator('[data-history-id="native-short-assistant"]').waitFor();
    assert.equal(await exact().count(),1,'reopen retains the same single reply');
    const refreshed = page.waitForResponse(r=>r.request().method()==='GET'&&new URL(r.url()).pathname.endsWith('/dark/snapshot'));
    fixture.disconnect();await (await refreshed).finished();
    await page.locator('[data-history-id="native-short-assistant"]').waitFor();
    assert.equal(await exact().count(),1);
    pass('reopen and reconnect keep the single persisted reply');
    const persisted = structuredClone(captured.messages);
    for (const [index,id] of ['real-repeat-a','real-repeat-b'].entries()) {
      const historyMessage = {...structuredClone(captured.messages[1]),entryId:id};
      persisted.push(historyMessage);
      for (const original of captured.events) {
        const event = structuredClone(original);
        if (event.messageId) event.messageId = id;
        let done;
        if (event.type === 'agent_settled') done = page.waitForResponse(r=>r.request().method()==='GET'&&new URL(r.url()).pathname.endsWith('/dark/snapshot'));
        fixture.emit(event);
        if (event.type === 'message_end') {
          fixture.setMessages(persisted);
          await page.locator(`[data-history-id="${id}"]`).waitFor();
        }
        if (done) { await (await done).finished(); await page.getByRole('button',{name:'发送消息',exact:true}).waitFor(); }
      }
      assert.equal(await exact().count(),index+2,'distinct real entries with identical content/timestamp must all remain');
      assert.equal(await page.locator('[data-history-id^="live-assistant-"]').count(),0);
    }
    pass('two more real entries with identical text and timestamps survive their own turn_end summaries');
    assert.equal(fixture.requests.filter(r=>['prompt','queue_message'].includes(r.body?.type)).length,0);
    assert.deepEqual(errors,[]);
  } catch(e) {
    await page.screenshot({path:'artifacts/turn-summary-mobile-failure.png',animations:'disabled'});
    throw e;
  } finally {
    fs.writeFileSync('artifacts/turn-summary-mobile-validation.json',JSON.stringify({scope:'Captured synthetic installed-native RPC turn + formal WASM mobile viewport; no real model or Android hardware',results,errors},null,2));
    await browser.close();await fixture.close();
  }
})().catch(e=>{console.error(e);process.exitCode=1;});
