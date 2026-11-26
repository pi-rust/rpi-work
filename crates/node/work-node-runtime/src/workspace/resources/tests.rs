use super::{NativeBinding, ResourceObservation, immutable, json, observation_matches};
#[test]
fn fresh_native_null_operation_matches_typed_omitted_optional_binding() {
    let body = json!({"hostId":"real-generation","binding":{"revision":1,"fingerprint":"a".repeat(64),"operationId":null}});
    let binding: NativeBinding = serde_json::from_value(body["binding"].clone()).unwrap();
    assert!(
        serde_json::to_value(&binding)
            .unwrap()
            .get("operationId")
            .is_none()
    );
    let saved = ResourceObservation {
        generation: "real-generation".into(),
        session_id: "same-id".into(),
        binding,
        body: body.clone(),
    };
    assert!(observation_matches(&body, &saved));
    let mut old = body.clone();
    old["hostId"] = json!("old-generation");
    assert!(!observation_matches(&old, &saved));
    let mut stale = body.clone();
    stale["binding"]["revision"] = json!(2);
    assert!(!observation_matches(&stale, &saved));
    let mut invalid = body;
    invalid["binding"]["revision"] = json!("1");
    assert!(!observation_matches(&invalid, &saved));
}
#[test]
fn immutable_identity_never_overwrites_and_bounds_inputs() {
    let root = tempdir();
    let path = root.path().join("deep/file");
    immutable(&path, b"original").unwrap();
    immutable(&path, b"original").unwrap();
    assert!(immutable(&path, b"different").is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"original");
    assert!(
        immutable(
            &root.path().join("too-large"),
            &vec![0; 8 * 1024 * 1024 + 1]
        )
        .is_err()
    );
    assert!(!root.path().join("too-large").exists());
}
#[test]
fn symlink_or_junction_ancestor_cannot_escape_source_root() {
    let root = tempdir();
    let outside = tempdir();
    let link = root.path().join("escape");
    #[cfg(unix)]
    std::os::unix::fs::symlink(outside.path(), &link).unwrap();
    #[cfg(windows)]
    {
        assert!(
            std::process::Command::new("cmd.exe")
                .args(["/C", "mklink", "/J"])
                .arg(&link)
                .arg(outside.path())
                .output()
                .unwrap()
                .status
                .success()
        );
    }
    assert!(immutable(&link.join("should-not-exist"), b"escape").is_err());
    assert!(!outside.path().join("should-not-exist").exists());
    #[cfg(windows)]
    std::fs::remove_dir(link).unwrap();
}

fn tempdir() -> tempfile::TempDir {
    // macOS exposes its temp directory through /var, which is a system symlink.
    // Resolve that parent so explicit fixture links exercise the path checks.
    tempfile::Builder::new()
        .tempdir_in(std::env::temp_dir().canonicalize().unwrap())
        .unwrap()
}

#[test]
fn concurrent_project_resource_writes_compete_for_one_revision() {
    use super::{
        NativeApply, NativeSelection, NativeTools, commit_project_resource_intent,
        read_project_resource_intent,
    };
    use std::sync::Barrier;

    let directory = tempdir();
    let path = directory
        .path()
        .join("project-resource-config/project.json");
    let request = |operation: &str, revision, package: &str| NativeApply {
        expected_revision: revision,
        operation_id: operation.into(),
        supersedes_operation_id: None,
        target: NativeSelection {
            main_package: package.into(),
            package_instructions: Vec::new(),
            skills: Vec::new(),
            extensions: Vec::new(),
            tools: NativeTools::Default {},
            supplementary_instructions: String::new(),
        },
    };
    let first = request("first", 0, "package-a");
    let second = request("second", 0, "package-b");
    let barrier = Barrier::new(2);
    let [one, two] = std::thread::scope(|scope| {
        let (barrier, path) = (&barrier, &path);
        let start = |request| {
            scope.spawn(move || {
                barrier.wait();
                commit_project_resource_intent(path, request)
            })
        };
        let a = start(&first);
        let b = start(&second);
        [a.join().unwrap(), b.join().unwrap()]
    });
    let (winner, loser) = match (one, two) {
        (Ok(1), Err(_)) => (&first, &second),
        (Err(_), Ok(1)) => (&second, &first),
        outcomes => panic!("expected exactly one write, got {outcomes:?}"),
    };
    let saved = read_project_resource_intent(&path).unwrap().unwrap();
    assert_eq!(saved.revision, 1);
    assert_eq!(saved.operation_id, winner.operation_id);
    assert_eq!(saved.target, winner.target);
    assert_eq!(commit_project_resource_intent(&path, winner).unwrap(), 1);
    assert!(commit_project_resource_intent(&path, loser).is_err());
    let same_id = request(&winner.operation_id, 1, &loser.target.main_package);
    assert!(commit_project_resource_intent(&path, &same_id).is_err());
    let next = request("next", 1, &loser.target.main_package);
    assert_eq!(commit_project_resource_intent(&path, &next).unwrap(), 2);
    assert_eq!(
        read_project_resource_intent(&path).unwrap().unwrap().target,
        next.target
    );
}

#[test]
fn global_mutations_protect_other_projects_and_offline_session_intents() {
    use super::{
        LocalWorkspace, NativeApply, NativeKind, NativeResource, NativeScope, NativeSelection,
        NativeTools, commit_project_resource_intent, resource_reference,
    };

    let root = tempdir();
    let workspace = LocalWorkspace::new("unused".into(), root.path().join("node")).unwrap();
    let beta = workspace.registry_path("beta");
    std::fs::create_dir_all(beta.parent().unwrap()).unwrap();
    let facts = json!({"recordedDigest":"original","path":root.path().join("global/skill")});
    let mut skill = NativeResource {
        reference: resource_reference(
            "alpha",
            NativeScope::Global,
            NativeKind::Skill,
            "example",
            &facts,
        )
        .unwrap(),
        scope: NativeScope::Global,
        kind: NativeKind::Skill,
        name: "example".into(),
        facts,
    };
    let beta_reference =
        resource_reference("beta", skill.scope, skill.kind, &skill.name, &skill.facts).unwrap();
    assert_ne!(beta_reference, skill.reference);
    let mut request = NativeApply {
        expected_revision: 0,
        operation_id: "select-skill".into(),
        supersedes_operation_id: None,
        target: NativeSelection {
            main_package: "package".into(),
            package_instructions: Vec::new(),
            skills: vec![beta_reference],
            extensions: Vec::new(),
            tools: NativeTools::Default {},
            supplementary_instructions: String::new(),
        },
    };
    let configured = workspace.project_resource_path("beta");
    commit_project_resource_intent(&configured, &request).unwrap();
    assert!(
        workspace
            .check_resource_references("alpha", &skill)
            .unwrap_err()
            .to_string()
            .contains("saved project")
    );
    skill.scope = NativeScope::Project;
    assert!(workspace.check_resource_references("alpha", &skill).is_ok());
    skill.scope = NativeScope::Global;
    request.expected_revision = 1;
    request.operation_id = "unselect-skill".into();
    request.target.skills.clear();
    commit_project_resource_intent(&configured, &request).unwrap();
    assert!(workspace.check_resource_references("alpha", &skill).is_ok());

    let target = json!({"version":1,"main_package":{"name":"example-package","scope":"project"},"skills":[root.path().join("global/skill/SKILL.md")],"tools":{"mode":"default"}});
    let baseline = json!({"version":1,"tools":{"mode":"default"}});
    let registry = |selection, revision, operations| {
        json!({
            "sessions":[{"sessionId":"offline","projectId":"beta"}],
            "resources":{"offline":{"selection":selection,"revision":revision,"observation":null,"operations":operations}}
        })
    };
    let pending = json!({
        "request":{"expected_revision":0,"operation_id":"apply-skill","target":target},
        "transition":{"version":1,"operation_id":"apply-skill","expected":{"revision":0,"fingerprint":null},"target":target},
        "status":"pending","result_revision":null,"observation":null,"error":null
    });
    for snapshot in [
        registry(baseline, 0, json!([pending])),
        registry(target, 1, json!([])),
    ] {
        std::fs::write(&beta, serde_json::to_vec(&snapshot).unwrap()).unwrap();
        assert!(
            workspace
                .check_resource_references("alpha", &skill)
                .unwrap_err()
                .to_string()
                .contains("saved session")
        );
    }
}
