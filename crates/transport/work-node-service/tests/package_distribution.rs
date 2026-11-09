//! Package distribution is authenticated data transport, never a host launch.
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use work_client::{LocalWorkspace, WorkClient, WorkspaceClient};
use work_core::api::{NativeKind, NativeScope, NodeApi, NodeErrorKind, PackageInstall, PackageRef};
use work_node_service::{HttpServer, ServerConfig};

fn config() -> ServerConfig {
    ServerConfig {
        bind: "127.0.0.1:0".parse().unwrap(),
        password: "isolated-package-test".into(),
        allowed_hosts: vec![],
        node_id: "package-owner".into(),
        assets: None,
        web_proxy: false,
        allow_sharing: true,
    }
}

fn reference(offer: &work_core::api::PackageOffer) -> PackageRef {
    PackageRef {
        id: offer.manifest.id.clone(),
        version: offer.manifest.version.clone(),
        sha256: offer.sha256.clone(),
    }
}

fn data(value: impl serde::Serialize) -> Value {
    serde_json::to_value(value).unwrap()
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // One isolated owner exercises the complete transport boundary.
async fn authenticated_distribution_shares_owner_without_executing_packages() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let root = tempfile::tempdir().unwrap();
            // A nonexistent executable makes accidental host startup a test failure.
            // Data and transcript directories are isolated, as in shared_owner.rs.
            let local = Arc::new(
                LocalWorkspace::with_session_dir(
                    root.path().join("host-must-not-start"),
                    root.path().join("data"),
                    Some(root.path().join("transcripts")),
                )
                .unwrap(),
            );
            let project = local.create_project("packages", None, false).await.unwrap();
            let local_port = WorkspaceClient::Local(local.clone());
            let node = local.node_owner();
            let mut server = HttpServer::start(node.clone(), config()).unwrap();
            let base = format!("http://{}", server.address());
            let http = WorkClient::new(&format!("{base}/")).unwrap();
            let remote_port = WorkspaceClient::Remote(http.clone());
            let raw = reqwest::Client::builder()
                .cookie_store(true)
                .build()
                .unwrap();
            let path = format!("{base}/api/v1/projects/{}/packages", project.id);
            assert_eq!(raw.get(&path).send().await.unwrap().status(), 401);
            for operation in ["bundle", "acquire", "install"] {
                assert_eq!(
                    raw.post(format!("{path}/{operation}"))
                        .header("x-rpi-web", "1")
                        .json(&json!({}))
                        .send()
                        .await
                        .unwrap()
                        .status(),
                    401
                );
            }
            assert_eq!(
                NodeApi::package_catalog(&remote_port, &project.id)
                    .await
                    .unwrap_err()
                    .kind,
                NodeErrorKind::AccessDenied
            );
            http.login("isolated-package-test").await.unwrap();
            assert_eq!(
                raw.post(format!("{base}/api/v1/auth"))
                    .header("x-rpi-web", "1")
                    .json(&json!({"password":"isolated-package-test"}))
                    .send()
                    .await
                    .unwrap()
                    .status(),
                200
            );

            let initial = NodeApi::package_catalog(&local_port, &project.id)
                .await
                .unwrap();
            assert_eq!(
                data(&initial),
                data(http.package_catalog(&project.id).await.unwrap())
            );
            assert_eq!(
                data(&initial),
                data(
                    NodeApi::package_catalog(&remote_port, &project.id)
                        .await
                        .unwrap()
                )
            );
            assert_eq!(initial.packages.len(), 3);
            assert!(
                initial
                    .packages
                    .iter()
                    .all(|p| p.manifest.version == "1.0.0" && !p.acquired)
            );
            assert_eq!(
                initial
                    .packages
                    .iter()
                    .filter(|p| p.manifest.kind == NativeKind::Package)
                    .count(),
                2
            );
            assert_eq!(
                initial
                    .packages
                    .iter()
                    .filter(|p| p.manifest.kind == NativeKind::Skill)
                    .count(),
                1
            );
            let selected = reference(&initial.packages[0]);
            let untrusted = PackageInstall {
                package: selected.clone(),
                scope: NativeScope::Project,
                trusted: false,
            };
            assert!(matches!(
                NodeApi::package_install(&remote_port, &project.id, untrusted.clone())
                    .await
                    .unwrap_err()
                    .kind,
                NodeErrorKind::AccessDenied | NodeErrorKind::InvalidInput
            ));

            for bad in [
                PackageRef {
                    version: "999.0.0".into(),
                    ..selected.clone()
                },
                PackageRef {
                    sha256: "0".repeat(64),
                    ..selected.clone()
                },
                PackageRef {
                    id: "unknown-package".into(),
                    ..selected.clone()
                },
            ] {
                assert!(http.package_bundle(&project.id, bad.clone()).await.is_err());
                assert!(
                    NodeApi::package_acquire(&remote_port, &project.id, bad)
                        .await
                        .is_err()
                );
            }
            // Existing security gates also govern every new POST, including the
            // read-only bundle export. No arbitrary Origin/Host or missing marker.
            for operation in ["bundle", "acquire", "install"] {
                let valid = if operation == "install" {
                    data(&untrusted)
                } else {
                    data(&selected)
                };
                assert_eq!(
                    raw.post(format!("{path}/{operation}"))
                        .json(&valid)
                        .send()
                        .await
                        .unwrap()
                        .status(),
                    403
                );
                assert_eq!(
                    raw.post(format!("{path}/{operation}"))
                        .header("x-rpi-web", "1")
                        .header("origin", "https://foreign.invalid")
                        .json(&valid)
                        .send()
                        .await
                        .unwrap()
                        .status(),
                    403
                );
                assert_eq!(
                    raw.post(format!("{path}/{operation}"))
                        .header("x-rpi-web", "1")
                        .header("host", "foreign.invalid")
                        .json(&valid)
                        .send()
                        .await
                        .unwrap()
                        .status(),
                    403
                );
            }
            // Typed envelopes reject unknown fields, including source URLs; no proxy surface.
            for operation in ["bundle", "acquire", "install"] {
                let valid = if operation == "install" {
                    data(&untrusted)
                } else {
                    data(&selected)
                };
                let mut unknown = valid.clone();
                unknown["url"] = json!("https://invalid.example/never-fetch");
                let mut wrong_type = valid;
                if operation == "install" {
                    wrong_type["trusted"] = json!("true");
                } else {
                    wrong_type["sha256"] = json!(42);
                }
                let mut nested_unknown = data(&untrusted);
                nested_unknown["package"]["url"] = json!("https://invalid.example/never-fetch");
                let invalid = if operation == "install" {
                    nested_unknown
                } else {
                    json!({})
                };
                for body in [unknown, wrong_type, invalid] {
                    assert_eq!(
                        raw.post(format!("{path}/{operation}"))
                            .header("x-rpi-web", "1")
                            .json(&body)
                            .send()
                            .await
                            .unwrap()
                            .status(),
                        400
                    );
                }
            }
            assert_eq!(
                data(&initial),
                data(
                    NodeApi::package_catalog(&local_port, &project.id)
                        .await
                        .unwrap()
                )
            );

            for offer in &initial.packages {
                let package = reference(offer);
                let local_bundle =
                    NodeApi::package_bundle(&local_port, &project.id, package.clone())
                        .await
                        .unwrap();
                let remote_bundle =
                    NodeApi::package_bundle(&remote_port, &project.id, package.clone())
                        .await
                        .unwrap();
                assert_eq!(local_bundle, remote_bundle);
                assert_eq!(remote_bundle.manifest, offer.manifest);
                assert_eq!(remote_bundle.sha256, package.sha256);
                assert_ne!(remote_bundle.sources, std::collections::BTreeMap::new());
                assert_eq!(
                    http.package_bundle(&project.id, package).await.unwrap(),
                    remote_bundle
                );
            }
            // Downloading all canonical JSON envelopes is read-only.
            assert_eq!(
                data(&initial),
                data(http.package_catalog(&project.id).await.unwrap())
            );
            assert_eq!(local.sessions().await.unwrap(), []);
            assert!(
                NodeApi::package_catalog(&remote_port, "missing-project")
                    .await
                    .is_err()
            );

            let acquired = NodeApi::package_acquire(&remote_port, &project.id, selected.clone())
                .await
                .unwrap();
            assert_eq!(acquired.package, selected);
            let after = NodeApi::package_catalog(&local_port, &project.id)
                .await
                .unwrap();
            assert_eq!(
                data(&after),
                data(http.package_catalog(&project.id).await.unwrap())
            );
            assert!(
                after
                    .packages
                    .iter()
                    .find(|p| p.manifest.id == selected.id)
                    .unwrap()
                    .acquired
            );
            assert_eq!(after.packages.iter().filter(|p| p.acquired).count(), 1);
            // Acquiring again is idempotent and does not require an execution host.
            assert_eq!(
                http.package_acquire(&project.id, selected.clone())
                    .await
                    .unwrap()
                    .package,
                selected
            );
            assert!(matches!(
                http.package_install(&project.id, untrusted).await,
                Err(work_client::ClientError::Api { status, .. })
                    if status == 400 || status == 403
            ));
            assert_eq!(local.sessions().await.unwrap(), []);
            assert_eq!(
                data(&after),
                data(
                    NodeApi::package_catalog(&local_port, &project.id)
                        .await
                        .unwrap()
                )
            );

            // Revocation still governs these routes; remote denial cannot fall back locally.
            server.sharing_control().set_enabled(false);
            for operation in ["bundle", "acquire", "install"] {
                let valid = if operation == "install" {
                    data(&PackageInstall {
                        package: selected.clone(),
                        scope: NativeScope::Project,
                        trusted: true,
                    })
                } else {
                    data(&selected)
                };
                assert_eq!(
                    raw.post(format!("{path}/{operation}"))
                        .header("x-rpi-web", "1")
                        .json(&valid)
                        .send()
                        .await
                        .unwrap()
                        .status(),
                    403
                );
            }
            assert_eq!(
                NodeApi::package_catalog(&remote_port, &project.id)
                    .await
                    .unwrap_err()
                    .kind,
                NodeErrorKind::AccessDenied
            );
            assert_eq!(
                data(&after),
                data(
                    NodeApi::package_catalog(&local_port, &project.id)
                        .await
                        .unwrap()
                )
            );
            server.drain().await.unwrap();
            local.shutdown().await;
        })
        .await;
}

#[tokio::test]
async fn old_server_errors_are_not_retried_or_fallen_back() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let observed = requests.clone();
    let app =
        axum::Router::new().fallback(move |method: axum::http::Method, uri: axum::http::Uri| {
            let observed = observed.clone();
            async move {
                observed
                    .lock()
                    .unwrap()
                    .push(format!("{method} {}", uri.path()));
                axum::http::StatusCode::NOT_FOUND
            }
        });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = stopped.await;
            })
            .await
            .unwrap();
    });
    let remote = WorkspaceClient::new(&format!("http://{address}/")).unwrap();
    let package = PackageRef {
        id: "base".into(),
        version: "1.0.0".into(),
        sha256: "0".repeat(64),
    };
    assert_eq!(
        NodeApi::package_catalog(&remote, "project")
            .await
            .unwrap_err()
            .kind,
        NodeErrorKind::NotFound
    );
    assert_eq!(
        NodeApi::package_bundle(&remote, "project", package.clone())
            .await
            .unwrap_err()
            .kind,
        NodeErrorKind::NotFound
    );
    assert_eq!(
        NodeApi::package_acquire(&remote, "project", package.clone())
            .await
            .unwrap_err()
            .kind,
        NodeErrorKind::NotFound
    );
    assert_eq!(
        NodeApi::package_install(
            &remote,
            "project",
            PackageInstall {
                package,
                scope: NativeScope::Project,
                trusted: false
            }
        )
        .await
        .unwrap_err()
        .kind,
        NodeErrorKind::NotFound
    );
    assert_eq!(
        *requests.lock().unwrap(),
        [
            "GET /api/v1/projects/project/packages",
            "POST /api/v1/projects/project/packages/bundle",
            "POST /api/v1/projects/project/packages/acquire",
            "POST /api/v1/projects/project/packages/install",
        ]
    );
    stop.send(()).unwrap();
    server.await.unwrap();
}
