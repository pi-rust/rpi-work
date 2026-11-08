// Scoped non-modal picker: top-layer when available, fixed-position fallback otherwise.
(menuId, anchorSelector, instance, notify) => {
  const menu = document.getElementById(menuId);
  const anchor = document.querySelector(anchorSelector);
  if (!menu || !anchor) { notify('close'); return; }
  const registry = window.__rpiComposerPickers ||= new Map();
  // Fallback browsers also get one open picker, not two overlapping menus.
  for (const dispose of [...registry.values()]) dispose(true);
  let disposed = false, frame = 0;
  const native = typeof menu.showPopover === 'function';
  const viewport = window.visualViewport;
  const visible = element => !element.disabled && element.getClientRects().length > 0;
  const options = () => [...menu.querySelectorAll('.composer-model-option')].filter(visible);
  const focusables = () => [...menu.querySelectorAll('button,input,summary,[tabindex="0"]')].filter(visible);
  function position() {
    frame = 0;
    if (disposed || !menu.isConnected || !anchor.isConnected) return;
    const rect = anchor.getBoundingClientRect();
    const style = getComputedStyle(menu);
    const number = name => parseFloat(style.getPropertyValue(name)) || 0;
    const leftEdge = (viewport?.offsetLeft || 0) + Math.max(8, number('--app-safe-left'));
    const topEdge = (viewport?.offsetTop || 0) + Math.max(8, number('--app-safe-top'));
    const rightEdge = (viewport?.offsetLeft || 0) + (viewport?.width || innerWidth) - Math.max(8, number('--app-safe-right'));
    const bottomEdge = (viewport?.offsetTop || 0) + (viewport?.height || innerHeight) - Math.max(8, number('--app-safe-bottom'));
    menu.style.width = `${Math.max(0, Math.min(number('--picker-width') || 320, rightEdge - leftEdge))}px`;
    const above = Math.max(0, Math.min(bottomEdge, rect.top - 6) - topEdge);
    const below = Math.max(0, bottomEdge - Math.max(topEdge, rect.bottom + 6));
    const upward = above >= Math.min(240, menu.scrollHeight) || above >= below;
    menu.style.maxHeight = `${Math.min(420, upward ? above : below)}px`;
    const height = menu.getBoundingClientRect().height;
    const width = menu.getBoundingClientRect().width;
    menu.style.left = `${Math.max(leftEdge, Math.min(rect.left, rightEdge - width))}px`;
    menu.style.top = `${Math.max(topEdge, Math.min(upward ? rect.top - 6 - height : rect.bottom + 6, bottomEdge - height))}px`;
    menu.style.visibility = 'visible';
  }
  function schedule() { if (!frame && !disposed) frame = requestAnimationFrame(position); }
  function dispose(close = false, restore = false) {
    if (disposed) return;
    disposed = true;
    cancelAnimationFrame(frame);
    resize.disconnect();
    mutation.disconnect();
    anchor.removeEventListener('click', triggerClick, true);
    document.removeEventListener('pointerdown', pointer, true);
    document.removeEventListener('keydown', keydown, true);
    document.removeEventListener('focusin', focusin, true);
    window.removeEventListener('resize', position);
    window.removeEventListener('scroll', scroll, true);
    viewport?.removeEventListener('resize', position);
    viewport?.removeEventListener('scroll', position);
    menu.removeEventListener('toggle', toggle);
    registry.delete(instance);
    const active = document.activeElement;
    if (native && menu.matches(':popover-open')) menu.hidePopover();
    if (!native) menu.style.display = 'none';
    if (restore || menu.contains(active) || active === document.body) {
      if (anchor.isConnected && !anchor.disabled) anchor.focus({ preventScroll: true });
    }
    if (close) notify('close');
  }
  function triggerClick(event) {
    event.preventDefault(); event.stopImmediatePropagation(); dispose(true, true);
  }
  function pointer(event) {
    if (!menu.contains(event.target) && !anchor.contains(event.target)) {
      // Preserve the clicked input/button's default focus, including the other picker.
      dispose(true);
    }
  }
  function toggle(event) { if (event.newState === 'closed') dispose(true); }
  function focusin(event) {
    if (!menu.contains(event.target) && !anchor.contains(event.target)) dispose(true);
  }
  function scroll(event) {
    // Reading this menu must not dismiss it; shell scrolling repositions the anchor.
    if (!menu.contains(event.target)) schedule();
  }
  function keydown(event) {
    if (event.isComposing || event.keyCode === 229) return;
    if (event.key === 'Escape') {
      event.preventDefault(); event.stopPropagation(); dispose(true, true); return;
    }
    if (!menu.contains(document.activeElement)) return;
    const input = document.activeElement.matches('input,textarea');
    const list = options();
    if (['ArrowDown', 'ArrowUp'].includes(event.key) || (!input && ['Home', 'End'].includes(event.key))) {
      if (!list.length) return;
      event.preventDefault();
      const current = list.indexOf(document.activeElement);
      let index;
      if (event.key === 'Home') index = 0;
      else if (event.key === 'End') index = list.length - 1;
      else if (current < 0) index = event.key === 'ArrowUp' ? list.length - 1 : 0;
      else index = (current + (event.key === 'ArrowDown' ? 1 : -1) + list.length) % list.length;
      list[index].focus({ preventScroll: true });
      list[index].scrollIntoView({ block: 'nearest' });
    } else if (event.key === 'Tab') {
      const all = focusables();
      const boundary = event.shiftKey ? all[0] : all[all.length - 1];
      if (document.activeElement === boundary) {
        // Let Tab continue in document order from the invoking control, never trap it.
        anchor.focus({ preventScroll: true });
        dispose(true);
      }
    }
  }
  const resize = new ResizeObserver(schedule);
  const mutation = new MutationObserver(schedule);
  registry.set(instance, dispose);
  if (native) menu.showPopover();
  else { menu.removeAttribute('popover'); menu.style.display = 'flex'; }
  position();
  menu.addEventListener('toggle', toggle);
  anchor.addEventListener('click', triggerClick, true);
  document.addEventListener('pointerdown', pointer, true);
  document.addEventListener('keydown', keydown, true);
  document.addEventListener('focusin', focusin, true);
  window.addEventListener('resize', position);
  window.addEventListener('scroll', scroll, true);
  viewport?.addEventListener('resize', position);
  viewport?.addEventListener('scroll', position);
  resize.observe(menu);
  resize.observe(anchor);
  // Sibling model labels and growing composer content can move a same-size anchor.
  const composer = anchor.closest('.composer');
  if (composer) {
    resize.observe(composer);
    for (const control of composer.querySelectorAll('.composer-model-picker, .composer-thinking-select')) resize.observe(control);
  }
  mutation.observe(menu, { childList: true, subtree: true, characterData: true, attributes: true, attributeFilter: ['open', 'disabled'] });
  // Keep the picker usable without opening the soft keyboard on mobile.
  const initial = options().find(e => e.getAttribute('aria-pressed') === 'true') || options()[0] || focusables().find(e => !e.matches('input[type="search"]'));
  initial?.focus({ preventScroll: true });
}
