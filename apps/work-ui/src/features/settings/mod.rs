pub(crate) mod updates;
use crate::shared::{
    app_settings,
    components::Icon,
    theme::appearance::{AppearancePicker, AppearanceSettings},
};
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

// Persisted section keys and external shortcuts remain compatible with the old navigation.
fn category(section: &str) -> &'static str {
    match section {
        "display" | "appearance" | "footer" | "session-display" => "display",
        "devices" | "components" => "devices",
        "providers" => "providers",
        "updates" => "updates",
        "app-settings" => "app-settings",
        _ => "settings",
    }
}

const CATEGORIES: [(&str, &str, &str); 6] = [
    ("settings", "settings", "设置总览"),
    ("display", "sparkles", "界面与显示"),
    ("devices", "monitor", "工作协同"),
    ("providers", "server", "供应商"),
    ("updates", "download", "应用更新"),
    ("app-settings", "code", "高级设置"),
];

const DISPLAY_SECTIONS: [(&str, &str); 3] = [
    ("appearance", "外观与主题"),
    ("session-display", "会话展示"),
    ("footer", "底部状态栏"),
];

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
    appearance: Signal<AppearanceSettings>,
    update_busy: bool,
    mobile_client: bool,
) -> Element {
    let section = match settings_section.read().as_str() {
        "display" => "appearance".to_owned(),
        section => section.to_owned(),
    };
    let current_category = category(&section);
    // Scroll only the horizontal rails, never the document or the user's focus.
    use_effect(move || {
        let _section = settings_section.read().clone();
        let _ = document::eval(include_str!("settings_navigation.js"));
    });
    rsx! {
        style { {include_str!("settings.css")} }
        section { class: "page-view settings-page",
            header { class: "page-heading settings-page-heading",
                div {
                    div { class: "eyebrow", "让工具，适应你的习惯" }
                    h1 { "设置" }
                    p { class: "page-subtitle", "外观留在本设备，执行配置归当前节点。" }
                }
                button { class: "settings-return", r#type: "button", title: "返回会话", aria_label: "返回会话",
                    onclick: move |_| active_page.set("workspace".to_owned()), Icon { name: "chevron-right" } "返回会话" }
            }
            div { class: "settings-layout",
                div { class: "settings-navigation",
                    span { class: "settings-nav-label", "偏好设置" }
                    div { class: "settings-rail",
                        nav { aria_label: "设置分类",
                            for (key, icon, label) in CATEGORIES {
                                button { r#type: "button", class: if current_category == key { "active" } else { "" },
                                    aria_current: if current_category == key { "page" } else { "false" },
                                    onclick: { let next = if key == "display" { "appearance".to_owned() } else { key.to_owned() }; move |_| settings_section.set(next.clone()) },
                                    Icon { name: icon } "{label}"
                                }
                            }
                        }
                    }
                }
                div { class: "settings-content",
                    if current_category == "display" {
                        div { class: "settings-intro",
                            div { h2 { "界面与显示" } p { "打造一个愿意每天打开的工作台。" } }
                            span { class: "neutral-pill", Icon { name: "monitor" } "仅本设备" }
                        }
                    }
                    if current_category == "display" {
                        div { class: "settings-display-navigation settings-rail",
                            nav { aria_label: "界面与显示分类",
                                for (key, label) in DISPLAY_SECTIONS {
                                    button { r#type: "button", class: if section == key { "active" } else { "" },
                                        aria_current: if section == key { "page" } else { "false" },
                                        onclick: move |_| settings_section.set(key.to_owned()), "{label}"
                                    }
                                }
                            }
                        }
                    }
                    if section == "settings" || category(&section) == "settings" {
                        div { class: "page-heading", div { h2 { "设置总览" } p { "按工作目标管理偏好。显示设置保存在本设备，模型配置由当前节点管理。" } } }
                        div { class: "settings-cards",
                            for (key, icon, label, description, scope) in [
                                ("display", "sparkles", "界面与显示", "调整主题、会话排列与底部状态栏。", "本设备"),
                                ("devices", "monitor", "工作协同", "管理设备连接、远程协作与节点组件。", "设备与节点"),
                                ("providers", "server", "供应商", "管理模型供应商、凭据和可用模型。", "当前节点"),
                                ("updates", "download", "应用更新", "检查新版本、下载并安装更新。", "本设备"),
                                ("app-settings", "code", "高级设置", "直接编辑全局 JSON；常用显示偏好请使用界面与显示。", "本设备 · 高级"),
                            ] {
                                button { class: "settings-card", r#type: "button", onclick: { let next = if key == "display" { "appearance".to_owned() } else { key.to_owned() }; move |_| settings_section.set(next.clone()) },
                                    span { class: "settings-card-icon", Icon { name: icon } }
                                    strong { "{label}" } p { "{description}" } small { "{scope}" } Icon { name: "chevron-right" }
                                }
                            }
                        }
                    } else if section == "appearance" {
                        AppearancePicker { settings: appearance, embedded: true, onclose: move |()| settings_section.set("display".to_owned()) }
                    } else if section == "app-settings" {
                        div { class: "page-heading", div { h2 { "高级设置" } p { "全局 JSON 配置仅作用于本设备。修改前建议保留副本；常用显示偏好可在界面与显示中调整。" } } }
                        crate::shared::app_settings::AppSettingsPage {}
                    } else if section == "footer" {
                        crate::features::observability::footer_plugins::FooterSettings {}
                    } else if section == "session-display" {
                        SessionDisplaySettings {}
                    } else if section == "updates" {
                        crate::features::settings::updates::UpdatePage { busy: update_busy }
                    } else if section == "devices" {
                        div { class: "settings-node-workbench",
                            DevicesPage { shared_client, requests, base_url, status, auth_state, password, capabilities, onconnect, onlogin, mobile_client }
                        }
                    } else if section == "providers" {
                        ProviderSettings {
                            shared_client, connection_epoch, online, selected_session, session_generation, runtime_state,
                            oncomponents: move |_| settings_section.set("devices".to_owned()),
                        }
                    } else {
                        DevicesPage { shared_client, requests, base_url, status, auth_state, password, capabilities, onconnect, onlogin, mobile_client }
                    }
                }
            }
        }
    }
}

#[component]
fn SessionDisplaySettings() -> Element {
    let store = use_context::<Signal<app_settings::AppSettingsState>>();
    let state = store.read().clone();
    let mode = app_settings::session_display_mode(&state.value);
    let set_mode = move |next: app_settings::SessionDisplayMode| {
        if app_settings::update_session_display_mode(store, next) {
            app_settings::request_write(store);
        }
    };
    rsx! {
        section { class: "settings-panel session-display-settings", aria_label: "会话展示设置",
            div { class: "settings-section-title",
                div {
                    h3 { "会话展示" }
                    p { "只影响本设备的展示，不改变节点、项目或会话内容。" }
                }
            }
            div { class: "session-preference-row",
                div {
                    label { r#for: "session-display-setting", "会话展示方式" }
                    p { "按节点分组，或平铺浏览。侧栏保留现有项目树与操作入口。" }
                }
                select { id: "session-display-setting",
                    value: if mode == app_settings::SessionDisplayMode::Flat { "flat" } else { "grouped" },
                    disabled: !state.ready,
                    onchange: move |event| set_mode(if event.value() == "flat" { app_settings::SessionDisplayMode::Flat } else { app_settings::SessionDisplayMode::GroupedByNode }),
                    option { value: "grouped", "按节点分组" }
                    option { value: "flat", "平铺会话" }
                }
            }
            div { class: "session-preference-preview",
                Icon { name: "monitor" }
                span { if mode == app_settings::SessionDisplayMode::Flat { "直接浏览会话，保留原有项目树与操作入口；手机切换器使用相同偏好。" } else { "节点标题下仍是原有的项目与会话；手机切换器使用相同偏好。" } }
            }
            p { class: "session-preference-notice", "选择后自动保存到本设备。" }
            if !state.ready { p { role: "status", "正在读取本设备设置…" } }
            if let Some(error) = state.error.as_deref() { p { class: "app-settings-error", role: "alert", "{error}" } }
        }
    }
}

#[cfg(test)]
mod navigation_tests {
    use super::category;

    #[test]
    fn legacy_shortcuts_keep_their_category() {
        for section in ["appearance", "footer", "session-display", "display"] {
            assert_eq!(category(section), "display");
        }
        assert_eq!(category("components"), "devices");
        for section in [
            "devices",
            "providers",
            "updates",
            "app-settings",
            "settings",
        ] {
            assert_eq!(category(section), section);
        }
        assert_eq!(category("unknown"), "settings");
    }
}
