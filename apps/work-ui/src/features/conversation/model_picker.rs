use std::collections::{BTreeMap, BTreeSet};

use std::sync::atomic::{AtomicU64, Ordering};

use crate::shared::components::Icon;
use dioxus::prelude::*;
use work_client::ModelDescriptor;

pub const CSS: &str = include_str!("model_picker.css");

fn provider_name(provider: &str) -> &str {
    match provider {
        "deepseek" => "DeepSeek",
        "openai" => "OpenAI",
        "" => "未报告供应商",
        other => other,
    }
}

/// Search is local presentation only; retain the full catalog for selection validation.
fn matches_query(model: &ModelDescriptor, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    query.is_empty()
        || [&model.provider, &model.name, &model.id]
            .into_iter()
            .any(|value| value.to_lowercase().contains(&query))
        || provider_name(&model.provider)
            .to_lowercase()
            .contains(&query)
}

/// Keep reported provider identities intact; display names are never setter values.
fn group_models(models: &[ModelDescriptor]) -> BTreeMap<&str, Vec<&ModelDescriptor>> {
    let mut groups = BTreeMap::<&str, Vec<&ModelDescriptor>>::new();
    for model in models {
        let group = groups.entry(model.provider.as_str()).or_default();
        if !group.iter().any(|known| known.id == model.id) {
            group.push(model);
        }
    }
    for group in groups.values_mut() {
        group.sort_by(|a, b| (&a.name, &a.id).cmp(&(&b.name, &b.id)));
    }
    groups
}

fn duplicate_ids(models: &[ModelDescriptor]) -> BTreeSet<&str> {
    let mut providers = BTreeMap::<&str, BTreeSet<&str>>::new();
    for model in models {
        providers
            .entry(&model.id)
            .or_default()
            .insert(&model.provider);
    }
    providers
        .into_iter()
        .filter_map(|(id, providers)| (providers.len() > 1).then_some(id))
        .collect()
}

/// Some nodes qualify catalog ids but report a bare runtime model id. Only
/// remove an exact provider prefix when the authoritative provider agrees.
pub(crate) fn matches_model(model: &ModelDescriptor, provider: &str, id: &str) -> bool {
    (provider.is_empty() || model.provider == provider)
        && (model.id == id
            || (!provider.is_empty() && model.id.strip_prefix(&format!("{provider}/")) == Some(id)))
}

fn is_current(
    models: &[ModelDescriptor],
    model: &ModelDescriptor,
    current_provider: &str,
    current_model: &str,
) -> bool {
    matches_model(model, current_provider, current_model)
        && (!current_provider.is_empty() || !duplicate_ids(models).contains(model.id.as_str()))
}

/// Reject unknown/stale descriptors and blank ids. A reported provider makes a
/// duplicate id unambiguous; without one, the setter cannot choose a provider.
pub(crate) fn selectable_model(models: &[ModelDescriptor], selected: &ModelDescriptor) -> bool {
    !selected.id.trim().is_empty()
        && models.iter().any(|model| model == selected)
        && (!selected.provider.trim().is_empty()
            || !duplicate_ids(models).contains(selected.id.as_str()))
}

fn current_model_name<'a>(models: &'a [ModelDescriptor], provider: &str, id: &'a str) -> &'a str {
    if id.trim().is_empty() {
        return "选择模型";
    }
    models
        .iter()
        .find(|model| is_current(models, model, provider, id))
        .filter(|model| !model.name.trim().is_empty())
        .map_or(id, |model| model.name.as_str())
}

fn thinking_label(level: &str) -> &str {
    match level {
        "off" => "关闭",
        "minimal" => "最低",
        "low" => "低",
        "medium" => "中",
        "high" => "高",
        "xhigh" => "更高",
        "max" => "最高",
        "" => "思考等级",
        other => other,
    }
}

/// Local, non-modal top-layer surface; never change the app's general Modal behavior.
#[component]
fn PickerPopover(
    id: &'static str,
    title: &'static str,
    anchor: &'static str,
    class: &'static str,
    onclose: EventHandler,
    children: Element,
) -> Element {
    static NEXT_INSTANCE: AtomicU64 = AtomicU64::new(1);
    let instance = use_hook(|| NEXT_INSTANCE.fetch_add(1, Ordering::Relaxed));
    let mut task = use_signal(|| None::<dioxus::core::Task>);
    use_effect(move || {
        let script = format!(
            "({})({id:?}, {anchor:?}, {instance:?}, message => dioxus.send(message));",
            include_str!("model_picker.js")
        );
        task.set(Some(spawn(async move {
            if document::eval(&script).recv::<String>().await.is_ok() {
                onclose.call(());
            }
        })));
    });
    use_drop(move || {
        let _ = document::eval(&format!(
            "window.__rpiComposerPickers?.get({instance:?})?.();"
        ));
        if let Some(task) = task.write().take() {
            task.cancel();
        }
    });
    rsx! {
        div {
            id, class: "composer-picker-popover composer-model-dialog {class}",
            role: "dialog", aria_label: title, "popover": "manual",
            style: "visibility:hidden",
            {children}
        }
    }
}

/// Presentation only: discovery, session scope, mutation, and verified readback
/// belong to the caller. Increment `commit_revision` only after verified success.
#[component]
pub fn ModelPicker(
    models: Vec<ModelDescriptor>,
    current_model: String,
    current_provider: String,
    loading: bool,
    error: Option<String>,
    read_supported: bool,
    write_supported: bool,
    idle: bool,
    busy: bool,
    #[props(default)] thinking_busy: bool,
    #[props(default)] verification_required: bool,
    #[props(default = ".composer-model-picker")] anchor: &'static str,
    #[props(default)] preference: bool,
    commit_revision: u64,
    onrefresh: EventHandler,
    onselect: EventHandler<ModelDescriptor>,
) -> Element {
    let mut open = use_signal(|| false);
    let mut query = use_signal(String::new);
    let mut last_commit = use_signal(|| commit_revision);
    use_effect(use_reactive((&commit_revision,), move |(revision,)| {
        if *last_commit.peek() != revision {
            last_commit.set(revision);
            open.set(false);
        }
    }));

    let filtered = models
        .iter()
        .filter(|model| matches_query(model, &query.read()))
        .cloned()
        .collect::<Vec<_>>();
    let groups = group_models(&filtered);
    let searching = !query.read().trim().is_empty();
    let duplicates = duplicate_ids(&models);
    let locked =
        loading || busy || thinking_busy || verification_required || !idle || !write_supported;
    let title = format!("{current_provider} / {current_model}");
    let display_name = current_model_name(&models, &current_provider, &current_model);

    rsx! {
        button {
            r#type: "button",
            class: "composer-select composer-model-picker",
            aria_label: "选择供应商与模型",
            title,
            aria_haspopup: "dialog",
            aria_expanded: open(),
            aria_controls: "composer-model-picker",
            "data-model-id": "{current_model}",
            "data-provider": "{current_provider}",
            onclick: move |_| {
                query.set(String::new());
                open.set(!open());
            },
            span { class: "composer-model-provider", "{display_name}" }
            span { class: "composer-model-chevron", Icon { name: "chevron" } }
        }
        if open() {
            PickerPopover {
                id: "composer-model-picker",
                title: "选择供应商与模型",
                anchor,
                class: "composer-model-popover",
                onclose: move |()| open.set(false),
                div { class: "composer-picker-heading",
                    span { "选择模型" }
                    button {
                        r#type: "button", class: "composer-model-refresh", title: "刷新模型列表",
                        aria_label: "刷新模型列表", disabled: loading || busy || thinking_busy,
                        onclick: move |_| onrefresh.call(()),
                        Icon { name: "refresh" }
                    }
                }
                div { class: "composer-model-body",
                    div { class: "composer-model-notices", aria_live: "polite",
                        if loading { p { role: "status", "正在加载模型列表…" } }
                        if busy { p { role: "status", "正在切换模型，等待读回确认…" } }
                        if thinking_busy { p { role: "status", "正在切换思考等级，暂不可切换模型。" } }
                        if verification_required { p { class: "composer-config-unconfirmed", "上次配置请求超时，结果待核对。超时不代表节点操作已取消；请刷新核对当前配置后继续。" } }
                        if let Some(message) = &error {
                            p { role: "alert", "模型操作失败：{message}。可刷新后重试。" }
                        }
                        if !read_supported { p { "当前节点不支持读取模型列表。" } }
                        if !write_supported { p { "当前节点不支持切换模型；仍可浏览列表。" } }
                        if !idle { p { "运行中暂不可切换模型" } }
                        if !loading && models.is_empty() { p { "暂无可用模型；可刷新模型列表重试。" } }
                        if !duplicates.is_empty() {
                            p { "多个供应商报告了相同模型 ID；选择时会连同供应商一起提交，只有未报告供应商的条目无法区分：" }
                            ul { class: "composer-model-duplicates",
                                for id in &duplicates { li { key: "{id}", code { "{id}" } } }
                            }
                        }
                    }
                    div { class: "composer-model-searchbar",
                        div { class: "composer-model-search",
                            Icon { name: "search" }
                            input {
                                r#type: "search", aria_label: "搜索供应商或模型",
                                placeholder: "搜索供应商、模型名称或 ID",
                                value: "{query}",
                                oninput: move |event| query.set(event.value()),
                            }
                        }
                        if !query.read().is_empty() {
                            button {
                                r#type: "button", class: "button composer-model-clear",
                                aria_label: "清除模型搜索",
                                onclick: move |_| query.set(String::new()),
                                "清除"
                            }
                        }
                    }
                    p { class: "composer-model-search-status", role: "status",
                        if searching { "找到 {filtered.len()} 个模型" }
                        else { "按供应商分组 · {models.len()} 个模型" }
                    }
                    if searching && filtered.is_empty() && !loading {
                        p { class: "composer-model-empty", "没有匹配的模型，请尝试其他名称或清除搜索。" }
                    }
                    div { class: "composer-model-groups",
                        for (provider, group) in groups {
                            // Native toggles survive parent updates; searching opens matching groups.
                            ProviderGroup {
                                key: "{provider}",
                                provider: provider.to_string(),
                                initially_open: true,
                                searching,
                                models: group.into_iter().cloned().collect::<Vec<_>>(),
                                current_ids: models.iter()
                                    .filter(|model| is_current(&models, model, &current_provider, &current_model))
                                    .map(|model| (model.provider.clone(), model.id.clone())).collect::<Vec<_>>(),
                                duplicates: duplicates.iter().map(|id| (*id).to_string()).collect::<Vec<_>>(),
                                locked,
                                onselect: move |model| { onselect.call(model); if preference { open.set(false); } },
                                oncurrent: move |()| open.set(false),
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ProviderGroup(
    provider: String,
    initially_open: bool,
    searching: bool,
    models: Vec<ModelDescriptor>,
    current_ids: Vec<(String, String)>,
    duplicates: Vec<String>,
    locked: bool,
    onselect: EventHandler<ModelDescriptor>,
    oncurrent: EventHandler,
) -> Element {
    let initial_open = use_signal(|| initially_open);
    rsx! {
        details {
            class: "composer-provider-group", "data-provider": "{provider}", open: initial_open() || searching,
            summary {
                span { class: "composer-provider-label", "{provider_name(&provider)}" }
                span { class: "composer-provider-count", "{models.len()}" }
                span { class: "composer-provider-chevron", Icon { name: "chevron" } }
            }
            div { class: "composer-provider-models",
                for model in models {
                    {
                        let current = current_ids.contains(&(model.provider.clone(), model.id.clone()));
                        // Only an entry that reports no provider stays ambiguous.
                        let ambiguous = model.provider.trim().is_empty() && duplicates.contains(&model.id);
                        let disabled = locked || ambiguous || model.id.trim().is_empty();
                        rsx! {
                            button {
                                key: "{model.provider}/{model.id}", r#type: "button",
                                class: "composer-model-option",
                                "data-model-id": "{model.id}", "data-provider": "{model.provider}",
                                aria_pressed: current, disabled,
                                autofocus: false,
                                onclick: {
                                    let model = model.clone();
                                    move |_| {
                                        if disabled { return; }
                                        if current { oncurrent.call(()); }
                                        else { onselect.call(model.clone()); }
                                    }
                                },
                                span { class: "composer-model-label",
                                    span { "{model.name}" }
                                    code { "{model.id}" }
                                    if ambiguous { small { "未报告供应商且 ID 与其他供应商重复，无法切换" } }
                                    if model.id.trim().is_empty() { small { "未报告有效模型 ID" } }
                                }
                                if current {
                                    span { class: "composer-model-check", aria_label: "当前模型",
                                        Icon { name: "check" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Keep discovery order and exact raw identities; whitespace is only used to
/// reject blank entries, never to normalize values sent back to the caller.
fn available_thinking_levels(levels: &[String]) -> Vec<&str> {
    let mut seen = BTreeSet::new();
    levels
        .iter()
        .map(String::as_str)
        .filter(|level| !level.trim().is_empty() && seen.insert(*level))
        .collect()
}

/// Presentation only, like `ModelPicker`: the caller owns mutation and verified
/// readback. Pending/error updates leave the dialog open; only a new committed
/// revision (or an explicit user dismissal) closes it.
#[component]
pub fn ThinkingPicker(
    levels: Vec<String>,
    current_level: String,
    disabled: bool,
    write_supported: bool,
    busy: bool,
    #[props(default)] loading: bool,
    #[props(default)] load_error: Option<String>,
    #[props(default)] verification_required: bool,
    #[props(default = ".composer-thinking-select")] anchor: &'static str,
    #[props(default)] preference: bool,
    onrefresh: EventHandler,
    error: Option<String>,
    commit_revision: u64,
    onselect: EventHandler<String>,
) -> Element {
    let mut open = use_signal(|| false);
    let mut last_commit = use_signal(|| commit_revision);
    use_effect(use_reactive((&commit_revision,), move |(revision,)| {
        if *last_commit.peek() != revision {
            last_commit.set(revision);
            open.set(false);
        }
    }));

    let levels = available_thinking_levels(&levels);
    let locked = disabled || !write_supported || busy || loading || verification_required;
    // Loading/error can be inspected and retried; a genuinely empty or
    // unsupported catalog retains the previous disabled-trigger behavior.
    let trigger_disabled = disabled
        || (!write_supported && load_error.is_none() && !verification_required)
        || busy
        || (levels.is_empty() && !loading && load_error.is_none() && !verification_required);

    rsx! {
        button {
            r#type: "button",
            class: "composer-select composer-thinking-select",
            aria_label: "思考等级",
            aria_haspopup: "dialog",
            aria_expanded: open(),
            "data-thinking-level": "{current_level}",
            title: "思考等级：{thinking_label(&current_level)}（{current_level}）",
            aria_controls: "composer-thinking-picker",
            disabled: trigger_disabled,
            onclick: move |_| {
                if !trigger_disabled { open.set(!open()); }
            },
            span { class: "composer-thinking-label",
                if !current_level.is_empty() { "思考 · " }
                "{thinking_label(&current_level)}"
            }
            span { class: "composer-model-chevron", Icon { name: "chevron" } }
        }
        if open() {
            PickerPopover {
                id: "composer-thinking-picker",
                title: "选择思考等级",
                anchor,
                class: "composer-thinking-dialog",
                onclose: move |()| open.set(false),
                div { class: "composer-picker-heading",
                    span { "思考等级" }
                    button {
                        r#type: "button", class: "composer-model-refresh composer-thinking-refresh",
                        aria_label: "刷新思考等级", title: "刷新思考等级", disabled: loading || busy || disabled,
                        onclick: move |_| onrefresh.call(()),
                        Icon { name: "refresh" }
                    }
                }
                div { class: "composer-model-body",
                    div { class: "composer-model-notices", aria_live: "polite",
                        if loading { p { role: "status", "正在加载思考等级…" } }
                        if busy { p { role: "status", "正在切换思考等级，等待读回确认…" } }
                        if verification_required { p { class: "composer-config-unconfirmed", "上次配置请求超时，结果待核对。超时不代表节点操作已取消；请刷新核对当前配置后继续。" } }
                        if let Some(message) = &load_error { p { role: "alert", "{message}" } }
                        if let Some(message) = &error {
                            p { role: "alert", "思考等级操作失败：{message}" }
                        }
                        if disabled || !write_supported { p { "当前暂不可切换思考等级。" } }
                        if !loading && levels.is_empty() { p { "暂无可用思考等级。" } }
                    }
                    div { class: "composer-thinking-options",
                        for level in levels {
                            {
                                let current = level == current_level;
                                rsx! {
                                    button {
                                        key: "{level}", r#type: "button",
                                        class: "composer-thinking-option composer-model-option",
                                        "data-thinking-level": "{level}",
                                        aria_pressed: current,
                                        disabled: locked,
                                        autofocus: false,
                                        onclick: {
                                            let level = level.to_string();
                                            move |_| {
                                                if locked { return; }
                                                if current { open.set(false); }
                                                else { onselect.call(level.clone()); if preference { open.set(false); } }
                                            }
                                        },
                                        span { class: "composer-model-label", "{thinking_label(level)}" }
                                        if current {
                                            span { class: "composer-model-check", aria_label: "当前思考等级",
                                                Icon { name: "check" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(provider: &str, id: &str) -> ModelDescriptor {
        ModelDescriptor {
            id: id.into(),
            name: id.into(),
            provider: provider.into(),
            reasoning: false,
            api: None,
            context_window: None,
            max_tokens: None,
        }
    }

    #[test]
    fn current_label_uses_verified_identity_not_just_provider_or_first_catalog_entry() {
        let mut first = model("alpha", "alpha/shared");
        first.name = "Alpha model".into();
        let mut second = model("beta", "beta/shared");
        second.name = "Beta model".into();
        let catalog = vec![first, second];
        assert_eq!(current_model_name(&catalog, "beta", "shared"), "Beta model");
        assert_eq!(
            current_model_name(&catalog, "alpha", "shared"),
            "Alpha model"
        );
        assert_eq!(current_model_name(&catalog, "missing", "shared"), "shared");
        assert_eq!(current_model_name(&catalog, "alpha", ""), "选择模型");
        assert_eq!(current_model_name(&[], "alpha", "raw-id"), "raw-id");
        let duplicates = vec![model("alpha", "same"), model("beta", "same")];
        assert_eq!(current_model_name(&duplicates, "", "same"), "same");
    }

    #[test]
    fn thinking_display_labels_do_not_rewrite_custom_protocol_values() {
        for (raw, label) in [
            ("off", "关闭"),
            ("minimal", "最低"),
            ("low", "低"),
            ("medium", "中"),
            ("high", "高"),
            ("xhigh", "更高"),
            ("max", "最高"),
        ] {
            assert_eq!(thinking_label(raw), label);
        }
        assert_eq!(thinking_label("custom/raw"), "custom/raw");
        assert_eq!(thinking_label(" HIGH "), " HIGH ");
    }

    #[test]
    fn search_matches_provider_display_name_model_name_and_raw_id() {
        let mut entry = model("openai", "gpt/model-v1");
        entry.name = "工作台模型".into();
        for query in ["", "  ", " OpenAI ", "MODEL-V1", "工作台"] {
            assert!(matches_query(&entry, query), "{query}");
        }
        assert!(!matches_query(&entry, "deepseek"));
        assert!(matches_query(&model("", "unknown"), "未报告供应商"));
        // Filtering must never make a duplicate raw ID safe to submit.
        let catalog = vec![model("openai", "shared"), model("deepseek", "shared")];
        assert_eq!(
            catalog
                .iter()
                .filter(|m| matches_query(m, "openai"))
                .count(),
            1
        );
        // A reported provider disambiguates a shared raw id.
        assert!(selectable_model(&catalog, &catalog[0]));
        assert!(!selectable_model(&catalog, &model("", "shared")));
    }

    #[test]
    fn thinking_levels_dedupe_exactly_without_rewriting_raw_values() {
        let levels = ["high", " high ", "HIGH", "high", "custom/raw", " high "].map(String::from);
        assert_eq!(
            available_thinking_levels(&levels),
            vec!["high", " high ", "HIGH", "custom/raw"]
        );
    }

    #[test]
    fn thinking_levels_skip_blanks_without_inventing_current_or_default_choices() {
        let levels = ["", " ", "\t\n", "\u{3000}", " low ", "low"].map(String::from);
        let choices = available_thinking_levels(&levels);
        assert_eq!(choices, vec![" low ", "low"]);
        assert!(!choices.contains(&"unsupported-current"));
        assert_eq!(available_thinking_levels(&[]), Vec::<&str>::new());
        assert_eq!(
            available_thinking_levels(&[" \t".into(), String::new()]),
            Vec::<&str>::new()
        );
    }

    #[test]
    fn groups_keep_reported_providers_and_sort_deterministically() {
        let models = vec![
            model("openai", "z"),
            model("", "b"),
            model("openai", "a"),
            model("openai", "a"),
        ];
        let groups = group_models(&models);
        assert_eq!(
            groups.keys().copied().collect::<Vec<_>>(),
            vec!["", "openai"]
        );
        assert_eq!(
            groups["openai"]
                .iter()
                .map(|m| m.id.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "z"]
        );
        assert_eq!(provider_name(""), "未报告供应商");
        assert_eq!(provider_name("deepseek"), "DeepSeek");
        assert_eq!(provider_name("openai"), "OpenAI");
        assert_eq!(provider_name("CustomProvider"), "CustomProvider");
    }

    #[test]
    fn current_matching_requires_exact_provider_or_unambiguous_raw_id() {
        let models = vec![
            model("openai", "same"),
            model("deepseek", "same"),
            model("", "unique"),
        ];
        assert!(is_current(&models, &models[0], "openai", "same"));
        assert!(!is_current(&models, &models[1], "openai", "same"));
        assert!(!is_current(&models, &models[0], "", "same"));
        assert!(is_current(&models, &models[2], "", "unique"));
        assert!(!is_current(&models, &models[0], "OpenAI", "same"));
    }

    #[test]
    fn qualified_catalog_and_bare_runtime_ids_require_provider_agreement() {
        let model = model("alpha", "alpha/family/model-v1");
        assert!(matches_model(&model, "alpha", "family/model-v1"));
        assert!(matches_model(&model, "alpha", "alpha/family/model-v1"));
        assert!(!matches_model(&model, "beta", "family/model-v1"));
        assert!(!matches_model(&model, "", "family/model-v1"));
        assert!(!matches_model(&model, "alpha", "model-v1"));
    }

    #[test]
    fn validation_rejects_unknown_blank_and_stale_descriptors() {
        let models = vec![
            model("a", "shared"),
            model("b", "shared"),
            model("a", "ok"),
            model("a", "ok"),
            model("", " "),
        ];
        assert_eq!(duplicate_ids(&models), BTreeSet::from(["shared"]));
        // Provider-qualified duplicates are selectable; unqualified ids are not.
        assert!(selectable_model(&models, &models[0]));
        assert!(selectable_model(&models, &models[1]));
        assert!(selectable_model(&models, &models[2]));
        assert!(!selectable_model(&models, &models[4]));
        assert!(!selectable_model(&models, &model("other", "ok")));
        assert!(!selectable_model(&models, &model("a", "unknown")));
        assert!(!selectable_model(&models, &model("", "shared")));
        let mut stale = models[2].clone();
        stale.name = "changed".into();
        assert!(!selectable_model(&models, &stale));
        assert!(!selectable_model(&[], &models[2]));
    }
}
