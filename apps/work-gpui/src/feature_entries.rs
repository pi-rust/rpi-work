//! Device preferences for optional workbench entry points.

use serde_json::{Value, json};
use work_core::api::{NativeCatalog, NativeKind};
use work_viewmodel::resources::catalog::display_name;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Entry {
    Goal,
    Collaboration,
    Discovery,
    Mcp,
    Web,
    Chat,
    RpiHost,
}

impl Entry {
    pub const ALL: [Self; 7] = [
        Self::Goal,
        Self::Collaboration,
        Self::Discovery,
        Self::Mcp,
        Self::Web,
        Self::Chat,
        Self::RpiHost,
    ];

    pub const fn key(self) -> &'static str {
        match self {
            Self::Goal => "goal",
            Self::Collaboration => "collaboration",
            Self::Discovery => "discovery",
            Self::Mcp => "mcp",
            Self::Web => "web",
            Self::Chat => "chatIntegrations",
            Self::RpiHost => "rpiHost",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Goal => "目标模式",
            Self::Collaboration => "工作协同快捷入口",
            Self::Discovery => "发现附近节点",
            Self::Mcp => "MCP 服务页签",
            Self::Web => "Web 访问入口",
            Self::Chat => "接入聊天软件",
            Self::RpiHost => "rpi Host 管理",
        }
    }

    pub const fn default_visible(self) -> bool {
        matches!(self, Self::Discovery | Self::Mcp | Self::Web | Self::Chat)
            || (cfg!(unix) && matches!(self, Self::RpiHost))
    }
}

pub(super) struct Visibility([bool; Entry::ALL.len()]);

impl Default for Visibility {
    fn default() -> Self {
        Self(Entry::ALL.map(Entry::default_visible))
    }
}

impl Visibility {
    pub fn load(value: &Value) -> Self {
        Self(Entry::ALL.map(|entry| {
            value[entry.key()]
                .as_bool()
                .unwrap_or(entry.default_visible())
        }))
    }

    pub fn value(&self) -> Value {
        Value::Object(
            Entry::ALL
                .into_iter()
                .map(|entry| (entry.key().into(), json!(self.shows(entry))))
                .collect(),
        )
    }

    pub fn shows(&self, entry: Entry) -> bool {
        self.0[entry as usize]
    }

    pub fn toggle(&mut self, entry: Entry) {
        self.0[entry as usize] = !self.shows(entry);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Installation {
    Installed,
    Unverified,
    Missing,
}

pub(super) fn installation(catalog: &NativeCatalog, name: &str) -> Installation {
    let mut status = Installation::Missing;
    for resource in &catalog.resources {
        if resource.kind != NativeKind::Extension || !resource.name.is_ascii() {
            continue;
        }
        let stem = display_name(&resource.name);
        let stem = resource.facts["version"]
            .as_str()
            .and_then(|version| stem.strip_suffix(&format!("-v{version}")))
            .unwrap_or(&stem);
        let normalized = stem.replace('_', "-").to_ascii_lowercase();
        let key = normalized.strip_prefix("lib").unwrap_or(&normalized);
        if key != name && !(name == WEB_PACKAGE && key == "rpi-web-extension") {
            continue;
        }
        if resource.facts["installed"] == true {
            return Installation::Installed;
        }
        status = Installation::Unverified;
    }
    status
}

pub(super) const GOAL_PACKAGE: &str = "rpi-goal";
pub(super) const MCP_PACKAGES: [&str; 2] = ["rpi-mcp-adapter", "rpi-mcp-server"];
pub(super) const WEB_PACKAGE: &str = "rpi-web";
pub(super) const GOAL_INSTALL: &str = "rpi install rpi-goal";
pub(super) const GOAL_GUIDE: &str =
    "https://github.com/pi-rust/rpi-extension/tree/main/packages/rpi-goal";

#[cfg(test)]
mod tests {
    use super::*;
    use work_core::api::{NativeResource, NativeScope};

    #[test]
    fn existing_preferences_keep_existing_entries_and_new_entries_are_opt_in() {
        let mut visibility = Visibility::load(&json!({}));
        assert!(!visibility.shows(Entry::Goal));
        assert!(!visibility.shows(Entry::Collaboration));
        assert!(visibility.shows(Entry::Discovery));
        visibility.toggle(Entry::Goal);
        visibility.toggle(Entry::Mcp);
        let restored = Visibility::load(&visibility.value());
        assert!(restored.shows(Entry::Goal));
        assert!(!restored.shows(Entry::Mcp));
        assert!(restored.shows(Entry::Web));
    }

    #[test]
    fn goal_matches_native_library_names_but_preserves_unknown_installation() {
        let mut catalog = NativeCatalog {
            protects_saved_references: true,
            profile: Value::Null,
            diagnostics: Vec::new(),
            resources: vec![NativeResource {
                reference: "test".into(),
                scope: NativeScope::Global,
                kind: NativeKind::Extension,
                name: "lib_rpi_goal_demo".into(),
                facts: json!({"installed": true}),
            }],
        };
        assert_eq!(installation(&catalog, GOAL_PACKAGE), Installation::Missing);
        let resource = &mut catalog.resources[0];
        resource.name = "librpi_goal".into();
        resource.facts = json!({"installed": false});
        assert_eq!(
            installation(&catalog, GOAL_PACKAGE),
            Installation::Unverified
        );
        catalog.resources[0].facts["installed"] = json!(true);
        assert_eq!(
            installation(&catalog, GOAL_PACKAGE),
            Installation::Installed
        );
        assert_eq!(
            installation(&catalog, MCP_PACKAGES[0]),
            Installation::Missing
        );

        catalog.resources[0].name = "librpi_goal-v0.2.7-eb723ffab2084180b2abad77b7f0cc1a".into();
        assert_eq!(
            installation(&catalog, GOAL_PACKAGE),
            Installation::Installed
        );
        catalog.resources[0].name =
            "librpi_web_extension-v0.1.94-preview-5919918863b90e48c6391a56315f7c7d".into();
        catalog.resources[0].facts["version"] = json!("0.1.94-preview");
        assert_eq!(installation(&catalog, WEB_PACKAGE), Installation::Installed);
    }
}
