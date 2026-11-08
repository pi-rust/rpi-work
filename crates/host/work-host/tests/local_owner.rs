//! This process needs only an isolated Work data directory and an RPC fixture.
use serde_json::json;
use work_component_api::Call;

#[tokio::test]
#[ignore = "requires RPI_TEST_ISOLATED and an isolated RPC fixture"]
async fn desktop_owns_the_workspace_without_a_node_extension() {
    assert_eq!(std::env::var("RPI_TEST_ISOLATED").as_deref(), Ok("1"));
    for name in [
        "RPI_WORK_DATA_DIR",
        "RPI_CODING_AGENT_DIR",
        "RPI_WORK_RPI_BIN",
    ] {
        assert!(std::env::var_os(name).is_some(), "Missing isolated {name}");
    }
    tokio::task::LocalSet::new()
        .run_until(async {
            let guard = work_host::start()
                .unwrap_or_else(|| panic!("{}", work_host::startup_error().unwrap_or("not ready")));
            let client = work_host::client().unwrap();
            let work_client::WorkspaceClient::Local(owner) = &client else {
                panic!("Work must own a local workspace, not supervise an HTTP plugin");
            };
            assert!(std::sync::Arc::ptr_eq(
                &owner.node_owner(),
                &work_node_host::client().unwrap()
            ));
            assert!(work_host::start().is_none(), "Never create a second owner");
            assert!(client.is_local());
            let sharing = work_host::http::sharing().await;
            assert!(sharing.listening && sharing.enabled);
            assert!(
                sharing.password.is_empty(),
                "Do not expose passwords in the compatibility view"
            );
            let grant = work_host::http::issue_qr_authorization().await.unwrap();
            assert_eq!(grant.node_id, sharing.node_id);
            let project_root =
                std::path::PathBuf::from(std::env::var_os("RPI_WORK_DATA_DIR").unwrap())
                    .join("exact-project");
            let project = client
                .create_project_directory("Work-owned project", &project_root, false)
                .await
                .unwrap();
            let remote = work_client::WorkClient::new(&sharing.address).unwrap();
            remote.login("isolated-owner-test").await.unwrap();
            assert!(
                remote
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
            assert!(remote.projects().await.is_err());
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
        })
        .await;
}
