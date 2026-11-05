const observers = window.__rpiHistoryPagers ||= new Map();
observers.get(id)?.();
const root = document.getElementById(id), container = root?.closest('.transcript');
if (container && (enabled || manual)) {
    let previous = container.scrollTop, sent = false, intentUntil = 0, dragging = false;
    const valid = () => root.isConnected && container.isConnected
        && root.dataset.sessionScope === scope && container.dataset.sessionScope === scope;
    const request = explicit => {
        if (!sent && valid() && (explicit ? manual : enabled && container.scrollTop <= 96)) {
            sent = true; dioxus.send(true);
        }
    };
    const scroll = () => {
        const top = container.scrollTop;
        if (top < previous && (dragging || performance.now() <= intentUntil)) request(false);
        previous = top;
    };
    const wheel = event => { if (event.deltaY < 0) { intentUntil = performance.now() + 700; request(false); } };
    const key = event => {
        // Text editing and handled/modified keys are not transcript paging intent.
        const target = event.target;
        if (event.defaultPrevented || event.isComposing || event.keyCode === 229
            || event.altKey || event.ctrlKey || event.metaKey || event.shiftKey
            || (target instanceof Element && (target.closest('textarea, input, select') || target.isContentEditable))) return;
        if (['Home', 'PageUp', 'ArrowUp'].includes(event.key)) { intentUntil = performance.now() + 700; request(false); }
    };
    const pointer = () => { dragging = true; };
    const release = () => { dragging = false; };
    const touch = () => { intentUntil = performance.now() + 700; };
    // Navigation is not upward-scroll intent; explicitly cancel an old wheel/touch gesture.
    const navigation = () => { intentUntil = 0; dragging = false; previous = container.scrollTop; };
    const load = event => { if (event.detail === scope) request(true); };
    // Timeline browsing has its own edge, independent of transcript.scrollTop.
    // Automatic loads still obey the error/capability gate, unlike manual retry.
    const timelineLoad = event => { if (event.detail === scope && enabled) request(true); };
    root.addEventListener('rpi-history-navigation', navigation);
    root.addEventListener('rpi-history-load', load);
    root.addEventListener('rpi-history-load-auto', timelineLoad);
    container.addEventListener('scroll', scroll, { passive: true });
    container.addEventListener('wheel', wheel, { passive: true });
    container.addEventListener('keydown', key);
    container.addEventListener('pointerdown', pointer);
    container.addEventListener('touchmove', touch, { passive: true });
    addEventListener('pointerup', release);
    addEventListener('pointercancel', release);
    observers.set(id, () => {
        root.removeEventListener('rpi-history-navigation', navigation);
        root.removeEventListener('rpi-history-load', load);
        root.removeEventListener('rpi-history-load-auto', timelineLoad);
        container.removeEventListener('scroll', scroll);
        container.removeEventListener('wheel', wheel);
        container.removeEventListener('keydown', key);
        container.removeEventListener('pointerdown', pointer);
        container.removeEventListener('touchmove', touch);
        removeEventListener('pointerup', release);
        removeEventListener('pointercancel', release);
        observers.delete(id);
    });
}
