const assert=require('node:assert/strict'),fs=require('node:fs');
const {chromium}=require('../support/browser.cjs');const {startFixture}=require('../fixtures/ui-fixture.cjs');
(async()=>{
 const node=await startFixture(),browser=await chromium.launch({headless:true,executablePath:process.env.CHROME_PATH});const page=await browser.newPage({viewport:{width:1280,height:950}}),results=[],errors=[];
 page.setDefaultTimeout(8000);page.on('pageerror',e=>errors.push(e.message));const pass=name=>{results.push(name);console.log('PASS',name)};
 const bottom=()=>page.waitForFunction(()=>{const el=document.querySelector('.transcript');return el&&el.scrollHeight-el.clientHeight-el.scrollTop<4});
 try{
  await page.goto(process.env.WORK_UI_URL||'http://127.0.0.1:4180/');await page.getByRole('button',{name:'打开设置',exact:true}).click();await page.getByRole('navigation',{name:'设置分类'}).getByRole('button',{name:'执行节点',exact:true}).click();await page.getByRole('button',{name:'连接其他节点',exact:true}).click();await page.getByLabel('节点地址',{exact:true}).fill(node.url);await page.getByRole('button',{name:'连接节点',exact:true}).click();await page.locator('.server-details [role=status]').filter({hasText:'在线'}).waitFor();await page.getByRole('button',{name:'返回会话',exact:true}).click();await page.locator('.project-row').filter({hasText:'rpi-work'}).click();await page.locator('.session-entry[data-session-id="dark"] .session-item').click();
  const composer=page.getByRole('textbox',{name:'发送给 agent 的消息',exact:true});await composer.waitFor();node.controls.promptDelayMs=500;await composer.fill('请检查当前项目');await page.getByRole('button',{name:'发送消息',exact:true}).click();await page.locator('.pending-user-message').getByText('请检查当前项目',{exact:true}).waitFor();await page.locator('.reply-waiting').getByText('正在发送…',{exact:true}).waitFor();await page.locator('.composer-actions .send-button.is-loading').waitFor();assert(await page.locator('.composer-actions .send-button').isDisabled());await page.locator('.reply-waiting').getByText('等待回复…',{exact:true}).waitFor();
  node.emit({type:'message_end',message:{entryId:'accepted-question',role:'user',content:'请检查当前项目'}});await page.locator('.pending-user-message').waitFor({state:'hidden'});await page.locator('.reply-waiting').waitFor();
  assert.equal(await page.locator('.composer-actions button').count(),1);assert(await page.getByRole('button',{name:'停止运行',exact:true}).isEnabled());assert(await page.getByRole('button',{name:'停止运行',exact:true}).evaluate(element=>element.classList.contains('send-button')));
  pass('sending displays the user message immediately; acceptance and user echo keep a waiting indicator until a response');
  node.emit({type:'agent_start',runId:'reply-follow'});node.emit({type:'message_update',message:{entryId:'reply',role:'assistant'},eventType:'text_delta',delta:'收到，正在检查。'});await page.locator('.transcript').getByText('收到，正在检查。',{exact:true}).waitFor();await page.locator('.reply-waiting').getByText('正在处理…',{exact:true}).waitFor();
  assert.equal(await page.locator('.reply-waiting').count(),1);assert.equal(await page.locator('.activity-spinner').count(),2);
  assert.equal(await page.locator('.transcript .agent-avatar, .transcript .message-avatar, .transcript .message-author').count(),0);
  const promptsBeforeDraft=node.requests.filter(request=>request.body.type==='prompt').length;
  await composer.fill('继续补充，但暂不发送');await page.getByRole('button',{name:'按所选方式投递消息',exact:true}).waitFor();assert(await page.locator('.composer-actions .send-button').isEnabled());assert.equal(await page.locator('.composer-actions .activity-spinner').count(),0);assert.equal(await page.getByRole('button',{name:'停止运行',exact:true}).count(),0);
  await composer.fill('');await page.locator('.composer-actions .send-button.is-loading').waitFor();assert(await page.getByRole('button',{name:'停止运行',exact:true}).isEnabled());assert.equal(await page.locator('.composer-actions button').count(),1);assert.equal(node.requests.filter(request=>request.body.type==='prompt').length,promptsBeforeDraft);
  node.emit({type:'tool_execution_start',toolCallId:'loading-read',toolName:'read',args:{path:'README.md'}});await page.locator('.reply-waiting').waitFor();node.emit({type:'tool_execution_end',toolCallId:'loading-read',toolName:'read',result:{content:[{type:'text',text:'File checked'}]}});await page.locator('.reply-waiting').waitFor();
  pass('streamed text and tools retain a single activity indicator; typing restores send and clearing restores loading without a new prompt');
  for(const width of [1280,390,320]){
    await page.setViewportSize({width,height:width===1280?950:844});await page.locator('.transcript').evaluate(element=>{element.scrollTop=element.scrollHeight});await bottom();
    assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth));
    const button=page.locator('.composer-actions .send-button'),bounds=await button.boundingBox();assert(bounds.x>=0&&bounds.x+bounds.width<=width+1);
    assert.equal(await page.locator('.composer-actions button').count(),1);
    assert(await page.locator('.transcript>:not(.empty-state)').evaluateAll(elements=>elements.every(element=>element.getBoundingClientRect().width<=721)));
    assert(await page.locator('.conversation-markdown').first().evaluate(element=>parseFloat(getComputedStyle(element).lineHeight)/parseFloat(getComputedStyle(element).fontSize)<=1.56));
    if(width===1280){const message=await page.locator('.message-turn').first().boundingBox(),input=await page.locator('.composer-main>.composer').boundingBox();assert(message.width<=720);assert(input.width<=744);assert(Math.abs(message.x+message.width/2-input.x-input.width/2)<2)}
    if(width<=390)assert(bounds.height>=44);
    await page.screenshot({path:`artifacts/conversation-loading-${width}.png`,animations:'disabled'});
  }
  await page.setViewportSize({width:1280,height:950});
  pass('loading controls and compact messages stay within the viewport at desktop and 390/320');
  for(let i=0;i<28;i++)node.emit({type:'message_update',message:{entryId:`long-reply-${i}`,role:'assistant'},eventType:'text_delta',delta:`检查结果 ${i}：${'持续输出内容，用于验证自动跟随。'.repeat(25)}`});await page.locator('.transcript').getByText(/检查结果 27/).waitFor();await bottom();
  pass('streamed output follows continuously while the reader is at the bottom');
  await page.locator('.transcript').evaluate(el=>{el.scrollTop=160;el.dispatchEvent(new Event('scroll'))});node.emit({type:'message_update',message:{entryId:'after-reading',role:'assistant'},eventType:'text_delta',delta:'上翻阅读后追加的一段输出'});await page.getByRole('button',{name:'回到最新消息',exact:true}).waitFor();assert(await page.locator('.transcript').evaluate(el=>el.scrollTop<240));await page.getByRole('button',{name:'回到最新消息',exact:true}).click();await bottom();await page.getByRole('button',{name:'回到最新消息',exact:true}).waitFor({state:'hidden'});
  pass('scrolling up pauses following; Latest resumes it without a smooth-scroll race');
  await page.locator('.session-entry[data-session-id="architecture"] .session-item').click();await page.locator('.transcript').getByText('这是 梳理 Agent 架构 的独立历史。',{exact:true}).waitFor();await page.locator('.reply-waiting').waitFor({state:'hidden'});assert.equal(await page.locator('.send-button.is-loading').count(),0);
  await page.locator('.session-entry[data-session-id="dark"] .session-item').click();await page.locator('.reply-waiting').getByText('正在处理…',{exact:true}).waitFor();
  pass('switching sessions isolates loading and restoring an active session restores its activity');
  node.emit({type:'agent_end',runId:'reply-follow'});await page.locator('.reply-waiting').getByText('正在处理…',{exact:true}).waitFor();assert.equal(await page.locator('.send-button.is-loading').count(),1);
  node.emit({type:'agent_settled',runId:'reply-follow',outcome:'completed'});await page.getByRole('button',{name:'发送消息',exact:true}).waitFor();await page.locator('.reply-waiting').waitFor({state:'hidden'});assert(await page.getByRole('button',{name:'发送消息',exact:true}).isDisabled());await page.locator('.transcript').evaluate(el=>{el.scrollTop=180;el.dispatchEvent(new Event('scroll'))});node.controls.promptDelayMs=0;node.controls.failPrompt=true;await composer.fill('这条消息会被节点拒绝');await page.getByRole('button',{name:'发送消息',exact:true}).click();await page.locator('.composer-foot [role=status]').filter({hasText:/指令未.*接受/}).waitFor();await page.locator('.reply-waiting').waitFor({state:'hidden'});assert.equal(await composer.inputValue(),'这条消息会被节点拒绝');await bottom();
  pass('a user send returns to latest; rejection stops loading and preserves the draft');
  node.controls.failPrompt=false;await composer.fill('验证停止等待');await page.getByRole('button',{name:'发送消息',exact:true}).click();await page.locator('.reply-waiting').getByText('等待回复…',{exact:true}).waitFor();node.emit({type:'agent_start',runId:'stop-waiting'});
  await page.getByRole('button',{name:'停止运行',exact:true}).click();await page.locator('.composer-foot [role=status]').filter({hasText:'停止请求已发送'}).waitFor();assert(node.requests.some(request=>request.body.type==='abort'));await page.locator('.send-button.is-loading').waitFor();
  await page.emulateMedia({reducedMotion:'reduce'});assert.equal(await page.locator('.reply-waiting .activity-spinner').evaluate(element=>getComputedStyle(element).animationName),'none');
  node.emit({type:'agent_settled',runId:'stop-waiting',outcome:'interrupted'});await page.locator('.reply-waiting').waitFor({state:'hidden'});await page.locator('.send-button.is-loading').waitFor({state:'hidden'});
  assert.equal(await page.locator('.transcript .message-avatar, .transcript .message-author').count(),0);
  assert(await page.locator('.agent-message').first().evaluate(element=>parseFloat(getComputedStyle(element).marginBottom)<=16));
  pass('stop acknowledgement does not finish loading; interrupted settlement does, with compact avatar-free output and reduced-motion support');
  await page.setViewportSize({width:320,height:844});assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth));await page.screenshot({path:'artifacts/reply-follow-320.png',animations:'disabled'});assert.deepEqual(errors,[]);pass('mobile output and reduced-motion layout remain bounded with no JavaScript errors');
 }catch(error){errors.push(error.message);await page.screenshot({path:'artifacts/reply-follow-failure.png',animations:'disabled'});throw error}
 finally{fs.writeFileSync('artifacts/reply-follow-validation.json',JSON.stringify({results,errors},null,2));await browser.close();await node.close();}
})().catch(error=>{console.error(error);process.exitCode=1});
