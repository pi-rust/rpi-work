use crate::shared::components::{Icon, Modal};
use dioxus::prelude::*;
pub const CSS: &str = include_str!("management.css");

#[cfg(test)]
#[path = "management_tests.rs"]
mod tests;

pub fn node_error(error: work_client::ClientError) -> String {
    match error {
        work_client::ClientError::Api { status, .. } if matches!(status.as_u16(), 404 | 405) => {
            "节点未提供此操作或目标已不存在，请刷新列表；旧版 rpi-http 需要更新后使用此功能。"
                .into()
        }
        work_client::ClientError::Api { status, .. } if status.as_u16() == 401 => {
            "节点登录已失效，请重新连接并登录。".into()
        }
        error => error.to_string(),
    }
}

pub fn open_menu(id: &str) {
    let literal = serde_json::to_string(id).unwrap();
    let _ = document::eval(&format!(
        "document.getElementById({literal})?.showPopover();"
    ));
}

#[derive(Clone, PartialEq)]
pub enum SessionAction {
    Rename(String),
    Archive(String, bool),
    Pin(String, bool),
    Fork(String, Option<String>),
    Clone(String),
    Copy(String),
    Details(String),
}

pub fn use_archived_projects() -> Signal<std::collections::HashSet<String>> {
    use_project_set("rpi-work-archived-projects-v1")
}

pub fn use_expanded_projects() -> Signal<std::collections::HashSet<String>> {
    use_project_set("rpi-work-expanded-projects-v1")
}

fn use_project_set(storage_key: &'static str) -> Signal<std::collections::HashSet<String>> {
    let mut archive = use_signal(std::collections::HashSet::<String>::new);
    let mut ready = use_signal(|| false);
    use_effect(move || {
        spawn(async move {
            let script = format!(
                "try{{dioxus.send(JSON.parse(localStorage.getItem('{storage_key}')||'[]'));}}catch(_){{dioxus.send([]);}}"
            );
            if let Ok(value) = document::eval(&script).recv::<Vec<String>>().await {
                // Merge so a project opened before storage finishes loading is retained.
                archive.write().extend(value);
            }
            ready.set(true);
        });
    });
    use_effect(move || {
        let items = archive.read().iter().cloned().collect::<Vec<_>>();
        if !*ready.read() {
            return;
        }
        spawn(async move {
            let value = serde_json::to_string(&items).unwrap_or_else(|_| "[]".into());
            let script = format!(
                "try{{localStorage.setItem('{storage_key}',{});dioxus.send(true);}}catch(_){{dioxus.send(false);}}",
                serde_json::to_string(&value).unwrap()
            );
            let _ = document::eval(&script).recv::<bool>().await;
        });
    });
    archive
}

pub const RECENT_SESSION_LIMIT: usize = 5;

/// Browser time works on web, desktop WebView and mobile alike.
pub fn use_sidebar_clock() -> Signal<u64> {
    let mut now = use_signal(|| 0);
    use_future(move || async move {
        let mut clock = document::eval(
            "clearInterval(window.__rpiSidebarClock);dioxus.send(Date.now());window.__rpiSidebarClock=setInterval(()=>dioxus.send(Date.now()),60000);",
        );
        while let Ok(value) = clock.recv::<u64>().await {
            now.set(value);
        }
    });
    use_drop(|| {
        let _ = document::eval("clearInterval(window.__rpiSidebarClock);");
    });
    now
}

pub fn relative_updated_at(updated_at: u64, now: u64) -> String {
    if updated_at == 0 || now == 0 {
        return String::new();
    }
    let minutes = now.saturating_sub(updated_at) / 60_000;
    match minutes {
        0 => "刚刚".into(),
        1..60 => format!("{minutes}分"),
        60..1440 => format!("{}小时", minutes / 60),
        1440..43200 => format!("{}天", minutes / 1440),
        43200..525600 => format!("{}个月", minutes / 43200),
        _ => format!("{}年", minutes / 525600),
    }
}

pub fn sidebar_session_status(
    session: &work_client::SessionSummary,
) -> Option<(&'static str, &'static str)> {
    if session.error.is_some() || matches!(session.status.as_str(), "error" | "failed") {
        Some(("failed", "执行失败"))
    } else if session.pending_resume
        || matches!(
            session.status.as_str(),
            "waiting" | "waiting_for_input" | "awaiting_approval"
        )
    {
        Some(("waiting", "等待处理"))
    } else if matches!(session.status.as_str(), "running" | "streaming" | "busy") {
        Some(("running", "运行中"))
    } else {
        None
    }
}

/// Keep the selected conversation reachable even when it is older than the preview.
pub fn session_preview_visible(index: usize, expanded: bool, selected: bool) -> bool {
    expanded || index < RECENT_SESSION_LIMIT || selected
}

#[component]
pub fn SidebarSession(
    session: work_client::SessionSummary,
    selected: bool,
    busy: bool,
    now: u64,
    onopen: EventHandler<String>,
    onaction: EventHandler<SessionAction>,
) -> Element {
    let id = session.session_id.clone();
    let title = session.title.clone().unwrap_or_else(|| "未命名会话".into());
    let status = sidebar_session_status(&session);
    let description =
        status.map_or_else(|| title.clone(), |(_, label)| format!("{title} · {label}"));
    let menu = format!("session-menu-{id}");
    let updated = relative_updated_at(session.updated_at, now);
    rsx! {
        div { class: if selected { "session-entry selected" } else { "session-entry" }, "data-session-id": "{id}",
            button {
                class: if selected { "session-item current" } else { "session-item" },
                aria_label: "{title}", aria_current: if selected { "true" } else { "false" }, title: "{description}",
                onclick: { let id=id.clone(); move |_| onopen.call(id.clone()) },
                oncontextmenu: move |event| { event.prevent_default(); open_menu(&menu); },
                if let Some((tone, label)) = status {
                    span { class: "session-dot {tone}", title: "{label}", "aria-hidden": "true" }
                }
                span { class: "session-title", "{title}" }
                if session.pinned { small { class: "session-pin", "置顶" } }
                small { class: "session-updated", "aria-hidden": "true", "{updated}" }
            }
            SessionMenu { id: id.clone(), title: title.clone(), archived: session.archived, pinned: session.pinned, busy, onaction }
        }
    }
}

pub fn use_menu_behaviour() {
    use_effect(|| {
        let _ = document::eval(
            r#"
        window.__rpiMenuDispose?.();
        const prepare=e=>{if(e.target.matches?.('.workspace-menu')&&e.newState==='open'){e.target.style.left='8px';e.target.style.top='8px';}};
        const position=menu=>{
            const trigger=document.querySelector('[popovertarget="'+CSS.escape(menu.id)+'"]');
            if(!trigger)return;const r=trigger.getBoundingClientRect(),m=menu.getBoundingClientRect();
            menu.style.left=Math.max(8,Math.min(innerWidth-m.width-8,r.right-m.width))+'px';
            menu.style.top=Math.max(8,Math.min(innerHeight-m.height-8,r.bottom+4))+'px';
        };
        const opened=e=>{if(!e.target.matches?.('.workspace-menu')||e.newState!=='open')return;position(e.target);e.target.querySelector('button:not(:disabled)')?.focus({preventScroll:true});};
        const resized=()=>document.querySelectorAll('.workspace-menu:popover-open').forEach(position);
        const click=e=>{const menu=e.target.closest?.('.workspace-menu');if(menu&&e.target.closest('button'))menu.hidePopover();};
        const key=e=>{const menu=e.target.closest?.('.workspace-menu')||(e.key==='Escape'?document.querySelector('.workspace-menu:popover-open'):null);if(!menu)return;
            if(e.key==='Escape'){e.preventDefault();e.stopImmediatePropagation();menu.hidePopover();return;}
            const items=[...menu.querySelectorAll('button:not(:disabled)')],i=items.indexOf(document.activeElement);
            let next;if(e.key==='ArrowDown')next=(i+1)%items.length;else if(e.key==='ArrowUp')next=(i+items.length-1)%items.length;else if(e.key==='Home')next=0;else if(e.key==='End')next=items.length-1;
            if(next!==undefined){e.preventDefault();items[next]?.focus();}
        };
        document.addEventListener('beforetoggle',prepare,true);document.addEventListener('toggle',opened,true);document.addEventListener('click',click,true);window.addEventListener('keydown',key,true);addEventListener('resize',resized);
        window.__rpiMenuDispose=()=>{document.removeEventListener('beforetoggle',prepare,true);document.removeEventListener('toggle',opened,true);document.removeEventListener('click',click,true);window.removeEventListener('keydown',key,true);removeEventListener('resize',resized);};
    "#,
        );
    });
    use_drop(|| {
        let _ = document::eval("window.__rpiMenuDispose?.();");
    });
}

#[component]
pub fn SessionMenu(
    id: String,
    title: String,
    archived: bool,
    pinned: bool,
    busy: bool,
    #[props(default = "session".to_owned())] prefix: String,
    onaction: EventHandler<SessionAction>,
) -> Element {
    let menu_id = format!("{prefix}-menu-{id}");
    rsx! {
        button { class: "session-row-menu", title: "会话操作", aria_label: "{title} 的会话操作", "popovertarget": "{menu_id}", Icon { name: "more" } }
        div { id: "{menu_id}", class: "workspace-menu", popover: "auto", role: "menu", aria_label: "会话操作",
            onkeydown: move |event| event.stop_propagation(),
            button { role: "menuitem", disabled: busy, onclick: { let id=id.clone(); move |_| onaction.call(SessionAction::Rename(id.clone())) }, "重命名" }
            button { role: "menuitem", disabled: busy, onclick: { let id=id.clone(); move |_| onaction.call(SessionAction::Pin(id.clone(), !pinned)) }, if pinned { "取消置顶" } else { "置顶" } }
            button { role: "menuitem", disabled: busy, onclick: { let id=id.clone(); move |_| onaction.call(SessionAction::Fork(id.clone(), None)) }, "新建分支（Fork）" }
            button { role: "menuitem", disabled: busy, onclick: { let id=id.clone(); move |_| onaction.call(SessionAction::Clone(id.clone())) }, "复制会话（Clone）" }
            hr {}
            button { role: "menuitem", onclick: { let id=id.clone(); move |_| onaction.call(SessionAction::Copy(id.clone())) }, "复制会话标识" }
            button { role: "menuitem", onclick: { let id=id.clone(); move |_| onaction.call(SessionAction::Details(id.clone())) }, "详情与配置" }
            hr {}
            button { role: "menuitem", disabled: busy, onclick: move |_| onaction.call(SessionAction::Archive(id.clone(), !archived)), if archived { "恢复会话" } else { "归档会话" } }
        }
    }
}

#[component]
pub fn RenameDialog(
    name: String,
    busy: bool,
    error: Option<String>,
    onsave: EventHandler<String>,
    onclose: EventHandler<()>,
) -> Element {
    let mut value = use_signal(|| name);
    rsx! { Modal { id: "rename-conversation", title: "重命名会话", class: "rename-dialog", onclose,
        form { onsubmit: move |event| { event.prevent_default(); if !busy && !value.read().trim().is_empty() { onsave.call(value.read().trim().to_owned()); } },
            div { class: "modal-body", label { r#for: "rename-conversation-name", "会话名称" }
                input { id: "rename-conversation-name", autofocus: true, value: "{value}", maxlength: "80", disabled: busy, oninput: move |event| value.set(event.value()) }
                if let Some(error)=error { p { role: "alert", "{error}" } }
            }
            div { class: "modal-actions", button { r#type: "button", disabled: busy, onclick: move |_| onclose.call(()), "取消" }
                button { class: "button primary", r#type: "submit", disabled: busy || value.read().trim().is_empty(), if busy { "正在保存…" } else { "保存" } }
            }
        }
    } }
}
