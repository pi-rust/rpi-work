use dioxus::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};

const RESIZE: &str = include_str!("sidebar_resize.js");
static NEXT_HANDLE: AtomicU64 = AtomicU64::new(1);

/// Independent from workspace/session state, including high-frequency dragging.
#[component]
pub fn Handle() -> Element {
    let instance = use_hook(|| NEXT_HANDLE.fetch_add(1, Ordering::Relaxed));
    let id = format!("rpi-sidebar-resize-{instance}");
    let mount_id = serde_json::to_string(&id).unwrap();
    let cleanup_id = mount_id.clone();
    use_drop(move || {
        let _ = document::eval(&format!(
            "window.__rpiSidebarResize?.get({cleanup_id})?.();"
        ));
    });
    rsx! {
        div {
            id, class: "sidebar-resize-handle", role: "separator", tabindex: "0",
            aria_label: "调整侧栏宽度", aria_orientation: "vertical", aria_controls: "project-sidebar",
            aria_valuemin: "200", aria_valuemax: "480", aria_valuenow: "272",
            title: "拖动或按左右方向键调整宽度，双击或按 Enter 恢复默认",
            onmounted: move |_| {
                let _ = document::eval(&format!("const id={mount_id};{RESIZE}"));
            }
        }
    }
}
