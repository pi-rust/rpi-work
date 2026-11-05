// Run at the field, before Dioxus's delegated keyboard handler, on Web and WebView.
// Local input is DOM-owned: only revisioned external edits may write its value.
const registry = window.__rpiComposerInputs ||= new Map();
registry.get(id)?.();
const field = document.getElementById(id);
if (field) {
    let composing = false, pending = null, frame = 0, revision = -1;
    const publish = () => field.dispatchEvent(new Event('input', { bubbles: true }));
    const apply = packet => {
        if (!field.isConnected || field.value === packet.text) return;
        let text = packet.text;
        if (field.value !== packet.expected) {
            // The browser is ahead of a native render batch. Merge explicit
            // appended quotations, but never clear/replace newer local edits.
            if (text.startsWith(packet.expected) && text.length > packet.expected.length) {
                text = field.value + text.slice(packet.expected.length);
            } else { publish(); return; }
        }
        field.value = text;
        if (text !== packet.text) publish();
    };
    const sync = packet => {
        if (!Number.isSafeInteger(packet.revision) || packet.revision <= revision) return;
        revision = packet.revision;
        if (composing || frame) {
            const sourceText = packet.text;
            let addition = packet.text.startsWith(packet.expected) && packet.text.length > packet.expected.length
                ? packet.text.slice(packet.expected.length) : null;
            if (pending) {
                const expected = packet.expected === pending.sourceText ? pending.expected : packet.expected;
                // An input snapshot may replace the Rust draft between quotes.
                // Keep the unapplied append operations, not only its latest text.
                if (addition !== null && pending.addition !== null) {
                    addition = pending.addition + addition;
                    packet = { ...packet, expected, text: expected + addition };
                } else packet = { ...packet, expected };
            }
            pending = { ...packet, sourceText, addition };
        } else apply(packet);
    };
    const start = () => { composing = true; };
    const end = () => {
        composing = false;
        cancelAnimationFrame(frame);
        // Final committed input may follow compositionend. Let that input land
        // before reconciling a queued external quotation or clear operation.
        frame = requestAnimationFrame(() => {
            frame = 0;
            if (composing || !pending) return;
            const packet = pending; pending = null;
            apply(packet);
        });
    };
    const keydown = event => {
        const enter = event.key === 'Enter' || event.code === 'Enter' || event.code === 'NumpadEnter';
        if (enter && (composing || event.isComposing || event.keyCode === 229)) {
            // Candidate confirmation must still reach the IME's default handling.
            event.stopPropagation();
        }
    };
    field.addEventListener('compositionstart', start);
    field.addEventListener('compositionend', end);
    field.addEventListener('keydown', keydown, true);
    const dispose = () => {
        cancelAnimationFrame(frame); pending = null;
        field.removeEventListener('compositionstart', start);
        field.removeEventListener('compositionend', end);
        field.removeEventListener('keydown', keydown, true);
        registry.delete(id);
    };
    dispose.sync = sync;
    registry.set(id, dispose);
}
dioxus.send(Boolean(field));
