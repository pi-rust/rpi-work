/* Desktop server / mobile client interaction prototype. No pairing/auth/server API is called. */
function connectionId(){const bytes=crypto.getRandomValues(new Uint8Array(16));bytes[6]=(bytes[6]&15)|64;bytes[8]=(bytes[8]&63)|128;const hex=[...bytes].map(b=>b.toString(16).padStart(2,'0')).join('');return `${hex.slice(0,8)}-${hex.slice(8,12)}-${hex.slice(12,16)}-${hex.slice(16,20)}-${hex.slice(20)}`;}
const connection={role:'pc',endpoint:'https://bigfish-pc.local:8443',nodeId:connectionId(),fingerprint:'4D:7A:16:8C:2F:90:65:B1',invite:null,request:null,devices:[],client:{phase:'idle',deviceId:null,lastSeq:0,notice:''},form:{mode:'scan',name:'我的手机',endpoint:'',code:'',link:'',error:''},timers:new Set(),epoch:0,tick:null,camera:null,cameraFrame:null,cameraEpoch:0};
const connectionLabels={idle:'尚未连接',requesting:'正在提交配对',awaiting:'等待电脑确认',syncing:'正在同步会话',online:'已连接',offline:'连接已断开',reconnecting:'正在重连',revoked:'设备授权已撤销',rejected:'电脑拒绝了配对',expired:'配对邀请已过期'};
function connectButton(action,label,extra=''){return `<button class="button" data-connect="${action}" ${extra}>${label}</button>`;}
function connectionTime(){const seconds=Math.max(0,Math.ceil(((connection.invite?.expires||0)-Date.now())/1000));return `${Math.floor(seconds/60)}:${String(seconds%60).padStart(2,'0')}`;}
function usableInvite(){return connection.invite&&['ready','pending'].includes(connection.invite.status)&&connection.invite.expires>Date.now();}
function normalizeNodeAddress(raw){
 let u;try{u=new URL(raw.trim());}catch{throw Error('请输入完整的电脑服务地址，例如 https://bigfish-pc.local:8443。');}
 if(!['https:','http:'].includes(u.protocol)||u.username||u.password||u.search||u.hash||!['','/'].includes(u.pathname))throw Error('服务地址只包含 HTTP(S) 协议、主机和端口，不包含账号、路径或查询参数。');
 const host=u.hostname.replace(/^\[|\]$/g,'').toLowerCase();
 if(['localhost','::1','0.0.0.0','::'].includes(host)||/^127\./.test(host))throw Error('手机上的 localhost / 127.0.0.1 指向手机本机，请填写电脑的局域网或远程地址。');
 if(u.protocol==='http:'&&!/^(10\.|192\.168\.|172\.(1[6-9]|2\d|3[01])\.)/.test(host)&&!host.endsWith('.local'))throw Error('远程连接请使用 HTTPS；HTTP 示例只接受局域网地址。');
 return u.origin;
}
function createPairInvite(){
 clearInterval(connection.tick);if(['requesting','awaiting'].includes(connection.client.phase)){connection.epoch++;connection.timers.forEach(clearTimeout);connection.timers.clear();}connection.request=null;
 if(['requesting','awaiting'].includes(connection.client.phase))connection.client.phase='expired';
 const bytes=crypto.getRandomValues(new Uint8Array(16)),nonce=[...bytes].map(b=>b.toString(16).padStart(2,'0')).join('');
 connection.invite={id:connectionId(),nonce,code:String(crypto.getRandomValues(new Uint32Array(1))[0]%1000000).padStart(6,'0'),expires:Date.now()+300000,status:'ready'};
 connection.tick=setInterval(()=>{
  $$('.pair-expiry').forEach(e=>e.textContent=connectionTime());
  if(!usableInvite()&&connection.invite&&['ready','pending'].includes(connection.invite.status))expirePairInvite();
 },1000);
 return connection.invite;
}
function pairInviteUrl(){
 const invite=connection.invite;if(!invite)return '';
 const data={v:1,node:connection.nodeId,nonce:invite.nonce,exp:invite.expires,fp:connection.fingerprint};
 return connection.endpoint+'/connect#pair='+btoa(JSON.stringify(data)).replace(/\+/g,'-').replace(/\//g,'_').replace(/=+$/,'');
}
function decodePairLink(value){
 let url;try{url=new URL(value.trim());}catch{throw Error('未识别到有效的配对链接。');}
 const endpoint=normalizeNodeAddress(url.origin);if(url.pathname!=='/connect'||url.search)throw Error('请扫描 RPI Work 电脑端生成的连接二维码。');
 const encoded=new URLSearchParams(url.hash.slice(1)).get('pair');if(!encoded||encoded.length>2048)throw Error('配对链接缺少邀请信息。');
 let data;try{const base=encoded.replace(/-/g,'+').replace(/_/g,'/');data=JSON.parse(atob(base+'='.repeat((4-base.length%4)%4)));}catch{throw Error('配对邀请内容不完整，请重新扫码。');}
 if(data?.v!==1||typeof data.node!=='string'||!/^[a-f0-9]{32}$/.test(data.nonce||'')||!Number.isFinite(data.exp)||typeof data.fp!=='string')throw Error('配对邀请格式不正确。');
 if(data.exp<=Date.now())throw Error('配对邀请已过期，请在电脑上刷新二维码。');
 return {endpoint,data};
}
function pairQrSvg(){const code=qrcode(0,'M');code.addData(pairInviteUrl(),'Byte');code.make();return code.createSvgTag({cellSize:4,margin:16,scalable:true,alt:'RPI Work 电脑连接二维码（本地演示）'});}
function clientStateClass(){return connection.client.phase==='online'?'completed':['requesting','syncing','reconnecting'].includes(connection.client.phase)?'running':['awaiting'].includes(connection.client.phase)?'waiting':'stopped';}
function authorizedClientSession(){const device=connection.devices.find(d=>d.id===connection.client.deviceId&&!d.revoked);return device?state.sessions.find(s=>s.project===device.project):null;}
function connectionPreviewNotice(){return `<div class="connection-demo-note">${icon('info')}交互原型：二维码与本地图像识别可用；地址、授权和会话同步是模拟数据，未开放电脑网络服务。</div>`;}
function renderDeviceWorkspace(){
 stopPairCamera();const isPc=connection.role==='pc';
 $('#page-view').innerHTML=`<div class="overline">YOUR COMPUTER. YOUR AGENT. EVERYWHERE.</div><div class="page-heading"><div><h1>${isPc?'电脑运行，手机接续':'连接你的电脑'}</h1><p>${isPc?'PC 提供 Agent 服务，手机作为客户端查看、发送和确认。':'扫描电脑二维码，或在高级模式手工连接。'}</p></div>${isPc?`<button class="button primary" id="pair-device">${icon('phone')}扫码连接手机</button>`:''}</div><div class="connection-role-switch" aria-label="连接角色演示"><button data-connect="role-pc" class="${isPc?'active':''}">${icon('monitor')}电脑 · 服务端</button><button data-connect="role-phone" class="${isPc?'':'active'}">${icon('phone')}手机 · 客户端</button><span>在本页切换两端体验</span></div>${connectionPreviewNotice()}${isPc?renderPcServer():renderMobileClient()}`;
}
function renderPcServer(){
 const active=connection.devices.filter(d=>!d.revoked),request=connection.request?.status==='awaiting';
 return `<div class="connection-layout"><section class="server-overview"><div class="server-identity"><span class="connection-node-icon">${icon('monitor')}</span><div><h2>这台电脑</h2><p>BIGFISH-DESKTOP · Windows · rpi-web</p></div><span class="status-badge completed"><i></i>服务就绪 · 示例</span></div><div class="connection-topology"><span>${icon('monitor')}PC 服务端<small>模型 / 工具 / 项目文件</small></span><i>HTTP + SSE</i><span>${icon('phone')}手机客户端<small>对话 / 变更 / 回答确认</small></span></div><div class="node-address"><small>手机可访问的服务地址 · 示例</small><code>${escapeHTML(connection.endpoint)}</code></div><div class="server-details"><div><span>服务端口</span><strong>${new URL(connection.endpoint).port||'443'}</strong></div><div><span>共享项目</span><strong>按设备授权</strong></div><div><span>模型凭据</span><strong>保留在电脑</strong></div></div><div class="connection-server-actions">${connectButton('server-address','设置服务地址')}${connectButton('pair','显示连接二维码')}${connectButton('phone-advanced','手工连接 · 高级')}</div><p class="connection-help">同一局域网使用电脑可达地址；远程连接使用已配置的 HTTPS 或专用网络入口。正式版由 PC 承载会话，手机退出后任务仍可继续。</p></section><aside class="connection-steps"><h3>连接只需三步</h3><div><b>1</b><span>电脑显示二维码<small>短时邀请，到期可刷新</small></span></div><div><b>2</b><span>手机扫码并核对电脑<small>填写设备名，提交配对请求</small></span></div><div><b>3</b><span>电脑确认设备和范围<small>授权后同步会话，后续自动重连</small></span></div></aside></div>${request?`<div class="connection-pending-banner">${icon('phone')}<span><strong>${escapeHTML(connection.request.name)}</strong> 正在等待电脑确认</span>${connectButton('review','核对并授权')}</div>`:''}<div class="connection-clients-heading"><h2>已授权的手机 <small>${active.length}</small></h2><span>配对与在线状态分别管理</span></div>${active.length?active.map(renderClientDevice).join(''):'<div class="connection-empty">还没有授权设备。用手机扫描电脑上的二维码即可开始。</div>'}${connection.devices.some(d=>d.revoked)?`<details class="revoked-clients"><summary>已撤销的设备 · ${connection.devices.filter(d=>d.revoked).length}</summary>${connection.devices.filter(d=>d.revoked).map(d=>`<p>${escapeHTML(d.name)} · 授权已撤销，需要重新配对</p>`).join('')}</details>`:''}`;
}
function renderClientDevice(device){return `<article class="device-card connection-client-card"><div class="device-card-top"><span class="device-icon">${icon('phone')}</span><div><h3>${escapeHTML(device.name)}</h3><p>${device.project} · ${device.scope==='observe'?'仅查看':'查看 / 发送 / 回答确认'}</p></div><span class="status-badge ${device.online?'completed':'stopped'}"><i></i>${device.online?'在线':'离线'} · 已配对</span></div><div class="connection-device-meta"><span>设备 ${device.id.slice(0,8)}</span><span>通过${device.method==='qr'?'扫码':'高级模式'}连接 · 模拟</span></div><button class="button" data-connect="revoke" data-id="${device.id}" ${connection.client.deviceId===device.id?'id="unpair"':''}>撤销设备授权</button></article>`;}
function renderMobileClient(){
 const c=connection.client,device=connection.devices.find(d=>d.id===c.deviceId),phase=c.phase,session=authorizedClientSession();
 if(['online','offline','reconnecting','syncing'].includes(phase))return `<section class="mobile-connection-card"><div class="mobile-server-title">${icon('monitor')}<div><h2>BIGFISH-DESKTOP</h2><code>${escapeHTML(connection.endpoint)}</code></div><span class="status-badge ${clientStateClass()}"><i></i>${connectionLabels[phase]}</span></div><div class="client-sync-progress"><span>${phase==='online'?'已同步项目与会话':phase==='syncing'?'正在获取快照并接续事件流':phase==='reconnecting'?'正在恢复授权连接与事件游标':'已保留已有消息，电脑任务继续运行'}</span><small>游标 ${c.lastSeq} · 演示</small></div><div class="client-project"><span>已授权项目</span><strong>${escapeHTML(device?.project||'rpi-work')}</strong><small>${device?.scope==='observe'?'只读客户端':'协作客户端'}</small></div>${phase==='online'?`<button class="client-session-preview" data-connect="open-session">${icon('message')}<span>${escapeHTML(session?.title||'暂无可用会话')}<small>${device?.scope==='observe'?'只读查看已授权项目':'在手机接续同一会话'} · 示例</small></span>${icon('chevron-right')}</button><div class="connection-server-actions">${connectButton('disconnect','模拟手机断线')}${connectButton('role-pc','查看电脑授权')}</div>`:`<div class="connection-server-actions">${connectButton('reconnect',phase==='reconnecting'?'正在重连…':'重连电脑',phase==='syncing'||phase==='reconnecting'?'disabled':'')}${connectButton('role-pc','查看电脑')}</div>`}<p class="connection-help">手机只连接电脑提供的 API 和事件流。模型凭据、项目文件与工具执行保留在 PC。</p></section>`;
 if(['requesting','awaiting'].includes(phase))return `<section class="mobile-connection-card"><div class="pair-mobile-state">${icon('phone')}<h2>${connectionLabels[phase]}</h2><p>请在电脑上核对设备名称与两端校验码。</p>${connection.request?`<div class="pair-verify-code">${connection.request.checksum}</div><small>${escapeHTML(connection.request.name)} · ${escapeHTML(connection.endpoint)}</small>`:''}</div><div class="connection-server-actions">${connectButton('review','在本页体验电脑确认')}${connectButton('cancel-request','取消配对请求')}</div></section>`;
 const terminal=['revoked','rejected','expired'].includes(phase);
 return `<section class="mobile-connect-form">${terminal?`<div class="connection-feedback">${icon('info')}<span>${connectionLabels[phase]}。${phase==='revoked'?'重新连接需要电脑签发新的邀请。':'请重新获取电脑上的邀请。'}</span></div>`:''}<div class="mobile-connect-tabs"><button data-connect="scan-mode" class="${connection.form.mode==='scan'?'active':''}">${icon('image')}扫码连接</button><button data-connect="advanced-mode" class="${connection.form.mode==='advanced'?'active':''}">${icon('settings')}高级 · 手工输入</button></div><div class="field"><label for="client-device-name">本机设备名称</label><input id="client-device-name" value="${escapeHTML(connection.form.name)}" maxlength="40" placeholder="例如：我的手机"></div>${connection.form.mode==='scan'?`<div class="pair-scanner"><video id="pair-camera" playsinline muted hidden></video><div id="scanner-placeholder">${icon('image')}<strong>扫描电脑屏幕上的二维码</strong><span>相机画面只在本机识别</span></div><p id="scanner-status" role="status"></p></div><div class="connection-server-actions">${connectButton('camera','打开相机')}${connectButton('camera-stop','关闭相机')}${connectButton('pick-qr','从图片识别')}</div><details class="pair-link-entry"><summary>粘贴配对链接</summary><textarea id="pair-link" rows="3" class="modal-textarea" placeholder="粘贴电脑复制的配对链接…">${escapeHTML(connection.form.link)}</textarea>${connectButton('link-connect','读取并连接')}</details><button class="pair-demo-shortcut" data-connect="fill-scan">使用当前电脑邀请演示扫码 ${icon('chevron-right')}</button>`:`<div class="field"><label for="client-server-url">PC 服务地址</label><input id="client-server-url" value="${escapeHTML(connection.form.endpoint)}" inputmode="url" autocomplete="url" placeholder="https://bigfish-pc.local:8443"></div><div class="field"><label for="client-pair-code">电脑上显示的一次性配对码</label><input id="client-pair-code" value="${escapeHTML(connection.form.code)}" inputmode="numeric" autocomplete="one-time-code" maxlength="7" placeholder="6 位配对码"></div><div class="connection-server-actions">${connectButton('manual-connect','请求连接电脑')}${connectButton('fill-manual','填入当前电脑示例')}</div><p class="connection-help">手工连接与扫码使用相同的设备授权流程；电脑确认前不会获得会话访问权限。</p>`}<p class="connection-form-error" role="alert">${escapeHTML(connection.form.error)}</p></section>`;
}
function openPairing(refresh=false){
 stopPairCamera();if(refresh||!usableInvite())createPairInvite();connection.role='pc';
 if(connection.request?.status==='awaiting'){openPairApproval();return;}
 const i=connection.invite;
 showModal('手机扫码连接这台电脑',`<div class="pair-dialog-intro"><span class="status-badge completed"><i></i>PC 服务端 · 演示邀请</span><p>手机扫描二维码，核对电脑信息后提交配对请求。</p></div><div class="pair-dialog-layout"><div class="pair-qr-wrap"><div class="pairing-qr" id="pairing-qr">${pairQrSvg()}</div><p>邀请有效期 <strong class="pair-expiry">${connectionTime()}</strong></p></div><div class="pair-invite-info"><small>服务地址</small><code>${escapeHTML(connection.endpoint)}</code><small>电脑身份指纹 · 示例</small><code>${connection.fingerprint}</code><div class="pair-manual-code"><span>高级模式配对码</span><strong id="pair-code">${i.code.slice(0,3)} ${i.code.slice(3)}</strong></div><p>同一网络或可达的远程入口。配对通过后，手机接续电脑上的会话。</p>${connectButton('copy-link','复制配对链接')}${connectButton('refresh','刷新二维码')}</div></div><details class="pair-invite-advanced"><summary>高级：查看配对链接</summary><textarea id="pair-url" class="modal-textarea" rows="3" readonly>${escapeHTML(pairInviteUrl())}</textarea><small>演示链接，不是有效服务凭据。</small></details><details class="pair-invite-demo"><summary>体验配对状态</summary><button class="button" id="simulate-pair">模拟手机扫码</button>${connectButton('expire','模拟邀请过期')}</details>`,connectButton('phone-advanced','手工输入 · 高级'));
 $('#modal').classList.add('connection-dialog');
}
function openMobileConnect(mode='scan'){
 if($('#modal').open)$('#modal').close();stopPairCamera();connection.role='phone';connection.form.mode=mode;connection.form.error='';navigate('devices');
}
function openPairApproval(){
 const r=connection.request;if(!r||r.status!=='awaiting'||!usableInvite())return;
 stopPairCamera();connection.role='pc';
 showModal('电脑确认新设备',`<p>请核对手机上显示的设备名与校验码，再选择授权范围。</p><div class="pair-approval-device">${icon('phone')}<div><strong>${escapeHTML(r.name)}</strong><small>${r.method==='qr'?'扫码配对':'高级手工配对'} · 等待电脑授权</small></div></div><div class="pair-verify-code">${r.checksum}</div><div class="field"><label for="pair-project-scope">可访问的项目</label><select id="pair-project-scope"><option>rpi-work</option><option>pi-rust</option><option>rpi-package</option></select></div><div class="field"><label for="pair-access-scope">客户端权限</label><select id="pair-access-scope"><option value="collaborate">协作 · 查看、发送、回答确认</option><option value="observe">只读 · 查看对话与变更</option></select></div><label class="pair-check"><input id="pair-verified" type="checkbox">我已核对两端显示的设备与校验码</label><p class="modal-help">邀请剩余 <span class="pair-expiry">${connectionTime()}</span> · 授权仅针对上面选定的项目。</p>`,`<button class="button" data-connect="reject">拒绝设备</button><button class="button primary" data-connect="approve" id="approve-pair" disabled>授权此手机</button>`);
 $('#modal').classList.add('connection-dialog');
}
function connectSchedule(callback,delay){const epoch=connection.epoch,timer=setTimeout(()=>{connection.timers.delete(timer);if(connection.epoch===epoch)callback();},delay);connection.timers.add(timer);}
function connectionError(message){connection.form.error=message;const e=$('.connection-form-error');if(e)e.textContent=message;else toast(message);}
function requestClientPair(method,link){
 try{
  const name=connection.form.name.trim();if(!name||name.length>40)throw Error('请填写 1–40 字的设备名称。');
  let endpoint,data;
  if(method==='qr'){({endpoint,data}=decodePairLink(link));if(endpoint!==connection.endpoint||data.node!==connection.nodeId||data.nonce!==connection.invite?.nonce||data.fp!==connection.fingerprint)throw Error('这不是当前电脑的有效邀请。原型仅支持本页电脑邀请。');}
  else{endpoint=normalizeNodeAddress(connection.form.endpoint);if(endpoint!==connection.endpoint)throw Error('未找到此示例电脑；请核对服务地址，或填入当前电脑示例。');const code=connection.form.code.replace(/\s/g,'');if(!/^\d{6}$/.test(code)||code!==connection.invite?.code)throw Error('配对码不正确，请核对电脑上显示的 6 位数字。');}
  if(!usableInvite()||connection.invite.status!=='ready')throw Error('配对邀请已失效或已被使用，请在电脑上刷新。');
  stopPairCamera();connection.epoch++;connection.timers.forEach(clearTimeout);connection.timers.clear();connection.form.error='';connection.invite.status='pending';connection.request={id:connectionId(),name,method,status:'requesting',checksum:String(crypto.getRandomValues(new Uint32Array(1))[0]%1000000).padStart(6,'0'),endpoint};connection.client={phase:'requesting',deviceId:null,lastSeq:0,notice:''};
  if(state.page==='devices')renderDevices();
  connectSchedule(()=>{if(!usableInvite())return;connection.request.status='awaiting';connection.client.phase='awaiting';if(state.page==='devices')renderDevices();},220);
  return true;
 }catch(e){connectionError(e.message);return false;}
}
function simulatePairScan(){
 if(!usableInvite())createPairInvite();if(!requestClientPair('qr',pairInviteUrl()))return;
 connectSchedule(()=>openPairApproval(),260);
}
function settleClientPair(approve){
 const r=connection.request;if(!r||r.status!=='awaiting'||!usableInvite())return;
 if(approve&&!$('#pair-verified')?.checked)return;
 r.status=approve?'approved':'rejected';connection.invite.status='used';clearInterval(connection.tick);
 if(approve){const device={id:connectionId(),name:r.name,method:r.method,scope:$('#pair-access-scope').value,project:$('#pair-project-scope').value,online:false,revoked:false};connection.devices.push(device);connection.client={phase:'syncing',deviceId:device.id,lastSeq:0,notice:''};state.pairing=true;connectSchedule(()=>{device.online=true;connection.client.phase='online';connection.client.lastSeq=36;if(state.page==='devices')renderDevices();},550);}
 else connection.client.phase='rejected';
 $('#modal').close();if(state.page==='devices')renderDevices();
}
function expirePairInvite(){
 if(!connection.invite||connection.invite.status==='used')return;
 connection.invite.status='expired';if(['requesting','awaiting'].includes(connection.client.phase)){connection.epoch++;connection.timers.forEach(clearTimeout);connection.timers.clear();}clearInterval(connection.tick);
 if(connection.request&&!['approved','rejected'].includes(connection.request.status))connection.request.status='expired';
 if(['awaiting','requesting'].includes(connection.client.phase))connection.client.phase='expired';
 if($('#modal').open&&$('#modal').classList.contains('connection-dialog'))showModal('配对邀请已过期',`<div class="pair-expired">${icon('clock')}<h3>请刷新二维码再试一次</h3><p>旧二维码与旧配对码不再有效，尚未授权的手机不会获得访问权限。</p></div>`,connectButton('refresh','生成新邀请'));
 if(state.page==='devices')renderDevices();
}
function revokeClient(id){
 const device=connection.devices.find(d=>d.id===id);if(!device||device.revoked)return;device.revoked=true;device.online=false;
 if(connection.client.deviceId===id){connection.epoch++;connection.timers.forEach(clearTimeout);connection.timers.clear();connection.client.phase='revoked';connection.client.deviceId=null;stopPairCamera();}
 state.pairing=connection.devices.some(d=>!d.revoked);if(state.page==='devices')renderDevices();toast('已模拟撤销此设备授权，重新连接需要新邀请');
}
function disconnectClient(){const device=connection.devices.find(d=>d.id===connection.client.deviceId);if(!device||device.revoked)return;connection.epoch++;connection.timers.forEach(clearTimeout);connection.timers.clear();device.online=false;connection.client.phase='offline';renderDevices();}
function reconnectClient(){
 const device=connection.devices.find(d=>d.id===connection.client.deviceId);if(!device||device.revoked){connection.client.phase='revoked';renderDevices();return;}
 connection.client.phase='reconnecting';renderDevices();connectSchedule(()=>{if(device.revoked)return;connection.client.phase='syncing';renderDevices();connectSchedule(()=>{if(device.revoked)return;device.online=true;connection.client.phase='online';connection.client.lastSeq=42;renderDevices();},300);},450);
}
function stopPairCamera(){const active=!!connection.camera;connection.cameraEpoch++;if(connection.camera)connection.camera.getTracks().forEach(t=>t.stop());connection.camera=null;if(connection.cameraFrame)cancelAnimationFrame(connection.cameraFrame);connection.cameraFrame=null;const video=$('#pair-camera');if(video){video.pause();video.srcObject=null;video.hidden=true;}const placeholder=$('#scanner-placeholder');if(placeholder)placeholder.hidden=false;if(active&&$('#scanner-status'))$('#scanner-status').textContent='相机已关闭，可重新打开或使用其他方式连接。';}
async function startPairCamera(){
 stopPairCamera();const epoch=connection.cameraEpoch,status=$('#scanner-status');
 if(!window.isSecureContext||!navigator.mediaDevices?.getUserMedia){status.textContent='此环境不能打开相机。可从二维码图片识别，或使用高级手工模式。';return;}
 try{
  status.textContent='正在打开相机…';
  const media=await navigator.mediaDevices.getUserMedia({video:{facingMode:{ideal:'environment'},width:{ideal:960}},audio:false});
  if(epoch!==connection.cameraEpoch||connection.role!=='phone'||!$('#pair-camera')){media.getTracks().forEach(t=>t.stop());return;}
  connection.camera=media;const video=$('#pair-camera');video.srcObject=media;video.hidden=false;$('#scanner-placeholder').hidden=true;await video.play();status.textContent='正在本机识别，请对准电脑二维码…';const canvas=document.createElement('canvas'),ctx=canvas.getContext('2d',{willReadFrequently:true});let last=0;
  const tick=time=>{if(epoch!==connection.cameraEpoch)return;if(time-last>240&&video.readyState>=2){last=time;const scale=Math.min(1,960/video.videoWidth);canvas.width=Math.round(video.videoWidth*scale);canvas.height=Math.round(video.videoHeight*scale);ctx.drawImage(video,0,0,canvas.width,canvas.height);const image=ctx.getImageData(0,0,canvas.width,canvas.height),qr=jsQR(image.data,canvas.width,canvas.height,{inversionAttempts:'dontInvert'});if(qr){stopPairCamera();connection.form.link=qr.data;requestClientPair('qr',qr.data);return;}}connection.cameraFrame=requestAnimationFrame(tick);};
  connection.cameraFrame=requestAnimationFrame(tick);
 }catch(e){if(epoch!==connection.cameraEpoch)return;stopPairCamera();if($('#scanner-status'))$('#scanner-status').textContent=e.name==='NotAllowedError'?'未获得相机权限，可从图片识别或使用高级模式。':'无法打开相机，请检查设备或使用高级模式。';}
}
async function scanPairImage(file){
 if(!file)return;stopPairCamera();const epoch=connection.cameraEpoch;
 try{if(!['image/png','image/jpeg','image/webp'].includes(file.type)||file.size>10*1024*1024)throw Error('请选择不超过 10 MB 的 PNG、JPG 或 WebP 二维码图片。');
  const image=await createImageBitmap(file);if(epoch!==connection.cameraEpoch||connection.role!=='phone'){image.close();return;}const scale=Math.min(1,1600/Math.max(image.width,image.height)),canvas=document.createElement('canvas');canvas.width=Math.max(1,Math.round(image.width*scale));canvas.height=Math.max(1,Math.round(image.height*scale));const ctx=canvas.getContext('2d',{willReadFrequently:true});ctx.drawImage(image,0,0,canvas.width,canvas.height);image.close();const data=ctx.getImageData(0,0,canvas.width,canvas.height),qr=jsQR(data.data,canvas.width,canvas.height);if(!qr)throw Error('未识别到二维码，请使用清晰完整的电脑配对码图片。');connection.form.link=qr.data;requestClientPair('qr',qr.data);
 }catch(e){if(epoch===connection.cameraEpoch)connectionError(e.message);}finally{$('#pair-image-input').value='';}
}
document.addEventListener('click',async event=>{
 const b=event.target.closest('button[data-connect]');if(!b||b.disabled)return;const action=b.dataset.connect;
 if(action==='pair'||action==='refresh')openPairing(action==='refresh');
 if(action==='role-pc'){stopPairCamera();connection.role='pc';renderDevices();}
 if(action==='role-phone'||action==='phone-advanced')openMobileConnect(action==='phone-advanced'?'advanced':'scan');
 if(action==='scan-mode'||action==='advanced-mode'){stopPairCamera();connection.form.mode=action==='scan-mode'?'scan':'advanced';connection.form.error='';renderDevices();}
 if(action==='review')openPairApproval();if(action==='approve')settleClientPair(true);if(action==='reject')settleClientPair(false);
 if(action==='expire')expirePairInvite();if(action==='revoke')revokeClient(b.dataset.id);
 if(action==='disconnect')disconnectClient();if(action==='reconnect')reconnectClient();
 if(action==='fill-manual'){if(!usableInvite())createPairInvite();connection.form.endpoint=connection.endpoint;connection.form.code=connection.invite.code;renderDevices();}
 if(action==='fill-scan'){if(!usableInvite())createPairInvite();connection.form.link=pairInviteUrl();requestClientPair('qr',connection.form.link);}
 if(action==='manual-connect')requestClientPair('manual');if(action==='link-connect')requestClientPair('qr',connection.form.link);
 if(action==='cancel-request'){connection.epoch++;connection.timers.forEach(clearTimeout);connection.timers.clear();if(connection.request)connection.request.status='cancelled';if(connection.invite)connection.invite.status='used';clearInterval(connection.tick);connection.client.phase='idle';renderDevices();}
 if(action==='open-session'){
  const device=connection.devices.find(d=>d.id===connection.client.deviceId&&!d.revoked),session=authorizedClientSession();
  if(!device||!session||connection.client.phase!=='online')return;
  if(device.scope==='observe'){
   const messages=(state.messages[session.id]||[]).map(m=>m.role==='stream'?m.blocks.filter(b=>b.kind==='text').map(b=>b.text).join('\n'):m.text||'');
   showModal(escapeHTML(session.title),`<p>只读客户端 · ${escapeHTML(device.project)} · 由电脑提供会话数据（示例）</p><div class="client-readonly-preview">${messages.length?messages.map(text=>'<p>'+escapeHTML(text.slice(0,2000))+'</p>').join(''):'<p>此会话尚未加载对话记录。</p>'}</div><p class="modal-help">此授权仅可查看对话与变更，不能发送指令或回应确认。</p>`);return;
  }
  selectSession(session.id);toast('演示接续已授权项目的会话；实际同步仍需节点 API');
 }
 if(action==='copy-link'){if(!usableInvite())return;try{await navigator.clipboard.writeText(pairInviteUrl());toast('已复制演示配对链接');}catch{const input=$('#pair-url');input.closest('details').open=true;input.focus();input.select();toast('请复制已选中的配对链接');}}
 if(action==='server-address'){showModal('电脑服务地址',`<p>填写手机可以访问的 PC 地址。这里只更新示例，不改变实际监听端口或防火墙。</p><div class="field"><label for="server-address-input">对外服务地址</label><input id="server-address-input" type="url" value="${escapeHTML(connection.endpoint)}"></div><p class="connection-form-error" role="alert"></p>`,connectButton('save-address','保存示例地址'));}
 if(action==='save-address'){try{connection.endpoint=normalizeNodeAddress($('#server-address-input').value);createPairInvite();$('#modal').close();renderDevices();toast('示例地址已更新，旧邀请已失效');}catch(e){connectionError(e.message);}}
 if(action==='camera')startPairCamera();if(action==='camera-stop')stopPairCamera();if(action==='pick-qr')$('#pair-image-input').click();
});
document.addEventListener('input',event=>{const fields={'client-device-name':'name','client-server-url':'endpoint','client-pair-code':'code','pair-link':'link'},key=fields[event.target.id];if(key)connection.form[key]=event.target.value;});
document.addEventListener('change',event=>{if(event.target.id==='pair-verified')$('#approve-pair').disabled=!event.target.checked;if(event.target.id==='pair-image-input')scanPairImage(event.target.files[0]);});
document.addEventListener('DOMContentLoaded',()=>{document.body.insertAdjacentHTML('beforeend','<input id="pair-image-input" type="file" accept="image/png,image/jpeg,image/webp" hidden>');$('#modal').addEventListener('close',()=>$('#modal').classList.remove('connection-dialog'));});
document.addEventListener('visibilitychange',()=>{if(document.hidden)stopPairCamera();});
window.addEventListener('beforeunload',()=>{stopPairCamera();clearInterval(connection.tick);connection.timers.forEach(clearTimeout);});
