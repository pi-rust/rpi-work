/* Product interaction models only: no provider, upload, shell or device requests. */
Object.assign(paths, {
  queue:'<path d="M4 6h12M4 12h12M4 18h7m6-3 4 3-4 3"/>',
  paperclip:'<path d="m9 15 7-7a3 3 0 0 0-4-4L4 12a5 5 0 0 0 7 7l8-8a2 2 0 0 0-3-3l-8 8"/>',
  image:'<rect x="3" y="3" width="18" height="18" rx="3"/><circle cx="8" cy="8" r="2"/><path d="m21 15-5-5-8 11m-5-4 4-4 4 4"/>',
  compress:'<path d="M4 3v5h5m11-5v5h-5M4 21v-5h5m11 5v-5h-5M8 8l-5-5m13 5 5-5M8 16l-5 5m13-5 5 5"/>',
  chart:'<path d="M4 3v18h17M8 16v-4m5 4V7m5 9v-6"/>',
  refresh:'<path d="M20 7v5h-5M4 17v-5h5"/><path d="M5 8a8 8 0 0 1 13-3l2 2M4 17l2 2a8 8 0 0 0 13-3"/>',
  up:'<path d="m6 14 6-6 6 6"/>',down:'<path d="m6 10 6 6 6-6"/>',
  pause:'<path d="M8 5v14M16 5v14"/>', play:'<path d="m7 4 14 8-14 8z"/>',
  globe:'<circle cx="12" cy="12" r="9"/><ellipse cx="12" cy="12" rx="4" ry="9"/><path d="M3 12h18"/>',
});
const runtime = {sessions:{},queueExpanded:false,toolFilter:'all',statsScope:'turn',thinking:'high',autoCompact:true,threshold:80,providerStatus:{DeepSeek:'ready',OpenAI:'ready',Custom:'unset'},objectUrls:[],zoom:1};
const sampleImage={id:'reference-image',name:'theme-reference.svg',kind:'image',size:18400,src:'assets/theme-reference.svg',demo:true};
const sampleDocument={id:'reference-doc',name:'需求说明.md',kind:'document',size:2400,demo:true,text:'# 深色模式需求\n\n- 默认跟随系统主题\n- 允许手动切换\n- 桌面和手机共用语义化颜色\n- 保留主题偏好\n\n此文件为原型内置示例。'};
const callSamples=[
 {id:'read',name:'read',title:'读取主题文件',source:'内置工具',kind:'read',status:'done',duration:'0.4s',permission:'项目内读取',args:{path:'assets/theme.css',offset:1,limit:160},output:'已读取 86 行。发现 12 个主题变量，可沿用现有 token。',lines:'86 行',attempts:1},
 {id:'edit',name:'edit',title:'更新主题变量',source:'内置工具',kind:'write',status:'done',duration:'2.1s',permission:'工作区写入',args:{path:'assets/theme.css',operation:'replace',expectedMatches:1},output:'替换成功。3 files changed, 67 insertions(+), 10 deletions(-)。',lines:'3 个文件',attempts:1},
 {id:'shell',name:'powershell',title:'运行组件测试',source:'内置工具',kind:'exec',status:'waiting',duration:'—',permission:'需单次授权',args:{command:'cargo test -p work-ui --lib',cwd:'D:/Projects/rpi-work',timeoutMs:120000},output:'等待用户确认；尚未执行命令。',lines:'项目目录',attempts:1},
 {id:'grep',name:'grep',title:'查找颜色常量',source:'内置工具',kind:'read',status:'done',duration:'0.2s',permission:'项目内读取',args:{pattern:'#[0-9a-fA-F]{6}',path:'src',limit:100},output:'src/components/theme.rs:18  #ffffff\nsrc/state/settings.rs:34  #282b27\n共 2 处匹配。',lines:'2 处匹配',attempts:1},
 {id:'web',name:'webfetch',title:'获取主题设计参考',source:'rpi-webfetch',kind:'web',status:'failed',duration:'10.0s',permission:'外部网络',args:{url:'https://example.com/theme-guide',maxChars:8000},output:'NETWORK_TIMEOUT：在 10 秒内未收到响应。可重试，或使用本地参考文件继续。',lines:'网络超时',attempts:1},
 {id:'mcp',name:'docs.search',title:'检索组件文档',source:'MCP · docs',kind:'mcp',status:'done',duration:'1.3s',permission:'已授权连接',args:{query:'theme preferences',limit:3},output:'返回 3 条文档摘要。引用将与消息一起保留。\n1. Theme tokens\n2. System appearance\n3. Preferences storage',lines:'3 条结果',attempts:1},
 {id:'child',name:'delegate_task',title:'检查手机端布局',source:'rpi-subagents',kind:'agent',status:'running',duration:'6.2s',permission:'继承受限工具',args:{task:'检查 390px 窄屏布局，不修改文件',tools:['read','grep']},output:'子 Agent 正在检查布局。演示子任务，可取消。',lines:'只读子任务',attempts:1},
 {id:'browser',name:'browser',title:'真机页面验证',source:'待接入浏览器工具',kind:'web',status:'skipped',duration:'—',permission:'能力不可用',args:{action:'screenshot',viewport:{width:390,height:844}},output:'当前节点未提供交互式浏览器能力。webfetch 不能替代截图与点击操作。',lines:'未安装',attempts:0},
];
function runtimeSession(){
  if(!runtime.sessions[state.session])runtime.sessions[state.session]={drafts:[],queue:state.session==='dark'?[{id:'q1',text:'完成后检查手机端设置抽屉的对比度。',kind:'followUp'},{id:'q2',text:'先保留现有快捷键，不要调整键位。',kind:'steering'}]:[],paused:false,compact:null,calls:state.session==='dark'?structuredClone(callSamples):[]};
  return runtime.sessions[state.session];
}
function rtButton(action,label,ic='',extra=''){return `<button class="button" data-harness="${action}" ${extra}>${ic?icon(ic):''}${label}</button>`;}
function formatTokens(n){return n==null?'—':n>=1000?`${(n/1000).toFixed(1)}k`:String(n);}
function formatBytes(n){return n>=1048576?`${(n/1048576).toFixed(1)} MB`:`${Math.max(1,Math.round(n/1024))} KB`;}
function runtimeModal(title,body,footer='',wide=false){showModal(title,body,footer);$('#modal').classList.toggle('wide-modal',wide);}
function currentUsage(){if(state.session!=='dark')return null;return runtime.statsScope==='session'?{input:128000,output:18300,cache:96000,write:0,reasoning:5200,cost:0.0726,requests:9}:{input:32000,output:4800,cache:24000,write:0,reasoning:1200,cost:0.0184,requests:3};}
function renderHarnessUI(){
  if(!$('#draft-attachments')){
    $('.composer').insertAdjacentHTML('afterbegin','<div id="draft-attachments" class="draft-attachments"></div>');
    $('.composer-options').insertAdjacentHTML('beforeend',`<button class="mode-button" data-harness="thinking" aria-label="思考强度"><span id="thinking-label">High</span></button>`);
    $('#send-button').insertAdjacentHTML('beforebegin',`<button class="queue-send" data-harness="enqueue" aria-label="将当前草稿加入队列" title="加入队列（Alt + Enter）">${icon('queue')}</button>`);
    $('.composer-bottom').classList.add('with-queue');
    $('.composer-area').insertAdjacentHTML('beforeend','<footer id="runtime-footer" class="runtime-footer" aria-label="Agent 运行统计"></footer>');
  }
  const session=runtimeSession();
  syncTimelineCalls();
  $('.tab .count').textContent=availableChanges().length;
  $('#thinking-label').textContent=runtime.thinking==='off'?'Off':runtime.thinking==='high'?'High':'Max';
  $('#draft-attachments').innerHTML=session.drafts.map(a=>`<div class="draft-chip ${a.kind==='image'?'is-image':''}"><button data-harness="preview-draft" data-id="${a.id}" aria-label="预览 ${escapeHTML(a.name)}">${a.kind==='image'?`<img src="${escapeHTML(a.src)}" alt="">`:icon('file')}<span>${escapeHTML(a.name)}<small>${formatBytes(a.size)} · ${a.demo?'示例':'仅本地'}</small></span></button><button class="attachment-remove" data-harness="remove-attachment" data-id="${a.id}" aria-label="移除 ${escapeHTML(a.name)}">${icon('x')}</button></div>`).join('');
  renderQueueShelf();renderRuntimeFooter();renderInspectorRuntime();renderCapabilitiesUI();renderStreamChrome();renderSessionResources();
  $$('.tab').forEach(t=>{t.classList.toggle('active',t.dataset.tab===state.tab);t.setAttribute('aria-selected',String(t.dataset.tab===state.tab));});
}
function renderRuntimeFooter(){
  const usage=currentUsage(),ctx=state.session==='dark'?(runtimeSession().compact?12400:48600):0;
  $('#runtime-footer').innerHTML=`<div class="footer-metrics"><button data-harness="stats" title="输入总量，含缓存读写">${icon('chart')}<span>↑ ${formatTokens(usage?.input)}</span><span>↓ ${formatTokens(usage?.output)}</span></button><button data-harness="stats" class="cache-metric" title="缓存读取 / 输入总量">缓存 <b>${usage?((usage.cache/usage.input)*100).toFixed(0)+'%':'—'}</b></button><button data-harness="context" class="context-meter" title="当前上下文估算，不是累计用量"><span class="context-ring" style="--used:${Math.round(ctx/128000*100)}%"></span>上下文 ${Math.round(ctx/128000*100)}%</button><button data-harness="stats" class="cost-metric">${usage?'$'+usage.cost.toFixed(4):'费用 —'} <span class="metric-estimate">估</span></button><button data-harness="stats" class="speed-metric">${usage?'42 tok/s':'速度 —'}</button><button data-harness="tools" class="tool-metric">${runtimeSession().calls.length} tools</button></div><button data-harness="stats" class="footer-detail">${runtime.statsScope==='turn'?'上一轮':'会话累计'} · 演示 ${icon('chevron-right')}</button>`;
}
function renderInspectorRuntime(){
  $('#runtime-context')?.remove();
  const ctx=state.session==='dark'?(runtimeSession().compact?12400:48600):0;
  $('#inspector-content').insertAdjacentHTML('afterbegin',`<div class="inspector-section runtime-context" id="runtime-context"><div class="inspector-label">运行概览 <span>演示</span></div><button class="inspector-context-button" data-harness="context"><span>上下文 <strong>${formatTokens(ctx)} <small>/ 128k</small></strong></span><span>${Math.round(ctx/128000*100)}% ${icon('chevron-right')}</span></button><div class="context-track"><span style="width:${ctx/128000*100}%"></span><i style="left:${runtime.threshold}%"></i></div><div class="data-line"><span>思考强度</span><span>${runtime.thinking}</span></div><div class="data-line"><span>自动压缩</span><span>${runtime.autoCompact?runtime.threshold+'% 时触发':'已关闭'}</span></div><div class="runtime-shortcuts"><button data-harness="provider">${icon('server')}Provider</button><button data-harness="context">${icon('compress')}管理上下文</button><button data-harness="queue">${icon('queue')}队列 ${runtimeSession().queue.length}</button></div></div>`);
}
function renderQueueShelf(){
  const s=runtimeSession(),steer=s.queue.filter(q=>q.kind==='steering').length;
  $('#queue-shelf').innerHTML=`<div class="queue-shelf"><button data-harness="queue" class="queue-overview">${icon('queue')}<strong>消息队列</strong><span class="queue-badge">${s.queue.length}</span><span class="queue-caption">${s.paused?'已暂停':s.queue.length?`${steer} 引导 · ${s.queue.length-steer} 后续`:'空闲'}</span>${icon('chevron-right')}</button><div>${s.queue.length?`<button class="icon-button" data-harness="pause-queue" aria-label="${s.paused?'恢复':'暂停'}队列">${icon(s.paused?'play':'pause')}</button>`:''}<button class="icon-button" data-harness="new-queue" aria-label="添加队列消息">${icon('plus')}</button></div></div>`;
}
function queueModal(){
  const s=runtimeSession();
  runtimeModal('消息队列',`<div class="queue-explanation"><span>${icon('queue')}后续消息：当前轮次结算后处理</span><span>${icon('sparkles')}引导消息：在宿主支持的安全边界交付</span></div><div class="queue-modal-toolbar"><span>${s.queue.length} 条等待中 · ${s.paused?'已暂停':'演示队列'}</span><div>${rtButton('new-queue','添加','plus')}${rtButton('pause-queue',s.paused?'恢复':'暂停',s.paused?'play':'pause')}${rtButton('clear-queue','清空')}</div></div><div class="queue-items">${s.queue.length?s.queue.map((q,i)=>`<article class="queue-item"><div class="queue-item-main"><span class="queue-index">${i+1}</span><div><span class="queue-kind ${q.kind==='steering'?'steering':''}">${q.kind==='steering'?'引导消息':'后续消息'}</span><p>${escapeHTML(q.text)}</p><small>${q.attachments?.length?`附带 ${q.attachments.length} 个附件 · `:''}${q.skills?.length?`已指定 ${q.skills.length} 个技能，尚未载入 · `:''}尚未交付</small></div></div><div class="queue-item-actions"><button class="icon-button" data-harness="move-queue" data-id="${q.id}" data-direction="-1" aria-label="上移第 ${i+1} 条" ${i===0?'disabled':''}>${icon('up')}</button><button class="icon-button" data-harness="move-queue" data-id="${q.id}" data-direction="1" aria-label="下移第 ${i+1} 条" ${i===s.queue.length-1?'disabled':''}>${icon('down')}</button><button data-harness="edit-queue" data-id="${q.id}">编辑</button><button data-harness="delete-queue" data-id="${q.id}" aria-label="删除第 ${i+1} 条队列消息">移除</button></div></article>`).join(''):'<div class="empty-compact">队列是空的。运行时也可以先写好下一条指令。</div>'}</div><p class="modal-help">排序、暂停与编辑是产品提案；原型不会给真实 Agent 发送消息。</p>`,`${rtButton('consume-queue','模拟处理下一条','play',`${s.paused||!s.queue.length||state.running||hasPendingAsk()||(state.session==='dark'&&state.approval==='pending')?'disabled':''}`)}<button class="button primary" data-close-modal>完成</button>`,true);
}
function queueEditor(id){
  const q=runtimeSession().queue.find(q=>q.id===id);
  runtimeModal(q?'编辑队列消息':'添加队列消息',`<div class="field"><label for="queue-kind">交付方式</label><select id="queue-kind"><option value="followUp">后续消息 · 当前轮次结束后</option><option value="steering">引导消息 · 安全边界交付</option></select></div><div class="field"><label for="queue-text">消息内容</label><textarea class="modal-textarea" id="queue-text" rows="4" placeholder="描述接下来要处理的事情…">${escapeHTML(q?.text||$('#prompt').value)}</textarea></div><p class="modal-help">引导不会强行中断正在执行的命令；具体交付时机由运行时决定。</p>`,`${rtButton('queue','返回队列')}<button class="button primary" data-harness="save-queue" data-id="${q?.id||''}">保存到队列</button>`);
  if(q)$('#queue-kind').value=q.kind;
}
function enqueueDraft(){
  const text=$('#prompt').value.trim(),s=runtimeSession();
  if(!text&&!s.drafts.length&&!s.draftSkills?.length){queueEditor();return;}
  s.queue.push({id:'q'+Date.now(),text:text||(s.draftSkills?.length?'使用指定技能继续处理':'查看附件并继续处理'),kind:'followUp',attachments:s.drafts.splice(0),skills:skillSession().draftSkills.splice(0)});$('#prompt').value='';renderHarnessUI();toast('已加入当前会话的演示队列');
}
function statsModal(){
  const u=currentUsage(),scope=runtime.statsScope==='turn'?'上一轮':'会话累计';
  runtimeModal('用量与运行统计',`<div class="segmented">${['turn','session'].map(k=>`<button class="${runtime.statsScope===k?'active':''}" data-harness="stats-scope" data-value="${k}">${k==='turn'?'上一轮':'会话累计'}</button>`).join('')}</div><p class="stats-caption">${scope} · ${u?'DeepSeek · 已结算示例':'尚无已结算用量'} · 不代表当前提供方实测</p><div class="metric-grid"><div><small>输入总量</small><strong>${u?u.input.toLocaleString():'—'}</strong><span>含缓存读取与写入</span></div><div><small>输出 Token</small><strong>${u?u.output.toLocaleString():'—'}</strong><span>思考 ${u?u.reasoning.toLocaleString():'—'}，已包含在输出内</span></div><div><small>缓存命中率</small><strong>${u?(u.cache/u.input*100).toFixed(1)+'%':'—'}</strong><span>按输入 token 计算</span></div><div><small>费用估算 · USD</small><strong>${u?'$'+u.cost.toFixed(4):'—'}</strong><span>示例费率，非账单</span></div></div><dl class="stats-details"><div><dt>未缓存输入</dt><dd>${u?(u.input-u.cache-u.write).toLocaleString():'—'}</dd></div><div><dt>缓存读取 / 写入</dt><dd>${u?u.cache.toLocaleString()+' / '+u.write:'—'}</dd></div><div><dt>输入 + 输出总量</dt><dd>${u?(u.input+u.output).toLocaleString():'—'}</dd></div><div><dt>模型请求 / 重试</dt><dd>${u?u.requests+' / 1':'—'}</dd></div><div><dt>首 token 延迟 / 生成速度</dt><dd>${u?'0.8s / 42 tok/s':'—'}</dd></div><div><dt>工具调用 / 失败</dt><dd>${runtimeSession().calls.length} / ${runtimeSession().calls.filter(c=>c.status==='failed').length}</dd></div><div><dt>上下文压缩</dt><dd>${runtimeSession().compact?'1 次 · 模拟':'0 次'}</dd></div></dl><div class="info-banner">${icon('info')}命中率 = 缓存读取 ÷ 输入总量。累计比例按 token 加权；缓存读写与思考量不会重复相加。未上报的统计显示 —。</div>`,rtButton('context','查看上下文','compress'),true);
}
function contextModal(){
  const s=runtimeSession(),before=state.session==='dark'?48600:0,now=s.compact?12400:before;
  runtimeModal('上下文与压缩',`<div class="context-hero"><div><small>当前上下文 · 估算</small><strong>${formatTokens(now)} <span>/ 128k</span></strong></div><span>${Math.round(now/128000*100)}%</span></div><div class="context-composition"><span class="ctx-system" style="flex:${before?6:0}"></span><span class="ctx-history" style="flex:${before?(s.compact?4:31):0}"></span><span class="ctx-tools" style="flex:${before?(s.compact?1.4:9):0}"></span><span class="ctx-files" style="flex:${before?(s.compact?1:2.6):0}"></span></div><div class="context-legend"><span><i class="ctx-system"></i>系统 ${before?'6k':'—'}</span><span><i class="ctx-history"></i>消息 ${s.compact?'4k':before?'31k':'—'}</span><span><i class="ctx-tools"></i>工具 ${s.compact?'1.4k':before?'9k':'—'}</span><span><i class="ctx-files"></i>附件 ${s.compact?'1k':before?'2.6k':'—'}</span></div><p class="modal-help">组成是本稿的估算示意；正式版需宿主提供分类数据，缺失时隐藏分类。</p><div class="settings-row"><div><strong>自动压缩</strong><small>达到阈值后，在运行时允许的边界触发</small></div><button class="switch ${runtime.autoCompact?'on':''}" data-harness="auto-compact" role="switch" aria-checked="${runtime.autoCompact}" aria-label="自动压缩"></button></div><div class="field"><label for="compact-threshold">触发阈值 <span id="threshold-label">${runtime.threshold}%</span></label><input id="compact-threshold" type="range" min="50" max="95" step="5" value="${runtime.threshold}" ${runtime.autoCompact?'':'disabled'}></div><div class="field"><label for="compact-preserve">需要保留的内容</label><textarea id="compact-preserve" class="modal-textarea" rows="2">保留任务目标、未完成步骤、变更文件和验证结果。</textarea></div>${s.compact?'<div class="result-card"><div>48.6k → 12.4k · 已生成摘要（模拟）</div><p>原始历史保留；累计 token 与费用不会归零。</p></div>':''}<p class="modal-help">${state.running||hasPendingAsk()||(state.session==='dark'&&state.approval==='pending')?'当前正在运行或等待工具确认；请先结算本轮，再手动压缩。':'压缩创建摘要并保留最近上下文，原始记录仍可查看。'}</p>`,`${rtButton('compact-preview','预览摘要','file')}${rtButton('compact-run','模拟压缩','compress',`${!before||state.running||hasPendingAsk()||(state.session==='dark'&&state.approval==='pending')?'disabled':''}`)}`,true);
}
function renderCompactionNotice(){return `<div class="compaction-event">${icon('compress')}上下文已压缩 <strong>48.6k → 12.4k</strong><button data-harness="compact-preview">查看摘要</button><span>模拟</span></div>`;}
function compactPreview(){runtimeModal('压缩摘要预览',`<div class="summary-preview"><h3>目标</h3><p>为工作台加入深色主题，默认跟随系统，保留手动选择。</p><h3>已完成</h3><p>定义两组颜色变量、主题切换组件与设置存储。</p><h3>保留的关键上下文</h3><p>3 个变更文件、当前分支、已有验证结果与手机适配要求。</p><h3>下一步</h3><p>检查手机设置抽屉对比度；保留现有快捷键。</p></div><p class="modal-help">这是预先编写的摘要示例，尚未调用模型。正式版不能保证固定压缩比例。</p>`,rtButton('context','返回上下文'));}
function toolSettings(){sessionResources('tools');}
function providerConfig(provider){
  const key=provider||state.model;
  runtimeModal(`${escapeHTML(key)} · 连接设置`,`<div class="field"><label for="provider-name">配置名称</label><input id="provider-name" value="${escapeHTML(key)}" readonly></div><div class="field"><label for="provider-url">服务地址（仅演示，不会连接）</label><input id="provider-url" value="${key==='DeepSeek'?'https://api.deepseek.com':key==='OpenAI'?'https://api.openai.com/v1':'https://gateway.example.com/v1'}"></div><div class="field"><label for="credential-ref">凭据来源</label><input id="credential-ref" value="执行节点环境变量 / 受保护凭据存储" readonly></div><div class="field"><label for="provider-model">模型发现</label><select id="provider-model"><option>由宿主返回可用模型</option><option>自定义模型 ID（配置提案）</option></select></div><div class="provider-specs"><span>请求超时 <b>120s</b></span><span>重试上限 <b>2 次</b></span><span>退避策略 <b>指数退避</b></span></div><div class="info-banner">${icon('shield')}原型不接收真实 API Key，也不会发送凭据或连接请求。正式版在执行节点管理认证。</div><div id="connection-result"></div>`,`${rtButton('provider-test','模拟连接成功','check',`data-provider="${key}"`)}${rtButton('provider-fail','模拟限流失败',null,`data-provider="${key}"`)}`,true);
}
function renderProviders(){
  $('#page-view').innerHTML=`<div class="overline">THE RIGHT MODEL FOR THE WORK.</div><div class="page-heading"><div><h1>模型与 Provider</h1><p>连接属于执行节点，当前会话独立选择模型与思考强度。</p></div>${rtButton('provider-custom','添加连接','plus')}</div><div class="info-banner">${icon('info')}连接状态与模型能力为设计示例。正式版从宿主发现模型、认证和工具/图像支持情况。</div><div class="provider-grid">${[['DeepSeek','OpenAI-compatible','工具调用 / 思考 / 流式'],['OpenAI','Responses','工具调用 / 图像 / 流式'],['Custom','自定义网关','能力由宿主发现']].map(([name,api,caps])=>`<article class="provider-card"><div class="provider-card-title"><span class="provider-avatar">${name[0]}</span><div><h3>${name==='Custom'?'自定义 Provider':name}</h3><small>${api}</small></div><span class="status-badge ${runtime.providerStatus[name]==='ready'?'completed':'stopped'}"><i></i>${runtime.providerStatus[name]==='ready'?'已配置 · 示例':'未配置'}</span></div><p>${caps}</p><div class="provider-card-meta"><span>认证来源</span><strong>执行节点</strong><span>可用模型</span><strong>${name==='Custom'?'待发现':'宿主动态发现'}</strong></div><div class="provider-card-actions">${rtButton('provider-config','连接设置','settings',`data-provider="${name}"`)}${name!=='Custom'?`<button class="button ${state.model===name?'primary':''}" data-model="${name}">${state.model===name?'当前选择':'用于此会话'}</button>`:''}</div></article>`).join('')}</div><div class="provider-runtime-note"><span>${icon('terminal')}</span><div><h3>运行时与模型分别管理</h3><p>当前运行时：rpi harness。Codex app-server 需要独立运行时适配。</p></div></div>`;
}
function thinkingModal(){runtimeModal('思考强度',`<p>展示宿主针对当前模型提供的级别；不支持的级别应隐藏。</p>${[['off','关闭','适合直接回答与轻量任务'],['high','High','当前原型默认，适合一般编程任务'],['max','Max','适合复杂任务，可能增加延迟与用量']].map(([value,label,desc])=>`<button class="model-option ${runtime.thinking===value?'selected':''}" data-harness="set-thinking" data-value="${value}"><div><strong>${label}</strong><small>${desc}</small></div>${runtime.thinking===value?icon('check'):''}</button>`).join('')}`);}
function renderMessageAttachments(attachments=[]){return attachments.length?`<div class="message-attachments">${attachments.map(a=>`<button class="message-attachment ${a.kind==='image'?'image-attachment':''}" data-harness="preview-message" data-id="${a.id}" aria-label="预览附件 ${escapeHTML(a.name)}">${a.kind==='image'?`<img src="${escapeHTML(a.src)}" alt="${escapeHTML(a.name)}">`:icon('file')}<span>${escapeHTML(a.name)}<small>${formatBytes(a.size)} · ${a.kind==='image'?'图像参考':'文档附件'}</small></span>${icon('chevron-right')}</button>`).join('')}</div>`:'';}
function renderReferenceAttachments(){return renderMessageAttachments([sampleImage,sampleDocument]);}
function attachmentMenu(){runtimeModal('图片与附件',`<div class="upload-zone" id="upload-zone">${icon('paperclip')}<strong>拖入图片、文档或代码文件</strong><span>支持粘贴图片 · 最多 6 个 · 每个不超过 10 MB</span><button class="button primary" data-harness="pick-files">选择本地文件</button></div><p class="modal-help">仅在浏览器本地预览，不上传。PNG / JPG / WebP 可预览图像；PDF 只展示元数据；文本与代码显示安全摘要。</p><div class="attachment-samples">${rtButton('sample-image','添加示例图片','image')}${rtButton('sample-doc','添加示例文档','file')}</div><div class="field"><label>或引用当前项目文件</label>${files.map(f=>`<button class="search-result" data-attach="${f.path}">${icon('file')}${f.path}</button>`).join('')}</div>`,'',true);}
function addDraft(a){const s=runtimeSession();if(s.drafts.length>=6){toast('最多添加 6 个附件');return false;}if(s.drafts.some(f=>f.id===a.id)){toast('此附件已添加');return false;}s.drafts.push(a);renderHarnessUI();return true;}
async function addLocalFiles(fileList){
  const owner=runtimeSession();let added=0,rejected=0;
  for(const file of [...fileList]){
    if(owner.drafts.length>=6||file.size>10*1024*1024){rejected++;continue;}
    const image=['image/png','image/jpeg','image/webp'].includes(file.type),pdf=file.type==='application/pdf'||/\.pdf$/i.test(file.name),text=/\.(txt|md|csv|json|rs|ts|js|log)$/i.test(file.name);
    if(!image&&!pdf&&!text){rejected++;continue;}
    const a={id:'file-'+crypto.randomUUID(),name:file.name,size:file.size,kind:image?'image':'document',mime:file.type};
    if(image){a.src=URL.createObjectURL(file);runtime.objectUrls.push(a.src);}
    else if(text){a.text=await file.slice(0,12000).text();a.truncated=file.size>12000;}
    owner.drafts.push(a);added++;
  }
  renderHarnessUI();$('#file-input').value='';if($('#modal').open&&$('#upload-zone'))$('#modal').close();toast(`${added} 个附件已在本地添加${rejected?`；${rejected} 个超出限制或不支持`:''}`);
}
function findAttachment(id){if(id===sampleImage.id)return sampleImage;if(id===sampleDocument.id)return sampleDocument;for(const s of Object.values(runtime.sessions)){const a=s.drafts.find(a=>a.id===id)||s.queue.flatMap(q=>q.attachments||[]).find(a=>a.id===id);if(a)return a;}for(const messages of Object.values(state.messages)){const a=messages.flatMap(m=>m.attachments||[]).find(a=>a.id===id);if(a)return a;}}
function previewAttachment(id){const a=findAttachment(id);if(!a)return;runtime.zoom=1;runtimeModal(escapeHTML(a.name),`${a.kind==='image'?`<div class="image-preview"><img id="preview-image" src="${escapeHTML(a.src)}" alt="${escapeHTML(a.name)}"></div><div class="image-controls">${rtButton('image-out','缩小')}${rtButton('image-reset','适应窗口')}${rtButton('image-in','放大')}</div>`:a.text?`<pre class="document-preview">${escapeHTML(a.text)}</pre>${a.truncated?'<p class="modal-help">只展示前 12 KB 文本。</p>':''}`:`<div class="empty-compact">${icon('file')} PDF 文档 · ${formatBytes(a.size)}<p>当前原型仅展示元数据，不解析 PDF 内容。</p></div>`}<p class="modal-help">${formatBytes(a.size)} · ${a.demo?'内置示例':'用户选择的本地文件'} · 未上传到服务或模型</p>`,'',true);}
function submitHarnessMessage(){
  if(state.running){stopStream();return;}
  const s=runtimeSession(),text=$('#prompt').value.trim();
  if(handleCapabilityCommand(text)){$('#prompt').value='';return;}
  if(text==='/stream'){$('#prompt').value='';startStreamDemo();return;}
  const commands={'/compact':contextModal,'/queue':queueModal,'/model':modelModal,'/tools':toolSettings,'/stats':statsModal,'/provider':()=>providerConfig(state.model)};
  if(commands[text]){$('#prompt').value='';commands[text]();return;}
  if(!text&&!s.drafts.length&&!s.draftSkills?.length)return;
  (state.messages[state.session]||=[]).push({role:'user',text:text||(s.draftSkills?.length?'请使用指定技能处理当前任务':'请结合附件继续处理'),attachments:s.drafts.splice(0),skills:takeDraftSkills(),resourceSnapshot:captureSessionResources()});$('#prompt').value='';startStreamRun();
}
document.addEventListener('click',e=>{
  const button=e.target.closest('button');if(button?.dataset.model){if(state.page==='providers')renderProviders();renderHarnessUI();}
  const b=e.target.closest('[data-harness]');if(!b||b.disabled)return;
  const s=runtimeSession(),id=b.dataset.id;
  switch(b.dataset.harness){
    case 'stats':statsModal();break;
    case 'stats-scope':runtime.statsScope=b.dataset.value;statsModal();renderRuntimeFooter();break;
    case 'context':contextModal();break;
    case 'compact-preview':compactPreview();break;
    case 'auto-compact':runtime.autoCompact=!runtime.autoCompact;contextModal();renderHarnessUI();break;
    case 'compact-run':{if(state.running||hasPendingAsk()||(state.session==='dark'&&state.approval==='pending'))return;const preserve=$('#compact-preserve').value;s.compact={before:48600,after:12400,preserve};$('#modal').close();renderTab();toast('已生成压缩摘要示例，累计用量保持不变');break;}
    case 'queue':queueModal();break;
    case 'new-queue':queueEditor();break;
    case 'edit-queue':queueEditor(id);break;
    case 'save-queue':{const text=$('#queue-text').value.trim();if(!text){$('#queue-text').focus();toast('请填写队列消息');break;}const q=s.queue.find(q=>q.id===id);if(q){q.text=text;q.kind=$('#queue-kind').value;}else s.queue.push({id:'q'+Date.now(),text,kind:$('#queue-kind').value});renderHarnessUI();queueModal();break;}
    case 'enqueue':enqueueDraft();break;
    case 'delete-queue':s.queue=s.queue.filter(q=>q.id!==id);renderHarnessUI();queueModal();break;
    case 'move-queue':{const index=s.queue.findIndex(q=>q.id===id),next=index+Number(b.dataset.direction);if(index>=0&&next>=0&&next<s.queue.length)[s.queue[index],s.queue[next]]=[s.queue[next],s.queue[index]];renderHarnessUI();queueModal();break;}
    case 'pause-queue':s.paused=!s.paused;renderHarnessUI();if($('#modal').open)queueModal();break;
    case 'clear-queue':s.queue=[];renderHarnessUI();queueModal();break;
    case 'consume-queue':{if(s.paused||state.running||hasPendingAsk()||(state.session==='dark'&&state.approval==='pending'))return;const q=s.queue.shift();if(q)(state.messages[state.session]||=[]).push({role:'user',text:`[队列模拟交付 · ${q.kind==='steering'?'引导':'后续'}] ${q.text}`,attachments:q.attachments,skills:recordSkillLoads(q.skills||[]),resourceSnapshot:captureSessionResources()});$('#modal').close();state.tab='conversation';renderTab();toast('已模拟交付一条消息，未启动模型');break;}
    case 'tools':$('#modal').close();focusTimeline('tools');break;
    case 'tool-failed':focusTimeline('web');break;
    case 'tool-settings':toolSettings();break;
    case 'retry-tool':{const c=s.calls.find(c=>c.id===id);if(!c)return;c.attempts++;c.status='done';c.duration='1.1s';c.lines='800 字符';c.output='模拟重试成功，返回示例参考摘要。未发起网络请求。';refreshTimeline();toast('已模拟重试成功');break;}
    case 'cancel-tool':{const c=s.calls.find(c=>c.id===id);if(!c)return;c.status='cancelled';c.output='子任务已取消（演示）。';refreshTimeline();break;}
    case 'tool-output':{const c=s.calls.find(c=>c.id===id);if(c)runtimeModal(`${c.name} · 输出`,`<pre class="document-preview">${escapeHTML(c.output)}</pre><p class="modal-help">调用记录与工具状态均为演示。</p>`,'',true);break;}
    case 'provider':providerConfig(state.model);break;
    case 'provider-custom':providerConfig('Custom');break;
    case 'provider-config':providerConfig(b.dataset.provider);break;
    case 'provider-test':{const address=$('#provider-url');try{const u=new URL(address.value);if(!['https:','http:'].includes(u.protocol))throw Error();$('#connection-result').innerHTML='<div class="result-card"><div>连接成功 · 模拟</div><p>已模拟认证有效、模型可发现；没有发起网络请求。</p></div>';}catch{toast('请输入完整的 HTTP(S) 地址');address.focus();}break;}
    case 'provider-fail':$('#connection-result').innerHTML='<div class="provider-error"><strong>429 · 请求过于频繁（模拟）</strong><p>建议按 Retry-After 等待，或切换已授权的模型。认证错误不应盲目重试。</p></div>';break;
    case 'thinking':thinkingModal();break;
    case 'set-thinking':runtime.thinking=b.dataset.value;$('#modal').close();renderHarnessUI();break;
    case 'pick-files':$('#file-input').click();break;
    case 'sample-image':if(addDraft({...sampleImage}))$('#modal').close();break;
    case 'sample-doc':if(addDraft({...sampleDocument}))$('#modal').close();break;
    case 'remove-attachment':s.drafts=s.drafts.filter(a=>a.id!==id);renderHarnessUI();break;
    case 'preview-draft':case 'preview-message':previewAttachment(id);break;
    case 'image-in':runtime.zoom=Math.min(3,runtime.zoom+.25);$('#preview-image').style.transform=`scale(${runtime.zoom})`;break;
    case 'image-out':runtime.zoom=Math.max(.5,runtime.zoom-.25);$('#preview-image').style.transform=`scale(${runtime.zoom})`;break;
    case 'image-reset':runtime.zoom=1;$('#preview-image').style.transform='';break;
  }
});
document.addEventListener('input',e=>{if(e.target.id==='compact-threshold'){runtime.threshold=Number(e.target.value);$('#threshold-label').textContent=runtime.threshold+'%';renderHarnessUI();}});
document.addEventListener('change',e=>{if(e.target.id==='file-input')addLocalFiles(e.target.files);});
document.addEventListener('paste',e=>{if(e.target.id!=='prompt')return;const images=[...e.clipboardData.items].filter(i=>i.kind==='file'&&i.type.startsWith('image/')).map(i=>i.getAsFile()).filter(Boolean);if(images.length){e.preventDefault();addLocalFiles(images);}});
document.addEventListener('dragover',e=>{if(e.target.closest('.composer,.upload-zone')){e.preventDefault();$('.composer').classList.add('drag-active');}});
document.addEventListener('dragleave',e=>{if(!e.relatedTarget||!e.target.closest('.composer')?.contains(e.relatedTarget))$('.composer')?.classList.remove('drag-active');});
document.addEventListener('drop',e=>{if(!e.target.closest('.composer,.upload-zone'))return;e.preventDefault();$('.composer').classList.remove('drag-active');addLocalFiles(e.dataTransfer.files);});
document.addEventListener('keydown',e=>{if(e.target.id==='prompt'&&e.key==='Enter'&&e.altKey&&!e.isComposing){e.preventDefault();e.stopImmediatePropagation();enqueueDraft();}},true);
window.addEventListener('beforeunload',()=>runtime.objectUrls.forEach(url=>URL.revokeObjectURL(url)));
