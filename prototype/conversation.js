/* A single conversation timeline owns plans, tool activity and command output. */
function syncTimelineCalls(){
  const calls=runtimeSession().calls,shell=calls.find(c=>c.id==='shell');
  if(!shell)return;
  shell.status=state.approval==='pending'?'waiting':state.approval==='approved'?'done':'cancelled';
  shell.duration=state.approval==='approved'?'1.8s':'—';
  shell.output=state.approval==='approved'?'PS D:/Projects/rpi-work> cargo test -p work-ui --lib\n\nrunning 12 tests\ntest result: ok. 12 passed; 0 failed\n\nexit code: 0（演示，未执行真实命令）':state.approval==='pending'?'等待单次授权；尚未执行命令。':'用户拒绝执行，未启动命令。';
}
function timelineOpen(id){return runtimeSession().openTimeline?.includes(id);}
function inlinePlan(){
  const done=state.approval==='approved';
  return `<details class="inline-plan" id="timeline-plan" data-timeline-id="plan" ${timelineOpen('plan')?'open':''}><summary><span>${icon('checklist')}任务计划</span><span>${done?'3':'2'}/3 已完成 ${icon('chevron-down')}</span></summary><ol>${[['定义主题变量','复用两套语义化颜色',true],['添加主题偏好','跟随系统并保存手动选择',true],['验证桌面与手机体验',done?'示例测试已通过':'等待测试命令确认',done]].map(([title,description,complete])=>`<li>${icon(complete?'circle-check':'clock')}<div><strong>${title}</strong><small>${description}</small></div></li>`).join('')}</ol><div class="todo-audit">${inlineCall(runtimeSession().calls.find(c=>c.id==='todo'))}</div></details>`;
}
function inlineCall(call){
  const labels={done:'完成',failed:'失败',waiting:'待确认',running:'运行中',cancelled:'已取消',skipped:'未提供'};
  return `<details class="tool-call inline-call ${call.status}" id="call-${call.id}" data-timeline-id="${call.id}" ${timelineOpen(call.id)?'open':''}><summary><span class="call-icon">${icon(call.kind==='exec'?'terminal':call.kind==='web'?'globe':call.kind==='mcp'?'link':call.kind==='agent'?'blocks':'file')}</span><div><strong>${escapeHTML(call.title)}</strong><span><code>${call.name}</code><span>${escapeHTML(call.source)}</span></span></div><span class="call-status">${labels[call.status]}</span><time>${call.duration}</time>${icon('chevron-down')}</summary>${call.status==='failed'?'<div class="inline-call-warning">网络超时，可展开查看错误并重试</div>':''}<div class="call-body"><div class="call-meta"><span>${icon('shield')}${call.permission}</span><span>第 ${call.attempts} 次尝试</span><span>${call.lines}</span></div><div class="call-section-title">调用参数</div><pre>${escapeHTML(JSON.stringify(call.args,null,2))}</pre><div class="call-section-title">${call.kind==='exec'?'命令输出':'工具结果'}</div><pre class="call-output">${escapeHTML(call.output)}</pre><div class="call-actions">${call.status==='failed'?rtButton('retry-tool','模拟重试','refresh',`data-id="${call.id}"`):''}${call.status==='running'?rtButton('cancel-tool','取消子任务','stop',`data-id="${call.id}"`):''}${call.kind==='write'?'<button class="button" data-review-scope="turn">查看本轮变更</button>':''}${rtButton('tool-output','展开输出','terminal',`data-id="${call.id}"`)}</div></div></details>`;
}
function renderUnifiedConversation(){
  syncTimelineCalls();
  const calls=runtimeSession().calls;
  const byId=id=>calls.find(c=>c.id===id);
  return `<div class="day-divider">今天 · 10 月 23 日</div><article class="message"><div class="message-avatar">B</div><div class="message-content"><div class="message-author">你<time>14:32</time></div><p>给工作台增加深色模式，默认跟随系统，也可以在设置里切换。桌面和手机都要适配，尽量复用现有的主题变量。</p><button class="context-chip" data-action="context">${icon('folder')}rpi-work</button></div></article>${agentMessage(`<p>我会复用现有主题变量，补齐深色配色，并保存你的主题偏好。</p>${renderHistoricalAsk()}${renderSkillUse(skillSession().skillEvents.find(e=>e.id==='skill-read'))}${inlinePlan()}<div class="timeline-activity" id="timeline-tools"><div class="activity-label">${icon('blocks')}检查项目与参考资料 <span>3 次调用</span></div><details class="tool-batch" id="timeline-inspect" data-timeline-id="inspect" ${timelineOpen('inspect')?'open':''}><summary>${icon('circle-check')}已读取文件、检索颜色和组件文档 <span>3 项完成</span>${icon('chevron-down')}</summary><div class="tool-batch-content">${['read','grep','mcp'].map(id=>inlineCall(byId(id))).join('')}</div></details>${inlineCall(byId('web'))}<p class="timeline-note">参考页面暂时无法访问。我会依据现有项目和你提供的附件继续。</p>${inlineCall(byId('edit'))}${inlineCall(byId('child'))}<details class="tool-batch unavailable-batch" data-timeline-id="optional" ${timelineOpen('optional')?'open':''}><summary>${icon('info')}浏览器验证能力尚未配置 ${icon('chevron-down')}</summary><div class="tool-batch-content">${inlineCall(byId('browser'))}</div></details></div><p>主题切换已经准备好，接下来运行组件测试。</p><button class="change-summary" data-review-scope="turn"><span>${icon('diff')}本轮涉及 3 个文件</span><span class="stat"><span class="added">+67</span> <span class="removed">−10</span> ${icon('chevron-right')}</span></button>${inlineCall(byId('shell'))}${approvalCard()}`)}`;
}
function focusTimeline(target='tools'){
  const wasConversation=state.page==='workspace'&&state.tab==='conversation';
  if(!wasConversation){navigate('workspace');state.tab='conversation';renderTab();}
  const id=target==='plan'?'timeline-plan':target==='terminal'?'call-shell':target==='tools'?'timeline-tools':'call-'+target;
  const el=document.getElementById(id);
  if(!el){toast('当前会话还没有相应的执行记录');return;}
  for(let parent=el.parentElement;parent&&parent.id!=='task-body';parent=parent.parentElement)if(parent.tagName==='DETAILS')parent.open=true;
  if(el.tagName==='DETAILS')el.open=true;
  el.scrollIntoView({block:'start',behavior:'auto'});
  el.classList.add('timeline-highlight');setTimeout(()=>el.classList.remove('timeline-highlight'),1200);
}
function refreshTimeline(){
  const scroll=$('#task-body').scrollTop;
  renderConversation();renderHarnessUI();$('#task-body').scrollTop=scroll;
}
document.addEventListener('toggle',event=>{
  const el=event.target;
  if(!el.matches?.('details[data-timeline-id]')||!el.isConnected)return;
  const s=runtimeSession();s.openTimeline||=[];
  s.openTimeline=s.openTimeline.filter(id=>id!==el.dataset.timelineId);
  if(el.open)s.openTimeline.push(el.dataset.timelineId);
},true);
document.addEventListener('click',event=>{
  const button=event.target.closest('button');
  if(button?.dataset.timeline)focusTimeline(button.dataset.timeline);
});
