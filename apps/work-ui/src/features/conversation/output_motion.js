// Small rings only: preserve their animation phase while changing speed.
// The workspace supplies an estimated live-output cadence, not exact tokens/s.
const element = document.getElementById(id);
if (element) {
    const registry = window.__rpiOutputMotion ??= new Map();
    registry.get(id)?.();
    const media = matchMedia('(prefers-reduced-motion: reduce)');
    const update = () => {
        const requested = Number(element.dataset.outputRate) || 1;
        const rate = Math.max(1, Math.min(requested, media.matches ? 1.6 : 3.2));
        for (const animation of element.getAnimations()) {
            if (['activity-spin', 'session-activity-spin'].includes(animation.animationName)) {
                animation.updatePlaybackRate(rate);
            }
        }
    };
    const observer = new MutationObserver(update);
    observer.observe(element, { attributes: true, attributeFilter: ['data-output-rate'] });
    media.addEventListener('change', update);
    const frame = requestAnimationFrame(update);
    registry.set(id, () => {
        cancelAnimationFrame(frame);
        observer.disconnect();
        media.removeEventListener('change', update);
        registry.delete(id);
    });
}
