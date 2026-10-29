/* File explorer design: bundled snapshots, no arbitrary filesystem access or server execution. */
const projectFileState={projects:{},project:null};
function projectFileView(){return projectFileState.projects[projectFileState.project]||=( {query:'',selected:'README.md',collapsed:new Set(),mode:'preview',wrap:true,mobilePreview:false} );}
function projectEntries(){return (typeof metaSnapshotEntries==='function'?metaSnapshotEntries():null)||projectFilesCatalog[projectFileState.project]?.files||[];}
function currentProjectFile(){return projectEntries().find(f=>f.path===projectFileView().selected);}
function openProjectFiles(){
 projectFileState.project=state.project;projectFileView().mobilePreview=false;
 $('#project-files-dialog').classList.toggle('project-phone-preview',$('#app').classList.contains('phone-preview'));
 renderProjectFiles();$('#project-files-dialog').showModal();$('#project-files-toggle').setAttribute('aria-expanded','true');
}
function projectTree(files,prefix=''){
 const folders=new Map(),leaves=[];
 for(const file of files){const tail=file.path.slice(prefix.length),slash=tail.indexOf('/');if(slash<0)leaves.push(file);else{const dir=prefix+tail.slice(0,slash+1);if(!folders.has(dir))folders.set(dir,[]);folders.get(dir).push(file);}}
 const view=projectFileView();
 return `<ul>${[...folders].sort(([a],[b])=>a.localeCompare(b)).map(([dir,children])=>{const open=!!view.query||!view.collapsed.has(dir);return `<li><button class="project-directory" data-project-dir="${escapeHTML(dir)}" aria-expanded="${open}">${icon(open?'chevron-down':'chevron-right')}${icon('folder')}<span>${escapeHTML(dir.slice(prefix.length,-1))}</span><small>${children.length}</small></button>${open?projectTree(children,dir):''}</li>`;}).join('')}${leaves.sort((a,b)=>a.path.localeCompare(b.path)).map(file=>`<li><button class="project-file-row ${view.selected===file.path?'selected':''}" data-project-file="${escapeHTML(file.path)}" ${view.selected===file.path?'aria-current="true"':''} title="${escapeHTML(file.path)}">${icon(file.kind==='image'?'image':'file')}<span>${escapeHTML(file.path.slice(prefix.length))}</span></button></li>`).join('')}</ul>`;
}
function projectMarkdownInline(text){
 const tokens=/`([^`]+)`|\*\*([^*]+)\*\*|\[([^\]]+)\]\(([^\s)]+)\)/g;let result='',last=0;
 for(const token of text.matchAll(tokens)){
  result+=escapeHTML(text.slice(last,token.index));last=token.index+token[0].length;
  if(token[1])result+='<code>'+escapeHTML(token[1])+'</code>';
  else if(token[2])result+='<strong>'+escapeHTML(token[2])+'</strong>';
  else{
   let path='';try{const url=new URL(token[4],'https://snapshot.invalid/'+currentProjectFile().path);if(url.origin==='https://snapshot.invalid')path=decodeURIComponent(url.pathname.slice(1));}catch{}
   result+=projectEntries().some(file=>file.path===path)?`<button class="project-md-link" data-project-file="${escapeHTML(path)}">${escapeHTML(token[3])}</button>`:`<span>${escapeHTML(token[3])} <small>(${escapeHTML(token[4])})</small></span>`;
  }
 }
 return result+escapeHTML(text.slice(last));
}
function projectMarkdown(text){
 const rows=text.split('\n');let code=false;return rows.map(line=>{
  if(line.startsWith('```')){code=!code;return code?'<pre><code>':'</code></pre>';}
  const safe=escapeHTML(line);if(code)return safe+'\n';
  const heading=/^(#{1,6})\s+(.*)$/.exec(line);if(heading)return `<h${heading[1].length}>${projectMarkdownInline(heading[2])}</h${heading[1].length}>`;
  if(/^[-*] /.test(line))return `<div class="project-md-item">• ${projectMarkdownInline(line.slice(2))}</div>`;
  return safe?`<p>${projectMarkdownInline(line)}</p>`:'<div class="project-md-space"></div>';
 }).join('')+(code?'</code></pre>':'');
}
function renderProjectFilePreview(){
 const file=currentProjectFile(),view=projectFileView(),target=$('#project-file-preview');if(!target)return;
 if(!file){target.innerHTML='<div class="project-preview-empty">'+icon('file')+'<h3>选择一个项目文件</h3><p>在目录中浏览，或按文件名和路径搜索。</p></div>';return;}
 const previewable=['markdown','html','image'].includes(file.kind);
 target.innerHTML=`<header class="project-preview-heading"><button class="project-back" data-project-action="back">${icon('chevron-right')}文件</button><div><strong>${escapeHTML(file.path.split('/').pop())}</strong><small>${escapeHTML(file.path)}</small></div><button class="icon-button" data-project-action="copy" aria-label="复制文件路径">${icon('paperclip')}</button></header><div class="project-preview-tools"><div class="project-preview-modes" role="group" aria-label="文件预览方式">${previewable?`<button data-project-mode="preview" aria-pressed="${view.mode==='preview'}">预览</button>`:''}<button data-project-mode="source" aria-pressed="${!previewable||view.mode==='source'}">${file.kind==='image'?'文件信息':'源码'}</button></div><span>${formatBytes(file.size)} · 只读</span><button data-project-action="quote">${icon('message')}引用到对话</button></div>${file.truncated?'<div class="project-file-warning">大文件快照已截断，仅展示前 24 KiB。</div>':''}<div class="project-preview-content ${view.wrap?'wrap-lines':''}"></div>`;
 const content=$('.project-preview-content');
 if(file.kind==='image')content.innerHTML=view.mode==='source'?`<pre class="project-file-info">${escapeHTML(file.path)}\n${formatBytes(file.size)}\nSVG 图片 · 只读快照</pre>`:`<div class="project-image-preview"><img src="${escapeHTML(file.src)}" alt="${escapeHTML(file.path)}"></div>`;
 else if(previewable&&view.mode==='preview'){
  if(file.kind==='markdown')content.innerHTML=`<article class="project-markdown">${projectMarkdown(file.text)}</article>`;
  else{
   content.innerHTML='<div class="project-html-note">静态 HTML · 不执行脚本，不加载外部资源</div><iframe class="project-html-preview" sandbox title="HTML 文件静态预览" referrerpolicy="no-referrer"></iframe>';
   $('.project-html-preview').srcdoc=`<!doctype html><meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'none'; style-src 'unsafe-inline'; img-src data:; form-action 'none'; base-uri 'none'">`+file.text;
  }
 }else content.innerHTML=`<div class="project-code-toolbar"><span>文本内容 · ${file.text.split('\n').length} 行</span><button data-project-action="wrap" aria-pressed="${view.wrap}">自动换行</button></div><pre class="project-code">${file.text.split('\n').map((line,i)=>`<span class="project-code-line"><i>${i+1}</i><code>${escapeHTML(line)||' '}</code></span>`).join('')}</pre>`;
}
function renderProjectTree(){
 const view=projectFileView(),files=projectEntries().filter(file=>file.path.toLocaleLowerCase().includes(view.query.trim().toLocaleLowerCase()));
 $('#project-file-tree').innerHTML=files.length?projectTree(files):'<div class="project-no-results">没有匹配的文件。<button data-project-action="clear">清除搜索</button></div>';
 $('#project-files-count').textContent=`${files.length} / ${projectEntries().length} 个文件`;
}
function renderProjectFiles(){
 const view=projectFileView();
 $('#project-files-dialog').innerHTML=`<header class="project-files-header"><div><h2 id="project-files-heading">项目文件</h2><p>${escapeHTML(projectLabel(projectFileState.project))} · 文件快照 · 只读</p></div><button class="icon-button" data-project-action="close" aria-label="关闭项目文件">${icon('x')}</button></header><div class="project-files-layout ${view.mobilePreview?'show-project-preview':''}"><aside class="project-file-browser" aria-label="项目文件目录"><label class="project-file-search">${icon('search')}<input id="project-file-search" aria-label="搜索项目文件" placeholder="搜索文件名或路径…" value="${escapeHTML(view.query)}"></label><div class="project-file-caption"><span id="project-files-count"></span><button data-project-action="collapse">折叠目录</button></div><nav id="project-file-tree" aria-label="文件和文件夹"></nav><div class="project-snapshot-note">已收录文件的静态快照；正式版由 PC 提供目录和内容。</div></aside><section id="project-file-preview" class="project-file-preview" aria-label="项目文件预览"></section></div>`;
 renderProjectTree();renderProjectFilePreview();
}
document.addEventListener('click',event=>{
 const button=event.target.closest('button');if(!button||button.disabled)return;
 if(button.id==='project-files-toggle'){openProjectFiles();return;}
 if(!button.closest('#project-files-dialog'))return;
 const view=projectFileView();
 if(button.dataset.projectDir){const dir=button.dataset.projectDir;view.collapsed.has(dir)?view.collapsed.delete(dir):view.collapsed.add(dir);renderProjectTree();return;}
 if(button.dataset.projectFile){view.selected=button.dataset.projectFile;view.mode='preview';view.mobilePreview=true;$('.project-files-layout').classList.add('show-project-preview');renderProjectTree();renderProjectFilePreview();$('.project-back').focus();return;}
 if(button.dataset.projectMode){view.mode=button.dataset.projectMode;renderProjectFilePreview();return;}
 switch(button.dataset.projectAction){
  case 'close':$('#project-files-dialog').close();break;
  case 'back':view.mobilePreview=false;$('.project-files-layout').classList.remove('show-project-preview');$('[data-project-file][aria-current="true"]')?.focus();break;
  case 'clear':view.query='';$('#project-file-search').value='';renderProjectTree();$('#project-file-search').focus();break;
  case 'collapse':projectEntries().forEach(file=>{const dirs=file.path.split('/');dirs.pop();while(dirs.length){view.collapsed.add(dirs.join('/')+'/');dirs.pop();}});renderProjectTree();break;
  case 'wrap':view.wrap=!view.wrap;renderProjectFilePreview();break;
  case 'copy':navigator.clipboard?.writeText(currentProjectFile().path).then(()=>toast('已复制文件路径')).catch(()=>toast(currentProjectFile().path));break;
  case 'quote':{const reference=`@${projectFileState.project}/${currentProjectFile().path}`;$('#prompt').value+=($('#prompt').value?'\n':'')+reference;$('#project-files-dialog').close();$('#prompt').focus();toast('已将文件路径加入草稿，尚未发送');break;}
 }
});
document.addEventListener('input',event=>{if(event.target.id==='project-file-search'){projectFileView().query=event.target.value;renderProjectTree();}});
$('#project-files-dialog').addEventListener('close',()=>{$('#project-files-toggle').setAttribute('aria-expanded','false');});
$('#project-files-dialog').addEventListener('keydown',event=>{if(event.key==='Escape')event.stopPropagation();});
document.addEventListener('DOMContentLoaded',()=>{
 const resources=$('#session-resources'),group=document.createElement('div');group.className='workspace-toolbar-actions';resources.before(group);group.innerHTML=`<button id="project-files-toggle" class="project-files-toggle" aria-label="浏览项目文件与预览" aria-haspopup="dialog" aria-controls="project-files-dialog" aria-expanded="false" title="项目文件与预览">${icon('folder')}<span>项目文件</span></button>`;group.append(resources);
});
