use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use work_client::{WorkClient, WorkspaceClient};
use work_core::components::{Action, Bundle, Call, InstallRequest, Manifest, Panel};
use work_node_runtime::workspace::LocalWorkspace;
use work_node_service::{HttpServer, ServerConfig};

struct SharingOwner {
    control: work_node_service::SharingControl,
    address: String,
}
impl work_http_component::RemoteAccessOwner for SharingOwner {
    fn snapshot(&self) -> work_http_component::Snapshot {
        work_http_component::Snapshot {
            listening: true,
            enabled: self.control.enabled(),
            status: "fixture".into(),
            warning: String::new(),
            public_url: self.address.clone(),
            app_url: self.address.clone(),
            address: self.address.clone(),
            can_configure: false,
            address_hint: String::new(),
            node_id: "component-test".into(),
            web_hosted: false,
            configuration: None,
            restart_required: false,
        }
    }
    fn set_enabled(&self, enabled: bool) -> Result<(), String> {
        self.control.set_enabled(enabled);
        Ok(())
    }
}
#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn components_share_the_owner_and_do_not_start_an_agent() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let root = tempfile::tempdir().unwrap();
            let owner = Arc::new(
                LocalWorkspace::new(root.path().join("must-not-run"), root.path().join("data"))
                    .unwrap(),
            );
            let local = WorkspaceClient::Local(Arc::new(
                work_client::LocalWorkspace::from_node_owner(owner.clone()),
            ));
            owner
                .component_host()
                .register_capability(
                    "owner.identity",
                    false,
                    Arc::new(|_, _| Ok(serde_json::json!("same-owner"))),
                )
                .unwrap();
            let mut server = HttpServer::start(
                owner.clone(),
                ServerConfig {
                    bind: "127.0.0.1:0".parse().unwrap(),
                    password: "fixture-only".into(),
                    allowed_hosts: vec![],
                    node_id: "component-fixture".into(),
                    assets: None,
                    web_proxy: false,
                    allow_sharing: true,
                },
            )
            .unwrap();
            let remote = WorkClient::new(&format!("http://{}/", server.address())).unwrap();
            assert!(remote.component_catalog().await.is_err());
            remote.login("fixture-only").await.unwrap();
            let bundle = Bundle::new(
                Manifest {
                    schema_version: 1,
                    runtime: None,
                    id: "custom-panel".into(),
                    version: "1.0.0".into(),
                    title: "Custom".into(),
                    description: "fixture".into(),
                    panels: vec![Panel {
                        id: "settings".into(),
                        title: "Settings".into(),
                        slot: "settings".into(),
                        entry: "index.html".into(),
                        protocol: 1,
                    }],
                    actions: BTreeMap::from([(
                        "identity".into(),
                        Action {
                            capability: "owner.identity".into(),
                            local_only: false,
                        },
                    )]),
                },
                BTreeMap::from([("index.html".into(), "<h1>Independent UI</h1>".into())]),
            )
            .unwrap();
            remote
                .component_install(InstallRequest {
                    bundle,
                    grants: BTreeSet::from(["owner.identity".into()]),
                })
                .await
                .unwrap();
            assert_eq!(
                local.component_catalog().await.unwrap(),
                remote.component_catalog().await.unwrap()
            );
            let call = || Call {
                action: "identity".into(),
                arguments: serde_json::json!({}),
            };
            assert_eq!(
                local.component_call("custom-panel", call()).await.unwrap(),
                remote.component_call("custom-panel", call()).await.unwrap()
            );
            assert_eq!(
                remote
                    .component_panel("custom-panel", "settings")
                    .await
                    .unwrap(),
                "<h1>Independent UI</h1>"
            );
            assert!(
                remote
                    .component_panel("../escape", "settings")
                    .await
                    .is_err()
            );
            remote.component_remove("custom-panel").await.unwrap();
            assert_eq!(
                local.component_catalog().await.unwrap(),
                [] as [work_core::components::Installed; 0]
            );

            work_http_component::register(
                owner.component_host(),
                Arc::new(SharingOwner {
                    control: server.sharing_control(),
                    address: format!("http://{}/", server.address()),
                }),
            )
            .unwrap();
            let status = remote
                .component_call(
                    "rpi-http",
                    Call {
                        action: "status".into(),
                        arguments: serde_json::json!({}),
                    },
                )
                .await
                .unwrap();
            assert_eq!(status["canConfigure"], false);
            assert!(status.get("password").is_none());
            let set = |enabled| Call {
                action: "set-enabled".into(),
                arguments: serde_json::json!({"enabled":enabled}),
            };
            assert!(remote.component_call("rpi-http", set(false)).await.is_err());
            assert!(server.sharing_control().enabled());
            local.component_call("rpi-http", set(false)).await.unwrap();
            assert!(!server.sharing_control().enabled());
            assert!(remote.component_catalog().await.is_err());
            assert_eq!(local.component_catalog().await.unwrap().len(), 1);
            local.component_call("rpi-http", set(true)).await.unwrap();
            remote.login("fixture-only").await.unwrap();
            assert_eq!(remote.component_catalog().await.unwrap().len(), 1);
            server.drain().await.unwrap();
            owner.shutdown().await;
        })
        .await;
}
