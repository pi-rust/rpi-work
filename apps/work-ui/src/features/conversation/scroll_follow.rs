use dioxus::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};

pub const CSS: &str = include_str!("scroll_follow.css");
static NEXT_INSTANCE: AtomicU64 = AtomicU64::new(1);

pub fn follow_output(scope: &str) {
    let literal = serde_json::to_string(scope).unwrap();
    let _ = document::eval(&format!(
        "document.dispatchEvent(new CustomEvent('rpi-follow-output',{{detail:{literal}}}));"
    ));
}

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
                    "↓ 最新消息"
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
let container=null,mo=null,ro=null,raf=0,active=true,initialized=false,following=true,unread=false,lastHeight=0,lastVisible=0,restorePending=false,intentUntil=0,interacting=false,anchor=null,observed=new WeakSet();
const record=api.records.get(scope);
const send=()=>{{if(active)try{{dioxus.send(unread);}}catch(_){{}}}};
const nearBottom=()=>container&&container.scrollHeight-container.clientHeight-container.scrollTop<=48;
// Near-bottom is a passive displacement tolerance, not proof of a pinned viewport.
const isAtBottom=()=>container&&container.scrollHeight-container.clientHeight-container.scrollTop<=1;
const save=()=>{{if(container&&container.getAttribute('data-session-scope')===scope)api.records.set(scope,{{top:container.scrollTop,following}});}};
const positionButton=()=>{{const root=document.getElementById(id),button=root?.querySelector('button');if(!container||!button)return;const r=container.getBoundingClientRect();button.style.left=Math.max(8,Math.min(innerWidth-8,r.left+r.width/2))+'px';button.style.top=Math.max(8,Math.min(innerHeight-8,r.bottom-12))+'px';}};
const clearAnchor=()=>{{if(anchor){{if(container)container.style.overflowAnchor=anchor.overflowAnchor;anchor=null;}}}};
const preserveReadingAnchor=()=>{{if(!container||!initialized||interacting||(following&&nearBottom()))return;clearAnchor();const r=container.getBoundingClientRect();const node=[...container.querySelectorAll('[data-history-id]')].find(e=>{{const b=e.getBoundingClientRect();return b.bottom>r.top&&b.top<r.bottom;}});if(!node)return;anchor={{node,offset:node.getBoundingClientRect().top-r.top,overflowAnchor:container.style.overflowAnchor}};container.style.overflowAnchor='none';following=false;intentUntil=0;save();}};
const update=()=>{{raf=0;if(!active)return;const found=document.querySelector(selector);if(found!==container){{bind(found);return;}}if(!container||!container.isConnected||container.clientHeight===0)return;
if(container.getAttribute('data-session-scope')&&container.getAttribute('data-session-scope')!==scope){{bind(null);return;}}
const height=container.scrollHeight;
if(anchor){{if(anchor.node.isConnected&&container.contains(anchor.node)){{const delta=anchor.node.getBoundingClientRect().top-container.getBoundingClientRect().top-anchor.offset;if(Math.abs(delta)>.5)container.scrollTop+=delta;save();}}else clearAnchor();}}
for(const child of container.children)if(ro&&!observed.has(child)){{observed.add(child);ro.observe(child);}}
if(!initialized){{if(restorePending){{const target=record?.top||0;const max=Math.max(0,height-container.clientHeight);container.scrollTop=Math.min(target,max);if(max>=target-1)restorePending=false;}}else if(!record)container.scrollTop=container.scrollHeight;
if(height>0&&!restorePending){{initialized=true;following=record?!!record.following:true;if(following)container.scrollTop=container.scrollHeight;else{{unread=true;send();}}lastHeight=height;api.records.set(scope,{{top:container.scrollTop,following}});}}}}
else if(height>lastHeight&&!following&&!unread){{unread=true;send();}}
// Resize and collapsed tools can change the scroll range without a scroll event.
// Keep an active follower pinned, and never offer a jump when nothing is hidden.
if(initialized&&!restorePending){{if(following)container.scrollTop=container.scrollHeight;if(isAtBottom()){{clearAnchor();following=true;if(unread){{unread=false;send();}}save();}}}}
lastHeight=height;lastVisible=container.clientHeight;positionButton();}};
const schedule=()=>{{if(!raf&&active)raf=requestAnimationFrame(update);}};
const onIntent=()=>{{clearAnchor();intentUntil=performance.now()+700;}};
const onKey=event=>{{if(['PageUp','PageDown','Home','End','ArrowUp','ArrowDown',' '].includes(event.key))onIntent();}};
const onPointer=()=>{{interacting=true;onIntent();}};
const onRelease=()=>{{if(interacting){{interacting=false;onIntent();}}}};
const onScroll=event=>{{if(!active||!container||!initialized)return;if(anchor){{schedule();return;}}const atBottom=!!isAtBottom();if(atBottom)intentUntil=0;
// Native focus restoration can move a follower a few pixels with no size change.
// Re-pin passive near-bottom movement; wheel/touch/key/drag intent must still pause.
if(!atBottom&&following&&event.isTrusted&&!interacting&&performance.now()>intentUntil&&(nearBottom()||container.scrollHeight!==lastHeight||container.clientHeight!==lastVisible)){{schedule();return;}}
following=atBottom;if(unread===atBottom){{unread=!atBottom;send();}}save();positionButton();}};
const bind=(next)=>{{clearAnchor();if(container){{container.removeEventListener('scroll',onScroll);container.removeEventListener('wheel',onIntent);container.removeEventListener('touchmove',onIntent);container.removeEventListener('keydown',onKey);container.removeEventListener('pointerdown',onPointer);if(ro)ro.disconnect();}}container=next;if(!container)return;
const ownScope=container.getAttribute('data-session-scope');if(ownScope&&ownScope!==scope){{container=null;return;}}if(!ownScope)container.setAttribute('data-session-scope',scope);
following=record?!!record.following:true;restorePending=!!record;initialized=false;lastHeight=0;lastVisible=0;unread=false;observed=new WeakSet();container.addEventListener('scroll',onScroll,{{passive:true}});container.addEventListener('wheel',onIntent,{{passive:true}});container.addEventListener('touchmove',onIntent,{{passive:true}});container.addEventListener('keydown',onKey);container.addEventListener('pointerdown',onPointer);ro=new ResizeObserver(schedule);ro.observe(container);schedule();}};
const onViewport=()=>{{schedule();positionButton();}};
// One explicit jump, not two competing smooth animations through capture + Rust.
const onClick=()=>{{if(!container||!container.isConnected||container.clientHeight===0)return;clearAnchor();following=true;intentUntil=0;interacting=false;container.scrollTo({{top:container.scrollHeight,behavior:'instant'}});unread=false;save();send();schedule();}};
const onOutput=event=>{{if(event.detail!==scope||!container)return;clearAnchor();following=true;intentUntil=0;unread=false;container.scrollTop=container.scrollHeight;save();send();schedule();}};
// User-input navigation is a scoped reading action, never a new output/follow request.
const navigateTo=target=>{{if(!active||!container?.isConnected||container.clientHeight===0||container.getAttribute('data-session-scope')!==scope||!(target instanceof HTMLElement)||!container.contains(target)||!target.hasAttribute('data-user-message-key'))return false;clearAnchor();restorePending=false;initialized=true;following=false;interacting=false;intentUntil=performance.now()+700;container.scrollTop+=target.getBoundingClientRect().top-container.getBoundingClientRect().top-12;lastHeight=container.scrollHeight;lastVisible=container.clientHeight;unread=!isAtBottom();save();send();positionButton();schedule();return true;}};
const instance={{scope,navigateTo,preserveReadingAnchor,dispose(){{if(!active)return;clearAnchor();active=false;if(raf)cancelAnimationFrame(raf);if(mo)mo.disconnect();if(ro)ro.disconnect();if(container){{container.removeEventListener('scroll',onScroll);container.removeEventListener('wheel',onIntent);container.removeEventListener('touchmove',onIntent);container.removeEventListener('keydown',onKey);container.removeEventListener('pointerdown',onPointer);}}removeEventListener('resize',onViewport);removeEventListener('pointerup',onRelease);removeEventListener('pointercancel',onRelease);visualViewport?.removeEventListener('resize',onViewport);document.removeEventListener('rpi-follow-output',onOutput);save();api.instances.delete(id);}},goToLatest:onClick}};
api.instances.set(id,instance);
mo=new MutationObserver(changes=>{{if(!container||changes.some(change=>change.target===container||container.contains(change.target)||change.target===document.getElementById(id)))schedule();}});mo.observe(document.documentElement,{{childList:true,subtree:true,characterData:true,attributes:true,attributeFilter:['data-session-scope','hidden','style','class','open']}});
addEventListener('resize',onViewport,{{passive:true}});visualViewport?.addEventListener('resize',onViewport,{{passive:true}});
document.addEventListener('rpi-follow-output',onOutput);
addEventListener('pointerup',onRelease);addEventListener('pointercancel',onRelease);
bind(document.querySelector(selector));
}})();"
    )
}
