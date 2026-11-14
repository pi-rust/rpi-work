'use strict';
// Local interaction demo only. No fetch, WebSocket, native bridge or storage writes.
const shell = document.querySelector('.app-shell');
const toastElement = document.querySelector('.toast');
let toastTimer;
let homeMode = false;
function toast(message) { clearTimeout(toastTimer); toastElement.textContent = message; toastElement.hidden = false; toastTimer = setTimeout(() => { toastElement.hidden = true; }, 2600); }
function syncNavigation() {
  const mobile = matchMedia('(max-width:767px)').matches;
  document.querySelector('.mobile-menu').setAttribute('aria-expanded',String(mobile ? shell.classList.contains('nav-open') : !shell.classList.contains('nav-collapsed')));
  document.querySelector('.main-shell').inert = mobile && (shell.classList.contains('nav-open') || shell.classList.contains('project-files-open'));
}
function showView(view) {
  if (view === 'home') { openHome(); return; }
  for (const dialog of document.querySelectorAll('dialog[open]')) dialog.close();
  document.body.dataset.view = view;
  for (const page of document.querySelectorAll('.page')) page.hidden = page.id !== `${view}-page`;
  for (const button of document.querySelectorAll('.nav-link[data-view]')) {
    const active = button.dataset.view === view;
    button.classList.toggle('active',active);
    if (active) button.setAttribute('aria-current','page'); else button.removeAttribute('aria-current');
  }
  document.querySelector('#section-label').textContent = {workspace:'工作台',resources:'资源中心',settings:'设置'}[view];
  shell.classList.remove('nav-open','context-open');
  setProjectFiles(false);
  syncNavigation();
  history.replaceState(null,'',`#${view === 'workspace' && homeMode ? 'home' : view}`);
  document.querySelector('.welcome-identity').hidden = view !== 'workspace' || !homeMode;
  const homeActive = view === 'workspace' && homeMode;
  document.querySelector('.welcome-nav').classList.toggle('active',homeActive);
  if (homeActive) document.querySelector('.welcome-nav').setAttribute('aria-current','page');
  else document.querySelector('.welcome-nav').removeAttribute('aria-current');
}
function showTab(tab) {
  const context = document.querySelector('#context-dialog');if (context.open) context.close();
  if (document.body.dataset.view !== 'workspace') showView('workspace');
  document.querySelector('#chat-panel').hidden = tab !== 'chat';
  document.querySelector('#changes-panel').hidden = tab !== 'changes';
}
function setProjectFiles(open) {
  const panel = document.querySelector('#project-files-panel');
  panel.hidden = !open;
  shell.classList.toggle('project-files-open',open);
  if (!open) shell.classList.remove('project-files-diff-open');
  for (const button of document.querySelectorAll('[data-action="project-files"]')) button.setAttribute('aria-expanded',String(open));
  syncNavigation();
}
function toggleProjectFiles() {
  const open = document.querySelector('#project-files-panel').hidden;
  setProjectFiles(open);
  if (open) showProjectFileList();
}
function showProjectFileList() {
  shell.classList.remove('project-files-diff-open');
  document.querySelector('.project-tree').hidden = false;
  document.querySelector('.project-changes').hidden = false;
  document.querySelector('#project-file-diff').hidden = true;
}
const projectFileDiffs = {
  'appearance.rs': {path:'apps/work-ui/src/shared/theme/appearance.rs',added:'+24',deleted:'−16',lines:'<div><span>495</span>let import = {</div><div class="deleted"><span>496</span>− &nbsp;move |_| {</div><div class="added"><span>496</span>+ &nbsp;move |event: FormEvent| {</div><div class="added"><span>497</span>+ &nbsp;&nbsp;let file = event.files().into_iter().next();</div><div><span>498</span>&nbsp;&nbsp;&nbsp;spawn(async move {</div><div class="added"><span>499</span>+ &nbsp;&nbsp;&nbsp;let text = read_import_file(file).await;</div><div><span>500</span>&nbsp;&nbsp;&nbsp;});</div><div><span>501</span>&nbsp;}</div>'},
  'theme_contract.rs': {path:'apps/work-ui/src/shared/theme/theme_contract.rs',added:'+12',deleted:'−4',lines:'<div><span>38</span>pub struct ThemeContract {</div><div class="deleted"><span>39</span>− &nbsp;pub mode: String,</div><div class="added"><span>39</span>+ &nbsp;pub mode: ThemeMode,</div><div class="added"><span>40</span>+ &nbsp;pub density: Density,</div><div><span>41</span>&nbsp;pub accent: Accent,</div><div class="added"><span>42</span>+ &nbsp;pub tool_rendering: ToolRendering,</div><div><span>43</span>}</div>'},
  'clear-blue-saas.json': {path:'examples/themes/clear-blue-saas.json',added:'+8',deleted:'−0',lines:'<div class="added"><span>1</span>+ {</div><div class="added"><span>2</span>+ &nbsp;"name": "澄蓝",</div><div class="added"><span>3</span>+ &nbsp;"surface": "#f7faff",</div><div class="added"><span>4</span>+ &nbsp;"accent": "#3264e8",</div><div class="added"><span>5</span>+ &nbsp;"contrast": "AA",</div><div class="added"><span>6</span>+ &nbsp;"density": "comfortable"</div><div class="added"><span>7</span>+ }</div>'}
};
function renderProjectFileDiff(file = 'appearance.rs') {
  const diff = projectFileDiffs[file] || projectFileDiffs['appearance.rs'];
  document.querySelector('#project-file-diff-path').textContent = diff.path;
  document.querySelector('#project-file-diff-code').innerHTML = diff.lines;
  const count = document.querySelector('#project-file-diff-path').parentElement.querySelector('.diff-count');
  count.querySelector('b').textContent = diff.added;count.querySelector('em').textContent = diff.deleted;
  for (const tab of document.querySelectorAll('[data-file-tab]')) {
    const active = tab.dataset.fileTab === file;tab.classList.toggle('active',active);tab.setAttribute('aria-selected',String(active));
  }
  mountIcons(document.querySelector('#project-file-diff'));
}
function openProjectFileDiff(file = 'appearance.rs') {
  setProjectFiles(true);
  shell.classList.add('project-files-diff-open');
  document.querySelector('.project-tree').hidden = true;
  document.querySelector('.project-changes').hidden = true;
  document.querySelector('#project-file-diff').hidden = false;
  renderProjectFileDiff(file);
}
// Curated fixtures only. Scope identity is (node id, session id), never the title.
const demoNodes = [
  {id:'local',name:'本机',sessions:[
    {id:'design',title:'重新设计工作台界面',project:'rpi-work',recent:0,state:'completed'},
    {id:'theme',title:'修复桌面主题导入',project:'rpi-work',recent:2,state:'running'},
    {id:'architecture',title:'梳理 Agent 架构',project:'pi-rust',recent:4,state:'approval'}
  ]},
  {id:'studio',name:'开发机',sessions:[
    {id:'design',title:'验证手机端布局',project:'rpi-work',recent:1,state:'failed'},
    {id:'build',title:'检查编译结果',project:'pi-rust',recent:3,state:'queued'}
  ]}
];
let currentNode = 'local', currentSession = 'design', sessionLayout = 'grouped';
const scopeKey = (node,id) => JSON.stringify([node,id]);
const sessionDrafts = new Map(), sessionPreferences = new Map();
const homeState = {node:'local',project:'rpi-work',draft:'',model:'DeepSeek V4 Pro',thinking:'高'};
const previewMessages = new Map();
let previewSequence = 0;
function rememberDraft() {
  const value = document.querySelector('#prompt').value;
  if (homeMode) homeState.draft = value;
  else sessionDrafts.set(scopeKey(currentNode,currentSession),value);
}
function syncWorkspaceMode() {
  document.body.dataset.entry = homeMode ? 'home' : 'chat';
  for (const element of document.querySelectorAll('.welcome-heading,.welcome-project,.welcome-suggestions,.welcome-footnote')) element.hidden = !homeMode;
  document.querySelector('.welcome-identity').hidden = !homeMode || document.body.dataset.view !== 'workspace';
  document.querySelector('#chat-panel').hidden = homeMode;
  document.querySelector('#changes-panel').hidden = true;
  document.querySelector('#prompt').placeholder = homeMode ? '有什么想法？尽管告诉 rpi…' : '说说接下来要做什么…';
  const active = homeMode && document.body.dataset.view === 'workspace';
  document.querySelector('.welcome-nav').classList.toggle('active',active);
  if (active) document.querySelector('.welcome-nav').setAttribute('aria-current','page');
  else document.querySelector('.welcome-nav').removeAttribute('aria-current');
  const node = demoNodes.find(item => item.id === (homeMode ? homeState.node : currentNode));
  document.querySelector('#connection-node').textContent = `${node.name}在线 · 演示`;
  if (homeMode) document.querySelector('#configuration-origin').textContent = `${node.name} · ${homeState.project}`;
}
function openHome() {
  rememberDraft();homeMode = true;
  showView('workspace');syncWorkspaceMode();
  document.querySelector('#prompt').value = homeState.draft;
  document.querySelector('#prompt').dispatchEvent(new Event('input'));
  syncPreference();renderSessionLists();
}
function openHomeProjects() {
  const list = document.querySelector('#welcome-project-list');list.replaceChildren();
  for (const node of demoNodes) for (const project of new Set(node.sessions.map(session => session.project))) {
    const button = document.createElement('button');button.className = 'welcome-project-choice';
    button.dataset.homeNode = node.id;button.dataset.homeProject = project;
    button.setAttribute('aria-pressed',String(node.id === homeState.node && project === homeState.project));
    const icon = document.createElement('i');icon.dataset.icon = 'folder';
    const name = document.createElement('strong');name.textContent = project;
    const origin = document.createElement('span');origin.textContent = node.name;
    const check = document.createElement('i');check.dataset.icon = 'check';
    button.append(icon,name,origin,check);list.append(button);
  }
  mountIcons(list);document.querySelector('#welcome-project-dialog').showModal();
}
function renderPreviewConversation() {
  const messages = previewMessages.get(scopeKey(currentNode,currentSession));
  document.body.dataset.preview = String(Boolean(messages));
  document.querySelector('#fixture-transcript').hidden = Boolean(messages);
  const thread = document.querySelector('#preview-transcript');thread.hidden = !messages;
  thread.replaceChildren();
  if (!messages) return;
  for (const text of messages) {
    const article = document.createElement('article');article.className = 'user-message';
    const bubble = document.createElement('div');bubble.className = 'user-bubble';bubble.textContent = text;
    article.append(bubble);thread.append(article);
  }
  const notice = document.createElement('p');notice.className = 'preview-message-note';
  notice.textContent = '对话预览已创建。这里仅展示输入内容，尚未连接模型。';thread.append(notice);
}
function sendPreviewMessage() {
  const text = document.querySelector('#prompt').value.trim();if (!text) return;
  if (homeMode) {
    const node = demoNodes.find(item => item.id === homeState.node);
    const id = `preview-${++previewSequence}`;
    node.sessions.unshift({id,title:Array.from(text).slice(0,24).join(''),project:homeState.project,recent:-previewSequence,state:'idle'});
    const key = scopeKey(node.id,id);
    sessionPreferences.set(key,{model:homeState.model,thinking:homeState.thinking});
    previewMessages.set(key,[text]);selectSession(node.id,id);homeState.draft = '';
  } else {
    const messages = previewMessages.get(scopeKey(currentNode,currentSession));
    if (!messages) { toast('这是设计预览：消息没有发送，草稿已保留。');return; }
    messages.push(text);renderPreviewConversation();
  }
  document.querySelector('#prompt').value = '';
  document.querySelector('#prompt').dispatchEvent(new Event('input'));
  sessionDrafts.set(scopeKey(currentNode,currentSession),'');
  document.querySelector('#prompt').focus();
  document.querySelector('#chat-panel').scrollTop = document.querySelector('#chat-panel').scrollHeight;
}
function currentPreference() {
  if (homeMode) return homeState;
  const key = scopeKey(currentNode,currentSession);
  if (!sessionPreferences.has(key)) sessionPreferences.set(key,{model:currentNode === 'local' ? 'DeepSeek V4 Pro' : 'GPT-5',thinking:currentNode === 'local' ? '高' : '中'});
  return sessionPreferences.get(key);
}
function syncPreference() {
  const preference = currentPreference();
  document.querySelector('.model-name').textContent = preference.model;
  document.querySelector('.model-button').title = `当前模型：${preference.model}`;
  document.querySelector('.thinking-name').textContent = `思考 · ${preference.thinking}`;
  document.querySelector('.thinking-button').title = `思考等级：${preference.thinking}`;
  document.querySelector('#context-model-name').textContent = preference.model;
  document.querySelector('#context-model-detail').textContent = `${preference.model === 'GPT-5' ? 'openai' : 'deepseek'} · 思考等级：${preference.thinking}`;
  document.querySelector('.context-model .model-symbol').textContent = preference.model === 'GPT-5' ? 'G' : 'D';
  for (const button of document.querySelectorAll('[data-model]')) button.setAttribute('aria-pressed',String(button.dataset.model === preference.model));
  for (const button of document.querySelectorAll('[data-thinking]')) button.setAttribute('aria-pressed',String(button.dataset.thinking === preference.thinking));
}
// Demo labels only; production must use observed runtime/settlement, not infer idle=done.
const demoSessionStates = {
  completed:{label:'已完成',icon:'check'},running:{label:'运行中',icon:'play'},
  approval:{label:'待授权',icon:'shield'},failed:{label:'出错',icon:'info'},
  queued:{label:'排队中',icon:'clock'},idle:{label:'空闲',icon:'clock'},
  stopped:{label:'已停止',icon:'close'},unknown:{label:'状态未知',icon:'info'}
};
function sessionButton(node,session,flat,compactStatus=false) {
  const button = document.createElement('button');
  button.className = 'session' + (flat ? ' flat-session' : '');
  button.dataset.node = node.id; button.dataset.sessionId = session.id;
  button.setAttribute('aria-label',`${session.title}，${node.name}，${session.project}`);
  const selected = !homeMode && currentNode === node.id && currentSession === session.id;
  button.classList.toggle('active',selected);
  if (selected) button.setAttribute('aria-current','true');
  const copy = document.createElement('span'); copy.className = 'session-copy';
  const title = document.createElement('span'); title.className = 'session-name'; title.textContent = session.title; copy.append(title);
  const meta=document.createElement('small');meta.className='session-meta';
  if (flat) {const origin = document.createElement('span');origin.className='session-origin-label';origin.textContent = `${node.name} · ${session.project}`;meta.append(origin);}
  const state=demoSessionStates[session.state] || demoSessionStates.unknown;
  const status=document.createElement('span');status.className='session-state';status.dataset.state=demoSessionStates[session.state]?session.state:'unknown';
  if(compactStatus) {
    status.classList.add('sidebar-state');status.title=state.label;status.setAttribute('aria-hidden','true');
  } else {
    const icon=document.createElement('i');icon.dataset.icon=state.icon;const label=document.createElement('span');label.textContent=state.label;status.append(icon,label);meta.append(status);
  }
  if(flat || !compactStatus)copy.append(meta);
  button.append(copy);if(compactStatus)button.append(status);
  button.title = `${session.title} · ${node.name} · ${session.project} · ${state.label}`;
  button.setAttribute('aria-description',state.label);
  return button;
}
function renderSessionList(container,query = '') {
  container.replaceChildren();
  container.classList.toggle('flat-list',sessionLayout === 'flat');
  let matches = 0;
  const flatItems = [];
  for (const node of demoNodes) {
    const list = node.sessions.filter(session => `${node.name} ${session.project} ${session.title}`.toLowerCase().includes(query.toLowerCase()));
    if (!list.length) continue;
    matches += list.length;
    if (sessionLayout === 'flat') {for (const session of list) flatItems.push({node,session});continue;}
    const group = document.createElement('details'); group.className = 'node-group';group.open = true;group.dataset.nodeGroup = node.id;
    const heading = document.createElement('summary');heading.className = 'node-heading';
    const name = document.createElement('strong');name.textContent = node.name;
    const count = document.createElement('span');count.className = 'node-session-count';count.textContent = String(list.length);
    const icon = document.createElement('i');icon.dataset.icon = node.id === 'local' ? 'monitor' : 'server';
    heading.append(icon,name);
    if (node.id === currentNode) {const current = document.createElement('small');current.textContent = '当前';heading.append(current);}
    heading.append(count);group.append(heading);
    for (const project of [...new Set(list.map(session => session.project))]) {
      const label = document.createElement('div');label.className = 'node-project';label.textContent = project;group.append(label);
      const sessions = document.createElement('div');sessions.className = 'session-list';
      for (const session of list.filter(item => item.project === project)) sessions.append(sessionButton(node,session,false));
      group.append(sessions);
    }
    container.append(group);
  }
  for (const {node,session} of flatItems.sort((a,b) => a.session.recent - b.session.recent)) container.append(sessionButton(node,session,true));
  if (!matches) {const empty = document.createElement('p');empty.className = 'session-empty';empty.textContent = '没有匹配的节点、项目或会话。';container.append(empty);}
  mountIcons(container);
}
// The sidebar keeps the existing recent-sessions + project-tree organization.
// Only its node heading visibility follows the display preference.
function sidebarSession(node,session,recent = false) {
  const row = document.createElement('div');row.className = 'sidebar-session-entry';
  row.append(sessionButton(node,session,recent,true));
  const actions = document.createElement('button');actions.className = 'session-row-action';
  actions.setAttribute('aria-label',`${session.title}的会话操作`);actions.dataset.toast = '设计预览：会话操作沿用现有产品，不执行真实操作';
  const icon=document.createElement('i');icon.dataset.icon='more';actions.append(icon);row.append(actions);return row;
}
function renderSidebarSessions() {
  const container = document.querySelector('#sidebar-sessions');container.replaceChildren();
  container.dataset.displayMode = sessionLayout;
  const node = demoNodes.find(item => item.id === currentNode);
  const recent = document.createElement('section');recent.className='sidebar-recent';recent.setAttribute('aria-label','最近的会话');
  const heading=document.createElement('p');heading.className='sidebar-section-label';heading.textContent='最近的会话';recent.append(heading);
  for(const session of [...node.sessions].sort((a,b)=>a.recent-b.recent).slice(0,2)) recent.append(sidebarSession(node,session,true));
  container.append(recent);
  const divider=document.createElement('div');divider.className='sidebar-session-divider';divider.textContent='当前项目与会话';container.append(divider);
  const catalog=document.createElement('div');catalog.className='sidebar-catalog';container.append(catalog);
  for(const node of demoNodes) {
    const group=document.createElement(sessionLayout==='grouped'?'details':'div');group.className=sessionLayout==='grouped'?'node-group':'node-project-list';group.dataset.nodeGroup=node.id;
    if(sessionLayout==='grouped') {
      group.open=node.id===currentNode;
      const summary=document.createElement('summary');summary.className='node-heading';
      const icon=document.createElement('i');icon.dataset.icon=node.id==='local'?'monitor':'server';
      const title=document.createElement('strong');title.textContent=node.name;
      const count=document.createElement('span');count.className='node-session-count';count.textContent=String(node.sessions.length);
      summary.append(icon,title,count);group.append(summary);
    }
    for(const project of [...new Set(node.sessions.map(session=>session.project))]) {
      const entry=document.createElement('details');entry.className='project-outline';entry.open=true;entry.dataset.project=project;
      const summary=document.createElement('summary');summary.className='project-heading';
      const folder=document.createElement('button');folder.className='project-folder-button';folder.dataset.newSessionNode=node.id;folder.dataset.newSessionProject=project;
      folder.setAttribute('aria-label',`在${node.name}的${project}中新建会话`);folder.title=`在 ${project} 中新建会话`;
      const folderIcon=document.createElement('i');folderIcon.dataset.icon='folder';folder.append(folderIcon);
      const title=document.createElement('span');title.className='project-name';title.textContent=project;
      summary.append(folder,title);
      if(sessionLayout==='flat') {const origin=document.createElement('small');origin.className='project-node-origin';origin.textContent=node.name;summary.append(origin);}
      for(const [iconName,label] of [['more','项目操作']]) {
        const action=document.createElement('button');action.className='project-row-action';action.setAttribute('aria-label',`${project} ${label}`);action.dataset.toast=`设计预览：${label}沿用现有产品`;
        const icon=document.createElement('i');icon.dataset.icon=iconName;action.append(icon);summary.append(action);
      }
      entry.append(summary);
      const list=document.createElement('div');list.className='session-list';
      for(const session of node.sessions.filter(item=>item.project===project)) list.append(sidebarSession(node,session));
      entry.append(list);group.append(entry);
    }
    catalog.append(group);
  }
  mountIcons(container);
}
function renderSessionLists() {
  renderSidebarSessions();
  renderSessionList(document.querySelector('#switcher-sessions'),document.querySelector('#session-search').value.trim());
  document.querySelector('#session-display-setting').value = sessionLayout;
  document.querySelector('#session-display-description').textContent = sessionLayout === 'grouped'
    ? '节点标题下仍是原有的项目与会话；手机切换器使用相同偏好。'
    : '侧栏保留项目树并标注节点；手机会话列表平铺，每条保留节点和项目归属。';
}
function selectSession(nodeId,sessionId) {
  const node = demoNodes.find(item => item.id === nodeId), session = node?.sessions.find(item => item.id === sessionId);
  if (!session) return;
  rememberDraft();homeMode = false;
  const dialog = document.querySelector('#session-dialog'); if (dialog.open) dialog.close();
  currentNode = node.id;currentSession = session.id;
  document.querySelector('#session-title').textContent = session.title;
  document.querySelector('.session-trigger').title = `${session.title} · ${node.name} · ${session.project}`;
  for (const selector of ['#session-origin','#configuration-origin','.context-session-scope']) document.querySelector(selector).textContent = `${node.name} · ${session.project}`;
  document.querySelector('#connection-node').textContent = `${node.name}在线 · 演示`;
  document.querySelector('.demo-history-note').hidden = node.id === 'local' && session.id === 'design';
  document.querySelector('#prompt').value = sessionDrafts.get(scopeKey(node.id,session.id)) || '';
  document.querySelector('#prompt').dispatchEvent(new Event('input'));
  renderSessionLists();syncPreference();showView('workspace');showTab('chat');
  syncWorkspaceMode();renderPreviewConversation();
  document.querySelector('.session-trigger').focus();
  // The inline fixture notice communicates the boundary without covering controls.
}
function openSessionSwitcher(search = false) {
  const dialog = document.querySelector('#session-dialog');
  document.querySelector('#session-search').value = '';renderSessionLists();
  if (!dialog.open) dialog.showModal();
  document.querySelector('.session-trigger').setAttribute('aria-expanded','true');
  if (search) document.querySelector('#session-search').focus();
  else document.querySelector('#switcher-sessions .session.active')?.focus({preventScroll:true});
}
const resources = {
  packages:[
    {name:'开发基础',id:'developer',description:'读写代码、执行命令与管理开发流程。为日常项目提供一组恰好的基础能力。',icon:'code',tone:'blue',tags:['4 个工具','代码开发'],version:'内置 Package'},
    {name:'Rust 开发环境',id:'rust-development',description:'将 Rust Skills 与开发扩展整合到一起，覆盖编译诊断、代码导航和开发工作流。',icon:'terminal',tone:'mint',tags:['27 个 Skills','18 个扩展'],version:'v1.0.0'},
    {name:'工作基础',id:'base',description:'工作区约定、基础提示词与通用 Skills。让不同任务拥有一致、可追溯的执行基础。',icon:'layers',tone:'purple',tags:['基础提示词','工作区规范'],version:'内置 Package'}
  ],
  skills:[
    {name:'Rust 开发',id:'rust-router',description:'按问题类型选择对应知识与工具，覆盖 Cargo、编译诊断和常见 Rust 开发任务。',icon:'code',tone:'blue',tags:['Rust','开发'],version:'Skill · 示例'},
    {name:'Dioxus 开发',id:'dioxus-knowledge-patch',description:'面向 Rust Web、桌面和移动应用的 Dioxus 组件、事件与状态管理知识。',icon:'layout',tone:'mint',tags:['UI','Dioxus'],version:'Skill · 示例'},
    {name:'开发路由',id:'rust-development',description:'开发 Package 的本地入口，将具体工作交给对应 Skill，不自动扩大执行权限。',icon:'branch',tone:'purple',tags:['Package 本地','路由'],version:'Skill · 示例'}
  ],
  extensions:[
    {name:'代码导航',id:'codegraph',description:'搜索符号、查看调用关系并分析修改影响范围，为代码理解提供结构化辅助。',icon:'branch',tone:'blue',tags:['符号搜索','调用关系'],permission:'只读代码索引',version:'Extension · 示例'},
    {name:'开发诊断',id:'lens',description:'执行受限的本地诊断，检查格式、编译状态与代码差异，不自动修改源文件。',icon:'search',tone:'mint',tags:['检查','只读诊断'],permission:'执行受限诊断',version:'Extension · 示例'},
    {name:'任务清单',id:'todo',description:'把当前会话的待办拆成具体任务，跟踪进度并保留清晰的完成条件。',icon:'skills',tone:'purple',tags:['会话任务','进度'],permission:'会话任务状态',version:'Extension · 示例'}
  ]
};
let resourceKind = 'packages', resourceSource = '当前节点';
function renderResources() {
  const query = document.querySelector('#resource-search').value.trim().toLowerCase();
  const list = resources[resourceKind].filter(item => `${item.name} ${item.id} ${item.description}`.toLowerCase().includes(query));
  const grid = document.querySelector('#resource-grid');
  grid.replaceChildren();
  for (const item of list) {
    const card = document.createElement('article'); card.className = `resource-package resource-card-${resourceKind}`;
    const kindLabel = {packages:'能力集合',skills:'知识与工作流',extensions:'运行扩展'}[resourceKind];
    const kindDetails = resourceKind === 'packages'
      ? `<span><b>组成</b>${item.tags.join(' · ')}</span><span><b>作用范围</b>当前节点</span>`
      : resourceKind === 'skills'
        ? `<span><b>适用领域</b>${item.tags.join(' · ')}</span><span><b>触发方式</b>按任务路由</span>`
        : `<span class="extension-status"><i class="status-dot online"></i>可用扩展</span><span><b>权限边界</b>${item.permission}</span>`;
    // All markup below is curated local fixture content, not model/user HTML.
    card.innerHTML = `<span class="resource-icon ${item.tone}"><i data-icon="${item.icon}"></i></span><div class="resource-heading"><h2>${item.name}<span class="resource-kind-label">${kindLabel}</span></h2><p class="package-id">${item.id}</p></div><p class="package-description">${item.description}</p><div class="resource-meta">${kindDetails}</div><div class="package-footer"><span>${resourceSource === '当前节点' ? item.version : resourceSource + ' · 示例'}</span><button data-action="resource-detail" data-resource="${item.name}">查看详情<i data-icon="arrow-right"></i></button></div>`;
    grid.append(card);
  }
  document.querySelector('.resource-empty').hidden = list.length !== 0;
  document.querySelector('#resource-source-note').textContent = {
    '当前节点':'节点已安装资源与本地设计相互独立。查看详情后，再决定是否用于当前会话。',
    '发现资源':'发现目录不等于已安装。正式安装前需要核对来源、清单和权限；原型不会执行安装。',
    '本地设计':'本地设计是未执行的资源声明，不等于当前节点已经安装或信任的能力。'
  }[resourceSource];
  mountIcons(grid);
}
let fullscreenPending=false;
function syncFullscreen() {
  const button=document.querySelector('[data-action="fullscreen"]');
  const active=!!document.fullscreenElement;
  const supported=!!document.fullscreenEnabled && typeof document.documentElement.requestFullscreen==='function';
  const label=active?'退出全屏':supported?'进入全屏':'当前浏览器不支持全屏';
  button.setAttribute('aria-label',label);button.title=label;button.setAttribute('aria-pressed',String(active));button.setAttribute('aria-busy',String(fullscreenPending));button.disabled=fullscreenPending || (!active && !supported);
  button.replaceChildren();const icon=document.createElement('i');icon.dataset.icon=active?'fullscreen-exit':'fullscreen';button.append(icon);mountIcons(button);
}
async function toggleFullscreen() {
  if(fullscreenPending)return;
  const button=document.querySelector('[data-action="fullscreen"]');const restoreFocus=document.activeElement===button;
  fullscreenPending=true;syncFullscreen();
  try {
    if(document.fullscreenElement)await document.exitFullscreen();
    else await document.documentElement.requestFullscreen();
  } catch {toast('全屏切换未成功；当前显示状态保持不变。');}
  finally {fullscreenPending=false;syncFullscreen();if(restoreFocus)button.focus({preventScroll:true});}
}
document.addEventListener('fullscreenchange',syncFullscreen);
function toggleToolDetails() {
  const records=document.querySelector('#tool-records');const detailed=records.dataset.rendering!=='detailed';
  records.dataset.rendering=detailed?'detailed':'simple';
  const button=document.querySelector('[data-action="tool-details"]');button.setAttribute('aria-pressed',String(detailed));button.querySelector('span').textContent=detailed?'简洁显示':'详细命令';
  for(const call of records.querySelectorAll('.command-call'))call.open=detailed;
}
document.addEventListener('click', event => {
  const button = event.target.closest('button');
  if (!button) return;
  if (button.dataset.toast) {
    if (button.closest('.project-heading')) event.preventDefault();
    toast(button.dataset.toast);
  }
  if (button.dataset.settingsPanel) {
    const panel = button.dataset.settingsPanel;
    document.querySelector('#appearance-panel').hidden = panel !== 'appearance';
    document.querySelector('#session-display-panel').hidden = panel !== 'session-display';
    for (const item of document.querySelectorAll('[data-settings-panel]')) {
      const active = item === button;item.classList.toggle('active',active);item.setAttribute('aria-pressed',String(active));
    }
  }
  if (button.dataset.view) showView(button.dataset.view);
  if (button.dataset.action === 'home') openHome();
  if (button.dataset.action === 'welcome-project') openHomeProjects();
  if (button.dataset.homeProject) {
    homeState.node = button.dataset.homeNode;homeState.project = button.dataset.homeProject;
    document.querySelector('#welcome-project-name').textContent = homeState.project;
    const node = demoNodes.find(item => item.id === homeState.node);
    document.querySelector('#welcome-project-node').textContent = node.name;
    document.querySelector('#configuration-origin').textContent = `${node.name} · ${homeState.project}`;
    document.querySelector('#welcome-project-dialog').close();syncWorkspaceMode();
  }
  if (button.dataset.prompt) {
    const input = document.querySelector('#prompt');
    input.value = input.value.trim() ? `${input.value}\n${button.dataset.prompt}` : button.dataset.prompt;
    input.dispatchEvent(new Event('input'));input.focus();input.setSelectionRange(input.value.length,input.value.length);
  }
  if (button.dataset.tab) {
    if (button.closest('#project-files-panel')) setProjectFiles(false);
    showTab(button.dataset.tab);
  }
  if (button.dataset.newSessionProject) {
    event.preventDefault();
    const node=demoNodes.find(item=>item.id===button.dataset.newSessionNode);
    document.querySelector('.new-session-project').textContent=`${node.name} · ${button.dataset.newSessionProject}`;
    document.querySelector('#new-session-dialog').showModal();
  }
  if (button.dataset.sessionId) selectSession(button.dataset.node,button.dataset.sessionId);
  if (button.dataset.action === 'sessions') openSessionSwitcher();
  if (button.dataset.action === 'fullscreen') void toggleFullscreen();
  if (button.dataset.action === 'tool-details') toggleToolDetails();
  if (button.hasAttribute('data-close')) button.closest('dialog').close();
  if (button.dataset.action === 'menu') {
    shell.classList.toggle(matchMedia('(max-width:767px)').matches ? 'nav-open' : 'nav-collapsed');
    syncNavigation();
    if (matchMedia('(max-width:767px)').matches && shell.classList.contains('nav-open')) document.querySelector('.sidebar .brand').focus();
  }
  if (button.classList.contains('scrim')) { if (shell.classList.contains('project-files-open')) setProjectFiles(false); shell.classList.remove('nav-open'); syncNavigation(); document.querySelector('.mobile-menu').focus(); }
  if (button.dataset.action === 'project-files') toggleProjectFiles();
  if (button.dataset.action === 'project-file-diff') openProjectFileDiff(button.dataset.file);
  if (button.dataset.action === 'project-file-diff-back') showProjectFileList();
  if (button.dataset.fileTab) renderProjectFileDiff(button.dataset.fileTab);
  if (button.dataset.action === 'configuration') document.querySelector('#configuration-dialog').showModal();
  if (button.dataset.action === 'context') {
    const dialog = document.querySelector('#context-dialog');
    if (dialog.open) dialog.close(); else dialog.showModal();
    document.querySelector('.topbar [data-action="context"]').setAttribute('aria-expanded',String(dialog.open));
  }
  if (button.dataset.providerAction) {
    const action = button.dataset.providerAction;
    if (action === 'add' || action === 'edit') document.querySelector('#provider-definition-dialog').showModal();
    if (action === 'key') {
      const row = button.closest('.provider-row');
      document.querySelector('#provider-key-name').textContent = row?.querySelector('.provider-identity h3')?.childNodes[0]?.textContent?.trim() || '供应商';
      document.querySelector('#provider-key-dialog').showModal();
    }
    if (action === 'models') document.querySelector('#provider-models-dialog').showModal();
  }
  if (button.dataset.action === 'resource-detail') {
    const item = resources[resourceKind].find(resource => resource.name === button.dataset.resource);
    if (item) {
      document.querySelector('#detail-resource-name').textContent = item.name;
      document.querySelector('#detail-resource-description').textContent = item.description;
      document.querySelector('#detail-resource-source').textContent = resourceSource;
      document.querySelector('#detail-resource-target').textContent = resourceSource === '本地设计' ? '本机设计目录' : '本机节点 · rpi-work';
      document.querySelector('#detail-resource-version').textContent = resourceSource === '当前节点' ? item.version : `${resourceSource} · 待核对清单`;
      const action = document.querySelector('#detail-resource-action');
      action.disabled = resourceSource === '当前节点';
      action.textContent = resourceSource === '当前节点' ? '已安装' : resourceSource === '本地设计' ? '导入到本地设计' : '安装到当前节点';
      action.dataset.resourceName = item.name;
      action.dataset.resourceSource = resourceSource;
      document.querySelector('#detail-resource-boundary').textContent = resourceSource === '当前节点'
        ? '节点已安装；查看详情不会自动把资源加入当前会话。'
        : resourceSource === '本地设计'
          ? '导入只保存资源声明，不代表节点已安装、已信任或拥有执行权限。'
          : '安装前应核对来源、版本、清单和权限；安装不会自动加入当前会话。';
    }
    document.querySelector('#detail-dialog').showModal();
  }
  if (button.hasAttribute('data-resource-action')) {
    button.textContent = '原型不执行安装';button.disabled = true;
    toast('这是安装确认预览：不会连接节点、写入目录或改变当前会话。');
  }
  if (button.dataset.action === 'model') document.querySelector('#model-dialog').showModal();
  if (button.dataset.action === 'thinking') document.querySelector('#thinking-dialog').showModal();
  if (button.dataset.thinking) {
    currentPreference().thinking = button.dataset.thinking;syncPreference();button.closest('dialog').close();
  }
  if (button.dataset.model) {
    currentPreference().model = button.dataset.model;syncPreference();
    button.closest('dialog').close();
  }
  if (button.dataset.kind) { resourceKind = button.dataset.kind; document.querySelectorAll('[data-kind]').forEach(b => b.classList.toggle('active',b === button)); renderResources(); }
  if (button.dataset.source) { resourceSource = button.dataset.source; document.querySelectorAll('[data-source]').forEach(b => b.classList.toggle('active',b === button)); renderResources(); }
  if (button.dataset.settings) {
    document.querySelectorAll('[data-settings]').forEach(b => b.classList.toggle('active',b === button));
    showSettingsCategory(button.dataset.settings);
    if (button.dataset.settings !== '界面与显示' && button.dataset.settings !== '工作协同' && button.dataset.settings !== '供应商') toast(`${button.dataset.settings}：当前原型沿用现有功能。`);
  }
  if (button.dataset.theme) {
    document.querySelectorAll('[data-theme]').forEach(b => b.classList.toggle('selected',b === button));
    toast(`已选 ${button.dataset.theme}；仅展示选择状态，不写入真实偏好。`);
  }
  if (button.dataset.mode) { document.querySelectorAll('[data-mode]').forEach(b => b.classList.toggle('active',b === button)); toast('明暗模式入口示意。本轮视觉稿展示浅色方案。'); }
  if (button.classList.contains('send-button')) sendPreviewMessage();
  if (button.dataset.action === 'search') openSearch();
});
document.querySelector('#session-display-setting').addEventListener('change',event => {
  sessionLayout = event.target.value === 'flat' ? 'flat' : 'grouped';renderSessionLists();
});
function showSettingsCategory(category) {
  const collaboration = category === '工作协同';
  const providers = category === '供应商';
  const intro = document.querySelector('.settings-intro');
  const tabs = document.querySelector('.settings-tabs');
  const appearance = document.querySelector('#appearance-panel');
  const sessionDisplay = document.querySelector('#session-display-panel');
  const collaborationPanel = document.querySelector('#collaboration-panel');
  const providerPanel = document.querySelector('#providers-panel');
  for (const element of [intro,tabs,appearance,sessionDisplay]) element.hidden = collaboration || providers || (element === appearance ? !document.querySelector('[data-settings-panel="appearance"]').classList.contains('active') : element === sessionDisplay ? !document.querySelector('[data-settings-panel="session-display"]').classList.contains('active') : false);
  collaborationPanel.hidden = !collaboration;
  providerPanel.hidden = !providers;
  if (collaboration) {
    document.querySelector('.settings-intro h2').textContent = '工作协同';
    document.querySelector('.settings-intro p').textContent = '连接节点、设备与可信协助，权限边界保持清楚。';
  } else if (providers) {
    document.querySelector('.settings-intro h2').textContent = '模型供应商';
    document.querySelector('.settings-intro p').textContent = '配置保存到当前执行节点，密钥、模型和会话选择分开管理。';
  } else {
    document.querySelector('.settings-intro h2').textContent = '界面与显示';
    document.querySelector('.settings-intro p').textContent = '打造一个愿意每天打开的工作台。';
  }
}
function openSearch() { openSessionSwitcher(true); }
document.querySelector('#session-search').addEventListener('input',event => renderSessionList(document.querySelector('#switcher-sessions'),event.target.value.trim()));
document.querySelector('#session-dialog').addEventListener('close',() => document.querySelector('.session-trigger').setAttribute('aria-expanded','false'));
document.querySelector('.brand').addEventListener('click', event => {event.preventDefault();showView('workspace');});
document.querySelector('#resource-search').addEventListener('input',renderResources);
document.querySelector('#provider-search')?.addEventListener('input', event => {
  const query = event.target.value.trim().toLowerCase();
  for (const row of document.querySelectorAll('.provider-row')) row.hidden = !row.textContent.toLowerCase().includes(query);
});
document.querySelector('#prompt').addEventListener('input', event => { document.querySelector('.send-button').disabled = !event.target.value.trim(); });
document.querySelector('#prompt').addEventListener('keydown', event => { if (event.key === 'Enter' && !event.shiftKey && !event.isComposing) { event.preventDefault(); if (event.target.value.trim()) document.querySelector('.send-button').click(); } });
document.addEventListener('keydown', event => {
  if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'j') {event.preventDefault();openHome();document.querySelector('#prompt').focus();}
  if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'k') {event.preventDefault();openSearch();}
  if(event.key==='Escape' && document.fullscreenElement && !document.querySelector('dialog[open]') && !shell.classList.contains('nav-open')) {
    void document.exitFullscreen().catch(()=>toast('退出全屏未成功，请使用浏览器的退出全屏操作。'));
  }
  if (event.key === 'Escape' && shell.classList.contains('nav-open')) {
    shell.classList.remove('nav-open'); syncNavigation(); document.querySelector('.mobile-menu').focus();
  }
  if (event.key === 'Tab' && !document.querySelector('dialog[open]') && matchMedia('(max-width:767px)').matches && shell.classList.contains('nav-open')) {
    const items = [...document.querySelectorAll('.sidebar a[href],.sidebar button:not(:disabled)')];
    const first = items[0], last = items[items.length - 1];
    if (event.shiftKey && document.activeElement === first) {event.preventDefault();last.focus();}
    else if (!event.shiftKey && document.activeElement === last) {event.preventDefault();first.focus();}
  }
});
matchMedia('(max-width:767px)').addEventListener('change',syncNavigation);
document.querySelector('#context-dialog').addEventListener('close', () => {
  document.querySelector('.topbar [data-action="context"]').setAttribute('aria-expanded','false');
});
function routeHash() {
  const view = location.hash.slice(1);
  if (view === 'workspace' && homeMode) {selectSession(currentNode,currentSession);return;}
  showView(['home','workspace','resources','settings'].includes(view) ? view : 'home');
}
window.addEventListener('hashchange',routeHash);
mountIcons();syncFullscreen();renderResources();renderSessionLists();syncPreference();routeHash();
