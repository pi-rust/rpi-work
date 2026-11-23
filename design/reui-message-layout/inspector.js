// The preview document runs in an opaque-origin iframe. It can request only
// navigation between fixture routes, opening a fixture file, or a page comment.
function projectPreviewDocument(route, projectName, branch, projectFiles) {
  const escape = value => String(value).replace(/[&<>"']/g, char => (
    { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[char]
  ));
  const fileList = projectFiles.map(file =>
    '<button class="file-link" data-path="' + escape(file.path) + '"><code>' +
    escape(file.path) + '</code><span aria-hidden="true">↗</span></button>').join('');
  const project = escape(projectName);
  const rpi = projectName === 'rpi-work';
  const settings = route === '/settings';
  let page;
  if (route === '/') {
    page = '<div class="doc-path"><span>项目文档</span><span>/</span><code>README.md</code></div>' +
      '<h1>项目概览</h1><h2>' + (rpi ? 'RPI Work' : project) + '</h2>' +
      '<p>' + (rpi ? '使用 Rust 与 Dioxus 构建的跨平台 Agent 工作台。桌面、浏览器和 Android 客户端共享项目、会话与工具运行状态。' : '在此工作区查看当前分支、打开示例文件或调整项目预览。') + '</p>' +
      (rpi ? '<h3>本地开发</h3><pre><span>$</span> task dev</pre>' : '') +
      '<h3>工作区文件</h3><div class="files">' + fileList + '</div>';
  } else if (settings) {
    page = '<div class="doc-path">项目文档<span>/</span>设置</div><h1>项目设置</h1>' +
      '<p>此表单可用于检查本地页面的输入与布局。</p><label>项目名称<input value="' + (rpi ? 'RPI Work' : project) + '"></label>' +
      '<label>测试命令<input value="npm test"></label>';
  } else {
    page = '<h1>页面未收录</h1><p>此地址没有本地预览。</p><button class="link" data-route="/">返回项目首页</button>';
  }
  return `<!doctype html><html lang="zh-CN"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
  <style>
    *{box-sizing:border-box}body{margin:0;color:#24262a;background:#fff;font:14px/1.7 -apple-system,BlinkMacSystemFont,"PingFang SC",sans-serif}
    button,input{font:inherit;color:inherit}button{border:0;background:none;cursor:pointer}button:hover{background:#f4f4f4}button:focus-visible{outline:2px solid #497acea}
    header{display:flex;align-items:center;gap:8px;padding:10px clamp(14px,5vw,32px);border-bottom:1px solid #e7e7e7;font-size:12px}
    header b{overflow:hidden;text-overflow:ellipsis;white-space:nowrap;margin-right:auto}nav{display:flex;gap:3px}nav button{border-radius:5px;padding:3px 7px;white-space:nowrap;font-size:12px;color:#67686c}nav [aria-current=page]{background:#f1f1f2;color:#242529}
    main{padding:clamp(22px,7vw,44px) clamp(16px,6vw,42px);max-width:810px;margin:auto}
    .doc-path{display:flex;align-items:center;gap:8px;font-size:11px;color:#6d7074;margin-bottom:16px}
    h1{font-size:clamp(23px,5.8vw,34px);line-height:1.35;letter-spacing:-.055em;margin:0 0 22px;font-weight:650}h2{font-size:19px;margin:0;letter-spacing:-.025em}
    p{margin:6px 0 22px;color:#55585c;line-height:1.85}h3{font-size:14px;margin:26px 0 9px}
    code,pre{font-family:ui-monospace,SFMono-Regular,Consolas,monospace}pre{background:#f7f7f8;border:1px solid #e5e5e6;border-radius:7px;font-size:12px;padding:10px 13px;margin:0}
    pre span{color:#84878b}.file-link{display:flex;width:100%;align-items:center;gap:8px;justify-content:space-between;padding:8px 0;border-bottom:1px solid #ececee;text-align:left}.file-link code{overflow-wrap:anywhere;font-size:12px}.file-link span{color:#7f8185}
    .branch{display:inline-block;font-size:11px;color:#62656a;background:#f4f4f5;border-radius:4px;padding:1px 6px;overflow-wrap:anywhere}footer{padding-top:22px}
    label{display:grid;gap:5px;font-size:12px;margin:14px 0}input{border:1px solid #d9d9db;border-radius:6px;padding:7px 9px;max-width:100%}button.link{text-decoration:underline}
    html.annotating body,html.annotating button{cursor:crosshair}
    .annotation-hover{outline:2px solid #477cda;outline-offset:2px;background-color:#477cda0b}
  </style><header><b>${project}</b><nav aria-label="项目文档"><button data-route="/" ${route === '/' ? 'aria-current="page"' : ''}>概览</button><button data-route="/settings" ${settings ? 'aria-current="page"' : ''}>设置</button></nav></header>
  <main>${page}<footer><code class="branch">${escape(branch)}</code></footer></main><script>
    let annotating = false;
    const send = (type, value) => parent.postMessage({ channel: 'project-preview', type, value }, '*');
    const marker = () => document.querySelector('.annotation-hover');
    window.addEventListener('message', e => {
      if (e.source !== parent || e.data?.type !== 'annotation-mode') return;
      annotating = e.data.active === true;
      document.documentElement.classList.toggle('annotating', annotating);
      if (!annotating) marker()?.classList.remove('annotation-hover');
    });
    document.addEventListener('pointerover', e => {
      if (!annotating) return;
      const target = e.target.closest('h1,h2,h3,p,button,label,input,pre,.branch');
      marker()?.classList.remove('annotation-hover');
      target?.classList.add('annotation-hover');
    });
    document.addEventListener('click', e => {
      if (annotating) {
        e.preventDefault();
        const node = e.target.closest('h1,h2,h3,p,button,label,input,pre,.branch');
        if (node) send('annotate', [node.localName, (node.innerText || node.value || '').trim().slice(0, 48)].join(' · '));
        return;
      }
      const button = e.target.closest('button');
      if (button?.dataset.route) send('navigate', button.dataset.route);
      if (button?.dataset.path) send('file', button.dataset.path);
    });
    send('ready');
  <\/script></html>`;
}

function initializeInspector() {
  const pane = $('#inspector');
  const handle = $('.inspector-divider');
  const expand = $('[data-action="expand-inspector"]');
  const widthSetting = 'rpi-inspector-width';
  let preferredWidth;
  try { preferredWidth = Number(localStorage.getItem(widthSetting)) || undefined; } catch {}
  let dragging = false;

  function limits() {
    const sidebar = $('.sidebar').getBoundingClientRect().width;
    const rail = $('.activity-rail').getBoundingClientRect().width;
    return { min: 320, max: Math.max(320, Math.floor(shell.clientWidth - rail - sidebar - 350)) };
  }
  function syncWidth() {
    if (narrow.matches) {
      shell.style.removeProperty('--dock-width');
      return;
    }
    const {min, max} = limits();
    const width = Math.min(max, Math.max(min, preferredWidth || Math.round(shell.clientWidth * .38)));
    shell.style.setProperty('--dock-width', width + 'px');
    handle.setAttribute('aria-valuemax', max);
    handle.setAttribute('aria-valuenow', width);
  }
  function resize(width) {
    const {min, max} = limits();
    preferredWidth = Math.min(max, Math.max(min, Math.round(width)));
    syncWidth();
  }
  function storeWidth() {
    try {
      if (preferredWidth) localStorage.setItem(widthSetting, preferredWidth);
      else localStorage.removeItem(widthSetting);
    } catch {}
  }
  handle.addEventListener('pointerdown', event => {
    if (event.button !== 0) return;
    dragging = true;
    handle.setPointerCapture(event.pointerId);
    shell.classList.add('resizing-inspector');
    event.preventDefault();
  });
  handle.addEventListener('pointermove', event => {
    if (dragging) resize(shell.getBoundingClientRect().right - event.clientX);
  });
  handle.addEventListener('lostpointercapture', () => {
    dragging = false;
    shell.classList.remove('resizing-inspector');
    storeWidth();
  });
  handle.addEventListener('dblclick', () => {
    preferredWidth = undefined;
    syncWidth();
    storeWidth();
  });
  handle.addEventListener('keydown', event => {
    if (!['ArrowLeft','ArrowRight','Home','End'].includes(event.key)) return;
    event.preventDefault();
    const {min, max} = limits();
    const step = event.shiftKey ? 80 : 16;
    if (event.key === 'Home') resize(min);
    else if (event.key === 'End') resize(max);
    else resize(pane.offsetWidth + (event.key === 'ArrowLeft' ? step : -step));
    storeWidth();
  });
  function fullView(full) {
    shell.dataset.dockFull = String(full);
    const label = full ? '返回分栏视图' : '进入完整视图';
    expand.setAttribute('aria-label', label);
    expand.title = label;
    expand.setAttribute('aria-pressed', full);
  }
  expand.addEventListener('click', () => fullView(shell.dataset.dockFull !== 'true'));
  document.addEventListener('keydown', event => {
    if (!(event.metaKey || event.ctrlKey) || !event.shiftKey || event.code !== 'KeyB' || $('dialog[open]')) return;
    event.preventDefault();
    if (shell.dataset.dockFull === 'true') fullView(false);
    else setPanel('inspector', !reviewing);
  });
  new MutationObserver(() => {
    if (shell.dataset.inspector === 'closed') fullView(false);
    syncWidth();
  }).observe(shell, {attributes: true, attributeFilter: ['data-inspector','data-navigation']});
  new ResizeObserver(syncWidth).observe(shell);
  narrow.addEventListener('change', () => { fullView(false); syncWidth(); });
  fullView(false);
  syncWidth();
}

// Keep granted File handles and their edited text in this page's memory, scoped
// to the project that opened the picker. Reading starts only for the active file.
function buildSourceFolder(fileList) {
  const root = {directories: new Map(), entries: []};
  const entries = [];
  for (const handle of fileList) {
    const [, ...parts] = handle.webkitRelativePath.split('/');
    if (!parts.length) continue;
    const path = parts.join('/');
    let node = root;
    for (const part of parts.slice(0,-1)) {
      if (!node.directories.has(part)) node.directories.set(part, {directories: new Map(), entries: []});
      node = node.directories.get(part);
    }
    node.entries.push(entries.length);
    entries.push({name: handle.name, path, start: 1, handle});
  }
  if (!entries.length) return;
  let selected = entries.findIndex(entry => /^readme(\.|$)/i.test(entry.path));
  if (selected < 0) selected = 0;
  const parents = entries[selected].path.split('/').slice(0,-1);
  const expanded = new Set(parents.map((_,index) => parents.slice(0,index+1).join('/')));
  return {
    name: fileList[0].webkitRelativePath.split('/')[0], selected, root, entries,
    expanded, opened: [], enabled: true, edits: new Map(), pending: new Map()
  };
}

function renderSourceDirectory(host, folder, makeButton, node = folder.root, prefix = '') {
  for (const [name, children] of [...node.directories].sort(([a],[b]) => a.localeCompare(b))) {
    const path = prefix + name;
    const details = document.createElement('details');
    details.className = 'file-folder local-folder';
    const title = document.createElement('summary');
    title.textContent = name;
    const list = document.createElement('div');
    details.append(title, list);
    function expand() {
      if (details.open) {
        folder.expanded.add(path);
        if (!list.childElementCount) renderSourceDirectory(list, folder, makeButton, children, path + '/');
      } else {
        folder.expanded.delete(path);
        list.replaceChildren();
      }
    }
    details.open = folder.expanded.has(path);
    if (details.open) expand();
    details.addEventListener('toggle', expand);
    host.append(details);
  }
  for (const index of [...node.entries].sort((a,b) => folder.entries[a].name.localeCompare(folder.entries[b].name))) {
    host.append(makeButton(folder.entries[index], index));
  }
}

async function readLocalSource(entry) {
  if (entry.handle.size > 1024 * 1024) throw new Error('此文件超过 1 MiB，无法在此预览。');
  if (/^(image|audio|video|font)\//.test(entry.handle.type) || entry.handle.type === 'application/pdf') {
    throw new Error('此文件为二进制格式，无法显示源码。');
  }
  let bytes;
  try { bytes = new Uint8Array(await entry.handle.arrayBuffer()); }
  catch { throw new Error('无法读取此文件。请重新打开文件夹。'); }
  if (bytes.some(byte => byte < 32 && ![9,10,12,13].includes(byte))) {
    throw new Error('此文件为二进制格式，无法显示源码。');
  }
  try { return new TextDecoder('utf-8', {fatal: true}).decode(bytes); }
  catch { throw new Error('此文件使用了不支持的编码。可预览 UTF-8 文本文件。'); }
}
