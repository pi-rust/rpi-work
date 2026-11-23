// Explicit, browser-local presentation fixtures only; not a history transport.
// Default navigation remains immediate. URL fixtures exercise pending/error UI.
const historyPreview = (() => {
  const requested = new URLSearchParams(location.search).get('history');
  const mode = ['loading', 'error'].includes(requested) ? requested : null;
  const eligible = new Set();
  const loaded = new Set();
  let target;
  let phase = 'ready';
  let generation = 0;
  let timer;

  function blocked() {
    return Boolean(session && target === session.id && phase !== 'ready');
  }

  function load(fail, focusDraft = false) {
    const id = target;
    const ticket = ++generation;
    phase = 'loading';
    timer = setTimeout(() => {
      if (ticket !== generation || session?.id !== id) return;
      phase = fail ? 'error' : 'ready';
      if (!fail) loaded.add(id);
      renderTranscript();
      updateRunState();
      renderQueue();
      renderReview();
      if (!fail) {
        scrollToLatestTurn();
        if (focusDraft && document.activeElement === conversation) draft.focus({ preventScroll: true });
      }
    }, 800);
  }

  function select(thread) {
    if (thread?.id === target && phase !== 'ready') return;
    clearTimeout(timer);
    generation += 1;
    target = thread?.id;
    phase = 'ready';
    if (mode && target && eligible.has(target) && !loaded.has(target)) load(mode === 'error');
  }

  function render(root) {
    conversation.dataset.history = blocked() ? phase : 'ready';
    root.setAttribute('aria-busy', String(blocked() && phase === 'loading'));
    if (!blocked()) return false;
    const card = element('section', 'history-state');
    card.setAttribute('role', phase === 'error' ? 'alert' : 'status');
    const heading = element('h2', '', phase === 'error' ? '未能加载会话记录' : '正在加载会话记录');
    const description = element('p', '', phase === 'error'
      ? '草稿和附件仍保留在当前会话。重试成功后即可继续发送。'
      : '正在准备这段对话。你可以先编辑草稿，加载完成后再发送。');
    card.append(heading, description);
    if (phase === 'error') card.append(actionButton('重试加载', 'retry-history', 'history-retry'));
    else {
      const skeleton = element('div', 'history-skeleton');
      skeleton.setAttribute('aria-hidden', 'true');
      for (let index = 0; index < 3; index += 1) skeleton.append(element('span'));
      card.append(skeleton);
    }
    card.append(element('small', 'history-fixture-note', '本地展示演示 · 不会请求节点或模型'));
    root.replaceChildren(card);
    return true;
  }

  document.addEventListener('click', event => {
    if (!event.target.closest('[data-action="retry-history"]') || !blocked() || phase !== 'error') return;
    conversation.focus({ preventScroll: true });
    load(false, true);
    renderTranscript();
    updateRunState();
    renderReview();
  });

  return { blocked, select, render, seed: threads => threads.forEach(thread => eligible.add(thread.id)), phase: () => phase };
})();
