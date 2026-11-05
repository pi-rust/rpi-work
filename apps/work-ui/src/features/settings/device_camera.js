// Executed inside a Dioxus eval with decodeFrame and CAMERA_CLEANUP in scope.
const epoch = window.__rpiWorkCameraEpoch;
const video = document.getElementById('device-camera');
let sent = false;
let cancelPermission;
const cancelled = new Promise(resolve => { cancelPermission = () => resolve(null); });
const c = {
    stream: null, frame: 0, observer: null, timeout: 0,
    finish(value) {
        if (sent) return;
        sent = true;
        cancelPermission();
        clearTimeout(c.timeout);
        c.stream?.getTracks().forEach(track => track.stop());
        if (c.frame) cancelAnimationFrame(c.frame);
        c.observer?.disconnect();
        document.removeEventListener('visibilitychange', hidden);
        window.removeEventListener('pagehide', close);
        if (video && video.srcObject === c.stream) {
            video.pause(); video.srcObject = null; video.hidden = true;
        }
        if (window.__rpiWorkCamera === c) window.__rpiWorkCamera = null;
        dioxus.send(value);
    }
};
const close = () => {
    if (window.__rpiWorkCamera === c) window.__rpiWorkCameraEpoch++;
    c.finish('');
};
const hidden = () => { if (document.hidden) close(); };
const valid = () => !sent && window.__rpiWorkCameraEpoch === epoch && video?.isConnected && !document.hidden;
window.__rpiWorkCamera = c;
window.__rpiWorkQrAlbum?.cancel();
document.addEventListener('visibilitychange', hidden);
window.addEventListener('pagehide', close);
c.observer = new MutationObserver(() => { if (!video?.isConnected) close(); });
c.observer.observe(document.documentElement, { childList: true, subtree: true });
if (!valid() || !window.isSecureContext || !navigator.mediaDevices?.getUserMedia) {
    c.finish('ERR:当前环境不能打开相机，请使用相册扫码，或通过 HTTPS 打开网页。');
} else {
    c.timeout = setTimeout(() => c.finish('ERR:相机启动超时，请重试或使用相册扫码。'), 30000);
    try {
        const permission = (async () => {
            let stream;
            try {
                stream = await navigator.mediaDevices.getUserMedia({ video: { facingMode: { ideal: 'environment' } }, audio: false });
            } catch (error) {
                // Retry device constraints only. Never re-prompt after denial.
                if (!valid() || !['OverconstrainedError', 'ConstraintNotSatisfiedError'].includes(error.name)) throw error;
                stream = await navigator.mediaDevices.getUserMedia({ video: true, audio: false });
            }
            if (!valid()) { stream.getTracks().forEach(track => track.stop()); return null; }
            return stream;
        })();
        const stream = await Promise.race([permission, cancelled]);
        if (!stream || !valid()) {
            stream?.getTracks().forEach(track => track.stop()); c.finish('');
        } else {
            c.stream = stream; video.srcObject = stream; video.hidden = false;
            await video.play();
            clearTimeout(c.timeout);
            if (!valid()) {
                c.finish('');
            } else {
                let detector;
                try { if (window.BarcodeDetector) detector = new BarcodeDetector({ formats: ['qr_code'] }); } catch (_) {}
                let lastDecode = -Infinity;
                const loop = async timestamp => {
                    if (!valid()) { c.finish(''); return; }
                    // jsQR is CPU-bound. Avoid running it at the display's frame rate.
                    if (timestamp - lastDecode >= 180) {
                        lastDecode = timestamp;
                        try {
                            let value = '';
                            if (detector) {
                                try { value = (await detector.detect(video))[0]?.rawValue || ''; }
                                catch (_) { detector = null; }
                            }
                            if (!valid()) { c.finish(''); return; }
                            if (!value) value = decodeFrame(video);
                            if (value) { c.finish(value); return; }
                        } catch (_) { /* A video frame may not be ready yet. */ }
                    }
                    if (valid()) c.frame = requestAnimationFrame(loop);
                    else c.finish('');
                };
                c.frame = requestAnimationFrame(loop);
            }
        }
    } catch (error) {
        const message = error.name === 'NotAllowedError' || error.name === 'PermissionDeniedError'
            ? '未获得相机权限，请重试或使用相册扫码。'
            : error.name === 'NotFoundError' || error.name === 'DevicesNotFoundError'
                ? '未找到可用相机，请使用相册扫码或手动输入地址。'
                : '无法打开相机，请检查设备或使用相册扫码。';
        c.finish(valid() ? 'ERR:' + message : '');
    }
}
