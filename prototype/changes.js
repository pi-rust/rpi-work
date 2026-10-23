/* 120 local fixtures demonstrate navigation, not 120 real repository changes. */
const additionalChanges=Array.from({length:117},(_,i)=>{
  const directories=['src/components','src/views','src/state','crates/work-client/src','crates/work-core/src','tests','docs'];
  const names=['appearance','sidebar','workspace','messages','attachments','queue','providers','context','stream','session','tool_call','usage','preferences','transport','reconnect','approval','history'];
  const dir=directories[i%directories.length],name=names[i%names.length]+(Math.floor(i/names.length)?'_'+Math.floor(i/names.length):'')+(dir==='docs'?'.md':'.rs');
  const status=i%23===0?'R':i%19===0?'D':i%17===0?'A':'M';
  const binary=i===116,added=status==='D'?0:4+i%23,removed=status==='A'?0:1+i%9;
  return {name:binary?'theme-preview.png':name,path:binary?'assets/theme-preview.png':dir+'/'+name,status,added:binary?0:added,removed:binary?0:removed,binary,origin:'workspace',oldPath:status==='R'?dir+'/legacy_'+name:null,lines:binary?[]:status==='D'?[['del','1','// Removed legacy module'],['del','2','pub fn legacy_theme() {}']]:[['','1',dir==='docs'?'# Theme integration':'use crate::theme::Appearance;'],['','2',''],['del','3','// Previous appearance behavior'],['add','3','// Shared theme preference'],['add','4','// Keep system and user appearance in sync'],['add','5','// Design fixture only; no source file was changed.'],['','6','']]};
});
const allChangeFixtures=[...files.map(f=>({...f,status:'M',origin:'turn'})),...additionalChanges];
const changeState={sessions:{}};
function reviewState(){return changeState.sessions[state.session]||=( {scope:'workspace',query:'',status:'all',unreviewed:false,reviewed:new Set(),collapsed:new Set(),mobileDiff:false,onlyDiff:false} );}
function availableChanges(){return state.session==='dark'?allChangeFixtures:[];}
function currentChangeFile(){return availableChanges()[state.selectedFile];}
function filteredChanges(){
  const r=reviewState(),groups=new Map();
  availableChanges().filter(f=>(r.scope!=='turn'||f.origin==='turn')&&(!r.query||(f.path+' '+(f.oldPath||'')).toLowerCase().includes(r.query.toLowerCase()))&&(r.status==='all'||f.status===r.status)&&(!r.unreviewed||!r.reviewed.has(f.path))).forEach(f=>{const dir=f.path.slice(0,f.path.lastIndexOf('/'));if(!groups.has(dir))groups.set(dir,[]);groups.get(dir).push(f);});
  return [...groups.values()].flat();
}
function reviewScope(scope){reviewState().scope=scope;reviewState().mobileDiff=false;setTab('changes');}
function changeRow(f){const r=reviewState(),idx=availableChanges().indexOf(f),selected=idx===state.selectedFile;return `<button class="change-file ${selected?'selected':''}" data-change-index="${idx}" aria-pressed="${selected}" title="${escapeHTML(f.path)}"><span class="change-file-icon">${icon(f.binary?'image':'file')}</span><span class="change-file-name">${escapeHTML(f.name)}<small>${f.origin==='turn'?'本轮涉及':f.oldPath?'从 '+escapeHTML(f.oldPath.split('/').pop()):'已有工作区变更'}</small></span><span class="file-change-status status-${f.status}">${f.binary?'BIN':f.status}</span><span class="file-review-mark ${r.reviewed.has(f.path)?'reviewed':''}" aria-label="${r.reviewed.has(f.path)?'已审阅':'未审阅'}">${icon(r.reviewed.has(f.path)?'circle-check':'clock')}</span></button>`;}
function renderChangeWorkspace(){
  const r=reviewState(),all=availableChanges(),visible=filteredChanges();
  const searchActive=document.activeElement?.id==='change-search',cursor=searchActive?document.activeElement.selectionStart:null;
  const oldScroll=$('.change-file-list')?.scrollTop||0;
  if(!visible.some(f=>availableChanges().indexOf(f)===state.selectedFile)&&visible.length)state.selectedFile=all.indexOf(visible[0]);
  const selected=visible.length?currentChangeFile():null;
  const grouped=new Map();visible.forEach(f=>{const dir=f.path.slice(0,f.path.lastIndexOf('/'));if(!grouped.has(dir))grouped.set(dir,[]);grouped.get(dir).push(f);});
  const reviewed=all.filter(f=>r.reviewed.has(f.path)).length;
  $('#task-body').innerHTML=`<section class="change-workspace ${r.mobileDiff?'show-diff':''}" aria-label="变更审阅"><div class="review-overview"><div><h2>工作区变更 <span>${all.length}</span></h2><p>只预览选中的文件 · 示例数据，未改动真实项目</p></div><span class="review-progress">${reviewed}/${all.length} 已审阅</span></div><div class="review-scope" role="group" aria-label="变更范围"><button data-review-scope="workspace" class="${r.scope==='workspace'?'active':''}">整个工作区 <b>${all.length}</b></button><button data-review-scope="turn" class="${r.scope==='turn'?'active':''}">本轮涉及 <b>${all.filter(f=>f.origin==='turn').length}</b></button><span>审阅基准：工作区 ↔ HEAD</span></div><div class="change-layout"><aside class="change-browser" aria-label="变更文件列表"><div class="change-filters"><div class="change-search">${icon('search')}<input id="change-search" value="${escapeHTML(r.query)}" placeholder="搜索文件名或路径…" aria-label="搜索变更文件"></div><div class="change-filter-row"><select id="change-status" aria-label="筛选变更类型"><option value="all">全部类型</option><option value="M">已修改 M</option><option value="A">新增 A</option><option value="D">删除 D</option><option value="R">重命名 R</option></select><label><input type="checkbox" id="change-unreviewed" ${r.unreviewed?'checked':''}>未审阅</label></div></div><div class="change-list-caption"><span>${visible.length} 个文件</span><button data-review-action="folders">${r.collapsed.size?'展开目录':'折叠目录'}</button></div><div class="change-file-list">${visible.length?[...grouped].map(([dir,entries])=>`<section class="change-directory"><button class="change-directory-title" data-review-directory="${escapeHTML(dir)}" aria-expanded="${!r.collapsed.has(dir)}">${icon(r.collapsed.has(dir)?'chevron-right':'chevron-down')}${icon('folder')}<span>${escapeHTML(dir)}</span><small>${entries.length}</small></button><div ${r.collapsed.has(dir)?'hidden':''}>${entries.map(changeRow).join('')}</div></section>`).join(''):'<div class="change-no-results"><strong>没有匹配的文件</strong><p>试试其他路径、类型或审阅范围。</p><button class="button" data-review-action="reset">清除筛选</button></div>'}</div></aside><div class="change-preview">${selected?renderSelectedDiff(selected,visible):'<div class="change-preview-empty">'+icon('diff')+'<p>没有可预览的变更</p></div>'}</div></div></section>`;
  $('#change-status').value=r.status;$('.change-file-list').scrollTop=oldScroll;
  if(searchActive){$('#change-search').focus();$('#change-search').setSelectionRange(cursor,cursor);}
}
function renderSelectedDiff(f,visible){
  const r=reviewState(),index=visible.indexOf(f),labels={M:'已修改',A:'新增',D:'已删除',R:'重命名'};
  const lines=r.onlyDiff?f.lines.filter(([kind])=>kind):f.lines;
  return `<div class="selected-file-heading"><button class="back-to-files" data-review-action="back">${icon('chevron-right')}文件列表</button><div class="selected-file-path"><strong>${escapeHTML(f.name)}</strong><span title="${escapeHTML(f.path)}">${escapeHTML(f.path)}</span></div><button class="icon-button" data-review-action="copy" aria-label="复制文件路径">${icon('paperclip')}</button></div><div class="selected-file-meta"><span class="file-change-status status-${f.status}">${labels[f.status]}</span><span>${f.origin==='turn'?'本轮涉及的文件':'其他工作区变更'}</span><span class="file-lines"><b class="added">+${f.added}</b> <b class="removed">−${f.removed}</b></span></div>${f.oldPath?`<div class="renamed-path">${escapeHTML(f.oldPath)} → ${escapeHTML(f.path)}</div>`:''}<div class="review-file-toolbar"><label><input id="mark-reviewed" type="checkbox" ${r.reviewed.has(f.path)?'checked':''}>已审阅</label><label><input id="only-diff" type="checkbox" ${r.onlyDiff?'checked':''}>只看差异</label><button data-review-action="feedback">${icon('message')}提意见</button></div><div class="selected-diff-scroll">${f.binary?`<div class="binary-preview">${icon('image')}<strong>二进制文件</strong><p>不生成文本 diff；正式版按文件类型提供预览。</p></div>`:`<div class="diff-lines">${lines.map(([type,n,text])=>`<div class="diff-line ${type}"><span class="line-number">${n}</span><span>${type==='add'?'+':type==='del'?'−':' '}</span><span>${escapeHTML(text)}</span></div>`).join('')}</div>`}</div><div class="review-navigation"><div><button class="icon-button" data-review-step="-1" aria-label="上一个文件" ${index<=0?'disabled':''}>${icon('up')}</button><span>${index+1} / ${visible.length}</span><button class="icon-button" data-review-step="1" aria-label="下一个文件" ${index>=visible.length-1?'disabled':''}>${icon('down')}</button></div><button class="button" data-review-action="next-unreviewed">下一个未审阅 ${icon('chevron-right')}</button></div>`;
}
function selectChange(index){
  state.selectedFile=index;const f=currentChangeFile();if(!f)return;
  reviewState().collapsed.delete(f.path.slice(0,f.path.lastIndexOf('/')));reviewState().mobileDiff=true;
  renderChangeWorkspace();$('.change-file.selected')?.scrollIntoView({block:'nearest'});
}
document.addEventListener('click',event=>{
  const b=event.target.closest('button');if(!b||b.disabled)return;
  const r=reviewState();
  if(b.dataset.reviewScope){reviewScope(b.dataset.reviewScope);return;}
  if(b.hasAttribute('data-change-index')){selectChange(Number(b.dataset.changeIndex));return;}
  if(b.hasAttribute('data-review-directory')){const dir=b.dataset.reviewDirectory;r.collapsed.has(dir)?r.collapsed.delete(dir):r.collapsed.add(dir);renderChangeWorkspace();return;}
  const visible=filteredChanges(),current=currentChangeFile(),index=visible.indexOf(current);
  if(b.hasAttribute('data-review-step')){const next=visible[index+Number(b.dataset.reviewStep)];if(next)selectChange(availableChanges().indexOf(next));return;}
  switch(b.dataset.reviewAction){
    case 'folders':if(r.collapsed.size)r.collapsed.clear();else filteredChanges().forEach(f=>r.collapsed.add(f.path.slice(0,f.path.lastIndexOf('/'))));renderChangeWorkspace();break;
    case 'reset':r.query='';r.status='all';r.unreviewed=false;renderChangeWorkspace();break;
    case 'back':r.mobileDiff=false;renderChangeWorkspace();break;
    case 'copy':navigator.clipboard?.writeText(current.path).then(()=>toast('已复制文件路径')).catch(()=>toast(current.path));break;
    case 'feedback':setTab('conversation');$('#prompt').value=`请检查 ${current.path} 的修改，特别关注 `;$('#prompt').focus();break;
    case 'next-unreviewed':{const ordered=[...visible.slice(index+1),...visible.slice(0,index+1)],next=ordered.find(f=>!r.reviewed.has(f.path));if(next)selectChange(availableChanges().indexOf(next));else toast('当前筛选范围的文件已全部审阅');break;}
  }
});
document.addEventListener('input',event=>{if(event.target.id==='change-search'){reviewState().query=event.target.value;renderChangeWorkspace();}});
document.addEventListener('change',event=>{
  const r=reviewState();
  if(event.target.id==='change-status')r.status=event.target.value;
  else if(event.target.id==='change-unreviewed')r.unreviewed=event.target.checked;
  else if(event.target.id==='mark-reviewed'){const f=currentChangeFile();if(event.target.checked)r.reviewed.add(f.path);else r.reviewed.delete(f.path);}
  else if(event.target.id==='only-diff')r.onlyDiff=event.target.checked;
  else return;
  renderChangeWorkspace();
});
document.addEventListener('keydown',event=>{
  const b=event.target.closest?.('[data-change-index]');if(!b||!['ArrowDown','ArrowUp'].includes(event.key))return;
  event.preventDefault();const visible=filteredChanges(),index=visible.indexOf(availableChanges()[Number(b.dataset.changeIndex)]),next=visible[index+(event.key==='ArrowDown'?1:-1)];
  if(next){selectChange(availableChanges().indexOf(next));$('.change-file.selected')?.focus();}
});
