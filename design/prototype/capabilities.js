/* Capability design samples. This file does not invoke any real plugin/skill. */
const skillSamples=[
 {id:'project-theme',name:'theme-design',scope:'project',source:'项目',path:'D:/Projects/rpi-work/.rpi/skills/theme-design/SKILL.md',status:'available',description:'定义主题语义、浅深色配色和组件适配约定。',trigger:'任务涉及工作台主题、颜色变量或系统外观时。',content:'---\nname: theme-design\ndescription: Define shared light and dark theme tokens.\n---\n\n# Theme design\n\n- Reuse semantic color tokens.\n- Preserve system appearance and user preference.\n- Check narrow-screen contrast.\n\nSee references/colors.md for example palettes.\n\nThis is a built-in prototype sample.',refs:['references/colors.md'],auto:true},
 {id:'global-responsive',name:'responsive-review',scope:'global',source:'全局',path:'~/.rpi/agent/skills/responsive-review/SKILL.md',status:'available',description:'检查窄屏布局、触控目标与键盘遮挡。',trigger:'需要评估桌面与手机共享界面时。',content:'---\nname: responsive-review\ndescription: Review small-screen layouts.\n---\n\n# Responsive review\n\nInspect navigation, touch targets and safe areas.\nReport findings with file references.\n\nPrototype sample only.',refs:[],auto:true},
 {id:'global-review',name:'code-review',scope:'global',source:'全局',path:'~/.rpi/agent/skills/code-review/SKILL.md',status:'available',description:'根据明确的检查范围审阅变更，保留可定位证据。',trigger:'仅手动指定；不出现在模型自动发现目录中。',content:'---\nname: code-review\ndescription: Review an explicit change scope.\ndisable-model-invocation: true\n---\n\n# Code review\n\nInspect the selected change and explain actionable findings.\n\nPrototype sample only.',refs:[],auto:false},
 {id:'global-theme',name:'theme-design',scope:'global',source:'全局',path:'~/.rpi/agent/skills/theme-design/SKILL.md',status:'shadowed',description:'与项目技能同名；本项目使用项目内版本。',trigger:'已被项目同名技能覆盖。',content:'---\nname: theme-design\ndescription: Global theme conventions.\n---\n\n# Global theme design\n\nThis sample loses to the project skill.',refs:[],auto:true},
 {id:'package-notes',name:'release-notes',scope:'package',source:'资源包',path:'<package>/skills/release-notes/SKILL.md',status:'available',description:'按用户可见行为整理发布说明。',trigger:'准备版本说明或变更摘要时。',content:'---\nname: release-notes\ndescription: Write release notes from changes.\n---\n\n# Release notes\n\nExplain user-visible changes and relevant limitations.\n\nPrototype sample only.',refs:[],auto:true},
 {id:'broken',name:'broken-skill',scope:'global',source:'全局',path:'~/.rpi/agent/skills/broken-skill/SKILL.md',status:'invalid',description:'元数据解析失败，不能加入可用目录。',trigger:'不可用：parse_failed。',content:'---\nname: [invalid YAML\ndescription: sample\n---\n\nBroken metadata sample.',refs:[],auto:false},
];
callSamples.push(
 {id:'skill-read',name:'read',title:'载入 theme-design 技能',source:'项目 Skill · 示例',kind:'skill',status:'done',duration:'0.1s',permission:'读取技能指令',args:{path:skillSamples[0].path},output:'已载入 SKILL.md 全文。此事件证明读取，不代表所有建议都已被采用。',lines:'技能全文',attempts:1},
 {id:'ask-history',name:'ask_user',title:'确认手机端优先方案',source:'rpi-ask-user · 示例',kind:'ask',status:'done',duration:'已回答',permission:'用户交互',args:{question:'手机端首版优先采用哪种入口？'},output:'历史示例：先使用响应式 Web 接续电脑任务。',lines:'历史问答示例',attempts:1},
 {id:'todo',name:'todo',title:'更新本轮待办进度',source:'rpi-todo',kind:'todo',status:'done',duration:'0.1s',permission:'会话内任务列表',args:{action:'list',includeDone:true},output:'定义主题变量：完成\n添加主题偏好：完成\n验证桌面与手机体验：等待确认',lines:'3 项待办',attempts:1}
);
const capabilities={owner:null,skillQuery:'',skillScope:'all',skillUsed:false,skillDetailTab:'overview',skillDetailId:null,btw:{visible:false,epoch:0,history:[],queue:[],active:null,timer:null,input:'',recent:false}};
function skillSession(){
  const s=runtimeSession();
  if(!s.skillEvents)s.skillEvents=state.session==='dark'?[{id:'skill-read',skillId:'project-theme',reason:'本轮涉及主题变量与系统外观；Agent 按描述匹配后读取。',method:'模型按描述匹配 · 示例',evidence:'read → SKILL.md 成功',adopted:'尚无结构化采用证据'}]:[];
  s.draftSkills||=[];s.askRequests||=[];
  return s;
}
function hasPendingAsk(){return (runtimeSession().askRequests||[]).some(q=>q.status==='pending');}
function skillById(id){return skillSamples.find(s=>s.id===id);}
function resetBtwThread(hide=false){
  const b=capabilities.btw;clearTimeout(b.timer);b.epoch++;b.history=[];b.queue=[];b.active=null;b.input='';b.recent=false;if(hide)b.visible=false;$('#btw-panel')?.replaceChildren();
}
function renderCapabilitiesUI(){
  if(capabilities.owner!==state.session){resetBtwThread(true);capabilities.owner=state.session;}
  if(!$('#composer-capabilities'))$('.composer-bottom').insertAdjacentHTML('beforebegin','<div id="composer-capabilities" class="composer-capabilities"></div>');
  if(!$('#btw-panel'))$('#app').insertAdjacentHTML('beforeend','<aside id="btw-panel" aria-label="BTW 旁问" hidden></aside>');
  const s=skillSession(),b=capabilities.btw;
  $('#app').classList.toggle('ask-waiting',hasPendingAsk());
  $('#composer-capabilities').innerHTML=`<button data-capability="skill-picker">${icon('checklist')}Skills <span>${s.draftSkills.length?'待发送 '+s.draftSkills.length:s.skillEvents.length?'已载入 '+s.skillEvents.length:'选择'}</span></button><button data-capability="btw-open">${icon('message')}BTW 旁问 ${b.active?'<i class="status-dot"></i>':b.history.length?'<span>'+b.history.length+'</span>':''}</button><button data-capability="common-tools">${icon('blocks')}常用能力</button>${s.draftSkills.length?`<div class="draft-skills">${s.draftSkills.map(id=>`<span>${escapeHTML(skillById(id).name)}<button data-capability="skill-unpin" data-id="${id}" aria-label="移除技能 ${escapeHTML(skillById(id).name)}">${icon('x')}</button></span>`).join('')}</div>`:''}`;
  $('#skills-context')?.remove();
  renderSessionResources();
  $('#app').classList.toggle('btw-open',b.visible);$('#btw-panel').hidden=!b.visible;
  if(b.visible)renderBtwPanel();
}
function renderSkillUse(event){
  const skill=skillById(event.skillId);
  return `<details class="tool-call skill-use" id="call-${event.id}" data-timeline-id="${event.id}" ${timelineOpen(event.id)?'open':''}><summary>${icon('checklist')}<div><strong>载入技能 · ${escapeHTML(skill.name)}</strong><small>${event.method}</small></div><span>全文已载入</span>${icon('chevron-down')}</summary><div class="skill-use-body"><p>${escapeHTML(event.reason)}</p><div class="skill-use-evidence"><span>${event.evidence}</span><span>${event.adopted}</span></div><code>${escapeHTML(skill.path)}</code><div>${skill.refs.map(ref=>`<span class="context-chip">${escapeHTML(ref)} · 关联资源，未读取</span>`).join('')}</div><button class="button" data-capability="skill-detail" data-id="${skill.id}">查看技能说明与来源</button></div></details>`;
}
function renderSkillMessageRefs(ids=[]){return ids.length?'<div class="message-skill-refs">'+ids.map(id=>`<button data-capability="skill-detail" data-id="${id}">${icon('checklist')}已指定 ${escapeHTML(skillById(id).name)}</button>`).join('')+'</div>':'';}
function takeDraftSkills(){return recordSkillLoads(skillSession().draftSkills.splice(0));}
function recordSkillLoads(ids){
  const s=skillSession();
  ids=ids.filter(id=>sessionSkillAvailable(skillById(id)));
  ids.forEach(id=>{
    if(s.skillEvents.some(e=>e.skillId===id))return;
    const skill=skillById(id),callId='skill-'+crypto.randomUUID();
    s.skillEvents.push({id:callId,skillId:id,reason:'用户将此技能明确指定到本条消息；原型模拟载入全文。',method:'用户明确指定 · 模拟',evidence:'模拟的全文载入事件',adopted:'不证明建议已被采用'});
    s.calls.push({id:callId,name:'read',kind:'skill',source:skill.source,status:'done',args:{path:skill.path},output:'模拟载入全文',duration:'0.1s',attempts:1});
  });
  return ids;
}
function pinSkill(id){const skill=skillById(id),s=skillSession();if(!skill||!sessionSkillAvailable(skill)){toast('该技能当前不可用，请查看来源与诊断');return;}if(s.draftSkills.includes(id)){toast('已加入下条消息');return;}s.draftSkills.push(id);renderCapabilitiesUI();toast('已指定到下条消息，尚未载入全文');}
function skillStatus(skill){if(!resourceSkillReady(skill))return skillAcquisitionState(skill);if(skill.status==='available'&&!sessionSkillAvailable(skill))return '本会话未启用';return skill.status==='invalid'?'解析失败':skill.status==='shadowed'?'被项目覆盖':skillSession().skillEvents.some(e=>e.skillId===skill.id)?'全文已载入':skillSession().draftSkills.includes(skill.id)?'已指定，待发送':skill.auto?'可发现':'仅手动指定';}
function renderSkillCatalogue(){
  const focused=document.activeElement?.id==='skill-search',pos=focused?document.activeElement.selectionStart:0;
  const listed=skillSamples.filter(skill=>(capabilities.skillScope==='all'||skill.scope===capabilities.skillScope)&&(!capabilities.skillUsed||skillSession().skillEvents.some(e=>e.skillId===skill.id))&&(skill.name+' '+skill.description).toLowerCase().includes(capabilities.skillQuery.toLowerCase()));
  $('#page-view').innerHTML=`<div class="overline">KNOW HOW. KNOW WHY.</div><div class="page-heading"><div><h1>Skills，清楚地用在每一步</h1><p>找到技能、明确来源，再查看它是否被载入。</p></div></div><div class="info-banner">${icon('info')}新技能需先下载到执行节点、安装并启用，才能指定到消息。已可用项为预先下载并安装的示例；可发现不等于已使用。</div><div class="skill-catalogue-filters"><input id="skill-search" value="${escapeHTML(capabilities.skillQuery)}" placeholder="搜索技能名称或用途…" aria-label="搜索技能"><select id="skill-scope" aria-label="技能来源"><option value="all">全部来源</option><option value="project">项目</option><option value="global">全局</option><option value="package">资源包</option></select><label><input id="skill-used" type="checkbox" ${capabilities.skillUsed?'checked':''}>本会话已载入</label></div><div class="skill-catalogue-count">${listed.length} 项 · 实际来源优先级由宿主解析</div><div class="skill-catalogue">${listed.map(skill=>`<article class="skill-catalogue-card" data-skill-id="${skill.id}"><div class="skill-catalogue-heading"><span class="extension-icon">${icon('checklist')}</span><div><h3>${escapeHTML(skill.name)}</h3><small>${skill.source} · ${skill.auto?'允许按描述发现':'仅手动'}</small></div><span class="skill-state ${!resourceSkillReady(skill)?'unavailable':skill.status}">${skillStatus(skill)}</span></div><p>${escapeHTML(skill.description)}</p><code>${escapeHTML(skill.path)}</code><div class="skill-acquisition-state">${skillAcquisitionState(skill)}</div>${!skill.packageId?resourceSteps(resourceStore.skills.downloaded.has(skill.id),resourceStore.skills.installed.has(skill.id),sessionSkillAvailable(skill)):''}<div class="skill-card-actions">${skillResourceActions(skill)}<button class="button" data-capability="skill-detail" data-id="${skill.id}">查看说明</button><button class="button" data-capability="skill-pin" data-id="${skill.id}" ${!sessionSkillAvailable(skill)?'disabled':''}>指定到下条消息</button></div></article>`).join('')||'<div class="empty-compact">没有匹配的技能。</div>'}</div>`;
  $('#skill-scope').value=capabilities.skillScope;if(focused){$('#skill-search').focus();$('#skill-search').setSelectionRange(pos,pos);}
}
function skillDetail(id,tab='overview'){
  const skill=skillById(id);if(!skill)return;capabilities.skillDetailId=id;capabilities.skillDetailTab=tab;
  const events=skillSession().skillEvents.filter(e=>e.skillId===id);
  const body=tab==='source'?`<pre class="document-preview">${escapeHTML(skill.content)}</pre><p class="modal-help">预先编写的示例内容，未读取本机 SKILL.md。</p>`:tab==='history'?events.length?events.map(e=>`<div class="skill-history"><strong>${e.method}</strong><p>${e.reason}</p><small>${e.evidence} · ${e.adopted}</small></div>`).join(''):'<div class="empty-compact">暂无全文载入记录。指定到草稿还不算使用。</div>':`<p>${escapeHTML(skill.description)}</p><dl class="stats-details"><div><dt>来源</dt><dd>${skill.source}</dd></div><div><dt>当前状态</dt><dd>${skillStatus(skill)}</dd></div><div><dt>自动发现</dt><dd>${skill.auto?'按描述匹配':'仅手动指定'}</dd></div></dl><div class="skill-trigger"><strong>适用时机</strong><p>${escapeHTML(skill.trigger)}</p></div><code class="skill-path">${escapeHTML(skill.path)}</code>${skill.status==='shadowed'?'<div class="provider-error">项目 theme-design 覆盖此全局同名版本。此版本不进入本项目可用目录。</div>':skill.status==='invalid'?'<div class="provider-error">parse_failed：示例 YAML 元数据格式错误。修复并重新发现前不能调用。</div>':''}<p class="modal-help">技能提供操作指导，不授予工具权限。关闭可发现性也不能撤回已进入上下文的指令。</p>`;
  runtimeModal(escapeHTML(skill.name),`<div class="segmented">${[['overview','概览'],['source','SKILL.md'],['history','使用记录']].map(([value,label])=>`<button data-capability="skill-detail-tab" data-value="${value}" class="${tab===value?'active':''}">${label}</button>`).join('')}</div><div class="skill-detail-content">${body}</div>`,`${skillResourceActions(skill)}${skill.status==='available'?`<button class="button primary" data-capability="skill-pin" data-id="${skill.id}" ${!sessionSkillAvailable(skill)?'disabled':''}>指定到下条消息</button>`:''}`,true);
}
function skillPicker(){runtimeModal('给下条消息指定技能',`<p>先选择技能。发送前只作为草稿引用，不把它标记成已使用。</p>${skillSamples.filter(s=>sessionSkillAvailable(s)).map(skill=>`<button class="skill-picker-row" data-capability="skill-pin" data-id="${skill.id}">${icon('checklist')}<span><strong>${escapeHTML(skill.name)}</strong><small>${skill.source} · ${escapeHTML(skill.description)}</small></span>${skillSession().draftSkills.includes(skill.id)?icon('check'):icon('plus')}</button>`).join('')}`,'',true);}
function renderHistoricalAsk(){return `<article class="tool-call ask-history" id="call-ask-history"><div>${icon('message')}<strong>ask_user · 手机端方案</strong><span>已回答 · 历史示例</span></div><p>手机端首版优先采用哪种入口？</p><blockquote>先用响应式 Web 接续电脑上的任务。</blockquote><button class="small-link" data-capability="ask-new" data-mode="single">体验一次新提问 ${icon('chevron-right')}</button></article>`;}
function askSample(mode='single'){
  const s=skillSession();s.askRequests.filter(q=>q.status==='pending').forEach(q=>{q.status='cancelled';q.notice='已由新示例请求替代';const c=s.calls.find(c=>c.id===q.id);if(c){c.status='cancelled';c.output=q.notice;}});
  const id='ask-'+crypto.randomUUID();
  const info=mode==='multiple'?{title:'哪些事件需要通知？',context:'可以选择多项，也可以补充其他事件。',options:[['completed','任务完成','工作交付后通知'],['failed','任务失败','需要修正后继续'],['input','需要回答','Agent 正在等待你']] }:mode==='freeform'?{title:'还有哪些交互必须保留？',context:'用自己的话描述需求，空白内容不会提交。',options:[]}: {title:'手机端首版优先采用哪种入口？',context:'这是需求澄清，不是 shell 权限审批。',options:[['web','响应式 Web（建议）','先打通同会话跨端接续'],['native','原生手机版','优先准备 Android / iOS 打包']]};
  s.askRequests.push({id,mode,...info,status:'pending',values:[],freeform:'',comment:'',error:'',answer:null});
  s.calls.push({id,name:'ask_user',kind:'ask',status:'waiting',source:'rpi-ask-user',duration:'—',permission:'用户交互',args:{question:info.title},output:'等待回答',attempts:1});
  if($('#modal').open)$('#modal').close();navigate('workspace');state.tab='conversation';renderTab();$('.ask-card:last-child')?.scrollIntoView({block:'start'});
}
function renderAskRequest(q){
  const pending=q.status==='pending',labels={answered:'已回答',pending:'等待回答',cancelled:'已取消',expired:'已超时',external:'其他设备已处理'};
  return `<article class="tool-call ask-card ${pending?'waiting':''}" data-ask-id="${q.id}"><div class="ask-card-heading">${icon('message')}<strong>Agent 想确认一个问题</strong><span>${labels[q.status]} · 演示</span></div><h3>${q.title}</h3><p>${q.context}</p>${pending?`<fieldset><legend>${q.mode==='multiple'?'可多选':q.mode==='freeform'?'自由回答':'请选择一项'}</legend>${q.options.map(([value,title,description])=>`<label class="ask-option"><input type="${q.mode==='multiple'?'checkbox':'radio'}" name="${q.id}" data-ask-value="${value}" ${q.values.includes(value)?'checked':''}><span><strong>${title}</strong><small>${description}</small></span></label>`).join('')}</fieldset><label class="ask-field-label">${q.mode==='freeform'?'你的回答':'其他答案（可选）'}<textarea data-ask-freeform rows="2" class="modal-textarea" placeholder="填写你的想法…">${escapeHTML(q.freeform)}</textarea></label>${q.mode!=='freeform'?`<label class="ask-field-label">补充说明（可选）<input data-ask-comment value="${escapeHTML(q.comment)}" placeholder="说明选择的原因…"></label>`:''}<p class="ask-error" role="alert">${q.error}</p><div class="ask-primary-actions"><button class="button primary" data-capability="ask-submit" data-id="${q.id}">提交回答</button><button class="button" data-capability="ask-cancel" data-id="${q.id}">取消回答</button></div><details class="ask-demo-controls"><summary>体验其他状态</summary><button data-capability="ask-expire" data-id="${q.id}">模拟超时</button><button data-capability="ask-external" data-id="${q.id}">模拟另一设备回应</button><small>未选择任何选项；建议项不会自动提交。</small></details>`:`<div class="ask-answer">${q.answer?`<strong>${escapeHTML(q.answer)}</strong>${q.comment?'<p>'+escapeHTML(q.comment)+'</p>':''}`:`<span>${q.notice||'未返回答案，不能据此继续推断用户偏好。'}</span>`}</div><button class="small-link" data-capability="ask-new" data-mode="${q.mode}">演示新请求 ${icon('chevron-right')}</button>`}</article>`;
}
function renderAdditionalConversation(){const s=skillSession();return `${s.skillEvents.filter(e=>e.id!=='skill-read').map(e=>renderSkillUse(e)).join('')}${s.askRequests.map(renderAskRequest).join('')}`;}
function answerAsk(id,status='answered'){
  const q=skillSession().askRequests.find(q=>q.id===id);if(!q||q.status!=='pending'){toast('此请求已结束，请勿重复提交');return;}
  if(status==='answered'){
    const answers=q.values.map(value=>q.options.find(o=>o[0]===value)?.[1]).filter(Boolean);if(q.freeform.trim())answers.push(q.freeform.trim());
    if(!answers.length){q.error='请选择选项或填写答案；补充说明不能代替回答。';refreshTimeline();return;}q.answer=answers.join('；');
  }
  if(status==='external')q.answer='响应式 Web（另一设备的模拟回应）';
  q.status=status;const c=runtimeSession().calls.find(c=>c.id===id);c.status=['answered','external'].includes(status)?'done':'cancelled';c.output=q.answer||'未回答';refreshTimeline();updateStatus();
}
function renderGoalExample(){runtimeModal('目标与待办',`<div class="goal-example"><span class="capability-badge">goal · 展示已有目标状态的样例</span><h3>完成工作台主题适配</h3><p>范围：主题变量、用户偏好、桌面与手机体验。</p><div><span>轮次预算 <strong>3 / 12</strong></span><span>时间预算 <strong>8 / 30 分钟</strong></span></div></div><div class="settings-row"><div><strong>todo · 当前会话分支</strong><small>稳定任务 ID，按当前分支恢复</small></div><span>2 / 3</span></div><p class="modal-help">目标是持续工作约束，todo 是步骤。此处仅展示样例，没有创建目标、自动任务或后台运行。</p><button class="button" data-capability="show-todo">回到对话中的待办进度</button>`,'',true);}
function commonTools(){runtimeModal('常用能力',`<p>每种能力使用合适的交互，结果仍归属于同一会话。</p><div class="common-tool-list">${[['stream-demo','流式消息','逐步生成正文与工具输出，演示停止、重连与结算'],['ask-single','ask_user','单选提问，用户明确回答后才返回'],['ask-multiple','ask_user · 多选','选择多项并补充说明'],['ask-freeform','ask_user · 自由回答','没有预设选项的需求澄清'],['btw-open','BTW 旁问','独立历史、独立取消、不打断主任务'],['show-todo','todo','把会话待办折叠进对话中的计划卡'],['goal','goal','查看目标、预算与持续工作状态样例'],['skill-picker','Skills','发现与指定技能，展示全文载入证据'],['tools-config','工具与权限','查看文件、命令、网络与 MCP 能力']].map(([action,title,desc])=>`<button class="common-tool-row" data-capability="${action}"><strong>${title}</strong><span>${desc}</span>${icon('chevron-right')}</button>`).join('')}</div>`,'',true);}
function renderBtwPanel(){
  const b=capabilities.btw,el=$('#btw-panel'),history=$('#btw-history'),scroll=history?.scrollTop||0,atBottom=!history||history.scrollHeight-history.clientHeight-scroll<32,focused=document.activeElement?.id==='btw-input',cursor=focused?document.activeElement.selectionStart:0;
  el.innerHTML=`<div class="btw-header"><div><strong>BTW <span>旁路对话</span></strong><small>主任务继续 · 无工具权限</small></div><button class="icon-button" data-capability="btw-hide" aria-label="隐藏旁问">${icon('x')}</button></div><div class="btw-boundary">${icon('shield')}独立历史，不自动写入主会话。仅参考最近已提交文本的快照，不含图片与工具结果。</div><div class="btw-actions"><button data-capability="btw-new">新话题</button><button data-capability="btw-cancel" ${!b.active&&!b.queue.length?'disabled':''}>取消旁问</button><span>${b.queue.length?'排队 '+b.queue.length:b.active?'生成中':'就绪'} · 演示</span></div><div id="btw-history" class="btw-history">${b.history.length?b.history.map(item=>`<article class="btw-turn" data-btw-id="${item.id}"><div class="btw-question">${escapeHTML(item.question)}</div><div class="btw-answer">${escapeHTML(item.answer||(item.status==='running'?'正在模拟回答…':'已取消，未形成完整回答。')).replace(/\n/g,'<br>')}</div><div class="btw-turn-meta">${escapeHTML(item.provider)} · ${escapeHTML(item.thinking)} · ${item.status==='completed'?'已完成':item.status==='running'?'生成中':'已取消'}${item.status==='completed'?`<button data-capability="btw-reference" data-id="${item.id}">引用到主输入框</button>`:''}</div></article>`).join(''):'<div class="btw-empty">顺便问个问题。<p>例如：为什么把技能与工具分开显示？</p><button class="button" data-capability="btw-example">试问这个问题</button></div>'}${b.queue.length?'<div class="btw-queued">'+b.queue.map(item=>'<p>等待：'+escapeHTML(item.question)+'</p>').join('')+'</div>':''}</div><div class="btw-composer"><textarea id="btw-input" rows="2" placeholder="旁问，不打断主任务…" aria-label="旁问消息">${escapeHTML(b.input)}</textarea><div><small>${b.recent?'仅保留最近历史 · ':''}Enter 发送 · 独立演示用量</small><button class="send-button" data-capability="btw-send" aria-label="发送旁问">${icon('arrow-up')}</button></div></div>`;
  if($('#btw-history'))$('#btw-history').scrollTop=atBottom?$('#btw-history').scrollHeight:scroll;
  if(focused){$('#btw-input').focus();$('#btw-input').setSelectionRange(cursor,cursor);}
}
function openBtw(question){const b=capabilities.btw;b.visible=true;if($('#modal').open)$('#modal').close();renderCapabilitiesUI();if(question){b.input=question;sendBtw();}else $('#btw-input')?.focus();}
function sendBtw(){
  const b=capabilities.btw,question=($('#btw-input')?.value||b.input).trim();if(!question)return;
  if(new TextEncoder().encode(question).length>20000){toast('旁问最多 20,000 UTF-8 字节');return;}
  if(b.queue.length>=8){toast('旁问待执行队列最多 8 条');return;}
  const item={id:crypto.randomUUID(),question,answer:'',status:'queued',provider:state.model,thinking:runtime.thinking};b.queue.push(item);b.input='';startBtwNext();if(b.visible)renderBtwPanel();
}
function startBtwNext(){
  const b=capabilities.btw;if(b.active||!b.queue.length)return;
  const item=b.queue.shift(),epoch=b.epoch,owner=state.session;item.status='running';b.active=item;b.history.push(item);
  if(b.history.length>16){b.history.shift();b.recent=true;}
  const answer='旁问示例：工具负责执行动作，Skill 提供操作指导。\n“可发现”只表示进入候选目录，“全文已载入”才有读取证据；是否采用还需要更多证据。\n此回复没有读取文件、调用工具或写入主会话。',fragments=answer.match(/[\s\S]{1,8}/gu)||[];
  const tick=()=>{
    if(b.epoch!==epoch||capabilities.owner!==owner)return;
    item.answer+=fragments.shift()||'';
    const history=$('#btw-history'),pinned=!history||history.scrollHeight-history.clientHeight-history.scrollTop<32,node=$(`[data-btw-id="${item.id}"] .btw-answer`);
    if(node)node.textContent=item.answer;
    if(history&&pinned)history.scrollTop=history.scrollHeight;
    if(fragments.length)b.timer=setTimeout(tick,70);
    else{item.status='completed';b.active=null;startBtwNext();renderCapabilitiesUI();}
  };
  b.timer=setTimeout(tick,180);
}
function cancelBtw(){const b=capabilities.btw;clearTimeout(b.timer);b.epoch++;if(b.active)b.active.status='cancelled';b.active=null;b.queue=[];if(b.visible)renderBtwPanel();renderCapabilitiesUI();}
function handleCapabilityCommand(text){
  if(text==='/skills'){navigate('skills');return true;}
  if(text==='/ask'||text==='/ask_user'){askSample('single');return true;}
  if(text==='/todos'){focusTimeline('plan');return true;}
  if(text==='/goal'){renderGoalExample();return true;}
  if(text==='/btw'||text.startsWith('/btw ')){
    const rest=text.slice(4).trim();if(rest==='hide'){capabilities.btw.visible=false;renderCapabilitiesUI();}else if(rest==='cancel')cancelBtw();else if(rest==='clear'){resetBtwThread();renderCapabilitiesUI();}else if(rest==='show'||!rest)openBtw();else if(rest==='status'){openBtw();toast(`旁问 ${capabilities.btw.active?'生成中':'就绪'}，待执行 ${capabilities.btw.queue.length} 条`);}else if(rest.startsWith('new ')){resetBtwThread();openBtw(rest.slice(4));}else openBtw(rest);return true;
  }
  if(text.startsWith('/skill:')){const name=text.split(/\s+/)[0].slice(7),skill=skillSamples.find(s=>s.name===name&&sessionSkillAvailable(s));if(skill){pinSkill(skill.id);if(text.trim().includes(' '))return false;toast('已指定技能，请补充任务后发送');}else toast('当前示例目录中没有这个可用技能');return true;}
  return false;
}
document.addEventListener('click',event=>{
  const button=event.target.closest('[data-capability]');if(!button||button.disabled)return;
  const id=button.dataset.id,b=capabilities.btw;
  switch(button.dataset.capability){
    case 'stream-demo':$('#modal').close();startStreamDemo();break;
    case 'skill-picker':skillPicker();break;
    case 'skill-pin':pinSkill(id);if($('#modal').open)$('#modal').close();if(state.page==='skills')renderSkillCatalogue();break;
    case 'skill-unpin':skillSession().draftSkills=skillSession().draftSkills.filter(s=>s!==id);renderCapabilitiesUI();break;
    case 'skill-detail':skillDetail(id);break;
    case 'skill-detail-tab':skillDetail(capabilities.skillDetailId,button.dataset.value);break;
    case 'common-tools':commonTools();break;
    case 'ask-single':askSample('single');break;
    case 'ask-multiple':askSample('multiple');break;
    case 'ask-freeform':askSample('freeform');break;
    case 'ask-new':askSample(button.dataset.mode);break;
    case 'ask-submit':answerAsk(id);break;
    case 'ask-cancel':answerAsk(id,'cancelled');break;
    case 'ask-expire':answerAsk(id,'expired');break;
    case 'ask-external':answerAsk(id,'external');break;
    case 'show-todo':if($('#modal').open)$('#modal').close();focusTimeline('plan');break;
    case 'goal':renderGoalExample();break;
    case 'tools-config':toolSettings();break;
    case 'btw-open':openBtw();break;
    case 'btw-hide':b.visible=false;renderCapabilitiesUI();break;
    case 'btw-new':resetBtwThread();renderCapabilitiesUI();break;
    case 'btw-cancel':cancelBtw();break;
    case 'btw-send':sendBtw();break;
    case 'btw-example':openBtw('为什么把技能与工具分开显示？');break;
    case 'btw-reference':{const item=b.history.find(i=>i.id===id);if(item?.status!=='completed')return;$('#prompt').value+=`${$('#prompt').value?'\n\n':''}[引用旁问]\n${item.answer}\n\n`;b.visible=false;navigate('workspace');state.tab='conversation';renderTab();$('#prompt').focus();toast('仅插入主输入框，尚未发送');break;}
  }
});
document.addEventListener('input',event=>{
  if(event.target.id==='skill-search'){capabilities.skillQuery=event.target.value;renderSkillCatalogue();}
  if(event.target.id==='btw-input')capabilities.btw.input=event.target.value;
  const card=event.target.closest('[data-ask-id]');if(!card)return;const q=skillSession().askRequests.find(q=>q.id===card.dataset.askId);if(!q||q.status!=='pending')return;
  if(event.target.hasAttribute('data-ask-freeform'))q.freeform=event.target.value;
  if(event.target.hasAttribute('data-ask-comment'))q.comment=event.target.value;
  if(q.error){q.error='';card.querySelector('.ask-error').textContent='';}
});
document.addEventListener('change',event=>{
  if(event.target.id==='skill-scope'){capabilities.skillScope=event.target.value;renderSkillCatalogue();}
  if(event.target.id==='skill-used'){capabilities.skillUsed=event.target.checked;renderSkillCatalogue();}
  const card=event.target.closest('[data-ask-id]');if(!card||!event.target.hasAttribute('data-ask-value'))return;
  const q=skillSession().askRequests.find(q=>q.id===card.dataset.askId);if(!q||q.status!=='pending')return;
  q.values=q.mode==='multiple'?[...card.querySelectorAll('[data-ask-value]:checked')].map(el=>el.dataset.askValue):[event.target.dataset.askValue];q.error='';
  card.querySelector('.ask-error').textContent='';
});
document.addEventListener('keydown',event=>{if(event.target.id==='btw-input'&&event.key==='Enter'&&!event.shiftKey&&!event.isComposing){event.preventDefault();sendBtw();}if(event.key==='Escape'&&capabilities.btw.visible){capabilities.btw.visible=false;renderCapabilitiesUI();}});
