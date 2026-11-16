use dioxus::prelude::*;

#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
#[derive(Clone, Copy)]
struct BackgroundControl {
    enter: EventHandler<()>,
    error: Signal<String>,
}

/// Tray and window state live above the workspace and survive page navigation.
#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
pub(super) fn use_background() {
    use dioxus::desktop::{
        icon_from_memory,
        trayicon::{
            MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent,
            menu::{Menu, MenuItem},
        },
        use_tray_icon_event_handler, use_tray_menu_event_handler, window,
    };
    let desktop = window();
    let mut error = use_signal(String::new);
    let (restore_id, exit_id, tray) = use_hook(|| {
        let menu = Menu::new();
        let restore = MenuItem::new("恢复工作窗口", true, None);
        let exit = MenuItem::new("退出并停止节点", true, None);
        let tray = (|| {
            menu.append_items(&[&restore, &exit])
                .map_err(|e| e.to_string())?;
            let icon = icon_from_memory(include_bytes!("../../../../assets/branding/rpi-256.png"))
                .map_err(|e| e.to_string())?;
            TrayIconBuilder::new()
                .with_tooltip("RPI Work · 点击托盘图标恢复窗口")
                .with_icon(icon)
                .with_menu(Box::new(menu))
                .with_menu_on_left_click(false)
                .build()
                .map_err(|e| e.to_string())
        })();
        (restore.id().clone(), exit.id().clone(), tray)
    });
    use_tray_menu_event_handler({
        let desktop = desktop.clone();
        move |event| {
            if event.id == restore_id {
                restore_window(&desktop);
            } else if event.id == exit_id {
                desktop.close();
            }
        }
    });
    use_tray_icon_event_handler({
        let desktop = desktop.clone();
        move |event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            ) {
                restore_window(&desktop);
            }
        }
    });
    use_context_provider(|| BackgroundControl {
        error,
        enter: EventHandler::new(move |()| {
            if let Err(message) = &tray {
                error.set(format!("无法创建系统托盘，窗口保持打开。{message}"));
                return;
            }
            if super::local_node::client().is_none() {
                error.set("本机节点尚未准备好，暂时无法转入后台。".into());
                return;
            }
            // Keep drafts, dialogs and in-flight owner requests in their scopes.
            // Hiding WebView pauses visibility-aware catalog and queue timers.
            error.set(String::new());
            if let Err(message) = desktop.webview.set_visible(false) {
                error.set(format!("无法隐藏工作窗口。{message}"));
                return;
            }
            desktop.set_visible(false);
            memory_level(&desktop, true);
        }),
    });
}

#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
fn restore_window(desktop: &dioxus::desktop::DesktopContext) {
    let _ = desktop.webview.set_visible(true);
    memory_level(desktop, false);
    desktop.set_visible(true);
    desktop.set_minimized(false);
    desktop.set_focus();
}

#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
fn memory_level(desktop: &dioxus::desktop::DesktopContext, low: bool) {
    #[cfg(target_os = "windows")]
    {
        use dioxus::desktop::wry::{MemoryUsageLevel, WebViewExtWindows};
        // Older WebView2 versions may not support this optional optimization.
        let _ = desktop.webview.set_memory_usage_level(if low {
            MemoryUsageLevel::Low
        } else {
            MemoryUsageLevel::Normal
        });
    }
    #[cfg(not(target_os = "windows"))]
    let _ = (desktop, low);
}

#[component]
pub(crate) fn BackgroundRunButton() -> Element {
    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    {
        let control = use_context::<BackgroundControl>();
        return rsx! {
            button { class: "button", title: "隐藏窗口并降低界面占用，节点和任务继续运行。需要恢复时，点击系统托盘图标或托盘菜单中的“恢复工作窗口”。",
                onclick: move |_| control.enter.call(()),
                crate::shared::components::Icon { name: "monitor" }
                "一键后台运行"
            }
            if !control.error.read().is_empty() {
                p { class: "connection-form-error", role: "alert", "{control.error}" }
            }
        };
    }
    #[cfg(any(target_arch = "wasm32", target_os = "android"))]
    rsx! {}
}
