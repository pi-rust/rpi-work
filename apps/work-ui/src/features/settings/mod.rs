pub(crate) mod updates;
use crate::shared::components::Icon;
use dioxus::core::Task;
use dioxus::prelude::*;
use work_client::{AuthState, Capabilities, RuntimeState, WorkspaceClient};
mod providers;
use providers::ProviderSettings;

pub(crate) mod connection_panel;
pub(crate) use connection_panel::ConnectionScreen;
mod devices;
use devices::DevicesPage;
pub(crate) use devices::{DesktopPairing, use_pairing};

#[component]
pub fn SettingsPage(
    shared_client: Signal<Option<WorkspaceClient>>,
    requests: Signal<Vec<Task>>,
    active_page: Signal<String>,
    settings_section: Signal<String>,
    base_url: Signal<String>,
    status: Signal<String>,
    auth_state: Signal<Option<AuthState>>,
    password: Signal<crate::app::connections::NodePassword>,
    runtime_state: Signal<Option<RuntimeState>>,
    capabilities: Signal<Option<Capabilities>>,
    selected_session: Signal<Option<String>>,
    session_generation: Signal<u64>,
    connection_epoch: Signal<u64>,
    online: bool,
    onconnect: EventHandler<()>,
    onlogin: EventHandler<()>,
    onappearance: EventHandler<()>,
    update_busy: bool,
    mobile_client: bool,
) -> Element {
    let section = settings_section.read().clone();
    rsx! {
        style { {include_str!("settings.css")} }
        section { class: "page-view settings-page",
            div { class: "settings-navigation",
                nav { aria_label: "设置分类",
                    button { class: if section == "settings" { "active" } else { "" }, aria_current: if section == "settings" { "page" } else { "" },
                        onclick: move |_| settings_section.set("settings".to_owned()), Icon { name: "settings" } "通用" }
                    button { class: if section == "devices" { "active" } else { "" }, aria_current: if section == "devices" { "page" } else { "" },
                        onclick: move |_| settings_section.set("devices".to_owned()), Icon { name: "monitor" } if mobile_client { "连接我的电脑" } else { "执行节点" } }
                    button { class: if section == "providers" { "active" } else { "" }, aria_current: if section == "providers" { "page" } else { "" },
                        onclick: move |_| settings_section.set("providers".to_owned()), Icon { name: "server" } "供应商" }
                    button { class: if section == "footer" { "active" } else { "" }, aria_current: if section == "footer" { "page" } else { "" },
                        onclick: move |_| settings_section.set("footer".to_owned()), Icon { name: "settings" } "底部状态栏" }
                    button { class: if section == "updates" { "active" } else { "" }, aria_current: if section == "updates" { "page" } else { "" },
                        onclick: move |_| settings_section.set("updates".to_owned()), Icon { name: "download" } "应用更新" }
                }
                button { class: "settings-return", onclick: move |_| active_page.set("workspace".to_owned()), Icon { name: "chevron-right" } "返回会话" }
            }
            if section == "settings" {
                div { class: "page-heading", div { h1 { "设置" } p { if mobile_client { "管理工作空间偏好与电脑连接。" } else { "管理工作空间偏好、执行节点与模型连接。" } } } }
                div { class: "settings-cards",
                    button { class: "settings-card", onclick: move |_| onappearance.call(()),
                        span { class: "settings-card-icon", Icon { name: "sparkles" } }
                        strong { "外观与主题" } p { "主题风格、明暗模式与界面密度。" } small { "本设备" } Icon { name: "chevron-right" }
                    }
                    button { class: "settings-card", onclick: move |_| settings_section.set("devices".to_owned()),
                        span { class: "settings-card-icon", Icon { name: "monitor" } }
                        strong { if mobile_client { "连接我的电脑" } else { "执行节点" } } p { if mobile_client { "扫描电脑二维码，连接项目与会话。" } else { "使用本机执行节点，或连接其他节点。" } } small { if mobile_client { "扫码连接" } else { "本机与其他节点" } } Icon { name: "chevron-right" }
                    }
                    button { class: "settings-card", onclick: move |_| settings_section.set("providers".to_owned()),
                        span { class: "settings-card-icon", Icon { name: "server" } }
                        strong { "模型供应商" } p { "管理节点保存的供应商定义、凭据与模型。" } small { "节点文件配置" } Icon { name: "chevron-right" }
                    }
                    button { class: "settings-card", onclick: move |_| settings_section.set("footer".to_owned()),
                        span { class: "settings-card-icon", Icon { name: "settings" } }
                        strong { "底部状态栏" } p { "直接显示会话用量、上下文与已加载工具；配置本设备 UI 插件。" } small { "本设备 · 无需节点插件" } Icon { name: "chevron-right" }
                    }
                    button { class: "settings-card", onclick: move |_| settings_section.set("updates".to_owned()),
                        span { class: "settings-card-icon", Icon { name: "download" } }
                        strong { "应用更新" } p { "检查新版本、下载更新与安装。" } small { "当前版本 " {env!("CARGO_PKG_VERSION")} } Icon { name: "chevron-right" }
                    }
                }
            } else if section == "footer" {
                crate::features::observability::footer_plugins::FooterSettings {}
            } else if section == "updates" {
                crate::features::settings::updates::UpdatePage { busy: update_busy }
            } else if section == "providers" {
                ProviderSettings { shared_client, connection_epoch, online, selected_session, session_generation, onreturn: move |_| active_page.set("workspace".to_owned()) }
            } else {
                DevicesPage { shared_client, requests, base_url, status, auth_state, password, capabilities, onconnect, onlogin, mobile_client }
            }
        }
    }
}
