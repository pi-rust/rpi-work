// One timer per mounted node/connection epoch; the Rust side calls complete()
// only after its bounded refresh ends, so requests never overlap.
window.__rpiWorkspaceSync ??= new Map();
window.__rpiWorkspaceSync.get(id)?.dispose();
let timer = null, pending = false, again = false, alive = true;
let nextManual = false, lastRequest = 0;
const tick = () => {
  clearTimeout(timer);
  timer = null;
  if (!alive || document.hidden) return;
  if (pending) { again = true; return; }
  pending = true;
  const manual = nextManual;
  nextManual = false;
  dioxus.send(manual);
};
const changed = () => {
  clearTimeout(timer);
  timer = null;
  if (document.hidden) { again = false; return; }
  tick();
};
const state = {
  request(serial) {
    if (!alive || serial <= lastRequest) return;
    lastRequest = serial;
    nextManual = true;
    // An older in-flight snapshot cannot satisfy a new manual request.
    // tick coalesces it into exactly one follow-up instead of overlapping.
    tick();
  },
  complete() {
    pending = false;
    if (!alive || document.hidden) return;
    if (again) { again = false; tick(); }
    else timer = setTimeout(tick, 3000);
  },
  dispose() {
    alive = false;
    clearTimeout(timer);
    document.removeEventListener('visibilitychange', changed);
    window.removeEventListener('focus', changed);
    if (window.__rpiWorkspaceSync.get(id) === state) window.__rpiWorkspaceSync.delete(id);
  },
};
window.__rpiWorkspaceSync.set(id, state);
document.addEventListener('visibilitychange', changed);
window.addEventListener('focus', changed);
tick();
