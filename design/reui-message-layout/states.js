'use strict';
// Visual scenarios only. No model, node or workspace I/O.
const demoFiles = {
  'theme.ts': { path:'src/lib/theme.ts', hunk:'@@ -22,3 +22,5 @@ · 预览片段', rows:[
    ['context',22,22,'export function useTheme(choice: ThemeChoice) {'],
    ['remove',23,'','  const dark = false'],
    ['add','',23,'  const dark = choice.dark'],
    ['add','',24,'  const tokens = themeTokens(choice)'],
    ['context',24,25,'  return buildTheme(tokens)'],
  ]},
  'settings-page.tsx': { path:'src/components/settings-page.tsx', hunk:'@@ -61,3 +61,5 @@ · 预览片段', rows:[
    ['context',61,61,'<section aria-label="外观">'],
    ['remove',62,'','  <span>默认主题</span>'],
    ['add','',62,'  <Switch checked={theme.dark}'],
    ['add','',63,'    onCheckedChange={setDark} />'],
    ['context',63,64,'</section>'],
  ]},
  'App.tsx': { path:'src/App.tsx', hunk:'@@ -80,2 +80,4 @@ · 预览片段', rows:[
    ['context',80,80,'function App() {'],
    ['add','',81,'  const { theme, setDark } = useTheme()'],
    ['add','',82,'  return <Workspace theme={theme} />'],
    ['context',81,83,'}'],
  ]},
};
let selectedDemoFile = 'theme.ts';
function openPreviewFile(name, reveal = true) {
  const file = demoFiles[name];
  if (!file) return;
  selectedDemoFile = name;
  if (reveal) setPanel('files');
  const preview = $('.diff-preview');
  preview.replaceChildren();
  const header = document.createElement('div'); header.className = 'code-caption';
  const title = document.createElement('span'); title.id = 'diff-filename'; title.textContent = name; title.title = file.path;
  const copyButton = document.createElement('button'); copyButton.className = 'icon-button push'; copyButton.dataset.demo = 'copy-diff'; copyButton.setAttribute('aria-label','复制当前文件 diff'); copyButton.innerHTML = '<i data-icon="copy"></i>';
  header.append(title,copyButton);
  const scroller = document.createElement('div'); scroller.className = 'diff-scroll'; scroller.tabIndex = 0; scroller.setAttribute('aria-label',name + ' 差异');
  const table = document.createElement('table'); table.className = 'diff-table'; table.setAttribute('aria-label',name + ' 的演示 diff');
  const caption = document.createElement('caption'); caption.className = 'sr-only'; caption.textContent = '依次为旧行号、新行号、变更符号和代码；仅为演示片段'; table.append(caption);
  const head = document.createElement('thead'); const headRow = document.createElement('tr');
  for (const text of ['旧','新','±','代码']) { const th = document.createElement('th'); th.scope='col'; th.textContent=text; headRow.append(th); }
  head.append(headRow); table.append(head);
  const tbody = document.createElement('tbody'); const hunk = document.createElement('tr'); hunk.className = 'diff-hunk';
  const hunkCell = document.createElement('td'); hunkCell.colSpan=4; hunkCell.textContent=file.hunk; hunk.append(hunkCell); tbody.append(hunk);
  for (const [kind,oldLine,newLine,code] of file.rows) {
    const tr = document.createElement('tr'); tr.className='diff-line diff-' + kind;
    for (const value of [oldLine,newLine,kind==='add'?'+':kind==='remove'?'−':' ',code]) {
      const cell=document.createElement('td'); cell.textContent=String(value); tr.append(cell);
    }
    tbody.append(tr);
  }
  table.append(tbody); scroller.append(table); preview.append(header,scroller); renderIcons(preview);
  document.querySelectorAll('.file-tree [data-open-file]').forEach(button => {
    if (button.dataset.openFile===name) button.setAttribute('aria-current','true'); else button.removeAttribute('aria-current');
  });
}
openPreviewFile(selectedDemoFile,false);

const initialDemoMessages = $('#messages').innerHTML;
const demoChunks = [
  '已检查共享主题契约，接下来验证消息布局切换。',
  '正文与输入框沿同一列对齐；打开文件面板时，两者同时收窄。',
  '代码块、表格和工具输出各自横向滚动，不会把整条消息撑出阅读区。',
  '你可以上翻查看前文。新内容仍会进入当前轮次，但不会把视口抢回底部。',
];
let demoTimer = null;
let demoChunkIndex = 0;
let followDemoOutput = false;
let unreadDemoOutput = false;
let demoScrollFrame = 0;
function atDemoBottom() { return scrollArea.scrollHeight-scrollArea.clientHeight-scrollArea.scrollTop <= 36; }
function updateDemoJump() {
  const button = $('.jump-bottom');
  button.hidden = atDemoBottom();
  button.classList.toggle('has-unread',unreadDemoOutput);
  button.setAttribute('aria-label',unreadDemoOutput?'有新内容，回到最新消息':'回到最新消息');
  if (atDemoBottom()) { unreadDemoOutput=false; button.classList.remove('has-unread'); }
}
function settleDemoScroll() {
  if (demoScrollFrame) return;
  demoScrollFrame = requestAnimationFrame(() => {
    demoScrollFrame=0;
    if (followDemoOutput) scrollArea.scrollTop=scrollArea.scrollHeight;
    updateDemoJump();
  });
}
scrollArea.addEventListener('scroll', () => { followDemoOutput=atDemoBottom(); if (followDemoOutput) unreadDemoOutput=false; updateDemoJump(); },{passive:true});
new ResizeObserver(settleDemoScroll).observe($('#messages'));
new ResizeObserver(settleDemoScroll).observe(scrollArea);
new MutationObserver(settleDemoScroll).observe($('#messages'),{childList:true,subtree:true,characterData:true});
function stopDemoTimer() { if (demoTimer!==null) clearInterval(demoTimer); demoTimer=null; }
function setDemoBusy(busy) {
  document.body.dataset.busy=String(busy);
  $('.stop-button').hidden=!busy;
  $('.send-button').hidden=busy;
  $('#preview-state').setAttribute('data-running',String(busy));
}
function scenarioTurn(question,content) {
  const article=document.createElement('article'); article.className='turn scenario-turn'; article.setAttribute('aria-label','状态演示轮次');
  // content is a fixed, developer-authored template; user input uses textContent.
  article.innerHTML='<div class="user-message message"><div class="user-bubble"></div></div><div class="assistant-message">'+content+'</div>';
  article.querySelector('.user-bubble').textContent=question;
  return article;
}
function showDemoScenario(value) {
  stopDemoTimer(); demoChunkIndex=0; followDemoOutput=false; unreadDemoOutput=false;
  $('#messages').innerHTML=initialDemoMessages;
  document.body.dataset.scenario=value;
  setDemoBusy(value==='running');
  if (value==='running') {
    $('#messages').append(scenarioTurn('继续验证长消息与滚动跟随。',`
      <p class="commentary">先运行布局测试，再检查右栏打开后的代码和表格宽度。</p>
      <details class="tool-call live-tool" open><summary><span class="activity-spinner" aria-hidden="true"></span><span>正在运行 <code>node verify-layout.cjs</code></span><span class="duration push">本地演示</span><i data-icon="chevron-right"></i></summary><div class="tool-output"><div class="code-caption">终端输出 · 模拟增量</div><pre><code>PASS reading column aligns with composer\nPASS theme setting persists\n<span id="live-terminal">正在检查长消息…</span></code></pre></div></details>
      <div class="stream-output prose" id="stream-output" aria-label="增量回答"></div>
      <div class="working-status"><span class="activity-spinner" aria-hidden="true"></span><span id="demo-progress">执行中 · 仅本地演示</span></div>
      <div class="demo-controls" role="group" aria-label="增量预览控制"><span>演示控制</span><button class="text-button" data-demo="play">播放增量</button><button class="text-button" data-demo="append">追加一段</button><button class="text-button" data-demo="finish">完成本轮</button></div>
    `));
  } else if (value==='failed') {
    $('#messages').append(scenarioTurn('检查消息布局测试，保留失败信息。',`
      <p class="commentary">已完成布局修改，正在验证窗口收窄后的表现。</p>
      <details class="tool-call failed-tool" open><summary><i data-icon="x"></i><span>运行失败 <code>node verify-layout.cjs</code></span><span class="duration push">exit 1</span><i data-icon="chevron-right"></i></summary><div class="tool-output"><div class="code-caption">终端输出</div><pre><code>FAIL composer aligns with reading column
<span class="error-text">Expected: 32px
Received: 47px</span>

at verify-layout.cjs:95
1 failed · 25 passed</code></pre></div></details>
      <div class="error-notice"><i data-icon="x"></i><div><strong>这轮没有全部通过</strong><p>保留失败的命令、退出码和输出，不把执行失败显示为已完成。</p></div></div>
      <div class="answer message" data-copy="失败发生在非覆盖式滚动条占用的 15px 空间，修复输入框边距后需要重新运行测试。"><div class="prose"><p>失败发生在滚动条占用的 <code>15px</code> 空间。消息列与输入框的边距需要同时计算这段宽度。</p><p>下面可以本地模拟重新检查，不会运行实际命令。</p></div><div class="retry-row"><button class="text-button retry-button" data-demo="retry"><i data-icon="terminal"></i>模拟重试</button><span class="muted small">仅用于检查状态转换</span></div></div>
    `));
  } else if (value==='long') {
    $('#messages').append(scenarioTurn('展开说明消息区的排版规则，包含表格、代码和路径引用。',`
      <div class="answer message" data-copy="消息区排版规则：正文、活动和输出分层；阅读列限宽；代码与表格局部滚动；上翻历史时不自动回到底部。"><div class="prose long-prose">
      <h2>消息区的阅读层次</h2><p>一轮消息不是一组等权重的卡片。用户输入是起点，过程说明帮助理解当前动作，最终回答承担主要阅读内容。工具输出默认只保留摘要，需要时再展开。</p>
      <h3>1. 保持稳定的对齐线</h3><p>正文和输入框共享 <code>768px</code> 的最大宽度。右侧打开文件或运行面板后，两者一起收窄，不出现“正文一条线、输入框另一条线”。</p>
      <blockquote>阅读中的稳定性比自动跳转更重要。上翻历史时，新的增量不应改变当前视口。</blockquote>
      <h3>2. 内容密度，而不是堆叠边框</h3><div class="table-scroll wide-content" tabindex="0" aria-label="消息排版规则表格"><table class="prose-table"><thead><tr><th scope="col">内容类型</th><th scope="col">默认展示</th><th scope="col">展开后</th><th scope="col">对齐规则</th></tr></thead><tbody><tr><td>用户输入</td><td>紧凑气泡</td><td>完整文本、引用操作</td><td>阅读列右侧</td></tr><tr><td>读取 / 搜索</td><td>聚合活动摘要</td><td>独立调用、路径、结果</td><td>正文左侧</td></tr><tr><td>命令执行</td><td>命令与状态</td><td>终端输出、退出码</td><td>正文左侧</td></tr><tr><td>修改文件</td><td>文件数和增删行</td><td>文件清单、带行号的 diff</td><td>正文 / 右栏</td></tr></tbody></table></div>
      <h3>3. 代码只在自身区域滚动</h3><p>下面故意放入长行，验证阅读列不会被撑宽。复制按钮复制原始代码，不包含行号。</p>
      <div class="code-block wide-content"><div class="code-caption"><span>Rust · 长行与行号</span><button class="icon-button push" aria-label="复制长代码" data-demo="copy-long-code"><i data-icon="copy"></i></button></div><pre class="numbered-code" tabindex="0" aria-label="长代码示例"><code id="long-code"><span class="code-line"><span class="code-number" aria-hidden="true">1</span><span><span class="syntax-keyword">pub fn</span> reading_column(viewport: f32, sidebar: f32, rail: f32) -&gt; f32 {</span></span><span class="code-line"><span class="code-number" aria-hidden="true">2</span><span>    <span class="syntax-keyword">let</span> available_width = viewport - sidebar - rail - inset_left - inset_right - scrollbar_gutter_left - scrollbar_gutter_right;</span></span><span class="code-line"><span class="code-number" aria-hidden="true">3</span><span>    available_width.min(<span class="syntax-string">768.0</span>)</span></span><span class="code-line"><span class="code-number" aria-hidden="true">4</span><span>}</span></span></code></pre></div>
      <h3>4. 明确区分失败与完成</h3><ul><li>执行中保留当前活动，不提前显示完成标记。</li><li>失败展示退出码和原始输出，最终说明单独呈现。</li><li>停止后保留已经生成的内容，不清空这轮记录。</li></ul>
      <h3>5. 路径属于内容，不是装饰</h3><p>涉及文件时，使用可操作的路径：<button class="inline-file" data-open-file="theme.ts">src/lib/theme.ts</button> 与 <button class="inline-file" data-open-file="settings-page.tsx">src/components/settings-page.tsx</button>。选择不同路径，应展示对应的 diff，而不是只替换标题。</p>
      <p class="muted small">本页所有数据均为演示内容。此长回答仅用于排版验收，不代表来自模型的实时回答。</p></div><div class="message-actions"><button class="icon-button" aria-label="复制长回答" data-action="copy"><i data-icon="copy"></i></button><button class="icon-button" aria-label="引用长回答" data-action="quote"><i data-icon="quote"></i></button></div></div>
    `));
  }
  renderIcons($('#messages')); applySettings();
  requestAnimationFrame(() => {
    if (value==='running' || value==='failed') { followDemoOutput=true; scrollArea.scrollTop=scrollArea.scrollHeight; }
    else if (value==='long') scrollArea.scrollTop += $('.scenario-turn').getBoundingClientRect().top - scrollArea.getBoundingClientRect().top - 20;
    else scrollArea.scrollTop=0;
    updateDemoJump();
  });
}
function appendDemoChunk() {
  if (document.body.dataset.busy!=='true') return;
  const output=$('#stream-output');
  if (!output || demoChunkIndex>=demoChunks.length) { stopDemoTimer(); return; }
  if (!followDemoOutput) unreadDemoOutput=true;
  const p=document.createElement('p'); p.textContent=demoChunks[demoChunkIndex++]; output.append(p);
  $('#live-terminal').textContent=`PASS layout step ${demoChunkIndex} / ${demoChunks.length}`;
  $('#demo-progress').textContent=`执行中 · 已预览 ${demoChunkIndex} / ${demoChunks.length} 段`;
  if (demoChunkIndex===demoChunks.length) { stopDemoTimer(); $('[data-demo="play"]').textContent='增量已展示'; $('[data-demo="play"]').disabled=true; $('[data-demo="append"]').disabled=true; }
  settleDemoScroll();
}
function finishDemoTurn(stopped=false) {
  if (document.body.dataset.busy!=='true') return;
  stopDemoTimer(); setDemoBusy(false);
  const status=$('.working-status'); status.classList.add('settled-status');
  status.innerHTML=stopped?'<i data-icon="x"></i><span>已停止 · 保留已有内容</span>':'<i class="success" data-icon="check"></i><span>演示完成 · 未运行实际命令</span>';
  const summary=$('.live-tool>summary');
  summary.innerHTML=stopped?'<i data-icon="x"></i><span>命令演示已停止</span><span class="duration push">cancelled</span>':'<i class="success" data-icon="check"></i><span>命令演示已完成</span><span class="duration push">exit 0 · 演示</span>';
  $('.demo-controls').hidden=true; renderIcons($('.scenario-turn')); updateDraft(); settleDemoScroll();
}
$('#preview-state').addEventListener('change',()=>showDemoScenario($('#preview-state').value));
document.addEventListener('click',event=>{
  const button=event.target.closest('button'); if (!button) return;
  if (button.dataset.action==='bottom') { followDemoOutput=true; unreadDemoOutput=false; updateDemoJump(); }
  switch(button.dataset.demo) {
    case 'append': appendDemoChunk(); break;
    case 'play':
      if (demoTimer!==null) { stopDemoTimer(); button.textContent='继续增量'; }
      else { button.textContent='暂停增量'; appendDemoChunk(); if (demoChunkIndex<demoChunks.length) demoTimer=setInterval(appendDemoChunk,650); }
      break;
    case 'finish': finishDemoTurn(); break;
    case 'stop': finishDemoTurn(true); break;
    case 'retry': $('#preview-state').value='running'; showDemoScenario('running'); toast('已进入本地重试演示，不执行真实命令'); break;
    case 'copy-diff': {
      const file=demoFiles[selectedDemoFile];
      void copy('--- a/'+file.path+'\n+++ b/'+file.path+'\n'+file.hunk+'\n'+file.rows.map(([kind,,,text])=>(kind==='add'?'+':kind==='remove'?'-':' ')+text).join('\n')); break;
    }
    case 'copy-long-code': void copy([...$('#long-code').querySelectorAll('.code-line')].map(line=>line.lastElementChild.textContent).join('\n')); break;
  }
});
window.addEventListener('pagehide',()=>{stopDemoTimer();cancelAnimationFrame(demoScrollFrame)});
requestAnimationFrame(updateDemoJump);
