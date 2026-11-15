use std::collections::BTreeMap;
use work_core::api::{NativeCatalog, NativeKind, NativeResource, NativeScope, PackageOffer};

#[derive(Clone, Debug)]
pub(super) enum ResourceChoice {
    Installed(NativeResource),
    Public(PackageOffer),
}

impl PartialEq for ResourceChoice {
    fn eq(&self, other: &Self) -> bool {
        self.key() == other.key()
    }
}

impl ResourceChoice {
    pub fn kind(&self) -> NativeKind {
        match self {
            Self::Installed(resource) => resource.kind,
            Self::Public(offer) => offer.manifest.kind,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Self::Installed(resource) => &resource.name,
            Self::Public(offer) => &offer.manifest.title,
        }
    }

    pub fn native_name(&self) -> &str {
        match self {
            Self::Installed(resource) => &resource.name,
            Self::Public(offer) => &offer.manifest.name,
        }
    }

    pub fn version(&self) -> &str {
        match self {
            Self::Installed(resource) => resource.facts["version"].as_str().unwrap_or("未记录"),
            Self::Public(offer) => &offer.manifest.version,
        }
    }

    pub fn key(&self) -> String {
        serde_json::to_string(&(kind_key(self.kind()), self.native_name(), self.version()))
            .unwrap_or_default()
    }

    pub fn available<'a>(&self, catalog: &'a NativeCatalog) -> Option<&'a NativeResource> {
        catalog
            .resources
            .iter()
            .filter(|resource| resource.facts["installed"] == true && self.matches(resource))
            .max_by_key(|resource| resource.scope == NativeScope::Project)
    }

    pub fn matches(&self, resource: &NativeResource) -> bool {
        self.kind() == resource.kind
            && self.native_name() == resource.name
            && match self {
                Self::Public(offer) => {
                    resource.facts["version"].as_str() == Some(offer.manifest.version.as_str())
                }
                Self::Installed(original) => {
                    original.facts["version"] == resource.facts["version"]
                        && original.facts["recordedDigest"] == resource.facts["recordedDigest"]
                }
            }
    }
}

pub(super) fn kind_key(kind: NativeKind) -> &'static str {
    match kind {
        NativeKind::Extension => "extensions",
        NativeKind::Skill => "skills",
        NativeKind::Package => "packages",
    }
}

pub(super) fn kind_label(kind: NativeKind) -> &'static str {
    match kind {
        NativeKind::Extension => "扩展",
        NativeKind::Skill => "技能",
        NativeKind::Package => "Package",
    }
}

pub(super) fn kind_icon(kind: NativeKind) -> &'static str {
    match kind {
        NativeKind::Extension => "tool",
        NativeKind::Skill => "skills",
        NativeKind::Package => "blocks",
    }
}

pub(super) fn counts<'a>(kinds: impl Iterator<Item = &'a NativeKind>) -> [usize; 3] {
    let mut values = [0; 3];
    for kind in kinds {
        values[match kind {
            NativeKind::Extension => 0,
            NativeKind::Skill => 1,
            NativeKind::Package => 2,
        }] += 1;
    }
    values
}

/// A project inherits the node's global resources. Preserve versions and unknown
/// records separately, and prefer the project reference for the same verified bytes.
pub(super) fn installed_resources(
    catalog: &NativeCatalog,
    global_only: bool,
) -> Vec<NativeResource> {
    let mut versions = BTreeMap::<String, NativeResource>::new();
    for resource in catalog.resources.iter().filter(|resource| {
        resource.facts["installed"] == true
            && (!global_only || resource.scope == NativeScope::Global)
    }) {
        let key = serde_json::to_string(&(
            kind_key(resource.kind),
            &resource.name,
            &resource.facts["version"],
            &resource.facts["recordedDigest"],
        ))
        .unwrap_or_default();
        let entry = versions.entry(key).or_insert_with(|| resource.clone());
        if resource.scope == NativeScope::Project {
            *entry = resource.clone();
        }
    }
    versions.into_values().collect()
}

pub(super) fn installation_origin(
    resource: &NativeResource,
    catalog: &NativeCatalog,
) -> &'static str {
    let choice = ResourceChoice::Installed(resource.clone());
    let global = catalog.resources.iter().any(|candidate| {
        candidate.scope == NativeScope::Global
            && candidate.facts["installed"] == true
            && choice.matches(candidate)
    });
    match (resource.scope, global) {
        (NativeScope::Project, true) => "项目安装 · 全局也可用",
        (NativeScope::Project, false) => "项目安装",
        (NativeScope::Global, _) => "全局可用",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn resource(scope: NativeScope, version: &str, installed: Option<bool>) -> NativeResource {
        NativeResource {
            reference: format!("{scope:?}-{version}"),
            scope,
            kind: NativeKind::Extension,
            name: "review".into(),
            facts: json!({"version": version, "installed": installed, "recordedDigest": version}),
        }
    }

    #[test]
    fn scope_counts_share_the_version_preserving_list_and_ignore_unknown_installation() {
        let catalog = NativeCatalog {
            profile: json!({}),
            diagnostics: vec![],
            resources: vec![
                resource(NativeScope::Global, "1.0.0", Some(true)),
                resource(NativeScope::Project, "1.0.0", Some(true)),
                resource(NativeScope::Project, "2.0.0", Some(true)),
                resource(NativeScope::Global, "3.0.0", None),
                resource(NativeScope::Project, "4.0.0", Some(false)),
            ],
        };
        let effective = installed_resources(&catalog, false);
        assert_eq!(effective.len(), 2);
        assert!(
            effective
                .iter()
                .all(|resource| resource.scope == NativeScope::Project)
        );
        assert_eq!(installed_resources(&catalog, true).len(), 1);
        assert_eq!(
            counts(effective.iter().map(|resource| &resource.kind)),
            [2, 0, 0]
        );
    }
}
