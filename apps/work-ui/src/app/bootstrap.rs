pub fn launch() {
    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    if std::env::args_os().nth(1).as_deref() == Some(std::ffi::OsStr::new("--apply-update")) {
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
        let args = match std::env::args_os()
            .skip(1)
            .map(std::ffi::OsString::into_string)
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(args) => args,
            Err(_) => {
                work_host::report_error("启动参数必须是 UTF-8；未启动本机节点。");
                std::process::exit(2);
            }
        };
        // Explicit modes/targets do not use the environment target. Never
        // reinterpret an invalid default target as permission to start an owner.
        let configured = if args.is_empty() {
            match std::env::var("RPI_WORK_NODE_URL") {
                Ok(address) => Some(address),
                Err(std::env::VarError::NotPresent) => None,
                Err(_) => {
                    work_host::report_error(
                        "RPI_WORK_NODE_URL 必须是 UTF-8 地址；未启动本机节点。",
                    );
                    std::process::exit(2);
                }
            }
        } else {
            None
        };
        let mode = match super::desktop_launch::parse(&args, configured.as_deref()) {
            Ok(mode) => mode,
            Err(error) => {
                work_host::report_error(&error);
                std::process::exit(2);
            }
        };
        match mode {
            super::desktop_launch::Mode::Help => {
                println!("{}", super::desktop_launch::HELP);
                return;
            }
            super::desktop_launch::Mode::Headless { shutdown_stdin } => {
                let result = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|_| "无法创建无头宿主运行时。".to_owned())
                    .and_then(|runtime| runtime.block_on(work_host::run_headless(shutdown_stdin)));
                if let Err(error) = result {
                    work_host::report_error(&error);
                    std::process::exit(1);
                }
                return;
            }
            super::desktop_launch::Mode::Gui(Some(address)) => {
                super::local_node::set_requested_address(address);
            }
            super::desktop_launch::Mode::Gui(None) => {}
        }
        // Remote mode has no guard. Owning GUI mode retains the same host as
        // work-cli serve until core-owned children shut down.
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
            .launch(super::entry::Root);
    }
    #[cfg(any(target_arch = "wasm32", target_os = "android"))]
    dioxus::launch(super::entry::Root);
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
        .with_window_icon(Some(
            dioxus::desktop::icon_from_memory(include_bytes!(
                "../../../../assets/branding/rpi-256.png"
            ))
            .expect("the bundled Web brand PNG must decode"),
        ))
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
        assert!(
            window.window_icon.is_some(),
            "use the Web brand, not Dioxus's fallback"
        );
    }
}
