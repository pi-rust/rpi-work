use dioxus::prelude::*;

/// Pinned local Lucide geometry shared with the Web; labels stay on controls.
#[component]
pub fn Icon(name: String) -> Element {
    let icon = super::icons::definition(&name);
    rsx! {
        svg {
            class: "icon",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            "data-icon": name,
            "data-lucide": icon.name,
            "aria-hidden": "true",
            "focusable": "false",
            // Only compile-time, allowlisted SVG primitives; never caller markup.
            dangerous_inner_html: icon.svg,
        }
    }
}

#[cfg(test)]
mod icon_tests {
    use crate::shared::icons::definition;

    #[test]
    fn every_semantic_icon_uses_the_pinned_web_catalog() {
        let map: std::collections::BTreeMap<String, String> =
            serde_json::from_str(include_str!("../../../../assets/icons/lucide-map.json")).unwrap();
        assert_eq!(map.len(), 43);
        for (name, expected) in map {
            let icon = definition(&name);
            assert_eq!(
                icon.name, expected,
                "{name} must resolve to its Web geometry"
            );
            assert_ne!(icon, definition(""), "{name} must not use the fallback");
            assert!(icon.svg.starts_with('<'));
            for unsafe_markup in ["<script", "<image", "<use", "href=", "onload=", "style="] {
                assert!(
                    !icon.svg.contains(unsafe_markup),
                    "{name} must be static geometry"
                );
            }
        }
    }

    #[test]
    fn unknown_names_have_a_stable_safe_fallback() {
        let fallback = definition("");
        assert_eq!(fallback.name, "circle-help");
        for unknown in ["unknown", "Download", "<svg onload='alert(1)'>", "工具"] {
            assert_eq!(definition(unknown), fallback);
        }
        assert_ne!(fallback, definition("info"));
        assert_ne!(fallback, definition("ask"));
    }
}

/// Native modal provides focus containment and restores the invoking control on close.
#[component]
pub fn Modal(
    id: &'static str,
    title: &'static str,
    #[props(default)] subtitle: Option<String>,
    #[props(default)] trap_focus: bool,
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
                          if (!dialog.open) return;
                          const layers = [...document.querySelectorAll('dialog[open]')];
                          if (layers.at(-1) !== dialog) return;
                          if (event.key === 'Tab' && {trap_focus}) {{
                            const targets = [...dialog.querySelectorAll('button, a[href], input, select, textarea, [tabindex]')]
                              .filter(el => !el.disabled && el.tabIndex >= 0 && el.getClientRects().length);
                            const first = targets[0], last = targets.at(-1);
                            if (!first) {{ event.preventDefault(); dialog.focus(); return; }}
                            if (event.shiftKey && (document.activeElement === first || !targets.includes(document.activeElement))) {{
                              event.preventDefault(); last.focus();
                            }} else if (!event.shiftKey && (document.activeElement === last || !targets.includes(document.activeElement))) {{
                              event.preventDefault(); first.focus();
                            }}
                            return;
                          }}
                          if (event.key !== 'Escape') return;
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
