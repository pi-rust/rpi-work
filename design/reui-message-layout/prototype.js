const $ = (selector, root = document) => root.querySelector(selector);
const shell = $('.shell');
const transcript = $('#transcript');
const conversation = $('.conversation');
const draft = $('#draft');
const model = $('#model');
const effort = $('#effort');
const form = $('.composer');
const searchDialog = $('#search-dialog');
const settingsDialog = $('#settings-dialog');
const searchInput = $('#search-input');
const threadDialog = $('#thread-dialog');
const archiveDialog = $('#archive-dialog');
const commentDialog = $('#comment-dialog');
const mobile = matchMedia('(max-width: 700px)');
const narrow = matchMedia('(max-width: 900px)');
const compact = matchMedia('(max-width: 1199px)');
const projects = [
  { name: 'rpi-work', branch: 'feat/next-iteration' },
  { name: 'pi-rust', branch: 'main' },
  ...workbench.projects,
];

const files = [
  {
    name: 'transcript.tsx', path: 'src/components/transcript.tsx', start: 83,
    lines: [
      [' ', 'function Transcript({ turns }) {'],
      ['-', '  const rows = flatten(turns);'],
      ['+', '  const rows = groupTurns(turns);'],
      [' ', '  return ('],
      ['-', '    <MessageList rows={rows} />'],
      ['+', '    <ReadingColumn rows={rows} />'],
      [' ', '  );'],
      [' ', '}'],
    ],
  },
  {
    name: 'composer.tsx', path: 'src/components/composer.tsx', start: 46,
    lines: [
      [' ', 'function Composer({ busy }) {'],
      ['-', '  return <SendButton />;'],
      ['+', '  return busy ? <StopButton />'],
      ['+', '    : <SendButton />;'],
      [' ', '}'],
    ],
  },
  {
    name: 'workspace.css', path: 'src/workspace.css', start: 12,
    lines: [
      [' ', '.reading-column {'],
      ['-', '  max-width: 100%;'],
      ['+', '  width: min(100%, 728px);'],
      ['+', '  margin-inline: auto;'],
      [' ', '}'],
    ],
  },
];

const sessions = [
  {
    id: 'layout', project: 'rpi-work', title: '重整消息区布局', age: '刚刚',
    prompt: '把工作台整理成左侧会话、中间对话、右侧审阅的布局。长消息要好读，输入框固定在底部。',
    commentary: '我先梳理消息列表和输入区的容器，再检查打开审阅面板后正文还能否保持合适的宽度。',
    activity: '检查了 3 个文件', duration: '12 秒',
    output: '读取 src/components/transcript.tsx\n读取 src/components/composer.tsx\n检查 src/workspace.css 的分栏和滚动边界',
    heading: '消息区布局已调整',
    summary: '对话和输入框共用同一阅读宽度。打开右侧审阅时，正文会随可用空间收窄。',
    bullets: ['按会话轮次组织消息，工具输出默认折叠。', '输入框保留在底部，正文和代码差异各自滚动。'],
    code: 'function ReadingColumn({ children }) {\n  return (\n    <div className="reading-column">\n      {children}\n    </div>\n  );\n}',
    followup: {
      prompt: '继续检查执行中、工具失败和长回答，确保打开右侧工作区后也能正常阅读。',
      commentary: '我会分别检查增量输出、失败恢复和长内容滚动，再核对正文与输入区的对齐。',
      output: '执行中 · 新内容追加到当前轮，停止后保留已输出内容\n工具失败 · 显示退出码和诊断输出，可重试\n长回答 · 代码与表格局部滚动，正文保持阅读宽度',
      heading: '三种状态已补齐',
      summary: '消息区、工具过程和右侧工作区使用统一的布局规则。阅读旧消息时保留视口，回到底部后继续跟随。',
      bullets: ['执行过程可以折叠，正文保留完整上下文。', '审批、队列和失败状态都跟随所属会话。', '浏览器、源码和差异可在右侧直接切换。'],
    },
    files,
  },
  {
    id: 'connection', project: 'rpi-work', title: '修复断线后的提示', age: '2 小时',
    prompt: '连接断开时，用户应该能从当前会话直接重试连接。',
    heading: '断线恢复流程',
    summary: '恢复入口留在当前会话。重连期间保留草稿，成功后再更新节点状态。',
    files: [],
  },
  {
    id: 'release', pinned: true, project: 'rpi-work', title: '整理发布检查项', age: '昨天',
    prompt: '列一下发布前需要检查的项目。',
    heading: '发布前检查',
    bullets: ['运行工作区测试和前端构建。', '检查各平台安装包、校验文件及更新元数据。'],
    files: [],
  },
  {
    id: 'runtime', project: 'pi-rust', title: '梳理运行时入口', age: '昨天',
    prompt: '找出运行时启动和停止的入口。',
    heading: '运行时入口',
    summary: '启动流程负责解析配置和初始化会话，停止流程先取消运行，再释放会话资源。',
    files: [],
  },
];

const viewState = new Map();
const collapsedProjects = new Set();
const collapsedSections = new Set();
const pinnedProjects = new Set();
const navigationKey = 'rpi-work-layout-navigation-v1';
try {
  const saved = JSON.parse(localStorage.getItem(navigationKey));
  const pins = Array.isArray(saved?.pins) ? saved.pins : (localStorage.getItem('rpi-work-layout-sessions-v1') ? [] : ['rpi-work']);
  for (const name of pins) if (projects.some(project => project.name === name)) pinnedProjects.add(name);
  if (Array.isArray(saved?.collapsed)) {
    for (const id of saved.collapsed) if (id === 'projects' || id === 'pinned-threads') collapsedSections.add(id);
  }
  if (!saved) saveNavigation();
} catch {
  pinnedProjects.add('rpi-work');
}

function saveNavigation() {
  try {
    localStorage.setItem(navigationKey, JSON.stringify({pins: [...pinnedProjects], collapsed: [...collapsedSections]}));
  } catch {}
}

const storageKey = 'rpi-work-layout-sessions-v1';
let session = sessions[0];
let welcomeProject = session.project;
let selectedFile = 0;
let navigating = !compact.matches;
let reviewing = !narrow.matches;
let commentTarget;

function icon(name, className = '') {
  return `<svg class="icon ${className}" aria-hidden="true" focusable="false"><use href="#icon-${name}"></use></svg>`;
}

function mountIcons(root = document) {
  for (const placeholder of root.querySelectorAll('[data-icon]')) {
    placeholder.outerHTML = icon(placeholder.dataset.icon);
  }
}

function element(tag, className, value) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (value !== undefined) node.textContent = value;
  return node;
}

function stateFor(id) {
  if (!viewState.has(id)) {
    viewState.set(id, {
      draft: '', model: workbench.defaultModel, effort: workbench.defaultEffort, mode: 'code', attachments: [], comments: [],
      turns: [], queue: [], staged: [], finding: false, timer: 0, active: null, sessionApproved: false,
      busyMode: workbench.followUpMode ?? 'queue', environment: 'local', base: '',
    });
  }
  return viewState.get(id);
}

function state() {
  return stateFor(session?.id);
}

function saveHistory() {
  try {
    const history = sessions.map(thread => {
      const { id, title, age, project, archived, environment, base, fixture, pinned, omitFollowup } = thread;
      const { timer, active, sessionApproved, ...data } = stateFor(id);
      return { id, title, age, project, archived, environment, base, fixture, pinned, omitFollowup, data };
    });
    localStorage.setItem(storageKey, JSON.stringify(history));
  } catch {
    // Browsers can disable or limit storage; the preview still works in memory.
  }
}

function restoreHistory() {
  try {
    const saved = JSON.parse(localStorage.getItem(storageKey));
    if (!Array.isArray(saved)) return;
    const fixtures = new Map(sessions.map(thread => [thread.id, thread]));
    const valid = saved.filter(thread => thread
      && typeof thread.id === 'string' && typeof thread.title === 'string'
      && projects.some(project => project.name === thread.project));
    const restored = valid.map(({ data, ...thread }) => {
      const fixture = fixtures.get(thread.fixture ?? thread.id);
      if (data && Array.isArray(data.turns) && Array.isArray(data.queue)) {
        for (const turn of data.turns) {
          if (['running', 'approval'].includes(turn.phase)) turn.phase = 'interrupted';
        }
        Object.assign(stateFor(thread.id), data, { active: null, timer: 0, sessionApproved: false });
      }
      const restoredThread = { files: [], ...fixture, ...thread };
      if (thread.omitFollowup) delete restoredThread.followup;
      return restoredThread;
    });
    if (restored.length) sessions.splice(0, sessions.length, ...restored);
  } catch {
    // A malformed snapshot should not prevent the built-in example from opening.
    viewState.clear();
  }
}

function actionButton(label, action, className = 'action-button') {
  const button = element('button', className, label);
  button.type = 'button';
  button.dataset.action = action;
  return button;
}

function syncPanels() {
  shell.dataset.navigation = navigating ? 'open' : 'closed';
  shell.dataset.inspector = reviewing ? 'open' : 'closed';
  $('[data-action="navigation"]').setAttribute('aria-expanded', navigating);
  $('[data-action="inspector"]').setAttribute('aria-expanded', reviewing);
  const navDrawer = navigating && mobile.matches;
  const reviewDrawer = reviewing && narrow.matches;
  $('.scrim').hidden = !(navDrawer || reviewDrawer);
  $('.workspace').inert = navDrawer || reviewDrawer;
  $('.workspace-header').inert = navDrawer || reviewDrawer;
  $('.sidebar').inert = reviewDrawer;
  $('.inspector').inert = navDrawer;
  $('.activity-rail').inert = navDrawer || reviewDrawer;
}

function setPanel(panel, open, moveFocus = true) {
  if (panel === 'navigation') {
    navigating = open;
    if (open && mobile.matches) reviewing = false;
  } else {
    reviewing = open;
    if (open && mobile.matches) navigating = false;
  }
  syncPanels();
  if (!moveFocus) return;
  if (panel === 'navigation' && open && mobile.matches) {
    $('.sidebar-close').focus();
    return;
  }
  const target = panel === 'navigation'
    ? 'navigation'
    : (open && narrow.matches ? 'close-inspector' : 'inspector');
  $(`[data-action="${target}"]`).focus();
}

function threadButton(thread) {
  const button = element('button', 'thread-button');
  button.dataset.session = thread.id;
  if (thread === session) button.setAttribute('aria-current', 'true');
  button.append(element('span', 'thread-title', thread.title), element('span', 'thread-age', threadStatus(thread)));
  return button;
}

function projectRow(project, chats) {
  const group = element('div', 'sidebar-project');
  group.dataset.projectGroup = project.name;
  const details = element('details', 'project');
  const summary = element('summary');
  summary.innerHTML = `<span class="project-toggle">${icon('chevron-right')}</span><span class="project-folder">${icon('folder')}</span>`;
  summary.append(element('span', 'project-name', project.name));
  details.open = !collapsedProjects.has(project.name);
  details.addEventListener('toggle', () => {
    if (!details.isConnected) return;
    if (details.open) collapsedProjects.delete(project.name);
    else collapsedProjects.add(project.name);
  });
  const list = element('div', 'project-threads');
  list.append(...chats.map(threadButton));
  details.append(summary, list);

  const tools = element('div', 'project-tools');
  const compose = actionButton('', 'project-chat', 'icon-button project-compose');
  const menu = actionButton('', 'project-menu', 'icon-button');
  for (const [button, label, glyph] of [[menu, project.name + ' 项目操作', 'ellipsis'], [compose, '在 ' + project.name + ' 中新建对话', 'compose']]) {
    button.dataset.project = project.name;
    button.title = label;
    button.setAttribute('aria-label', label);
    button.innerHTML = icon(glyph);
    tools.append(button);
  }
  menu.setAttribute('aria-haspopup', 'menu');
  menu.setAttribute('aria-expanded', 'false');
  menu.setAttribute('aria-controls', 'project-menu');
  group.append(details, tools);
  return group;
}

function syncSection(id) {
  const open = !collapsedSections.has(id);
  document.getElementById(id).hidden = !open;
  $(`[data-action="toggle-section"][aria-controls="${id}"]`).setAttribute('aria-expanded', String(open));
}

function renderNavigation() {
  const menu = $('#project-menu');
  if (menu.matches(':popover-open')) menu.hidePopover();
  const filter = $('#chat-filter-mode').value;
  const visible = sessions.filter(thread => {
    const phase = stateFor(thread.id).active?.phase;
    return !thread.archived && (filter === 'all' || (filter === 'running' ? Boolean(phase) : phase === 'approval'));
  });
  const pinned = visible.filter(thread => thread.pinned && !pinnedProjects.has(thread.project)).map(threadButton);
  const recent = [];
  for (const project of projects) {
    const inPinned = pinnedProjects.has(project.name);
    const chats = visible.filter(thread => thread.project === project.name && (inPinned || !thread.pinned));
    if (filter === 'all' || chats.length) (inPinned ? pinned : recent).push(projectRow(project, chats));
  }
  $('#pinned-section').hidden = !pinned.length;
  if (filter !== 'all' && !visible.length) {
    recent.push(element('p', 'filter-empty', filter === 'running' ? '没有正在执行的会话' : '没有等待审批的会话'));
  }
  $('#pinned-threads').replaceChildren(...pinned);
  $('#projects').replaceChildren(...recent);
  syncSection('pinned-threads');
  syncSection('projects');
}

function projectMenuTrigger(name) {
  return $(`[data-action="project-menu"][data-project="${CSS.escape(name)}"]`);
}

function openProjectMenu(button) {
  const menu = $('#project-menu');
  const name = button.dataset.project;
  menu.dataset.project = name;
  menu.querySelector('[data-action="project-chat"]').dataset.project = name;
  menu.querySelector('[data-action="pin-project"] span').textContent = pinnedProjects.has(name) ? '取消置顶项目' : '置顶项目';
  menu.showPopover({ source: button });
  const anchor = button.getBoundingClientRect();
  const box = menu.getBoundingClientRect();
  menu.style.left = Math.max(8, Math.min(anchor.left, innerWidth - box.width - 8)) + 'px';
  menu.style.top = (anchor.bottom + box.height + 4 <= innerHeight ? anchor.bottom + 4 : Math.max(8, anchor.top - box.height - 4)) + 'px';
  button.setAttribute('aria-expanded', 'true');
  menu.querySelector('button').focus();
}

const projectMenu = $('#project-menu');
projectMenu.addEventListener('beforetoggle', event => {
  if (event.newState === 'closed') projectMenuTrigger(projectMenu.dataset.project)?.setAttribute('aria-expanded', 'false');
});
projectMenu.addEventListener('keydown', event => {
  if (event.key === 'Escape') {
    event.preventDefault();
    projectMenu.hidePopover();
    projectMenuTrigger(projectMenu.dataset.project)?.focus();
  } else if (['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) {
    event.preventDefault();
    const items = [...projectMenu.querySelectorAll('button')];
    const index = items.indexOf(document.activeElement);
    const next = event.key === 'Home' ? 0 : event.key === 'End' ? items.length - 1
      : (index + items.length + (event.key === 'ArrowUp' ? -1 : 1)) % items.length;
    items[next].focus();
  }
});

function threadStatus(thread) {
  const phase = stateFor(thread.id).active?.phase;
  return phase === 'approval' ? '待审批' : phase ? '执行中' : thread.pinned ? '置顶' : thread.age;
}

function updateThreadStatus(thread) {
  if ($('#chat-filter-mode').value !== 'all') { renderNavigation(); return; }
  const label = $(`[data-session="${thread.id}"] .thread-age`, $('.project-scroll'));
  if (label) label.textContent = threadStatus(thread);
}

function renderHeader() {
  const project = projects.find(item => item.name === (session?.project ?? welcomeProject));
  const data = state();
  const environment = session?.environment ?? data.environment;
  const base = session?.base || data.base || project.branch;
  $('#current-project').textContent = project.name;
  const totals = stats(session?.files ?? []);
  $('#header-diff').replaceChildren(...(session?.files.length ? [totals] : []));
  $('#current-title').textContent = session?.title ?? '新对话';
  $('.workspace').dataset.welcome = String(!session);
  $('[data-action="thread-actions"]').hidden = !session;
  $('#current-environment').textContent = environment === 'worktree' ? 'Worktree' : '本地';
  $('#current-environment').hidden = !session;
  $('#current-branch').textContent = session && environment === 'worktree' ? base : project.branch;
  $('#current-branch').hidden = !session && environment === 'worktree';
  $('#environment-picker').hidden = Boolean(session);
  $('#environment').value = environment;
  $('#base-picker').hidden = Boolean(session) || environment !== 'worktree';
  const branches = [...new Set([project.branch, 'main'])];
  $('#base-branch').replaceChildren(...branches.map(name => new Option(name, name)));
  $('#base-branch').value = branches.includes(base) ? base : project.branch;
}

function renderComposer() {
  const data = state();
  draft.value = data.draft;
  draft.placeholder = session ? '要求后续更改' : '描述任务，使用 @ 引用文件';
  model.value = data.model;
  effort.value = data.effort;
  $('#collaboration-mode').value = data.mode ?? 'code';
  const attachments = $('#attachments');
  attachments.replaceChildren(...data.attachments.map((name, index) => {
    const attachment = element('div', 'attachment');
    const remove = element('button');
    remove.type = 'button';
    remove.setAttribute('aria-label', `移除 ${name}`);
    remove.dataset.remove = index;
    remove.innerHTML = icon('x');
    attachment.append(element('span', '', name), remove);
    return attachment;
  }));
  for (const [index, comment] of data.comments.entries()) {
    const chip = element('div', 'attachment');
    const remove = actionButton('×', 'remove-comment');
    remove.dataset.index = index;
    remove.setAttribute('aria-label', `移除 ${comment.location} 的反馈`);
    chip.append(element('span', '', comment.location), remove);
    attachments.append(chip);
  }
  attachments.hidden = !attachments.childElementCount;
  updateRunState();
  renderQueue();
}

function updateRunState() {
  const data = state();
  const busy = Boolean(data.active);
  const awaitingApproval = data.active?.phase === 'approval';
  const mode = awaitingApproval ? 'queue' : data.busyMode;
  const send = $('button[type="submit"]', form);
  const historyBlocked = historyPreview.blocked();
  send.disabled = historyBlocked || !draft.value.trim();
  const hint = $('#history-input-hint');
  hint.hidden = !historyBlocked;
  hint.textContent = historyBlocked ? (historyPreview.phase() === 'error' ? '会话记录未加载，重试后可发送；草稿已保留。' : '会话记录加载中，可编辑草稿，暂不可发送。') : '';
  send.setAttribute('aria-label', busy ? (mode === 'steer' ? '引导本轮' : '加入队列') : '发送');
  send.title = send.getAttribute('aria-label');
  $('[data-action="stop"]').hidden = !busy;
  $('#busy-control').hidden = !busy;
  $('#busy-mode').disabled = awaitingApproval;
  $('#busy-mode').value = mode;
  model.disabled = busy && mode === 'steer';
  effort.disabled = model.disabled;
  $('#run-state').textContent = historyBlocked ? (historyPreview.phase() === 'error' ? '加载失败' : '加载中') : awaitingApproval ? '等待审批' : busy ? '演示执行中' : '本地演示';
}

function promptLine(text, attachments = [], comments = []) {
  const row = element('div', 'prompt-line');
  const lines = [text];
  if (attachments.length) lines.push(`附件：${attachments.join('、')}`);
  for (const comment of comments) lines.push(`${comment.location} · ${comment.text}`);
  row.append(element('p', 'prompt', lines.join('\n')));
  return row;
}

function stats(list) {
  const lines = list.flatMap(file => file.lines);
  const counts = element('span', 'stats');
  counts.append(
    element('span', 'additions', '+' + lines.filter(([kind]) => kind === '+').length),
    element('span', 'deletions', '−' + lines.filter(([kind]) => kind === '-').length),
  );
  return counts;
}

function fileButton(file, index, fromMessage) {
  const button = element('button', fromMessage ? 'change-row' : 'review-file');
  button.innerHTML = icon('file-diff');
  button.dataset.file = index;
  if (fromMessage) button.dataset.reveal = '';
  else button.setAttribute('aria-pressed', index === selectedFile);
  button.append(element('code', '', file.name), stats([file]));
  if (!fromMessage && state().staged.includes(file.path)) {
    button.classList.add('is-staged');
    button.append(element('span', 'staged-label', '已暂存'));
  }
  button.title = file.path;
  return button;
}

function replyActions(replyId) {
  const actions = element('div', 'message-actions');
  actions.dataset.replyId = replyId;
  const items = [
    ['copy-reply', '复制回答', 'copy'], ['like-reply', '有帮助', 'thumb'],
    ['dislike-reply', '没有帮助', 'thumb'], ['fork-reply', '从此处开始新对话', 'fork'],
  ];
  for (const [action, label, glyph] of items) {
    const button = actionButton('', action, 'icon-button');
    button.setAttribute('aria-label', label);
    button.title = label;
    button.innerHTML = icon(glyph);
    if (action === 'like-reply' || action === 'dislike-reply') {
      button.setAttribute('aria-pressed', state().ratings?.[replyId] === action);
    }
    actions.append(button);
  }
  return actions;
}

function renderTranscript() {
  if (historyPreview.render(transcript)) return;
  if (!session) {
    transcript.replaceChildren($('#welcome-template').content.cloneNode(true));
    $('#project-picker').replaceChildren(...projects.map(project => new Option(project.name, project.name)));
    $('#project-picker').value = welcomeProject;
    mountIcons(transcript);
    return;
  }

  const children = [];
  if (session.prompt) {
    const assistant = element('article', 'assistant');
    if (session.commentary) {
      assistant.append(element('p', '', session.commentary));
      const activity = element('details', 'activity');
      activity.dataset.activity = 'initial';
      const summary = element('summary');
      summary.innerHTML = icon('terminal');
      summary.append(element('span', '', session.activity), element('span', 'activity-meta', session.duration));
      summary.insertAdjacentHTML('beforeend', icon('chevron-right', 'chevron'));
      activity.append(summary, element('pre', '', session.output));
      assistant.append(activity);
    }
    const reply = element('div', 'reply');
    reply.append(element('h2', '', session.heading));
    if (session.summary) reply.append(element('p', '', session.summary));
    if (session.bullets) {
      const list = element('ul');
      list.append(...session.bullets.map(text => element('li', '', text)));
      reply.append(list);
    }
    if (session.code) {
      const block = element('section', 'message-code');
      const header = element('div', 'message-code-header');
      header.append(element('span', '', 'tsx'), actionButton('复制代码', 'copy-code'));
      const pre = element('pre');
      pre.tabIndex = 0;
      const code = element('code');
      for (const text of session.code.split('\n')) code.append(element('span', 'code-line', text));
      pre.append(code);
      block.append(header, pre);
      reply.append(block);
    }
    if (session.files.length) {
      const changes = element('section', 'changes-box');
      changes.setAttribute('aria-label', '本次修改');
      const heading = element('div', 'changes-heading');
      heading.innerHTML = icon('git-pull-request');
      heading.append(element('span', '', `本次修改 · ${session.files.length} 个文件`), stats(session.files));
      changes.append(heading, ...session.files.map((file, index) => fileButton(file, index, true)));
      reply.append(changes);
    }
    assistant.append(reply);
    assistant.append(replyActions('initial'));
    children.push(promptLine(session.prompt), assistant);
    if (session.followup) {
      const followup = session.followup;
      const reply = element('article', 'assistant followup-reply');
      reply.append(element('p', '', followup.commentary));
      const activity = element('details', 'activity');
      activity.dataset.activity = 'followup';
      const summary = element('summary');
      summary.innerHTML = icon('list-checks');
      summary.append(element('span', '', '验证了 3 种消息状态'), element('span', 'activity-meta', '示例验证'));
      summary.insertAdjacentHTML('beforeend', icon('chevron-right', 'chevron'));
      activity.append(summary, element('pre', '', followup.output));
      const body = element('div', 'reply');
      const list = element('ul');
      list.append(...followup.bullets.map(text => element('li', '', text)));
      const changes = element('section', 'changes-box');
      const heading = element('div', 'changes-heading');
      heading.innerHTML = icon('git-pull-request');
      heading.append(element('span', '', '变更摘要'), stats(session.files));
      changes.append(heading, ...session.files.map((file, index) => fileButton(file, index, true)));
      const preview = actionButton('', 'browser', 'reply-preview');
      preview.innerHTML = icon('globe');
      const caption = element('span');
      caption.append(element('strong', '', session.project + ' / 工作台'), element('small', '', '项目预览 · README.md'));
      preview.append(caption, element('span', 'preview-open', '在浏览器中打开'));
      body.append(element('h2', '', followup.heading), element('p', '', followup.summary), list, preview, changes);
      reply.append(activity, body, replyActions('followup'));
      const prompt = promptLine(followup.prompt);
      prompt.dataset.previewFollowup = 'true';
      children.push(prompt, reply);
    }
  }
  children.push(...state().turns.map(renderTurn));
  transcript.replaceChildren(...children);
}

function renderReview() {
  const review = $('#review-content');
  if (historyPreview.blocked()) {
    review.replaceChildren(element('p', 'review-empty', '加载会话记录后显示变更。'));
    return;
  }
  if (!session?.files.length) {
    review.replaceChildren(element('p', 'review-empty', '当前会话没有演示变更。'));
    return;
  }
  const file = session.files[selectedFile];
  const summary = element('div', 'review-summary');
  const branch = element('span', 'review-branch');
  branch.innerHTML = icon('git-branch');
  branch.append(element('code', '', $('#current-branch').textContent));
  const totals = element('div', 'review-totals');
  const staged = state().staged;
  const count = `${session.files.length} 个文件已修改`;
  totals.append(element('span', '', staged.length ? `${count} · ${staged.length} 已暂存` : count), stats(session.files));
  summary.append(branch, totals);

  const fileList = element('div', 'review-files');
  fileList.append(...session.files.map((item, index) => fileButton(item, index, false)));

  const diff = element('section', 'diff');
  diff.setAttribute('aria-label', file.path);
  const caption = element('div', 'diff-caption');
  const copy = element('button', 'icon-button');
  copy.dataset.action = 'copy';
  copy.setAttribute('aria-label', '复制差异片段');
  copy.title = '复制差异片段';
  copy.innerHTML = icon('copy');
  const stage = actionButton(staged.includes(file.path) ? '取消暂存' : '暂存', 'stage');
  const diffActions = element('div', 'diff-actions');
  diffActions.append(stage, copy);
  const openFile = actionButton(file.name, 'diff-source', 'diff-source');
  openFile.dataset.value = files.findIndex(item => item.path === file.path);
  openFile.setAttribute('aria-label', '打开源码 ' + file.name);
  openFile.title = file.path;
  caption.append(openFile, diffActions);
  const oldCount = file.lines.filter(([kind]) => kind !== '+').length;
  const newCount = file.lines.filter(([kind]) => kind !== '-').length;
  const range = element('div', 'diff-range', `@@ -${file.start},${oldCount} +${file.start},${newCount} @@`);
  const lines = element('div', 'diff-lines');
  lines.tabIndex = 0;
  lines.setAttribute('aria-label', '差异行');
  let before = file.start;
  let after = file.start;
  for (const [kind, text] of file.lines) {
    const row = element('div', 'diff-line');
    if (kind === '+') row.classList.add('add');
    if (kind === '-') row.classList.add('remove');
    const oldLine = kind === '+' ? '' : before++;
    const newLine = kind === '-' ? '' : after++;
    const comment = actionButton('', 'line-comment', 'line-comment');
    comment.dataset.location = `${file.path}:${newLine || oldLine}${newLine ? '' : '（旧）'}`;
    comment.setAttribute('aria-label', `在 ${comment.dataset.location} 添加反馈`);
    comment.innerHTML = icon('plus');
    row.append(element('span', '', oldLine), element('span', '', newLine));
    row.append(element('code', '', kind + ' ' + text));
    row.append(comment);
    lines.append(row);
    const pending = state().comments.filter(item => item.location === comment.dataset.location);
    for (const item of pending) lines.append(element('p', 'inline-feedback', item.text));
    if (state().finding && file === files[0] && newLine === file.start + 1) {
      const finding = element('div', 'inline-finding');
      finding.append(
        element('strong', '', '检查空会话'),
        element('p', '', '建议补测 groupTurns 收到空数组时的返回值。'),
        actionButton('关闭发现', 'dismiss-finding'),
      );
      lines.append(finding);
    }
  }
  diff.append(caption, range, lines);
  const filesHeading = element('div', 'review-files-heading');
  filesHeading.append(element('h3', 'inspector-title', '变更文件'), actionButton('运行审阅', 'run-review'));
  review.replaceChildren(summary, filesHeading, fileList, diff);
}

const chatFilter = $('#chat-filter');
$('#filter-chats').addEventListener('click', () => {
  chatFilter.hidden = !chatFilter.hidden;
  $('#filter-chats').setAttribute('aria-expanded', String(!chatFilter.hidden));
  if (!chatFilter.hidden) $('#chat-filter-mode').focus();
});
$('#chat-filter-mode').addEventListener('change', () => {
  chatFilter.hidden = true;
  $('#filter-chats').setAttribute('aria-expanded', 'false');
  $('#filter-chats').setAttribute('aria-pressed', String($('#chat-filter-mode').value !== 'all'));
  renderNavigation();
  $('#filter-chats').focus();
});

const chatTrail = [];
let trailPosition = -1;

function visitChat(direction) {
  for (let index = trailPosition + direction; index >= 0 && index < chatTrail.length; index += direction) {
    const entry = chatTrail[index];
    const thread = sessions.find(item => item.id === entry.id && !item.archived);
    if (entry.id && !thread) continue;
    trailPosition = index;
    welcomeProject = entry.project;
    showSession(thread ?? null, true);
    return;
  }
}

function updateTrail(fromHistory) {
  const entry = { id: session?.id ?? null, project: session?.project ?? welcomeProject };
  const last = chatTrail[trailPosition];
  if (!fromHistory && (!last || entry.id !== last.id || entry.project !== last.project)) {
    chatTrail.splice(trailPosition + 1, Infinity, entry);
    trailPosition = chatTrail.length - 1;
  }
  const available = item => !item.id || sessions.some(thread => thread.id === item.id && !thread.archived);
  $('[data-history="-1"]').disabled = !chatTrail.slice(0, trailPosition).some(available);
  $('[data-history="1"]').disabled = !chatTrail.slice(trailPosition + 1).some(available);
}

function showSession(next, fromHistory = false) {
  readingState.capture();
  session = next;
  historyPreview.select(next);
  if (next) {
    welcomeProject = next.project;
    collapsedProjects.delete(next.project);
  }
  updateTrail(fromHistory);
  selectedFile = 0;
  if (mobile.matches) setPanel('navigation', false, false);
  if (reviewing && narrow.matches) setPanel('inspector', false, false);
  renderNavigation();
  renderHeader();
  renderTranscript();
  renderComposer();
  renderReview();
  document.dispatchEvent(new Event('sessionchange'));
  readingState.restore();
}

function scrollToLatestTurn() {
  conversation.scrollTop = conversation.scrollHeight;
  const followup = $('[data-preview-followup]', transcript);
  if (followup && !state().turns.length) {
    conversation.scrollTop += followup.getBoundingClientRect().top - conversation.getBoundingClientRect().top - 24;
  }
}

function showSearchResults() {
  const query = searchInput.value.trim().toLowerCase().replace(/^>\s*/, '');
  const matches = sessions.filter(thread => !thread.archived
    && `${thread.title} ${thread.project}`.toLowerCase().includes(query));
  const results = matches.map(thread => {
    const button = element('button');
    button.innerHTML = icon('message-circle');
    button.append(element('span', '', thread.title), element('small', '', thread.project));
    button.dataset.session = thread.id;
    return button;
  });
  const commands = [
    { title: '新对话', action: 'new', icon: 'plus' },
    { title: '打开浏览器', action: 'browser', icon: 'globe' },
    { title: '打开代码工作区', action: 'code-panel', icon: 'code-xml' },
    { title: '打开终端', action: 'terminal', icon: 'terminal' },
    { title: '打开设置', action: 'settings', icon: 'settings' },
    { title: '查看上下文', action: 'show-context', icon: 'list-checks' },
  ].filter(command => command.title.includes(query));
  for (const command of commands) {
    const button = actionButton('', command.action);
    button.dataset.palette = 'true';
    button.innerHTML = icon(command.icon);
    button.append(element('span', '', command.title), element('small', '', '操作'));
    results.push(button);
  }
  $('#search-results').replaceChildren(...(results.length ? results : [element('p', '', '没有匹配的会话。')]));
}

function openSearch() {
  if ($('dialog[open]')) return;
  searchInput.value = '';
  showSearchResults();
  searchDialog.showModal();
  searchInput.focus();
}

const phaseLabels = {
  running: '演示执行中', approval: '等待审批', completed: '本地演示已完成。',
  failed: '工具执行失败。', declined: '已拒绝此次命令。', interrupted: '演示已停止。',
};

function renderTurn(turn) {
  const article = element('article', 'demo-turn');
  article.dataset.turn = turn.id;
  article.append(promptLine(turn.prompt, turn.attachments, turn.comments));
  for (const guidance of turn.guidance) {
    const message = promptLine(guidance.prompt, guidance.attachments, guidance.comments);
    message.prepend(element('span', 'guidance-label', '本轮引导'));
    article.append(message);
  }
  const completed = turn.phase === 'completed';
  const card = element(completed ? 'details' : 'div', completed ? 'turn-activity turn-execution' : 'turn-activity');
  if (completed) card.dataset.activity = 'execution:' + turn.id;
  const heading = element(completed ? 'summary' : 'div', 'turn-heading');
  const mark = element('span', 'agent-mark');
  mark.innerHTML = icon('terminal');
  heading.append(mark, element('strong', '', phaseLabels[turn.phase]));
  if (completed) heading.insertAdjacentHTML('beforeend', icon('chevron-right', 'execution-chevron'));
  if (turn.model && turn.effort) heading.append(element('small', 'turn-model', `${turn.model} · ${turn.effort}`));
  card.append(heading);
  const plan = element('ol', 'turn-plan');
  plan.setAttribute('aria-label', '任务计划');
  const steps = turn.review ? ['读取改动', '检查回归', '标注发现']
    : turn.mode === 'plan' ? ['确认目标', '拆分实施步骤', '整理验证方式'] : ['检查项目', '验证改动', '汇总结果'];
  for (const [i, label] of steps.entries()) {
    const done = i < turn.step;
    const active = i === turn.step && ['running', 'approval'].includes(turn.phase);
    const marker = done ? '已完成' : active ? '进行中' : '待执行';
    plan.append(element('li', done ? 'step-done' : active ? 'step-active' : '', `${label} · ${marker}`));
  }
  card.append(plan);
  if (turn.step > 0 || turn.phase === 'failed') {
    const output = element('details', 'activity');
    output.dataset.activity = turn.id;
    const summary = element('summary', '', '工具输出');
    const log = turn.phase === 'failed' ? 'npm test\n退出码 1 · 有一项示例断言失败' : '检查项目入口\n读取工作区示例文件';
    output.append(summary, element('pre', '', log));
    card.append(output);
  }
  if (turn.phase === 'approval') {
    const approval = element('section', 'approval-card');
    approval.setAttribute('aria-label', '命令审批');
    const label = element('strong', '', '允许执行此命令？');
    const command = element('code', '', 'npm test');
    const meta = element('p', '', `项目：${session.project} · 运行项目测试（演示）`);
    const actions = element('div', 'thread-actions');
    actions.append(
      actionButton('允许一次', 'approve-once', 'action-button primary'),
      actionButton('允许此会话', 'approve-session'),
      actionButton('拒绝', 'decline'),
    );
    for (const button of actions.children) button.dataset.turnId = turn.id;
    approval.append(label, command, meta, actions);
    card.append(approval);
  }
  if (turn.decision) card.append(element('p', 'runtime-label', turn.decision));
  if (turn.phase === 'failed') {
    const retry = actionButton('重试', 'retry-turn');
    retry.dataset.turnId = turn.id;
    card.append(retry);
  }
  article.append(card);
  if (turn.response) article.append(element('p', 'turn-response', turn.response));
  if (turn.mode === 'plan' && completed) {
    const implement = actionButton('开始实施', 'implement-plan', 'action-button primary');
    implement.dataset.turnId = turn.id;
    article.append(implement);
  }
  if (completed) article.append(replyActions(turn.id));
  return article;
}

function renderQueue() {
  const panel = $('#queue');
  const data = state();
  panel.hidden = !data.queue.length || historyPreview.blocked();
  panel.replaceChildren();
  if (!data.queue.length) return;
  const heading = element('div', 'queue-heading');
  heading.append(element('strong', '', `后续消息 · ${data.queue.length}`));
  if (!data.active) heading.append(actionButton('继续队列', 'resume-queue'));
  panel.append(heading);
  const list = element('div', 'queue-list');
  for (const item of data.queue) {
    const row = element('div', 'queue-row');
    const cancel = actionButton('移除', 'cancel-queued');
    cancel.dataset.queueId = item.id;
    row.append(element('span', '', item.prompt), cancel);
    list.append(row);
  }
  panel.append(list);
}

function updateTurnView(thread, turn, jump = false) {
  saveHistory();
  if (session !== thread || historyPreview.blocked()) return;
  const follow = conversation.scrollHeight - conversation.scrollTop - conversation.clientHeight < 48;
  const previous = transcript.querySelector(`[data-turn="${turn.id}"]`);
  const openActivities = new Set([...(previous?.querySelectorAll('details[data-activity][open]') ?? [])].map(node => node.dataset.activity));
  const view = renderTurn(turn);
  for (const detail of view.querySelectorAll('details[data-activity]')) detail.open = openActivities.has(detail.dataset.activity);
  // Completing a run must not hide tool output the reader has explicitly opened.
  if (!previous?.querySelector('.turn-execution') && previous?.querySelector('details.activity[open]')) {
    view.querySelector('.turn-execution')?.setAttribute('open', '');
  }
  if (previous) previous.replaceWith(view);
  else transcript.append(view);
  updateRunState();
  renderQueue();
  document.dispatchEvent(new Event('turnviewchange'));
  if (jump || follow) conversation.scrollTop = conversation.scrollHeight;
}

function finishTurn(thread, turn, phase) {
  const data = stateFor(thread.id);
  if (data.active !== turn) return;
  clearTimeout(data.timer);
  data.timer = 0;
  data.active = null;
  turn.phase = phase;
  if (turn.review && phase === 'completed') {
    data.finding = true;
    if (session === thread) renderReview();
  }
  updateTurnView(thread, turn);
  if (phase === 'completed' && data.queue.length) startTurn(thread, data.queue.shift());
  updateThreadStatus(thread);
  document.dispatchEvent(new Event('turnchange'));
}

function streamResult(thread, turn) {
  const data = stateFor(thread.id);
  if (data.active !== turn) return;
  turn.step = 2;
  turn.response = turn.review ? '已检查示例差异。' : turn.mode === 'plan' ? '计划包含范围确认、实施和验证三个步骤。' : '已检查示例文件。';
  updateTurnView(thread, turn);
  data.timer = setTimeout(() => {
    if (data.active !== turn) return;
    turn.response += turn.review ? ' 打开审阅面板即可查看行内发现。'
      : turn.mode === 'plan' ? ' 可开始实施并跟踪每一步的执行结果。' : ' 项目结构和改动已汇总。';
    turn.step = 3;
    finishTurn(thread, turn, 'completed');
  }, 1100);
}

function startTurn(thread, input, jump = false) {
  const data = stateFor(thread.id);
  const turn = { ...input, phase: 'running', step: 0, guidance: [], response: '' };
  data.active = turn;
  data.turns.push(turn);
  updateTurnView(thread, turn, jump);
  updateThreadStatus(thread);
  data.timer = setTimeout(() => {
    if (data.active !== turn) return;
    turn.step = 1;
    if (turn.mode === 'plan') {
      streamResult(thread, turn);
    } else if (turn.scenario === 'failure') {
      finishTurn(thread, turn, 'failed');
    } else if ((turn.scenario === 'approval' || workbench.askCommands || workbench.permissions === 'readonly') && !data.sessionApproved) {
      data.timer = 0;
      turn.phase = 'approval';
      updateTurnView(thread, turn);
      updateThreadStatus(thread);
    } else {
      streamResult(thread, turn);
    }
  }, 700);
}

function answerApproval(button) {
  const data = state();
  const turn = data.active;
  if (turn?.phase !== 'approval' || turn.id !== button.dataset.turnId) return;
  draft.focus();
  if (button.dataset.action === 'decline') {
    turn.decision = '审批结果：拒绝';
    finishTurn(session, turn, 'declined');
    return;
  }
  const sessionApproval = button.dataset.action === 'approve-session';
  if (sessionApproval) data.sessionApproved = true;
  turn.decision = sessionApproval ? '审批结果：允许此会话' : '审批结果：允许一次';
  turn.phase = 'running';
  streamResult(session, turn);
  updateThreadStatus(session);
}

function ensureThread(text) {
  if (!session) {
    const data = state();
    const newThread = {
      id: crypto.randomUUID(), project: welcomeProject,
      title: Array.from(text.split('\n')[0]).slice(0, 26).join(''), age: '刚刚', files: [],
      environment: data.environment, base: $('#base-branch').value,
    };
    sessions.unshift(newThread);
    viewState.set(newThread.id, data);
    viewState.delete(undefined);
    showSession(newThread);
  }
  return session;
}

function submitMessage(event) {
  event.preventDefault();
  if (historyPreview.blocked()) return;
  const text = draft.value.trim();
  if (!text) return;
  ensureThread(text);
  const data = state();
  const input = {
    id: crypto.randomUUID(), prompt: text, attachments: data.attachments, comments: data.comments,
    scenario: $('#scenario').value, model: data.model, effort: data.effort, mode: data.mode ?? 'code',
  };
  $('#scenario').value = 'normal';
  data.draft = '';
  data.attachments = [];
  data.comments = [];
  if (data.active?.phase === 'running' && data.busyMode === 'steer') {
    data.active.guidance.push(input);
    updateTurnView(session, data.active, true);
  } else {
    sendOrQueue(input);
  }
  if (input.comments.length) renderReview();
  renderComposer();
  draft.focus();
}

function sendOrQueue(input) {
  if (historyPreview.blocked()) { toast('请先加载会话记录'); return; }
  const data = state();
  if (data.active || data.queue.length) {
    data.queue.push(input);
    saveHistory();
    renderQueue();
  } else {
    startTurn(session, input, true);
  }
}

form.addEventListener('submit', submitMessage);

draft.addEventListener('input', () => {
  state().draft = draft.value;
  updateRunState();
  if (session) saveHistory();
});
draft.addEventListener('keydown', event => {
  if (event.defaultPrevented || event.key !== 'Enter' || event.shiftKey || event.isComposing || event.keyCode === 229) return;
  event.preventDefault();
  form.requestSubmit();
});
model.addEventListener('change', () => {
  state().model = model.value;
  saveHistory();
});
effort.addEventListener('change', () => {
  state().effort = effort.value;
  saveHistory();
});
$('#collaboration-mode').addEventListener('change', event => {
  state().mode = event.target.value;
  saveHistory();
});
$('#busy-mode').addEventListener('change', event => {
  state().busyMode = event.target.value;
  updateRunState();
});
$('#environment').addEventListener('change', event => {
  state().environment = event.target.value;
  renderHeader();
});
$('#base-branch').addEventListener('change', event => {
  state().base = event.target.value;
});
$('#attach').addEventListener('change', event => {
  state().attachments.push(...Array.from(event.target.files, file => file.name));
  event.target.value = '';
  renderComposer();
  if (session) saveHistory();
});
searchInput.addEventListener('input', showSearchResults);
searchInput.addEventListener('keydown', event => {
  const first = $('button', $('#search-results'));
  if (event.key === 'Enter' && !event.isComposing) first?.click();
  if (event.key === 'ArrowDown') {
    event.preventDefault();
    first?.focus();
  }
});

const systemColorMode = matchMedia('(prefers-color-scheme: dark)');
function setTheme(preference) {
  const chosen = ['light', 'dark'].includes(preference) ? preference : 'system';
  document.documentElement.dataset.theme = chosen === 'system'
    ? (systemColorMode.matches ? 'dark' : 'light') : chosen;
  $('#theme').value = chosen;
}
let savedColorMode;
try {
  savedColorMode = localStorage.getItem('rpi-layout-review-theme');
} catch {}
setTheme(savedColorMode);
systemColorMode.addEventListener('change', () => {
  if ($('#theme').value === 'system') setTheme('system');
});
$('#theme').addEventListener('change', event => {
  setTheme(event.target.value);
  try {
    localStorage.setItem('rpi-layout-review-theme', event.target.value);
  } catch {}
});

transcript.addEventListener('change', event => {
  if (event.target.id !== 'project-picker') return;
  welcomeProject = event.target.value;
  state().base = '';
  renderHeader();
});

function openThreadActions() {
  if (!session) return;
  $('#thread-name').value = session.title;
  $('[data-action="pin-thread"]').textContent = session.pinned ? '取消置顶' : '置顶会话';
  const busy = Boolean(state().active);
  $('[data-action="archive"]').disabled = busy;
  $('[data-action="fork"]').disabled = busy;
  const handoff = $('[data-action="handoff"]');
  handoff.textContent = session.environment === 'worktree' ? '交接到本地' : '交接到 Worktree';
  handoff.disabled = busy;
  threadDialog.showModal();
}

$('#rename-thread').addEventListener('submit', event => {
  event.preventDefault();
  const name = $('#thread-name').value.trim();
  if (!name || !session) return;
  session.title = name;
  threadDialog.close();
  renderHeader();
  renderNavigation();
  saveHistory();
});

function renderArchives() {
  const list = $('#archived-threads');
  const archived = sessions.filter(thread => thread.archived);
  const rows = archived.map(thread => {
    const row = element('div', 'archive-row');
    const restore = actionButton('恢复', 'restore');
    restore.dataset.threadId = thread.id;
    row.append(element('span', '', `${thread.project} / ${thread.title}`), restore);
    return row;
  });
  list.replaceChildren(...(rows.length ? rows : [element('p', 'review-empty', '没有已归档的会话。')]));
}

function changeThread(action, replyId = null) {
  if (!session || (state().active && !replyId)) return;
  threadDialog.close();
  if (action === 'archive') {
    session.archived = true;
    showSession(sessions.find(thread => !thread.archived) ?? null);
  } else if (action === 'handoff') {
    session.environment = session.environment === 'worktree' ? 'local' : 'worktree';
    state().sessionApproved = false;
    renderHeader();
    renderReview();
  } else {
    const fork = {
      ...session, fixture: session.fixture ?? session.id, id: crypto.randomUUID(),
      title: `${session.title}（分叉）`, age: '刚刚', pinned: false,
    };
    const from = state();
    const data = stateFor(fork.id);
    let turns = from.turns;
    if (replyId === 'initial') { delete fork.followup; fork.omitFollowup = true; }
    if (replyId) turns = turns.slice(0, turns.findIndex(turn => turn.id === replyId) + 1);
    data.turns = structuredClone(turns);
    data.model = from.model;
    data.effort = from.effort;
    sessions.unshift(fork);
    showSession(fork);
  }
  saveHistory();
  conversation.focus({ preventScroll: true });
}

function openComment(location, title) {
  commentTarget = { threadId: session?.id, location };
  $('#comment-title').textContent = title;
  $('#comment-location').textContent = location;
  $('#review-comment').value = '';
  commentDialog.showModal();
  $('#review-comment').focus();
}

$('#review-comment-form').addEventListener('submit', event => {
  event.preventDefault();
  const text = $('#review-comment').value.trim();
  if (!text || session?.id !== commentTarget?.threadId) return;
  state().comments.push({ location: commentTarget.location, text });
  commentDialog.close();
  renderComposer();
  renderReview();
  saveHistory();
  if (narrow.matches) setPanel('inspector', false, false);
  draft.focus();
});

function closeDrawer() {
  if (reviewing && narrow.matches) setPanel('inspector', false);
  else if (navigating && mobile.matches) setPanel('navigation', false);
}

document.addEventListener('click', event => {
  const button = event.target.closest('button');
  if (!button) {
    if (event.target.matches('.scrim')) closeDrawer();
    return;
  }
  if (button.dataset.history) visitChat(Number(button.dataset.history));
  if (button.dataset.palette && searchDialog.open) searchDialog.close();
  if (button.dataset.session) {
    if (searchDialog.open) searchDialog.close();
    showSession(sessions.find(thread => thread.id === button.dataset.session));
    $('.conversation').focus({ preventScroll: true });
  }
  if (button.dataset.suggestion) {
    draft.value = [draft.value, button.dataset.suggestion].filter(value => value.trim()).join('\n');
    draft.dispatchEvent(new Event('input'));
    draft.focus();
  }
  if (button.dataset.file !== undefined) {
    selectedFile = Number(button.dataset.file);
    if (button.dataset.reveal !== undefined) setPanel('inspector', true, false);
    renderReview();
    $(`.review-file[data-file="${selectedFile}"]`).focus();
  }
  if (button.dataset.remove !== undefined) {
    state().attachments.splice(Number(button.dataset.remove), 1);
    renderComposer();
    $('.add-control').focus();
    saveHistory();
  }
  switch (button.dataset.action) {
    case 'toggle-section': {
      const id = button.getAttribute('aria-controls');
      if (collapsedSections.has(id)) collapsedSections.delete(id);
      else collapsedSections.add(id);
      syncSection(id);
      saveNavigation();
      break;
    }
    case 'browse-projects': {
      setPanel('navigation', true, false);
      const name = session?.project ?? welcomeProject;
      const section = pinnedProjects.has(name) ? 'pinned-threads' : 'projects';
      collapsedSections.delete(section);
      $('#chat-filter-mode').value = 'all';
      $('#filter-chats').setAttribute('aria-pressed', 'false');
      $('#chat-filter').hidden = true;
      $('#filter-chats').setAttribute('aria-expanded', 'false');
      renderNavigation();
      saveNavigation();
      $(`[data-project-group="${CSS.escape(name)}"] summary`)?.focus();
      break;
    }
    case 'project-menu':
      openProjectMenu(button);
      break;
    case 'pin-project': {
      const name = projectMenu.dataset.project;
      projectMenu.hidePopover();
      if (pinnedProjects.has(name)) pinnedProjects.delete(name);
      else pinnedProjects.add(name);
      collapsedSections.delete(pinnedProjects.has(name) ? 'pinned-threads' : 'projects');
      saveNavigation();
      renderNavigation();
      projectMenuTrigger(name)?.focus();
      break;
    }
    case 'project-chat':
      event.preventDefault();
      if (projectMenu.matches(':popover-open')) projectMenu.hidePopover();
      welcomeProject = button.dataset.project;
      showSession(null);
      draft.focus();
      break;
    case 'like-reply':
    case 'dislike-reply': {
      const group = button.closest('.message-actions');
      const ratings = state().ratings ??= {};
      const key = group.dataset.replyId;
      ratings[key] = ratings[key] === button.dataset.action ? null : button.dataset.action;
      for (const sibling of group.querySelectorAll('[aria-pressed]')) sibling.setAttribute('aria-pressed', String(ratings[key] === sibling.dataset.action));
      saveHistory();
      break;
    }
    case 'fork-reply': changeThread('fork', button.closest('.message-actions').dataset.replyId); break;
    case 'new':
      showSession(null);
      draft.focus();
      break;
    case 'search': openSearch(); break;
    case 'settings': settingsDialog.showModal(); break;
    case 'thread-actions': openThreadActions(); break;
    case 'pin-thread':
      if (!session) break;
      session.pinned = !session.pinned;
      threadDialog.close();
      renderNavigation();
      saveHistory();
      break;
    case 'archives':
      renderArchives();
      archiveDialog.showModal();
      break;
    case 'archive':
    case 'fork':
    case 'handoff':
      changeThread(button.dataset.action);
      break;
    case 'restore': {
      const thread = sessions.find(item => item.id === button.dataset.threadId);
      if (!thread) break;
      thread.archived = false;
      archiveDialog.close();
      showSession(thread);
      saveHistory();
      conversation.focus({ preventScroll: true });
      break;
    }
    case 'navigation': setPanel('navigation', !navigating); break;
    case 'close-navigation': setPanel('navigation', false); $('[data-action="navigation"]').focus(); break;
    case 'inspector': setPanel('inspector', !reviewing || shell.dataset.tool !== 'changes'); break;
    case 'close-inspector': setPanel('inspector', false); break;
    case 'stop':
      if (state().active) finishTurn(session, state().active, 'interrupted');
      draft.focus();
      break;
    case 'approve-once':
    case 'approve-session':
    case 'decline':
      answerApproval(button);
      break;
    case 'cancel-queued':
      state().queue = state().queue.filter(item => item.id !== button.dataset.queueId);
      renderQueue();
      saveHistory();
      draft.focus();
      break;
    case 'resume-queue':
      if (!state().active && state().queue.length) startTurn(session, state().queue.shift(), true);
      draft.focus();
      break;
    case 'line-comment':
      openComment(button.dataset.location, '行内反馈');
      break;
    case 'remove-comment':
      state().comments.splice(Number(button.dataset.index), 1);
      renderComposer();
      renderReview();
      saveHistory();
      draft.focus();
      break;
    case 'retry-turn': {
      const turn = state().turns.find(item => item.id === button.dataset.turnId);
      if (!turn || turn.phase !== 'failed') break;
      const { prompt, attachments, comments, model, effort } = turn;
      sendOrQueue({ id: crypto.randomUUID(), prompt, attachments, comments, model, effort, scenario: 'normal' });
      draft.focus();
      break;
    }
    case 'implement-plan': {
      const turn = state().turns.find(item => item.id === button.dataset.turnId);
      if (!turn || turn.mode !== 'plan' || turn.phase !== 'completed') break;
      state().mode = 'code';
      renderComposer();
      sendOrQueue({
        id: crypto.randomUUID(), prompt: '按计划实施：' + turn.prompt,
        attachments: [], comments: [], model: state().model, effort: state().effort, mode: 'code', scenario: 'normal',
      });
      draft.focus();
      break;
    }
    case 'run-review':
      if (narrow.matches) setPanel('inspector', false, false);
      if (workbench.reviewDelivery === 'detached') {
        const thread = {
          ...session, id: crypto.randomUUID(), title: '审阅 · ' + session.title,
          fixture: session.fixture ?? session.id, age: '刚刚', pinned: false,
        };
        sessions.unshift(thread);
        showSession(thread);
        saveHistory();
      }
      sendOrQueue({
        id: crypto.randomUUID(), prompt: '审阅当前未提交的演示变更', attachments: [], comments: [],
        model: state().model, effort: state().effort, scenario: 'normal', review: true,
      });
      if (narrow.matches) draft.focus();
      break;
    case 'dismiss-finding':
      state().finding = false;
      saveHistory();
      renderReview();
      $('[data-action="run-review"]').focus();
      break;
    case 'stage': {
      const path = session.files[selectedFile].path;
      const staged = state().staged;
      if (staged.includes(path)) state().staged = staged.filter(item => item !== path);
      else staged.push(path);
      saveHistory();
      renderReview();
      $('[data-action="stage"]').focus();
      break;
    }
    case 'copy': copyDiff(button); break;
  }
});

async function copyDiff(button) {
  const line = $('.diff-range').textContent;
  const excerpt = session.files[selectedFile].lines.map(([kind, text]) => kind + text);
  try {
    await navigator.clipboard.writeText([line, ...excerpt].join('\n'));
    button.setAttribute('aria-label', '已复制差异片段');
    button.title = '已复制差异片段';
    button.innerHTML = icon('check');
  } catch {
    button.setAttribute('aria-label', '无法复制，可手动选择差异文本');
    button.title = '无法复制，可手动选择差异文本';
  }
}

document.addEventListener('keydown', event => {
  if (event.defaultPrevented || event.isComposing || event.keyCode === 229) return;
  if ($('dialog[open]')) return;
  if (event.key === 'Escape') closeDrawer();
  if (!(event.metaKey || event.ctrlKey)) return;
  if (event.key.toLowerCase() === 'k') {
    event.preventDefault();
    openSearch();
  }
  const key = event.key.toLowerCase();
  const action = key === 'n' || (key === 'o' && event.shiftKey) ? 'new'
    : key === 'j' ? 'terminal' : key === 'b' && !event.shiftKey ? 'navigation' : key === ',' ? 'settings' : null;
  if (action) {
    event.preventDefault();
    $(`[data-action="${action}"]`).click();
  }
});

compact.addEventListener('change', () => {
  navigating = !compact.matches;
  syncPanels();
});

mobile.addEventListener('change', () => {
  navigating = !compact.matches;
  syncPanels();
});
narrow.addEventListener('change', () => {
  if (narrow.matches) reviewing = false;
  syncPanels();
});

const shortcut = /Mac/.test(navigator.platform) ? '⌘' : 'Ctrl';
$('[data-action="new"] kbd').textContent = shortcut + ' N';
$('[data-action="search"] kbd').textContent = shortcut + ' K';
mountIcons();
restoreHistory();
historyPreview.seed(sessions);
syncPanels();
showSession(sessions.find(thread => !thread.archived) ?? null);
initializeWorkbench();
