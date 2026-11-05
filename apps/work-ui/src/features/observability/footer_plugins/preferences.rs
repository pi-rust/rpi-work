//! App-local preferences. Loading never writes. Only explicit settings events persist.
use super::registry::BUILTINS;
use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::future::{Future, poll_fn};
use std::pin::pin;
use std::task::Poll;

pub const STORAGE_KEY: &str = "rpi-work-footer-v1";
const MAX_BYTES: usize = 32 * 1024;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginPreference {
    pub id: String,
    pub enabled: bool,
    pub hidden_items: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FooterPreferences {
    pub version: u32,
    pub plugins: Vec<PluginPreference>,
}
impl Default for FooterPreferences {
    fn default() -> Self {
        Self {
            version: 1,
            plugins: BUILTINS
                .iter()
                .map(|p| PluginPreference {
                    id: p.id.into(),
                    enabled: p.id != "session-identity",
                    hidden_items: Vec::new(),
                })
                .collect(),
        }
    }
}
impl FooterPreferences {
    /// A complete ordered vector is required; no silently dropped unknown settings.
    pub fn parse(raw: &str) -> Result<Self, &'static str> {
        if raw.len() > MAX_BYTES {
            return Err("状态栏配置超过 32KB；请恢复默认状态栏。");
        }
        let value: Self =
            serde_json::from_str(raw).map_err(|_| "状态栏配置 JSON 无效；请恢复默认状态栏。")?;
        value.validate()?;
        Ok(value)
    }
    fn validate(&self) -> Result<(), &'static str> {
        if self.version != 1 || self.plugins.len() != BUILTINS.len() {
            return Err("状态栏配置版本或插件列表无效；请恢复默认状态栏。");
        }
        let mut plugins = BTreeSet::new();
        for preference in &self.plugins {
            let plugin = BUILTINS
                .iter()
                .find(|p| p.id == preference.id)
                .ok_or("状态栏配置含未知插件；请恢复默认状态栏。")?;
            if !plugins.insert(&preference.id) {
                return Err("状态栏配置含重复插件；请恢复默认状态栏。");
            }
            let mut items = BTreeSet::new();
            for id in &preference.hidden_items {
                if !plugin.items.iter().any(|i| i.id == id) || !items.insert(id) {
                    return Err("状态栏配置含未知或重复项目；请恢复默认状态栏。");
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FooterPreferenceStore {
    pub settings: FooterPreferences,
    pub ready: bool,
    pub storage_error: Option<String>,
    /// Invalid/unreadable storage is never overwritten until an explicit reset.
    pub corrupt_guard: bool,
    revision: u64,
    writing: bool,
}
#[derive(Clone, Copy)]
pub struct FooterPreferencesContext {
    pub store: Signal<FooterPreferenceStore>,
}

/// A missing `WebView` reply must not leave loading or the single writer stuck.
/// The pending receive is dropped on timeout; storage errors use the normal fallback.
async fn bridge_reply<T>(future: impl Future<Output = T>) -> Result<T, ()> {
    let mut reply = pin!(future);
    let mut timeout = pin!(async {
        #[cfg(target_arch = "wasm32")]
        gloo_timers::future::TimeoutFuture::new(1_000).await;
        #[cfg(not(target_arch = "wasm32"))]
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    });
    poll_fn(move |cx| {
        if let Poll::Ready(value) = reply.as_mut().poll(cx) {
            return Poll::Ready(Ok(value));
        }
        if timeout.as_mut().poll(cx).is_ready() {
            return Poll::Ready(Err(()));
        }
        Poll::Pending
    })
    .await
}

/// Call once in the App component. This hook does not subscribe App to settings.
pub fn use_footer_preferences() {
    let store = use_signal(FooterPreferenceStore::default);
    use_context_provider(|| FooterPreferencesContext { store });
    use_future(move || async move {
        let mut store = store;
        // Fixed bridge only: never evaluate provider output or imported source.
        let result = bridge_reply(document::eval(r"try { const v = localStorage.getItem('rpi-work-footer-v1'); dioxus.send({ok:true, raw:v}); } catch (_) { dioxus.send({ok:false, raw:null}); }").recv::<StorageRead>()).await;
        let loaded = match result {
            Ok(Ok(StorageRead {
                ok: true,
                raw: None,
            })) => Ok(FooterPreferences::default()),
            Ok(Ok(StorageRead {
                ok: true,
                raw: Some(raw),
            })) => FooterPreferences::parse(&raw).map_err(str::to_owned),
            _ => Err("无法读取本地状态栏配置；内存设置可用，恢复默认后可重试保存。".into()),
        };
        let mut state = store.write();
        match loaded {
            Ok(settings) => state.settings = settings,
            Err(error) => {
                state.storage_error = Some(error);
                state.corrupt_guard = true;
            }
        }
        state.ready = true;
    });
}
#[derive(Deserialize)]
struct StorageRead {
    ok: bool,
    raw: Option<String>,
}

/// Mutation is confined to explicit controls. A single writer coalesces rapid
/// changes and drains the latest revision, preventing stale asynchronous writes.
pub(super) fn change(
    mut context: FooterPreferencesContext,
    reset: bool,
    update: impl FnOnce(&mut FooterPreferences),
) {
    {
        let mut state = context.store.write();
        if !state.ready {
            return;
        }
        if reset {
            state.settings = FooterPreferences::default();
            state.corrupt_guard = false;
            state.storage_error = None;
        } else {
            let mut next = state.settings.clone();
            update(&mut next);
            if next.validate().is_err() {
                return;
            }
            state.settings = next;
        }
        state.revision = state.revision.wrapping_add(1);
        if state.corrupt_guard || state.writing {
            return;
        }
        state.writing = true;
    }
    spawn(async move {
        loop {
            let (settings, revision) = {
                let state = context.store.peek();
                (state.settings.clone(), state.revision)
            };
            let success = if let Ok(json) = serde_json::to_string(&settings) {
                // Encode JSON *as a JS string literal*, never interpolate raw values.
                let literal = serde_json::to_string(&json).expect("string serialization");
                let script = format!(
                    "try {{ localStorage.setItem('{STORAGE_KEY}', {literal}); dioxus.send(true); }} catch (_) {{ dioxus.send(false); }}"
                );
                matches!(
                    bridge_reply(document::eval(&script).recv::<bool>()).await,
                    Ok(Ok(true))
                )
            } else {
                false
            };
            let mut state = context.store.write();
            state.storage_error = if success {
                None
            } else {
                Some("无法保存本地状态栏配置；本次设置仅保留在内存中。".into())
            };
            if state.revision == revision || state.corrupt_guard {
                state.writing = false;
                break;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_hides_only_identity_and_preserves_saved_all_enabled() {
        let defaults = FooterPreferences::default();
        assert_eq!(defaults.version, 1);
        assert_eq!(defaults.plugins.len(), BUILTINS.len());
        for plugin in &defaults.plugins {
            assert_eq!(plugin.enabled, plugin.id != "session-identity");
            assert_eq!(plugin.hidden_items, Vec::<String>::new());
        }
        let saved = r#"{"version":1,"plugins":[{"id":"session-identity","enabled":true,"hidden_items":[]},{"id":"session-usage","enabled":true,"hidden_items":[]},{"id":"context-usage","enabled":true,"hidden_items":[]},{"id":"tool-activity","enabled":true,"hidden_items":[]}]}"#;
        let parsed = FooterPreferences::parse(saved).unwrap();
        assert!(parsed.plugins.iter().all(|plugin| plugin.enabled));
        assert_eq!(serde_json::to_string(&parsed).unwrap(), saved);
    }
    #[test]
    fn exact_schema_roundtrip_and_order() {
        let mut p = FooterPreferences::default();
        p.plugins.swap(0, 3);
        p.plugins[0].enabled = false;
        p.plugins[1].hidden_items.push("cost".into());
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.starts_with(
            r#"{"version":1,"plugins":[{"id":"tool-activity","enabled":false,"hidden_items":[]}"#
        ));
        assert_eq!(FooterPreferences::parse(&json), Ok(p));
    }
    #[test]
    fn rejects_corruption_without_silently_repairing() {
        let default = serde_json::to_value(FooterPreferences::default()).unwrap();
        for changed in [
            {
                let mut v = default.clone();
                v["version"] = 2.into();
                v
            },
            {
                let mut v = default.clone();
                v["plugins"][0]["id"] = "unknown".into();
                v
            },
            {
                let mut v = default.clone();
                v["plugins"][1] = v["plugins"][0].clone();
                v
            },
            {
                let mut v = default.clone();
                v["plugins"][0]["hidden_items"] = serde_json::json!(["identity", "identity"]);
                v
            },
            {
                let mut v = default.clone();
                v["plugins"][0]["hidden_items"] = serde_json::json!(["cost"]);
                v
            },
            {
                let mut v = default.clone();
                v["extra"] = true.into();
                v
            },
        ] {
            assert!(FooterPreferences::parse(&changed.to_string()).is_err());
        }
        assert!(FooterPreferences::parse(&" ".repeat(MAX_BYTES + 1)).is_err());
        assert!(FooterPreferences::parse("not json").is_err());
    }
}
