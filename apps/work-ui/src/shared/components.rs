use dioxus::prelude::*;

#[component]
pub fn Icon(name: String) -> Element {
    let path = match name.as_str() {
        "menu" => "M4 6h16M4 12h16M4 18h16",
        "search" => "M21 21l-5-5M18 10a8 8 0 1 1-16 0 8 8 0 0 1 16 0",
        "plus" => "M12 5v14M5 12h14",
        "refresh" => "M20 11a8 8 0 1 0 1 4m-1-4v-5m0 5h-5",
        "more" => "M5 12h.01M12 12h.01M19 12h.01",
        "close" => "M6 6l12 12M6 18L18 6",
        "chevron" => "M6 9l6 6 6-6",
        "chevron-right" => "m9 6 6 6-6 6",
        "phone" => {
            "M9 2h6a3 3 0 0 1 3 3v14a3 3 0 0 1-3 3H9a3 3 0 0 1-3-3V5a3 3 0 0 1 3-3ZM10 5h4m-3 14h2"
        }
        "info" => "M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0ZM12 11v6m0-10v1",
        "image" => {
            "M6 3h12a3 3 0 0 1 3 3v12a3 3 0 0 1-3 3H6a3 3 0 0 1-3-3V6a3 3 0 0 1 3-3ZM10 8a2 2 0 1 1-4 0 2 2 0 0 1 4 0Zm11 7-5-5-8 11m-5-4 4-4 4 4"
        }
        "folder" => "M3 7V4h6l3 3h9v13H3V7Z",
        "file" => "M5 3h9l5 5v13H5V3Zm9 0v6h5",
        "panel" => "M3 3h18v18H3V3Zm12 0v18",
        "layout" => "M3 3h18v18H3V3Zm6 0v18m0-12h12",
        "blocks" => "M3 3h7v7H3V3Zm11 0h7v7h-7V3ZM3 14h7v7H3v-7Zm11 0h7v7h-7v-7Z",
        "check" => "M5 12l4 4L19 6",
        "skills" => "M9 5h12M9 12h12M9 19h12M2 5l2 2 3-4M2 12l2 2 3-4M2 19l2 2 3-4",
        "settings" => {
            "M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8ZM9 3h6l1 3 3 1 2 5-2 5-3 1-1 3H9l-1-3-3-1-2-5 2-5 3-1 1-3Z"
        }
        "message" => "M3 4h18v13H8l-5 4V4Z",
        "diff" => "M5 3h14v18H5V3Zm3 5h8m-4-3v6m-4 5h8",
        "arrow" => "M12 20V4m-6 6 6-6 6 6",
        "back" => "M20 12H4m6-6-6 6 6 6",
        "sparkles" => "m12 2 3 7 7 3-7 3-3 7-3-7-7-3 7-3 3-7Z",
        "monitor" => "M3 3h18v14H3V3Zm9 14v4m-5 0h10",
        "server" => "M3 3h18v7H3V3Zm0 11h18v7H3v-7ZM7 6h1m-1 11h1",
        _ => "M5 5h14v14H5V5Zm3 7h8",
    };
    rsx! { svg { class: "icon", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "1.6", stroke_linecap: "round", stroke_linejoin: "round", "aria-hidden": "true", path { d: path } } }
}

/// Native modal provides focus containment and restores the invoking control on close.
#[component]
pub fn Modal(
    id: &'static str,
    title: &'static str,
    #[props(default)] subtitle: Option<String>,
    class: String,
    onclose: EventHandler,
    children: Element,
) -> Element {
    rsx! {
        dialog {
            id, class: "app-dialog {class}", aria_labelledby: "{id}-title",
            onmounted: move |_| {
                spawn(async move {
                    let script = format!(r"
                        const dialog = document.getElementById('{id}');
                        const previous = document.activeElement;
                        const escape = event => {{
                          if (event.key !== 'Escape' || !dialog.open) return;
                          const layers = [...document.querySelectorAll('dialog[open]')];
                          if (layers.at(-1) !== dialog) return;
                          event.preventDefault();
                          event.stopImmediatePropagation();
                          dialog.close();
                        }};
                        const defocusFrame = event => {{
                          if (event.target?.tagName === 'IFRAME') {{ event.target.blur(); dialog.focus(); }}
                        }};
                        const frameListeners = new Map();
                        const wireFrames = () => {{
                          dialog.querySelectorAll('iframe').forEach(frame => {{
                            if (frameListeners.has(frame)) return;
                            const targets = new Set();
                            const wire = () => {{
                              try {{
                                [frame.contentDocument, frame.contentWindow].filter(Boolean).forEach(target => {{
                                  target.addEventListener('keydown', escape, true);
                                  targets.add(target);
                                }});
                              }} catch (_) {{}}
                            }};
                            frame.addEventListener('load', wire);
                            wire();
                            frameListeners.set(frame, {{wire, targets}});
                          }});
                        }};
                        document.addEventListener('keydown', escape, true);
                        document.addEventListener('focusin', defocusFrame, true);
                        dialog.showModal();
                        wireFrames();
                        const focusGuard = setInterval(() => {{
                          wireFrames();
                          if (document.activeElement?.tagName === 'IFRAME') dialog.focus();
                        }}, 50);
                        const observer = new MutationObserver(wireFrames);
                        observer.observe(dialog, {{childList:true, subtree:true}});
                        await new Promise(resolve => {{
                          let disposed = false;
                          const dispose = closed => {{
                            if (disposed) return;
                            disposed = true;
                            document.removeEventListener('keydown', escape, true);
                            document.removeEventListener('focusin', defocusFrame, true);
                            dialog.removeEventListener('close', onClose);
                            clearInterval(focusGuard);
                            observer.disconnect();
                            ownerObserver.disconnect();
                            frameListeners.forEach(({{wire, targets}}, frame) => {{
                              frame.removeEventListener('load', wire);
                              targets.forEach(target => {{
                                try {{ target.removeEventListener('keydown', escape, true); }} catch (_) {{}}
                              }});
                            }});
                            frameListeners.clear();
                            const top = [...document.querySelectorAll('dialog[open]')].at(-1);
                            if (previous?.isConnected && (!top || top.contains(previous))) previous.focus();
                            // Cleanup runs synchronously even when the Dioxus task is gone.
                            dioxus.send(closed);
                            resolve();
                          }};
                          const onClose = () => dispose(true);
                          const ownerObserver = new MutationObserver(() => {{
                            if (!dialog.isConnected) dispose(false);
                          }});
                          dialog.addEventListener('close', onClose, {{once:true}});
                          ownerObserver.observe(document.body, {{childList:true, subtree:true}});
                          if (!dialog.isConnected) dispose(false);
                        }});
                    ");
                    if let Ok(closed) = document::eval(&script).recv::<bool>().await {
                        if closed { onclose.call(()); }
                    }
                });
            },
            div { class: "modal-header", div { h2 { id: "{id}-title", "{title}" } if let Some(subtitle) = subtitle { p { class: "modal-subtitle", "{subtitle}" } } }
                button { class: "icon-button", aria_label: "关闭弹框", onclick: move |_| {
                    let script = format!("document.getElementById('{id}')?.close(); dioxus.send(true);");
                    spawn(async move { let _ = document::eval(&script).recv::<bool>().await; });
                }, Icon { name: "close" } }
            }
            {children}
        }
    }
}
