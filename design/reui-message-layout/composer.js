// Composer shortcuts, file mentions and per-chat goals share the existing chat state.
(() => {
  const menu = $('#composer-menu');
  const addButton = $('.add-control');
  const goalDialog = $('#goal-dialog');
  const goalForm = $('#goal-form');
  const goalInput = $('#goal-input');
  const mode = $('#collaboration-mode');
  let choices = [];
  let highlighted = 0;
  let tokenStart = 0;
  let fromAdd = false;

  function closeMenu() {
    menu.hidden = true;
    fromAdd = false;
    choices = [];
    addButton.setAttribute('aria-expanded', 'false');
    draft.removeAttribute('aria-activedescendant');
  }

  function highlight(index) {
    highlighted = index;
    for (const [i, option] of Array.from(menu.children).entries()) {
      option.setAttribute('aria-selected', String(i === index));
    }
    const option = menu.children[index];
    if (option) {
      draft.setAttribute('aria-activedescendant', option.id);
      option.scrollIntoView({ block: 'nearest' });
    }
  }

  function showMenu(items) {
    if (!items.length) { closeMenu(); return; }
    choices = items;
    menu.replaceChildren(...items.map((item, index) => {
      const option = element('button', 'composer-option');
      option.id = 'composer-option-' + index;
      option.type = 'button';
      option.tabIndex = -1;
      option.setAttribute('role', 'option');
      option.innerHTML = icon(item.icon);
      option.append(element('span', '', item.title));
      if (item.detail) option.append(element('small', '', item.detail));
      option.addEventListener('click', () => select(index));
      return option;
    }));
    menu.hidden = false;
    addButton.setAttribute('aria-expanded', 'true');
    highlight(0);
  }

  function openGoal() {
    const goal = state().goal;
    $('#goal-title').textContent = goal ? '编辑目标' : '设定目标';
    goalInput.value = goal?.text ?? '';
    $('button[type="submit"]', goalForm).textContent = goal ? '保存目标' : '开始目标';
    $('[data-composer-action="clear-goal"]').hidden = !goal;
    goalDialog.showModal();
    goalInput.focus();
  }

  function planMode() {
    mode.value = mode.value === 'plan' ? 'code' : 'plan';
    mode.dispatchEvent(new Event('change', { bubbles: true }));
    draft.focus();
  }

  function commandOptions() {
    const actions = [
      { title: '/goal', detail: '设定目标', icon: 'target', run: openGoal },
      { title: '/plan', detail: '切换计划模式', icon: 'list-checks', run: planMode },
      { title: '/status', detail: '查看会话上下文', icon: 'message-circle', run: () => $('[data-action="show-context"]').click() },
      { title: '/new', detail: '新对话', icon: 'compose', run: () => $('[data-action="new"]').click() },
    ];
    if (session?.files?.length) actions.push({ title: '/review', detail: '审查当前改动', icon: 'file-diff', run: () => $('[data-action="run-review"]').click() });
    return actions;
  }

  function fileOptions(query = '') {
    const folder = sourceFolders.get(session?.project ?? welcomeProject);
    const candidates = folder?.enabled ? [...folder.entries, ...files] : files;
    const options = [];
    const matched = new Set();
    for (const file of candidates) {
      const path = file.path;
      if (matched.has(path) || !path.toLowerCase().includes(query)) continue;
      matched.add(path);
      options.push({title: file.name, detail: path, icon: 'code-xml', run() {
        if (!state().attachments.includes(path)) state().attachments.push(path);
        renderComposer(); saveHistory(); draft.focus();
      }});
      if (options.length === 50) break;
    }
    return options;
  }

  function queryAtCursor() {
    if (draft.selectionStart !== draft.selectionEnd) return null;
    const leading = draft.value.slice(0, draft.selectionStart);
    const match = /(?:^|\s)(@[^\s@]*|\/[^\s/]*)$/.exec(leading);
    if (!match || (match[1][0] === '/' && leading.trimStart() !== match[1])) return null;
    return match[1];
  }

  function refreshQuery() {
    const query = queryAtCursor();
    if (!query) { closeMenu(); return; }
    fromAdd = false;
    tokenStart = draft.selectionStart - query.length;
    const isFile = query.startsWith('@');
    const needle = query.slice(1).toLowerCase();
    showMenu(isFile ? fileOptions(needle) : commandOptions().filter(item => (item.title + item.detail).toLowerCase().includes(needle)));
  }

  function select(index) {
    const item = choices[index];
    if (!item) return;
    if (!fromAdd) {
      const query = queryAtCursor();
      if (!query || draft.selectionStart - query.length !== tokenStart) { closeMenu(); return; }
      draft.setRangeText('', tokenStart, draft.selectionEnd, 'end');
      draft.dispatchEvent(new Event('input'));
    }
    closeMenu();
    item.run();
  }

  addButton.addEventListener('click', () => {
    if (fromAdd && !menu.hidden) { closeMenu(); return; }
    fromAdd = true;
    showMenu([
      { title: '添加附件', icon: 'paperclip', run: () => $('#attach').click() },
      { title: '引用项目文件', detail: '@', icon: 'folder', run() { fromAdd = true; showMenu(fileOptions()); addButton.focus(); } },
      { title: state().goal ? '编辑目标' : '设定目标', detail: '/goal', icon: 'target', run: openGoal },
      { title: mode.value === 'plan' ? '退出计划模式' : '计划模式', detail: '/plan', icon: 'list-checks', run: planMode },
    ]);
  });
  draft.setAttribute('aria-autocomplete', 'list');
  draft.setAttribute('aria-controls', menu.id);
  draft.addEventListener('input', event => { if (!event.isComposing) refreshQuery(); });
  draft.addEventListener('compositionstart', closeMenu);
  draft.addEventListener('compositionend', refreshQuery);
  draft.addEventListener('keyup', event => {
    if (!event.isComposing && ['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown', 'Home', 'End', 'PageUp', 'PageDown'].includes(event.key)
      && !event.defaultPrevented && !fromAdd) {
      // Up/down navigate the open menu; only refresh when moving the caret.
      if (menu.hidden || event.shiftKey || !['ArrowUp', 'ArrowDown'].includes(event.key)) refreshQuery();
    }
  });
  draft.addEventListener('click', refreshQuery);
  document.addEventListener('pointerdown', event => {
    if (!menu.contains(event.target) && !addButton.contains(event.target) && event.target !== draft) closeMenu();
  });
  document.addEventListener('focusin', event => {
    if (event.target !== draft && event.target !== addButton && !menu.contains(event.target)) closeMenu();
  });
  form.addEventListener('keydown', event => {
    if (menu.hidden || event.isComposing || event.keyCode === 229 || event.shiftKey) return;
    if (event.key === 'Escape') { event.preventDefault(); closeMenu(); return; }
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault();
      highlight((highlighted + (event.key === 'ArrowDown' ? 1 : -1) + choices.length) % choices.length);
    } else if (event.key === 'Enter' || event.key === 'Tab') {
      event.preventDefault(); select(highlighted);
    }
  }, true);

  function renderGoal() {
    const data = state();
    const goal = data.goal;
    const progress = $('#goal-progress');
    progress.hidden = !goal;
    if (!goal) return;
    let status = goal.paused ? '已暂停' : '目标';
    if (!goal.paused && data.active?.goalId === goal.id) status = data.active.phase === 'approval' ? '等待审批' : '目标进行中';
    $('#goal-status').textContent = status;
    $('#goal-text').textContent = goal.text;
    const toggle = $('#goal-toggle');
    toggle.title = goal.paused ? '继续目标' : '暂停目标';
    toggle.setAttribute('aria-label', toggle.title);
    toggle.innerHTML = icon(goal.paused ? 'play' : 'pause');
  }

  function runGoal(goal) {
    const data = state();
    sendOrQueue({ id: crypto.randomUUID(), goalId: goal.id, prompt: goal.text, attachments: [], comments: [], model: data.model, effort: data.effort, mode: data.mode, scenario: $('#scenario').value });
    $('#scenario').value = 'normal';
  }

  function stopGoal(goal) {
    const data = state();
    data.queue = data.queue.filter(item => item.goalId !== goal.id);
    if (data.active?.goalId === goal.id) finishTurn(session, data.active, 'interrupted');
    renderQueue();
  }

  goalForm.addEventListener('submit', event => {
    event.preventDefault();
    const text = goalInput.value.trim();
    if (!text) { goalInput.focus(); return; }
    ensureThread(text);
    const data = state();
    if (data.goal) {
      data.goal.text = text;
      for (const item of data.queue) if (item.goalId === data.goal.id) item.prompt = text;
      if (data.active?.goalId === data.goal.id) {
        data.active.guidance.push({ prompt: text, attachments: [], comments: [] });
        updateTurnView(session, data.active);
      }
    } else {
      data.goal = { id: crypto.randomUUID(), text, paused: false };
      runGoal(data.goal);
    }
    goalDialog.close(); saveHistory(); renderGoal(); renderQueue(); draft.focus();
  });
  document.addEventListener('click', event => {
    const action = event.target.closest('[data-composer-action]')?.dataset.composerAction;
    if (action === 'goal') {
      if ($('.workspace').dataset.page !== 'chat') $('.rail-button[data-page="chat"]').click();
      openGoal(); return;
    }
    if (action !== 'clear-goal' && action !== 'toggle-goal') return;
    const data = state();
    const goal = data.goal;
    if (!goal) return;
    if (action === 'clear-goal') {
      stopGoal(goal); data.goal = null; goalDialog.close(); draft.focus();
    } else {
      goal.paused = !goal.paused;
      if (goal.paused) stopGoal(goal);
      else runGoal(goal);
    }
    saveHistory(); renderGoal();
  });
  document.addEventListener('sessionchange', () => { closeMenu(); renderGoal(); });
  document.addEventListener('turnviewchange', renderGoal);
  renderGoal();
})();
