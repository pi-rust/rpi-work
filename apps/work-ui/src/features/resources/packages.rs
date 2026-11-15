//! Canonical public resource identities and explicit browser downloads.
use dioxus::prelude::*;
use work_core::api::{PackageBundle, PackageCatalog, PackageOffer, PackageRef};

pub(super) fn reference(offer: &PackageOffer) -> PackageRef {
    PackageRef {
        id: offer.manifest.id.clone(),
        version: offer.manifest.version.clone(),
        sha256: offer.sha256.clone(),
    }
}

pub(super) fn acquired(catalog: Option<&PackageCatalog>, target: &PackageRef) -> bool {
    catalog.is_some_and(|c| {
        c.packages
            .iter()
            .any(|p| reference(p) == *target && p.acquired)
    })
}

pub(super) async fn download_bundle(bundle: &PackageBundle) -> Result<(), String> {
    let text = serde_json::to_string_pretty(bundle).map_err(|e| e.to_string())?;
    let data = serde_json::to_string(&format!("{text}\n")).map_err(|e| e.to_string())?;
    // A constant filename avoids treating metadata as a browser-local path.
    let script = format!(
        "try {{ const blob=new Blob([{data}],{{type:'application/json;charset=utf-8'}}); const url=URL.createObjectURL(blob); const a=document.createElement('a'); a.href=url; a.download='curated-resource.rpi-package.json'; a.click(); setTimeout(()=>URL.revokeObjectURL(url),1000); dioxus.send({{ok:true}}); }} catch(error) {{ dioxus.send({{ok:false,error:String(error)}}); }}"
    );
    let result = document::eval(&script)
        .recv::<serde_json::Value>()
        .await
        .map_err(|e| format!("下载不可用：{e}"))?;
    if result["ok"] == true {
        Ok(())
    } else {
        Err(format!("下载失败：{}", result["error"]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_exact_authoritative_acquired_entry_unlocks_install() {
        let mut catalog: PackageCatalog = serde_json::from_value(serde_json::json!({
            "source": "embedded", "diagnostics": [], "packages": [{
                "manifest": {
                    "schemaVersion": 1, "id": "base", "name": "base", "version": "1.0.0",
                    "kind": "package", "title": "Base", "description": "Text only",
                    "author": "Work", "origin": "embedded", "instructions": []
                },
                "sha256": "digest", "fileCount": 1, "sizeBytes": 10, "acquired": true
            }]
        }))
        .unwrap();
        let target = reference(&catalog.packages[0]);
        assert!(acquired(Some(&catalog), &target));
        let mut other = target.clone();
        other.sha256 = "changed".into();
        assert!(!acquired(Some(&catalog), &other));
        other = target.clone();
        other.version = "2.0.0".into();
        assert!(!acquired(Some(&catalog), &other));
        other = target.clone();
        other.id = "developer".into();
        assert!(!acquired(Some(&catalog), &other));
        catalog.packages[0].acquired = false;
        assert!(!acquired(Some(&catalog), &target));
    }

    #[test]
    fn no_catalog_never_authorizes_acquisition() {
        let target = PackageRef {
            id: "base".into(),
            version: "1.0.0".into(),
            sha256: "digest".into(),
        };
        assert!(!acquired(None, &target));
    }
}
