use super::*;

#[test]
fn extension_template_validates_and_publishes_as_an_inert_combined_design() {
    let sources = templates("Extension", "personal.review", "Review", "0.1.0");
    let mut resource = MetaResource {
        project_id: "project".into(),
        id: "personal.review".into(),
        name: "Review".into(),
        kind: "Extension".into(),
        version: "0.1.0".into(),
        revision: 1,
        baseline: sources.clone(),
        sources,
        validation: None,
        tests: Vec::new(),
        releases: Vec::new(),
    };
    assert!(validate(&resource).0);
    let package = design_package(&resource);
    let manifest = crate::features::extensions::design_manifest(
        &package["design"]["sources"],
        "personal.review",
        "0.1.0",
    )
    .unwrap()
    .unwrap();
    assert!(manifest.agent.is_some());
    assert!(manifest.ui.is_some());
    resource.sources.insert("extension.design.json".into(),
        r#"{"kind":"rpi-work-extension-design","id":"personal.review","version":"0.1.0","entry":"src/lib.rs"}"#.into());
    assert!(validate(&resource).0);
    resource
        .sources
        .insert("extension.design.json".into(), "{}".into());
    assert!(!validate(&resource).0);
}
