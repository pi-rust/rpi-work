// Installed once per document. Untrusted diagrams never execute in the app's origin.
const entries = new Map();
let frame = null, frameReady = null, current = null, sequence = 0, pumping = false;
const theme = () => {
    const css = getComputedStyle(document.documentElement);
    return { dark: document.documentElement.dataset.mode === 'dark',
        background: css.getPropertyValue('--card').trim(),
        foreground: css.getPropertyValue('--text').trim(),
        accent: css.getPropertyValue('--accent').trim(),
        line: css.getPropertyValue('--line-strong').trim() };
};
const notify = (entry, value) => { if (entries.get(entry.id) === entry) entry.send(value); };
const revoke = entry => { if (entry.url) URL.revokeObjectURL(entry.url); entry.url = null; };
function openViewer(id) {
    const entry = entries.get(id);
    const host = document.getElementById(id);
    const preview = host?.querySelector('.mermaid-image img');
    if (!entry?.url || !preview?.complete || !preview.naturalWidth) return;
    if (entry.viewer) {
        if (entry.viewer.dialog.open) return;
        // Native close events are queued: a quick reopen may precede the old cleanup.
        entry.viewer.destroy();
    }
    const opener = document.activeElement;
    const dialog = document.createElement('dialog');
    dialog.className = 'mermaid-viewer';
    dialog.setAttribute('aria-labelledby', `${id}-viewer-title`);
    const toolbar = document.createElement('div');
    toolbar.className = 'mermaid-viewer-toolbar';
    const title = document.createElement('h2');
    title.id = `${id}-viewer-title`; title.textContent = 'Mermaid 图表';
    toolbar.append(title);
    const button = (label, text, action) => {
        const control = document.createElement('button');
        control.type = 'button'; control.className = 'button';
        control.setAttribute('aria-label', label); control.textContent = text;
        control.addEventListener('click', action); toolbar.append(control);
        return control;
    };
    const viewport = document.createElement('div');
    viewport.className = 'mermaid-viewer-viewport';
    viewport.tabIndex = 0;
    viewport.setAttribute('role', 'region');
    viewport.setAttribute('aria-label', '图表视图，可滚动查看');
    const image = document.createElement('img');
    image.alt = preview.alt; image.draggable = false;
    viewport.append(image);
    let scale = 1, fitting = true;
    const apply = () => {
        if (!image.naturalWidth) return;
        if (fitting) scale = Math.min(1, (viewport.clientWidth - 24) / image.naturalWidth,
            (viewport.clientHeight - 24) / image.naturalHeight);
        scale = Math.max(0.01, Math.min(8, scale));
        image.style.width = `${image.naturalWidth * scale}px`;
        image.style.height = `${image.naturalHeight * scale}px`;
        zoomOut.disabled = scale <= 0.01; zoomIn.disabled = scale >= 8;
        percent.textContent = `${Math.round(scale * 100)}%`;
    };
    const zoom = factor => { fitting = false; scale *= factor; apply(); };
    const zoomOut = button('缩小图表', '−', () => zoom(1 / 1.25));
    const zoomIn = button('放大图表', '+', () => zoom(1.25));
    button('适应窗口', '适应窗口', () => {
        fitting = true; apply(); viewport.scrollTo(0, 0);
    });
    const percent = document.createElement('output');
    percent.setAttribute('aria-label', '缩放比例'); percent.setAttribute('aria-live', 'polite');
    toolbar.append(percent);
    const close = button('关闭图表查看器', '关闭', () => dialog.close());
    close.autofocus = true;
    dialog.append(toolbar, viewport);
    // Keep Tab within the viewer rather than cycling through browser chrome.
    dialog.addEventListener('keydown', event => {
        if (event.key !== 'Tab') return;
        const controls = [...dialog.querySelectorAll('button:not(:disabled),[tabindex="0"]')];
        const first = controls[0], last = controls[controls.length - 1];
        if (event.shiftKey && document.activeElement === first) {
            event.preventDefault(); last.focus();
        } else if (!event.shiftKey && document.activeElement === last) {
            event.preventDefault(); first.focus();
        }
    });
    document.body.append(dialog);
    const observer = new ResizeObserver(apply);
    let destroyed = false;
    const destroy = () => {
        if (destroyed) return;
        destroyed = true;
        dialog.removeEventListener('close', destroy);
        observer.disconnect(); image.onload = null; image.onerror = null;
        image.removeAttribute('src'); dialog.remove();
        if (entry.viewer?.dialog === dialog) entry.viewer = null;
        const top = [...document.querySelectorAll('dialog[open]')].at(-1);
        // A pending theme render disables the opener; focus the diagram instead.
        const target = opener?.isConnected && !opener.disabled ? opener : host;
        if (target?.isConnected && (!top || top.contains(target))) target.focus({ preventScroll: true });
    };
    entry.viewer = { dialog, image, destroy };
    dialog.addEventListener('close', destroy, { once: true });
    image.onload = () => { apply(); if (fitting) viewport.scrollTo(0, 0); };
    image.src = entry.url;
    dialog.showModal(); close.focus(); observer.observe(viewport);
}
// Executed only in an opaque-origin sandbox with no network permissions.
function renderer() {
    addEventListener('message', async event => {
        if (event.source !== parent || event.data?.type !== 'render') return;
        const { token, source, colors } = event.data;
        const root = document.getElementById('diagram');
        try {
            mermaid.initialize({ startOnLoad: false, securityLevel: 'strict',
                suppressErrorRendering: true, maxTextSize: 20000, maxEdges: 500,
                htmlLabels: false, theme: 'base', fontFamily: 'sans-serif',
                themeVariables: { darkMode: colors.dark, background: colors.background,
                    primaryColor: colors.background, primaryTextColor: colors.foreground,
                    primaryBorderColor: colors.accent, lineColor: colors.line,
                    secondaryColor: colors.background, tertiaryColor: colors.background,
                    mainBkg: colors.background, textColor: colors.foreground },
                flowchart: { htmlLabels: false, useMaxWidth: true },
                sequence: { useMaxWidth: true } });
            const result = await mermaid.render(`diagram-${token}`, source, root);
            const svg = new DOMParser().parseFromString(result.svg, 'image/svg+xml').documentElement;
            const box = svg.getAttribute('viewBox')?.split(/[\s,]+/).map(Number);
            if (box?.length === 4 && box.every(Number.isFinite) && box[2] > 0 && box[3] > 0) {
                svg.setAttribute('width', String(box[2])); svg.setAttribute('height', String(box[3]));
            }
            parent.postMessage({ type: 'rendered', token, svg: new XMLSerializer().serializeToString(svg) }, '*');
        } catch (_) { parent.postMessage({ type: 'rendered', token, error: true }, '*'); }
        finally { root.replaceChildren(); }
    });
    parent.postMessage({ type: 'ready' }, '*');
}
function ensureFrame(entry) {
    if (frame) return frameReady;
    frame = document.createElement('iframe');
    frame.title = '隔离的 Mermaid 渲染器';
    frame.setAttribute('sandbox', 'allow-scripts');
    frame.setAttribute('aria-hidden', 'true'); frame.tabIndex = -1;
    frame.style.cssText = 'position:fixed;left:-20000px;top:0;width:1200px;height:1px;visibility:hidden;pointer-events:none;border:0';
    const nonce = crypto.randomUUID();
    frameReady = new Promise((resolve, reject) => {
        const timer = setTimeout(() => { current = null; reject(new Error('renderer timeout')); }, 12000);
        current = { entry, ready: true, resolve: () => { clearTimeout(timer); resolve(); },
            cancel: () => { clearTimeout(timer); reject(new Error('cancelled')); } };
    });
    const safeLibrary = library.replace(/<\/script/gi, '<\\/script');
    frame.srcdoc = `<!doctype html><html><head><meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'nonce-${nonce}'; style-src 'unsafe-inline'; img-src 'none'; font-src 'none'; connect-src 'none'; base-uri 'none'; form-action 'none'"><style>body{margin:0}*{animation:none!important;transition:none!important}</style></head><body><div id="diagram"></div><script nonce="${nonce}">${safeLibrary}\n;(${renderer.toString()})();<\/script></body></html>`;
    document.body.append(frame);
    return frameReady;
}
function resetFrame() { frame?.remove(); frame = null; frameReady = null; }
addEventListener('message', event => {
    if (!frame || event.source !== frame.contentWindow || !current) return;
    if (current.ready && event.data?.type === 'ready') { const task = current; current = null; task.resolve(); }
    else if (event.data?.type === 'rendered' && event.data.token === current.token) {
        const task = current; current = null; task.resolve(event.data);
    }
});
async function pump() {
    if (pumping) return;
    pumping = true;
    try {
        for (;;) {
            const entry = [...entries.values()].find(entry => entry.pending);
            if (!entry) break;
            entry.pending = false;
            const generation = entry.generation;
            try {
                await ensureFrame(entry);
                if (entries.get(entry.id) !== entry || entry.generation !== generation) continue;
                const token = ++sequence;
                const result = await new Promise((resolve, reject) => {
                    const timer = setTimeout(() => { current = null; reject(new Error('render timeout')); }, 12000);
                    current = { entry, token, resolve: value => { clearTimeout(timer); resolve(value); },
                        cancel: () => { clearTimeout(timer); reject(new Error('cancelled')); } };
                    frame.contentWindow.postMessage({ type: 'render', token, source: entry.source, colors: theme() }, '*');
                });
                if (entries.get(entry.id) !== entry || entry.generation !== generation) continue;
                if (result.error || typeof result.svg !== 'string' || result.svg.length > 5000000) throw new Error('invalid SVG');
                // SVG image mode is inert: no scripts, clickable links or external resources.
                // No Mermaid-produced markup is ever inserted as HTML in the host DOM.
                const image = document.getElementById(entry.id)?.querySelector('.mermaid-image img');
                if (!image) continue;
                const oldUrl = entry.url;
                const url = URL.createObjectURL(new Blob([result.svg], { type: 'image/svg+xml' }));
                entry.url = url;
                image.onload = () => {
                    if (entries.get(entry.id) === entry && generation === entry.generation && entry.url === url) notify(entry, '');
                };
                image.onerror = () => {
                    if (entries.get(entry.id) === entry && generation === entry.generation && entry.url === url) notify(entry, '图表无法显示，请查看源码。');
                };
                image.src = url;
                // Replace every consumer before revoking the previous theme's Blob.
                if (entry.viewer) entry.viewer.image.src = url;
                if (oldUrl) URL.revokeObjectURL(oldUrl);
            } catch (_) {
                if (current) current = null;
                resetFrame();
                if (entry.generation === generation) notify(entry, 'Mermaid 渲染失败，请检查语法或查看源码。');
            }
        }
    } finally {
        pumping = false;
        if (!entries.size) resetFrame();
    }
}
function enqueue(entry) {
    entry.generation++;
    entry.pending = true;
    notify(entry, '正在绘制图表…');
    clearTimeout(entry.timer);
    entry.timer = setTimeout(pump, 120);
}
function dispose(id) {
    const entry = entries.get(id);
    if (!entry) return;
    entries.delete(id); clearTimeout(entry.timer);
    entry.viewer?.destroy();
    if (current?.entry === entry) {
        const task = current; current = null; resetFrame(); task.cancel();
    }
    const image = document.getElementById(id)?.querySelector('img');
    if (image) { image.onload = null; image.onerror = null; image.removeAttribute('src'); }
    revoke(entry);
    if (!entries.size && !pumping) resetFrame();
}
function mount(id, source, closed, send) {
    dispose(id);
    const entry = { id, source, send, generation: 0, pending: false, url: null };
    entries.set(id, entry);
    if (!closed) notify(entry, '等待完整 Mermaid 代码…');
    else if (source.length > 20000) notify(entry, 'Mermaid 代码超过 20,000 字符，请查看源码或拆分图表。');
    else if (/%%\s*\{|^\s*---(?:\r?\n|$)/.test(source)) notify(entry, '为安全起见，不支持图表内配置指令，请移除配置后重试。');
    else { entry.renderable = true; enqueue(entry); }
}
let lastTheme = JSON.stringify(theme());
new MutationObserver(() => {
    const next = JSON.stringify(theme());
    if (lastTheme === next) return;
    lastTheme = next;
    for (const entry of entries.values()) if (entry.renderable) enqueue(entry);
}).observe(document.documentElement, { attributes: true, attributeFilter: ['data-mode', 'data-theme', 'style', 'class'] });
window.__rpiMermaid = { mount, dispose, openViewer };
