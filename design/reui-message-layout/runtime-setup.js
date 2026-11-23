/* UI-only startup/setup fixture. Never probes, downloads or executes rpi. */
(() => {
  const dialog = document.querySelector('#runtime-dialog');
  const card = document.querySelector('#runtime-card');
  const locationInput = document.querySelector('#runtime-location');
  const progress = document.querySelector('#runtime-progress');
  const progressLabel = document.querySelector('#runtime-progress-label');
  const scenario = document.querySelector('#runtime-scenario');
  const key = 'rpi-work-runtime-setup-demo-v1';
  const fixture = new URLSearchParams(location.search).get('runtime');
  let confirmed = null;
  try {
    const saved = JSON.parse(localStorage.getItem(key));
    if (saved?.demo === true && typeof saved.label === 'string' && saved.label.trim()) confirmed = saved;
  } catch { /* Disabled or malformed storage does not prevent setup. */ }
  if (fixture === 'ready') confirmed = { demo: true, kind: 'fixture', label: '已有 rpi（演示）' };
  if (fixture === 'missing') confirmed = null;
  if (['download-error', 'verify-error'].includes(fixture)) scenario.value = fixture;
  let stage = 'checking';
  let selection = confirmed;
  let timer;
  let returnFocus = null;
  let pickerTicket = 0;
  let downloadProgress = 0;
  const action = name => dialog.querySelector(`[data-runtime-action="${name}"]`);
  const busy = () => ['checking', 'downloading', 'validating'].includes(stage);
  const messages = {
    checking: '正在检查启动环境（演示）；不会扫描本机 PATH 或执行程序。',
    missing: '未找到 rpi（演示）。可下载配置，或选择已有的 rpi 程序 / 安装目录。',
    downloading: '正在下载官方发布包（演示）；未发起网络请求，也未写入磁盘。',
    locating: '已选择安装位置，等待演示验证。浏览器无法确认其内容或版本。',
    validating: '正在验证版本与可执行性（演示）；没有运行 rpi。',
    ready: 'rpi 配置已保存（演示）。真实检测、安装及执行仍需接入桌面运行服务。',
    error: '设置未完成。可以重试，也可以选择已有 rpi。',
  };
  function error(text = '') {
    document.querySelector('#runtime-error').textContent = text;
    document.querySelector('#runtime-error').hidden = !text;
  }
  function render() {
    card.dataset.state = stage;
    document.querySelector('#runtime-status').textContent = stage === 'ready' ? 'rpi 已配置' : stage === 'missing' ? '未找到 rpi' : busy() ? '正在设置 rpi…' : 'rpi 待配置';
    document.querySelector('#runtime-card-note').textContent = stage === 'ready' ? '仅保存演示配置，未运行本机程序。' : '下载 rpi，或选择已有程序 / 安装目录。';
    const open = card.querySelector('[data-runtime-action="setup"]');
    open.textContent = stage === 'ready' ? '查看配置' : '下载引导';
    for (const button of card.querySelectorAll('button')) button.disabled = busy();
    document.querySelector('#runtime-phase').textContent = messages[stage] + (stage === 'ready' && confirmed ? ' 位置：' + confirmed.label : '');
    progress.hidden = progressLabel.hidden = !busy();
    if (stage === 'downloading' || (stage === 'validating' && selection?.kind === 'download-demo')) progress.value = downloadProgress;
    else progress.removeAttribute('value');
    progressLabel.textContent = stage === 'downloading' ? `正在下载 · ${downloadProgress}%（演示）` : stage === 'validating' ? '正在自动校验，完成后回填路径（演示）' : '正在检测（演示）';
    for (const control of dialog.querySelectorAll('[data-runtime-action], .runtime-option input, #runtime-scenario')) control.disabled = busy();
    action('verify').disabled = busy() || !selection || stage === 'ready';
    action('verify').hidden = !selection || stage === 'ready' || stage === 'downloading' || selection?.kind === 'download-demo';
    action('done').hidden = stage !== 'ready';
    action('download').hidden = stage === 'ready';
  }
  function later(callback, delay = 700) {
    clearTimeout(timer);
    timer = setTimeout(callback, delay);
  }
  function check(showIfMissing = false) {
    error();
    stage = 'checking'; render();
    later(() => {
      stage = confirmed ? 'ready' : 'missing'; render();
      if (showIfMissing && stage === 'missing') open();
    });
  }
  function open(manual = false) {
    if (!dialog.open) {
      const node = document.activeElement;
      returnFocus = { node, start: node.selectionStart, end: node.selectionEnd, direction: node.selectionDirection };
    }
    error();
    if (manual) stage = selection ? 'locating' : 'missing';
    locationInput.value = selection?.label ?? '';
    render();
    if (!dialog.open) dialog.showModal();
    (manual ? locationInput : action(stage === 'ready' ? 'done' : 'download')).focus();
  }
  function select(label, kind) {
    ++pickerTicket;
    error();
    locationInput.value = label;
    selection = { demo: true, kind, label };
    stage = 'locating'; render();
  }
  function verify() {
    if (!selection) return;
    ++pickerTicket;
    error(); stage = 'validating'; render();
    later(() => {
      if (!dialog.open) return;
      if (scenario.value === 'verify-error') {
        stage = 'error'; error('验证失败（演示）。未确认 rpi 的版本或可执行性，请重新选择或重试。');
      } else {
        confirmed = { ...selection, demo: true };
        locationInput.value = confirmed.label;
        try { localStorage.setItem(key, JSON.stringify(confirmed)); } catch { /* Session-only setup remains usable. */ }
        stage = 'ready';
      }
      render();
    });
  }
  function download() {
    ++pickerTicket;
    downloadProgress = 0;
    error(); selection = null; stage = 'downloading'; render();
    const tick = () => {
      if (!dialog.open) return;
      downloadProgress += 20;
      if (downloadProgress < 100) {
        render(); later(tick, 200);
      } else if (scenario.value === 'download-error') {
        stage = 'error'; error('下载失败（演示）。未写入任何文件，可重试下载或手动定位 rpi。'); render();
      } else {
        // Example result from a future desktop installer, not a real local path.
        selection = { demo: true, kind: 'download-demo', label: '~/.local/bin/rpi' };
        verify();
      }
    };
    later(tick, 200);
  }
  async function directory() {
    const ticket = ++pickerTicket;
    if (typeof window.showDirectoryPicker !== 'function') {
      document.querySelector('#runtime-directory-picker').dataset.ticket = String(ticket);
      document.querySelector('#runtime-directory-picker').click();
      return;
    }
    try {
      const handle = await window.showDirectoryPicker({ mode: 'read' });
      if (ticket !== pickerTicket || !dialog.open) return;
      if (handle.kind !== 'directory' || !handle.name) throw new Error('Invalid directory');
      select(handle.name + '/', 'directory-name');
    } catch (failure) {
      if (ticket === pickerTicket && dialog.open && failure.name !== 'AbortError') error('无法打开目录选择器，可手动填写位置。已有选择未改变。');
    }
  }
  document.addEventListener('click', event => {
    const button = event.target.closest('[data-runtime-action]');
    if (!button || button.disabled) return;
    switch (button.dataset.runtimeAction) {
      case 'setup': open(); break;
      case 'locate': open(true); break;
      case 'program':
        document.querySelector('#runtime-program-picker').dataset.ticket = String(++pickerTicket);
        document.querySelector('#runtime-program-picker').click();
        break;
      case 'directory': directory(); break;
      case 'done': dialog.close(); break;
      case 'recheck': check(); break;
      case 'download': download(); break;
      case 'verify': verify(); break;
    }
  });
  locationInput.addEventListener('input', () => {
    ++pickerTicket;
    error();
    selection = locationInput.value.trim() ? { demo: true, kind: 'manual-path', label: locationInput.value.trim() } : null;
    stage = selection ? 'locating' : 'missing'; render();
  });
  document.querySelector('#runtime-program-picker').addEventListener('change', event => {
    const file = event.target.files[0];
    event.target.value = '';
    if (!file || !dialog.open || busy() || Number(event.target.dataset.ticket) !== pickerTicket) return;
    if (!/^rpi(?:\.exe)?$/i.test(file.name)) return error('请选择名为 rpi 或 rpi.exe 的程序文件。已有选择未改变。');
    select(file.name, 'file-name');
  });
  document.querySelector('#runtime-directory-picker').addEventListener('change', event => {
    const name = event.target.files[0]?.webkitRelativePath.split('/')[0];
    event.target.value = '';
    if (name && dialog.open && !busy() && Number(event.target.dataset.ticket) === pickerTicket) select(name + '/', 'directory-name');
  });
  dialog.addEventListener('close', () => {
    clearTimeout(timer); ++pickerTicket;
    stage = confirmed ? 'ready' : 'missing';
    selection = confirmed;
    locationInput.value = confirmed?.label ?? '';
    error(); render();
    const previous = returnFocus;
    returnFocus = null;
    if (previous?.node.isConnected) {
      previous.node.focus({ preventScroll: true });
      if (typeof previous.start === 'number') previous.node.setSelectionRange(previous.start, previous.end, previous.direction);
    }
  });
  check(true);
})();
