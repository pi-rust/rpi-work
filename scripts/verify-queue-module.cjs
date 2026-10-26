const fs=require('node:fs'),assert=require('node:assert/strict'),path=require('node:path');
const {chromium}=require(require.resolve('playwright',{paths:[process.cwd(),path.resolve('../rpi-package/packages/rpi-web/frontend')]}));
const {startFixture}=require('./ui-fixture.cjs');
(async()=>{
 const fixture=await startFixture(),browser=await chromium.launch({headless:true,executablePath:'C:/Users/bigfish/scoop/apps/googlechrome/current/chrome.exe'}),page=await browser.newPage({viewport:{width:1100,height:950}});page.setDefaultTimeout(6500);
 const results=[],errors=[];page.on('pageerror',e=>errors.push(e.message));const pass=name=>{results.push(name);console.log('PASS',name)};
 try{
  await page.goto('http://127.0.0.1:4173/target/parity-modules-queue-web/');await page.getByRole('button',{name:'queue',exact:true}).click();await page.getByLabel('Fixture node URL').fill(fixture.url);await page.getByRole('button',{name:'Connect fixture',exact:true}).click();
  assert.equal((await page.locator('.review-queue-shelf').boundingBox()).height,0);
  await page.getByRole('button',{name:'打开消息队列',exact:true}).click();const dialog=page.getByRole('dialog',{name:'消息队列',exact:true});await dialog.getByText(/队列为空/).waitFor();assert(await dialog.evaluate(e=>e.matches(':modal')));
  pass('empty queue consumes no shelf height and opens a native modal');
  const draft=dialog.getByLabel('队列消息草稿',{exact:true}),delivery=dialog.getByLabel('消息投递方式',{exact:true});
  await draft.fill('引导当前轮次');await delivery.selectOption('steer');await dialog.getByRole('button',{name:'加入队列',exact:true}).click();await dialog.getByText('引导当前轮次',{exact:true}).waitFor();await page.waitForFunction(()=>document.querySelector('[aria-label="队列消息草稿"]')?.value==='');
  await draft.fill('完成后跟进');await delivery.selectOption('followUp');await dialog.getByRole('button',{name:'加入队列',exact:true}).click();await dialog.getByText('完成后跟进',{exact:true}).waitFor();
  assert(fixture.requests.some(r=>r.body.type==='queue_message'&&r.body.deliverAs==='steer'));assert(fixture.requests.some(r=>r.body.type==='queue_message'&&r.body.deliverAs==='followUp'));assert.equal(fixture.requests.filter(r=>r.body.type==='prompt').length,0);
  pass('both delivery modes use queue_message and clear only confirmed draft');
  await dialog.locator('.queue-item').filter({hasText:'引导当前轮次'}).getByRole('button',{name:'取消此队列消息',exact:true}).click();await dialog.getByText('引导当前轮次',{exact:true}).waitFor({state:'hidden'});
  await dialog.getByRole('button',{name:'清空待执行',exact:true}).click();await dialog.getByText(/队列为空/).waitFor();
  pass('single cancel and clear wait for actual queue readback');
  fixture.controls.failQueue=true;await draft.fill('失败后保留');await dialog.getByRole('button',{name:'加入队列',exact:true}).click();await dialog.getByRole('alert').filter({hasText:/queue rejected/}).waitFor();assert.equal(await draft.inputValue(),'失败后保留');
  await page.keyboard.press('Escape');await dialog.waitFor({state:'hidden'});await page.getByRole('button',{name:'打开消息队列',exact:true}).click();assert.equal(await draft.inputValue(),'失败后保留');fixture.controls.failQueue=false;
  pass('failed enqueue and closing dialog preserve scoped draft');
  fixture.controls.queueDelayMs=180;await draft.fill('本次提交');await dialog.getByRole('button',{name:'加入队列',exact:true}).click();await draft.fill('尚未发送的新草稿');await dialog.getByText('本次提交',{exact:true}).waitFor();assert.equal(await draft.inputValue(),'尚未发送的新草稿');fixture.controls.queueDelayMs=0;
  pass('late success cannot erase edits typed after submission');
  await page.keyboard.press('Escape');await page.getByRole('button',{name:'Session B',exact:true}).click();await page.getByRole('button',{name:'打开消息队列',exact:true}).click();await dialog.getByText(/队列为空/).waitFor();assert.equal(await draft.inputValue(),'');await page.keyboard.press('Escape');await page.getByRole('button',{name:'Session A',exact:true}).click();await page.getByRole('button',{name:'打开消息队列',exact:true}).first().click();await dialog.getByText('本次提交',{exact:true}).waitFor();assert.equal(await draft.inputValue(),'尚未发送的新草稿');
  pass('queue contents and drafts stay isolated by real session and local scope');
  fixture.setQueue([{entryId:'claimed-1',queue:'steer',text:'已经被预留',status:'reserved'},{entryId:'done-1',queue:'followUp',text:'已经消费',status:'completed'},{entryId:'unknown-1',queue:'followUp',text:'状态未提供',status:null}]);await dialog.getByRole('button',{name:'刷新',exact:true}).click();await dialog.getByText('已经被预留',{exact:true}).waitFor();for(const text of ['已经被预留','已经消费','状态未提供'])assert(await dialog.locator('.queue-item').filter({hasText:text}).getByRole('button',{name:'取消此队列消息',exact:true}).isDisabled());
  pass('reserved, completed and unknown items cannot be falsely cancelled');
  for(const width of [390,320]){await page.setViewportSize({width,height:844});await page.screenshot({path:`artifacts/ui-queue-module-${width}.png`});assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth));}
  pass('390/320 queue dialog remains bounded');assert.deepEqual(errors,[]);pass('no application JavaScript errors');fs.writeFileSync('artifacts/queue-module-validation.json',JSON.stringify({scope:'test-only queue component harness; parent SSE and busy composer integration required',results,errors},null,2));
 }catch(e){await page.screenshot({path:'artifacts/ui-queue-module-failure.png'});fs.writeFileSync('artifacts/queue-module-validation.json',JSON.stringify({results,errors,fatal:e.message},null,2));throw e;}
 finally{await browser.close();await fixture.close();}
})().catch(e=>{console.error(e);process.exitCode=1});
