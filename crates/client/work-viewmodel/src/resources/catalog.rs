use std::collections::BTreeMap;
use work_core::api::{NativeCatalog, NativeKind, NativeResource, NativeScope, PackageOffer};

#[derive(Clone, Debug)]
pub enum ResourceChoice {
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

    pub fn name(&self) -> String {
        match self {
            Self::Installed(resource) => display_name(&resource.name),
            Self::Public(offer) => offer.manifest.title.clone(),
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

fn valid_version(version: &str) -> bool {
    !version.is_empty()
        && version.matches('.').count() >= 1
        && version.chars().all(|ch| ch.is_ascii_digit() || ch == '.')
}

fn is_hex(value: &str, minimum: usize) -> bool {
    value.len() >= minimum && value.chars().all(|ch| ch.is_ascii_hexdigit())
}

fn is_uuid(value: &str) -> bool {
    let parts = value.split('-').collect::<Vec<_>>();
    parts.len() == 5
        && parts[0].len() == 8
        && parts[1..4].iter().all(|part| part.len() == 4)
        && parts[4].len() == 12
        && parts.iter().all(|part| is_hex(part, 1))
}

fn looks_like_digest(value: &str, minimum: usize) -> bool {
    is_uuid(value) || is_hex(value, minimum)
}

fn strip_uuid_suffix(name: &str) -> Option<&str> {
    let start = name.len().checked_sub(36)?;
    if start == 0 || !name[start - 1..].starts_with('-') || !is_uuid(&name[start..]) {
        return None;
    }
    Some(&name[..start - 1])
}

/// Installed names may carry a native version/digest suffix such as
/// `rpi_acp-v0.2.13-<digest>` or `librpi_acp-v0.2.17`. Keep the raw name for
/// management and matching; only show the stable identity in the UI.
pub fn display_name(name: &str) -> String {
    if let Some(marker) = name.rfind("-v") {
        if marker > 0 {
            let suffix = &name[marker + 2..];
            let (version, digest) = suffix
                .split_once('-')
                .map_or((suffix, None), |(version, digest)| (version, Some(digest)));
            if valid_version(version)
                && digest.is_none_or(|digest| digest.is_empty() || looks_like_digest(digest, 8))
            {
                return name[..marker].to_owned();
            }
        }
    }
    if let Some(base) = strip_uuid_suffix(name) {
        return base.to_owned();
    }
    if let Some(marker) = name.rfind('-') {
        let suffix = &name[marker + 1..];
        if marker > 0 && looks_like_digest(suffix, 16) {
            return name[..marker].to_owned();
        }
    }
    name.to_owned()
}

pub fn kind_key(kind: NativeKind) -> &'static str {
    match kind {
        NativeKind::Extension => "extensions",
        NativeKind::Skill => "skills",
        NativeKind::Package => "packages",
    }
}

pub fn kind_label(kind: NativeKind) -> &'static str {
    match kind {
        NativeKind::Extension => "扩展",
        NativeKind::Skill => "技能",
        NativeKind::Package => "Package",
    }
}

pub fn kind_icon(kind: NativeKind) -> &'static str {
    match kind {
        NativeKind::Extension => "tool",
        NativeKind::Skill => "skills",
        NativeKind::Package => "blocks",
    }
}

pub fn counts<'a>(kinds: impl Iterator<Item = &'a NativeKind>) -> [usize; 3] {
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
pub fn installed_resources(catalog: &NativeCatalog, global_only: bool) -> Vec<NativeResource> {
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

pub fn installation_origin(resource: &NativeResource, catalog: &NativeCatalog) -> &'static str {
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

    #[test]
    fn display_name_hides_native_version_and_digest_suffixes() {
        assert_eq!(
            display_name("rpi_acp-v0.2.13-37410c15e8a849f2aaeebce774f156d4"),
            "rpi_acp"
        );
        assert_eq!(display_name("librpi_acp-v0.2.17"), "librpi_acp");
        assert_eq!(
            display_name("review-01a1da59-9401-7673-860c-e386f83b5a0f"),
            "review"
        );
        assert_eq!(
            display_name("review-37410c15e8a849f2aaeebce774f156d4"),
            "review"
        );
        assert_eq!(display_name("rpi_ask_user"), "rpi_ask_user");
        assert_eq!(
            display_name("custom-vnext-not-a-digest"),
            "custom-vnext-not-a-digest"
        );
    }

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
