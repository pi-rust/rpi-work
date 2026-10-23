const $ = (s, root = document) => root.querySelector(s);
const $$ = (s, root = document) => [...root.querySelectorAll(s)];
const paths = {
  search: '<circle cx="10.5" cy="10.5" r="6.5"/><path d="m16 16 4.5 4.5"/>',
  layout: '<rect x="3" y="3" width="18" height="18" rx="3"/><path d="M9 3v18M9 9h12"/>',
  blocks: '<rect x="3" y="3" width="7" height="7" rx="1"/><rect x="14" y="3" width="7" height="7" rx="1"/><rect x="3" y="14" width="7" height="7" rx="1"/><rect x="14" y="14" width="7" height="7" rx="1"/>',
  monitor: '<rect x="3" y="3" width="18" height="13" rx="2"/><path d="M8 21h8m-4-5v5"/>',
  plus: '<path d="M12 5v14M5 12h14"/>',
  'chevron-down': '<path d="m6 9 6 6 6-6"/>',
  'chevron-right': '<path d="m9 6 6 6-6 6"/>',
  chevrons: '<path d="m8 8 4-4 4 4m-8 8 4 4 4-4"/>',
  settings: '<path d="m9 3-1 3-3 1v3l-2 2 2 2v3l3 1 1 3h6l1-3 3-1v-3l2-2-2-2V7l-3-1-1-3z"/><circle cx="12" cy="12" r="3"/>',
  folder: '<path d="M3 7V5a2 2 0 0 1 2-2h5l2 3h7a2 2 0 0 1 2 2v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/>',
  panel: '<rect x="3" y="3" width="18" height="18" rx="2"/><path d="M15 3v18"/>',
  menu: '<path d="M4 6h16M4 12h16M4 18h16"/>',
  more: '<circle cx="5" cy="12" r="1"/><circle cx="12" cy="12" r="1"/><circle cx="19" cy="12" r="1"/>',
  branch: '<circle cx="6" cy="5" r="2"/><circle cx="18" cy="5" r="2"/><circle cx="6" cy="19" r="2"/><path d="M6 7v10m0-5c8 0 12-1 12-5"/>',
  message: '<path d="M21 11.5a8.5 8.5 0 0 1-8.5 8.5H4l-2 2V11.5A8.5 8.5 0 1 1 21 11.5Z"/>',
  diff: '<rect x="4" y="3" width="16" height="18" rx="2"/><path d="M8 8h8m-4-3v6m-4 5h8"/>',
  terminal: '<rect x="3" y="4" width="18" height="16" rx="2"/><path d="m7 8 4 4-4 4m6 0h4"/>',
  checklist: '<rect x="4" y="4" width="16" height="17" rx="2"/><path d="M9 2v4m6-4v4M8 11l1 1 2-2m2 1h3m-8 5h8"/>',
  sparkles: '<path d="m12 3 2.5 6.5L21 12l-6.5 2.5L12 21l-2.5-6.5L3 12l6.5-2.5ZM20 2v4m-2-2h4"/>',
  'arrow-up': '<path d="M12 20V4m-6 6 6-6 6 6"/>',
  x: '<path d="m6 6 12 12M6 18 18 6"/>',
  check: '<path d="m5 12 4 4L19 6"/>',
  'circle-check': '<circle cx="12" cy="12" r="9"/><path d="m8 12 3 3 5-6"/>',
  clock: '<circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/>',
  file: '<path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><path d="M14 2v6h6"/>',
  shield: '<path d="m12 2 8 4v6c0 5-8 10-8 10S4 17 4 12V6z"/><path d="m8 12 3 3 5-6"/>',
  info: '<circle cx="12" cy="12" r="9"/><path d="M12 11v6m0-10v1"/>',
  link: '<path d="m10 13 4-4m-7 1-3 3a4 4 0 0 0 6 6l3-3m-2-8 3-3a4 4 0 0 1 6 6l-3 3"/>',
  phone: '<rect x="6" y="2" width="12" height="20" rx="3"/><path d="M10 5h4m-3 14h2"/>',
  stop: '<rect x="6" y="6" width="12" height="12" rx="1"/>',
  server: '<rect x="3" y="3" width="18" height="7" rx="2"/><rect x="3" y="14" width="18" height="7" rx="2"/><path d="M7 6.5h.01M7 17.5h.01"/>',
  sun: '<circle cx="12" cy="12" r="4"/><path d="M12 2v2m0 16v2M2 12h2m16 0h2M5 5l1 1m12 12 1 1M5 19l1-1M18 6l1-1"/>',
};
const icon = (name) => `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${paths[name] || paths.file}</svg>`;
const escapeHTML = (s) => String(s).replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
function hydrateIcons(root = document) { $$('[data-icon]', root).forEach(el => { el.outerHTML = icon(el.dataset.icon); }); }
const state = {
  page: 'workspace', tab: 'conversation', session: 'dark', model: 'DeepSeek', mode: '执行', approval: 'pending', running: false,
  project: 'rpi-work', selectedFile: 0, pairing: false, timer: null,
  sessions: [
    {id:'dark', title:'为工作台添加深色模式', project:'rpi-work'},
    {id:'architecture', title:'梳理 Agent 运行时架构', project:'rpi-work'},
    {id:'mobile', title:'优化手机端任务体验', project:'rpi-work'},
    {id:'rpc', title:'检查 RPC 事件流', project:'pi-rust'},
    {id:'plugin', title:'整理扩展加载方式', project:'rpi-package'}
  ],
  messages: {}, enabled: {'rpi-plan-mode':true,'rpi-permissions':true,'rpi-todo':true,'rpi-web':true,'rpi-mcp-adapter':false,'rpi-subagents':false}
};
const files = [
  {name:'theme.css',path:'assets/theme.css',added:28,removed:6,lines:[['','1','/* Workspace theme tokens */'],['del','2',':root {'],['del','3','  --background: #ffffff;'],['del','4','  --foreground: #282b27;'],['del','5','}'],['add','2',':root, [data-theme="light"] {'],['add','3','  color-scheme: light;'],['add','4','  --background: #fcfcfa;'],['add','5','  --foreground: #282b27;'],['add','6','}'],['add','7',''],['add','8','[data-theme="dark"] {'],['add','9','  color-scheme: dark;'],['add','10','  --background: #22261f;'],['add','11','  --foreground: #e1e5d9;'],['add','12','}']]},
  {name:'theme.rs',path:'src/components/theme.rs',added:31,removed:2,lines:[['','1','use dioxus::prelude::*;'],['add','2',''],['add','3','#[derive(Clone, Copy, PartialEq)]'],['add','4','pub enum Theme { Light, Dark, System }'],['add','5',''],['add','6','#[component]'],['add','7','pub fn ThemeToggle() -> Element {'],['add','8','    let mut dark = use_signal(|| false);'],['add','9','    rsx! {'],['add','10','        button {'],['add','11','            onclick: move |_| dark.toggle(),'],['add','12','            "切换主题"'],['add','13','        }'],['add','14','    }'],['add','15','}']]},
  {name:'settings.rs',path:'src/state/settings.rs',added:8,removed:2,lines:[['','1','use serde::{Deserialize, Serialize};'],['add','2',''],['add','3','#[derive(Serialize, Deserialize, Default)]'],['add','4','pub struct Appearance {'],['add','5','    pub theme: String,'],['add','6','    pub follow_system: bool,'],['add','7','}']]}
];
function toast(message) { const el=$('#toast'); el.textContent=message; el.classList.add('show'); clearTimeout(toast.timer); toast.timer=setTimeout(()=>el.classList.remove('show'),3300); }
function renderSessions() {
  $('#session-list').innerHTML = state.sessions.filter(s=>s.project===state.project).map(s=>`<button class="session-item ${s.id===state.session?'current':''}" data-session="${s.id}"><span class="session-dot ${s.id==='dark'?'':'done'}"></span><span>${escapeHTML(s.title)}</span></button>`).join('');
  $$('.project-row').forEach(el=>el.classList.toggle('selected',el.dataset.project===state.project));
}
function agentMessage(content, label='rpi', extra='') {
  return `<article class="message"><div class="message-avatar agent-avatar">r↗</div><div class="message-content"><div class="message-author">${label}<span class="via">${escapeHTML(state.model)} · 演示</span></div>${content}${extra}</div></article>`;
}
function approvalCard() {
  if(state.approval==='pending') return `<div class="approval"><div class="approval-heading">${icon('shield')}运行测试前，需要你的确认</div><p>Agent 希望在项目目录执行以下命令。</p><pre>cargo test -p work-ui --lib</pre><div class="approval-actions"><button class="button primary" id="approve">${icon('check')}允许这一次</button><button class="button" id="reject">拒绝</button><span class="approval-foot">仅此命令 · 此会话</span></div></div>`;
  if(state.approval==='approved') return `<div class="result-card"><div>${icon('circle-check')}验证完成 · 演示结果</div><p>12 项测试通过。可以查看 3 个文件的变更，继续提出修改意见。</p></div>`;
  return `<div class="info-banner">${icon('shield')}已拒绝执行命令。Agent 保留文件变更，等待你的下一步指示。此操作为演示。</div>`;
}
function defaultConversation() { return renderUnifiedConversation(); }
function renderConversation() {
  let html = state.session==='dark' ? defaultConversation() : `<div class="empty-state"><div class="empty-symbol">${icon('sparkles')}</div><div class="eyebrow">A LITTLE IDEA. A NEW POSSIBILITY.</div><h2>从一个想法开始</h2><p>告诉 Agent 你想做什么，接下来的步骤一起完成。</p><div class="suggestions"><button class="suggestion" data-suggest="帮我梳理这个项目的结构与关键入口">梳理项目结构</button><button class="suggestion" data-suggest="先制定实现计划，暂时不要修改文件">先制定计划</button><button class="suggestion" data-suggest="检查最近的代码变更，并指出潜在问题">审阅代码变更</button></div></div>`;
  (state.messages[state.session] || []).forEach(m=>{ html+=m.role==='stream'?renderStreamMessage(m):m.role==='user'?`<article class="message"><div class="message-avatar">B</div><div class="message-content"><div class="message-author">你<span class="via">刚刚</span></div><p>${escapeHTML(m.text).replace(/\n/g,'<br>')}</p>${renderMessageAttachments(m.attachments||[])}${renderSkillMessageRefs(m.skills||[])}${renderResourceSnapshot(m.resourceSnapshot)}</div></article>`:agentMessage(`<p>${escapeHTML(m.text)}</p>`); });
  if(state.session==='dark') html=html.replace('<button class="context-chip"', renderReferenceAttachments()+'<button class="context-chip"');
  $('#task-body').innerHTML=html;
  if(runtimeSession().compact) $('#task-body').insertAdjacentHTML('beforeend',renderCompactionNotice());
  $('#task-body').insertAdjacentHTML('beforeend',renderAdditionalConversation());
  renderStreamChrome();
}
function renderChanges() { renderChangeWorkspace(); }
function renderInspector() {
  if(state.session!=='dark') { $('#inspector-content').innerHTML=`<div class="inspector-section"><div class="inspector-label">执行环境 ${icon('monitor')}</div><div class="environment-card"><div class="environment-heading"><span>${icon('monitor')}</span><div>这台电脑<small>Windows · 本地节点 · 演示</small></div></div><div class="environment-path">D:/Projects/${escapeHTML(state.project)}</div></div><div class="data-line"><span>运行时</span><span>rpi harness</span></div><div class="data-line"><span>模型提供方</span><span>${escapeHTML(state.model)}</span></div></div><div class="inspector-section"><div class="inspector-label">任务上下文</div><p class="view-subtitle">这是一个新的演示会话。计划、工具记录与文件变更会随真实任务进展更新。</p></div>`; return; }
  $('#inspector-content').innerHTML=`<div class="inspector-section"><div class="inspector-label">执行环境 ${icon('monitor')}</div><div class="environment-card"><div class="environment-heading"><span>${icon('monitor')}</span><div>这台电脑<small>Windows · 本地节点</small></div><i class="status-dot"></i></div><div class="environment-path">D:/Projects/${escapeHTML(state.project)}</div></div><div class="data-line"><span>运行时</span><span>rpi harness</span></div><div class="data-line"><span>模型提供方</span><span>${escapeHTML(state.model)}</span></div><div class="data-line"><span>权限策略</span><span>${icon('shield')}按需确认</span></div><button class="small-link" data-nav="devices">查看执行节点 ${icon('chevron-right')}</button></div><div class="inspector-section"><div class="inspector-label">任务计划 ${icon('checklist')}</div><div class="progress-title"><span>深色模式适配</span><span>${state.approval==='approved'?'3':'2'}/3</span></div><div class="plan-list"><div class="plan-step">${icon('circle-check')}定义主题变量</div><div class="plan-step">${icon('circle-check')}添加主题偏好设置</div><div class="plan-step ${state.approval==='approved'?'':'pending'}">${icon(state.approval==='approved'?'circle-check':'clock')}验证桌面与手机体验</div></div><div class="progress-track"><span style="width:${state.approval==='approved'?'100':'66'}%"></span></div><button class="small-link" data-timeline="plan">查看完整计划 ${icon('chevron-right')}</button></div><div class="inspector-section"><div class="inspector-label">本轮涉及文件 <span>3 个文件</span></div>${files.map((f,i)=>`<button class="file-row" data-inspector-file="${i}">${icon('file')}<span>${f.name}</span><span class="added">+${f.added}</span><span class="removed">−${f.removed}</span></button>`).join('')}</div><div class="inspector-section"><div class="inspector-label">会话扩展 <span>演示配置</span></div><span class="extension-pill">${icon('checklist')}plan-mode</span><span class="extension-pill">${icon('shield')}permissions</span><span class="extension-pill">${icon('check')}todo</span></div><div class="inspector-section"><div class="inspector-label">框架提案 <span>待讨论</span></div><div class="architecture-mini"><div class="architecture-layer">Dioxus UI<small>Desktop / Web / Mobile</small></div><div class="architecture-layer">rpi-web · HTTP + SSE<small>会话管理 / 事件恢复 / 文件预览</small></div><div class="architecture-layer">rpi · native host<small>模型 / 工具 / 扩展 / 持久会话</small></div></div></div>`;
}
function updateStatus() {
  const el=$('#task-status'); let text='等待你的确认', cls='waiting';
  if(hasPendingAsk()){text='等待你的回答 · 演示';cls='waiting';}
  else if(state.running){text='正在执行 · 演示';cls='running';}
  else if(state.session!=='dark'){text='就绪';cls='completed';}
  else if(state.approval==='approved'){text='已完成 · 演示';cls='completed';}
  else if(state.approval==='rejected'){text='等待下一步指示';cls='stopped';}
  const stream=latestStream();
  if(stream&&!hasPendingAsk()){text=streamStatus(stream)+' · 演示';cls=stream.phase==='completed'?'completed':streamTerminal(stream)?'stopped':stream.phase==='awaiting_settlement'?'waiting':'running';}
  el.className=`status-badge ${cls}`; el.innerHTML=`<i></i>${text}`;
  $('#send-button').innerHTML=icon(state.running?'stop':'arrow-up');$('#send-button').setAttribute('aria-label',state.running?'停止生成':'发送消息');
}
function renderTab() {
  $$('.tab').forEach(t=>{ t.classList.toggle('active',t.dataset.tab===state.tab); t.setAttribute('aria-selected',String(t.dataset.tab===state.tab)); });
  $('#task-body').classList.toggle('changes-body',state.tab==='changes');
  $('#app').classList.toggle('review-mode',state.tab==='changes');
  (state.tab==='changes'?renderChanges:renderConversation)();
  updateStatus();
  renderHarnessUI();
}
function setTab(tab) { if(['tools','terminal','plan'].includes(tab)){focusTimeline(tab);return;} navigate('workspace'); state.tab=tab; renderTab(); $('#task-body').scrollTop=0; $('#inspector').classList.remove('visible'); }
function selectSession(id) {
  if(state.running && state.session!==id) { toast('请先等待演示完成或停止当前生成'); return; }
  const s=state.sessions.find(s=>s.id===id); if(!s)return;
  runtimeSession().textDraft=$('#prompt').value;
  state.session=id;state.project=s.project; state.tab='conversation';
  $('#prompt').value=runtimeSession().textDraft||'';
  $('#task-title').textContent=s.title;$('#breadcrumb-project').textContent=s.project;$('#breadcrumb-title').textContent=s.title;
  $('.tab .count').textContent=id==='dark'?'120':'0';
  $('#task-branch').textContent=id==='dark'?'feature/dark-mode':'main';
  closeSidebar(); navigate('workspace'); renderSessions(); renderTab(); renderInspector();renderInspectorRuntime();
}
function closeSidebar() { $('.sidebar').classList.remove('open');$('#scrim').classList.remove('visible'); }
function navigate(page) {
  state.page=page; $$('.primary-nav .nav-item').forEach(n=>n.classList.toggle('active',n.dataset.nav===page));
  $('#workspace-view').hidden=page!=='workspace'; $('#page-view').hidden=page==='workspace';
  $('#breadcrumb-title').textContent=page==='workspace'?(state.sessions.find(s=>s.id===state.session)?.title||'新任务'):page==='extensions'?'扩展':'执行节点';
  if(page!=='devices')stopPairCamera(); if(page==='extensions')renderExtensions(); if(page==='devices')renderDevices(); closeSidebar();
  if(page==='providers') {$('#breadcrumb-title').textContent='Providers';renderProviders();}
  if(page==='skills') {$('#breadcrumb-title').textContent='Skills';renderSkillCatalogue();}
  if(page==='packages') {$('#breadcrumb-title').textContent='Packages';renderPackageLibrary();}
}
function renderExtensions() {
  const extensions=[['rpi-plan-mode','checklist','先梳理计划，再进入执行。把复杂工作拆成可确认的步骤。','已有扩展'],['rpi-permissions','shield','按项目配置工具权限，在需要时请求单次执行确认。','已有扩展'],['rpi-todo','check','追踪任务步骤，让长时间工作始终保持方向。','已有扩展'],['rpi-web','layout','连接原生 Agent，提供会话、文件与 Git 预览能力。','基础桥接'],['rpi-mcp-adapter','link','接入 MCP 工具，把已有服务加入工作流程。','已有扩展'],['rpi-subagents','blocks','为适合并行的任务分配子 Agent，共享受限能力。','已有扩展']];
  $('#page-view').innerHTML=`<div class="overline">MAKE IT YOURS.</div><div class="page-heading"><div><h1>刚好适合你的工作方式</h1><p>从 rpi 扩展生态中组合能力，保持核心轻巧。</p></div></div><div class="info-banner">${icon('info')}以下名称来自本机扩展仓库；开关仅演示会话配置体验，不会安装、启用或卸载真实扩展。</div><div class="card-grid">${extensions.map(([name,ic,desc,badge])=>`<article class="extension-card"><div class="extension-card-top"><span class="extension-icon">${icon(ic)}</span><button class="switch ${state.enabled[name]?'on':''}" role="switch" aria-checked="${state.enabled[name]}" aria-label="切换 ${name}" data-extension="${name}"></button></div><h3>${name}</h3><p>${desc}</p><div class="extension-card-footer"><span>${badge}</span><span>${state.enabled[name]?'已选择':'未选择'} · 演示</span></div></article>`).join('')}</div>`;
}
function renderDevices(){renderDeviceWorkspace();}
function showModal(title,body,footer='') {
  $('#modal').classList.remove('wide-modal');
  $('#modal-content').innerHTML=`<div class="modal-header"><h2>${title}</h2><button class="icon-button" data-close-modal aria-label="关闭弹窗">${icon('x')}</button></div><div class="modal-body">${body}</div>${footer?`<div class="modal-footer">${footer}</div>`:''}`;
  if(!$('#modal').open)$('#modal').showModal();
}
function newTask() {
  showModal('新建任务',`<p>选择项目，给这次工作一个清晰的起点。</p><div class="field"><label for="new-title">任务名称</label><input id="new-title" placeholder="例如：整理项目架构" maxlength="60" autocomplete="off"></div><div class="field"><label for="new-project">项目</label><select id="new-project"><option>rpi-work</option><option>pi-rust</option><option>rpi-package</option></select></div><div class="field"><label for="new-model">运行时与模型</label><select id="new-model"><option>rpi / DeepSeek</option><option>rpi / OpenAI</option></select></div>`,`<button class="button" data-close-modal>取消</button><button class="button primary" id="create-task">创建任务</button>`);
  $('#new-project').value=state.project; $('#new-title').focus();
}
function modelModal() {
  showModal('选择模型与运行时',`<p>模型与运行时分别配置。此处仅改变原型显示，不发起 API 请求。</p>${['DeepSeek','OpenAI'].map(m=>`<button class="model-option ${state.model===m?'selected':''}" data-model="${m}"><span class="model-logo">${m[0]}</span><div><strong>${m}</strong><small>rpi harness · 模型由宿主发现</small></div>${state.model===m?icon('check'):''}</button>`).join('')}<div class="model-option"><span class="extension-icon">${icon('terminal')}</span><div><strong>Codex app-server</strong><small>规划中 · 独立运行时适配，不属于模型切换</small></div></div>`);
}
function searchModal() {
  showModal('找到你的工作',`<input class="search-input" id="search-input" placeholder="搜索项目或任务…" aria-label="搜索项目或任务"><div class="search-results" id="search-results"></div>`); renderSearch('');$('#search-input').focus();
}
function renderSearch(q) { const results=state.sessions.filter(s=>(s.title+s.project).toLowerCase().includes(q.toLowerCase())); $('#search-results').innerHTML=results.map(s=>`<button class="search-result" data-search-session="${s.id}">${icon('message')}<span>${escapeHTML(s.title)}<small class="text-muted"> · ${escapeHTML(s.project)}</small></span></button>`).join('')||'<p class="text-muted">没有匹配的项目或任务</p>'; }
function sendMessage() { return submitHarnessMessage(); }
document.addEventListener('click',event=>{
  const b=event.target.closest('button'); if(!b)return;
  if(b.hasAttribute('data-close-modal')) {$('#modal').close();return;}
  if(b.dataset.nav){navigate(b.dataset.nav);return;}
  if(b.dataset.tab){setTab(b.dataset.tab);return;}
  if(b.dataset.switchTab){setTab(b.dataset.switchTab);return;}
  if(b.dataset.session){selectSession(b.dataset.session);return;}
  if(b.dataset.project){const s=state.sessions.find(s=>s.project===b.dataset.project);if(s)selectSession(s.id);return;}
  if(b.dataset.tool){const el=$(`#tool-${b.dataset.tool}`);el.hidden=!el.hidden;return;}
  if(b.hasAttribute('data-file')){state.selectedFile=Number(b.dataset.file);renderChanges();return;}
  if(b.hasAttribute('data-inspector-file')){state.selectedFile=Number(b.dataset.inspectorFile);reviewState().scope='turn';reviewState().mobileDiff=true;setTab('changes');return;}
  if(b.dataset.model){state.model=b.dataset.model;$('#model-label').textContent=state.model;renderInspector();if(state.page==='workspace')renderTab();$('#modal').close();toast(`已切换为 ${state.model} · 演示配置`);return;}
  if(b.dataset.suggest){$('#prompt').value=b.dataset.suggest;$('#prompt').focus();return;}
  if(b.dataset.searchSession){$('#modal').close();selectSession(b.dataset.searchSession);return;}
  if(b.dataset.extension){state.enabled[b.dataset.extension]=!state.enabled[b.dataset.extension];renderExtensions();toast('已更新演示配置，未改变真实扩展');return;}
  if(b.dataset.action==='context'){toast(`上下文目录：D:/Projects/${state.project}（演示）`);return;}
  switch(b.id){
    case 'new-task': newTask();break;
    case 'create-task': {const input=$('#new-title');const title=input.value.trim();if(!title){input.focus();input.setAttribute('aria-invalid','true');toast('请填写任务名称');break;}const id=`task-${Date.now()}`;const project=$('#new-project').value;state.model=$('#new-model').value.includes('DeepSeek')?'DeepSeek':'OpenAI';$('#model-label').textContent=state.model;state.sessions.unshift({id,title,project});$('#modal').close();selectSession(id);toast('已创建演示任务');break;}
    case 'settings-button':settingsModal();break;
    case 'search-button':searchModal();break;
    case 'send-button':sendMessage();break;
    case 'model-button':modelModal();break;
    case 'mode-button':state.mode=state.mode==='执行'?'规划':'执行';$('#mode-label').textContent=state.mode;toast(`切换到${state.mode}模式 · 演示`);break;
    case 'menu-toggle':$('.sidebar').classList.toggle('open');$('#scrim').classList.toggle('visible');break;
    case 'scrim':closeSidebar();break;
    case 'context-toggle':if(state.tab==='changes'||innerWidth<=1040||$('#app').classList.contains('phone-preview'))$('#inspector').classList.toggle('visible');else $('#app').classList.toggle('context-hidden');break;
    case 'context-close':$('#inspector').classList.remove('visible');$('#app').classList.add('context-hidden');break;
    case 'preview-toggle':$('#app').classList.toggle('phone-preview');b.innerHTML=$('#app').classList.contains('phone-preview')?'桌面预览 <span>↗</span>':'手机预览 <span>↗</span>';closeSidebar();break;
    case 'approve':state.approval='approved';renderTab();renderInspector();renderSessions();toast('已允许，示例测试结果已展示（未执行命令）');break;
    case 'reject':state.approval='rejected';renderTab();renderInspector();toast('已拒绝此演示命令');break;
    case 'feedback-diff':setTab('conversation');$('#prompt').value=`请检查 ${currentChangeFile().path} 的修改，特别关注 `;$('#prompt').focus();break;
    case 'pair-device':openPairing();break;
    case 'simulate-pair':simulatePairScan();break;
    case 'unpair':revokeClient(connection.client.deviceId);break;
    case 'node-details':showModal('这台电脑',`<p>此节点承载项目文件、凭据、rpi 原生进程和持久会话。关闭手机或断开页面不会取消任务；关闭执行节点会影响运行。</p><div class="data-line"><span>连接方式</span><span>本地 HTTP + SSE</span></div><div class="data-line"><span>界面框架</span><span>Dioxus（提案）</span></div><div class="data-line"><span>执行桥接</span><span>rpi-web → rpi RPC</span></div><p>以上为架构示意。</p>`);break;
    case 'add-node':openMobileConnect('advanced');break;
    case 'attach-button':attachmentMenu();break;
    case 'add-project':showModal('打开项目',`<p>正式版桌面端通过系统目录选择器打开本机项目。手机端选择执行节点上已授权的项目。</p><div class="field"><label for="project-path">项目路径（演示）</label><input id="project-path" value="D:/Projects/rpi-work" readonly></div>`,`<button class="button primary" data-close-modal>知道了</button>`);break;
    case 'session-options':showModal('任务操作',`<button class="search-result" id="duplicate-demo">${icon('branch')}从当前任务创建分支会话（演示）</button><button class="search-result" id="reset-demo">${icon('clock')}重置默认演示场景</button>`);break;
    case 'reset-demo':disposeStreams();delete runtime.sessions.dark;delete changeState.sessions.dark;clearTimeout(state.timer);state.running=false;state.approval='pending';state.messages.dark=[];$('#modal').close();selectSession('dark');toast('已重置演示场景');break;
    case 'duplicate-demo':$('#modal').close();newTask();$('#new-title').value=`${state.sessions.find(s=>s.id===state.session).title} · 分支`;toast('演示：创建独立新会话；未复制运行时历史');break;
  }
  if(b.dataset.attach){$('#prompt').value+=` @${b.dataset.attach} `;$('#modal').close();$('#prompt').focus();}
});
document.addEventListener('input',e=>{if(e.target.id==='search-input')renderSearch(e.target.value);});
document.addEventListener('keydown',e=>{
  if((e.metaKey||e.ctrlKey)&&e.key.toLowerCase()==='k'){e.preventDefault();searchModal();}
  if((e.metaKey||e.ctrlKey)&&e.key.toLowerCase()==='n'){e.preventDefault();newTask();}
  if(e.key==='Escape'){closeSidebar();$('#inspector').classList.remove('visible');}
  if(e.target.id==='prompt'&&e.key==='Enter'&&!e.shiftKey&&!e.isComposing){e.preventDefault();sendMessage();}
});
$('#modal').addEventListener('click',e=>{if(e.target===$('#modal')){const r=$('#modal').getBoundingClientRect();if(e.clientX<r.left||e.clientX>r.right||e.clientY<r.top||e.clientY>r.bottom)$('#modal').close();}});
$('.brand').addEventListener('click',e=>{e.preventDefault();navigate('workspace');});
document.addEventListener('DOMContentLoaded',()=>{hydrateIcons();renderSessions();renderInspector();renderTab();initThemeUI();});
