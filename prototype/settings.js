/* Workspace-wide configuration is separate from project and conversation navigation. */
function renderSettingsNavigation(page){
 $('#settings-navigation')?.remove();
 $('#page-view').insertAdjacentHTML('afterbegin',`<div class="settings-navigation" id="settings-navigation"><nav aria-label="设置分类">${[['settings','settings','通用'],['devices','monitor','执行节点'],['providers','server','Providers']].map(([id,glyph,title])=>`<button data-settings-page="${id}" ${page===id?'aria-current="page"':''}>${icon(glyph)}${title}</button>`).join('')}</nav><button data-nav="workspace" class="settings-return">${icon('chevron-right')}返回会话</button></div>`);
}
function renderSettings(){
 $('#page-view').innerHTML=`<div class="page-heading"><div><h1>设置</h1><p>管理工作空间偏好、执行节点与模型连接。</p></div></div><div class="settings-cards"><button class="settings-card" data-settings-action="appearance"><span class="settings-card-icon">${icon('sparkles')}</span><strong>外观与主题</strong><p>主题风格、明暗模式与界面密度。</p><small>本设备</small>${icon('chevron-right')}</button><button class="settings-card" data-settings-page="devices"><span class="settings-card-icon">${icon('monitor')}</span><strong>执行节点</strong><p>管理运行 Agent 的电脑、手机配对与设备授权。</p><small>电脑与手机连接</small>${icon('chevron-right')}</button><button class="settings-card" data-settings-page="providers"><span class="settings-card-icon">${icon('server')}</span><strong>Providers</strong><p>管理模型服务连接、认证与可用模型。</p><small>连接配置</small>${icon('chevron-right')}</button></div>`;
 renderSettingsNavigation('settings');
}
document.addEventListener('click',event=>{
 const button=event.target.closest('button');if(!button||button.disabled)return;
 if(button.dataset.settingsPage)navigate(button.dataset.settingsPage);
 if(button.dataset.settingsAction==='appearance')appearanceSettingsModal();
});
