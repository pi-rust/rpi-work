// The command is rendered as an escaped text node by Rust, never interpolated here.
const observers = window.__rpiCommandPreviews ||= new Map();
observers.get(id)?.();
const root = document.getElementById(id);
const pre = root?.querySelector('.command-text');
if (pre) {
    let disposed = false, previous = null;
    const measure = () => {
        if (disposed || !pre.getClientRects().length) return;
        const line = parseFloat(getComputedStyle(pre).lineHeight);
        if (!Number.isFinite(line) || line <= 0) return;
        const overflow = pre.scrollHeight > line * 4 + 1;
        if (previous !== overflow) { previous = overflow; dioxus.send(overflow); }
    };
    const resize = new ResizeObserver(measure);
    resize.observe(pre);
    const card = root.closest('details');
    card?.addEventListener('toggle', measure);
    document.fonts?.addEventListener('loadingdone', measure);
    const dispose = () => {
        if (disposed) return;
        disposed = true;
        resize.disconnect();
        card?.removeEventListener('toggle', measure);
        document.fonts?.removeEventListener('loadingdone', measure);
        if (observers.get(id) === dispose) observers.delete(id);
    };
    observers.set(id, dispose);
    document.fonts?.ready.then(measure);
    measure();
}
