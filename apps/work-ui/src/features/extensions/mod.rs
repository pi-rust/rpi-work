//! Host-owned rendering of inert extension UI designs. No plugin code runs here.
use dioxus::prelude::*;
use serde_json::Value;
use work_core::extensions::{ExtensionManifest, UiExtensionPreference, UiWidget};

pub const CSS: &str = include_str!("extensions.css");

/// Read the canonical source, preserving old entry-only designs without silently
/// upgrading them to executable Agent extensions or UI contributions.
pub fn design_manifest(
    sources: &Value,
    id: &str,
    version: &str,
) -> Result<Option<ExtensionManifest>, String> {
    let Some(source) = sources.get("extension.design.json") else {
        return Ok(None);
    };
    let text = source.as_str().ok_or("extension.design.json 必须是文本")?;
    let value: Value =
        serde_json::from_str(text).map_err(|e| format!("扩展清单 JSON 无效：{e}"))?;
    let modern = value.get("schemaVersion").is_some()
        || value.get("agent").is_some()
        || value.get("ui").is_some();
    if !modern {
        if value["kind"] != "rpi-work-extension-design"
            || value["id"] != id
            || value["version"] != version
            || !value["entry"]
                .as_str()
                .is_some_and(|entry| sources[entry].is_string())
        {
            return Err("旧扩展设计的身份、版本或源码入口无效".into());
        }
        return Ok(None);
    }
    let manifest: ExtensionManifest =
        serde_json::from_value(value).map_err(|e| format!("扩展声明不受支持：{e}"))?;
    manifest.validate()?;
    if manifest.id != id || manifest.version != version {
        return Err("扩展清单 ID/版本必须与所属设计包一致".into());
    }
    if let Some(agent) = &manifest.agent
        && !sources[&agent.entry].is_string()
    {
        return Err("Agent 声明的源码入口不存在".into());
    }
    Ok(Some(manifest))
}

#[must_use]
pub fn kind_label(manifest: Option<&ExtensionManifest>) -> &'static str {
    match manifest.map(|m| (m.agent.is_some(), m.ui.is_some())) {
        Some((true, true)) => "Agent + UI 扩展设计",
        Some((true, false)) => "Agent 扩展设计",
        Some((false, true)) => "纯 UI 扩展设计",
        _ => "旧版扩展设计 · 未声明 Agent / UI",
    }
}

#[component]
pub fn ExtensionPreview(
    manifest: ExtensionManifest,
    preferences: Vec<UiExtensionPreference>,
    onvisibility: EventHandler<(String, bool)>,
) -> Element {
    if let Err(error) = manifest.validate() {
        return rsx! { p { role: "alert", class: "resource-error", "扩展预览不可用：{error}" } };
    }
    let pin = manifest.pin();
    rsx! {
        style { "{CSS}" }
        section { class: "extension-contract", aria_label: "Agent 与 UI 扩展声明",
            h3 { "Agent 能力" }
            if let Some(agent) = &manifest.agent {
                p { "执行位置：节点 · rpi 原生扩展" }
                code { "{agent.entry}" }
                p { class: "meta", "节点安装/加载：未知。当前客户端尚未接入扩展管理协议。" }
                h4 { "声明的工具（不代表节点已提供）" }
                div { class: "resource-tool-tags", for tool in &agent.tools { code { "{tool}" } } }
                h4 { "请求权限（不是授权）" }
                div { class: "resource-tool-tags", for permission in &agent.permissions { code { "{permission}" } } }
                if agent.permissions.is_empty() { p { class: "meta", "未声明权限；实际策略仍由节点执行。" } }
            } else { p { class: "meta", "未提供 Agent 能力，不加入 Agent 会话配置。" } }
            h3 { "UI 能力" }
            if let Some(ui) = &manifest.ui {
                p { class: "resource-boundary", "本地设计预览 · 静态声明，不是节点实时数据。显示/隐藏只影响此设备，不会启用或停用 Agent。" }
                for contribution in &ui.contributions {
                    {
                        let visible = preferences.iter().find(|p| p.extension == pin && p.contribution_id == contribution.id).is_none_or(|p| p.visible);
                        let id = contribution.id.clone();
                        rsx! {
                            article { class: "extension-preview", key: "{pin.key()}/{contribution.id}",
                                header {
                                    div { strong { "{contribution.title}" } code { "{contribution.slot.label()} · {contribution.id}" } }
                                    button { class: "button", aria_label: format!("{} UI 预览：{}", if visible { "隐藏" } else { "显示" }, contribution.title), aria_pressed: visible,
                                        onclick: move |_| onvisibility.call((id.clone(), !visible)),
                                        if visible { "隐藏 UI" } else { "显示 UI" }
                                    }
                                }
                                if visible {
                                    UiWidgets { widgets: contribution.widgets.clone() }
                                } else { p { class: "meta", "此设备已隐藏该 UI；Agent 配置未改变。" } }
                            }
                        }
                    }
                }
            } else { p { class: "meta", "无自定义 UI。Agent 扩展仍可通过宿主默认工具卡展示调用。" } }
        }
    }
}

/// Host-owned data rendering shared by design previews and the isolated live demo.
/// Callers must validate their manifest before passing its widgets here.
#[component]
pub fn UiWidgets(widgets: Vec<UiWidget>) -> Element {
    rsx! {
        div { class: "extension-widgets",
            for widget in &widgets {
                match widget {
                    UiWidget::Text { text } => rsx! { p { "{text}" } },
                    UiWidget::Fields { fields } => rsx! { dl { for field in fields { div { dt { "{field.label}" } dd { "{field.value}" } } } } },
                    UiWidget::Progress { label, value, max } => rsx! { label { class: "extension-progress", span { "{label} · {value}/{max}" } progress { value: *value, max: *max, aria_label: "{label}" } } },
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn source(agent: bool, ui: bool) -> Value {
        let mut manifest = json!({"schemaVersion":1,"kind":"rpi-work-extension-design","id":"team.review","name":"Review","version":"1.0.0"});
        if agent {
            manifest["agent"] =
                json!({"runtime":"rpi-native","entry":"src/lib.rs","tools":[],"permissions":[]});
        }
        if ui {
            manifest["ui"] = json!({"schemaVersion":1,"contributions":[{"id":"status","title":"Status","slot":"settings-section","widgets":[{"type":"text","text":"<script>not executed</script>"}]}]});
        }
        json!({"extension.design.json":manifest.to_string(),"src/lib.rs":"// inert source"})
    }

    #[test]
    fn reads_agent_only_ui_only_and_combined_designs() {
        for (agent, ui) in [(true, false), (false, true), (true, true)] {
            let manifest = design_manifest(&source(agent, ui), "team.review", "1.0.0")
                .unwrap()
                .unwrap();
            assert_eq!(manifest.agent.is_some(), agent);
            assert_eq!(manifest.ui.is_some(), ui);
        }
    }

    #[test]
    fn legacy_is_not_automatically_upgraded() {
        let old = json!({"extension.design.json":json!({"kind":"rpi-work-extension-design","id":"team.review","name":"Review","version":"1.0.0","entry":"src/lib.rs"}).to_string(),"src/lib.rs":"// inert"});
        assert_eq!(design_manifest(&old, "team.review", "1.0.0").unwrap(), None);
        assert_eq!(
            design_manifest(&json!({"src/lib.rs":"legacy"}), "team.review", "1.0.0").unwrap(),
            None
        );
    }

    #[test]
    fn invalid_identity_entry_and_future_schema_are_rejected() {
        assert!(design_manifest(&source(true, true), "other.id", "1.0.0").is_err());
        let mut missing = source(true, false);
        missing.as_object_mut().unwrap().remove("src/lib.rs");
        assert!(design_manifest(&missing, "team.review", "1.0.0").is_err());
        let mut future = source(false, true);
        let mut manifest: Value =
            serde_json::from_str(future["extension.design.json"].as_str().unwrap()).unwrap();
        manifest["schemaVersion"] = json!(2);
        future["extension.design.json"] = json!(manifest.to_string());
        assert!(design_manifest(&future, "team.review", "1.0.0").is_err());
    }
}
