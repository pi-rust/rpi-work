/* UI-only startup/setup fixture. Never probes, downloads or executes rpi. */
(() => {
  const dialog = document.querySelector('#runtime-dialog');
  const card = document.querySelector('#runtime-card');
  const locationInput = document.querySelector('#runtime-location');
  const installInput = document.querySelector('#runtime-install-dir');
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
  let pickerTicket = 0;
  let directoryTarget = 'location';
  const action = name => dialog.querySelector(`[data-runtime-action="${name}"]`);
  const busy = () => ['checking', 'downloading', 'validating'].includes(stage);
  const messages = {
    checking: '正在检查启动环境（演示）；不会扫描本机 PATH 或执行程序。',
    missing: '未找到 rpi（演示）。可下载配置，或选择已有的 rpi 程序 / 安装目录。',
    downloading: '正在下载官方发布包（演示）；未发起网络请求，也未写入磁盘。',
    downloaded: '下载完成（演示），请继续演示验证。真实安装须验证校验和及可执行性。',
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
    document.querySelector('#runtime-progress').hidden = !busy();
    for (const control of dialog.querySelectorAll('[data-runtime-action], .runtime-option input, #runtime-scenario')) control.disabled = busy();
    action('verify').disabled = busy() || !selection || stage === 'ready';
    action('verify').hidden = stage === 'ready';
    action('done').hidden = stage !== 'ready';
    document.querySelector('#runtime-download').hidden = stage === 'ready';
    document.querySelector('#runtime-manual').hidden = stage === 'ready';
  }
  function later(callback, delay = 700) {
    clearTimeout(timer);
    timer = setTimeout(callback, delay);
  }
  function check() {
    error();
    stage = 'checking'; render();
    later(() => { stage = confirmed ? 'ready' : 'missing'; render(); });
  }
  function open(manual = false) {
    error();
    if (manual) {
      stage = selection ? 'locating' : 'missing';
      locationInput.value = selection?.label ?? '';
    }
    render();
    if (!dialog.open) dialog.showModal();
    (manual ? locationInput : action(stage === 'ready' ? 'done' : 'download')).focus();
  }
  function select(label, kind, target = 'location') {
    ++pickerTicket;
    error();
    if (target === 'install') {
      installInput.value = label;
    } else {
      locationInput.value = label;
      selection = { demo: true, kind, label };
      stage = 'locating'; render();
    }
  }
  async function directory(target) {
    directoryTarget = target;
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
      select(handle.name + '/', 'directory-name', target);
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
      case 'directory': directory('location'); break;
      case 'install-directory': directory('install'); break;
      case 'done': dialog.close(); break;
      case 'recheck': check(); break;
      case 'download':
        ++pickerTicket;
        error(); selection = null; stage = 'downloading'; render();
        later(() => {
          if (scenario.value === 'download-error') {
            stage = 'error'; error('下载失败（演示）。未写入任何文件，可重试下载或手动定位 rpi。');
          } else {
            selection = { demo: true, kind: 'download-demo', label: installInput.value.trim() || '默认安装位置（演示）' };
            stage = 'downloaded';
          }
          render();
        }, 1000);
        break;
      case 'verify':
        if (!selection) return;
        ++pickerTicket;
        error(); stage = 'validating'; render();
        later(() => {
          if (scenario.value === 'verify-error') {
            stage = 'error'; error('验证失败（演示）。未确认 rpi 的版本或可执行性，请重新选择或重试。');
          } else {
            confirmed = { ...selection, demo: true };
            try { localStorage.setItem(key, JSON.stringify(confirmed)); } catch { /* Session-only setup remains usable. */ }
            stage = 'ready';
          }
          render();
        });
        break;
    }
  });
  installInput.addEventListener('input', () => { ++pickerTicket; });
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
    if (name && dialog.open && !busy() && Number(event.target.dataset.ticket) === pickerTicket) select(name + '/', 'directory-name', directoryTarget);
  });
  dialog.addEventListener('close', () => {
    clearTimeout(timer); ++pickerTicket;
    stage = confirmed ? 'ready' : 'missing';
    selection = confirmed;
    locationInput.value = confirmed?.label ?? '';
    error(); render();
  });
  check();
})();
