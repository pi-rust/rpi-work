pub fn launch() {
    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    if std::env::args().nth(1).as_deref() == Some("--apply-update") {
        let result = std::env::args_os()
            .nth(2)
            .ok_or_else(|| "缺少更新计划".to_owned())
            .and_then(|path| work_update::native::run_helper(std::path::Path::new(&path)));
        if let Err(error) = result {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return;
    }
    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    {
        // Retain the workspace data lock until core-owned children shut down.
        let mut local_node = super::local_node::start();
        // Dioxus enables always-on-top for debug builds by default. A normal
        // workspace must let the user switch to other applications in every profile.
        let window = desktop_window();
        dioxus::LaunchBuilder::desktop()
            .with_cfg(
                dioxus::desktop::Config::new()
                    .with_window(window)
                    .with_custom_event_handler(move |event, _| {
                        // Tao's event loop exits the process without unwinding
                        // the launch stack. Release the owned node before that.
                        if matches!(event, dioxus::desktop::tao::event::Event::LoopDestroyed) {
                            if let Some(guard) = local_node.take() {
                                guard.shutdown();
                            }
                        }
                    })
                    .with_on_window(|window, _| {
                        if cfg!(debug_assertions) {
                            eprintln!(
                                "RPI Work window initialized: always_on_top={}",
                                window.is_always_on_top()
                            );
                        }
                    }),
            )
            .launch(super::workspace::App);
    }
    #[cfg(any(target_arch = "wasm32", target_os = "android"))]
    dioxus::launch(super::workspace::App);
}

#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
fn desktop_window() -> dioxus::desktop::WindowBuilder {
    use dioxus::desktop::tao::dpi::{LogicalSize, PhysicalPosition};

    // Dioxus debug builds restore a shared window-state.json. If it was saved
    // while minimized, it can contain (-32000, -32000) and a tiny client area.
    // Explicit position AND size bypass that cache; maximizing fits the current
    // monitor's work area instead of assuming a particular resolution or DPI.
    dioxus::desktop::WindowBuilder::new()
        .with_title("RPI Work")
        .with_always_on_top(false)
        .with_position(PhysicalPosition::new(0, 0))
        .with_inner_size(LogicalSize::new(1280.0, 800.0))
        .with_maximized(true)
}

#[cfg(all(test, not(any(target_arch = "wasm32", target_os = "android"))))]
mod tests {
    #[test]
    fn desktop_startup_overrides_stale_debug_window_geometry() {
        let window = super::desktop_window().window;
        assert!(
            window.position.is_some(),
            "must not restore off-screen coordinates"
        );
        assert!(
            window.inner_size.is_some(),
            "must not restore minimized dimensions"
        );
        assert!(window.maximized, "fit the current monitor's work area");
        assert!(window.visible);
        assert!(!window.always_on_top);
    }
}
