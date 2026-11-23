/* Browser-local collaboration review only: no requests, listeners, cameras or credentials. */
function initializeCollaboration({ save, toast }) {
  const section = $('[data-settings-section="connections"]');
  const dialog = $('#collab-dialog');
  const body = $('#collab-dialog-body');
  const stored = workbench.collaboration;
  const validAddress = raw => {
    const url = new URL(raw);
    if (!['http:', 'https:'].includes(url.protocol) || !url.hostname || url.username || url.password || url.search || url.hash) throw Error('address');
    return url.href;
  };
  const nodes = projectNodes.map(node => ({ ...node, address: node.id === 'local' ? 'http://192.0.2.10:13031/' : 'https://devbox.example:13031/' }));
  for (const item of Array.isArray(stored?.nodes) ? stored.nodes.slice(0, 30) : []) {
    try {
      if (!item || !/^node-[a-z0-9-]{1,64}$/.test(item.id) || nodes.some(node => node.id === item.id)) continue;
      const address = validAddress(item.address);
      if (nodes.some(node => node.address === address)) continue;
      const node = { id: item.id, name: address ? new URL(address).hostname : '', address, icon: 'globe', demo: true, detail: 'HTTP + WebSocket · 演示' };
      nodes.push(node);
      projectNodes.push(node);
    } catch { /* Invalid saved metadata is not a connected node. */ }
  }
  const state = {
    current: nodes.some(node => node.id === stored?.current && (node.id === 'local' || workbench.connections[node.id] === true)) ? stored.current : 'local',
    sharing: stored?.sharing === true, web: stored?.web === true, discoverable: stored?.discoverable === true,
    recent: Array.isArray(stored?.recent) ? [...new Set(stored.recent.filter(id => id !== 'local' && nodes.some(node => node.id === id)))].slice(0, 12) : [],
    nodes: nodes.filter(node => node.id.startsWith('node-')).map(({ id, address }) => ({ id, address })),
  };
  workbench.collaboration = state;
  const nearby = [nodes[1], { name: '办公室工作站', address: 'http://192.0.2.20:13031/', icon: 'globe' }];
  let scenario = 'success', discovered = null, discoveryTimer, connectionTimer, authTimer, authUntil = 0, connecting = false;
  let returnFocus, modalKind = '', selected, revoked = false;
  const current = () => nodes.find(node => node.id === state.current) ?? nodes[0];
  const connected = node => node.id === 'local' || workbench.connections[node.id] === true;
  const feedback = message => { $('#collab-feedback').textContent = message; };
  function button(label, action, id) {
    const node = element('button', 'action-button', label);
    node.type = 'button'; node.dataset.collabAction = action;
    if (id) node.dataset.collabId = id;
    return node;
  }
  function note(text, className = 'collab-note') { return element('p', className, text); }
  function row(node, actions, status) {
    const entry = element('div', 'connection-row'); entry.dataset.collabNode = node.id ?? node.address;
    const mark = element('span', 'collab-node-mark'); mark.innerHTML = icon(node.icon ?? 'globe');
    const words = element('div', 'collab-row-words'); words.append(element('strong', '', node.name), element('small', '', node.address));
    const tools = element('div', 'collab-actions'); tools.append(...actions);
    entry.append(mark, words, element('span', 'connection-status', status), tools);
    return entry;
  }
  function render() {
    const node = current(), local = node.id === 'local', available = scenario !== 'service-unavailable';
    $('#collab-current-icon').innerHTML = icon(node.icon);
    $('#collab-current-kind').textContent = local ? '当前节点 · 本机' : '当前节点 · 远程';
    $('#collab-current-name').textContent = node.name;
    $('#collab-current-address').textContent = node.address;
    $('#collab-current-meta').textContent = local ? '本机使用 RPC · 上方为其他设备的示例访问地址' : 'HTTP + WebSocket · 演示身份，不代表实际连接或鉴权';
    $('#collab-disconnect').hidden = local;
    $('#collab-authorize').disabled = !local || !state.sharing || !available;
    $('#collab-authorize').title = local ? '先开启远程协作，再演示 Android App 一次性授权' : '只能在拥有节点的电脑上签发授权';
    $('#collab-local-access').hidden = !local; $('#collab-remote-note').hidden = local;
    $('#collab-visitors').hidden = !local;
    for (const key of ['sharing', 'web', 'discoverable']) {
      const input = $('#collab-' + key); input.checked = state[key]; input.disabled = !local || (!available && key !== 'discoverable');
    }
    $('#collab-sharing-note').textContent = !available ? '入口未就绪（演示）。需准备 rpi-http 节点组件；此处不安装或监听。' : state.sharing ? '远程协作已开启（演示）；App 连接仍需鉴权。' : 'App 访问项目、会话与任务；开启后仍需鉴权。';
    $('#collab-web-note').textContent = !available ? 'Web 资源未就绪（演示）；不影响另一项权限的已存偏好。' : state.web ? 'Web 访问已开启（演示），不自动授予 App 权限。' : '独立控制浏览器入口，不跟随远程协作开关。';
    $('#connection-list').replaceChildren(...nodes.map(item => {
      const actions = [button('设置', 'node-settings', item.id)];
      if (!connected(item)) actions.push(button('模拟连接', 'connect-known', item.id));
      else if (item.id !== state.current) actions.push(button('切换', 'switch', item.id));
      return row(item, actions, item.id === state.current ? '当前 · 演示已连接' : connected(item) ? '演示已连接' : '未连接');
    }));
    const recent = state.recent.map(id => nodes.find(node => node.id === id));
    $('#collab-recent').replaceChildren(...(recent.length ? recent.map(item => row(item, [button(item.id === state.current ? '查看' : connected(item) ? '切换' : '模拟连接', item.id === state.current ? 'node-settings' : connected(item) ? 'switch' : 'connect-known', item.id)], item.id === state.current ? '当前' : '演示记录')) : [note('还没有最近连接。连接其他节点后会显示在这里。', 'collab-empty')]));
    $('#collab-visitor-list').replaceChildren(...(scenario === 'visitors' && !revoked ? [row({ name: 'Android 手机', address: 'App · 一次性授权 · 示例记录', icon: 'globe' }, [button('撤销演示授权', 'revoke')], '演示已授权')] : [note('暂无访问记录。这里展示访问本机的设备，不是本机连接过的节点。', 'collab-empty')]));
  }
  function restoreAction(action, id) {
    const target = [...section.querySelectorAll('[data-collab-action]')].find(item => item.dataset.collabAction === action && item.dataset.collabId === id);
    (target ?? section.querySelector(`[data-collab-id="${id}"]`) ?? $('#collab-current-name')).focus();
  }
  function switchNode(id) {
    const node = nodes.find(item => item.id === id); if (!node || !connected(node)) return;
    state.current = id; save(); render(); feedback('已切换节点选择（演示）；当前对话、草稿和项目未改变。');
  }
  function disconnect(node) {
    if (node.id === 'local') return;
    workbench.connections[node.id] = false;
    if (state.current === node.id) state.current = 'local';
    save(); render(); feedback('已模拟断开，返回本机；节点上的项目、任务和授权不会被删除。');
  }
  function closeTransient() {
    clearTimeout(connectionTimer); clearInterval(authTimer); connecting = false; authUntil = 0;
    for (const control of section.querySelectorAll('[data-collab-action="connect-known"]')) { control.disabled = false; control.textContent = '模拟连接'; }
  }
  function open(kind, title) {
    closeTransient(); modalKind = kind; selected = null;
    returnFocus = document.activeElement; body.replaceChildren(); $('#collab-dialog-title').textContent = title;
    dialog.showModal();
  }
  function connect(node, modal = false, source = null) {
    if (connecting) return;
    connecting = true;
    const confirm = modal ? body.querySelector('[data-collab-action="confirm"]') : null;
    if (confirm) { confirm.disabled = true; confirm.textContent = '正在模拟连接…'; }
    const trigger = modal ? null : source;
    if (trigger) { trigger.disabled = true; trigger.textContent = '正在连接…'; }
    const result = modal ? $('#collab-connect-status') : $('#collab-feedback'); result.textContent = '正在模拟连接…';
    connectionTimer = setTimeout(() => {
      connecting = false;
      if (trigger?.isConnected) { trigger.disabled = false; trigger.textContent = '模拟连接'; }
      if (!settingsDialog.open || section.hidden || (modal && !dialog.open)) return;
      if (scenario === 'connect-error') {
        result.textContent = '连接失败（演示）。未修改当前节点或保存连接，可重试。';
        if (confirm) { confirm.disabled = false; confirm.textContent = '重试模拟连接'; }
        if (trigger) trigger.closest('.connection-row').querySelector('.connection-status').textContent = '连接失败（演示）';
        return;
      }
      let known = nodes.find(item => item.address === node.address);
      if (!known) {
        if (state.nodes.length >= 30) { result.textContent = '演示节点已达 30 个，请使用已有节点。'; if (confirm) confirm.disabled = false; return; }
        known = { id: 'node-' + crypto.randomUUID(), name: new URL(node.address).hostname, address: node.address, icon: 'globe', demo: true, detail: 'HTTP + WebSocket · 演示' };
        nodes.push(known); projectNodes.push(known); state.nodes.push({ id: known.id, address: known.address });
        $('#project-node').append(new Option(known.name + '（演示）', known.id));
        renderNavigation();
      }
      if (known.id !== 'local') {
        workbench.connections[known.id] = true;
        state.recent = [known.id, ...state.recent.filter(id => id !== known.id)].slice(0, 12);
      }
      save(); render(); result.textContent = '演示连接完成。可在节点列表切换；未自动改变当前节点。';
      if (modal) dialog.close(); else restoreAction('node-settings', known.id);
      feedback('演示连接完成，可在节点列表切换；未自动改变当前节点。');
    }, 500);
  }
  function preview(address) {
    try {
      const safe = validAddress(address);
      selected = nodes.find(node => node.address === safe) ?? { address: safe, name: new URL(safe).hostname };
      const box = $('#collab-connect-preview'); box.replaceChildren(element('strong', '', selected.name), note('地址：' + safe), note('HTTP(S) + WebSocket · 演示信息，未向节点发起探测。'), note('正式连接仍需密码或一次性授权；此原型不收集密码。'), button('确认模拟连接', 'confirm'));
      $('#collab-connect-status').textContent = ''; $('#collab-connect-address').removeAttribute('aria-invalid');
    } catch {
      selected = null; $('#collab-connect-preview').replaceChildren();
      $('#collab-connect-address').setAttribute('aria-invalid', 'true');
      $('#collab-connect-status').textContent = '请输入不含账户、密码、查询参数或片段的完整 HTTP(S) 节点地址。';
    }
  }
  function manual(address = '', scanning = false) {
    open('connect', scanning ? '扫码连接 · 演示' : '连接其他节点');
    body.append(note(scanning ? '原型不启动摄像头、不读取相册；可输入节点地址继续演示。' : '先预览节点信息，再确认模拟连接；不会自动切换当前节点。'));
    const form = element('form', 'dialog-form'); form.id = 'collab-connect-form'; form.noValidate = true;
    form.innerHTML = '<label for="collab-connect-address">节点地址</label><input id="collab-connect-address" type="url" required maxlength="500" spellcheck="false" autocomplete="off" placeholder="https://node.example:13031/" aria-describedby="collab-connect-status"><button class="action-button" type="submit">预览节点</button>';
    body.append(form, element('p', 'collab-note'), element('div', 'collab-connect-preview'));
    body.children[2].id = 'collab-connect-status'; body.children[2].setAttribute('role', 'status'); body.lastChild.id = 'collab-connect-preview';
    $('#collab-connect-address').value = address;
    form.addEventListener('submit', event => { event.preventDefault(); preview($('#collab-connect-address').value.trim()); });
    $('#collab-connect-address').addEventListener('input', () => { closeTransient(); selected = null; $('#collab-connect-address').removeAttribute('aria-invalid'); $('#collab-connect-preview').replaceChildren(); $('#collab-connect-status').textContent = ''; });
    if (address) preview(address);
    $('#collab-connect-address').focus();
  }
  function authorization() {
    if (current().id !== 'local' || !state.sharing || scenario === 'service-unavailable') return;
    open('authorize', '一次性授权 · 演示');
    body.append(note('Android App · 120 秒有效、仅限一次；浏览器仍需独立登录。'), note(current().address), note('演示占位 · 不含票据，不可扫码', 'collab-code-placeholder'), note('', 'collab-auth-status'), button('重新生成演示', 'regenerate'), note('不签发真实授权。正式票据在使用或到期前可能仍有效，关闭弹窗不等于撤销；关闭远程协作可撤销该入口授权。'));
    body.querySelector('.collab-auth-status').setAttribute('role', 'status');
    regenerate();
  }
  function regenerate() {
    if (!dialog.open || modalKind !== 'authorize' || !state.sharing || scenario === 'service-unavailable') return;
    clearInterval(authTimer); authUntil = Date.now() + 120000;
    const tick = () => {
      const seconds = Math.max(0, Math.ceil((authUntil - Date.now()) / 1000));
      body.querySelector('.collab-auth-status').textContent = seconds ? `演示有效期剩余 ${seconds} 秒` : '演示已过期，请重新生成。';
      body.querySelector('.collab-code-placeholder').hidden = !seconds;
      if (!seconds) clearInterval(authTimer);
    };
    body.querySelector('.collab-code-placeholder').hidden = false; tick(); authTimer = setInterval(tick, 1000);
  }
  async function copyAddress() {
    const node = current();
    try {
      if (!navigator.clipboard?.writeText) throw Error('clipboard');
      await navigator.clipboard.writeText(node.address);
      if (settingsDialog.open && current().id === node.id) feedback('已复制示例地址，不含密码或授权信息。');
    } catch {
      if (settingsDialog.open && current().id === node.id) feedback('无法复制，可手动选择上方示例地址；未改动草稿。');
    }
  }
  function refresh() {
    if (discoveryTimer) return;
    $('#collab-refresh').disabled = true; $('#collab-discovery-status').textContent = '正在模拟发现…';
    discoveryTimer = setTimeout(() => {
      discoveryTimer = null; $('#collab-refresh').disabled = false;
      if (!settingsDialog.open || section.hidden) { $('#collab-discovery-status').textContent = '发现已取消，可重新刷新'; return; }
      discovered = scenario === 'discovery-empty' ? [] : nearby;
      $('#collab-discovery-status').textContent = `发现 ${discovered.length} 个节点（演示）`;
      $('#collab-discovered').replaceChildren(...(discovered.length ? discovered.map((node, index) => row(node, [button('连接', 'nearby', String(index))], '未认证 · 演示')) : [note('没有发现附近节点，可输入地址继续连接。', 'collab-empty')]));
    }, 500);
  }
  function nodeSettings(node) {
    open('node', node.name + ' · 节点设置');
    body.append(note(node.address), note(node.id === state.current ? '当前节点 · 演示' : connected(node) ? '演示已连接 · 未在使用' : '未连接'), note(node.id === 'local' ? '本机节点固定保留；共享、Web 与发现分别控制。' : '访问权限由拥有该节点的电脑管理。切换不会搬动项目与会话。'));
    if (node.id !== state.current && connected(node)) body.append(button('切换到该节点', 'switch-modal', node.id));
    if (node.id !== 'local' && connected(node)) body.append(button('模拟断开', 'disconnect-modal', node.id));
  }
  function handle(event) {
    const control = event.target.closest('[data-collab-action]'); if (!control) return;
    const action = control.dataset.collabAction, id = control.dataset.collabId;
    const node = nodes.find(item => item.id === id);
    if (action === 'refresh') refresh();
    else if (action === 'manual' || action === 'scan') manual('', action === 'scan');
    else if (action === 'nearby' && discovered?.[Number(id)]) manual(discovered[Number(id)].address);
    else if (action === 'confirm' && selected) connect(selected, true);
    else if (action === 'connect-known' && node) connect(node, false, control);
    else if (action === 'switch' && node) { switchNode(id); restoreAction('node-settings', id); }
    else if (action === 'switch-modal' && node) { switchNode(id); dialog.close(); }
    else if (action === 'disconnect-modal' && node) { disconnect(node); dialog.close(); }
    else if (action === 'disconnect-current') { const id = current().id; disconnect(current()); restoreAction('node-settings', id); }
    else if (action === 'node-settings' && node) nodeSettings(node);
    else if (action === 'authorize') authorization();
    else if (action === 'regenerate') regenerate();
    else if (action === 'copy') copyAddress();
    else if (action === 'revoke') { revoked = true; render(); feedback('已清除演示授权记录，不影响真实设备。'); $('#collab-scenario').focus(); }
    else if (action === 'share') {
      open('share', '节点地址二维码 · 演示');
      body.append(note(current().address), note('二维码占位 · 不可扫码', 'collab-code-placeholder'), note('地址分享不包含密码、不代表授权，也不会开启共享。真实编码与加密由原生客户端完成。'));
    }
  }
  section.addEventListener('click', handle); body.addEventListener('click', handle);
  for (const key of ['sharing', 'web', 'discoverable']) $('#collab-' + key).addEventListener('change', event => {
    if (current().id !== 'local' || event.target.disabled) return;
    state[key] = event.target.checked; save(); render();
    feedback('仅保存此浏览器的演示偏好；没有改变真实服务或权限。');
  });
  $('#collab-scenario').addEventListener('change', event => {
    scenario = event.target.value; revoked = false; render(); feedback('已切换演示场景；现有权限偏好未被自动修改。');
  });
  dialog.addEventListener('close', () => {
    if (dialog.open) return;
    if (connecting) feedback('连接演示已取消，未保存新节点。');
    closeTransient(); selected = null; body.replaceChildren();
    if (settingsDialog.open) {
      if (returnFocus?.isConnected && !returnFocus.disabled) returnFocus.focus();
      else if (returnFocus?.dataset.collabId) restoreAction(returnFocus.dataset.collabAction, returnFocus.dataset.collabId);
      else section.querySelector('[data-collab-action="manual"]').focus();
    }
  });
  settingsDialog.addEventListener('close', () => {
    if (settingsDialog.open) return;
    if (discoveryTimer) $('#collab-discovery-status').textContent = '发现已取消，可重新刷新';
    closeTransient(); clearTimeout(discoveryTimer); discoveryTimer = null; $('#collab-refresh').disabled = false;
    if (dialog.open) dialog.close();
  });
  if (state.nodes.length) renderNavigation();
  return { render };
}
