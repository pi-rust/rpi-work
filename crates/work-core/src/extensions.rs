//! Extension contracts, not an extension loader.
//!
//! Agent code belongs to the execution node. UI contributions are optional,
//! inert data rendered by the client. Neither a local manifest nor a client
//! visibility preference is evidence that an agent extension has been loaded.
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const EXTENSION_KIND: &str = "rpi-work-extension-design";

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExtensionManifest {
    pub schema_version: u32,
    pub kind: String,
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<AgentExtension>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui: Option<UiExtension>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AgentExtension {
    /// A package-relative source/artifact declaration, never executed by a client.
    pub entry: String,
    pub runtime: AgentExtensionRuntime,
    #[serde(default)]
    pub tools: Vec<String>,
    /// Requested permissions, not grants. The execution node must enforce policy.
    #[serde(default)]
    pub permissions: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum AgentExtensionRuntime {
    RpiNative,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UiExtension {
    pub schema_version: u32,
    pub contributions: Vec<UiContribution>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct UiContribution {
    pub id: String,
    pub slot: UiSlot,
    pub title: String,
    pub widgets: Vec<UiWidget>,
}

/// Known host slots. Declaring a slot does not automatically mount a live view.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum UiSlot {
    SessionInspector,
    ComposerAction,
    ConversationCard,
    SettingsSection,
}

impl UiSlot {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::SessionInspector => "session-inspector",
            Self::ComposerAction => "composer-action",
            Self::ConversationCard => "conversation-card",
            Self::SettingsSection => "settings-section",
        }
    }
}

/// Read-only UI schema v1. No HTML, script, CSS, URL, action or recursive nodes.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(tag = "type", rename_all = "kebab-case", deny_unknown_fields)]
pub enum UiWidget {
    Text { text: String },
    Fields { fields: Vec<UiField> },
    Progress { label: String, value: u32, max: u32 },
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct UiField {
    pub label: String,
    pub value: String,
}

/// An immutable identity: a new version never changes an existing selection.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ExtensionPin {
    pub id: String,
    pub version: String,
}

impl ExtensionPin {
    /// A delimiter-safe key after validation. Do not key by display name.
    #[must_use]
    pub fn key(&self) -> String {
        format!("{}@{}", self.id, self.version)
    }

    /// Validate an immutable extension identity.
    ///
    /// # Errors
    /// Returns a diagnostic for invalid identifiers or unsupported version syntax.
    pub fn validate(&self) -> Result<(), String> {
        validate_id(&self.id)?;
        if self.version.len() > 32
            || self.version.split('.').count() != 3
            || self.version.split('.').any(|part| {
                part.is_empty()
                    || !part.bytes().all(|b| b.is_ascii_digit())
                    || part.parse::<u64>().is_err()
            })
        {
            return Err("扩展 version 必须是三段数字版本号，最多 32 字节".into());
        }
        Ok(())
    }
}

/// Device-local presentation only; never sent as an Agent enable/disable command.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UiExtensionPreference {
    pub extension: ExtensionPin,
    pub contribution_id: String,
    pub visible: bool,
}

/// These operations are negotiated independently from ordinary tool availability.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[allow(clippy::struct_excessive_bools)] // Independent negotiated operations, not lifecycle states.
pub struct ExtensionCapabilities {
    pub catalog: bool,
    pub installation: bool,
    pub session_binding: bool,
    pub runtime_observation: bool,
    pub live_ui: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AgentExtensionState {
    /// No observation. This is not equivalent to "not installed" or "disabled".
    Unknown,
    NotInstalled,
    Installed,
    Loaded,
    Failed {
        message: String,
    },
    RestartRequired,
}

/// Only a node response can populate this observation, scoped to that node.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NodeExtensionObservation {
    pub node_id: String,
    pub extension: ExtensionPin,
    pub state: AgentExtensionState,
}

/// Actual session configuration is separate from the client's desired pins.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppliedSessionExtensions {
    pub node_id: String,
    pub session_id: String,
    pub revision: u64,
    pub extensions: Vec<ExtensionPin>,
}

impl ExtensionManifest {
    #[must_use]
    pub fn pin(&self) -> ExtensionPin {
        ExtensionPin {
            id: self.id.clone(),
            version: self.version.clone(),
        }
    }

    /// Validate the supported inert design schema and all bounded declarations.
    ///
    /// # Errors
    /// Returns a diagnostic for unknown schemas, invalid identities, unsafe paths,
    /// duplicate declarations or out-of-bounds UI data.
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1 || self.kind != EXTENSION_KIND {
            return Err("需要 schemaVersion 1 的 rpi-work-extension-design".into());
        }
        self.pin().validate()?;
        validate_text(&self.name, 160)?;
        if self.agent.is_none() && self.ui.is_none() {
            return Err("扩展至少需要 agent 或 ui 声明".into());
        }
        if let Some(agent) = &self.agent {
            validate_entry(&agent.entry)?;
            validate_ids(&agent.tools)?;
            validate_ids(&agent.permissions)?;
        }
        if let Some(ui) = &self.ui {
            if ui.schema_version != 1 || ui.contributions.is_empty() || ui.contributions.len() > 16
            {
                return Err("UI 需要 schemaVersion 1 和 1–16 个 contribution".into());
            }
            let mut ids = BTreeSet::new();
            for contribution in &ui.contributions {
                validate_id(&contribution.id)?;
                if !ids.insert(&contribution.id) {
                    return Err("UI contribution ID 重复".into());
                }
                validate_text(&contribution.title, 160)?;
                if contribution.widgets.is_empty() || contribution.widgets.len() > 16 {
                    return Err("每个 UI contribution 需要 1–16 个 widget".into());
                }
                for widget in &contribution.widgets {
                    match widget {
                        UiWidget::Text { text } => validate_text(text, 4096)?,
                        UiWidget::Fields { fields } => {
                            if fields.is_empty() || fields.len() > 20 {
                                return Err("UI fields 需要 1–20 项".into());
                            }
                            for field in fields {
                                validate_text(&field.label, 160)?;
                                validate_text(&field.value, 4096)?;
                            }
                        }
                        UiWidget::Progress { label, value, max } => {
                            validate_text(label, 160)?;
                            if *max == 0 || value > max {
                                return Err("UI progress 需要 0 ≤ value ≤ max 且 max > 0".into());
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

fn validate_id(id: &str) -> Result<(), String> {
    if id.is_empty()
        || id.len() > 90
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
        || matches!(id, "." | ".." | "__proto__" | "constructor" | "prototype")
    {
        return Err("扩展标识只能包含字母、数字、点、下划线或连字符，最多 90 字节".into());
    }
    Ok(())
}

fn validate_ids(ids: &[String]) -> Result<(), String> {
    if ids.len() > 64 {
        return Err("扩展工具或权限声明最多 64 项".into());
    }
    let mut seen = BTreeSet::new();
    for id in ids {
        validate_id(id)?;
        if !seen.insert(id) {
            return Err("扩展工具或权限声明重复".into());
        }
    }
    Ok(())
}

fn validate_text(text: &str, max: usize) -> Result<(), String> {
    if text.trim().is_empty() || text.len() > max {
        return Err(format!("扩展文本为空或超过 {max} 字节"));
    }
    Ok(())
}

fn validate_entry(entry: &str) -> Result<(), String> {
    validate_text(entry, 240)?;
    if entry.contains(['\\', ':'])
        || entry.chars().any(char::is_control)
        || entry
            .split('/')
            .any(|part| part.is_empty() || matches!(part, "." | ".."))
    {
        return Err("Agent entry 必须是包内相对路径，不能包含绝对路径或目录穿越".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn manifest(agent: bool, ui: bool) -> ExtensionManifest {
        let mut value = json!({"schemaVersion":1,"kind":EXTENSION_KIND,"id":"team.index","name":"Index","version":"1.0.0"});
        if agent {
            value["agent"] = json!({"runtime":"rpi-native","entry":"src/lib.rs","tools":["search"],"permissions":["project.read"]});
        }
        if ui {
            value["ui"] = json!({"schemaVersion":1,"contributions":[{"id":"status","slot":"session-inspector","title":"Index status","widgets":[{"type":"text","text":"<script>alert(1)</script>"}]}]});
        }
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn agent_ui_and_combined_are_independent() {
        for (agent, ui) in [(true, false), (false, true), (true, true)] {
            let item = manifest(agent, ui);
            item.validate().unwrap();
            let roundtrip: ExtensionManifest =
                serde_json::from_value(serde_json::to_value(&item).unwrap()).unwrap();
            assert_eq!(roundtrip, item);
        }
        assert!(manifest(false, false).validate().is_err());
    }

    #[test]
    fn unsupported_ui_fields_and_versions_never_become_executable() {
        let original = serde_json::to_value(manifest(true, true)).unwrap();
        for (pointer, value) in [
            ("/ui/contributions/0/slot", json!("full-screen")),
            ("/ui/contributions/0/widgets/0/type", json!("html")),
            ("/agent/runtime", json!("javascript")),
        ] {
            let mut invalid = original.clone();
            *invalid.pointer_mut(pointer).unwrap() = value;
            assert!(serde_json::from_value::<ExtensionManifest>(invalid).is_err());
        }
        let mut invalid = original;
        invalid["ui"]["contributions"][0]["widgets"][0]["script"] = json!("alert(1)");
        assert!(serde_json::from_value::<ExtensionManifest>(invalid).is_err());
        let mut item = manifest(true, true);
        item.ui.as_mut().unwrap().schema_version = 2;
        assert!(item.validate().is_err());
    }

    #[test]
    fn bounds_paths_progress_and_duplicate_ids_are_checked() {
        for entry in [
            "../secret",
            "/absolute",
            "C:/file",
            "a\\file",
            "a//file",
            "./file",
        ] {
            let mut item = manifest(true, false);
            item.agent.as_mut().unwrap().entry = entry.into();
            assert!(item.validate().is_err(), "{entry}");
        }
        let mut item = manifest(false, true);
        let ui = item.ui.as_mut().unwrap();
        ui.contributions.push(ui.contributions[0].clone());
        assert!(item.validate().is_err());
        let mut item = manifest(false, true);
        item.ui.as_mut().unwrap().contributions[0].widgets = vec![UiWidget::Progress {
            label: "Work".into(),
            value: 2,
            max: 1,
        }];
        assert!(item.validate().is_err());
        item.ui.as_mut().unwrap().contributions[0].widgets = vec![UiWidget::Text {
            text: "a".repeat(4097),
        }];
        assert!(item.validate().is_err());
        assert!(
            ExtensionPin {
                id: "invalid@id".into(),
                version: "1.0.0".into()
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn hiding_ui_is_not_an_agent_state_change() {
        let manifest = manifest(true, true);
        let observation = NodeExtensionObservation {
            node_id: "pc".into(),
            extension: manifest.pin(),
            state: AgentExtensionState::Loaded,
        };
        let mut preference = UiExtensionPreference {
            extension: manifest.pin(),
            contribution_id: "status".into(),
            visible: true,
        };
        preference.visible = false;
        assert_eq!(observation.state, AgentExtensionState::Loaded);
        assert_ne!(
            AgentExtensionState::Unknown,
            AgentExtensionState::NotInstalled
        );
        assert_eq!(
            ExtensionCapabilities::default(),
            ExtensionCapabilities {
                catalog: false,
                installation: false,
                session_binding: false,
                runtime_observation: false,
                live_ui: false
            }
        );
        assert!(
            serde_json::to_value(preference)
                .unwrap()
                .get("agent")
                .is_none()
        );
        let _: Value = serde_json::to_value(manifest).unwrap();
    }
}
