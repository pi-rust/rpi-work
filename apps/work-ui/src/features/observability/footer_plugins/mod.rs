//! Built-in client UI plugins; intentionally independent of node extensions.
//! Main integration: call `use_footer_preferences()` once in App, register `CSS`,
//! then render `FooterHost { context }` and `FooterSettings {}` where appropriate.
mod preferences;
mod registry;

use crate::shared::{app_settings, app_settings::AppSettingsState, components::Icon};
use dioxus::prelude::*;
use preferences::PluginPreference;
pub use preferences::{FooterPreferencesContext, use_footer_preferences};
use registry::{BUILTINS, FooterItem, FooterVisual, Tone};
use work_core::extensions::{RuntimeFooterSnapshot, UiSlot};

pub const CSS: &str = include_str!("footer_plugins.css");

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FooterContext {
    pub scope: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub thinking: Option<String>,
    pub metrics: super::runtime_stats::Metrics,
    pub context: super::runtime_stats::Context,
    pub tool_calls: usize,
    pub tool_failures: usize,
    pub online: bool,
}

impl FooterContext {
    /// Convert the built-in client context to the read-only extension host contract.
    #[must_use]
    pub fn host_snapshot(&self) -> RuntimeFooterSnapshot {
        RuntimeFooterSnapshot {
            online: self.online,
            provider: self.provider.clone(),
            model: self.model.clone(),
            thinking: self.thinking.clone(),
            input_tokens: self.metrics.input,
            output_tokens: self.metrics.output,
            cache_hit_rate: self.metrics.cache_percent,
            cost: self.metrics.cost,
            speed: self.metrics.speed,
            requests: self.metrics.requests,
            retries: self.metrics.retries,
            context_used_percent: self.context.percent,
            tool_calls: u64::try_from(self.tool_calls).ok(),
            tool_failures: u64::try_from(self.tool_failures).ok(),
        }
    }
}

fn select_runtime_footer_contributions(
    document: &serde_json::Value,
    contributions: Vec<crate::features::extensions::UiContributionEntry>,
) -> Vec<crate::features::extensions::UiContributionEntry> {
    let mut contributions = contributions
        .into_iter()
        .enumerate()
        .filter(|(_, entry)| {
            entry.contribution.slot == UiSlot::RuntimeFooter
                && app_settings::contribution_visible(document, &entry.key(), true)
        })
        .collect::<Vec<_>>();
    contributions.sort_by_key(|(default_order, entry)| {
        app_settings::contribution_order(document, &entry.key(), *default_order as i64)
    });
    // Keep a hostile or very large local design library from expanding the footer.
    contributions.truncate(16);
    contributions.into_iter().map(|(_, entry)| entry).collect()
}

#[component]
pub fn FooterHost(
    context: FooterContext,
    #[props(default)] contributions: Vec<crate::features::extensions::UiContributionEntry>,
) -> Element {
    // Keep the built-in footer and future UI contributions on the same bounded
    // host snapshot contract, even before live external contributions are mounted.
    let host_snapshot = context.host_snapshot();
    let _ = host_snapshot.validate();
    let preferences = use_context::<FooterPreferencesContext>();
    let settings_document = try_use_context::<Signal<AppSettingsState>>()
        .map(|store| store.read().value.clone())
        .unwrap_or_else(app_settings::default_value);
    let contributions = select_runtime_footer_contributions(&settings_document, contributions);
    let settings = preferences.store.read().settings.clone();
    let collected = registry::collect(BUILTINS, &context);
    let mut groups = Vec::new();
    for preference in &settings.plugins {
        if !preference.enabled {
            continue;
        }
        let Some((plugin, items)) = collected.iter().find(|(p, _)| p.id == preference.id) else {
            continue;
        };
        let items: Vec<_> = items
            .iter()
            .filter(|i| !preference.hidden_items.contains(&i.id))
            .cloned()
            .collect();
        if !items.is_empty() {
            groups.push((*plugin, items));
        }
    }
    if groups.is_empty() && contributions.is_empty() {
        return rsx! {};
    }
    let show_caption = groups
        .iter()
        .any(|(plugin, _)| plugin.id == "session-usage");
    rsx! {
        style { "{CSS}" }
        footer { class: "runtime-footer plugin-footer footer-metrics", aria_label: "Agent 运行统计",
            tabindex: "0",
            "aria-description": "只读状态栏；内容较多时可横向滑动，或聚焦后使用左右方向键浏览。",
            if !context.online {
                span { class: "footer-warning", role: "status", "节点未连接，统计未确认" }
            } else if !registry::confirmed(&context) {
                span { class: "footer-warning", "当前会话未确认，统计 —" }
            }
            for (plugin, items) in groups {
                div { class: "footer-plugin", "data-plugin-id": plugin.id,
                    for item in items {
                        span {
                            class: "footer-item {registry::metric_class(&item.id)}",
                            "data-item-id": "{item.id}",
                            "data-tone": match item.tone { Tone::Neutral => "neutral", Tone::Good => "good", Tone::Warning => "warning" },
                            role: "group",
                            aria_label: "{item.label}：{item.value}。{item.description}",
                            {metric_value(&item)}
                        }
                    }
                }
            }
            if show_caption { span { class: "footer-detail", "会话累计" } }
            for entry in contributions {
                crate::features::extensions::RuntimeFooterContribution {
                    contribution: entry.contribution,
                    snapshot: host_snapshot.clone(),
                    design_only: entry.design_only,
                }
            }
        }
    }
}

/// The registry supplies bounded typed data, never markup or parsed display text.
fn metric_value(item: &FooterItem) -> Element {
    rsx! {
        span { class: "footer-value", aria_hidden: "true",
            match &item.visual {
                FooterVisual::Text => rsx! { "{item.value}" },
                FooterVisual::Tokens { input, output } => rsx! {
                    Icon { name: "diff" }
                    span { "↑ {input}" }
                    span { "↓ {output}" }
                },
                FooterVisual::Cache { value, known } => rsx! {
                    span { "缓存 " }
                    b { class: "footer-cache-value", "data-known": "{known}", "{value}" }
                },
                FooterVisual::Context { value, used } => rsx! {
                    span {
                        class: "context-ring", "aria-hidden": "true",
                        "data-known": "{used.is_some()}",
                        style: used.map(|used| format!("--used:{used}%")),
                    }
                    span { "上下文 {value}" }
                },
                FooterVisual::Cost { value } => rsx! { "{value}" },
                FooterVisual::Tools { calls, failures } => rsx! {
                    span { {calls.map_or_else(|| "—".into(), |calls| calls.to_string())} " tools" }
                    if *failures > 0 { span { " ·失败{failures}" } }
                },
            }
        }
    }
}

#[component]
pub fn FooterSettings() -> Element {
    let context = use_context::<FooterPreferencesContext>();
    let state = context.store.read().clone();
    let count = state.settings.plugins.len();
    let cards = state
        .settings
        .plugins
        .iter()
        .enumerate()
        .map(|(index, preference)| settings_card(context, preference, index, count, state.ready));
    rsx! {
        style { "{CSS}" }
        section { class: "footer-plugin-settings", aria_label: "底部状态栏",
            h2 { "底部状态栏" }
            p { class: "footer-settings-help", "客户端内置 UI 插件，只读取当前会话；不在节点安装插件，不执行导入源码。" }
            if !state.ready { p { role: "status", "正在读取本地状态栏配置…" } }
            if let Some(error) = state.storage_error {
                p { class: "footer-settings-error", role: "alert", "{error}" }
            }
            if state.corrupt_guard {
                p { class: "footer-settings-help", "已暂停自动保存以保护原配置；可临时调整内存设置，恢复默认状态栏后才会覆盖本地配置。" }
            }
            div { class: "footer-plugin-cards", {cards} }
            button { r#type: "button", class: "footer-settings-reset", disabled: !state.ready,
                onclick: move |_| preferences::change(context, true, |_| {}), "恢复默认状态栏"
            }
        }
    }
}

fn settings_card(
    context: FooterPreferencesContext,
    preference: &PluginPreference,
    index: usize,
    count: usize,
    ready: bool,
) -> Element {
    let Some(plugin) = BUILTINS.iter().find(|p| p.id == preference.id) else {
        return rsx! {};
    };
    let id = plugin.id;
    rsx! {
        fieldset { class: "footer-plugin-card", aria_label: "{plugin.title} 插件设置",
            legend { "{plugin.title}" }
            div { class: "footer-plugin-card-header",
                label { class: "footer-settings-check",
                    input { r#type: "checkbox", checked: preference.enabled, disabled: !ready,
                        aria_label: "{plugin.title} 插件",
                        onchange: move |event| preferences::change(context, false, |s| {
                            if let Some(p) = s.plugins.iter_mut().find(|p| p.id == id) { p.enabled = event.checked(); }
                        })
                    }
                    span { "启用 {plugin.title}" }
                }
                span { class: "footer-plugin-version", "内置 v{plugin.version}" }
                div { class: "footer-plugin-order", aria_label: "{plugin.title} 排序",
                    button { r#type: "button", disabled: !ready || index == 0, aria_label: "上移 {plugin.title}",
                        onclick: move |_| preferences::change(context, false, |s| {
                            if let Some(i) = s.plugins.iter().position(|p| p.id == id) { if i > 0 { s.plugins.swap(i, i - 1); } }
                        }), "上移"
                    }
                    button { r#type: "button", disabled: !ready || index + 1 == count, aria_label: "下移 {plugin.title}",
                        onclick: move |_| preferences::change(context, false, |s| {
                            if let Some(i) = s.plugins.iter().position(|p| p.id == id) { if i + 1 < s.plugins.len() { s.plugins.swap(i, i + 1); } }
                        }), "下移"
                    }
                }
            }
            div { class: "footer-plugin-item-options", role: "group", aria_label: "{plugin.title} 显示项目",
                for item in plugin.items {
                    label { class: "footer-settings-check",
                        input { r#type: "checkbox", checked: !preference.hidden_items.iter().any(|i| i == item.id), disabled: !ready,
                            aria_label: item.label,
                            onchange: move |event| preferences::change(context, false, |s| {
                                if let Some(p) = s.plugins.iter_mut().find(|p| p.id == id) {
                                    p.hidden_items.retain(|i| i != item.id);
                                    if !event.checked() { p.hidden_items.push(item.id.into()); }
                                    // Hidden IDs have declaration order, independent of click order.
                                    p.hidden_items.sort_by_key(|id| plugin.items.iter().position(|i| i.id == id));
                                }
                            })
                        }
                        span { "{item.label}" }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use work_core::extensions::{ExtensionPin, UiContribution, UiWidget};

    fn entry(id: &str, slot: UiSlot) -> crate::features::extensions::UiContributionEntry {
        crate::features::extensions::UiContributionEntry {
            extension: ExtensionPin {
                id: "demo.test".into(),
                version: "1.0.0".into(),
            },
            contribution: UiContribution {
                id: id.into(),
                slot,
                title: id.into(),
                widgets: vec![UiWidget::Text {
                    text: "design".into(),
                }],
                host: None,
            },
            design_only: true,
        }
    }

    #[test]
    fn runtime_footer_selection_filters_and_orders_entries() {
        let document = json!({
            "version": 1,
            "contributions": {
                "demo.test@1.0.0/hidden": { "visible": false },
                "demo.test@1.0.0/first": { "order": -10 }
            }
        });
        let selected = select_runtime_footer_contributions(
            &document,
            vec![
                entry("hidden", UiSlot::RuntimeFooter),
                entry("settings", UiSlot::SettingsSection),
                entry("last", UiSlot::RuntimeFooter),
                entry("first", UiSlot::RuntimeFooter),
            ],
        );
        assert_eq!(
            selected
                .iter()
                .map(|entry| entry.contribution.id.as_str())
                .collect::<Vec<_>>(),
            ["first", "last"]
        );
        let many = (0..20)
            .map(|index| entry(&format!("item-{index}"), UiSlot::RuntimeFooter))
            .collect();
        assert_eq!(
            select_runtime_footer_contributions(&json!({}), many).len(),
            16
        );
    }
}
