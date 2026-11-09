//! A resource-scoped detail owner; authority stays in the parent NodeApi flow.
use crate::shared::components::Modal;
use dioxus::prelude::*;
use serde_json::Value;
use work_core::api::{NativeKind, NativeManagement, NativeScope};

pub(super) fn scope_label(scope: NativeScope) -> &'static str {
    match scope {
        NativeScope::Project => "当前项目",
        NativeScope::Global => "节点全局",
    }
}

pub(super) fn installation_label(facts: &Value) -> &'static str {
    match facts["installed"].as_bool() {
        Some(true) => "已安装",
        Some(false) => "未安装",
        None => "安装状态未核实",
    }
}

#[component]
pub(super) fn ResourceDetail(
    reference: String,
    name: String,
    kind: NativeKind,
    scope: NativeScope,
    facts: Value,
    busy: bool,
    loading: bool,
    online: bool,
    error: String,
    notice: String,
    onmanage: EventHandler<NativeManagement>,
    onclose: EventHandler<()>,
) -> Element {
    let mut action = use_signal(|| "view".to_owned());
    let initial_source = facts["source"].as_str().unwrap_or_default().to_owned();
    let initial_version = facts["version"].as_str().unwrap_or_default().to_owned();
    let mut source = use_signal(move || initial_source);
    let mut version = use_signal(move || initial_version);
    let mut trusted = use_signal(|| false);
    let blocked = busy || loading || !online;
    let target = reference.clone();
    let installable = facts["installed"] == false && !source.peek().trim().is_empty();
    let install_state = installation_label(&facts);
    let scope_text = scope_label(scope);
    rsx! {
        Modal { id: "node-resource-detail", title: "资源详情", subtitle: Some(name.clone()),
            class: "native-resource-detail", trap_focus: true, onclose,
            div { class: "native-detail-body",
            div { class: "native-detail-summary",
                strong { "{name}" }
                p { "{scope_text} · {install_state}" }
                p { "安装不代表本会话已选择或实际加载；请在会话资源设置中核对。" }
            }
            if !online { p { role: "status", "节点离线，以下为上次读取的记录。重连并回读后才能管理。" } }
            if let Some(description) = facts["description"].as_str() { p { "{description}" } }
            dl { class: "native-detail-facts",
                dt { "资源标识" } dd { "{reference}" }
                dt { "记录版本" } dd { {facts["version"].as_str().unwrap_or("未记录")} }
                dt { "节点来源" } dd { {facts["source"].as_str().unwrap_or("未记录")} }
                dt { "实际加载" } dd { "未核实（需会话资源状态证据）" }
            }
            if installable {
                fieldset { class: "native-detail-authorization", disabled: blocked,
                    legend { "直接安装此资源" }
                    label { "节点来源（已由目录提供）" input { aria_label: "资源安装来源", readonly: true, value: "{source}" } }
                    label { class: "native-trust", input { r#type: "checkbox", checked: trusted(), onchange: move |e| trusted.set(e.checked()) } "我已检查来源并明确授权安装。安装不会自动选择或加载到会话。" }
                    button { class: "button primary", disabled: blocked || !trusted(), onclick: { let name = name.clone(); let kind = kind; let scope = scope; move |_| {
                        onmanage.call(NativeManagement::Install { scope, kind, name: name.clone(), source: source(), version: super::native::recorded_version(version()), trusted: true });
                        trusted.set(false);
                    } }, "确认下载安装" }
                }
            }
            div { class: "native-actions native-detail-primary-actions",
                button { class: "button primary", disabled: blocked, onclick: { let reference=target.clone(); move |_| onmanage.call(NativeManagement::Inspect { reference: reference.clone() }) }, "从节点检查" }
                details { class: "native-detail-management",
                    summary { "管理此资源" }
                    div { class: "native-actions",
                        for (key, label) in [("update", "更新来源"), ("reinstall", "重新安装"), ("remove", "移除资源")] {
                            button { class: if key == "remove" { "button danger" } else { "button" }, disabled: blocked,
                                onclick: move |_| { action.set(key.to_owned()); trusted.set(false); }, "{label}" }
                        }
                    }
                }
            }
            if action() == "update" || action() == "reinstall" {
                fieldset { disabled: blocked, class: "native-detail-authorization",
                    legend { if action() == "update" { "授权更新此资源" } else { "授权重装此资源" } }
                    p { "目标：{name} · {scope_text}。仅作用于此资源，不自动应用会话配置。" }
                    label { "节点来源（授权项目内路径）"
                        input { aria_label: "此资源的节点来源", value: "{source}", oninput: move |e| { source.set(e.value()); trusted.set(false); } }
                    }
                    label { "记录版本（可选）"
                        input { aria_label: "此资源的记录版本", value: "{version}", oninput: move |e| { version.set(e.value()); trusted.set(false); } }
                    }
                    label { class: "native-trust",
                        input { r#type: "checkbox", checked: trusted(), onchange: move |e| trusted.set(e.checked()) }
                        "我已核对此资源和来源，授权此次操作。选择原生库可能执行代码。"
                    }
                    div { class: "native-actions",
                        button { class: "button primary", disabled: blocked || !trusted() || source().trim().is_empty(),
                            onclick: { let reference=target.clone(); move |_| {
                                let request = if action() == "update" {
                                    NativeManagement::Update { reference: reference.clone(), source: source(), version: super::native::recorded_version(version()), trusted: trusted() }
                                } else {
                                    NativeManagement::Reinstall { reference: reference.clone(), source: source(), version: super::native::recorded_version(version()), trusted: trusted() }
                                };
                                trusted.set(false);
                                onmanage.call(request);
                            } }, "确认并提交" }
                        button { class: "button", onclick: move |_| { action.set("view".into()); trusted.set(false); }, "取消" }
                    }
                }
            }
            if action() == "remove" {
                section { class: "native-remove-confirm", aria_label: "移除确认",
                    p { "确认从{scope_text}移除「{name}」？此操作由节点检查占用情况，不会自动释放会话。" }
                    div { class: "native-actions",
                        button { class: "button danger", disabled: blocked, onclick: { let reference=target.clone(); move |_| { action.set("view".into()); onmanage.call(NativeManagement::Remove { reference: reference.clone() }); } }, "确认移除" }
                        button { class: "button", disabled: busy, onclick: move |_| action.set("view".into()), "取消" }
                    }
                }
            }
            if busy { p { role: "status", "节点操作中。关闭详情不会取消已经提交的操作，返回后请回读核对。" } }
            if !notice.is_empty() { pre { class: "native-detail-notice", role: "status", "{notice}" } }
            if !error.is_empty() { p { role: "alert", class: "resource-error", "{error}" } }
            details { summary { "技术信息与安装诊断（不代表实际加载）" }
                pre { {serde_json::to_string_pretty(&facts).unwrap_or_default()} }
            }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn missing_installation_and_lease_are_never_evidence_of_installation_or_loading() {
        assert_eq!(installation_label(&json!({"installed": true})), "已安装");
        assert_eq!(installation_label(&json!({"installed": false})), "未安装");
        for facts in [
            json!({}),
            json!({"inUse": true}),
            json!({"installed": "true"}),
        ] {
            assert_eq!(installation_label(&facts), "安装状态未核实");
        }
    }
}
