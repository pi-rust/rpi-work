use super::*;

fn package(id: &str, version: &str, agent: bool, ui: bool) -> Value {
    let mut manifest = json!({"schemaVersion":1,"kind":"rpi-work-extension-design","id":id,"version":version,"name":"Review extension"});
    if agent {
        manifest["agent"] = json!({"runtime":"rpi-native","entry":"src/lib.rs","tools":["review"],"permissions":["project.read"]});
    }
    if ui {
        manifest["ui"] = json!({"schemaVersion":1,"contributions":[{"id":"status","slot":"session-inspector","title":"Review status","widgets":[{"type":"text","text":"Static design, not running"}]}]});
    }
    json!({"schemaVersion":1,"kind":"rpi-work-design-package","id":id,"version":version,"name":"Review extension","author":"Test","description":"Extension design","resources":{"skills":[],"tools":[],"prompts":[]},"design":{"resourceKind":"Extension","sources":{"extension.design.json":manifest.to_string(),"src/lib.rs":"// inert source"}},"extra":{"preserve":true}})
}

#[test]
fn new_designs_roundtrip_without_claiming_node_installation() {
    for (agent, ui) in [(true, false), (false, true), (true, true)] {
        let document = package("review.test", "1.0.0", agent, ui);
        let mut library = ResourceLibrary::default();
        import_design_package(&mut library, &document).unwrap();
        let mut saved = library.to_value();
        saved["extensions"][0]["node_installed"] = json!(true);
        saved["extensions"][0]["manifest"] = json!({"ui":"malicious stored duplicate"});
        let restored = ResourceLibrary::from_value(&saved).unwrap();
        let manifest = restored.extensions[0].manifest.as_ref().unwrap();
        assert_eq!(manifest.agent.is_some(), agent);
        assert_eq!(manifest.ui.is_some(), ui);
        assert_eq!(package_manifest_value(&restored.packages[0]), document);
        assert_eq!(
            extension_state(&restored.extensions[0]),
            if agent {
                "已获取设计 · 节点状态未知"
            } else {
                "已获取 UI 设计 · 客户端预览"
            }
        );
        assert!(
            restored.to_value()["extensions"][0]
                .get("node_installed")
                .is_none()
        );
    }
}

#[test]
fn visibility_does_not_change_agent_selection_revision_or_published_sources() {
    let mut library = ResourceLibrary::default();
    let original = package("review.test", "1.0.0", true, true);
    import_design_package(&mut library, &original).unwrap();
    let pin = library.extensions[0].manifest.as_ref().unwrap().pin();
    set_extension_selected(library.session_mut("node-a/session"), &pin, true);
    let selection = library.sessions.clone();
    set_ui_visible(&mut library, &pin, "status", false);
    assert_eq!(library.sessions, selection);
    assert_eq!(package_manifest_value(&library.packages[0]), original);
    let mut restored = ResourceLibrary::from_value(&library.to_value()).unwrap();
    assert!(!restored.ui_preferences[0].visible);
    assert_eq!(restored.sessions, selection);
    set_extension_selected(restored.session_mut("node-a/session"), &pin, false);
    assert!(!restored.ui_preferences[0].visible);
}

#[test]
fn agent_pins_are_fixed_by_version_and_scoped_by_node_and_session() {
    let mut library = ResourceLibrary::default();
    import_design_package(&mut library, &package("review.test", "1.0.0", true, true)).unwrap();
    let old = library.extensions[0].manifest.as_ref().unwrap().pin();
    set_extension_selected(library.session_mut("node-a/session"), &old, true);
    import_design_package(&mut library, &package("review.test", "2.0.0", true, true)).unwrap();
    assert_eq!(library.sessions[0].extension_pins, vec![old.clone()]);
    assert_eq!(
        library.session_mut("node-b/session").extension_pins,
        Vec::<ExtensionPin>::new()
    );
    let new = library.extensions[1].manifest.as_ref().unwrap().pin();
    set_extension_selected(library.session_mut("node-a/session"), &new, true);
    assert_eq!(library.sessions[0].extension_pins, vec![new]);
    assert_eq!(
        library.sessions[1].extension_pins,
        Vec::<ExtensionPin>::new()
    );
    let restored = ResourceLibrary::from_value(&library.to_value()).unwrap();
    assert_eq!(restored.sessions, library.sessions);
}

#[test]
fn invalid_new_ui_is_rejected_atomically_and_legacy_remains_inert() {
    let mut library = ResourceLibrary::default();
    let mut invalid = package("review.test", "1.0.0", true, true);
    let mut manifest: Value = serde_json::from_str(
        invalid["design"]["sources"]["extension.design.json"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    manifest["ui"]["contributions"][0]["widgets"][0]["type"] = json!("html");
    invalid["design"]["sources"]["extension.design.json"] = json!(manifest.to_string());
    let before = library.to_value();
    assert!(import_design_package(&mut library, &invalid).is_err());
    assert_eq!(library.to_value(), before);
    invalid["design"]["sources"]
        .as_object_mut()
        .unwrap()
        .remove("extension.design.json");
    import_design_package(&mut library, &invalid).unwrap();
    assert!(library.extensions[0].manifest.is_none());
}

#[test]
fn old_sessions_default_to_no_agent_pins_or_ui_preferences() {
    let old =
        json!({"sessions":[{"session_id":"node/session","revision":5,"instructions":"keep"}]});
    let restored = ResourceLibrary::from_value(&old).unwrap();
    assert_eq!(
        restored.sessions[0].extension_pins,
        Vec::<ExtensionPin>::new()
    );
    assert_eq!(restored.ui_preferences, Vec::<UiExtensionPreference>::new());
    assert_eq!(restored.sessions[0].revision, 5);
}
