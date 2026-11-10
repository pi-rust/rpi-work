//! Client-owned global application settings.
//!
//! These settings control Work UI presentation only. They do not install,
//! authorize, select, or load native resources or node extensions.

use dioxus::prelude::*;
use serde_json::{Value, json};

pub const VERSION: u64 = 1;

/// Stable keys shared by UI consumers. They are presentation preferences only.
#[allow(dead_code)]
pub mod ids {
    pub mod menus {
        pub const EXTENSIONS: &str = "extensions";
        pub const PACKAGES: &str = "packages";
        pub const SKILLS: &str = "skills";
        pub const META_AGENT: &str = "meta-agent";
    }

    pub mod contributions {
        pub const RESOURCE_AUTHORING_SETTINGS: &str = "resource-authoring@1.0.0/resource-settings";
    }

    pub mod storage {
        pub const APPEARANCE: &str = "appearance";
        pub const FOOTER: &str = "footer";
        pub const SESSION_DISPLAY: &str = "sessionDisplay";
        pub const DEFAULT_PROVIDER: &str = "defaultProvider";
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionDisplayMode {
    GroupedByNode,
    Flat,
}

impl SessionDisplayMode {
    pub const fn key(self) -> &'static str {
        match self {
            Self::GroupedByNode => "grouped",
            Self::Flat => "flat",
        }
    }
}

#[must_use]
pub fn session_display_mode(document: &Value) -> SessionDisplayMode {
    match document
        .get(ids::storage::SESSION_DISPLAY)
        .and_then(Value::as_object)
        .and_then(|entry| entry.get("mode"))
        .and_then(Value::as_str)
    {
        Some("flat") => SessionDisplayMode::Flat,
        _ => SessionDisplayMode::GroupedByNode,
    }
}

pub fn update_session_display_mode(
    store: Signal<AppSettingsState>,
    mode: SessionDisplayMode,
) -> bool {
    update_namespace(
        store,
        ids::storage::SESSION_DISPLAY,
        json!({ "mode": mode.key() }),
    )
    .is_some()
}

/// Return the provider/model pair used when creating a new session.
#[must_use]
pub fn default_provider_model(document: &Value) -> Option<(String, String)> {
    let value = document
        .get(ids::storage::DEFAULT_PROVIDER)
        .and_then(Value::as_object)?;
    Some((
        value.get("provider")?.as_str()?.to_owned(),
        value.get("model")?.as_str()?.to_owned(),
    ))
}

pub fn update_default_provider(
    store: Signal<AppSettingsState>,
    provider: &str,
    model: &str,
) -> bool {
    update_namespace(
        store,
        ids::storage::DEFAULT_PROVIDER,
        json!({ "provider": provider, "model": model }),
    )
    .is_some()
}

pub fn clear_default_provider(store: Signal<AppSettingsState>) -> bool {
    update_namespace(store, ids::storage::DEFAULT_PROVIDER, Value::Null).is_some()
}

#[cfg(any(target_arch = "wasm32", target_os = "android"))]
const STORAGE_KEY: &str = "rpi-work-settings-v1";

#[derive(Clone, Debug, PartialEq)]
pub struct AppSettingsState {
    pub value: Value,
    pub raw: String,
    pub ready: bool,
    pub busy: bool,
    pub error: Option<String>,
    pub notice: Option<String>,
    revision: u64,
    writing: bool,
}

impl Default for AppSettingsState {
    fn default() -> Self {
        let value = default_value();
        Self {
            raw: format_value(&value),
            value,
            ready: false,
            busy: false,
            error: None,
            notice: None,
            revision: 0,
            writing: false,
        }
    }
}

#[must_use]
pub fn default_value() -> Value {
    json!({
        "version": VERSION,
        "menus": {},
        "contributions": {}
    })
}

#[must_use]
pub fn default_text() -> String {
    format_value(&default_value())
}

#[must_use]
pub fn format_value(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| "{\n  \"version\": 1\n}".into()) + "\n"
}

/// Return whether a menu entry is visible. Unknown entries use the built-in default.
#[must_use]
pub fn menu_visible(document: &Value, id: &str, default: bool) -> bool {
    document
        .get("menus")
        .and_then(Value::as_object)
        .and_then(|menus| menus.get(id))
        .and_then(Value::as_object)
        .and_then(|entry| entry.get("visible"))
        .and_then(Value::as_bool)
        .unwrap_or(default)
}

/// Return a menu order preference. The caller supplies the built-in order.
#[must_use]
pub fn menu_order(document: &Value, id: &str, default: i64) -> i64 {
    document
        .get("menus")
        .and_then(Value::as_object)
        .and_then(|menus| menus.get(id))
        .and_then(Value::as_object)
        .and_then(|entry| entry.get("order"))
        .and_then(Value::as_i64)
        .unwrap_or(default)
}

/// Return whether a UI contribution is visible on this device.
#[must_use]
pub fn contribution_visible(document: &Value, key: &str, default: bool) -> bool {
    document
        .get("contributions")
        .and_then(Value::as_object)
        .and_then(|contributions| contributions.get(key))
        .and_then(Value::as_object)
        .and_then(|entry| entry.get("visible"))
        .and_then(Value::as_bool)
        .unwrap_or(default)
}

/// Return a contribution order preference. Unknown entries use the host order.
#[must_use]
pub fn contribution_order(document: &Value, key: &str, default: i64) -> i64 {
    document
        .get("contributions")
        .and_then(Value::as_object)
        .and_then(|contributions| contributions.get(key))
        .and_then(Value::as_object)
        .and_then(|entry| entry.get("order"))
        .and_then(Value::as_i64)
        .unwrap_or(default)
}

/// Update one device-local UI contribution preference and return the snapshot.
pub fn update_contribution(
    mut store: Signal<AppSettingsState>,
    key: &str,
    value: Value,
) -> Option<Value> {
    let mut state = store.write();
    if !state.ready || state.error.is_some() {
        return None;
    }
    let object = state.value.as_object_mut()?;
    let contributions = object
        .entry("contributions")
        .or_insert_with(|| json!({}))
        .as_object_mut()?;
    contributions.insert(key.to_owned(), value);
    state.raw = format_value(&state.value);
    state.error = None;
    state.notice = None;
    state.revision = state.revision.wrapping_add(1);
    Some(state.value.clone())
}

/// Validate the stable envelope while retaining unknown fields for forward compatibility.
pub fn parse(text: &str) -> Result<Value, String> {
    let value: Value =
        serde_json::from_str(text).map_err(|error| format!("JSON 解析失败：{error}"))?;
    let object = value
        .as_object()
        .ok_or_else(|| "全局设置必须是 JSON 对象。".to_owned())?;
    if object.get("version").and_then(Value::as_u64) != Some(VERSION) {
        return Err(format!("全局设置版本必须是 {VERSION}。"));
    }
    for key in ["menus", "contributions"] {
        if let Some(value) = object.get(key) {
            if !value.is_object() {
                return Err(format!("全局设置字段 {key} 必须是 JSON 对象。"));
            }
        }
    }
    Ok(value)
}

/// Load the client settings once at application startup.
///
/// Invalid bytes remain in `raw` and are never overwritten automatically. UI
/// consumers use the default document until the user explicitly saves a valid
/// replacement.
pub async fn load_state() -> AppSettingsState {
    match read_text().await {
        Ok(raw) => match parse(&raw) {
            Ok(value) => AppSettingsState {
                raw,
                value,
                ready: true,
                busy: false,
                error: None,
                notice: None,
                revision: 0,
                writing: false,
            },
            Err(error) => AppSettingsState {
                raw,
                value: default_value(),
                ready: true,
                busy: false,
                error: Some(format!("全局设置未应用：{error} 原始内容已保留。")),
                notice: None,
                revision: 0,
                writing: false,
            },
        },
        Err(error) => AppSettingsState {
            raw: default_text(),
            value: default_value(),
            ready: true,
            busy: false,
            error: Some(error),
            notice: None,
            revision: 0,
            writing: false,
        },
    }
}

#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
fn native_path() -> Option<std::path::PathBuf> {
    #[cfg(target_os = "windows")]
    let root = std::env::var_os("LOCALAPPDATA").map(std::path::PathBuf::from);
    #[cfg(target_os = "macos")]
    let root = std::env::var_os("HOME")
        .map(|path| std::path::PathBuf::from(path).join("Library/Application Support"));
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let root = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|path| std::path::PathBuf::from(path).join(".config"))
        });
    root.map(|path| path.join("rpi-work").join("settings.json"))
}

#[must_use]
pub fn storage_label() -> String {
    #[cfg(any(target_arch = "wasm32", target_os = "android"))]
    {
        "当前设备 WebView 本地存储（rpi-work-settings-v1）".into()
    }
    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    {
        native_path().map_or_else(
            || "本设备设置文件路径不可用".into(),
            |path| path.display().to_string(),
        )
    }
}

pub async fn read_text() -> Result<String, String> {
    #[cfg(any(target_arch = "wasm32", target_os = "android"))]
    {
        let raw = document::eval(&format!(
            "try {{ dioxus.send(localStorage.getItem({})); }} catch (_) {{ dioxus.send(null); }}",
            serde_json::to_string(STORAGE_KEY).expect("static storage key")
        ))
        .recv::<Option<String>>()
        .await
        .map_err(|_| "无法读取本设备全局设置。".to_owned())?;
        Ok(raw.unwrap_or_else(default_text))
    }
    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    {
        let Some(path) = native_path() else {
            return Err("无法定位本设备全局设置文件。".into());
        };
        match std::fs::read_to_string(path) {
            Ok(text) => Ok(text),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(default_text()),
            Err(_) => Err("无法读取本设备全局设置文件。".into()),
        }
    }
}

/// Update one top-level preference namespace in memory and return the complete snapshot.
/// The caller owns the asynchronous write, which keeps this helper usable by UI stores.
pub fn update_namespace(
    mut store: Signal<AppSettingsState>,
    namespace: &str,
    value: Value,
) -> Option<Value> {
    let mut state = store.write();
    if !state.ready || state.error.is_some() {
        return None;
    }
    let object = state.value.as_object_mut()?;
    object.insert(namespace.to_owned(), value);
    state.raw = format_value(&state.value);
    state.error = None;
    state.notice = None;
    state.revision = state.revision.wrapping_add(1);
    Some(state.value.clone())
}

/// Coalesce namespace writes so appearance and footer migration cannot lose
/// each other's top-level fields through racing snapshots.
pub fn request_write(mut store: Signal<AppSettingsState>) {
    {
        let mut state = store.write();
        if !state.ready || state.error.is_some() || state.writing {
            return;
        }
        state.writing = true;
    }
    spawn(async move {
        loop {
            let (snapshot, revision) = {
                let state = store.peek();
                (state.value.clone(), state.revision)
            };
            let success = write_value(&snapshot).await.is_ok();
            let mut state = store.write();
            if !success {
                state.error = Some("无法保存迁移后的全局设置；当前修改仅保留在内存中。".into());
            }
            if state.revision == revision || !success {
                state.writing = false;
                break;
            }
        }
    });
}

pub async fn write_value(value: &Value) -> Result<(), String> {
    let text = format_value(value);
    #[cfg(any(target_arch = "wasm32", target_os = "android"))]
    {
        let literal = serde_json::to_string(&text).expect("settings text is serializable");
        let ok = document::eval(&format!(
            "try {{ localStorage.setItem({}, {}); dioxus.send(true); }} catch (_) {{ dioxus.send(false); }}",
            serde_json::to_string(STORAGE_KEY).expect("static storage key"),
            literal
        ))
        .recv::<bool>()
        .await
        .map_err(|_| "无法保存本设备全局设置。".to_owned())?;
        if ok {
            Ok(())
        } else {
            Err("无法保存本设备全局设置，当前修改仅保留在内存。".into())
        }
    }
    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    {
        let Some(path) = native_path() else {
            return Err("无法定位本设备全局设置文件。".into());
        };
        let parent = path.parent().ok_or("无法定位全局设置目录。")?;
        std::fs::create_dir_all(parent).map_err(|_| "无法创建全局设置目录。")?;
        let temporary = path.with_extension("json.tmp");
        std::fs::write(&temporary, text).map_err(|_| "无法写入全局设置临时文件。")?;
        std::fs::rename(&temporary, &path).map_err(|_| "无法替换全局设置文件。")?;
        Ok(())
    }
}

#[component]
pub fn AppSettingsPage() -> Element {
    let mut store = use_context::<Signal<AppSettingsState>>();
    let state = store.read().clone();

    let save = move |_| {
        if !state.ready || state.busy {
            return;
        }
        let source = store.peek().raw.clone();
        store.write().busy = true;
        store.write().error = None;
        store.write().notice = None;
        spawn(async move {
            match parse(&source) {
                Ok(value) => match write_value(&value).await {
                    Ok(()) => {
                        let mut state = store.write();
                        state.value = value.clone();
                        state.raw = format_value(&value);
                        state.busy = false;
                        state.error = None;
                        state.notice = Some("全局设置已保存。".into());
                    }
                    Err(message) => {
                        let mut state = store.write();
                        state.busy = false;
                        state.error = Some(message);
                    }
                },
                Err(message) => {
                    let mut state = store.write();
                    state.busy = false;
                    state.error = Some(message);
                }
            }
        });
    };
    let reset = move |_| {
        if !store.peek().ready || store.peek().busy {
            return;
        }
        spawn(async move {
            let confirmed = document::eval(
                "dioxus.send(window.confirm('恢复默认全局设置？当前编辑内容将被替换，但点击保存前不会写入磁盘。'))",
            )
            .recv::<bool>()
            .await
            .unwrap_or(false);
            if confirmed {
                let mut state = store.write();
                state.raw = default_text();
                state.error = None;
                state.notice =
                    Some("已恢复编辑器中的默认内容；点击保存后才会覆盖本地设置。".into());
            }
        });
    };

    rsx! {
        section { class: "app-settings-editor settings-panel", aria_label: "全局设置文件",
            h2 { "全局设置文件" }
            p { class: "app-settings-help", "控制本设备 Work 的菜单、面板和 UI 插件显示偏好；不会安装或启用节点资源。" }
            p { class: "app-settings-path", "存储位置：" {storage_label()} }
            if !state.ready { p { role: "status", "正在读取全局设置…" } }
            if let Some(error) = state.error.as_deref() { p { class: "app-settings-error", role: "alert", "{error}" } }
            if let Some(notice) = state.notice.as_deref() { p { class: "app-settings-notice", role: "status", "{notice}" } }
            textarea {
                class: "app-settings-json",
                aria_label: "全局设置 JSON",
                rows: 22,
                value: "{state.raw}",
                disabled: !state.ready || state.busy,
                oninput: move |event| {
                    let mut state = store.write();
                    state.raw = event.value();
                    state.error = None;
                    state.notice = None;
                }
            }
            div { class: "app-settings-actions",
                button { r#type: "button", disabled: !state.ready || state.busy, onclick: save, "保存设置" }
                button { r#type: "button", disabled: state.busy, onclick: reset, "恢复默认编辑内容" }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_document_is_valid_and_pretty_printed() {
        let text = default_text();
        assert!(text.ends_with('\n'));
        assert_eq!(parse(&text).unwrap(), default_value());
    }

    #[test]
    fn unknown_fields_are_retained() {
        let value = parse(r#"{"version":1,"menus":{},"future":{"keep":true}}"#).unwrap();
        assert_eq!(value["future"]["keep"], true);
    }

    #[test]
    fn provider_default_is_optional_and_reads_provider_model_pair() {
        assert_eq!(default_provider_model(&default_value()), None);
        let value =
            parse(r#"{"version":1,"defaultProvider":{"provider":"openai","model":"gpt-4o"}}"#)
                .unwrap();
        assert_eq!(
            default_provider_model(&value),
            Some(("openai".into(), "gpt-4o".into()))
        );
        let incomplete = parse(r#"{"version":1,"defaultProvider":{"provider":"openai"}}"#).unwrap();
        assert_eq!(default_provider_model(&incomplete), None);
    }

    #[test]
    fn session_display_defaults_to_grouped_and_accepts_flat() {
        assert_eq!(
            session_display_mode(&default_value()),
            SessionDisplayMode::GroupedByNode
        );
        let value = parse(r#"{"version":1,"sessionDisplay":{"mode":"flat"}}"#).unwrap();
        assert_eq!(session_display_mode(&value), SessionDisplayMode::Flat);
        let unknown = parse(r#"{"version":1,"sessionDisplay":{"mode":"other"}}"#).unwrap();
        assert_eq!(
            session_display_mode(&unknown),
            SessionDisplayMode::GroupedByNode
        );
    }

    #[test]
    fn menu_visibility_uses_default_for_unknown_entries() {
        let value = default_value();
        assert!(menu_visible(&value, "packages", true));
        assert!(!menu_visible(&value, "packages", false));
    }

    #[test]
    fn menu_order_uses_configured_integer() {
        let value = parse(r#"{"version":1,"menus":{"packages":{"order":-5}}}"#).unwrap();
        assert_eq!(menu_order(&value, "packages", 20), -5);
        assert_eq!(menu_order(&value, "skills", 30), 30);
    }

    #[test]
    fn contribution_visibility_uses_default_for_unknown_entries() {
        let value = parse(
            r#"{"version":1,"contributions":{"demo@1.0.0/status":{"visible":false,"order":-2}}}"#,
        )
        .unwrap();
        assert!(!contribution_visible(&value, "demo@1.0.0/status", true));
        assert!(contribution_visible(&value, "demo@1.0.0/other", true));
        assert_eq!(contribution_order(&value, "demo@1.0.0/status", 10), -2);
        assert_eq!(contribution_order(&value, "demo@1.0.0/other", 10), 10);
    }

    #[test]
    fn known_containers_must_be_objects() {
        assert!(parse(r#"{"version":1,"menus":[]}"#).is_err());
        assert!(parse(r#"{"version":1,"contributions":"bad"}"#).is_err());
    }
}
