// `id` is JSON-encoded by Rust; tool IDs come from DOM data, never JS source.
// Only the current transcript's scalar state is kept on window (no DOM refs).
const row = document.getElementById(id);
const transcript = row?.closest('.transcript');
const scope = transcript?.dataset.sessionScope;
if (row && scope && !row.__rpiToolDisclosureFailure) {
    if (window.__rpiToolDisclosures?.scope !== scope) {
        window.__rpiToolDisclosures = { scope, tools: new Map() };
    }
    // Clear even when the next session has no tool cards to mount. The observer
    // belongs to the transcript, not the window registry, and holds no cards.
    if (!transcript.__rpiToolDisclosureScopeObserver) {
        transcript.__rpiToolDisclosureScopeObserver = new MutationObserver(changes => {
            const current = changes[changes.length - 1].target.dataset.sessionScope;
            if (window.__rpiToolDisclosures?.scope !== current) {
                window.__rpiToolDisclosures = { scope: current, tools: new Map() };
            }
        });
        transcript.__rpiToolDisclosureScopeObserver.observe(transcript, {
            attributes: true, attributeFilter: ['data-session-scope'],
        });
    }
    const registry = window.__rpiToolDisclosures;
    const toolId = row.dataset.toolId;
    let state = registry.tools.get(toolId);
    if (!state) {
        state = { open: failed || row.open, failed };
        registry.tools.set(toolId, state);
    } else if (failed && !state.failed) {
        state.open = true;
    }
    state.failed = failed;
    row.open = state.open;
    const active = () => row.isConnected
        && row.closest('.transcript')?.dataset.sessionScope === scope
        && window.__rpiToolDisclosures === registry;
    // Only failure changes call this; ordinary output/timing renders never
    // write open. A repeated failed mount must respect a user's manual close.
    row.__rpiToolDisclosureFailure = nextFailed => {
        if (!active()) return;
        if (nextFailed && !state.failed) {
            row.open = true;
            state.open = true;
        }
        state.failed = nextFailed;
    };
    row.addEventListener('toggle', event => {
        if (event.target === row && active()) state.open = row.open;
    });
}
// No scope: leave the native initial attribute and legacy failure effect alone.
