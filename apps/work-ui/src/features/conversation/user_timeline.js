// `id` and `scope` are independently JSON-encoded by the Rust mount effect.
(() => {
  const registry = window.__rpiUserTimelines ||= new Map();
  const root = document.getElementById(id);
  const previous = registry.get(id);
  if (previous && previous.scope === scope && previous.root === root) {
    previous.refresh();
    return;
  }
  previous?.dispose();
  if (!root || root.dataset.sessionScope !== scope) return;
  const container = root.closest('.transcript-frame');
  const transcript = container?.querySelector('.transcript');
  const nav = root.querySelector('.timeline-panel');
  const list = root.querySelector('.timeline-list');
  if (!transcript || !nav || !list || transcript.dataset.sessionScope !== scope) return;

  let disposed = false;
  let frame = 0;
  let structureFrame = 0;
  let buttons = [];
  let ordered = [];
  let targets = new Map();
  let active = null;
  let focused = null;
  let previewButton = null;
  let pointerInside = false;
  let focusInside = false;
  let browseUntil = 0;
  let drag = null;
  let ignorePointerClick = false;
  let releasedAt = null;
  let earlierUntil = 0;
  let railAnchor = null;
  let railStep = 0;
  const removers = [];
  const tooltip = document.createElement('div');
  tooltip.id = `${id}-preview`;
  tooltip.className = 'timeline-preview';
  tooltip.setAttribute('role', 'tooltip');
  tooltip.hidden = true;
  const previewIndex = document.createElement('span');
  previewIndex.className = 'timeline-preview-index';
  const previewLabel = document.createElement('span');
  previewLabel.className = 'timeline-label';
  tooltip.append(previewIndex, previewLabel);
  root.append(tooltip);

  const valid = () => !disposed && root.isConnected && transcript.isConnected
    && root.dataset.sessionScope === scope && transcript.dataset.sessionScope === scope
    && root.closest('.transcript-frame') === container && container.contains(transcript);
  const on = (node, name, listener, options) => {
    node.addEventListener(name, listener, options);
    removers.push(() => node.removeEventListener(name, listener, options));
  };
  const tick = target => {
    const button = target instanceof Element ? target.closest('.timeline-node') : null;
    return button && list.contains(button) && buttons.includes(button) ? button : null;
  };
  const hidePreview = () => {
    previewButton?.removeAttribute('aria-describedby');
    previewButton = null;
    tooltip.hidden = true;
  };
  const rememberRail = () => {
    const step = buttons[0]?.offsetHeight || 0;
    // A resize-induced scroll event can arrive before ResizeObserver. Keep
    // the old logical anchor until that observer reconciles the new step.
    if (railStep && step !== railStep) return;
    railStep = step;
    const top = list.getBoundingClientRect().top;
    const button = buttons.find(button => button.getBoundingClientRect().bottom > top + .5);
    railAnchor = button ? { key: button.dataset.timelineTarget, offset: button.getBoundingClientRect().top - top } : null;
  };
  const loadEarlier = automatic => {
    if (!valid()) return;
    const load = root.querySelector('.timeline-load-earlier');
    if (!load || load.disabled || (automatic && load.dataset.autoLoad !== 'true')) return;
    if (root.dataset.nativeTimeline === 'true') {
      root.dispatchEvent(new CustomEvent('rpi-timeline-command', { detail: { action: 'load' } }));
      return;
    }
    const pager = container.querySelector('.history-pager');
    pager?.dispatchEvent(new CustomEvent(automatic ? 'rpi-history-load-auto' : 'rpi-history-load', { detail: scope }));
  };
  const maybeLoadEarlier = () => {
    if (!earlierUntil || performance.now() > earlierUntil || list.scrollTop > 1) return;
    earlierUntil = 0; // One edge arrival consumes intent, never replay it after a response.
    loadEarlier(true);
  };
  const browseEarlier = () => {
    browseUntil = performance.now() + 1000;
    earlierUntil = performance.now() + 700;
    maybeLoadEarlier();
  };
  const reveal = button => {
    // Only the rail moves. Never use DOM scrolling APIs that affect ancestors.
    const top = button.getBoundingClientRect().top - list.getBoundingClientRect().top + list.scrollTop;
    if (top < list.scrollTop) list.scrollTop = top;
    else if (top + button.offsetHeight > list.scrollTop + list.clientHeight)
      list.scrollTop = top + button.offsetHeight - list.clientHeight;
    rememberRail();
  };
  const showPreview = button => {
    if (!valid() || !button?.isConnected || !buttons.includes(button)) return;
    if (previewButton !== button) hidePreview();
    previewButton = button;
    previewIndex.textContent = `${buttons.indexOf(button) + 1} / ${buttons.length}`;
    previewLabel.textContent = button.dataset.preview || '';
    tooltip.style.maxWidth = `${Math.min(240, Math.max(0, transcript.clientWidth - 64))}px`;
    tooltip.hidden = false;
    const box = root.getBoundingClientRect();
    const buttonBox = button.getBoundingClientRect();
    // Keep both ends inside the rail AND viewport, even on short windows.
    const low = Math.max(0, -box.top);
    const high = Math.min(box.height, window.innerHeight - box.top);
    tooltip.style.maxHeight = `${Math.max(0, high - low)}px`;
    const half = tooltip.offsetHeight / 2;
    const center = buttonBox.top + buttonBox.height / 2 - box.top;
    tooltip.style.top = `${Math.max(low + half, Math.min(center, high - half))}px`;
    button.setAttribute('aria-describedby', tooltip.id);
  };
  const rove = () => {
    const selected = buttons.includes(focused) ? focused : buttons.includes(active) ? active : buttons[0];
    for (const button of buttons) {
      const value = button === selected ? 0 : -1;
      if (button.tabIndex !== value) button.tabIndex = value;
    }
  };
  const setActive = button => {
    const changed = active !== button;
    if (changed) {
      active?.removeAttribute('aria-current');
      active = button;
    }
    if (active && active.getAttribute('aria-current') !== 'location') active.setAttribute('aria-current', 'location');
    rove();
    if (changed && active && !pointerInside && !focusInside && !drag && performance.now() >= browseUntil)
      reveal(active);
  };
  const update = () => {
    frame = 0;
    if (!valid()) { dispose(); return; }
    // Use the same 12px reading inset as navigateTo (plus subpixel tolerance).
    // A 96px probe can otherwise skip the clicked input when rows are short.
    const top = transcript.getBoundingClientRect().top + Math.min(13, transcript.clientHeight * .15);
    let left = 0;
    let right = ordered.length;
    while (left < right) {
      const middle = (left + right) >>> 1;
      if (ordered[middle].target.getBoundingClientRect().top <= top) left = middle + 1;
      else right = middle;
    }
    setActive(ordered[Math.max(0, left - 1)]?.button || null);
    if (previewButton) showPreview(previewButton);
  };
  const schedule = () => {
    if (!valid()) { dispose(); return; }
    if (!frame) frame = requestAnimationFrame(update);
  };
  const status = text => {
    const node = root.querySelector('.timeline-navigation-status');
    if (node) node.textContent = text;
  };
  const navigate = (button, loadWindow = true) => {
    if (!valid()) return false;
    const key = button?.dataset.timelineTarget;
    const target = targets.get(key);
    if (!target?.isConnected || !transcript.contains(target)) {
      if (loadWindow && root.dataset.nativeTimeline === 'true' && key?.startsWith('entry:')) {
        earlierUntil = 0;
        root.dispatchEvent(new CustomEvent('rpi-timeline-command', { detail: { action: 'navigate', target: key.slice(6) } }));
        setActive(button);
        hidePreview();
        return true;
      }
      return false;
    }
    root.dispatchEvent(new CustomEvent('rpi-timeline-command', { detail: { action: 'live' } }));
    earlierUntil = 0;
    // Notify the host before EVERY navigation, cancelling automatic paging intent.
    container.querySelector('.history-pager')?.dispatchEvent(new Event('rpi-history-navigation'));
    let success = false;
    for (const instance of window.__rpiScrollFollow?.instances?.values() || []) {
      if (instance.scope === scope && typeof instance.navigateTo === 'function') {
        success = instance.navigateTo(target) === true;
        if (success) break;
      }
    }
    status(success ? '' : '消息导航尚未就绪，请重试。');
    if (success) setActive(button);
    hidePreview();
    return success;
  };
  const stopDrag = () => {
    const gesture = drag;
    if (!gesture) return;
    drag = null;
    releasedAt = { x: gesture.x, y: gesture.y };
    cancelAnimationFrame(gesture.frame);
    delete list.dataset.scrubbing;
    if (list.hasPointerCapture(gesture.pointerId)) list.releasePointerCapture(gesture.pointerId);
    browseUntil = performance.now() + 1000;
  };
  const cancelGesture = () => { stopDrag(); hidePreview(); };
  const scrub = (gesture, elapsed = 0) => {
    if (!valid()) { dispose(); return; }
    const box = list.getBoundingClientRect();
    if (!box.height || !buttons.length) return;
    const edge = Math.min(24, box.height / 4);
    const direction = gesture.y < box.top + edge ? -1 : gesture.y > box.bottom - edge ? 1 : 0;
    list.scrollTop += direction * 480 * elapsed / 1000;
    rememberRail();
    if (direction < 0 && gesture.moved) browseEarlier();
    const y = Math.max(box.top, Math.min(gesture.y, box.bottom - 1));
    // Binary search real button geometry, including mobile sizes and load control.
    let left = 0;
    let right = buttons.length;
    while (left < right) {
      const middle = (left + right) >>> 1;
      if (buttons[middle].getBoundingClientRect().bottom <= y) left = middle + 1;
      else right = middle;
    }
    const button = buttons[Math.min(left, buttons.length - 1)];
    if (gesture.last !== button) {
      gesture.last = button;
      // Native summary-only ticks preview while scrubbing; load one window on release.
      navigate(button, false);
    }
    showPreview(button);
  };

  const sizes = new ResizeObserver(entries => {
    // Desktop/mobile use different tick heights. Keeping the old pixel
    // scrollTop can clip both edge ticks (only nine whole ticks remain).
    // Preserve the visible input and its offset instead; never scroll the
    // transcript or require a click to reveal the full default rail.
    if (valid() && railAnchor && entries.some(entry => entry.target === root)) {
      const button = buttons.find(button => button.dataset.timelineTarget === railAnchor.key);
      if (button) list.scrollTop += button.getBoundingClientRect().top - list.getBoundingClientRect().top - railAnchor.offset;
      railStep = buttons[0]?.offsetHeight || 0;
      rememberRail();
    }
    schedule();
  });
  const observedRows = new Set();
  sizes.observe(transcript);
  sizes.observe(root);
  const refresh = () => {
    structureFrame = 0;
    if (!valid()) { dispose(); return; }
    const nextButtons = Array.from(list.querySelectorAll('.timeline-node[data-timeline-target]'));
    const wanted = new Set(nextButtons.map(button => button.dataset.timelineTarget));
    const nextTargets = new Map();
    // IDs are compared as data, never interpolated into CSS selectors.
    for (const target of container.querySelectorAll('.transcript [data-user-message-key]')) {
      const key = target.dataset.userMessageKey;
      if (transcript.contains(target) && wanted.has(key) && !nextTargets.has(key)) nextTargets.set(key, target);
    }
    const changed = nextButtons.length !== buttons.length || nextButtons.some((button, index) => button !== buttons[index])
      || nextTargets.size !== targets.size || [...nextTargets].some(([key, target]) => targets.get(key) !== target);
    if (changed) {
      const prepended = buttons.length && nextButtons.indexOf(buttons[0]) > 0;
      const anchor = railAnchor;
      cancelGesture();
      buttons = nextButtons;
      targets = nextTargets;
      const byKey = new Map(buttons.map(button => [button.dataset.timelineTarget, button]));
      if (prepended && anchor) {
        const button = byKey.get(anchor.key);
        if (button) list.scrollTop += button.getBoundingClientRect().top - list.getBoundingClientRect().top - anchor.offset;
      }
      rememberRail();
      ordered = [...targets].map(([key, target]) => ({ target, button: byKey.get(key) }));
      if (!buttons.includes(focused)) focused = null;
    }
    // Observe message-level rows, not individual images/markdown descendants.
    const rows = new Set([...transcript.children]);
    for (const holder of transcript.querySelectorAll('.message-list')) {
      rows.add(holder);
      for (const row of holder.children) rows.add(row);
    }
    for (const row of transcript.querySelectorAll('.message-turn')) rows.add(row);
    for (const row of observedRows) if (!rows.has(row)) { sizes.unobserve(row); observedRows.delete(row); }
    for (const row of rows) if (!observedRows.has(row)) { sizes.observe(row); observedRows.add(row); }
    if (previewButton) showPreview(previewButton);
    schedule();
  };
  const scheduleStructure = () => {
    if (!valid()) { dispose(); return; }
    if (!structureFrame) structureFrame = requestAnimationFrame(refresh);
  };
  const railObserver = new MutationObserver(scheduleStructure);
  railObserver.observe(nav, { childList: true, subtree: true, attributes: true, attributeFilter: ['data-timeline-target', 'data-preview'] });
  const transcriptObserver = new MutationObserver(records => {
    // Ignore streamed text and markdown churn. Only row structure/target identity
    // can invalidate the map; ResizeObserver covers text/image layout changes.
    if (records.some(record => record.type === 'attributes'
      || record.target === transcript || record.target instanceof Element && record.target.matches('.message-list')
      || [...record.addedNodes, ...record.removedNodes].some(node => node instanceof Element
        && (node.matches('[data-user-message-key], .message-turn, .message-list')
          || node.querySelector('[data-user-message-key], .message-turn, .message-list'))))) scheduleStructure();
  });
  transcriptObserver.observe(transcript, { childList: true, subtree: true, attributes: true, attributeFilter: ['data-user-message-key', 'data-session-scope'] });
  // A local, structural-only observer also cleans up removed roots/scopes even
  // before Rust's drop eval arrives. Never observe the entire document.
  const frameObserver = new MutationObserver(() => { if (!valid()) dispose(); });
  frameObserver.observe(container, { childList: true, attributes: true, subtree: false, attributeFilter: ['data-session-scope'] });

  on(transcript, 'scroll', schedule, { passive: true });
  on(window, 'resize', schedule, { passive: true });
  on(root, 'pointerenter', event => {
    if (!valid()) return;
    if (event.pointerType !== 'touch') pointerInside = true;
  });
  on(root, 'pointerleave', () => {
    pointerInside = false;
    if (!drag) { releasedAt = null; hidePreview(); }
  });
  const hover = event => {
    if (!valid() || drag || event.pointerType === 'touch') return;
    if (releasedAt?.x === event.clientX && releasedAt?.y === event.clientY) return;
    releasedAt = null;
    const button = tick(event.target);
    if (button) showPreview(button);
    else if (!tooltip.contains(event.target)) hidePreview();
  };
  on(list, 'pointerover', hover);
  on(list, 'pointermove', event => {
    if (!drag) { hover(event); return; }
    if (!valid()) { dispose(); return; }
    const gesture = drag;
    if (event.pointerId !== gesture.pointerId) return;
    gesture.x = event.clientX;
    gesture.y = event.clientY;
    if (event.buttons !== 1) { cancelGesture(); return; }
    if (!gesture.moved && Math.hypot(gesture.x - gesture.startX, gesture.y - gesture.startY) < 4) return;
    gesture.moved = true;
    list.dataset.scrubbing = 'true';
    if (gesture.frame) return;
    const step = time => {
      if (drag !== gesture) return;
      if (!valid()) { dispose(); return; }
      const elapsed = gesture.time ? Math.min(32, time - gesture.time) : 0;
      gesture.time = time;
      scrub(gesture, elapsed);
      if (drag === gesture) gesture.frame = requestAnimationFrame(step);
    };
    gesture.frame = requestAnimationFrame(step);
  });
  on(list, 'pointerdown', event => {
    if (drag) { cancelGesture(); return; }
    if (!valid() || !event.isPrimary || event.button !== 0 || event.buttons !== 1) return;
    const button = tick(event.target);
    if (!button) return;
    event.preventDefault(); // Preserve composer draft focus and window position.
    ignorePointerClick = true;
    releasedAt = null;
    drag = { pointerId: event.pointerId, startX: event.clientX, startY: event.clientY,
      x: event.clientX, y: event.clientY, moved: false, start: button, last: null, frame: 0, time: 0 };
    list.setPointerCapture(event.pointerId);
    showPreview(button);
  });
  // Keep compatibility mouse focus from stealing the composer selection.
  // Pointer capture handles touch scrubbing only within this narrow rail.
  on(list, 'mousedown', event => { if (valid() && tick(event.target)) event.preventDefault(); });
  on(list, 'pointerup', event => {
    const gesture = drag;
    if (!gesture || event.pointerId !== gesture.pointerId) return;
    if (!valid() || event.button !== 0 || event.buttons !== 0) { cancelGesture(); return; }
    gesture.x = event.clientX;
    gesture.y = event.clientY;
    if (gesture.moved) {
      scrub(gesture);
      if (!targets.has(gesture.last?.dataset.timelineTarget)) navigate(gesture.last);
    } else navigate(gesture.start);
    cancelGesture();
  });
  for (const name of ['pointercancel', 'lostpointercapture']) on(list, name, event => {
    if (event.pointerId === drag?.pointerId) cancelGesture();
  });
  on(window, 'blur', () => { pointerInside = false; focusInside = false; cancelGesture(); });
  on(root, 'click', event => {
    if (!valid()) return;
    const button = tick(event.target);
    if (button) {
      if (event.detail !== 0 && ignorePointerClick) {
        ignorePointerClick = false;
        event.preventDefault();
        event.stopPropagation();
        return;
      }
      navigate(button); // Includes Enter/Space and assistive activation (detail 0).
      return;
    }
    const load = event.target instanceof Element ? event.target.closest('.timeline-load-earlier') : null;
    if (load && root.contains(load) && !load.disabled) loadEarlier(false);
  });
  on(nav, 'focusin', event => {
    if (!valid()) return;
    focusInside = true;
    const button = tick(event.target);
    focused = button;
    rove();
    if (button) showPreview(button);
  });
  on(nav, 'focusout', event => {
    if (event.relatedTarget instanceof Node && nav.contains(event.relatedTarget)) return;
    focusInside = false;
    focused = null;
    hidePreview();
    rove();
  });
  on(list, 'keydown', event => {
    if (!valid() || event.isComposing) return;
    const button = tick(event.target);
    if (!button) return;
    const index = buttons.indexOf(button);
    let next;
    switch (event.key) {
      case 'ArrowUp': next = Math.max(0, index - 1); break;
      case 'ArrowDown': next = Math.min(buttons.length - 1, index + 1); break;
      case 'Home': next = 0; break;
      case 'End': next = buttons.length - 1; break;
      default: return;
    }
    event.preventDefault();
    const target = buttons[next];
    focused = target;
    rove();
    reveal(target);
    if (event.key === 'ArrowUp' || event.key === 'Home') browseEarlier();
    else earlierUntil = 0;
    target.focus({ preventScroll: true });
    showPreview(target);
  });
  on(document, 'keydown', event => {
    if (!valid()) { dispose(); return; }
    if (event.key === 'Escape' && !event.isComposing && (previewButton || drag)) {
      event.stopPropagation();
      cancelGesture();
    }
  }, true);
  on(list, 'wheel', event => {
    if (!valid()) return;
    browseUntil = performance.now() + 1000;
    if (event.deltaY < 0) browseEarlier();
    else earlierUntil = 0;
    // Do not bubble a rail wheel into transcript/ScrollFollow user-intent handlers.
    event.stopPropagation();
  }, { passive: true });
  on(list, 'scroll', () => {
    if (!valid()) { dispose(); return; }
    rememberRail();
    maybeLoadEarlier();
    if (drag) return;
    if (previewButton === focused && focused) showPreview(focused);
    else hidePreview();
  }, { passive: true });

  function dispose() {
    if (disposed) return;
    disposed = true;
    cancelGesture();
    cancelAnimationFrame(frame);
    cancelAnimationFrame(structureFrame);
    railObserver.disconnect();
    transcriptObserver.disconnect();
    frameObserver.disconnect();
    sizes.disconnect();
    root.__rpiNativeTimelineDispose?.();
    delete root.__rpiNativeTimelineDispose;
    for (const remove of removers) remove();
    tooltip.remove();
    for (const button of buttons) {
      button.removeAttribute('aria-current');
      button.removeAttribute('aria-describedby');
    }
    targets.clear();
    observedRows.clear();
    ordered = [];
    buttons = [];
    if (registry.get(id)?.dispose === dispose) registry.delete(id);
  }
  registry.set(id, { scope, root, dispose, refresh: scheduleStructure });
  refresh();
})();
