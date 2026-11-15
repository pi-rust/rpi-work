use std::sync::Arc;
use work_client::{LocalWorkspace, WorkClient};
use work_node_service::{HttpServer, ServerConfig};

#[tokio::test]
async fn independent_switches_revoke_only_their_own_connections() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let root = tempfile::tempdir().unwrap();
            let assets = root.path().join("web");
            std::fs::create_dir(&assets).unwrap();
            std::fs::write(assets.join("index.html"), "<html>Work built-in Web</html>").unwrap();
            let local = Arc::new(
                LocalWorkspace::new(
                    root.path().join("must-not-launch-rpi"),
                    root.path().join("data"),
                )
                .unwrap(),
            );
            let test_password = "test-only";
            let config = ServerConfig {
                bind: "127.0.0.1:0".parse().unwrap(),
                password: test_password.into(),
                allowed_hosts: vec![],
                node_id: "same-work-owner".into(),
                assets: None,
                web_proxy: false,
                allow_sharing: false,
            };
            let mut collaboration = HttpServer::start(local.node_owner(), config.clone()).unwrap();
            let web_config = ServerConfig {
                assets: Some(assets),
                ..config
            };
            let mut web = HttpServer::start_web(local.node_owner(), web_config, None).unwrap();
            let remote_origin = format!("http://{}/", collaboration.address());
            let web_origin = format!("http://{}/", web.address());
            let remote = WorkClient::new(&remote_origin).unwrap();
            let browser = WorkClient::new(&web_origin).unwrap();
            let raw = reqwest::Client::new();
            assert!(remote.projects().await.is_err());
            for path in ["", ".well-known/rpi-work-entry", "api/v1/projects"] {
                let response = raw.get(format!("{web_origin}{path}")).send().await.unwrap();
                assert_eq!(response.status(), 403);
                assert_eq!(response.headers()["cache-control"], "no-store");
            }

            // A Web login and its API stay usable while collaboration is closed.
            web.sharing_control().set_enabled(true);
            assert_eq!(raw.get(&web_origin).send().await.unwrap().status(), 200);
            assert!(browser.login(test_password).await.unwrap().authenticated);
            let project = local
                .create_project("same-node-project", None, false)
                .await
                .unwrap();
            assert_eq!(browser.projects().await.unwrap(), vec![project.clone()]);
            assert!(remote.login(test_password).await.is_err());
            assert!(collaboration.issue_qr_authorization().is_err());

            collaboration.sharing_control().set_enabled(true);
            assert!(remote.login(test_password).await.unwrap().authenticated);
            assert_eq!(remote.projects().await.unwrap(), vec![project.clone()]);
            // The API entrance never serves the built-in browser interface.
            assert_eq!(raw.get(&remote_origin).send().await.unwrap().status(), 404);

            web.sharing_control().set_enabled(false);
            assert!(browser.projects().await.is_err());
            assert_eq!(raw.get(&web_origin).send().await.unwrap().status(), 403);
            assert_eq!(remote.projects().await.unwrap(), vec![project.clone()]);
            assert!(collaboration.issue_qr_authorization().is_ok());
            web.sharing_control().set_enabled(true);
            assert!(
                browser.projects().await.is_err(),
                "old Web login must stay revoked"
            );
            browser.login(test_password).await.unwrap();

            collaboration.sharing_control().set_enabled(false);
            assert!(remote.projects().await.is_err());
            assert_eq!(browser.projects().await.unwrap(), vec![project.clone()]);
            assert_eq!(local.projects().await.unwrap(), vec![project]);
            collaboration.sharing_control().set_enabled(true);
            assert!(
                remote.projects().await.is_err(),
                "old collaboration login must stay revoked"
            );

            collaboration.drain().await.unwrap();
            web.drain().await.unwrap();
            local.shutdown().await;
        })
        .await;
}
