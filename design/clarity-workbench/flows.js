'use strict';

// Standalone interaction prototype. All state is in this tab's memory.
// Preparation, installation, execution and publishing advance by explicit clicks.
(() => {
  const $ = selector => document.querySelector(selector);
  const escape = value => String(value).replace(/[&<>"']/g, char => ({
    '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
  }[char]));
  const labels = { all: '资源中心', extensions: '扩展', skills: '技能库', packages: 'Packages' };
  const typeLabel = { extensions: '扩展', skills: '技能', packages: 'Package' };
  const typeIcon = { extensions: 'puzzle', skills: 'skills', packages: 'blocks' };
  const sourceCode = { 公共库: 'public', 已安装: 'installed', 本地设计: 'design' };
  const codeSource = { public: '公共库', installed: '已安装', design: '本地设计' };
  const keyFor = item => item.key || `${item.kind}/${item.id}`;
  const projectKey = (node, project) => JSON.stringify([node, project]);
  const state = {
    selection: new Set(),
    catalog: Object.entries(resources).flatMap(([kind, items]) => items.map(item => ({
      ...item, kind, version: '1.0.0', source: '原型示例目录',
    }))),
    installed: new Map(),
    applied: new Map(),
    defaults: new Map(),
    runs: new Map(),
    extraProjects: [],
    libraryNode: 'local',
    libraryProject: 'rpi-work',
    installedNode: 'local',
    installedProject: '*',
    origin: null,
    applyToCurrent: false,
    detail: null,
    plan: null,
    providerReady: new Set(['local', 'studio']),
    designs: [{
      id: 'project-conventions', name: '项目约定草稿', kind: 'skills', version: '0.1.0',
      content: '# 项目约定\n\n先阅读 README，修改后检查相关测试。', revision: 1,
    }],
    authorRevision: 1,
    savedRevision: 0,
    validRevision: 0,
    testedRevision: 0,
    publishedKey: null,
    onboardingStep: 0,
    selectedBackend: new Map(),
  };
  state.catalog.push({
    kind: 'skills', id: 'code-review', name: '代码审查',
    description: '检查代码变更的正确性、边界条件和可维护性，给出具体问题与验证建议。',
    icon: 'shield-check', tags: ['代码质量', '变更审阅'], version: '1.0.0', source: '原型示例目录',
  });
  state.installed.set(projectKey('local', 'rpi-work'), new Set([
    'packages/developer', 'packages/base', 'skills/rust-router', 'extensions/codegraph', 'extensions/todo',
  ]));
  state.installed.set(projectKey('local', '*'), new Set(['packages/base', 'extensions/todo']));
  state.applied.set(scopeKey('local', 'design'), ['packages/developer', 'skills/rust-router', 'extensions/codegraph']);

  function itemFor(key) { return state.catalog.find(item => keyFor(item) === key); }
  function nodeName(id) { return demoNodes.find(node => node.id === id)?.name || id; }
  function currentSessionRecord() {
    return demoNodes.find(node => node.id === currentNode)?.sessions.find(session => session.id === currentSession);
  }
  function currentProject() { return homeMode ? homeState.project : currentSessionRecord()?.project || 'rpi-work'; }
  function currentNodeId() { return homeMode ? homeState.node : currentNode; }
  function installed(key, node = state.libraryNode, project = state.libraryProject) {
    return state.installed.get(projectKey(node, project))?.has(key)
      || state.installed.get(projectKey(node, '*'))?.has(key) || false;
  }
  function installationLabel(key, node, project) {
    const global = state.installed.get(projectKey(node, '*'))?.has(key);
    const local = project !== '*' && state.installed.get(projectKey(node, project))?.has(key);
    if (global && local) return '项目安装 · 全局也可用';
    if (local) return '项目安装';
    if (global) return project === '*' ? '全局安装' : '全局可用';
    return '尚未安装';
  }
  function resourceContext() {
    return resourceSource === '已安装'
      ? { node: state.installedNode, project: state.installedProject }
      : { node: state.libraryNode, project: state.libraryProject };
  }
  function projectOptions() {
    const result = [];
    for (const node of demoNodes) {
      const names = new Set([
        ...node.sessions.map(session => session.project),
        ...state.extraProjects.filter(project => project.node === node.id).map(project => project.name),
      ]);
      for (const name of names) result.push({ node: node.id, name, key: projectKey(node.id, name) });
    }
    return result;
  }
  function createDemoSession(nodeId, project, title, tools = [], model = 'DeepSeek V4 Pro', draft = '') {
    const node = demoNodes.find(item => item.id === nodeId);
    const id = `preview-${++previewSequence}`;
    node.sessions.unshift({ id, title, project, recent: -previewSequence, state: 'idle' });
    const key = scopeKey(nodeId, id);
    previewMessages.set(key, []);
    sessionDrafts.set(key, draft);
    sessionPreferences.set(key, { model, thinking: '高' });
    state.applied.set(key, [...tools]);
    closeDialogs();
    selectSession(nodeId, id);
    return key;
  }
  function closeDialogs() { document.querySelectorAll('dialog[open]').forEach(dialog => dialog.close()); }
  function showDialog(id, opener = document.activeElement) {
    closeDialogs();
    const dialog = $(id);
    dialog._opener = opener;
    dialog.showModal();
    dialog.scrollTop = 0;
    mountIcons(dialog);
  }
  function genericDialog(title, body, actions = '') {
    $('#flow-dialog-title').textContent = title;
    $('#flow-dialog-body').innerHTML = body;
    $('#flow-dialog-actions').innerHTML = actions;
    showDialog('#flow-dialog');
  }
  function button(action, title, extra = '', primary = false) {
    return `<button class="button${primary ? ' primary' : ''}" data-flow="${action}" ${extra}>${title}</button>`;
  }
  function prototypeNote() {
    return '<p class="flow-prototype">交互原型 · 示例数据与状态仅保存在当前页面，刷新后重置。</p>';
  }
  document.body.insertAdjacentHTML('beforeend', `
    <dialog id="flow-dialog" class="flow-dialog" aria-labelledby="flow-dialog-title">
      <div class="dialog-header"><h2 id="flow-dialog-title"></h2><button class="icon-button" data-close aria-label="关闭流程窗口"><i data-icon="close"></i></button></div>
      <div id="flow-dialog-body"></div><div id="flow-dialog-actions" class="flow-actions"></div>${prototypeNote()}
    </dialog>
    <dialog id="work-dialog" class="flow-dialog wide-dialog" aria-labelledby="work-dialog-title">
      <div class="dialog-header"><h2 id="work-dialog-title">开始工作</h2><button class="icon-button" data-flow="close-work" aria-label="关闭工作准备"><i data-icon="close"></i></button></div>
      <div id="work-dialog-body"></div><div id="work-dialog-actions" class="flow-actions"></div>${prototypeNote()}
    </dialog>
    <dialog id="selection-dialog" class="flow-dialog" aria-labelledby="selection-dialog-title">
      <div class="dialog-header"><h2 id="selection-dialog-title">已选工具</h2><button class="icon-button" data-close aria-label="关闭已选工具"><i data-icon="close"></i></button></div>
      <div id="selection-dialog-list" class="selected-list"></div><p class="selection-hint">一个主 Package，可搭配多个技能与扩展。</p>
      <div class="flow-actions">${button('clear-selection', '清空')}${button('project-defaults', '设为项目默认')}${button('checkout', '开始工作', '', true)}</div>${prototypeNote()}
    </dialog>`);
  const detailDialog = $('#detail-dialog');
  detailDialog.className = 'flow-dialog';
  detailDialog.innerHTML = `
    <div class="dialog-header"><h2 id="detail-resource-name">工具详情</h2><button class="icon-button" data-close aria-label="关闭资源详情"><i data-icon="close"></i></button></div>
    <p id="detail-resource-description" class="flow-lead"></p>
    <dl id="detail-resource-facts" class="flow-facts"></dl><div id="detail-resource-content" class="flow-box"></div>
    <div id="detail-resource-actions" class="flow-actions"></div>${prototypeNote()}`;
  document.querySelectorAll('.flow-dialog').forEach(dialog => {
    dialog.addEventListener('close', () => {
      if (dialog._opener?.isConnected && !dialog._opener.closest('[hidden]')) dialog._opener.focus({ preventScroll: true });
    });
  });
  $('#work-dialog').addEventListener('cancel', event => {
    if (state.plan?.mode === 'install' && state.plan.resumePlan) {
      event.preventDefault();
      cancelInstallation();
    }
  });

  function syncRoute() {
    const params = new URLSearchParams({ kind: resourceKind, source: sourceCode[resourceSource] });
    if (resourceSource === '已安装') {
      params.set('node', state.installedNode);
      params.set('project', state.installedProject);
    }
    history.replaceState(null, '', `#resources?${params}`);
  }
  function syncResourceNavigation() {
    for (const entry of document.querySelectorAll('[data-resource-entry]')) {
      const active = document.body.dataset.view === 'resources'
        && (entry.closest('.sidebar') !== null || entry.dataset.resourceEntry === resourceKind);
      entry.classList.toggle('active', active);
      if (entry.closest('.sidebar')) {
        if (active) entry.setAttribute('aria-current', 'page');
        else entry.removeAttribute('aria-current');
      }
    }
  }
  function openResources(kind = 'all', adjust = false) {
    if (document.body.dataset.view === 'workspace') {
      rememberDraft();
      state.origin = { node: currentNodeId(), session: currentSession, home: homeMode, project: currentProject() };
      state.libraryNode = currentNodeId();
      state.libraryProject = currentProject();
      state.applyToCurrent = adjust;
    }
    if (adjust) {
      const selected = homeMode
        ? state.defaults.get(projectKey(homeState.node, homeState.project)) || []
        : state.applied.get(scopeKey(currentNode, currentSession)) || [];
      state.selection = new Set(selected);
    }
    resourceKind = labels[kind] ? kind : 'all';
    showView('resources');
    renderResources();
    syncRoute();
  }
  function returnToOrigin() {
    const origin = state.origin;
    if (origin?.home) openHome();
    else if (origin) selectSession(origin.node, origin.session);
    else showView('workspace');
  }
  function selectionMarkup() {
    if (!state.selection.size) return '<p class="selection-empty">选择需要的扩展、技能或 Package，它们会保留在这里。</p>';
    return [...state.selection].map(key => {
      const item = itemFor(key);
      return `<div class="selected-tool"><i data-icon="${typeIcon[item.kind]}"></i><div><strong>${escape(item.name)}</strong><small>${typeLabel[item.kind]} · v${escape(item.version)}<br>${escape(state.libraryProject)} · ${installed(key) ? '可用 · 演示' : '开始工作时准备'}</small></div><button class="icon-button" data-flow="remove-selection" data-key="${escape(key)}" aria-label="移除${escape(item.name)}"><i data-icon="close"></i></button></div>`;
    }).join('');
  }
  function renderSelection() {
    const markup = selectionMarkup();
    $('#selected-list').innerHTML = markup;
    $('#selection-dialog-list').innerHTML = markup;
    document.querySelectorAll('[data-selection-count]').forEach(element => element.textContent = state.selection.size);
    document.querySelectorAll('[data-flow=checkout],[data-flow=project-defaults],[data-flow=clear-selection]').forEach(element => element.disabled = !state.selection.size);
    $('.selection-dock').hidden = !state.selection.size;
    $('#library-return').innerHTML = state.origin
      ? `<span>${escape(nodeName(state.origin.node))} · ${escape(state.origin.project)}</span><button class="text-button" data-flow="return-origin"><i data-icon="back"></i>${state.origin.home ? '返回新对话' : '返回原会话'}</button>` : '';
    mountIcons($('.resource-workbench'));
    mountIcons($('#selection-dialog'));
  }
  function renderResources() {
    if (!$('#resource-grid')) return;
    const query = $('#resource-search').value.trim().toLowerCase();
    const designView = resourceSource === '本地设计';
    const context = resourceContext();
    const catalog = designView ? [] : state.catalog.filter(item => {
      if (resourceSource === '公共库' && item.localOnly) return false;
      return resourceSource !== '已安装' || installed(keyFor(item), context.node, context.project);
    });
    const list = catalog.filter(item => (resourceKind === 'all' || item.kind === resourceKind)
      && `${item.name} ${item.id} ${item.description} ${item.source}`.toLowerCase().includes(query));
    $('#resource-page-title').textContent = '资源中心';
    $('#resource-page-subtitle').textContent = resourceKind === 'all'
      ? '浏览扩展、技能与 Packages，组合后开始工作。'
      : `选择需要的${typeLabel[resourceKind]}，也可跨分类搭配其他工具。`;
    $('#library-node').textContent = nodeName(state.libraryNode);
    $('#library-project').textContent = state.libraryProject;
    $('.installed-scope').hidden = resourceSource !== '已安装';
    const scopes = demoNodes.flatMap(node => [
      { key: projectKey(node.id, '*'), label: `${node.name} · 全局` },
      ...projectOptions().filter(project => project.node === node.id)
        .map(project => ({ key: project.key, label: `${node.name} · ${project.name}（项目可用）` })),
    ]);
    $('#installed-scope').innerHTML = scopes.map(scope => `<option value="${escape(scope.key)}" ${scope.key === projectKey(state.installedNode, state.installedProject) ? 'selected' : ''}>${escape(scope.label)}</option>`).join('');
    const total = designView ? state.designs.length + state.catalog.filter(item => item.localOnly).length : catalog.length;
    $('#resource-count-dimension').textContent = resourceSource === '公共库'
      ? `公共库 · ${total} 个资源版本`
      : designView ? `本地设计 · ${state.designs.length} 个草稿 · ${total - state.designs.length} 个发布版本`
        : `${nodeName(context.node)} · ${context.project === '*' ? '全局安装' : `${context.project} 项目可用`} · ${total} 个资源版本`;
    $('#resource-count-rule').textContent = resourceSource === '公共库'
      ? '按类型、资源 ID 与版本统计目录条目。'
      : designView ? '草稿与发布版本分别计数。'
        : context.project === '*' ? '仅统计此节点的全局安装。'
          : '包含项目安装与全局可用资源，同一版本合并计数。';
    document.querySelectorAll('[data-kind]').forEach(element => {
      const active = element.dataset.kind === resourceKind;
      element.classList.toggle('active', active);
      element.setAttribute('aria-pressed', String(active));
    });
    document.querySelectorAll('[data-source]').forEach(element => {
      const active = element.dataset.source === resourceSource;
      element.classList.toggle('active', active);
      element.setAttribute('aria-pressed', String(active));
    });
    document.querySelectorAll('[data-count]').forEach(element => {
      const kind = element.dataset.count;
      const items = designView ? [...state.designs, ...state.catalog.filter(item => item.localOnly)] : catalog;
      const count = items.filter(item => item.kind === kind).length;
      element.textContent = String(count).padStart(2, '0');
      const caption = element.parentElement.querySelector('small');
      if (designView) {
        const drafts = state.designs.filter(item => item.kind === kind).length;
        caption.textContent = `草稿 ${drafts} · 版本 ${count - drafts}`;
      } else if (resourceSource === '已安装' && context.project !== '*') {
        const global = catalog.filter(item => item.kind === kind
          && state.installed.get(projectKey(context.node, '*'))?.has(keyFor(item))).length;
        caption.textContent = `全局可用 ${global} · 项目独有 ${count - global}`;
      } else caption.textContent = resourceSource === '公共库' ? '目录资源版本' : '全局安装版本';
    });
    $('#resource-grid').innerHTML = list.map(item => {
      const key = keyFor(item);
      const selected = state.selection.has(key);
      const available = installed(key, context.node, context.project);
      return `<article class="resource-package flow-resource" data-resource-key="${escape(key)}">
        <span class="resource-icon"><i data-icon="${typeIcon[item.kind]}"></i></span>
        <div class="resource-heading"><h2>${escape(item.name)}<span class="resource-kind-label">${typeLabel[item.kind]}</span></h2><p class="package-id">${escape(item.id)} · v${escape(item.version)}</p></div>
        <p class="package-description">${escape(item.description)}</p>
        <div class="resource-meta"><span>${escape(item.source)}</span><span>${escape(item.tags.join(' · '))}</span></div>
        <div class="package-footer"><span>${available ? `${installationLabel(key, context.node, context.project)} · 演示` : '尚未安装'}</span>
          <button class="button" data-flow="detail" data-key="${escape(key)}">查看详情</button>
          <button class="button" data-flow="install" data-key="${escape(key)}">${available ? '安装到…' : '安装'}</button>
          <button class="button ${selected ? 'is-selected' : 'primary'}" data-flow="select-tool" data-key="${escape(key)}" aria-pressed="${selected}">${selected ? '已选' : '选择'}${selected ? '<i data-icon="check"></i>' : '<i data-icon="plus"></i>'}</button>
        </div></article>`;
    }).join('');
    let count = list.length;
    if (designView) count = renderDesigns(query);
    $('#resource-result-count').textContent = `显示 ${count} / 共 ${total} 项`;
    $('.resource-empty').hidden = count > 0;
    $('#resource-source-note').textContent = {
      公共库: '目录数量与工作项目独立。点击安装后选择全局或指定项目，选择工具组合后可开始工作。所有安装均为演示。',
      已安装: '全局表示此节点所有项目可用。项目视图合并全局与项目安装，每行标明来源。所有安装记录均为演示。',
      本地设计: '本地设计用于保存、查看和导出内容。源码开发、测试与发布从“开发工具”进入。',
    }[resourceSource];
    syncResourceNavigation();
    renderSelection();
    mountIcons($('#resources-page'));
  }
  function toggleSelection(key) {
    if (state.selection.has(key)) state.selection.delete(key);
    else {
      const item = itemFor(key);
      const previousVersion = [...state.selection].find(selected => {
        const previous = itemFor(selected);
        return previous.kind === item.kind && previous.id === item.id;
      });
      if (previousVersion) {
        genericDialog('替换工具版本？',
          `<p class="flow-lead">将“${escape(item.name)}”从 v${escape(itemFor(previousVersion).version)} 改为 v${escape(item.version)}，其他已选工具会保留。</p>`,
          '<button class="button" data-close>取消</button>' + button('replace-version', '确认替换版本', `data-key="${escape(key)}" data-previous="${escape(previousVersion)}"`, true));
        return;
      }
      if (item.kind === 'packages') {
        const previous = [...state.selection].find(selected => itemFor(selected).kind === 'packages');
        if (previous) {
          genericDialog('替换主 Package？',
            `<p class="flow-lead">当前组合已选择“${escape(itemFor(previous).name)}”。改用“${escape(item.name)}”后，已选技能和扩展会保留。</p>`,
            '<button class="button" data-close>取消</button>' + button('replace-package', '确认替换', `data-key="${escape(key)}"`, true));
          return;
        }
      }
      state.selection.add(key);
    }
    renderResources();
    if ($('#detail-dialog').open) renderDetail(key);
  }
  function renderDetail(key) {
    const item = itemFor(key);
    if (!item) return;
    state.detail = key;
    const context = resourceContext();
    const selected = state.selection.has(key);
    const inSession = !homeMode && state.applied.get(scopeKey(currentNode, currentSession))?.includes(key);
    $('#detail-resource-name').textContent = item.name;
    $('#detail-resource-description').textContent = item.description;
    $('#detail-resource-facts').innerHTML = [
      ['类型', typeLabel[item.kind]], ['来源', item.source], ['版本', `v${item.version}`],
      ['查看范围', `${nodeName(context.node)} · ${context.project === '*' ? '全局' : context.project}`],
      ['安装来源', `${installationLabel(key, context.node, context.project)} · 演示记录`],
      ['工具组合', selected ? '已选择' : '尚未选择'],
      ['当前会话', inSession ? '已应用 · 演示记录' : '尚未应用此工具'],
    ].map(([title, value]) => `<dt>${escape(title)}</dt><dd>${escape(value)}</dd>`).join('');
    const content = item.kind === 'packages'
      ? ['主 Package 提供工作约定与基础能力。', ...item.tags, '技能与扩展可在当前组合中另外选择。']
      : item.kind === 'extensions'
        ? [item.permission || '由目标节点确认请求的能力', '安装前查看来源与权限，应用到会话后核对加载状态。']
        : ['以 SKILL.md 内容描述工作方法与适用场景。', ...item.tags];
    $('#detail-resource-content').innerHTML = `<h3>${item.kind === 'packages' ? '包含内容' : item.kind === 'extensions' ? '使用要求' : '技能内容'}</h3><ul>${content.map(line => `<li>${escape(line)}</li>`).join('')}</ul>`;
    const ownInstallation = state.installed.get(projectKey(context.node, context.project))?.has(key);
    $('#detail-resource-actions').innerHTML = (ownInstallation
      ? button('uninstall', context.project === '*' ? '移除全局安装' : '移除项目安装', `data-key="${escape(key)}"`) : '')
      + button('install', '安装到…', `data-key="${escape(key)}"`)
      + button('select-tool', selected ? '从组合移除' : '加入工具组合', `data-key="${escape(key)}"`, true);
  }
  function openDetail(key) { renderDetail(key); showDialog('#detail-dialog'); }

  function renderDesigns(query) {
    const drafts = state.designs.filter(item => (resourceKind === 'all' || resourceKind === item.kind)
      && `${item.name} ${item.id} ${item.content}`.toLowerCase().includes(query));
    const releases = state.catalog.filter(item => item.localOnly
      && (resourceKind === 'all' || resourceKind === item.kind)
      && `${item.name} ${item.id}`.toLowerCase().includes(query));
    $('#resource-grid').innerHTML = [
      ...drafts.map(item => `<article class="resource-package flow-resource">
        <span class="resource-icon"><i data-icon="${typeIcon[item.kind]}"></i></span><div class="resource-heading"><h2>${escape(item.name)}<span class="resource-kind-label">设计草稿</span></h2><p class="package-id">${escape(item.id)} · v${escape(item.version)}</p></div>
        <p class="package-description">${escape(item.content.split('\n').filter(Boolean).slice(0, 2).join(' · '))}</p>
        <div class="package-footer"><span>${typeLabel[item.kind]} · 当前页面内存</span>${button('edit-design', '继续开发', `data-design="${escape(item.id)}"`)}${button('export-design', '导出 JSON', `data-design="${escape(item.id)}"`)}</div></article>`),
      ...releases.map(item => `<article class="resource-package flow-resource">
        <span class="resource-icon"><i data-icon="${typeIcon[item.kind]}"></i></span><div class="resource-heading"><h2>${escape(item.name)}<span class="resource-kind-label">本地发布 · 演示</span></h2><p class="package-id">${escape(item.id)} · v${escape(item.version)}</p></div>
        <p class="package-description">${escape(item.description)}</p><div class="package-footer"><span>演示测试记录已关联</span>${button('detail', '查看版本', `data-key="${escape(keyFor(item))}"`)}${button('install', '安装', `data-key="${escape(keyFor(item))}"`)}${button('select-tool', state.selection.has(keyFor(item)) ? '已选' : '选择', `data-key="${escape(keyFor(item))}"`, true)}</div></article>`),
    ].join('');
    return drafts.length + releases.length;
  }

  function planStages(plan) {
    if (plan.mode === 'install') return ['获取工具内容', '核对示例清单', '安装并回读'];
    return [plan.mode === 'defaults' ? '保存项目默认组合' : '应用工具组合', '工作环境就绪'];
  }
  function matchingCurrent(plan) {
    return !homeMode && currentNode === plan.node && currentProject() === plan.project
      && !['running', 'waiting', 'disconnected'].includes(state.runs.get(scopeKey(currentNode, currentSession)));
  }
  function checkout(mode = 'work') {
    if (!state.selection.size) return;
    state.plan = {
      mode, node: state.libraryNode, project: state.libraryProject,
      model: 'DeepSeek V4 Pro', destination: 'new', intent: '',
      scenario: 'success', stage: 'configure', index: 0, error: null, keys: [...state.selection],
    };
    if (state.applyToCurrent && matchingCurrent(state.plan)) state.plan.destination = 'current';
    renderWork();
    showDialog('#work-dialog');
  }
  function missingTools(plan) {
    if (plan.mode === 'install') {
      const project = plan.scope === 'global' ? '*' : plan.project;
      return plan.keys.filter(key => !state.installed.get(projectKey(plan.node, project))?.has(key));
    }
    return plan.keys.filter(key => !installed(key, plan.node, plan.project));
  }
  function installTools(keys, resumePlan = null) {
    const context = resourceContext();
    state.plan = {
      mode: 'install', keys, node: resumePlan?.node || context.node,
      project: resumePlan?.project || (context.project === '*' ? state.libraryProject : context.project),
      scope: '', scenario: resumePlan?.scenario === 'apply-failed' ? 'success' : resumePlan?.scenario || 'success',
      stage: 'configure', index: 0, error: null, resumePlan,
    };
    const projects = projectOptions().filter(project => project.node === state.plan.node);
    if (!projects.some(project => project.name === state.plan.project)) state.plan.project = projects[0]?.name || '';
    renderWork();
    if (!$('#work-dialog').open) showDialog('#work-dialog');
  }
  function renderInstallation() {
    const plan = state.plan;
    const projects = projectOptions().filter(project => project.node === plan.node);
    const missing = missingTools(plan);
    $('#work-dialog-title').textContent = '选择安装范围';
    $('#work-dialog-body').innerHTML = `
      <p class="flow-lead">${plan.resumePlan ? '工作项目缺少所选工具，请确认安装位置后继续。' : '为这些资源选择安装位置。'}</p>
      <div class="flow-box"><h3>待安装资源 · ${plan.keys.length} 项</h3>${plan.keys.map(key => {
        const item = itemFor(key);
        return `<p>${escape(item.name)} · v${escape(item.version)} <span class="flow-footnote">${plan.scope && !missing.includes(key) ? '此范围已安装' : ''}</span></p>`;
      }).join('')}</div>
      <div class="flow-form"><label>安装节点<select id="install-node" ${plan.resumePlan ? 'disabled' : ''}>${demoNodes.map(node => `<option value="${escape(node.id)}" ${node.id === plan.node ? 'selected' : ''}>${escape(node.name)}</option>`).join('')}</select></label></div>
      <fieldset class="install-scope-choices"><legend>安装到哪里？</legend>
        <label class="install-scope-choice"><input type="radio" name="install-scope" value="global" ${plan.scope === 'global' ? 'checked' : ''}><span><strong>节点全局</strong><small>${escape(nodeName(plan.node))}上的所有项目都可使用。</small></span></label>
        <label class="install-scope-choice"><input type="radio" name="install-scope" value="project" ${plan.scope === 'project' ? 'checked' : ''}><span><strong>指定项目</strong><small>仅供所选项目使用。</small></span></label>
      </fieldset>
      ${plan.scope === 'project' ? `<div class="flow-form"><label>安装项目<select id="install-project" ${plan.resumePlan ? 'disabled' : ''}>${projects.length ? projects.map(project => `<option value="${escape(project.name)}" ${project.name === plan.project ? 'selected' : ''}>${escape(project.name)}</option>`).join('') : '<option value="">请先添加项目</option>'}</select></label></div>` : ''}
      <p class="flow-footnote">来源与使用要求可在资源详情查看。安装后可从资源中心加入工作组合。</p>
      ${preparationScenarios(plan, true)}`;
    const valid = plan.scope === 'global' || (plan.scope === 'project' && !!plan.project);
    $('#work-dialog-actions').innerHTML = button('cancel-install', plan.resumePlan ? '返回工作设置' : '取消')
      + button('begin-installation', plan.scope && !missing.length ? '此范围已安装，继续' : '确认并演示安装', valid ? '' : 'disabled', true);
  }
  function preparationScenarios(plan, installation = false) {
    const scenarios = installation
      ? [['success', '正常完成'], ['download-failed', '获取失败，随后重试'], ['install-unknown', '安装结果待核对']]
      : [['success', '正常完成'], ['download-failed', '获取失败，随后重试'], ['install-unknown', '安装结果待核对'], ['apply-failed', '会话应用失败，随后重试']];
    return `<details class="flow-scenarios"><summary>流程演示选项</summary><label for="work-scenario">准备过程的演示结果</label><select id="work-scenario">${scenarios.map(([value, label]) => `<option value="${value}" ${plan.scenario === value ? 'selected' : ''}>${label}</option>`).join('')}</select></details>`;
  }
  function cancelInstallation() {
    const resumePlan = state.plan?.resumePlan;
    if (resumePlan) {
      state.plan = resumePlan;
      renderWork();
    } else $('#work-dialog').close();
  }
  function renderWork() {
    const plan = state.plan;
    if (!plan) return;
    if (plan.stage !== 'configure') { renderPreparation(); return; }
    if (plan.mode === 'install') { renderInstallation(); return; }
    $('#work-dialog-title').textContent = plan.mode === 'defaults' ? '设置项目默认组合' : '开始工作';
    const missing = missingTools(plan);
    const projectList = projectOptions();
    const currentKey = projectKey(plan.node, plan.project);
    const currentAllowed = matchingCurrent(plan);
    if (!currentAllowed) plan.destination = 'new';
    const modelReady = plan.model !== 'GPT-5' || state.providerReady.has(`openai:${plan.node}`);
    $('#work-dialog-body').innerHTML = `
      <p class="flow-lead">${plan.mode === 'defaults' ? '确认这个项目后续新会话采用的工具组合。' : '确认项目与工具组合，准备完成后进入会话。'}</p>
      <div class="flow-form">
        <label>工作项目<select id="work-project">${projectList.map(project => `<option value="${escape(project.key)}" ${project.key === currentKey ? 'selected' : ''}>${escape(nodeName(project.node))} · ${escape(project.name)}</option>`).join('')}</select></label>
        ${plan.mode === 'work' ? `<label>工作会话<select id="work-destination"><option value="new" ${plan.destination === 'new' ? 'selected' : ''}>新建会话</option><option value="current" ${plan.destination === 'current' ? 'selected' : ''} ${currentAllowed ? '' : 'disabled'}>用于当前会话${currentAllowed ? '' : '（目标项目或状态不适用）'}</option></select></label>` : ''}
        ${plan.mode === 'work' ? `<label>会话模型<select id="work-model"><option ${plan.model === 'DeepSeek V4 Pro' ? 'selected' : ''}>DeepSeek V4 Pro</option><option ${plan.model === 'GPT-5' ? 'selected' : ''}>GPT-5</option></select></label>
        ${!modelReady ? `<div class="flow-callout"><p>目标节点的 OpenAI 示例配置尚未准备。</p>${button('prepare-model', '使用示例模型配置')}</div>` : ''}
        <label>${plan.destination === 'current' ? '继续工作的想法（可选）' : '这次想做什么（可选）'}<textarea id="work-intent" rows="2" placeholder="例如，检查当前项目的代码变更">${escape(plan.intent)}</textarea></label>` : ''}
      </div>
      <div class="flow-box"><h3>工具组合 · ${plan.keys.length} 项</h3>${plan.keys.map(key => `<p>${escape(itemFor(key).name)} <span class="flow-footnote">· ${typeLabel[itemFor(key).kind]} · ${missing.includes(key) ? '需要准备' : '目标范围已安装'}</span></p>`).join('')}</div>
      ${missing.length ? `<p class="flow-footnote">目标项目有 ${missing.length} 项资源待安装。下一步将弹出全局或项目范围选择。</p>` : ''}
      ${preparationScenarios(plan)}
      <p class="flow-error" id="work-error" role="alert"></p>`;
    $('#work-dialog-actions').innerHTML = '<button class="button" data-close>返回选择</button>'
      + button('begin-preparation', '预览准备过程', modelReady ? '' : 'disabled', true);
  }
  function renderPreparation() {
    const plan = state.plan;
    const stages = planStages(plan);
    const ready = plan.index >= stages.length;
    $('#work-dialog-title').textContent = plan.mode === 'install' ? '安装资源 · 演示' : '准备工作 · 演示';
    $('#work-dialog-body').innerHTML = `<p class="flow-lead">${escape(nodeName(plan.node))} · ${plan.mode === 'install' && plan.scope === 'global' ? '节点全局' : escape(plan.project)} · ${plan.mode === 'install' ? '资源安装' : plan.mode === 'defaults' ? '项目默认组合' : plan.destination === 'new' ? '新建会话' : '当前会话'}</p>
      <ol class="flow-steps">${stages.map((title, index) => {
        const status = index < plan.index ? 'complete' : index === plan.index ? (plan.error ? 'failed' : 'current') : 'pending';
        return `<li data-status="${status}"><span class="flow-step-mark">${index < plan.index ? '✓' : index + 1}</span><span>${title}</span><small>${index < plan.index ? '演示已确认' : index === plan.index ? '等待演示操作' : '待处理'}</small></li>`;
      }).join('')}</ol><div role="status" class="flow-callout"><p>${escape(plan.error || (ready ? '准备演示已完成，可以继续。' : '点击“推进演示”查看下一阶段。'))}</p></div>
      <p class="flow-footnote">每一步均为可点击的状态演示，实际节点和安装目录保持独立。</p>`;
    $('#work-dialog-actions').innerHTML = ready
      ? button('finish-preparation', plan.mode === 'install' ? (plan.resumePlan ? '继续工作准备' : '完成安装') : plan.mode === 'defaults' ? '保存默认组合并返回' : plan.destination === 'new' ? '进入新会话' : '返回当前会话', '', true)
      : (plan.mode === 'install' ? button('cancel-install', '取消准备') : '<button class="button" data-close>取消准备</button>')
        + button(plan.error ? 'recover-preparation' : 'advance-preparation',
          plan.error ? (plan.errorType === 'unknown' ? '核对演示状态' : '重试此步骤') : '推进演示', '', true);
  }
  function advancePreparation() {
    const plan = state.plan;
    if (!plan || plan.stage !== 'preparing' || plan.index >= planStages(plan).length || plan.error) return;
    const failureIndex = plan.mode === 'install'
      ? { 'download-failed': 0, 'install-unknown': 2 }[plan.scenario]
      : { 'apply-failed': 0 }[plan.scenario];
    if (plan.index === failureIndex) {
      plan.errorType = plan.scenario === 'install-unknown' ? 'unknown' : 'failed';
      plan.error = {
        'download-failed': '获取失败 · 演示。已选工具和工作输入已保留，可重试当前步骤。',
        'install-unknown': '安装结果待核对 · 演示。先回读确认，再继续应用工具组合。',
        'apply-failed': '应用失败 · 演示。安装记录已保留，当前会话尚未更改，可重试应用。',
      }[plan.scenario];
      renderPreparation();
      return;
    }
    if (plan.mode === 'install' && plan.index === 2) {
      const destination = projectKey(plan.node, plan.scope === 'global' ? '*' : plan.project);
      const available = new Set(state.installed.get(destination) || []);
      plan.keys.forEach(key => available.add(key));
      state.installed.set(destination, available);
    }
    plan.index += 1;
    renderPreparation();
    renderResources();
  }
  function finishPreparation() {
    const plan = state.plan;
    if (!plan || plan.index < planStages(plan).length) return;
    if (plan.mode === 'install') {
      if (plan.resumePlan) {
        state.plan = plan.resumePlan;
        state.plan.stage = 'preparing';
        state.plan.index = 0;
        renderPreparation();
      } else {
        state.installedNode = plan.node;
        state.installedProject = plan.scope === 'global' ? '*' : plan.project;
        resourceSource = '已安装';
        renderResources();
        syncRoute();
        $('#work-dialog').close();
        toast('演示安装已完成，可在已安装资源中查看。');
      }
      return;
    }
    if (plan.mode === 'defaults') {
      state.defaults.set(projectKey(plan.node, plan.project), [...plan.keys]);
      $('#work-dialog').close();
      renderResources();
      toast('项目默认工具组合已保存到本次演示。');
      return;
    }
    if (plan.destination === 'current') {
      if (!matchingCurrent(plan)) {
        plan.error = '会话上下文已变化，请重新确认工作项目与会话。';
        plan.stage = 'configure';
        renderWork();
        $('#work-error').textContent = plan.error;
        return;
      }
      state.applied.set(scopeKey(currentNode, currentSession), [...plan.keys]);
      sessionPreferences.set(scopeKey(currentNode, currentSession), { ...currentPreference(), model: plan.model });
      const draft = sessionDrafts.get(scopeKey(currentNode, currentSession)) || '';
      if (plan.intent.trim()) sessionDrafts.set(scopeKey(currentNode, currentSession), [draft, plan.intent.trim()].filter(Boolean).join('\n'));
      $('#work-dialog').close();
      selectSession(currentNode, currentSession);
    } else {
      const title = plan.intent.trim() || `${itemFor(plan.keys[0]).name}工作会话`;
      createDemoSession(plan.node, plan.project, Array.from(title).slice(0, 24).join(''), plan.keys, plan.model, plan.intent);
    }
    syncWorkspaceTools();
    $('#prompt').focus();
  }

  function syncWorkspaceTools() {
    const key = scopeKey(currentNode, currentSession);
    const selected = homeMode
      ? state.defaults.get(projectKey(homeState.node, homeState.project)) || []
      : state.applied.get(key) || [];
    const bar = $('#work-tools-bar');
    bar.hidden = false;
    bar.title = `${homeMode ? '项目默认工具' : '会话工具'} · ${selected.length ? selected.map(tool => itemFor(tool)?.name || tool).join('、') : '尚未选择'}`;
    bar.innerHTML = `<button class="text-button" data-flow="session-tools" aria-label="查看会话工具"><i data-icon="blocks"></i>${homeMode ? '默认' : '会话'} ${selected.length}</button>
      ${!homeMode ? '<button class="icon-button workspace-session-actions" data-flow="session-actions" aria-label="会话操作"><i data-icon="more"></i></button>' : ''}`;
    const preview = $('#run-preview');
    const run = state.runs.get(key);
    preview.hidden = homeMode || !run;
    if (run && !homeMode) {
      const content = {
        running: ['运行中 · 演示', '正在按本次工具组合处理需求。', button('run-wait', '演示等待确认') + button('run-complete', '演示完成') + button('run-stop', '停止') + button('run-disconnect', '演示断线')],
        waiting: ['等待你的确认 · 演示', '开发诊断请求检查当前项目的构建状态。', button('run-approve', '查看并回答', '', true) + button('run-stop', '停止')],
        completed: ['本轮完成 · 演示', '可以检查项目文件与示例变更，再继续提出修改意见。', button('review-changes', '查看变更') + button('run-idle', '继续对话')],
        stopped: ['已停止 · 演示', '输入草稿保留，可以调整需求后继续。', button('run-idle', '继续对话')],
        disconnected: ['连接中断 · 演示', '运行结果尚未确认，重新连接后核对历史与状态。', button('run-reconnect', '重连并核对', '', true)],
      }[run];
      preview.innerHTML = `<strong>${content[0]}</strong><p>${content[1]}</p><div class="flow-actions">${content[2]}</div>`;
    }
    const context = $('.resource-context');
    context.querySelector('.capability-row strong').textContent = selected.length ? `${selected.length} 项会话工具` : '尚未选择工具';
    context.querySelector('.capability-row small').textContent = selected.length ? '已应用 · 演示记录' : '可从工具库选择';
    context.querySelector('.skill-chips').replaceChildren(...selected.map(tool => {
      const span = document.createElement('span');
      span.textContent = itemFor(tool)?.name || tool;
      return span;
    }));
    mountIcons(bar);
    mountIcons(preview);
  }
  function sessionTools() {
    const selected = homeMode
      ? state.defaults.get(projectKey(homeState.node, homeState.project)) || []
      : state.applied.get(scopeKey(currentNode, currentSession)) || [];
    genericDialog(homeMode ? '项目默认工具' : '当前会话工具',
      `<p class="flow-lead">${escape(nodeName(currentNodeId()))} · ${escape(currentProject())}</p><div class="flow-box"><h3>${homeMode ? '新会话默认选择' : '已应用的组合 · 演示'}</h3>${selected.length ? selected.map(key => `<p>${escape(itemFor(key).name)} · ${typeLabel[itemFor(key).kind]}</p>`).join('') : '<p>尚未选择工具。</p>'}</div>`,
      (homeMode ? '' : button('session-actions', '会话操作'))
        + '<button class="button" data-close>关闭</button>' + button('adjust-tools', '调整工具', '', true));
  }

  function openProjectChooser(library = false) {
    genericDialog('选择工作项目',
      `<div class="flow-list">${projectOptions().map(project => `<button data-flow="choose-project" data-project="${escape(project.key)}" data-library="${library}"><i data-icon="folder"></i><span><strong>${escape(project.name)}</strong><small>${escape(nodeName(project.node))}</small></span><i data-icon="chevron-right"></i></button>`).join('')}</div>
      <details class="flow-scenarios"><summary>添加项目</summary><div class="flow-form"><label>项目名称<input id="flow-project-name" maxlength="80" placeholder="例如 my-app"></label><label>项目所属节点<select id="flow-project-node">${demoNodes.map(node => `<option value="${escape(node.id)}">${escape(node.name)}</option>`).join('')}</select></label>${button('add-project', '添加演示项目', `data-library="${library}"`)}</div></details>`,
      '<button class="button" data-close>返回</button>');
  }
  function sessionActions() {
    if (homeMode) { openProjectChooser(); return; }
    const session = currentSessionRecord();
    genericDialog('会话操作',
      `<p class="flow-lead">${escape(nodeName(currentNode))} · ${escape(session.project)}</p><div class="flow-form"><label>会话名称<input id="rename-session" maxlength="100" value="${escape(session.title)}"></label></div>`,
      button('rename-session', '保存名称') + button('fork-session', '分支为新会话') + button('backend', '选择执行后端') + button('archive-session', '归档会话'));
  }
  function openArchives() {
    const archived = demoNodes.flatMap(node => node.sessions.filter(session => session.archived).map(session => ({ node, session })));
    genericDialog('已归档会话',
      `<div class="flow-list">${archived.length ? archived.map(({ node, session }) => `<button data-flow="restore-session" data-node="${escape(node.id)}" data-session="${escape(session.id)}"><i data-icon="folder"></i><span><strong>${escape(session.title)}</strong><small>${escape(node.name)} · ${escape(session.project)} · 点击恢复</small></span><i data-icon="refresh"></i></button>`).join('') : '<p class="flow-empty">尚无已归档会话。会话操作中可以归档和恢复演示会话。</p>'}</div>`,
      '<button class="button" data-close>关闭</button>');
  }

  function openAuthoring(design) {
    if (design) {
      $('#author-name').value = design.name;
      $('#author-id').value = design.id;
      $('#author-kind').value = design.kind;
      $('#author-version').value = design.version;
      $('#author-source').value = design.content;
      invalidateDraft();
    }
    showView('authoring');
  }
  function invalidateDraft() {
    state.authorRevision += 1;
    state.validRevision = 0;
    state.testedRevision = 0;
    state.publishedKey = null;
    $('[data-flow=test-draft]').disabled = true;
    $('[data-flow=publish-draft]').disabled = true;
    $('#author-test-result').textContent = '源码已改变，保存并重新校验后可测试。';
    $('#author-status').textContent = `r${state.authorRevision} · 有未保存编辑`;
    $('#author-published').replaceChildren();
  }
  function currentDesign() {
    return {
      id: $('#author-id').value.trim(), name: $('#author-name').value.trim(),
      kind: $('#author-kind').value, version: $('#author-version').value.trim(),
      content: $('#author-source').value, revision: state.authorRevision,
    };
  }
  function validDesign(design) {
    return design && typeof design === 'object'
      && ['id', 'name', 'content', 'version', 'kind'].every(field => typeof design[field] === 'string')
      && /^[a-z0-9][a-z0-9._-]{0,79}$/.test(design.id)
      && design.name.length > 0 && design.name.length <= 80
      && design.content.trim().length > 0 && design.content.length <= 200000
      && /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(design.version)
      && ['skills', 'packages', 'extensions'].includes(design.kind);
  }
  function saveDraft() {
    const design = currentDesign();
    if (!validDesign(design)) {
      $('#author-status').textContent = '请检查名称、英文 ID、三段版本号和非空源码内容。';
      return false;
    }
    const index = state.designs.findIndex(item => item.id === design.id);
    if (index >= 0) state.designs[index] = design;
    else state.designs.push(design);
    state.savedRevision = state.authorRevision;
    $('#author-status').textContent = `r${state.authorRevision} · 草稿已保存到本次页面`;
    return true;
  }
  function importDesign() {
    genericDialog('导入设计文件',
      `<p class="flow-lead">粘贴设计内容，校验后存入本次原型的“本地设计”。</p><div class="flow-form"><label>设计 JSON<textarea id="design-json" rows="10" spellcheck="false" placeholder='{"id":"my-skill","name":"我的技能","kind":"skills","version":"1.0.0","content":"# 我的技能"}'></textarea></label></div><p id="design-error" class="flow-error" role="alert"></p>`,
      button('fill-design-example', '填入示例') + button('confirm-import', '校验并导入', '', true));
  }
  function exportDesign(id) {
    const design = state.designs.find(item => item.id === id);
    if (!design) return;
    const url = URL.createObjectURL(new Blob([JSON.stringify(design, null, 2)], { type: 'application/json' }));
    const link = document.createElement('a');
    link.href = url;
    link.download = `${design.id}.json`;
    link.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  }

  function showScenarios() {
    genericDialog('体验业务流程',
      `<p class="flow-lead">选择一个起点体验原型，当前页面中的选择和草稿会保留。</p><div class="flow-list">
        <button data-resource-entry="all"><i data-icon="blocks"></i><span><strong>选好工具，开始工作</strong><small>公共库、分类选择、工作准备与会话</small></span><i data-icon="arrow-right"></i></button>
        <button data-flow="onboarding"><i data-icon="download"></i><span><strong>首次使用引导</strong><small>环境准备、模型配置和项目选择</small></span><i data-icon="arrow-right"></i></button>
        <button data-flow="connect-device"><i data-icon="monitor"></i><span><strong>连接其他设备</strong><small>选择目标节点并进入其项目</small></span><i data-icon="arrow-right"></i></button>
        <button data-flow="authoring"><i data-icon="code"></i><span><strong>开发自己的工具</strong><small>草稿、校验、测试与本地发布</small></span><i data-icon="arrow-right"></i></button>
        <button data-flow="archives"><i data-icon="folder"></i><span><strong>找回归档会话</strong><small>恢复原会话和输入草稿</small></span><i data-icon="arrow-right"></i></button>
      </div>`, '<button class="button" data-close>返回</button>');
  }
  function onboarding() {
    const step = state.onboardingStep;
    const steps = [
      ['准备运行环境', '进入桌面后检测 rpi。这个示例演示缺少运行环境时的自动准备路径。', '演示缺少 rpi'],
      ['下载与校验', '查看下载、文件校验与 RPC 能力检查，完成后在同一窗口继续。', '演示环境准备完成'],
      ['配置模型', '为当前节点选择供应商、模型和凭据来源。', '使用示例模型配置'],
      ['选择工作项目', '打开已有项目或添加新的项目，再开始第一项工作。', '选择项目并继续'],
    ];
    genericDialog('首次使用引导',
      `<p class="flow-lead">桌面首次进入 · 演示</p><ol class="flow-steps">${steps.map(([title], index) => `<li data-status="${index < step ? 'complete' : index === step ? 'current' : 'pending'}"><span class="flow-step-mark">${index < step ? '✓' : index + 1}</span>${title}</li>`).join('')}</ol><div class="flow-box"><h3>${steps[step][0]}</h3><p>${steps[step][1]}</p></div>`,
      '<button class="button" data-close>稍后继续</button>' + button('onboarding-next', steps[step][2], '', true));
  }
  function connectDevice() {
    genericDialog('连接工作节点',
      `<p class="flow-lead">选择一个示例节点，体验项目与会话归属的切换。</p><div class="flow-form"><label>目标设备<select id="connect-node">${demoNodes.map(node => `<option value="${escape(node.id)}" ${node.id !== currentNodeId() ? 'selected' : ''}>${escape(node.name)}</option>`).join('')}</select></label><label>节点地址<input id="connect-address" value="http://studio.example:13031" placeholder="https://your-node.example"></label></div><div class="flow-box"><h3>连接后</h3><p>显示目标节点自己的项目与会话。原节点上的任务和草稿可以在返回后继续查看。</p></div>`,
      '<button class="button" data-close>取消</button>' + button('confirm-connect', '连接演示节点', '', true));
  }
  function backendChooser() {
    genericDialog('选择执行后端',
      `<p class="flow-lead">选择后将在当前项目新建对应后端的演示会话。</p><div class="flow-form"><label>执行后端<select id="backend-choice"><option>RPI</option><option>Claude</option><option>Codex</option><option>OpenCode</option></select></label></div><div class="flow-box"><p>原会话、草稿和运行状态可以通过侧栏找回。实际后端选项以目标节点公布的能力为准。</p></div>`,
      '<button class="button" data-close>取消</button>' + button('confirm-backend', '确认新建会话', '', true));
  }

  function readRoute() {
    const [view, query = ''] = location.hash.slice(1).split('?');
    if (view === 'authoring') return;
    if (view === 'resources' || ['skills', 'extensions', 'packages'].includes(view)) {
      const params = new URLSearchParams(query);
      resourceKind = labels[params.get('kind')] ? params.get('kind') : labels[view] ? view : 'all';
      resourceSource = codeSource[params.get('source')] || '公共库';
      const node = params.get('node');
      const project = params.get('project');
      if (demoNodes.some(item => item.id === node)
        && (project === '*' || projectOptions().some(item => item.node === node && item.name === project))) {
        state.installedNode = node;
        state.installedProject = project;
      }
      renderResources();
      syncRoute();
    }
  }
  window.clarityFlow = { renderResources, readRoute };

  document.addEventListener('click', event => {
    const target = event.target.closest('button');
    if (!target) return;
    if (target.dataset.resourceEntry) { openResources(target.dataset.resourceEntry); return; }
    if (target.dataset.kind) {
      resourceKind = target.dataset.kind;
      renderResources();
      syncRoute();
      return;
    }
    if (target.dataset.source) {
      resourceSource = target.dataset.source;
      renderResources();
      syncRoute();
      return;
    }
    const action = target.dataset.flow;
    if (!action) return;
    const key = target.dataset.key;
    if (action === 'scenarios') showScenarios();
    if (action === 'onboarding') { state.onboardingStep = 0; onboarding(); }
    if (action === 'onboarding-next') {
      if (state.onboardingStep < 3) { state.onboardingStep += 1; onboarding(); }
      else { openHome(); openProjectChooser(); }
    }
    if (action === 'connect-device') connectDevice();
    if (action === 'confirm-connect') {
      const node = demoNodes.find(item => item.id === $('#connect-node').value);
      const address = $('#connect-address').value.trim();
      try {
        const url = new URL(address);
        if (!['http:', 'https:'].includes(url.protocol) || url.username || url.password) throw new Error();
      } catch {
        $('#connect-address').setCustomValidity('请输入不含用户名和密码的 HTTP(S) 地址。');
        $('#connect-address').reportValidity();
        return;
      }
      closeDialogs();
      openHome();
      currentNode = node.id;
      currentSession = node.sessions.find(session => !session.archived)?.id || '';
      homeState.node = node.id;
      homeState.project = node.sessions[0]?.project || '新项目';
      $('#welcome-project-name').textContent = homeState.project;
      $('#welcome-project-node').textContent = node.name;
      syncWorkspaceMode();
      syncWorkspaceTools();
      renderSessionLists();
      syncPreference();
    }
    if (action === 'backend') backendChooser();
    if (action === 'confirm-backend') {
      const backend = $('#backend-choice').value;
      const key = createDemoSession(currentNodeId(), currentProject(), `${backend} 工作会话`);
      state.selectedBackend.set(key, backend);
      toast(`${backend} 演示会话已创建，可从侧栏找回原会话。`);
    }
    if (action === 'detail') openDetail(key);
    if (action === 'install') installTools([key]);
    if (action === 'select-tool') toggleSelection(key);
    if (action === 'remove-selection') { state.selection.delete(key); renderResources(); }
    if (action === 'replace-package') {
      [...state.selection].filter(selected => itemFor(selected).kind === 'packages').forEach(selected => state.selection.delete(selected));
      state.selection.add(key);
      $('#flow-dialog').close();
      renderResources();
    }
    if (action === 'replace-version') {
      state.selection.delete(target.dataset.previous);
      state.selection.add(key);
      $('#flow-dialog').close();
      renderResources();
    }
    if (action === 'clear-selection') { state.selection.clear(); renderResources(); }
    if (action === 'clear-search') {
      $('#resource-search').value = '';
      resourceKind = 'all';
      renderResources();
      syncRoute();
    }
    if (action === 'review-selection') { renderSelection(); showDialog('#selection-dialog'); }
    if (action === 'checkout') checkout();
    if (action === 'project-defaults') checkout('defaults');
    if (action === 'prepare-model') { state.providerReady.add(`openai:${state.plan.node}`); renderWork(); }
    if (action === 'begin-preparation') {
      const missing = missingTools(state.plan);
      if (missing.length) { installTools(missing, state.plan); return; }
      state.plan.stage = 'preparing';
      state.plan.index = 0;
      renderPreparation();
    }
    if (action === 'begin-installation') {
      const plan = state.plan;
      if (plan.mode !== 'install' || !plan.scope || (plan.scope === 'project' && !plan.project)) return;
      plan.keys = missingTools(plan);
      plan.stage = 'preparing';
      plan.index = plan.keys.length ? 0 : planStages(plan).length;
      renderPreparation();
    }
    if (action === 'cancel-install') cancelInstallation();
    if (action === 'close-work') {
      if (state.plan?.mode === 'install') cancelInstallation();
      else $('#work-dialog').close();
    }
    if (action === 'advance-preparation') advancePreparation();
    if (action === 'recover-preparation') {
      state.plan.error = null;
      state.plan.scenario = 'success';
      renderPreparation();
    }
    if (action === 'finish-preparation') finishPreparation();
    if (action === 'return-origin') returnToOrigin();
    if (action === 'back-library') openResources(resourceKind);
    if (action === 'adjust-tools') openResources('all', true);
    if (action === 'session-tools') sessionTools();
    if (action === 'library-project') openProjectChooser(true);
    if (action === 'choose-project') {
      const project = projectOptions().find(item => item.key === target.dataset.project);
      if (target.dataset.library === 'true') {
        state.libraryNode = project.node;
        state.libraryProject = project.name;
        renderResources();
      } else {
        homeState.node = project.node;
        homeState.project = project.name;
        $('#welcome-project-name').textContent = project.name;
        $('#welcome-project-node').textContent = nodeName(project.node);
        syncWorkspaceMode();
        syncWorkspaceTools();
      }
      $('#flow-dialog').close();
    }
    if (action === 'add-project') {
      const name = $('#flow-project-name').value.trim();
      if (!name) { $('#flow-project-name').focus(); return; }
      const node = $('#flow-project-node').value;
      if (!projectOptions().some(project => project.node === node && project.name === name)) state.extraProjects.push({ node, name });
      const library = target.dataset.library === 'true';
      if (library) { state.libraryNode = node; state.libraryProject = name; }
      else {
        homeState.node = node;
        homeState.project = name;
        $('#welcome-project-name').textContent = name;
        $('#welcome-project-node').textContent = nodeName(node);
        syncWorkspaceMode();
        syncWorkspaceTools();
      }
      $('#flow-dialog').close();
      renderResources();
    }
    if (action === 'session-actions') sessionActions();
    if (action === 'create-project-session') {
      const dialog = $('#new-session-dialog');
      const title = $('#new-session-title').value.trim() || '新工作会话';
      const node = dialog.dataset.node;
      const project = dialog.dataset.project;
      createDemoSession(node, project, title, state.defaults.get(projectKey(node, project)) || []);
    }
    if (action === 'provider-save-definition') {
      const dialog = $('#provider-definition-dialog');
      const id = dialog.querySelector('[aria-label="供应商 ID"]').value.trim();
      const name = dialog.querySelector('[aria-label="显示名称"]').value.trim();
      if (!id || !name) return;
      const providerList = $('#provider-list');
      let row = [...providerList.children].find(item => item.dataset.savedProvider === id);
      if (!row) { row = document.createElement('article'); row.className = 'provider-row'; row.dataset.savedProvider = id; providerList.append(row); }
      row.innerHTML = `<div class="provider-row-heading"><span class="provider-mark">${escape(name.slice(0, 1))}</span><div class="provider-identity"><h3>${escape(name)} <span class="provider-id">${escape(id)}</span></h3><p>已保存定义 · 演示</p></div><span class="provider-status">待配置密钥</span></div><div class="provider-endpoint"><span>Base URL</span><code>${escape(dialog.querySelector('[aria-label="Base URL"]').value)}</code></div><div class="provider-actions"><button class="button" data-provider-action="key">配置密钥</button><button class="button" data-provider-action="edit">编辑定义</button></div>`;
      dialog.close();
      toast('供应商定义已保存到本次演示。');
    }
    if (action === 'provider-save-key') {
      const dialog = $('#provider-key-dialog');
      const input = dialog.querySelector('[aria-label="新 API Key"]');
      if (!input.value.trim()) { input.focus(); return; }
      const name = $('#provider-key-name').textContent;
      const row = [...$('#provider-list').children].find(item => item.querySelector('h3')?.textContent.trim().startsWith(name));
      if (row) row.querySelector('.provider-status').textContent = '已配置 · 演示';
      const providerId = row?.dataset.savedProvider || row?.querySelector('.provider-id')?.textContent.trim();
      if (providerId) state.providerReady.add(`${providerId}:${currentNodeId()}`);
      input.value = '';
      dialog.close();
      toast('示例配置状态已更新，输入内容已清空。');
    }
    if (action === 'rename-session') {
      const title = $('#rename-session').value.trim();
      if (!title) return;
      currentSessionRecord().title = title;
      $('#flow-dialog').close();
      selectSession(currentNode, currentSession);
    }
    if (action === 'fork-session') {
      const node = demoNodes.find(item => item.id === currentNode);
      const original = currentSessionRecord();
      const id = `preview-${++previewSequence}`;
      const oldKey = scopeKey(currentNode, currentSession), newKey = scopeKey(currentNode, id);
      rememberDraft();
      node.sessions.unshift({ ...original, id, title: `${original.title} · 分支`, state: 'idle', recent: -previewSequence });
      const messages = previewMessages.get(oldKey);
      previewMessages.set(newKey, messages ? [...messages] : ['从现有会话分出的一条演示思路']);
      sessionDrafts.set(newKey, '');
      sessionPreferences.set(newKey, { ...currentPreference() });
      state.applied.set(newKey, [...(state.applied.get(oldKey) || [])]);
      $('#flow-dialog').close();
      selectSession(currentNode, id);
    }
    if (action === 'archive-session') {
      currentSessionRecord().archived = true;
      $('#flow-dialog').close();
      openHome();
    }
    if (action === 'archives') openArchives();
    if (action === 'restore-session') {
      demoNodes.find(node => node.id === target.dataset.node).sessions.find(session => session.id === target.dataset.session).archived = false;
      $('#flow-dialog').close();
      selectSession(target.dataset.node, target.dataset.session);
    }
    if (action === 'authoring') openAuthoring();
    if (action === 'import-design') importDesign();
    if (action === 'fill-design-example') $('#design-json').value = JSON.stringify({ id: 'my-review', name: '我的审查技能', kind: 'skills', version: '1.0.0', content: '# 代码审查\n\n检查边界条件并提出测试建议。' }, null, 2);
    if (action === 'confirm-import') {
      try {
        const raw = $('#design-json').value;
        if (raw.length > 200000) throw new Error('设计文件超过原型导入上限。');
        const design = JSON.parse(raw);
        if (!validDesign(design)) throw new Error('请填写有效的 id、name、kind、version 和 content。');
        if (state.designs.some(item => item.id === design.id)) throw new Error('已有同 ID 设计，请在本地设计中继续编辑。');
        state.designs.push({ id: design.id, name: design.name, kind: design.kind, version: design.version, content: design.content, revision: 1 });
        $('#flow-dialog').close();
        resourceSource = '本地设计';
        resourceKind = design.kind;
        renderResources();
        syncRoute();
      } catch (error) { $('#design-error').textContent = error.message; }
    }
    if (action === 'export-design') exportDesign(target.dataset.design);
    if (action === 'edit-design') openAuthoring(state.designs.find(item => item.id === target.dataset.design));
    if (action === 'save-draft') saveDraft();
    if (action === 'validate-draft' && saveDraft()) {
      state.validRevision = state.authorRevision;
      $('[data-flow=test-draft]').disabled = false;
      $('#author-status').textContent = `r${state.authorRevision} · 结构校验通过，可预览测试`;
    }
    if (action === 'test-draft' && state.validRevision === state.authorRevision) {
      genericDialog('资源测试预览',
        `<p class="flow-lead">${escape($('#author-name').value)} · r${state.authorRevision}</p><div class="flow-box"><h3>测试快照</h3><p>使用当前已保存的源码版本与示例项目，演示测试执行、结果检查和发布资格。</p></div><p>正式测试将由目标节点创建会话并执行模型请求。</p>`,
        button('test-failed', '演示测试失败') + button('test-passed', '演示测试通过', '', true));
    }
    if (action === 'test-failed') {
      state.testedRevision = 0;
      $('[data-flow=publish-draft]').disabled = true;
      $('#author-test-result').textContent = `r${state.authorRevision} · 测试失败演示，修改后重新校验与测试。`;
      $('#flow-dialog').close();
    }
    if (action === 'test-passed') {
      state.testedRevision = state.authorRevision;
      $('[data-flow=publish-draft]').disabled = false;
      $('#author-test-result').textContent = `r${state.authorRevision} · 演示测试通过，记录对应当前源码版本。`;
      $('#flow-dialog').close();
    }
    if (action === 'publish-draft' && state.testedRevision === state.authorRevision && state.validRevision === state.authorRevision) {
      const design = currentDesign();
      const existingVersions = state.catalog.filter(item => item.id === design.id && item.kind === design.kind);
      if (existingVersions.some(item => item.version === design.version)) {
        $('#author-status').textContent = '此版本已有发布记录。请使用新版本号，重新校验与测试后发布，原版本会保留。';
        return;
      }
      const item = { ...design, description: design.content.split('\n').filter(Boolean).slice(1).join(' ').slice(0, 180) || design.name, tags: ['本地发布'], source: `${nodeName(state.libraryNode)} · 本地演示发布`, localOnly: true, icon: typeIcon[design.kind] };
      if (existingVersions.length) item.key = `${design.kind}/${design.id}@${design.version}`;
      state.catalog.push(item);
      state.publishedKey = keyFor(item);
      $('#author-published').innerHTML = `<div class="flow-box"><h3>v${escape(design.version)} 已发布 · 演示</h3><p>发布记录已关联 r${state.authorRevision} 的演示测试。</p></div>${button('view-release', '查看发布版本')}`;
      $('[data-flow=publish-draft]').disabled = true;
    }
    if (action === 'view-release') {
      openResources(itemFor(state.publishedKey).kind);
      resourceSource = '本地设计';
      renderResources();
      syncRoute();
      openDetail(state.publishedKey);
    }
    if (action === 'uninstall') {
      const context = resourceContext();
      const removal = projectKey(context.node, context.project);
      const inUse = [...state.applied].some(([sessionKey, keys]) => {
        if (!keys.includes(key)) return false;
        const [node, sessionId] = JSON.parse(sessionKey);
        if (node !== context.node) return false;
        const session = demoNodes.find(item => item.id === node)?.sessions.find(item => item.id === sessionId);
        if (context.project === '*') return !state.installed.get(projectKey(node, session?.project))?.has(key);
        return session?.project === context.project && !state.installed.get(projectKey(node, '*'))?.has(key);
      });
      genericDialog('移除安装记录？',
        `<p class="flow-lead">${escape(itemFor(key).name)} · ${escape(nodeName(context.node))} · ${context.project === '*' ? '节点全局' : escape(context.project)}</p><p>${inUse ? '这个范围的工具正在被演示会话使用。请先调整相关会话的工具组合。' : `将移除此${context.project === '*' ? '全局' : '项目'}范围的演示安装记录。`}</p>`,
        '<button class="button" data-close>取消</button>' + button('confirm-uninstall', '确认移除', `data-key="${escape(key)}" data-removal="${escape(removal)}" ${inUse ? 'disabled' : ''}`, true));
    }
    if (action === 'confirm-uninstall') {
      state.installed.get(target.dataset.removal)?.delete(key);
      $('#flow-dialog').close();
      renderResources();
    }
    if (action.startsWith('run-')) {
      const sessionKey = scopeKey(currentNode, currentSession);
      const next = { 'run-wait': 'waiting', 'run-complete': 'completed', 'run-stop': 'stopped', 'run-disconnect': 'disconnected', 'run-reconnect': 'completed' }[action];
      if (next) state.runs.set(sessionKey, next);
      if (action === 'run-idle') state.runs.delete(sessionKey);
      if (action === 'run-approve') genericDialog('确认工具操作 · 演示',
        '<p class="flow-lead">开发诊断请求检查当前项目的构建状态。</p><div class="flow-box"><h3>请求范围</h3><p>目标为当前会话所属项目。请按请求内容决定是否继续。</p></div>',
        button('run-reject', '拒绝') + button('run-allow', '允许本次', '', true));
      if (action === 'run-allow' || action === 'run-reject') {
        state.runs.set(sessionKey, action === 'run-allow' ? 'running' : 'stopped');
        $('#flow-dialog').close();
      }
      const session = currentSessionRecord();
      if (session) session.state = ({
        running: 'running', waiting: 'approval', completed: 'completed',
        stopped: 'stopped', disconnected: 'unknown',
      })[state.runs.get(sessionKey)] || 'idle';
      renderSessionLists();
      syncWorkspaceTools();
    }
    if (action === 'review-changes') { showTab('changes'); }
  });

  document.addEventListener('change', event => {
    const target = event.target;
    if (target.id === 'installed-scope') {
      [state.installedNode, state.installedProject] = JSON.parse(target.value);
      renderResources();
      syncRoute();
      return;
    }
    const plan = state.plan;
    if (!plan || plan.stage !== 'configure') return;
    if (target.id === 'work-project') {
      const project = projectOptions().find(item => item.key === target.value);
      plan.node = project.node;
      plan.project = project.name;
    }
    if (target.id === 'install-node') {
      plan.node = target.value;
      plan.project = projectOptions().find(project => project.node === plan.node)?.name || '';
    }
    if (target.name === 'install-scope') plan.scope = target.value;
    if (target.id === 'install-project') plan.project = target.value;
    if (target.id === 'work-destination') plan.destination = target.value;
    if (target.id === 'work-model') plan.model = target.value;
    if (target.id === 'work-scenario') plan.scenario = target.value;
    if ((target.id.startsWith('work-') && target.id !== 'work-intent') || target.id.startsWith('install-') || target.name === 'install-scope') renderWork();
  });
  document.addEventListener('input', event => {
    if (event.target.id === 'work-intent' && state.plan) state.plan.intent = event.target.value;
    if (['author-name', 'author-kind', 'author-id', 'author-source', 'author-version'].includes(event.target.id)) invalidateDraft();
    if (event.target.id === 'connect-address') event.target.setCustomValidity('');
  });
  document.addEventListener('clarity:view', () => {
    syncResourceNavigation();
    if (document.body.dataset.view === 'resources') renderResources();
  });
  document.addEventListener('clarity:workspace', event => {
    if (event.detail === 'sent') {
      const key = scopeKey(currentNode, currentSession);
      if (!state.applied.has(key)) state.applied.set(key, [...(state.defaults.get(projectKey(currentNode, currentProject())) || [])]);
      state.runs.set(key, 'running');
      if (currentSessionRecord()) currentSessionRecord().state = 'running';
      renderSessionLists();
    }
    syncWorkspaceTools();
  });
  $('.welcome-heading').insertAdjacentHTML('beforeend',
    '<button class="welcome-tool-entry" data-resource-entry="all"><i data-icon="blocks"></i>选择扩展、技能或 Package<i data-icon="arrow-right"></i></button>');
  // Add a project entry to the existing project picker without changing its draft behavior.
  $('#welcome-project-dialog').insertAdjacentHTML('beforeend', '<button class="text-button" data-flow="all-projects"><i data-icon="plus"></i>添加或选择其他项目</button>');
  document.addEventListener('click', event => {
    if (event.target.closest('[data-flow=all-projects]')) openProjectChooser();
    const projectButton = event.target.closest('[data-new-session-project]');
    if (projectButton) {
      const dialog = $('#new-session-dialog');
      dialog.dataset.node = projectButton.dataset.newSessionNode;
      dialog.dataset.project = projectButton.dataset.newSessionProject;
      $('#new-session-title').value = '';
    }
  });
  $('#new-session-dialog p:last-of-type').textContent = '新会话将使用这个项目的默认工具组合。';
  $('#new-session-dialog > .button').remove();
  $('#new-session-dialog').insertAdjacentHTML('beforeend',
    '<div class="flow-form"><label>会话名称（可选）<input id="new-session-title" maxlength="100" placeholder="也可以从第一条需求开始"></label></div><div class="flow-actions session-create-actions"><button class="button" data-close>取消</button><button class="button primary" data-flow="create-project-session">进入新会话</button></div>');
  for (const selector of ['#provider-definition-dialog', '#provider-key-dialog']) {
    const save = $(selector).querySelector('.provider-dialog-actions .primary');
    save.removeAttribute('data-toast');
    save.dataset.flow = selector.includes('definition') ? 'provider-save-definition' : 'provider-save-key';
  }
  $('#provider-key-dialog').addEventListener('close', () => $('#provider-key-dialog input').value = '');
  $('.prototype-label').replaceWith(Object.assign(document.createElement('button'), {
    className: 'prototype-flow-entry', innerHTML: '<i data-icon="play"></i>设计预览 · 流程演示',
  }));
  $('.prototype-flow-entry').dataset.flow = 'scenarios';
  $('.topbar-right').prepend($('#work-tools-bar'));
  document.querySelectorAll('.desktop-connect-action button,.mobile-connect-actions button').forEach(element => {
    element.removeAttribute('data-toast');
    element.dataset.flow = 'connect-device';
  });
  resourceKind = 'all';
  resourceSource = '公共库';
  readRoute();
  renderResources();
  syncWorkspaceTools();
  mountIcons();
})();
