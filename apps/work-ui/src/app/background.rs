use dioxus::{core::Task, prelude::*};
use work_client::{QueueSnapshot, WorkspaceClient};
/// One request at a time; visibility and scope lifetime own the timer.
#[component]
pub(crate) fn BtwHost(onclose: EventHandler, children: Element) -> Element {
    rsx! {
        dialog {
            id: "btw-overlay",
            class: "app-dialog btw-overlay",
            aria_label: "BTW 独立旁问",
            onmounted: move |_| {
                spawn(async move {
                    let script = r"
                        const dialog = document.getElementById('btw-overlay');
                        if (!dialog) return;
                        const media = matchMedia('(max-width: 1040px)');
                        let ignoredCloses = 0;
                        const sync = () => {
                            if (!dialog.isConnected) return;
                            if (dialog.open) { ignoredCloses++; dialog.close(); }
                            if (media.matches) dialog.showModal(); else dialog.show();
                        };
                        const closed = () => {
                            if (ignoredCloses) { ignoredCloses--; return; }
                            dioxus.send(true);
                        };
                        dialog.addEventListener('close', closed);
                        media.addEventListener('change', sync);
                        sync();
                        const observer = new MutationObserver(() => {
                            if (!dialog.isConnected) {
                                observer.disconnect();
                                media.removeEventListener('change', sync);
                                dialog.removeEventListener('close', closed);
                                dioxus.send(false);
                            }
                        });
                        observer.observe(document.body, {childList:true, subtree:true});
                    ";
                    if let Ok(true) = document::eval(script).recv::<bool>().await { onclose.call(()); }
                });
            },
            button { class: "btw-scrim", r#type: "button", aria_label: "关闭旁问遮罩", onclick: move |_| onclose.call(()) }
            {children}
        }
    }
}

#[component]
pub(crate) fn QueueSync(
    session_id: String,
    client: Signal<Option<WorkspaceClient>>,
    session_generation: Signal<u64>,
    mut queue: Signal<Option<QueueSnapshot>>,
) -> Element {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let id = use_hook(|| NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
    let mut task = use_signal(|| None::<Task>);
    use_effect(move || {
        let Some(node) = client.read().clone() else {
            return;
        };
        let generation = *session_generation.peek();
        let session = session_id.clone();
        if let Some(previous) = task.write().take() {
            previous.cancel();
        }
        task.set(Some(spawn(async move {
            let script = format!(r"
                window.__rpiQueueSync ??= new Map();
                window.__rpiQueueSync.get({id})?.dispose();
                let timer = null, pending = false, alive = true;
                const tick = () => {{
                    if (!alive || document.hidden || pending) return;
                    pending = true;
                    dioxus.send(true);
                }};
                const changed = () => {{ clearTimeout(timer); timer = null; if (!document.hidden) tick(); }};
                const state = {{
                    complete() {{
                        pending = false;
                        if (alive && !document.hidden) timer = setTimeout(tick, 3000);
                    }},
                    dispose() {{
                        alive = false; clearTimeout(timer);
                        document.removeEventListener('visibilitychange', changed);
                        if (window.__rpiQueueSync.get({id}) === state) window.__rpiQueueSync.delete({id});
                    }}
                }};
                window.__rpiQueueSync.set({id}, state);
                document.addEventListener('visibilitychange', changed);
                tick();
            ");
            let mut clock = document::eval(&script);
            while clock.recv::<bool>().await.is_ok() {
                let snapshot = node.queue(&session, false).await;
                if *session_generation.peek() != generation { break; }
                if let Ok(snapshot) = snapshot { queue.set(Some(snapshot)); }
                let _ = document::eval(&format!("window.__rpiQueueSync?.get({id})?.complete();"));
            }
        })));
    });
    use_drop(move || {
        let _ = document::eval(&format!("window.__rpiQueueSync?.get({id})?.dispose();"));
        if let Some(task) = task.write().take() {
            task.cancel();
        }
    });
    rsx! {}
}
