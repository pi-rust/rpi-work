// This prototype owns browser-local state. Commands and integrations use fixtures.
const workbench = {
  projects: [], fontSize: '16', permissions: 'workspace', askCommands: false, reviewDelivery: 'inline',
  density: 'comfortable', messageLayout: 'reading', codeNumbers: true, notifyFinish: false, defaultModel: 'GPT-5.4', defaultEffort: '中',
  followUpMode: 'queue', showContext: true, showBottomControl: true,
  environments: {}, resources: {}, connections: {}, sourceDrafts: {}, automations: [],
};
try {
  const saved = JSON.parse(localStorage.getItem('rpi-workbench-v1'));
  if (saved && typeof saved === 'object') {
    workbench.projects = Array.isArray(saved.projects)
      ? saved.projects.filter(item => item && typeof item.name === 'string' && typeof item.path === 'string' && item.branch === 'main')
      : [];
    workbench.automations = Array.isArray(saved.automations)
      ? saved.automations.filter(item => item && typeof item.id === 'string' && typeof item.name === 'string'
        && typeof item.prompt === 'string' && typeof item.project === 'string' && Array.isArray(item.runs))
      : [];
    for (const key of ['environments', 'resources', 'connections', 'sourceDrafts']) {
      if (saved[key] && typeof saved[key] === 'object' && !Array.isArray(saved[key])) workbench[key] = saved[key];
    }
    if (['16', '18'].includes(saved.fontSize)) workbench.fontSize = saved.fontSize;
    if (['workspace', 'readonly', 'full'].includes(saved.permissions)) workbench.permissions = saved.permissions;
    workbench.askCommands = Boolean(saved.askCommands);
    if (['inline', 'detached'].includes(saved.reviewDelivery)) workbench.reviewDelivery = saved.reviewDelivery;
    if (['compact', 'comfortable'].includes(saved.density)) workbench.density = saved.density;
    if (['reading', 'classic'].includes(saved.messageLayout)) workbench.messageLayout = saved.messageLayout;
    if (typeof saved.codeNumbers === 'boolean') workbench.codeNumbers = saved.codeNumbers;
    workbench.notifyFinish = Boolean(saved.notifyFinish);
    if (typeof saved.showBottomControl === 'boolean') workbench.showBottomControl = saved.showBottomControl;
    if (['queue', 'steer'].includes(saved.followUpMode)) workbench.followUpMode = saved.followUpMode;
    if (typeof saved.showContext === 'boolean') workbench.showContext = saved.showContext;
    if (['GPT-5.4', 'GPT-5.3-Codex'].includes(saved.defaultModel)) workbench.defaultModel = saved.defaultModel;
    if (['中', '高', '低'].includes(saved.defaultEffort)) workbench.defaultEffort = saved.defaultEffort;
  }
} catch {
  // Storage may be disabled or an old preview may have left invalid data.
}

const sourceFolders = new Map();

function initializeWorkbench() {
  const workspace = $('.workspace');
  const pageContent = $('#page-content');
  const terminal = $('#terminal-panel');
  const terminalSessions = new Map();
  const catalog = [
    { id: 'code-review', type: 'skills', title: '代码审阅', icon: 'file-diff', subtitle: '检查变更中的回归和测试缺口', scope: '项目', detail: '阅读当前差异，按风险排序输出行内发现，并给出复现或验证步骤。', prompt: '审阅当前改动，优先检查回归和测试缺口。' },
    { id: 'test-runner', type: 'skills', title: '测试与修复', icon: 'list-checks', subtitle: '复现失败，修复并重新验证', scope: '项目', detail: '先检查项目的测试入口，复现失败后做最小修改，并记录验证结果。', prompt: '检查测试入口并运行测试，定位失败原因后修复。' },
    { id: 'skill-creator', type: 'skills', title: '技能创建', icon: 'code-xml', subtitle: '将常用流程整理为可复用技能', scope: '全局', detail: '定义触发条件、步骤和验证方式，产出可复用的 SKILL.md。', prompt: '帮我把当前项目的发布流程整理为一个技能。' },
    { id: 'github', type: 'plugins', title: 'GitHub', icon: 'git-branch', subtitle: 'Issues、Pull Requests 和代码审阅', scope: '账户', detail: '使用仓库、Issue 和 Pull Request 的上下文协助开发。连接状态为本地演示。' },
    { id: 'linear', type: 'plugins', title: 'Linear', icon: 'blocks', subtitle: '从项目任务开始开发', scope: '账户', detail: '查看任务、整理需求，并将开发结果关联回任务。连接状态为本地演示。' },
    { id: 'filesystem', type: 'mcp', title: 'Filesystem', icon: 'folder', subtitle: '读取项目文件和目录', scope: '项目', detail: '演示 MCP 工具列表：list_directory、read_file。范围为当前项目目录。' },
    { id: 'browser', type: 'mcp', title: 'Browser', icon: 'globe', subtitle: '网页检查和浏览器验证', scope: '全局', detail: '演示 MCP 工具列表：navigate、snapshot、screenshot。' },
  ];
  const resourceLabels = { skills: '技能', plugins: '插件', mcp: 'MCP 服务' };
  let resourceType = 'skills';
  let resourceQuery = '';
  let selectedResource = 'code-review';
  let reviewWasOpen = reviewing;
  let inspectorTab = 'changes';
  let autoClosedChanges = false;
  let sourceIndex = 0;
  const unsavedSources = new Map();
  let pickerProject;
  let currentSourceText = '';
  let annotating = false;
  let preview = {id: '0', history: ['/'], index: 0};
  const browserTabs = new Map([[preview.id, preview]]);
  let nextBrowserId = 1;
  let settingsTab = 'general';
  let toastTimer;
  let currentPage = 'chat';

  function saveWorkbench() {
    try {
      localStorage.setItem('rpi-workbench-v1', JSON.stringify(workbench));
    } catch {
      toast('浏览器存储不可用，设置暂存于当前页面');
    }
  }

  function toast(message) {
    const node = $('#toast');
    clearTimeout(toastTimer);
    node.textContent = message;
    node.hidden = false;
    toastTimer = setTimeout(() => { node.hidden = true; }, 3000);
  }

  function revokeSessionApprovals() {
    for (const data of viewState.values()) data.sessionApproved = false;
  }

  function controls(label, action, value, className = 'action-button') {
    const button = actionButton(label, action, className);
    if (value !== undefined) button.dataset.value = value;
    return button;
  }

  async function copyText(text) {
    try {
      await navigator.clipboard.writeText(text);
      toast('已复制');
    } catch {
      toast('复制失败，请选择文字后手动复制');
    }
  }

  function pageHeading(title, description, action) {
    const header = element('header', 'page-heading');
    const words = element('div');
    words.append(element('h2', '', title), element('p', '', description));
    header.append(words);
    if (action) header.append(action);
    return header;
  }

  function setPage(value, focus = true) {
    if (!['chat', 'resources', 'automations'].includes(value)) return;
    if (currentPage === 'chat' && value !== 'chat') {
      readingState.capture();
      reviewWasOpen = reviewing;
      setPanel('inspector', false, false);
    }
    const returning = currentPage !== 'chat' && value === 'chat';
    currentPage = value;
    workspace.dataset.page = value;
    pageContent.hidden = value === 'chat';
    for (const button of document.querySelectorAll('[data-page]')) {
      button.setAttribute('aria-current', button.dataset.page === value ? 'page' : 'false');
    }
    for (const button of document.querySelectorAll('.chat-header-action, .review-toggle')) {
      button.hidden = value !== 'chat' || (button.dataset.action === 'thread-actions' && !session);
    }
    if (returning) setPanel('inspector', reviewWasOpen && !narrow.matches, false);
    if (mobile.matches) setPanel('navigation', false, false);
    if (value === 'chat') {
      renderHeader();
      if (returning) readingState.restore();
      if (focus) conversation.focus({ preventScroll: true });
    } else {
      $('#current-title').textContent = value === 'resources' ? '技能与插件' : '计划任务';
      $('#current-project').textContent = 'RPI Work';
      if (value === 'resources') renderResources();
      else renderAutomations();
      if (focus) pageContent.focus();
    }
  }

  function resourceInstalled(id) {
    return workbench.resources[resourceKey(id)] ?? ['code-review', 'test-runner', 'filesystem'].includes(id);
  }

  function resourceKey(id) {
    const resource = catalog.find(item => item.id === id);
    return resource?.scope === '项目' ? (session?.project ?? welcomeProject) + '/' + id : id;
  }

  function renderResources() {
    const content = document.createDocumentFragment();
    content.append(pageHeading('技能与插件', '为任务添加可复用的流程和工具。'));
    const toolbar = element('div', 'resource-toolbar');
    const tabs = element('div', 'segmented');
    tabs.setAttribute('role', 'tablist');
    tabs.setAttribute('aria-label', '资源类型');
    for (const [key, label] of Object.entries(resourceLabels)) {
      const tab = controls(label, 'resource-tab', key);
      tab.setAttribute('role', 'tab');
      tab.setAttribute('aria-selected', resourceType === key);
      tabs.append(tab);
    }
    const search = element('input', 'resource-search');
    search.type = 'search';
    search.placeholder = '搜索资源';
    search.setAttribute('aria-label', '搜索资源');
    search.value = resourceQuery;
    search.addEventListener('input', () => {
      resourceQuery = search.value;
      renderResourceList();
    });
    toolbar.append(tabs, search);
    content.append(toolbar);
    const layout = element('div', 'resource-layout');
    layout.append(element('div', 'resource-list'), element('aside', 'resource-detail'));
    content.append(layout);
    pageContent.replaceChildren(content);
    renderResourceList();
  }

  function renderResourceList() {
    const visible = catalog.filter(item => item.type === resourceType
      && (item.title + item.subtitle).toLowerCase().includes(resourceQuery.toLowerCase()));
    if (!visible.some(item => item.id === selectedResource)) selectedResource = visible[0]?.id;
    const list = $('.resource-list');
    const rows = visible.map(item => {
      const row = controls('', 'resource-select', item.id, 'resource-row');
      row.setAttribute('aria-pressed', item.id === selectedResource);
      const mark = element('span', 'resource-mark');
      mark.innerHTML = icon(item.icon);
      const words = element('span', 'resource-words');
      words.append(element('strong', '', item.title), element('small', '', item.subtitle));
      row.append(mark, words, element('span', 'resource-status', resourceInstalled(item.id) ? '已启用' : '可添加'));
      return row;
    });
    list.replaceChildren(...(rows.length ? rows : [element('p', 'empty-state', '没有匹配的资源。')]));
    const detail = $('.resource-detail');
    const item = catalog.find(item => item.id === selectedResource);
    if (!item) {
      detail.replaceChildren();
      return;
    }
    const mark = element('div', 'resource-mark large');
    mark.innerHTML = icon(item.icon);
    const active = resourceInstalled(item.id);
    const action = controls(active ? '停用' : (item.type === 'skills' ? '启用技能' : '模拟连接'), 'resource-toggle', item.id);
    detail.replaceChildren(
      mark, element('h3', '', item.title), element('p', '', item.detail),
      element('p', 'settings-note', '作用域 · ' + (item.scope === '项目' ? (session?.project ?? welcomeProject) : item.scope)), action,
    );
    if (item.type === 'skills') {
      const use = controls('用于新对话', 'use-skill', item.id, 'action-button primary');
      use.disabled = !active;
      detail.append(use);
    }
    if (item.type !== 'skills') detail.append(element('p', 'settings-note', '演示连接在本地保存。'));
  }

  function renderAutomations() {
    const header = pageHeading('计划任务', '保存任务和运行时间，在这里查看演示运行记录。', controls('新建任务', 'new-automation', undefined, 'action-button primary'));
    const list = element('div', 'automation-list');
    if (!workbench.automations.length) {
      const empty = element('div', 'automation-empty');
      empty.innerHTML = icon('clock');
      empty.append(element('h3', '', '把重复的工作交给计划任务'), element('p', '', '例如每日审阅变更、整理待办，或检查测试结果。'));
      const templates = element('div', 'automation-templates');
      for (const [name, prompt] of [
        ['每日变更审阅', '审阅最近的改动，汇总风险和缺少的测试。'],
        ['项目健康检查', '检查构建、测试和依赖状态，汇总需要处理的问题。'],
        ['整理开发待办', '整理当前项目的 TODO 和待处理问题，并按优先级排序。'],
      ]) {
        const template = controls('', 'automation-template', prompt, 'automation-template');
        template.append(element('strong', '', name), element('span', '', prompt));
        templates.append(template);
      }
      empty.append(templates);
      list.append(empty);
    }
    for (const item of workbench.automations) {
      const card = element('article', 'automation-card');
      const heading = element('div', 'automation-card-heading');
      const words = element('div');
      words.append(element('h3', '', item.name), element('p', '', item.project + ' · ' + item.frequency + ' ' + item.time));
      heading.append(words, element('span', 'automation-status', item.paused ? '已暂停' : '已配置'));
      const actions = element('div', 'thread-actions');
      const run = controls('运行演示', 'run-automation', item.id, 'action-button primary');
      actions.append(run, controls('编辑', 'edit-automation', item.id), controls(item.paused ? '启用' : '暂停', 'pause-automation', item.id), controls('删除', 'delete-automation', item.id));
      card.append(heading, element('p', 'automation-prompt', item.prompt), actions);
      if (item.runs.length) {
        const history = element('div', 'automation-history');
        history.append(element('h4', '', '最近运行'));
        for (const record of item.runs.slice(0, 3)) {
          const thread = sessions.find(thread => thread.id === record.threadId);
          const phase = thread && stateFor(thread.id).turns.at(-1)?.phase;
          const status = !phase ? '已创建' : phase === 'running' ? '执行中' : phase === 'completed' ? '已完成' : phaseLabels[phase];
          const open = controls('', 'open-automation-run', record.threadId, 'automation-run');
          open.append(element('span', '', record.time), element('span', '', status));
          open.disabled = !thread;
          history.append(open);
        }
        card.append(history);
      }
      list.append(card);
    }
    pageContent.replaceChildren(header, element('p', 'page-note', '本地演示 · 到时自动执行需要连接调度服务'), list);
  }

  function openAutomation(item, prompt = '') {
    $('#automation-title').textContent = item ? '编辑计划任务' : '新建计划任务';
    $('#automation-id').value = item?.id ?? '';
    $('#automation-name').value = item?.name ?? '';
    $('#automation-prompt').value = item?.prompt ?? prompt;
    $('#automation-project').replaceChildren(...projects.map(project => new Option(project.name, project.name)));
    $('#automation-project').value = item?.project ?? welcomeProject;
    $('#automation-frequency').value = item?.frequency ?? '每天';
    $('#automation-time').value = item?.time ?? '09:00';
    $('#automation-dialog').showModal();
    $('#automation-name').focus();
  }

  function runAutomation(item) {
    const thread = { id: crypto.randomUUID(), title: item.name, project: item.project, age: '刚刚', files: [], environment: 'worktree', base: 'main' };
    sessions.unshift(thread);
    showSession(thread);
    startTurn(thread, {
      id: crypto.randomUUID(), prompt: item.prompt, attachments: [], comments: [],
      model: state().model, effort: state().effort, scenario: 'normal',
    }, true);
    item.runs.unshift({ threadId: thread.id, time: new Intl.DateTimeFormat('zh-CN', { month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit', timeZone: 'Asia/Shanghai' }).format(new Date()) });
    saveHistory();
    saveWorkbench();
  }

  function environment() {
    const name = session?.project ?? welcomeProject;
    return workbench.environments[name] ?? { setup: 'npm ci', test: 'npm test' };
  }

  function setSettingsNavOpen(open) {
    settingsDialog.dataset.browse = String(open);
    $('.preference-navigation').setAttribute('aria-expanded', String(open));
  }

  function renderPreferenceSearch() {
    const query = $('#settings-search').value.trim().toLocaleLowerCase();
    const list = $('#settings-search-results');
    $('#settings-destinations').hidden = Boolean(query);
    list.hidden = !query;
    list.replaceChildren();
    if (!query) return;
    for (const section of settingsDialog.querySelectorAll('[data-settings-section]')) {
      const title = section.querySelector('h3').textContent;
      const key = section.dataset.settingsSection;
      const labels = [...section.querySelectorAll('.settings-row strong, .dialog-form label, dt')];
      const exactPane = title.toLocaleLowerCase().includes(query);
      const matches = labels.filter(label => label.textContent.toLocaleLowerCase().includes(query));
      for (const label of exactPane ? [null] : matches) {
        const button = element('button', 'preference-search-result');
        button.dataset.settingsTab = key;
        if (label?.closest('label')?.control) button.dataset.field = label.closest('label').control.id;
        const words = element('span', '', label?.textContent ?? title);
        if (label) words.append(element('small', '', title));
        button.append(words);
        list.append(button);
      }
    }
    if (!list.children.length) list.append(element('p', 'preference-empty', '未找到结果'));
  }

  function renderPreferenceArchives() {
    const archived = sessions.filter(thread => thread.archived);
    const list = $('#preference-archives');
    list.replaceChildren(...archived.map(thread => {
      const row = element('div', 'settings-row');
      const words = element('span');
      words.append(element('strong', '', thread.title), element('small', '', thread.project));
      row.append(words, controls('恢复', 'settings-restore-thread', thread.id));
      return row;
    }));
    if (!archived.length) list.append(element('p', 'preference-empty', '没有已归档的聊天。'));
  }

  function renderPreferenceResources() {
    for (const list of settingsDialog.querySelectorAll('[data-settings-resources]')) {
      const items = catalog.filter(item => item.type === list.dataset.settingsResources);
      list.replaceChildren(...items.map(item => {
        const row = element('label', 'settings-row');
        const words = element('span');
        words.append(element('strong', '', item.title), element('small', '', item.subtitle + ' · ' + item.scope));
        const toggle = document.createElement('input');
        toggle.type = 'checkbox';
        toggle.setAttribute('role', 'switch');
        toggle.setAttribute('aria-label', item.title);
        toggle.checked = resourceInstalled(item.id);
        toggle.dataset.preferenceResource = item.id;
        row.append(words, toggle);
        return row;
      }));
    }
  }

  $('#settings-search').addEventListener('input', renderPreferenceSearch);
  $('#settings-search').addEventListener('keydown', event => {
    if (!event.target.value.trim()) return;
    const first = $('#settings-search-results button');
    if (event.key === 'Enter') {
      event.preventDefault();
      first?.click();
    } else if (event.key === 'ArrowDown') {
      event.preventDefault();
      first?.focus();
    } else if (event.key === 'Escape') {
      event.preventDefault();
      event.target.value = '';
      renderPreferenceSearch();
    }
  });
  $('.preference-navigation').addEventListener('click', () => {
    const open = settingsDialog.dataset.browse !== 'true';
    setSettingsNavOpen(open);
    if (open) $('#settings-search').focus();
  });
  settingsDialog.addEventListener('close', () => {
    setSettingsNavOpen(false);
    $('#settings-search').value = '';
    renderPreferenceSearch();
  });
  settingsDialog.addEventListener('change', event => {
    const id = event.target.dataset.preferenceResource;
    if (!id) return;
    workbench.resources[resourceKey(id)] = event.target.checked;
    saveWorkbench();
    if (currentPage === 'resources') renderResourceList();
  });
  $('#follow-up-default').addEventListener('change', event => {
    workbench.followUpMode = event.target.value;
    if (!session) {
      state().busyMode = workbench.followUpMode;
      renderComposer();
    }
    saveWorkbench();
  });
  $('#show-bottom-control').addEventListener('change', event => {
    workbench.showBottomControl = event.target.checked;
    document.documentElement.dataset.bottomControl = String(workbench.showBottomControl);
    saveWorkbench();
  });
  $('#show-context-control').addEventListener('change', event => {
    workbench.showContext = event.target.checked;
    $('.context-button').hidden = !workbench.showContext;
    saveWorkbench();
  });

  function renderSettings() {
    for (const tab of document.querySelectorAll('[data-settings-tab]')) {
      tab.setAttribute('aria-current', tab.dataset.settingsTab === settingsTab ? 'page' : 'false');
    }
    for (const section of document.querySelectorAll('[data-settings-section]')) section.hidden = section.dataset.settingsSection !== settingsTab;
    $('#show-bottom-control').checked = workbench.showBottomControl;
    document.documentElement.dataset.bottomControl = String(workbench.showBottomControl);
    $('#general-review-delivery').value = workbench.reviewDelivery;
    $('#follow-up-default').value = workbench.followUpMode;
    $('#show-context-control').checked = workbench.showContext;
    $('.context-button').hidden = !workbench.showContext;
    renderPreferenceArchives();
    renderPreferenceResources();
    $('#font-size').value = workbench.fontSize;
    $('#permissions').value = workbench.permissions;
    $('#ask-commands').checked = workbench.askCommands;
    $('#review-delivery').value = workbench.reviewDelivery;
    $('#reading-density').value = workbench.density;
    $('#message-layout').value = workbench.messageLayout;
    $('#code-numbers').checked = workbench.codeNumbers;
    $('#notify-finish').checked = workbench.notifyFinish;
    $('#default-model').value = workbench.defaultModel;
    $('#default-effort').value = workbench.defaultEffort;
    $('#permissions-note').textContent = {
      readonly: '只读演示在执行命令前显示审批，审批仅影响本地演示状态。',
      workspace: '工作区内执行，工作区外操作由审批流程确认。',
      full: '演示完全访问的运行配置。',
    }[workbench.permissions];
    $('#setup-script').value = environment().setup;
    $('#test-command').value = environment().test;
    renderConnections();
  }

  function renderConnections() {
    const rows = [
      { id: 'local', title: '此电脑', detail: '本地工作区', icon: 'terminal' },
      { id: 'devbox', title: '开发主机', detail: 'SSH · devbox.local', icon: 'globe' },
    ].map(item => {
      const row = element('div', 'connection-row');
      const words = element('div');
      words.append(element('strong', '', item.title), element('small', '', item.detail));
      const connected = workbench.connections[item.id] ?? item.id === 'local';
      const button = controls(connected ? '模拟断开' : '模拟连接', 'connection-toggle', item.id);
      row.innerHTML = icon(item.icon);
      row.append(words, element('span', 'connection-status', connected ? '演示已连接' : '未连接'), button);
      return row;
    });
    $('#connection-list').replaceChildren(...rows);
  }

  function terminalState() {
    const key = session?.project ?? welcomeProject;
    if (!terminalSessions.has(key)) terminalSessions.set(key, { output: ['RPI Work · 终端演示', '支持 pwd、ls、git status、npm test。'], history: [], historyIndex: 0 });
    return terminalSessions.get(key);
  }

  function renderTerminal() {
    const project = session?.project ?? welcomeProject;
    $('#terminal-output').textContent = terminalState().output.join('\n');
    $('#terminal-prompt').textContent = project + ' %';
    $('#terminal-output').scrollTop = $('#terminal-output').scrollHeight;
  }

  function openTerminal(open, focus = true) {
    const follow = conversation.scrollHeight - conversation.scrollTop - conversation.clientHeight < 48;
    terminal.hidden = !open;
    workspace.dataset.terminal = open ? 'open' : 'closed';
    $('[data-action="terminal"]').setAttribute('aria-expanded', open);
    if (open) {
      renderTerminal();
      if (focus) $('#terminal-command').focus();
    } else if (focus) $('[data-action="terminal"]').focus();
    if (follow) conversation.scrollTop = conversation.scrollHeight;
  }

  function executeTerminal(command) {
    const data = terminalState();
    const project = projects.find(item => item.name === (session?.project ?? welcomeProject));
    const line = command.trim();
    if (!line) return;
    data.history.push(line);
    data.historyIndex = data.history.length;
    if (line === 'clear') {
      data.output = [];
      renderTerminal();
      return;
    }
    data.output.push(project.name + ' % ' + line);
    if (line === 'pwd') data.output.push(project.path ?? '~/Projects/' + project.name);
    else if (line === 'ls') data.output.push('src/  tests/  package.json  README.md');
    else if (line === 'git status') {
      const paths = session?.files.map(file => file.path) ?? [];
      data.output.push('On branch ' + (session?.base || project.branch));
      data.output.push(...(paths.length ? paths.map(path => (state().staged.includes(path) ? 'staged:   ' : 'modified: ') + path) : ['Working tree clean（演示）']));
    } else if (line === environment().test || line === 'npm test') {
      data.output.push('> prototype-tests（演示）', '✓ conversation layout', '✓ composer interaction', '2 passed · 0 failed');
    } else data.output.push('演示终端暂不支持此命令。输入 pwd、ls、git status 或 npm test。');
    renderTerminal();
  }

  const sourceProject = () => session?.project ?? welcomeProject;
  function activeFolder() {
    const folder = sourceFolders.get(sourceProject());
    return folder?.enabled ? folder : null;
  }

  function sourceButton(file, index) {
    const button = controls('', 'open-source', index, 'file-tree-row');
    button.innerHTML = icon('code-xml');
    button.append(element('code', '', file.name));
    button.title = file.path;
    return button;
  }

  function renderFiles() {
    const host = $('#files-content');
    const folder = activeFolder();
    const project = sourceProject();
    const tree = element('div', 'file-tree');
    const header = element('div', 'file-tree-header');
    header.append(element('h3', 'inspector-title', folder?.name ?? project), controls('打开文件夹', 'pick-folder', undefined, 'folder-action'));
    if (sourceFolders.has(project)) {
      header.append(controls(folder ? '示例文件' : '本地文件', 'switch-source', undefined, 'folder-action'));
    }
    tree.append(header);
    if (folder) {
      renderSourceDirectory(tree, folder, sourceButton);
    } else {
      const directories = new Map();
      for (const [index, file] of files.entries()) {
        const directory = file.path.slice(0, file.path.lastIndexOf('/'));
        if (!directories.has(directory)) {
          const details = element('details', 'file-folder');
          details.open = true;
          details.append(element('summary', '', directory));
          directories.set(directory, details);
        }
        directories.get(directory).append(sourceButton(file, index));
      }
      tree.append(...directories.values());
    }
    host.replaceChildren(tree, element('div', 'source-view'));
    openSource(folder ? folder.selected : sourceIndex);
  }

  $('#source-folder-picker').addEventListener('change', event => {
    const folder = buildSourceFolder(event.target.files);
    event.target.value = '';
    if (!folder) return;
    sourceFolders.set(pickerProject, folder);
    if (sourceProject() === pickerProject && inspectorTab === 'files') renderFiles();
  });

  function showFixtureSource(index) {
    const folder = activeFolder();
    if (folder) folder.enabled = false;
    sourceIndex = index;
    setInspectorTab('files');
    $('.source-file-tab[aria-selected="true"]').focus();
  }

  function sourceKey() {
    const folder = activeFolder();
    return folder ? folder.entries[folder.selected].path : sourceProject() + '/' + files[sourceIndex].path;
  }

  function highlightCode(text) {
    const fragment = document.createDocumentFragment();
    const pattern = /(\/\/[^\n]*|"(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*')|\b(function|return|const|let|export|import|from|class|if|else)\b|(<\/?[A-Za-z][A-Za-z0-9]*)/g;
    let start = 0;
    for (const match of text.matchAll(pattern)) {
      fragment.append(document.createTextNode(text.slice(start, match.index)));
      fragment.append(element('span', match[1] ? 'token-literal' : match[2] ? 'token-keyword' : 'token-tag', match[0]));
      start = match.index + match[0].length;
    }
    fragment.append(document.createTextNode(text.slice(start)));
    return fragment;
  }

  async function openSource(index) {
    const folder = activeFolder();
    const entries = folder?.entries ?? files;
    const file = entries[index];
    if (!file) return;
    let openTabs;
    if (folder) {
      folder.selected = index;
      if (!folder.opened.includes(index)) folder.opened.push(index);
      openTabs = folder.opened;
    } else {
      sourceIndex = index;
      openTabs = files.map((_, i) => i);
    }
    const source = $('.source-view');
    const tabs = element('div', 'source-tabs');
    tabs.setAttribute('role', 'tablist');
    tabs.setAttribute('aria-label', '已打开的代码文件');
    for (const i of openTabs) {
      const tab = controls(entries[i].name, 'open-source', i, 'source-file-tab');
      tab.setAttribute('role', 'tab');
      tab.title = entries[i].path;
      tab.setAttribute('aria-selected', i === index);
      tabs.append(tab);
    }
    const header = element('div', 'source-header');
    const actions = element('div', 'thread-actions');
    header.append(element('h3', '', file.path), actions);
    const status = element('div', 'source-status', folder ? '本地文件 · 只读' : '变更片段');
    const pending = element('p', 'source-message', '读取文件中…');
    source.replaceChildren(tabs, header, pending, status);
    for (const button of document.querySelectorAll('.file-tree [data-action="open-source"]')) button.setAttribute('aria-pressed', Number(button.dataset.value) === index);

    const key = sourceKey();
    const saved = folder ? folder.edits.get(key) : workbench.sourceDrafts[key];
    let text;
    if (typeof saved === 'string') text = saved;
    else if (folder) {
      try { text = await readLocalSource(file); }
      catch (error) { pending.textContent = error.message; return; }
    } else text = file.lines.filter(([kind]) => kind !== '-').map(([, line]) => line).join('\n');

    // A different file, project, or folder may have been opened during the read.
    if (!pending.isConnected) return;
    currentSourceText = text;
    actions.append(controls('复制', 'copy-source'), controls('编辑', 'edit-source'));
    const lines = text.split('\n');
    const code = element('pre', 'source-code');
    code.tabIndex = 0;
    code.setAttribute('aria-label', file.name + ' 源码');
    if (lines.length > 2500 || text.length > 160000) {
      code.classList.add('plain-text');
      code.textContent = text;
    } else {
      for (const [line, value] of lines.entries()) {
        const row = element('div', 'source-line');
        const content = element('code');
        if (value.length > 4000) content.textContent = value;
        else content.append(highlightCode(value));
        row.append(element('span', '', file.start + line), content);
        code.append(row);
      }
    }
    status.textContent = (typeof saved === 'string' ? (folder ? '本页草稿' : '浏览器草稿 · 已保存') : folder ? '本地文件 · 只读' : '变更片段') + ' · ' + lines.length + ' 行';
    pending.replaceWith(code);
    if ((folder?.pending ?? unsavedSources).has(key)) editSource(false);
  }

  function editSource(focus = true) {
    const editor = element('textarea', 'source-editor');
    editor.setAttribute('aria-label', '源码编辑器');
    editor.spellcheck = false;
    const key = sourceKey();
    const pending = activeFolder()?.pending ?? unsavedSources;
    editor.value = pending.get(key) ?? currentSourceText;
    editor.addEventListener('input', () => pending.set(key, editor.value));
    $('.source-code').replaceWith(editor);
    const actions = $('.source-header .thread-actions');
    actions.replaceChildren(controls('取消', 'cancel-source'), controls('保存草稿', 'save-source', undefined, 'action-button primary'));
    $('.source-status').textContent = activeFolder() ? '编辑本页草稿 · 原文件只读' : '编辑浏览器草稿';
    if (focus) editor.focus();
  }

  function refreshDockTabs() {
    for (const tab of document.querySelectorAll('[data-inspector-tab]')) {
      const active = tab.dataset.inspectorTab === inspectorTab &&
        (inspectorTab !== 'browser' || tab.dataset.browserId === preview.id);
      tab.setAttribute('aria-selected', String(active));
      tab.tabIndex = active ? 0 : -1;
      if (active) tab.scrollIntoView({block: 'nearest', inline: 'nearest'});
    }
    $('[data-action="close-browser-tab"]').hidden = inspectorTab !== 'browser' || browserTabs.size === 1;
  }

  function newBrowserTab() {
    preview = {id: String(nextBrowserId++), history: ['/'], index: 0};
    browserTabs.set(preview.id, preview);
    const tab = $('[data-browser-id]').cloneNode(true);
    tab.dataset.browserId = preview.id;
    tab.setAttribute('aria-label', '浏览器标签页 ' + (Number(preview.id) + 1));
    $('[data-inspector-tab="files"]').before(tab);
    setInspectorTab('browser');
    $('#browser-address').focus();
    $('#browser-address').select();
  }

  function closeBrowserTab() {
    if (browserTabs.size < 2) return;
    const ids = [...browserTabs.keys()];
    const index = ids.indexOf(preview.id);
    const next = ids[index === 0 ? 1 : index - 1];
    const tab = $('[data-browser-id="' + preview.id + '"]');
    browserTabs.delete(preview.id);
    tab.remove();
    preview = browserTabs.get(next);
    if (!browserTabs.has('0')) {
      const first = [...browserTabs.values()][0];
      const firstTab = $('[data-browser-id="' + first.id + '"]');
      firstTab.setAttribute('aria-label', '浏览器');
    }
    refreshDockTabs();
    renderBrowser();
    $('[data-browser-id="' + preview.id + '"]').focus();
  }

  function renderBrowser(refresh = true) {
    const route = preview.history[preview.index];
    const isSettings = route === '/settings';
    const found = route === '/' || isSettings;
    const live = /^https?:\/\//.test(route);
    const project = projects.find(item => item.name === (session?.project ?? welcomeProject));
    const branch = session?.base || project.branch;
    const address = live ? route : 'rpi://project' + route;
    const title = live ? new URL(route).host : route === '/' ? '项目概览' : isSettings ? '项目设置' : route;
    const context = JSON.stringify(live ? [preview.id, route] : [preview.id, route, project.name, branch]);
    const currentFrame = $('#browser-content iframe');
    if (refresh || currentFrame.dataset.context !== context) {
      const frame = document.createElement('iframe');
      frame.title = '页面预览';
      frame.dataset.context = context;
      frame.dataset.fixture = String(!live);
      frame.referrerPolicy = 'no-referrer';
      frame.setAttribute('sandbox', live ? 'allow-scripts allow-forms' : 'allow-scripts');
      if (live) frame.src = address;
      else frame.srcdoc = projectPreviewDocument(route, project.name, branch, files);
      currentFrame.replaceWith(frame);
    }
    const external = $('#open-address');
    external.hidden = !live;
    if (live) external.href = address;
    else external.removeAttribute('href');
    $('[data-action="annotate"]').disabled = live;
    const tab = $('[data-browser-id="' + preview.id + '"]');
    tab.querySelector('span').textContent = title;
    tab.title = address;
    $('#browser-address').value = address;
    $('#browser-address').setCustomValidity('');
    $('#browser-route').value = found ? route : '';
    $('[data-action="browser-back"]').disabled = preview.index === 0;
    $('[data-action="browser-forward"]').disabled = preview.index === preview.history.length - 1;
    $('.browser-status').textContent = live ? new URL(route).host : found ? '本地预览' : '页面未收录';
  }

  function setAnnotationMode(active) {
    annotating = active;
    $('[data-action="annotate"]').setAttribute('aria-pressed', String(active));
    const frame = $('#browser-content iframe');
    if (frame.dataset.fixture !== 'true') return;
    frame.contentWindow.postMessage({type: 'annotation-mode', active}, '*');
    $('.browser-status').textContent = active ? '选择要批注的元素' : '本地预览';
  }

  window.addEventListener('message', event => {
    const frame = $('#browser-content iframe');
    if (frame.dataset.fixture !== 'true' || event.source !== frame.contentWindow || event.data?.channel !== 'project-preview') return;
    const {type, value} = event.data;
    if (type === 'ready') {
      frame.contentWindow.postMessage({type: 'annotation-mode', active: annotating}, '*');
    } else if (inspectorTab === 'browser' && reviewing) {
      if (type === 'navigate' && ['/', '/settings'].includes(value)) navigateBrowser(value);
      if (type === 'file') {
        const index = files.findIndex(file => file.path === value);
        if (index !== -1) {
          showFixtureSource(index);
        }
      }
      if (type === 'annotate' && annotating && typeof value === 'string' && value.length <= 80) {
        setAnnotationMode(false);
        openComment('页面 ' + preview.history[preview.index] + ' · ' + value, '页面批注');
      }
    }
  });

  function navigateBrowser(route) {
    if (annotating) setAnnotationMode(false);
    preview.history.splice(preview.index + 1);
    preview.history.push(route);
    preview.index = preview.history.length - 1;
    renderBrowser();
  }

  function setInspectorTab(value) {
    if (!['changes', 'files', 'browser'].includes(value)) return;
    if (value !== 'browser' && annotating) setAnnotationMode(false);
    inspectorTab = value;
    shell.dataset.tool = value;
    $('#review-content').hidden = value !== 'changes';
    $('#files-content').hidden = value !== 'files';
    $('#browser-content').hidden = value !== 'browser';
    $('.inspector-header h2').textContent = { changes: '代码审阅', files: '代码', browser: '浏览器' }[value];
    refreshDockTabs();
    if (value === 'files') renderFiles();
    if (value === 'browser') renderBrowser(false);
  }

  $('#project-form').addEventListener('submit', event => {
    event.preventDefault();
    const name = $('#project-name').value.trim();
    const path = $('#project-path').value.trim();
    $('#project-name').setCustomValidity('');
    if (!name || !path) return;
    if (projects.some(project => project.name === name)) {
      $('#project-name').setCustomValidity('项目名称已存在');
      $('#project-name').reportValidity();
      return;
    }
    const project = { name, path, branch: 'main' };
    projects.push(project);
    workbench.projects.push(project);
    saveWorkbench();
    $('#project-dialog').close();
    welcomeProject = name;
    showSession(null);
    toast('已添加演示项目');
    draft.focus();
  });
  $('#project-name').addEventListener('input', () => $('#project-name').setCustomValidity(''));

  $('#automation-form').addEventListener('submit', event => {
    event.preventDefault();
    const values = {
      name: $('#automation-name').value.trim(), prompt: $('#automation-prompt').value.trim(),
      project: $('#automation-project').value, frequency: $('#automation-frequency').value,
      time: $('#automation-time').value,
    };
    if (!values.name || !values.prompt) return;
    const item = workbench.automations.find(item => item.id === $('#automation-id').value);
    if (item) Object.assign(item, values);
    else workbench.automations.push({ id: crypto.randomUUID(), ...values, paused: false, runs: [] });
    saveWorkbench();
    $('#automation-dialog').close();
    renderAutomations();
    pageContent.focus();
  });

  $('#environment-form').addEventListener('input', () => { $('#environment-feedback').textContent = ''; });
  settingsDialog.addEventListener('toggle', event => {
    if (event.newState === 'open') {
      renderSettings();
      $('#environment-feedback').textContent = '';
    }
  });
  $('#environment-form').addEventListener('submit', event => {
    event.preventDefault();
    const test = $('#test-command').value.trim();
    if (!test) return;
    workbench.environments[session?.project ?? welcomeProject] = { setup: $('#setup-script').value, test };
    saveWorkbench();
    $('#environment-feedback').textContent = '已保存环境配置';
  });
  function withReadingAnchor(change) {
    const atBottom = conversation.scrollHeight - conversation.clientHeight - conversation.scrollTop < 40;
    const viewport = conversation.getBoundingClientRect();
    const anchor = [...$('#transcript').querySelectorAll('.prompt, .assistant > p, .reply h2, .reply p, .reply > ul, .message-code, .activity > summary, .turn-response')]
      .find(node => { const rect = node.getBoundingClientRect(); return rect.bottom > viewport.top + 8 && rect.top < viewport.bottom; });
    const top = anchor?.getBoundingClientRect().top;
    change();
    if (atBottom) conversation.scrollTop = conversation.scrollHeight;
    else if (anchor) conversation.scrollTop += anchor.getBoundingClientRect().top - top;
  }
  $('#font-size').addEventListener('change', event => {
    withReadingAnchor(() => {
      workbench.fontSize = event.target.value;
      document.documentElement.style.setProperty('--message-size', workbench.fontSize + 'px');
    });
    saveWorkbench();
  });
  for (const select of [$('#review-delivery'), $('#general-review-delivery')]) {
    select.addEventListener('change', event => {
      workbench.reviewDelivery = event.target.value;
      $('#review-delivery').value = workbench.reviewDelivery;
      $('#general-review-delivery').value = workbench.reviewDelivery;
      saveWorkbench();
    });
  }
  $('#message-layout').addEventListener('change', event => {
    withReadingAnchor(() => {
      workbench.messageLayout = event.target.value;
      document.documentElement.dataset.messageLayout = workbench.messageLayout;
    });
    saveWorkbench();
  });
  $('#reading-density').addEventListener('change', event => {
    withReadingAnchor(() => {
      workbench.density = event.target.value;
      document.documentElement.dataset.density = workbench.density;
    });
    saveWorkbench();
  });
  $('#code-numbers').addEventListener('change', event => {
    workbench.codeNumbers = event.target.checked;
    document.documentElement.dataset.codeNumbers = String(workbench.codeNumbers);
    saveWorkbench();
  });
  $('#notify-finish').addEventListener('change', event => {
    workbench.notifyFinish = event.target.checked;
    saveWorkbench();
  });
  $('#default-model').addEventListener('change', event => {
    workbench.defaultModel = event.target.value;
    if (!session) {
      state().model = workbench.defaultModel;
      renderComposer();
    }
    saveWorkbench();
  });
  $('#default-effort').addEventListener('change', event => {
    workbench.defaultEffort = event.target.value;
    if (!session) {
      state().effort = workbench.defaultEffort;
      renderComposer();
    }
    saveWorkbench();
  });
  $('#permissions').addEventListener('change', event => {
    workbench.permissions = event.target.value;
    revokeSessionApprovals();
    saveWorkbench();
    renderSettings();
  });
  $('#ask-commands').addEventListener('change', event => {
    workbench.askCommands = event.target.checked;
    revokeSessionApprovals();
    saveWorkbench();
  });
  $('#terminal-form').addEventListener('submit', event => {
    event.preventDefault();
    executeTerminal($('#terminal-command').value);
    $('#terminal-command').value = '';
  });
  $('#terminal-command').addEventListener('keydown', event => {
    if (!['ArrowUp', 'ArrowDown'].includes(event.key)) return;
    event.preventDefault();
    const data = terminalState();
    data.historyIndex += event.key === 'ArrowUp' ? -1 : 1;
    data.historyIndex = Math.max(0, Math.min(data.historyIndex, data.history.length));
    event.target.value = data.history[data.historyIndex] ?? '';
  });
  $('#browser-form').addEventListener('submit', event => {
    event.preventDefault();
    const address = $('#browser-address');
    try {
      let value = address.value.trim();
      if (!value) throw new Error('empty address');
      if (/^(?:localhost|127\.0\.0\.1|\[::1\])(?=[:/]|$)/i.test(value)) value = 'http://' + value;
      else if (/^[\w-]+(?:\.[\w-]+)+(?=[:/]|$)/i.test(value)) value = 'https://' + value;
      const current = preview.history[preview.index];
      const base = /^https?:/.test(current) ? current : 'rpi://project' + current;
      const target = new URL(value, base);
      if (target.protocol === 'rpi:' && target.host === 'project') navigateBrowser(target.pathname);
      else if (['http:', 'https:'].includes(target.protocol) && !target.username && !target.password) navigateBrowser(target.href);
      else throw new Error('unsupported address');
    } catch {
      address.setCustomValidity('请输入 HTTP、HTTPS 或 rpi://project/ 地址');
      address.reportValidity();
    }
  });
  $('#browser-address').addEventListener('input', event => event.target.setCustomValidity(''));
  $('#browser-route').addEventListener('change', event => navigateBrowser(event.target.value));
  $('#browser-viewport').addEventListener('change', event => {
    $('.browser-stage').dataset.viewport = event.target.value;
  });

  $('#transcript').addEventListener('change', event => {
    if (event.target.id === 'project-picker') renderBrowser(false);
  });
  document.addEventListener('sessionchange', () => {
    setPage('chat', false);
    // Empty changes should not consume half of a new conversation. Preserve
    // browser/source workspaces, and restore changes when returning to a thread
    // with files unless the user explicitly chose another panel state.
    if (!session && inspectorTab === 'changes' && reviewing) {
      autoClosedChanges = true;
      setPanel('inspector', false, false);
    } else if (session && autoClosedChanges) {
      autoClosedChanges = false;
      if (session.files?.length && inspectorTab === 'changes' && !narrow.matches) setPanel('inspector', true, false);
    }
    if (inspectorTab === 'files') renderFiles();
    setAnnotationMode(false);
    renderBrowser(false);
    renderTerminal();
  });
  document.addEventListener('turnchange', () => {
    if (currentPage === 'automations') renderAutomations();
    if (workbench.notifyFinish && session) toast(phaseLabels[state().turns.at(-1)?.phase] ?? '任务状态已更新');
  });
  document.addEventListener('click', event => {
    const button = event.target.closest('button');
    if (!button) return;
    if (button.dataset.inspectorTab || button.dataset.file !== undefined || ['inspector', 'close-inspector', 'browser', 'code-panel', 'new-browser-tab'].includes(button.dataset.action)) autoClosedChanges = false;
    if (button.dataset.page) setPage(button.dataset.page);
    if (button.dataset.inspectorTab) {
      const changed = button.dataset.browserId !== undefined && preview.id !== button.dataset.browserId;
      if (changed) {
        setAnnotationMode(false);
        preview = browserTabs.get(button.dataset.browserId);
      }
      setInspectorTab(button.dataset.inspectorTab);
    }
    if (button.dataset.settingsTab) {
      settingsTab = button.dataset.settingsTab;
      $('#settings-search').value = '';
      renderPreferenceSearch();
      renderSettings();
      setSettingsNavOpen(false);
      $('.settings-content').scrollTop = 0;
      if (button.dataset.field) {
        document.getElementById(button.dataset.field)?.focus();
      } else if (!button.isConnected || !button.offsetWidth) {
        const title = $('[data-settings-section="' + settingsTab + '"] h3');
        title.tabIndex = -1;
        title.focus();
      }
    }
    const value = button.dataset.value;
    switch (button.dataset.action) {
      case 'copy-code': copyText(session.code); break;
      case 'copy-reply': {
        const answer = button.closest('.assistant, .demo-turn').querySelector('.reply, .turn-response');
        // Copy answer content, not code-header buttons or workspace widgets.
        const sections = answer.matches('.reply')
          ? [...answer.children].filter(node => node.matches('h2, p, ul, .message-code')) : [answer];
        copyText(sections.map(node => node.matches('.message-code') ? $('pre', node).innerText : node.innerText).join('\n\n'));
        break;
      }
      case 'settings': renderSettings(); break;
      case 'settings-restore-thread': {
        const thread = sessions.find(item => item.id === value && item.archived);
        if (!thread) break;
        thread.archived = false;
        renderNavigation();
        saveHistory();
        renderPreferenceArchives();
        break;
      }
      case 'show-context': {
        const data = state();
        const fields = [
          ['项目', session?.project ?? welcomeProject],
          ['执行位置', session?.environment === 'worktree' ? 'Worktree' : '本地'],
          ['模型', data.model + ' · 推理强度 ' + data.effort],
          ['消息轮次', String(data.turns.length + (session?.prompt ? 1 : 0) + (session?.followup ? 1 : 0))],
          ['后续消息', String(data.queue.length)],
          ['权限', { readonly: '只读', workspace: '工作区可写', full: '完全访问' }[workbench.permissions]],
        ];
        $('#context-details').replaceChildren(...fields.map(([label, value]) => {
          const row = element('div');
          row.append(element('dt', '', label), element('dd', '', value));
          return row;
        }));
        $('#context-dialog').showModal();
        break;
      }
      case 'inspector': setInspectorTab('changes'); break;
      case 'browser':
      case 'code-panel':
        setPage('chat', false);
        setPanel('inspector', true, false);
        setInspectorTab(button.dataset.action === 'browser' ? 'browser' : 'files');
        $('.inspector-tabs [aria-selected="true"]').focus();
        break;
      case 'browser-back':
        if (preview.index > 0) {
          preview.index--;
          renderBrowser();
        }
        break;
      case 'browser-forward':
        if (preview.index < preview.history.length - 1) {
          preview.index++;
          renderBrowser();
        }
        break;
      case 'browser-reload': renderBrowser(); break;
      case 'new-browser-tab': setAnnotationMode(false); newBrowserTab(); break;
      case 'close-browser-tab': setAnnotationMode(false); closeBrowserTab(); break;
      case 'annotate': setAnnotationMode(!annotating); break;
      case 'diff-source':
        showFixtureSource(Number(value));
        break;
      case 'add-project':
        $('#project-form').reset();
        $('#project-dialog').showModal();
        break;
      case 'terminal': openTerminal(terminal.hidden); break;
      case 'close-terminal': openTerminal(false); break;
      case 'clear-terminal':
        terminalState().output = [];
        renderTerminal();
        $('#terminal-command').focus();
        break;
      case 'resource-tab':
        resourceType = value;
        resourceQuery = '';
        renderResources();
        $('[data-action="resource-tab"][data-value="' + value + '"]').focus();
        break;
      case 'resource-select':
        selectedResource = value;
        renderResourceList();
        $('[data-action="resource-select"][data-value="' + value + '"]').focus();
        break;
      case 'resource-toggle':
        workbench.resources[resourceKey(value)] = !resourceInstalled(value);
        saveWorkbench();
        renderResourceList();
        $('[data-action="resource-toggle"]').focus();
        break;
      case 'use-skill': {
        const item = catalog.find(item => item.id === value);
        if (!item || !resourceInstalled(value)) break;
        showSession(null);
        draft.value = '$' + item.id + ' ' + item.prompt;
        draft.dispatchEvent(new Event('input'));
        draft.focus();
        break;
      }
      case 'new-automation': openAutomation(); break;
      case 'automation-template':
        openAutomation(null, value);
        $('#automation-name').value = button.querySelector('strong').textContent;
        break;
      case 'edit-automation': openAutomation(workbench.automations.find(item => item.id === value)); break;
      case 'run-automation': runAutomation(workbench.automations.find(item => item.id === value)); break;
      case 'pause-automation': {
        const item = workbench.automations.find(item => item.id === value);
        item.paused = !item.paused;
        saveWorkbench();
        renderAutomations();
        $('[data-action="pause-automation"][data-value="' + value + '"]').focus();
        break;
      }
      case 'delete-automation':
        workbench.automations = workbench.automations.filter(item => item.id !== value);
        saveWorkbench();
        renderAutomations();
        pageContent.focus();
        break;
      case 'open-automation-run': {
        const thread = sessions.find(item => item.id === value);
        if (thread) showSession(thread);
        break;
      }
      case 'connection-toggle':
        workbench.connections[value] = !(workbench.connections[value] ?? value === 'local');
        saveWorkbench();
        renderConnections();
        $('[data-action="connection-toggle"][data-value="' + value + '"]').focus();
        break;
      case 'open-source':
        openSource(Number(value));
        $('.source-file-tab[aria-selected="true"]').focus();
        break;
      case 'pick-folder':
        pickerProject = sourceProject();
        $('#source-folder-picker').click();
        break;
      case 'switch-source': {
        const folder = sourceFolders.get(sourceProject());
        if (folder) { folder.enabled = !folder.enabled; renderFiles(); }
        break;
      }
      case 'edit-source': editSource(); break;
      case 'save-source':
      case 'cancel-source': {
        const folder = activeFolder();
        const key = sourceKey();
        (folder?.pending ?? unsavedSources).delete(key);
        if (button.dataset.action === 'save-source') {
          const text = $('.source-editor').value;
          if (folder) folder.edits.set(key, text);
          else { workbench.sourceDrafts[key] = text; saveWorkbench(); }
        }
        const project = sourceProject();
        openSource(folder?.selected ?? sourceIndex).then(() => {
          if (project === sourceProject() && !$('#files-content').hidden) $('[data-action="edit-source"]')?.focus();
        });
        break;
      }
      case 'copy-source': copyText(currentSourceText); break;
    }
    if (button.dataset.file !== undefined) setInspectorTab('changes');
  });
  document.addEventListener('keydown', event => {
    if (event.isComposing || $('dialog[open]')) return;
    if ((event.metaKey || event.ctrlKey) && event.shiftKey && event.code === 'KeyB') autoClosedChanges = false;
    const tab = event.target.closest('[role="tab"]');
    if (tab && ['ArrowLeft', 'ArrowRight'].includes(event.key)) {
      event.preventDefault();
      const tabs = [...tab.parentElement.querySelectorAll('[role="tab"]')];
      const next = (tabs.indexOf(tab) + (event.key === 'ArrowLeft' ? -1 : 1) + tabs.length) % tabs.length;
      tabs[next].click();
      tabs[next].focus();
    }
    if ((event.metaKey || event.ctrlKey) && event.code === 'Backquote') {
      event.preventDefault();
      setPage('chat', false);
      openTerminal(terminal.hidden);
    }
  });
  document.documentElement.style.setProperty('--message-size', workbench.fontSize + 'px');
  document.documentElement.dataset.density = workbench.density;
  document.documentElement.dataset.messageLayout = workbench.messageLayout;
  document.documentElement.dataset.codeNumbers = String(workbench.codeNumbers);
  renderSettings();
  setPage('chat', false);
  const entry = new URLSearchParams(location.search);
  const panels = { browser: 'browser', code: 'files', changes: 'changes' };
  const panel = panels[entry.get('panel')];
  setInspectorTab(typeof panel === 'string' ? panel : 'browser');
  if (typeof panel === 'string') setPanel('inspector', true, false);
  if (entry.get('view') === 'settings') settingsDialog.showModal();
  else if (entry.get('view') === 'new') showSession(null);
  initializeInspector();
  scrollToLatestTurn();
}
