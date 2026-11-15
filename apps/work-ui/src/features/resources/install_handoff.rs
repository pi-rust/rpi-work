use dioxus::prelude::*;
use work_core::api::PackageRef;

#[derive(Clone, Copy)]
pub(crate) struct InstallDraft(pub Signal<Option<Vec<PackageRef>>>);

pub(crate) fn use_install_draft(#[allow(unused_mut)] mut page: Signal<String>) {
    #[allow(unused_mut)]
    let mut draft = use_signal(|| None::<Vec<PackageRef>>);
    use_context_provider(|| InstallDraft(draft));
    #[cfg(target_arch = "wasm32")]
    use_future(move || async move {
        let raw = document::eval(r#"
            const marker = new URLSearchParams(location.hash.slice(1));
            const raw = marker.get('rpi-install');
            if (marker.has('rpi-install')) {
                marker.delete('rpi-install');
                const hash = marker.toString();
                history.replaceState(history.state, '', location.pathname + location.search + (hash ? '#' + hash : ''));
            }
            dioxus.send(raw);
        "#).recv::<Option<String>>().await.ok().flatten();
        if let Some(packages) = raw.as_deref().and_then(parse_draft) {
            draft.set(Some(packages));
            page.set("resources".into());
        }
    });
    let _ = page;
}

#[cfg(any(target_arch = "wasm32", test))]
fn parse_draft(raw: &str) -> Option<Vec<PackageRef>> {
    if raw.len() > 32_768 {
        return None;
    }
    let packages: Vec<PackageRef> = serde_json::from_str(raw).ok()?;
    (!packages.is_empty()
        && packages.len() <= 32
        && packages.iter().all(|p| {
            !p.id.trim().is_empty()
                && p.id.len() <= 256
                && !p.version.trim().is_empty()
                && p.version.len() <= 128
                && p.sha256.len() == 64
                && p.sha256.bytes().all(|b| b.is_ascii_hexdigit())
        }))
    .then_some(packages)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handoff_accepts_bounded_public_identities_without_install_authorization() {
        let package = PackageRef {
            id: "test.review".into(),
            version: "1.0.0".into(),
            sha256: "a".repeat(64),
        };
        assert_eq!(
            parse_draft(&serde_json::to_string(&vec![package.clone()]).unwrap()),
            Some(vec![package])
        );
        for raw in [
            "[]",
            "{}",
            "[{\"id\":\"x\",\"version\":\"1\",\"sha256\":\"bad\"}]",
            "[{\"id\":\"x\",\"version\":\"1\",\"sha256\":\"bad\",\"trusted\":true}]",
        ] {
            assert!(parse_draft(raw).is_none());
        }
    }
}
