use crate::shared::{
    brand::BrandMark,
    components::Icon,
    theme::appearance::{self, use_appearance},
};
use dioxus::prelude::*;
use work_host::setup::{Phase, Selection};

#[component]
pub(super) fn DesktopEntry() -> Element {
    let setup = use_hook(super::local_node::setup);
    let explicit_remote = super::local_node::requested_address()
        .is_some_and(|address| !super::local_node::is_local(address));
    let mut bypass = use_signal(|| explicit_remote);
    let mut status = use_signal(|| setup.snapshot());
    let mut selection = use_signal(|| Selection::Automatic);
    let mut choosing_file = use_signal(|| false);
    let appearance = use_appearance();
    use_effect({
        let setup = setup.clone();
        move || {
            if !explicit_remote {
                setup.begin(Selection::Automatic);
            }
        }
    });
    use_future({
        let setup = setup.clone();
        move || {
            let mut updates = setup.subscribe();
            async move {
                status.set(updates.borrow_and_update().clone());
                while updates.changed().await.is_ok() {
                    status.set(updates.borrow_and_update().clone());
                }
            }
        }
    });
    if *bypass.read() || status.read().phase == Phase::Ready {
        return rsx! { super::workspace::App { connection_setup: *bypass.read() && !explicit_remote } };
    }
    let current = status.read().clone();
    let busy = current.busy() || *choosing_file.read();
    let percent =
        (current.total > 0).then(|| current.received.saturating_mul(100) / current.total.max(1));
    let title = match current.phase {
        Phase::Failed => "运行环境需要处理",
        Phase::Cancelled => "稍后继续准备",
        _ => "正在准备 rpi",
    };
    rsx! {
        style { {include_str!("../shared/theme/styles.css")} }
        style { "{appearance::STYLESHEETS}" }
        style { {include_str!("desktop_setup.css")} }
        document::Link { rel: "icon", r#type: "image/svg+xml", href: crate::shared::brand::icon_url() }
        main { class: "runtime-setup", "data-theme": "{appearance.read().theme.id()}",
            section { class: "runtime-setup-card", aria_label: "准备 rpi 运行环境", aria_busy: busy,
                BrandMark { class: "runtime-setup-mark" }
                h1 { "{title}" }
                p { class: "runtime-setup-message", role: if current.phase == Phase::Failed { "alert" } else { "status" }, aria_live: "polite", "{current.message}" }
                if current.phase == Phase::Downloading {
                    div { class: "runtime-setup-progress",
                        progress { max: "100", value: percent.unwrap_or(0), aria_label: "rpi 下载进度" }
                        span { "{percent.unwrap_or(0)}%" }
                    }
                }
                if let Some(path) = current.path.as_ref() {
                    p { class: "runtime-setup-path", title: "{path.display()}", "{path.display()}" }
                }
                div { class: "runtime-setup-actions",
                    if busy {
                        button { class: "button", disabled: current.phase == Phase::Starting || *choosing_file.read(),
                            onclick: { let setup = setup.clone(); move |_| setup.cancel() }, "取消准备"
                        }
                    } else {
                        button { class: "button primary", onclick: {
                            let setup = setup.clone();
                            let retry = if !current.can_install {
                                current.path.clone().map(Selection::File)
                            } else { None };
                            move |_| setup.begin(retry.clone().unwrap_or_else(|| selection.peek().clone()))
                        }, Icon { name: "refresh" } "重新检测" }
                        if current.can_install {
                            button { class: "button", onclick: {
                                let setup = setup.clone();
                                move |_| { selection.set(Selection::Managed); setup.begin(Selection::Managed); }
                            }, Icon { name: "download" } "下载并安装 rpi" }
                        }
                        button { class: "button", onclick: {
                            let setup = setup.clone();
                            move |_| {
                                choosing_file.set(true);
                                let setup = setup.clone();
                                spawn(async move {
                                    let file = rfd::AsyncFileDialog::new().set_title("选择 rpi 可执行文件").pick_file().await;
                                    choosing_file.set(false);
                                    if let Some(file) = file {
                                        let chosen = Selection::File(file.path().to_owned());
                                        selection.set(chosen.clone());
                                        setup.begin(chosen);
                                    }
                                });
                            }
                        }, Icon { name: "folder" } "选择已有 rpi" }
                    }
                }
                button { class: "runtime-setup-remote", disabled: current.phase == Phase::Starting || *choosing_file.read(),
                    onclick: { let setup = setup.clone(); move |_| { setup.cancel(); bypass.set(true); } },
                    "连接远程节点或稍后配置"
                }
            }
        }
    }
}
