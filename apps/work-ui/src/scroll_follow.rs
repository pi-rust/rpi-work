use dioxus::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};

pub const CSS: &str = include_str!("scroll_follow.css");
static NEXT_INSTANCE: AtomicU64 = AtomicU64::new(1);

#[component]
pub fn ScrollFollow(
    scope: String,
    #[props(default = ".transcript".to_owned())] selector: String,
) -> Element {
    let instance = use_hook(|| NEXT_INSTANCE.fetch_add(1, Ordering::Relaxed));
    let id = format!("rpi-scroll-follow-{instance}");
    let unread = use_signal(|| false);
    let mut eval_task = use_signal(|| None::<dioxus::core::Task>);

    let effect_id = id.clone();
    let effect_scope = scope.clone();
    let effect_selector = selector.clone();
    let mut effect_unread = unread;
    use_effect(use_reactive(
        (&effect_scope, &effect_selector),
        move |(scope, selector)| {
            if let Some(task) = eval_task.write().take() {
                task.cancel();
            }
            let script = install_script(&effect_id, &scope, &selector);
            let task = spawn(async move {
                let mut evaluator = document::eval(&script);
                while let Ok(value) = evaluator.recv::<bool>().await {
                    effect_unread.set(value);
                }
            });
            eval_task.set(Some(task));
        },
    ));

    let cleanup_id = id.clone();
    use_drop(move || {
        if let Some(task) = eval_task.write().take() {
            task.cancel();
        }
        let id = serde_json::to_string(&cleanup_id).unwrap_or_else(|_| "\"\"".to_owned());
        let script = format!(
            "try{{const s=window.__rpiScrollFollow?.instances?.get({id});if(s)s.dispose();}}catch(_){{}}"
        );
        // `eval` starts synchronously; do not put the critical dispose call in a
        // task that can be cancelled as the component is being dropped.
        let _ = document::eval(&script);
    });

    let click_id = id.clone();
    rsx! {
        style { {CSS} }
        div { class: "scroll-follow", id: "{id}", aria_live: "off",
            if *unread.read() {
                button {
                    class: "scroll-follow-button",
                    r#type: "button",
                    aria_label: "回到最新消息",
                    onclick: move |_| {
                        let id = serde_json::to_string(&click_id).unwrap_or_else(|_| "\"\"".to_owned());
                        let script = format!("try{{window.__rpiScrollFollow?.instances?.get({id})?.goToLatest();}}catch(_){{}}dioxus.send(true);");
                        spawn(async move { let _ = document::eval(&script).recv::<bool>().await; });
                    },
                    "有新进展 · 回到底部"
                }
            }
        }
    }
}

fn install_script(id: &str, scope: &str, selector: &str) -> String {
    let id = serde_json::to_string(id).unwrap_or_else(|_| "\"\"".to_owned());
    let scope = serde_json::to_string(scope).unwrap_or_else(|_| "\"\"".to_owned());
    let selector = serde_json::to_string(selector).unwrap_or_else(|_| "\".transcript\"".to_owned());
    format!(
        r"(()=>{{
const api=window.__rpiScrollFollow||(window.__rpiScrollFollow={{records:new Map(),instances:new Map()}});
const id={id},scope={scope},selector={selector};
const previous=api.instances.get(id);if(previous)previous.dispose();
let container=null,mo=null,ro=null,raf=0,active=true,initialized=false,following=true,unread=false,lastHeight=0,restorePending=false;
const record=api.records.get(scope);
const send=()=>{{if(active)try{{dioxus.send(unread);}}catch(_){{}}}};
const nearBottom=()=>container&&container.scrollHeight-container.clientHeight-container.scrollTop<=48;
const save=()=>{{if(container&&container.getAttribute('data-session-scope')===scope)api.records.set(scope,{{top:container.scrollTop,following}});}};
const positionButton=()=>{{const root=document.getElementById(id),button=root?.querySelector('button');if(!container||!button)return;const r=container.getBoundingClientRect();button.style.left=Math.max(8,Math.min(innerWidth-8,r.left+r.width/2))+'px';button.style.top=Math.max(8,Math.min(innerHeight-8,r.bottom-12))+'px';}};
const update=()=>{{raf=0;if(!active)return;const found=document.querySelector(selector);if(found!==container){{bind(found);return;}}if(!container||!container.isConnected)return;
if(container.getAttribute('data-session-scope')&&container.getAttribute('data-session-scope')!==scope){{bind(null);return;}}
const height=container.scrollHeight;
if(!initialized){{if(restorePending){{const target=record?.top||0;const max=Math.max(0,height-container.clientHeight);container.scrollTop=Math.min(target,max);if(max>=target-1)restorePending=false;}}else if(!record)container.scrollTop=container.scrollHeight;
if(height>0&&!restorePending){{initialized=true;following=record?!!record.following:true;if(following)container.scrollTop=container.scrollHeight;lastHeight=height;api.records.set(scope,{{top:container.scrollTop,following}});}}}}
else if(height>lastHeight){{if(following)container.scrollTop=container.scrollHeight;else if(!unread){{unread=true;send();}}}}
lastHeight=height;positionButton();}};
const schedule=()=>{{if(!raf&&active)raf=requestAnimationFrame(update);}};
const onScroll=()=>{{if(!active||!container)return;const atBottom=!!nearBottom();following=atBottom;if(atBottom&&unread){{unread=false;send();}}save();positionButton();}};
const bind=(next)=>{{if(container){{container.removeEventListener('scroll',onScroll);if(ro)ro.disconnect();}}container=next;if(!container)return;
const ownScope=container.getAttribute('data-session-scope');if(ownScope&&ownScope!==scope){{container=null;return;}}if(!ownScope)container.setAttribute('data-session-scope',scope);
following=record?!!record.following:true;restorePending=!!record;initialized=false;lastHeight=0;unread=false;container.addEventListener('scroll',onScroll,{{passive:true}});ro=new ResizeObserver(schedule);ro.observe(container);schedule();}};
const onViewport=()=>{{schedule();positionButton();}};
const onClick=()=>{{if(!container)return;following=true;unread=false;container.scrollTo({{top:container.scrollHeight,behavior:'smooth'}});save();send();}};
const clickHandler=(event)=>{{const button=document.getElementById(id)?.querySelector('button');if(button&&button.contains(event.target))onClick();}};
const instance={{dispose(){{if(!active)return;active=false;if(raf)cancelAnimationFrame(raf);if(mo)mo.disconnect();if(ro)ro.disconnect();if(container)container.removeEventListener('scroll',onScroll);removeEventListener('resize',onViewport);visualViewport?.removeEventListener('resize',onViewport);document.removeEventListener('click',clickHandler,true);save();api.instances.delete(id);}},goToLatest:onClick}};
api.instances.set(id,instance);
mo=new MutationObserver(schedule);mo.observe(document.documentElement,{{childList:true,subtree:true,characterData:true,attributes:true,attributeFilter:['data-session-scope','hidden','style','class']}});
addEventListener('resize',onViewport,{{passive:true}});visualViewport?.addEventListener('resize',onViewport,{{passive:true}});document.addEventListener('click',clickHandler,true);
bind(document.querySelector(selector));
}})();"
    )
}
