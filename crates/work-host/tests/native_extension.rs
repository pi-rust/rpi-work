//! Run only with the isolated fixture environment provided by the smoke script.
use rpi_components::Call;
use serde_json::json;

#[tokio::test]
#[ignore = "requires RPI_TEST_ISOLATED plus an actual native rpi and HTTP extension"]
async fn desktop_host_is_a_supervised_client_of_the_native_extension() {
    assert_eq!(std::env::var("RPI_TEST_ISOLATED").as_deref(), Ok("1"));
    let selected = std::env::var("RPI_TEST_USE_SELECTED_RUNTIME").as_deref() == Ok("1");
    for name in [
        "RPI_WORK_DATA_DIR",
        "RPI_CODING_AGENT_DIR",
        "RPI_WORK_NODE_RPI_BIN",
    ] {
        assert!(std::env::var_os(name).is_some(), "Missing isolated {name}");
    }
    if !selected {
        assert!(std::env::var_os("RPI_WORK_NODE_EXTENSION").is_some());
    }
    let guard = work_host::start()
        .unwrap_or_else(|| panic!("{}", work_host::startup_error().unwrap_or("not ready")));
    let client = work_host::client().unwrap();
    assert!(matches!(client, work_client::WorkspaceClient::Remote(_)));
    assert!(client.is_local());
    let runtime = work_host::runtime::status().await.unwrap();
    assert!(runtime.running);
    assert_eq!(runtime.explicit_override, !selected);
    assert!(!runtime.restart_required);
    if selected {
        assert_eq!(runtime.selected.as_ref().unwrap().id, "rpi-http");
        let library = work_host::runtime::store()
            .unwrap()
            .selected()
            .unwrap()
            .unwrap()
            .library;
        work_host::runtime::restore_default().await.unwrap();
        let pending = work_host::runtime::status().await.unwrap();
        assert!(pending.running && pending.restart_required && pending.selected.is_none());
        assert!(
            library.is_file(),
            "Resetting future selection cannot remove the running DLL"
        );
    }
    assert!(work_host::start().is_none(), "Never create a second owner");
    let sharing = work_host::http::sharing().await;
    assert!(sharing.listening && sharing.enabled);
    assert!(
        sharing.password.is_empty(),
        "Compatibility view must not export a password"
    );
    let grant = work_host::http::issue_qr_authorization().await.unwrap();
    assert_eq!(grant.node_id, sharing.node_id);
    let project_root = std::path::PathBuf::from(std::env::var_os("RPI_WORK_DATA_DIR").unwrap())
        .join("exact-project");
    let project = client
        .create_project_directory("Native owner project", &project_root, false)
        .await
        .unwrap();
    assert!(
        client
            .projects()
            .await
            .unwrap()
            .iter()
            .any(|p| p.id == project.id)
    );
    client
        .component_call(
            "rpi-http",
            Call {
                action: "set-enabled".into(),
                arguments: json!({"enabled":false}),
            },
        )
        .await
        .unwrap();
    assert!(!work_host::http::sharing().await.enabled);
    assert!(
        client
            .projects()
            .await
            .unwrap()
            .iter()
            .any(|p| p.id == project.id)
    );
    assert!(work_host::http::issue_qr_authorization().await.is_err());
    client
        .component_call(
            "rpi-http",
            Call {
                action: "set-enabled".into(),
                arguments: json!({"enabled":true}),
            },
        )
        .await
        .unwrap();
    assert!(work_host::http::sharing().await.enabled);
    guard.checked_shutdown().unwrap();
    assert!(client.health().await.is_err());
    assert!(!work_host::runtime::status().await.unwrap().running);
}
