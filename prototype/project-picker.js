/* Directory-first project registration prototype. A display name never renames a directory. */
const projectPicker={mode:'existing',nameEdited:false,preferredType:'project',folderPath:'',folderQuery:''};
function projectLabel(project){return metaAgent.projects.find(p=>p.project===project)?.name||project;}
function normalizeProjectDirectory(raw){
 let path=raw.trim().replace(/\\/g,'/');if(!path||/[\u0000-\u001f]/.test(path))return null;
 let root,rest,windows=false;
 if(/^[A-Za-z]:\//.test(path)){windows=true;root=path[0].toUpperCase()+':/';rest=path.slice(3);}
 else if(path.startsWith('//')){const parts=path.slice(2).split('/').filter(Boolean);if(parts.length<2)return null;windows=true;root='//'+parts.shift()+'/'+parts.shift()+'/';rest=parts.join('/');}
 else if(path.startsWith('/')){root='/';rest=path.slice(1);}
 else return null;
 const segments=[];for(const part of rest.split('/')){if(!part||part==='.')continue;if(part==='..'){if(!segments.length)return null;segments.pop();continue;}if(windows&&/[<>:"|?*]/.test(part))return null;segments.push(part);}
 if(!segments.length)return null;path=root+segments.join('/');return {path,key:windows?path.toLowerCase():path,basename:segments.at(-1)};
}
function existingProjectAt(path){const target=normalizeProjectDirectory(path);return target?metaAgent.projects.find(project=>normalizeProjectDirectory(project.path)?.key===target.key):null;}
function addProjectForm(){
 Object.assign(projectPicker,{mode:'existing',nameEdited:false,preferredType:'project'});
 showModal('添加项目',`<p class="project-add-intro">打开已有目录，或新建一个项目目录。显示名称可以自定义，不会重命名文件夹。</p><div class="project-add-modes" role="group" aria-label="项目添加方式"><button data-project-form-action="mode" data-value="existing" aria-pressed="true">${icon('folder')}打开已有目录</button><button data-project-form-action="mode" data-value="new" aria-pressed="false">${icon('plus')}新建项目目录</button></div><div class="project-node-caption">${icon('monitor')}这台电脑 <span>执行节点</span></div><div class="field"><label for="project-path" id="project-path-label">已有项目目录</label><div class="project-path-input"><input id="project-path" placeholder="选择文件夹或输入完整目录" maxlength="240" autocomplete="off"><button data-project-form-action="browse">${icon('folder')}浏览…</button></div></div><div class="field"><label for="project-name">显示名称 <small>可自定义</small></label><input id="project-name" placeholder="默认使用目录名" maxlength="50" autocomplete="off"><small class="project-name-help">只影响侧栏中的名称，项目目录保持不变。</small></div><fieldset class="project-purpose"><legend>项目用途</legend><div class="meta-type-picker" role="group" aria-label="项目类型"><label><input type="radio" name="project-type" value="project" checked>${icon('folder')}<span><strong>普通项目</strong><small>日常开发和项目任务</small></span></label><label><input type="radio" name="project-type" value="capability">${icon('sparkles')}<span><strong>元Agent</strong><small>编写 Skill、Package 与扩展</small></span></label></div></fieldset><p id="project-existing-note" class="project-existing-note" hidden></p><p id="project-form-error" class="project-form-error" role="alert"></p><p class="modal-help">原型使用目录样例；不会访问、创建或重命名真实文件夹。</p>`,`<button class="button" data-close-modal>取消</button><button class="button primary" data-meta-action="create-project" data-project-form-action="submit" id="project-open-submit">打开项目</button>`);
 $('#modal').classList.add('project-add-modal');
}
function syncProjectRegistration(){
 const directory=normalizeProjectDirectory($('#project-path').value),existing=directory?existingProjectAt(directory.path):null;
 if(!projectPicker.nameEdited)$('#project-name').value=(existing?.name||directory?.basename||'').slice(0,50);
 $$('input[name="project-type"]').forEach(input=>{input.disabled=!!existing&&projectPicker.mode==='existing';input.checked=input.value===(existing&&projectPicker.mode==='existing'?existing.type:projectPicker.preferredType);});
 $('#project-existing-note').hidden=!existing;
 $('#project-existing-note').textContent=existing?(projectPicker.mode==='existing'?'此目录已添加。打开会复用原项目和会话，并应用这里的显示名称。':'此目录已经有项目，请选择“打开已有目录”。'):'';
 $('#project-form-error').textContent='';
}
function projectFolderCandidates(){return [...new Set([...metaAgent.projects.map(project=>project.path),'D:/Projects/rpi-game-package','D:/Projects/rpi-video'])];}
function openNodeFolderPicker(){
 projectPicker.folderPath=normalizeProjectDirectory($('#project-path').value)?.path||activeProjectProfile().path;projectPicker.folderQuery='';
 $('#project-folder-dialog').innerHTML=`<header class="project-folder-header"><div><h2 id="project-folder-heading">选择项目文件夹</h2><p>这台电脑 · 目录选择演示</p></div><button class="icon-button" data-project-folder-action="cancel" aria-label="关闭目录选择">${icon('x')}</button></header><div class="project-folder-body"><label class="project-folder-search">${icon('search')}<input id="project-folder-search" aria-label="筛选目录样例" placeholder="搜索目录名称或路径…"></label><div id="project-folder-list" class="project-folder-list" aria-label="目录样例"></div><div class="project-folder-selection"><small>选中的目录</small><code id="project-folder-selection"></code></div></div><footer class="project-folder-footer"><button class="button" data-project-folder-action="cancel">取消</button><button class="button primary" data-project-folder-action="choose">使用此目录</button></footer>`;
 renderNodeFolderList();$('#project-folder-dialog').showModal();
}
function renderNodeFolderList(){
 const paths=projectFolderCandidates().filter(path=>path.toLowerCase().includes(projectPicker.folderQuery.toLowerCase()));
 $('#project-folder-list').innerHTML=paths.map(path=>{const existing=existingProjectAt(path);return `<button data-project-folder-path="${escapeHTML(path)}" aria-pressed="${normalizeProjectDirectory(path)?.key===normalizeProjectDirectory(projectPicker.folderPath)?.key}">${icon('folder')}<span><strong>${escapeHTML(normalizeProjectDirectory(path)?.basename||path)}</strong><small>${escapeHTML(path)}</small>${existing?`<em>已添加 · ${escapeHTML(existing.name)}</em>`:''}</span>${icon('chevron-right')}</button>`;}).join('')||'<p class="project-folder-empty">没有匹配的目录样例，可返回后手工输入节点目录。</p>';
 $('#project-folder-selection').textContent=projectPicker.folderPath;
}
function submitProjectRegistration(){
 const error=message=>{$('#project-form-error').textContent=message;};
 if(state.running){error('请先完成或停止当前生成，再打开项目。');return;}
 const directory=normalizeProjectDirectory($('#project-path').value);if(!directory){error('请选择文件夹，或输入有效的完整项目目录。');$('#project-path').focus();return;}
 const name=($('#project-name').value.trim()||directory.basename).slice(0,50),existing=existingProjectAt(directory.path);
 if(existing){if(projectPicker.mode==='new'){error('这个目录已添加，请通过“打开已有目录”进入。');return;}existing.name=name;selectProject(existing.project);return;}
 const type=$('input[name="project-type"]:checked').value,id=type==='capability'&&!metaAgent.projects.some(project=>project.type==='capability')?'meta-personal':'project-'+crypto.randomUUID();
 let project=directory.basename;if(metaAgent.projects.some(profile=>profile.project===project))project+='-'+id.slice(-6);
 metaAgent.projects.push({id,name,type,project,path:directory.path,origin:projectPicker.mode==='existing'?'opened':'created',lastSession:null});selectProject(project);
}
function projectDisplaySettings(){
 const profile=activeProjectProfile();showModal('项目名称与目录',`<div class="field"><label for="project-display-name">显示名称</label><input id="project-display-name" maxlength="50" value="${escapeHTML(profile.name)}"></div><div class="field"><label for="project-display-path">项目目录</label><input id="project-display-path" readonly value="${escapeHTML(profile.path)}"></div><p class="modal-help">保存只修改项目的显示名称，不重命名文件夹，也不改变会话、项目用途或已启用资源。</p>`,`<button class="button" data-close-modal>取消</button><button class="button primary" data-project-form-action="rename">保存显示名称</button>`);
}
document.addEventListener('click',event=>{
 const button=event.target.closest('button');if(!button||button.disabled)return;
 if(button.dataset.projectFormAction==='mode'){
  projectPicker.mode=button.dataset.value;$$('[data-project-form-action="mode"]').forEach(el=>el.setAttribute('aria-pressed',String(el.dataset.value===projectPicker.mode)));
  $('#project-open-submit').textContent=projectPicker.mode==='existing'?'打开项目':'创建项目';$('#project-path-label').textContent=projectPicker.mode==='existing'?'已有项目目录':'新项目目录';syncProjectRegistration();
 }
 if(button.dataset.projectFormAction==='browse')openNodeFolderPicker();
 if(button.dataset.projectFormAction==='submit')submitProjectRegistration();
 if(button.dataset.projectFormAction==='settings')projectDisplaySettings();
 if(button.dataset.projectFormAction==='rename'){
  const name=$('#project-display-name').value.trim();if(!name){$('#project-display-name').focus();return;}activeProjectProfile().name=name;renderSessions();if(!$('#app').classList.contains('settings-mode'))$('#breadcrumb-project').textContent=name;$('#modal').close();toast('已更新显示名称，项目目录保持不变');
 }
 if(button.dataset.projectFolderPath){projectPicker.folderPath=button.dataset.projectFolderPath;renderNodeFolderList();}
 if(button.dataset.projectFolderAction==='cancel')$('#project-folder-dialog').close();
 if(button.dataset.projectFolderAction==='choose'){$('#project-path').value=projectPicker.folderPath;syncProjectRegistration();$('#project-folder-dialog').close();}
});
document.addEventListener('input',event=>{
 if(event.target.id==='project-path')syncProjectRegistration();
 if(event.target.id==='project-name')projectPicker.nameEdited=!!event.target.value.trim();
 if(event.target.id==='project-folder-search'){projectPicker.folderQuery=event.target.value;renderNodeFolderList();}
});
document.addEventListener('change',event=>{if(event.target.name==='project-type')projectPicker.preferredType=event.target.value;});
$('#project-folder-dialog').addEventListener('keydown',event=>{if(event.key==='Escape')event.stopPropagation();});
