// Installed only after the persisted draft map has passed Rust schema validation.
// Each keystroke stages one draft, never serializes or writes the whole map.
window.__rpiDraftFlush?.dispose();
let dirty = false;
const flush = () => {
    if (!dirty) return;
    try {
        localStorage.setItem('rpi-work-drafts-v1', JSON.stringify(initial));
        dirty = false;
    } catch (_) { /* The regular debounced writer reports storage errors in the UI. */ }
};
const visibility = () => { if (document.visibilityState === 'hidden') flush(); };
window.addEventListener('pagehide', flush);
window.addEventListener('beforeunload', flush);
document.addEventListener('visibilitychange', visibility);
window.__rpiDraftFlush = {
    update(scope, draft) { initial[scope] = draft; dirty = true; },
    flush,
    dispose() {
        window.removeEventListener('pagehide', flush);
        window.removeEventListener('beforeunload', flush);
        document.removeEventListener('visibilitychange', visibility);
    },
};
