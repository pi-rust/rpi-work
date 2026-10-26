const assert=require('node:assert/strict'),path=require('node:path'),fs=require('node:fs');
const {chromium}=require(require.resolve('playwright',{paths:[process.cwd(),path.resolve('../rpi-package/packages/rpi-web/frontend')]}));
(async()=>{
 const browser=await chromium.launch({headless:true,executablePath:process.env.CHROME_PATH||'C:/Users/bigfish/scoop/apps/googlechrome/current/chrome.exe'}),page=await browser.newPage({viewport:{width:1440,height:1000}}),results=[],errors=[];
 page.on('pageerror',e=>errors.push(e.message));const pass=t=>{results.push(t);console.log('PASS '+t);};
 await page.goto(process.env.PROTOTYPE_URL||'http://127.0.0.1:4173/prototype/');
 const cases=[['completed','completed'],['aborted','interrupted'],['interrupted','interrupted'],['failed','failed'],['suspended','suspended'],['compacted','completed'],['declined','declined'],['skipped','declined'],['future_value','settled_unknown']];
 for(const [outcome,expected] of cases){
  const result=await page.evaluate(outcome=>{const run={id:'fixture-run',session:'fixture-session',phase:'awaiting_settlement',blocks:[],lastSeq:0,applied:[]};reduceStream(run,{runId:run.id,seq:1,event:{type:'agent_settled',outcome,pendingResume:outcome==='aborted'}});return {phase:run.phase,raw:run.outcome,pendingResume:run.pendingResume,terminal:streamTerminal(run)};},outcome);
  assert.equal(result.phase,expected);assert.equal(result.raw,outcome);assert.equal(result.terminal,true);assert.equal(result.pendingResume,outcome==='aborted');
 }
 pass('native completed/aborted/suspended/compacted/declined outcomes retain their meanings and recovery flag');
 await page.locator('#model-button').click();await page.locator('[data-model="OpenAI"]').click();await page.locator('#mode-button').click();await page.locator('[data-harness="thinking"]').click();await page.locator('[data-harness="set-thinking"][data-value="max"]').click();
 await page.locator('[data-session="architecture"]').click();assert.equal(await page.locator('#model-label').innerText(),'DeepSeek');assert.equal(await page.locator('#mode-label').innerText(),'执行');assert.equal(await page.locator('#thinking-label').innerText(),'High');
 await page.locator('[data-session="dark"]').click();assert.equal(await page.locator('#model-label').innerText(),'OpenAI');assert.equal(await page.locator('#mode-label').innerText(),'规划');assert.equal(await page.locator('#thinking-label').innerText(),'Max');
 pass('provider, mode and thinking selection belong to the selected session instead of leaking across sessions');
 await page.locator('#new-task').click();await page.locator('#new-title').fill('独立模型设置');await page.locator('#new-model').selectOption('rpi / DeepSeek');await page.locator('#create-task').click();assert.equal(await page.locator('#model-label').innerText(),'DeepSeek');await page.locator('[data-session="dark"]').click();assert.equal(await page.locator('#model-label').innerText(),'OpenAI');
 pass('creating a session with another provider does not overwrite the previous session settings');
 await page.locator('#queue-shelf [data-harness="queue"]').click();const queueText=await page.locator('#modal').innerText();assert.match(queueText,/当前回答收尾/);assert.doesNotMatch(queueText,/当前轮次结算后/);await page.locator('#modal .modal-header [data-close-modal]').click();
 await page.locator('.primary-nav [data-nav="extensions"]').click();assert.match(await page.locator('#page-view').innerText(),/rpi-permission-gate/);assert.match(await page.locator('#page-view').innerText(),/单独安装不等于已启用执行拦截/);
 pass('follow-up delivery copy and permission-gate versus policy-storage distinction match the reviewed native boundary');
 assert.deepEqual(errors,[]);fs.writeFileSync('artifacts/runtime-semantics-validation.json',JSON.stringify({date:'2026-10-24',browser:await browser.version(),results,errors},null,2));await browser.close();
})().catch(e=>{console.error(e);process.exit(1);});
