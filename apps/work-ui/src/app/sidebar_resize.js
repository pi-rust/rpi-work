// Resize only the left project rail. Keep pointer-move work out of Dioxus's
// workspace render scope; CSS also clamps a saved preference on smaller screens.
const handle = document.getElementById(id);
const shell = handle?.closest('.app-shell');
const sidebar = shell?.querySelector('.sidebar');
if (handle && shell && sidebar) {
    const registry = window.__rpiSidebarResize ??= new Map();
    registry.get(id)?.();
    const key = 'rpi-work-sidebar-width-v1';
    const property = '--sidebar-preferred-width';
    const listeners = [];
    let preferred = null, active = null;
    const on = (target, type, listener) => {
        target.addEventListener(type, listener);
        listeners.push(() => target.removeEventListener(type, listener));
    };
    const limits = () => ({ min: 200, max: Math.max(200, Math.min(480, innerWidth * 0.4)) });
    const enabled = () => getComputedStyle(handle).display !== 'none';
    const update = () => {
        const bounds = limits();
        handle.setAttribute('aria-valuemin', String(bounds.min));
        handle.setAttribute('aria-valuemax', String(Math.round(bounds.max)));
        handle.setAttribute('aria-valuenow', String(Math.round(sidebar.getBoundingClientRect().width)));
    };
    const paint = () => {
        if (preferred === null) shell.style.removeProperty(property);
        else shell.style.setProperty(property, `${preferred}px`);
        update();
    };
    const setWidth = width => {
        const bounds = limits();
        preferred = Math.min(bounds.max, Math.max(bounds.min, Math.round(width)));
        paint();
    };
    const save = () => {
        try { localStorage.setItem(key, String(preferred)); } catch (_) { /* Resizing works without storage. */ }
    };
    try {
        const saved = Number(localStorage.getItem(key));
        if (Number.isFinite(saved) && saved >= 200 && saved <= 480) preferred = saved;
    } catch (_) { /* Storage may be disabled in a WebView. */ }
    paint();

    const finish = commit => {
        if (!active) return;
        const previous = active;
        active = null; // Releasing capture can synchronously dispatch lostpointercapture.
        document.documentElement.classList.remove('sidebar-resizing');
        handle.removeAttribute('data-resizing');
        if (commit && previous.moved) save();
        else { preferred = previous.before; paint(); }
        if (handle.hasPointerCapture(previous.pointer)) handle.releasePointerCapture(previous.pointer);
    };
    const move = event => {
        if (!active || event.pointerId !== active.pointer) return;
        if (Math.abs(event.clientX - active.x) > 0.5) active.moved = true;
        if (active.moved) setWidth(active.width + event.clientX - active.x);
    };
    const reset = () => {
        finish(false);
        preferred = null;
        paint();
        try { localStorage.removeItem(key); } catch (_) { /* Optional persistence. */ }
    };
    on(handle, 'pointerdown', event => {
        if (!enabled() || active || event.button !== 0 || !event.isPrimary) return;
        event.preventDefault();
        handle.focus({ preventScroll: true });
        active = { pointer: event.pointerId, x: event.clientX, width: sidebar.getBoundingClientRect().width, before: preferred, moved: false };
        try { handle.setPointerCapture(event.pointerId); }
        catch (_) { active = null; return; }
        document.documentElement.classList.add('sidebar-resizing');
        handle.setAttribute('data-resizing', 'true');
    });
    on(handle, 'pointermove', move);
    on(handle, 'pointerup', event => {
        if (event.pointerId !== active?.pointer) return;
        move(event);
        finish(true);
    });
    const cancelPointer = event => { if (event.pointerId === active?.pointer) finish(false); };
    on(handle, 'pointercancel', cancelPointer);
    on(handle, 'lostpointercapture', cancelPointer);
    on(handle, 'dblclick', () => { if (enabled()) reset(); });
    on(handle, 'keydown', event => {
        if (!enabled() || active) return;
        const bounds = limits(), step = event.shiftKey ? 40 : 10;
        const width = sidebar.getBoundingClientRect().width;
        const next = { ArrowLeft: width - step, ArrowRight: width + step, Home: bounds.min, End: bounds.max }[event.key];
        if (event.key === 'Enter') { event.preventDefault(); reset(); }
        else if (next !== undefined) { event.preventDefault(); setWidth(next); save(); }
    });
    on(window, 'keydown', event => {
        if (active && event.key === 'Escape') { event.preventDefault(); finish(false); }
    });
    on(window, 'blur', () => finish(false));
    on(window, 'resize', () => { finish(false); update(); });
    registry.set(id, () => {
        finish(false);
        listeners.forEach(dispose => dispose());
        shell.style.removeProperty(property);
        registry.delete(id);
    });
}
