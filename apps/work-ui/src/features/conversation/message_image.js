// Raster-only attachments. Payloads live only in mount's stack, never in entries or DOM.
const entries = new Map();
const MAX_BYTES = 10 * 1024 * 1024, MAX_SIDE = 16384, MAX_PIXELS = 40000000;
const INVALID = '图片无效或格式不受支持。';
const DECODE_ERROR = '图片解码失败，无法显示。';
const require = condition => { if (!condition) throw new Error(INVALID); };
function dimensions(width, height) {
    require(Number.isInteger(width) && Number.isInteger(height) && width > 0 && height > 0
        && width <= MAX_SIDE && height <= MAX_SIDE && width * height <= MAX_PIXELS);
    return { width, height };
}
function reader(bytes) {
    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    const bounds = (at, count) => require(at >= 0 && count >= 0 && at + count <= bytes.length);
    return {
        u16(at) { bounds(at, 2); return view.getUint16(at); },
        le16(at) { bounds(at, 2); return view.getUint16(at, true); },
        u32(at) { bounds(at, 4); return view.getUint32(at); },
        le32(at) { bounds(at, 4); return view.getUint32(at, true); },
        le24(at) { bounds(at, 3); return bytes[at] + bytes[at + 1] * 256 + bytes[at + 2] * 65536; },
        text(at, text) { bounds(at, text.length); return [...text].every((c, i) => bytes[at + i] === c.charCodeAt(0)); },
        bounds
    };
}
function png(bytes, r) {
    require(r.text(0, '\x89PNG\r\n\x1a\n') && r.u32(8) === 13 && r.text(12, 'IHDR'));
    const size = dimensions(r.u32(16), r.u32(20));
    r.bounds(24, 9);
    const depths = { 0: [1, 2, 4, 8, 16], 2: [8, 16], 3: [1, 2, 4, 8], 4: [8, 16], 6: [8, 16] };
    require(depths[bytes[25]]?.includes(bytes[24]) && bytes[26] === 0 && bytes[27] === 0 && bytes[28] <= 1);
    let at = 33, dataSeen = false, dataEnded = false;
    let animationFrames = 0, frames = 0, sequence = 0, pixels = 0;
    while (at < bytes.length) {
        r.bounds(at, 12);
        const length = r.u32(at);
        r.bounds(at + 12, length);
        require(!r.text(at + 4, 'IHDR'));
        if (r.text(at + 4, 'acTL')) {
            require(!dataSeen && !animationFrames && length === 8);
            animationFrames = r.u32(at + 8); require(animationFrames > 0);
        }
        if (r.text(at + 4, 'fcTL')) {
            require(animationFrames && length === 26 && r.u32(at + 8) === sequence++);
            const frame = dimensions(r.u32(at + 12), r.u32(at + 16));
            require(r.u32(at + 20) + frame.width <= size.width && r.u32(at + 24) + frame.height <= size.height
                && bytes[at + 32] <= 2 && bytes[at + 33] <= 1);
            if (!dataSeen) require(frame.width === size.width && frame.height === size.height);
            pixels += frame.width * frame.height; require(pixels <= MAX_PIXELS); frames++;
        }
        if (r.text(at + 4, 'fdAT')) require(dataSeen && frames && length >= 4 && r.u32(at + 8) === sequence++);
        if (r.text(at + 4, 'IDAT')) { require(!dataEnded); dataSeen = true; }
        else if (dataSeen) dataEnded = true;
        if (r.text(at + 4, 'IEND')) {
            require(length === 0 && dataSeen && at + 12 === bytes.length
                && (!animationFrames || frames === animationFrames));
            return size;
        }
        at += length + 12;
    }
    throw new Error(INVALID);
}
function jpeg(bytes, r) {
    require(r.u16(0) === 0xffd8 && r.u16(bytes.length - 2) === 0xffd9);
    let at = 2, size = null;
    const sof = new Set([0xc0, 0xc1, 0xc2]);
    while (at < bytes.length - 2) {
        require(bytes[at++] === 0xff);
        while (bytes[at] === 0xff) at++;
        r.bounds(at, 1);
        const marker = bytes[at++];
        require(marker !== 0 && marker !== 0xd8 && marker !== 0xd9 && marker !== 0xdc
            && !(marker >= 0xd0 && marker <= 0xd7));
        if (marker === 0x01) continue;
        const length = r.u16(at);
        require(length >= 2); r.bounds(at, length);
        const isSof = marker >= 0xc0 && marker <= 0xcf && ![0xc4, 0xc8, 0xcc].includes(marker);
        if (isSof) {
            require(sof.has(marker) && !size && length >= 11 && bytes[at + 2] === 8);
            const components = bytes[at + 7];
            require([1, 3, 4].includes(components) && length === 8 + 3 * components);
            size = dimensions(r.u16(at + 5), r.u16(at + 3));
        }
        if (marker === 0xda) {
            require(size && length >= 6 && length === 6 + 2 * bytes[at + 2]);
            // Entropy scans may contain only stuffed FF bytes/restarts. Validate every
            // later segment too so a second SOF or DNL cannot change decoder dimensions.
            at += length;
            while (at < bytes.length - 2) {
                if (bytes[at++] !== 0xff) continue;
                const start = at - 1;
                while (bytes[at] === 0xff) at++;
                r.bounds(at, 1);
                const next = bytes[at++];
                if (next === 0 || (next >= 0xd0 && next <= 0xd7)) continue;
                if (next === 0xd9) { require(at === bytes.length); return size; }
                require(next !== 0xdc); // DNL is unnecessary when a checked height is present.
                at = start;
                break;
            }
            if (at === bytes.length - 2) return size;
            continue;
        }
        at += length;
    }
    throw new Error(INVALID);
}
function gif(bytes, r) {
    require(r.text(0, 'GIF87a') || r.text(0, 'GIF89a'));
    const size = dimensions(r.le16(6), r.le16(8));
    r.bounds(0, 13);
    let at = 13, frames = 0, pixels = 0;
    if (bytes[10] & 128) at += 3 * (2 ** ((bytes[10] & 7) + 1));
    const blocks = () => {
        for (;;) {
            r.bounds(at, 1);
            const length = bytes[at++];
            if (!length) return;
            r.bounds(at, length); at += length;
        }
    };
    for (;;) {
        r.bounds(at, 1);
        const type = bytes[at++];
        if (type === 0x3b) { require(frames > 0 && at === bytes.length); return size; }
        if (type === 0x21) { r.bounds(at++, 1); blocks(); continue; }
        require(type === 0x2c); r.bounds(at, 9);
        const left = r.le16(at), top = r.le16(at + 2);
        const frame = dimensions(r.le16(at + 4), r.le16(at + 6));
        require(left + frame.width <= size.width && top + frame.height <= size.height);
        pixels += frame.width * frame.height; require(pixels <= MAX_PIXELS);
        const packed = bytes[at + 8]; require((packed & 24) === 0); at += 9;
        if (packed & 128) at += 3 * (2 ** ((packed & 7) + 1));
        r.bounds(at, 1); require(bytes[at] >= 2 && bytes[at] <= 8); at++;
        blocks(); frames++;
    }
}
function webp(bytes, r) {
    require(r.text(0, 'RIFF') && r.text(8, 'WEBP') && r.le32(4) + 8 === bytes.length);
    const chunks = (start, end, visit) => {
        let at = start;
        while (at < end) {
            require(at + 8 <= end);
            const length = r.le32(at + 4), data = at + 8, next = data + length + (length & 1);
            require(next <= end);
            if (length & 1) require(bytes[next - 1] === 0);
            visit(at, data, length); at = next;
        }
        require(at === end);
    };
    const raster = (at, data, length) => {
        if (r.text(at, 'VP8 ')) {
            require(length >= 10 && (bytes[data] & 1) === 0 && r.text(data + 3, '\x9d\x01\x2a'));
            const tag = r.le24(data);
            require(((tag >> 1) & 7) <= 3 && (tag & 16) !== 0 && (tag >>> 5) <= length - 10);
            return dimensions(r.le16(data + 6) & 0x3fff, r.le16(data + 8) & 0x3fff);
        }
        require(r.text(at, 'VP8L') && length >= 5 && bytes[data] === 0x2f);
        const bits = r.le32(data + 1); require((bits >>> 29) === 0);
        return dimensions((bits & 0x3fff) + 1, ((bits >>> 14) & 0x3fff) + 1);
    };
    let canvas = null, image = null, flags = 0, animation = false, frames = 0, pixels = 0, count = 0;
    chunks(12, bytes.length, (at, data, length) => {
        const first = count++ === 0;
        if (r.text(at, 'VP8X')) {
            require(first && length === 10 && (bytes[data] & 0xc1) === 0
                && bytes[data + 1] === 0 && bytes[data + 2] === 0 && bytes[data + 3] === 0);
            flags = bytes[data];
            canvas = dimensions(r.le24(data + 4) + 1, r.le24(data + 7) + 1);
        } else if (r.text(at, 'ANIM')) {
            require(canvas && (flags & 2) && !animation && !image && !frames && length === 6);
            animation = true;
        } else if (r.text(at, 'ANMF')) {
            require(animation && !image && length >= 16 && (bytes[data + 15] & 0xfc) === 0);
            const frame = dimensions(r.le24(data + 6) + 1, r.le24(data + 9) + 1);
            require(r.le24(data) * 2 + frame.width <= canvas.width
                && r.le24(data + 3) * 2 + frame.height <= canvas.height);
            pixels += frame.width * frame.height; require(pixels <= MAX_PIXELS);
            let decoded = null, alpha = false;
            chunks(data + 16, data + length, (nested, offset, size) => {
                if (r.text(nested, 'ALPH')) { require(!decoded && !alpha && size > 0); alpha = true; }
                else {
                    require(!decoded); decoded = raster(nested, offset, size);
                    require(!alpha || r.text(nested, 'VP8 '));
                }
            });
            require(decoded && decoded.width === frame.width && decoded.height === frame.height); frames++;
        } else if (r.text(at, 'VP8 ') || r.text(at, 'VP8L')) {
            require(!image && !animation && !(flags & 2));
            image = raster(at, data, length);
            if (canvas) require(image.width === canvas.width && image.height === canvas.height);
            else require(first);
        } else {
            // Extended ancillary chunks are only meaningful with a checked VP8X canvas.
            require(canvas && ['ICCP', 'ALPH', 'EXIF', 'XMP '].some(type => r.text(at, type)));
            if (r.text(at, 'ALPH')) require(!image && !animation && length > 0 && (flags & 16));
        }
    });
    require(image || (animation && frames > 0));
    return canvas || image;
}
function decodePayload(mime, data, maxBytes = MAX_BYTES) {
    require(['image/png', 'image/jpeg', 'image/gif', 'image/webp'].includes(mime));
    require(typeof data === 'string' && data.length > 0 && data.length % 4 === 0
        && data.length <= Math.ceil(maxBytes / 3) * 4);
    // Scan instead of a backtracking regexp on a potentially 14 MB string.
    const pad = data.endsWith('==') ? 2 : data.endsWith('=') ? 1 : 0;
    require(data.length / 4 * 3 - pad <= maxBytes);
    let last = 0;
    for (let i = 0; i < data.length - pad; i++) {
        const c = data.charCodeAt(i);
        last = c >= 65 && c <= 90 ? c - 65 : c >= 97 && c <= 122 ? c - 71
            : c >= 48 && c <= 57 ? c + 4 : c === 43 ? 62 : c === 47 ? 63 : -1;
        require(last >= 0);
    }
    require((pad !== 2 || (last & 15) === 0) && (pad !== 1 || (last & 3) === 0));
    const binary = atob(data), bytes = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
    const r = reader(bytes);
    const size = mime === 'image/png' ? png(bytes, r) : mime === 'image/jpeg' ? jpeg(bytes, r)
        : mime === 'image/gif' ? gif(bytes, r) : webp(bytes, r);
    return { bytes, ...size };
}
const alive = entry => entries.get(entry.id) === entry;
function notify(entry, value) {
    if (alive(entry)) { try { entry.send(value); } catch (_) { /* evaluator already unmounted */ } }
}
function releaseImage(entry) {
    clearTimeout(entry.timer); entry.timer = null;
    const image = entry.image;
    if (image) { image.onload = null; image.onerror = null; image.removeAttribute('src'); }
    entry.ready = false;
    if (entry.url) URL.revokeObjectURL(entry.url);
    entry.url = null;
}
function dispose(id) {
    const entry = entries.get(id);
    if (!entry) return;
    entry.viewer?.destroy(true);
    entries.delete(id);
    releaseImage(entry);
}
function mount(id, mime, data, send, project = false) {
    const restoringViewer = entries.get(id)?.viewer?.dialog.contains(document.activeElement);
    dispose(id);
    const host = document.getElementById(id);
    // An updated image disables its opener while decoding. Keep focus on the
    // stable figure instead of leaving it on the soon-disabled button.
    if (restoringViewer && host?.isConnected && !document.querySelector('dialog[open]')) {
        host.focus({ preventScroll: true });
    }
    const image = host?.querySelector('.message-image-preview img');
    const entry = { id, image, send, url: null, viewer: null, ready: false, timer: null };
    entries.set(id, entry);
    notify(entry, '正在加载图片…');
    try {
        require(image);
        const decoded = decodePayload(mime, data, project ? 20 * 1024 * 1024 : MAX_BYTES);
        const { width, height } = decoded;
        const blob = new Blob([decoded.bytes], { type: mime });
        const url = URL.createObjectURL(blob);
        entry.url = url;
        const valid = () => alive(entry) && entry.url === url;
        const fail = () => {
            if (!valid()) return;
            entry.viewer?.destroy(false); releaseImage(entry); notify(entry, DECODE_ERROR);
        };
        const validSize = (w, h) => (w === width && h === height)
            || (mime === 'image/jpeg' && w === height && h === width);
        // img.onload may accept headers despite a corrupt compressed stream.
        // Where available, require an actual bitmap decode before reporting ready.
        const checked = typeof createImageBitmap === 'function'
            ? createImageBitmap(blob).then(bitmap => {
                const sizeMatches = validSize(bitmap.width, bitmap.height);
                bitmap.close();
                if (!sizeMatches) throw new Error(DECODE_ERROR);
                return true;
            }).catch(() => { fail(); return false; })
            : Promise.resolve(true);
        image.onload = async () => {
            if (!valid() || !(await checked) || !valid()) return;
            if (!validSize(image.naturalWidth, image.naturalHeight)) { fail(); return; }
            clearTimeout(entry.timer); entry.timer = null;
            image.onload = null; image.onerror = fail;
            entry.ready = true; notify(entry, '');
        };
        image.onerror = fail;
        entry.timer = setTimeout(fail, 20000);
        image.src = url;
    } catch (_) { releaseImage(entry); notify(entry, INVALID); }
}
function openViewer(id) {
    const entry = entries.get(id), host = document.getElementById(id);
    const preview = entry?.image;
    if (!entry?.ready || !entry.url || !host || !preview?.naturalWidth) return;
    if (entry.viewer?.dialog.open) return;
    // Native close events are queued; removing the old listener must precede reopen.
    entry.viewer?.destroy(false);
    const opener = document.activeElement;
    const dialog = document.createElement('dialog');
    dialog.className = 'message-image-viewer';
    dialog.setAttribute('aria-labelledby', `${id}-viewer-title`);
    const toolbar = document.createElement('div'); toolbar.className = 'message-image-viewer-toolbar';
    const title = document.createElement('h2');
    title.id = `${id}-viewer-title`;
    title.textContent = host.querySelector('.message-image-heading > span')?.textContent || '消息图片';
    toolbar.append(title);
    const listeners = [];
    const listen = (node, type, handler) => {
        node.addEventListener(type, handler); listeners.push(() => node.removeEventListener(type, handler));
    };
    const button = (label, text, action) => {
        const control = document.createElement('button');
        control.type = 'button'; control.className = 'button';
        control.setAttribute('aria-label', label); control.textContent = text;
        listen(control, 'click', action); toolbar.append(control); return control;
    };
    const viewport = document.createElement('div');
    viewport.className = 'message-image-viewer-viewport'; viewport.tabIndex = 0;
    viewport.setAttribute('role', 'region'); viewport.setAttribute('aria-label', '图片视图，可滚动查看');
    const image = document.createElement('img');
    image.alt = preview.alt; image.draggable = false; viewport.append(image);
    let scale = 1, fitting = true, destroyed = false;
    const apply = () => {
        if (destroyed || !image.naturalWidth) return;
        if (fitting) scale = Math.min(1, Math.max(1, viewport.clientWidth - 24) / image.naturalWidth,
            Math.max(1, viewport.clientHeight - 24) / image.naturalHeight);
        scale = Math.max(0.01, Math.min(8, scale));
        image.style.width = `${image.naturalWidth * scale}px`;
        image.style.height = `${image.naturalHeight * scale}px`;
        zoomOut.disabled = scale <= 0.01; zoomIn.disabled = scale >= 8;
        percent.textContent = `${Math.round(scale * 100)}%`;
    };
    const zoom = factor => { fitting = false; scale *= factor; apply(); };
    const zoomOut = button('缩小图片', '−', () => zoom(1 / 1.25));
    const zoomIn = button('放大图片', '+', () => zoom(1.25));
    button('适应窗口', '适应窗口', () => { fitting = true; apply(); viewport.scrollTo(0, 0); });
    const percent = document.createElement('output');
    percent.setAttribute('aria-label', '缩放比例'); percent.setAttribute('aria-live', 'polite'); toolbar.append(percent);
    const close = button('关闭图片查看器', '关闭', () => dialog.close()); close.autofocus = true;
    dialog.append(toolbar, viewport);
    const observer = new ResizeObserver(apply);
    const usable = node => node?.isConnected && typeof node.focus === 'function' && !node.disabled
        && !node.closest('[hidden],[inert]') && node.getClientRects().length > 0;
    const destroy = (restore = true) => {
        if (destroyed) return;
        destroyed = true;
        const ownsViewer = entry.viewer?.dialog === dialog;
        const hadFocus = dialog.contains(document.activeElement);
        for (const remove of listeners) remove();
        observer.disconnect(); image.onload = null; image.onerror = null;
        image.removeAttribute('src');
        if (dialog.open) dialog.close();
        dialog.remove();
        if (ownsViewer) entry.viewer = null;
        // Don't override another dialog or focus explicitly moved elsewhere since close.
        const active = document.activeElement;
        if (!restore || !ownsViewer || !alive(entry)
            || (!hadFocus && active !== document.body && active !== opener)) return;
        const top = [...document.querySelectorAll('dialog[open]')].at(-1);
        const target = usable(opener) ? opener : host;
        if (usable(target) && (!top || top.contains(target))) target.focus({ preventScroll: true });
    };
    entry.viewer = { dialog, destroy };
    listen(dialog, 'close', () => destroy(true));
    listen(dialog, 'cancel', event => { event.preventDefault(); dialog.close(); });
    listen(dialog, 'keydown', event => {
        if (event.key !== 'Tab') return;
        const controls = [...dialog.querySelectorAll('button:not(:disabled),[tabindex="0"]')];
        const first = controls[0], last = controls.at(-1);
        if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last.focus(); }
        else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first.focus(); }
    });
    image.onload = () => { if (alive(entry) && entry.viewer?.dialog === dialog) apply(); };
    image.onerror = () => destroy(true);
    image.src = entry.url;
    document.body.append(dialog);
    try { dialog.showModal(); close.focus(); observer.observe(viewport); }
    catch (_) { destroy(true); }
}
window.__rpiMessageImages = { mount, dispose, openViewer, entries };
