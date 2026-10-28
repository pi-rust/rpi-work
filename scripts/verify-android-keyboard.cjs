// Requires an explicitly selected review device and an open conversation. Never sends a message.
const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const { execFileSync } = require('node:child_process');
const { once } = require('node:events');
const serial = process.env.ANDROID_SERIAL;
if (!serial) throw Error('Set ANDROID_SERIAL to the review emulator/device explicitly.');
const adbPath = process.env.ADB_PATH || path.join(process.env.ANDROID_HOME || process.env.ANDROID_SDK_ROOT || '', 'platform-tools', process.platform === 'win32' ? 'adb.exe' : 'adb');
const adb = (...args) => execFileSync(adbPath, ['-s', serial, ...args], { encoding: 'utf8', timeout: 15000 });
const pause = () => new Promise(resolve => setTimeout(resolve, 200));
const imeVisible = () => /mInputShown=true/.test(adb('shell', 'dumpsys', 'input_method'));
const waitFor = async predicate => { for (let i = 0; i < 40; i++) { if (await predicate()) return; await pause(); } throw Error('Timed out waiting for keyboard/window state'); };
(async () => {
  const appPid = adb('shell', 'pidof', 'com.pirust.rpiwork').trim();
  assert.match(appPid, /^\d+$/, 'Launch the review app first');
  const port = adb('forward', 'tcp:0', `localabstract:webview_devtools_remote_${appPid}`).trim();
  let ws;
  try {
    const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
    const target = targets.find(item => item.url.startsWith('https://dioxus.index.html/'));
    assert(target, 'Expected the local native Dioxus WebView');
    ws = new WebSocket(target.webSocketDebuggerUrl);
    await once(ws, 'open');
    let seq = 0;
    const pending = new Map();
    ws.onmessage = event => { const message = JSON.parse(event.data); const item = pending.get(message.id); if (!item) return; pending.delete(message.id); clearTimeout(item.timer); message.error ? item.reject(Error(message.error.message)) : item.resolve(message.result); };
    const evaluate = expression => new Promise((resolve, reject) => {
      const id = ++seq;
      const timer = setTimeout(() => { pending.delete(id); reject(Error('WebView evaluation timed out')); }, 10000);
      pending.set(id, { timer, reject, resolve: result => result.exceptionDetails ? reject(Error('WebView evaluation failed')) : resolve(result.result.value) });
      ws.send(JSON.stringify({ id, method: 'Runtime.evaluate', params: { expression, returnByValue: true } }));
    });
    if (imeVisible()) { adb('shell', 'input', 'keyevent', '4'); await waitFor(() => !imeVisible()); }
    const measure = () => evaluate(`(()=>{const rect=s=>document.querySelector(s)?.getBoundingClientRect().toJSON();return {height:innerHeight,dpr:devicePixelRatio,textarea:rect('.composer-main textarea'),send:rect('button[aria-label="发送消息"]'),overflow:document.documentElement.scrollWidth>innerWidth+1}})()`);
    const before = await measure();
    assert(before.textarea?.height > 0 && before.send?.height > 0, 'Open a conversation with the composer visible');
    const xmlFile = '/sdcard/rpi-work-keyboard-review.xml';
    adb('shell', 'uiautomator', 'dump', xmlFile);
    const xml = adb('shell', 'cat', xmlFile);
    const bounds = xml.match(/class="android\.webkit\.WebView"[^>]*bounds="\[(\d+),(\d+)\]\[(\d+),(\d+)\]"/);
    assert(bounds, 'Cannot determine the native WebView screen offset');
    const x = Math.round(+bounds[1] + (before.textarea.x + before.textarea.width / 2) * before.dpr);
    const y = Math.round(+bounds[2] + (before.textarea.y + before.textarea.height / 2) * before.dpr);
    adb('shell', 'input', 'tap', String(x), String(y));
    await waitFor(imeVisible);
    await waitFor(async () => (await measure()).height < before.height - 100);
    // Wait for the IME transition and the native WebView to paint the resized layout.
    for (let frame = 0; frame < 5; frame++) await pause();
    const during = await measure();
    assert(during.textarea.top >= 0 && during.textarea.bottom <= during.height + 1, 'Keyboard obscures the input');
    assert(during.send.top >= 0 && during.send.bottom <= during.height + 1, 'Keyboard obscures Send');
    assert.equal(during.overflow, false, 'Keyboard causes horizontal overflow');
    const screenshot = '/sdcard/rpi-work-keyboard-review.png';
    adb('shell', 'screencap', '-p', screenshot);
    adb('pull', screenshot, 'artifacts/android-keyboard-review.png');
    adb('shell', 'input', 'keyevent', '4');
    await waitFor(async () => !imeVisible() && (await measure()).height === before.height);
    fs.writeFileSync('artifacts/android-keyboard-review.json', JSON.stringify({ serial, before, during, restored: true }, null, 2));
    console.log('PASS real Android keyboard resizes the WebView, keeps input and Send visible, and restores the window');
  } finally {
    if (imeVisible()) adb('shell', 'input', 'keyevent', '4');
    ws?.close();
    adb('forward', '--remove', `tcp:${port}`);
  }
})().catch(error => { console.error(error.message); process.exitCode = 1; });
