use dioxus::core::spawn_forever;
use dioxus::prelude::*;
use work_update::Release;

const VERSION: &str = env!("CARGO_PKG_VERSION");
#[cfg(target_arch = "wasm32")]
const PREFERENCE: &str = "rpi-work-auto-update-v1";

#[derive(Clone)]
struct UpdateState {
    automatic: bool,
    phase: &'static str,
    message: String,
    release: Option<Release>,
    progress: u8,
    #[cfg(not(target_arch = "wasm32"))]
    prepared: Option<work_update::native::Prepared>,
}
impl Default for UpdateState {
    fn default() -> Self {
        Self {
            automatic: true,
            phase: "idle",
            message: "尚未检查更新".into(),
            release: None,
            progress: 0,
            #[cfg(not(target_arch = "wasm32"))]
            prepared: None,
        }
    }
}

pub fn use_updates() {
    let mut state = use_signal(UpdateState::default);
    use_context_provider(|| state);
    use_future(move || async move {
        #[cfg(target_arch = "wasm32")]
        if let Ok(Some(value)) = document::eval(&format!("try{{dioxus.send(JSON.parse(localStorage.getItem('{PREFERENCE}')||'null'));}}catch(_){{dioxus.send(null);}}"))
            .recv::<Option<bool>>().await { state.write().automatic = value; }
        #[cfg(not(target_arch = "wasm32"))]
        if let Ok(root) = cache_root()
            && let Ok(bytes) = std::fs::read(root.join("automatic.json"))
            && let Ok(value) = serde_json::from_slice::<bool>(&bytes)
        {
            state.write().automatic = value;
        }
        loop {
            let should_check = {
                let current = state.peek();
                current.automatic
                    && !matches!(
                        current.phase,
                        "checking" | "downloading" | "installing" | "ready"
                    )
            };
            if should_check {
                check(state).await;
            }
            #[cfg(target_arch = "wasm32")]
            gloo_timers::future::TimeoutFuture::new(6 * 60 * 60 * 1000).await;
            #[cfg(not(target_arch = "wasm32"))]
            tokio::time::sleep(std::time::Duration::from_secs(6 * 60 * 60)).await;
        }
    });
}

async fn check(mut state: Signal<UpdateState>) {
    {
        let mut state = state.write();
        state.phase = "checking";
        state.message = "正在检查 GitHub 稳定版本…".into();
    }
    let result = work_update::check(VERSION).await;
    let mut state = state.write();
    match result {
        Ok(Some(release)) => {
            state.message = format!("发现新版本 {}", release.manifest.version);
            state.release = Some(release);
            state.phase = "available";
        }
        Ok(None) => {
            state.phase = "current";
            state.message = "当前已是最新已发布版本".into();
            state.release = None;
        }
        Err(error) => {
            state.phase = "error";
            state.message = format!("检查更新失败：{error}");
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn cache_root() -> Result<std::path::PathBuf, String> {
    #[cfg(target_os = "android")]
    {
        android::cache_root()
    }
    #[cfg(not(target_os = "android"))]
    {
        work_update::native::cache_dir()
    }
}

#[component]
pub fn UpdatePage(busy: bool) -> Element {
    let mut state = use_context::<Signal<UpdateState>>();
    let snapshot = state.read().clone();
    let locked = matches!(snapshot.phase, "checking" | "downloading" | "installing");
    let toggle = move |event: Event<FormData>| {
        let automatic = event.checked();
        state.write().automatic = automatic;
        spawn_forever(async move {
            #[cfg(target_arch = "wasm32")]
            let result = document::eval(&format!("try{{localStorage.setItem('{PREFERENCE}',{automatic:?}.toString());dioxus.send(true);}}catch(_){{dioxus.send(false);}}"))
                .recv::<bool>().await.map_err(|e| e.to_string()).and_then(|saved| saved.then_some(()).ok_or("无法保存自动检查偏好".into()));
            #[cfg(not(target_arch = "wasm32"))]
            let result = cache_root().and_then(|root| {
                std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
                std::fs::write(root.join("automatic.json"), automatic.to_string())
                    .map_err(|e| e.to_string())
            });
            if let Err(error) = result {
                state.write().message = error;
            } else if automatic
                && !matches!(
                    state.peek().phase,
                    "checking" | "downloading" | "installing" | "ready"
                )
            {
                check(state).await;
            }
        });
    };
    let download = move |_| {
        #[cfg(not(target_arch = "wasm32"))]
        let available = state.read().release.clone();
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(release) = available {
            {
                let mut state = state.write();
                state.phase = "downloading";
                state.progress = 0;
                state.message = "正在下载并校验更新…".into();
            }
            spawn_forever(async move {
                let result = match cache_root() {
                    Ok(root) => {
                        work_update::native::download(&release, &root, move |done, total| {
                            let percent = u8::try_from(done.saturating_mul(100) / total.max(1))
                                .unwrap_or(100);
                            if state.peek().progress != percent {
                                state.write().progress = percent;
                            }
                        })
                        .await
                    }
                    Err(error) => Err(error),
                };
                let mut state = state.write();
                match result {
                    Ok(prepared) => {
                        state.phase = "ready";
                        state.message = "更新已下载并通过校验，可以安装。".into();
                        state.prepared = Some(prepared);
                    }
                    Err(error) => {
                        state.phase = "available";
                        state.message = format!("下载失败，可重试：{error}");
                    }
                }
            });
        }
    };
    let install = move |_| {
        if busy {
            return;
        }
        #[cfg(not(target_arch = "wasm32"))]
        let prepared = state.read().prepared.clone();
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(prepared) = prepared {
            #[cfg(target_os = "android")]
            let result = android::install(&prepared);
            #[cfg(not(target_os = "android"))]
            let result = work_update::native::start_install(&prepared);
            match result {
                Ok(()) => {
                    state.write().message = "正在安装更新…".into();
                    #[cfg(target_os = "android")]
                    {
                        state.write().message =
                            "请在系统安装界面确认更新；取消后可再次安装。".into();
                    }
                    #[cfg(not(target_os = "android"))]
                    std::process::exit(0);
                }
                Err(error) => {
                    state.write().message = error;
                }
            }
        }
        #[cfg(target_arch = "wasm32")]
        spawn(async move {
            let _ = document::eval("location.reload();dioxus.send(true);")
                .recv::<bool>()
                .await;
        });
    };
    rsx! {
        style { {include_str!("updates.css")} }
        div { class: "page-heading", div { h1 { "应用更新" } p { "本设备的客户端版本与更新。" } } }
        section { class: "update-card",
            div { class: "update-heading", h2 { "RPI Work" } span { class: "pill", "v{VERSION}" } }
            p { class: "update-platform", "平台：{work_update::target()} · 稳定通道" }
            label { class: "update-preference",
                input { r#type: "checkbox", checked: snapshot.automatic, onchange: toggle }
                span { strong { "自动检查更新" } small { "启动时和每 6 小时检查；下载、安装由你确认。" } }
            }
            p { class: "update-status", role: "status", aria_live: "polite", "{snapshot.message}" }
            if snapshot.phase == "downloading" {
                progress { max: "100", value: "{snapshot.progress}", aria_label: "更新下载进度" }
                small { "{snapshot.progress}%" }
            }
            if let Some(release) = snapshot.release {
                div { class: "update-notes", h3 { "版本 {release.manifest.version}" } p { "{release.manifest.notes}" } }
            }
            div { class: "update-actions",
                button { class: "button", disabled: locked || snapshot.phase == "ready", onclick: move |_| { spawn_forever(check(state)); }, "检查更新" }
                if snapshot.phase == "available" {
                    if cfg!(target_arch = "wasm32") {
                        p { "Web 客户端需先由电脑服务端部署新版本，再重新加载。" }
                        button { class: "button primary", disabled: busy, onclick: install, "重新加载" }
                    } else {
                        button { class: "button primary", onclick: download, "下载更新" }
                    }
                }
                if snapshot.phase == "ready" {
                    button { class: "button primary", disabled: busy, onclick: install,
                        if cfg!(target_os = "android") { "安装更新" } else { "安装并重启" }
                    }
                }
                a { class: "button", href: work_update::RELEASES_URL, target: "_blank", rel: "noopener noreferrer", "版本发布记录" }
            }
            if busy { p { class: "update-hint", "会话仍在运行，或存在未发送的草稿、附件。请先完成或清空，再安装更新。" } }
            if cfg!(target_os = "android") { p { class: "update-hint", "Android 安装需系统确认；首次可能需要允许此应用安装更新。" } }
        }
    }
}

#[component]
pub fn UpdateNotice(onopen: EventHandler<()>) -> Element {
    let state = use_context::<Signal<UpdateState>>();
    let mut dismissed = use_signal(String::new);
    let version = state
        .read()
        .release
        .as_ref()
        .map(|release| release.manifest.version.clone())
        .unwrap_or_default();
    rsx! {
        style { {include_str!("updates.css")} }
        if !version.is_empty() && *dismissed.read() != version {
            aside { class: "update-notice", aria_label: "新版本提醒",
                span { "RPI Work {version} 可更新" }
                button { class: "button", onclick: move |_| onopen.call(()), "查看更新" }
                button { class: "icon-button", aria_label: "稍后提醒", onclick: move |_| dismissed.set(version.clone()), "×" }
            }
        }
    }
}

#[cfg(target_os = "android")]
#[path = "updates_android.rs"]
mod android;
