#![cfg(not(target_arch = "wasm32"))]

use futures_util::StreamExt;
use serde_json::{Value, json};
use tokio::time::{Duration, timeout};
use work_client::{AcpClient, ClientError, RpiRpcClient};

#[tokio::test]
async fn cancelling_response_wait_after_full_write_keeps_transport_usable() {
    timeout(Duration::from_secs(2), async {
        let rpc = RpiRpcClient::spawn(env!("CARGO_BIN_EXE_rpc-fixture"), ".").await.unwrap();
        let mut stream = rpc.subscribe_events().unwrap();
        {
            let mut request = std::pin::pin!(rpc.request("fixture_hang", Value::Null));
            tokio::select! {
                result = &mut request => panic!("fixture must withhold response: {result:?}"),
                event = stream.next() => assert_eq!(event.unwrap().unwrap().event_type, "fixture_waiting"),
            }
            // Child receipt can precede the Tokio pipe's flush completion.
            // A subsequent completed request is a deterministic stdin fence.
            tokio::select! {
                result = &mut request => panic!("fixture must withhold response: {result:?}"),
                result = rpc.request("get_state", Value::Null) => { result.unwrap(); }
            }
        }
        rpc.request("get_state", Value::Null).await.unwrap();
        rpc.stop().await.unwrap();

        let acp = AcpClient::spawn(env!("CARGO_BIN_EXE_acp-fixture"), &[], ".").await.unwrap();
        let mut stream = acp.subscribe_events().unwrap();
        {
            let mut request = std::pin::pin!(acp.request("fixture_hang", Value::Null));
            tokio::select! {
                result = &mut request => panic!("fixture must withhold response: {result:?}"),
                event = stream.next() => assert_eq!(event.unwrap().unwrap().event_type, "message_update"),
            }
            tokio::select! {
                result = &mut request => panic!("fixture must withhold response: {result:?}"),
                result = acp.new_session(".") => { result.unwrap(); }
            }
        }
        acp.new_session(".").await.unwrap();
        acp.stop().await.unwrap();
    }).await.expect("response-wait cancellation is not protocol abort and must not corrupt stdin");
}

#[tokio::test]
async fn dropping_only_the_last_native_client_closes_retained_streams() {
    timeout(Duration::from_secs(3), async {
        let rpc = RpiRpcClient::spawn(env!("CARGO_BIN_EXE_rpc-fixture"), ".")
            .await
            .unwrap();
        let clone = rpc.clone();
        let mut stream = rpc.subscribe_events().unwrap();
        drop(rpc);
        clone.request("get_state", Value::Null).await.unwrap();
        drop(clone);
        assert!(matches!(
            stream.next().await.unwrap(),
            Err(ClientError::ChildExited(_))
        ));
        assert!(stream.next().await.is_none());

        let acp = AcpClient::spawn(env!("CARGO_BIN_EXE_acp-fixture"), &[], ".")
            .await
            .unwrap();
        let clone = acp.clone();
        let mut stream = acp.subscribe_events().unwrap();
        drop(acp);
        clone.new_session(".").await.unwrap();
        drop(clone);
        assert!(matches!(
            stream.next().await.unwrap(),
            Err(ClientError::ChildExited(_))
        ));
        assert!(stream.next().await.is_none());
    })
    .await
    .expect("last native handle must release the child even while a subscription remains");
}

#[tokio::test]
async fn cancelling_native_request_mid_write_invalidates_stream_before_retry() {
    timeout(Duration::from_secs(3), async {
        let rpc = RpiRpcClient::spawn(env!("CARGO_BIN_EXE_rpc-fixture"), ".")
            .await
            .unwrap();
        let mut stream = rpc.subscribe_events().unwrap();
        rpc.request("fixture_pause", Value::Null).await.unwrap();
        {
            let mut request =
                std::pin::pin!(rpc.request("get_state", json!({"padding":"x".repeat(900_000)})));
            tokio::select! {
                result = &mut request => panic!("expected stdin backpressure: {result:?}"),
                () = tokio::time::sleep(Duration::from_millis(20)) => {}
            }
        }
        // Check immediately, before an asynchronous cleanup task could run.
        assert!(matches!(
            rpc.request("get_state", Value::Null).await,
            Err(ClientError::ChildExited(_))
        ));
        assert!(matches!(
            stream.next().await.unwrap(),
            Err(ClientError::ChildExited(_))
        ));
        rpc.stop().await.unwrap();

        let acp = AcpClient::spawn(env!("CARGO_BIN_EXE_acp-fixture"), &[], ".")
            .await
            .unwrap();
        let mut stream = acp.subscribe_events().unwrap();
        acp.request("fixture_pause", Value::Null).await.unwrap();
        {
            let mut request =
                std::pin::pin!(acp.request("session/new", json!({"padding":"x".repeat(900_000)})));
            tokio::select! {
                result = &mut request => panic!("expected stdin backpressure: {result:?}"),
                () = tokio::time::sleep(Duration::from_millis(20)) => {}
            }
        }
        assert!(matches!(
            acp.new_session(".").await,
            Err(ClientError::ChildExited(_))
        ));
        assert!(matches!(
            stream.next().await.unwrap(),
            Err(ClientError::ChildExited(_))
        ));
        acp.stop().await.unwrap();
    })
    .await
    .expect("cancelled partial frame must fail closed without awaiting the response timeout");
}

#[tokio::test]
async fn cancelled_acp_permission_response_fails_closed_and_closes_card() {
    timeout(Duration::from_secs(3), async {
        let client = AcpClient::spawn(env!("CARGO_BIN_EXE_acp-fixture"),
            &["--wait-permission".into(), "--pause-on-permission".into()], ".").await.unwrap();
        let session = client.new_session(".").await.unwrap();
        let mut stream = client.subscribe_events().unwrap();
        let consume = async {
            assert_eq!(stream.next().await.unwrap().unwrap().event_type, "ui_dialog_request");
            {
                let mut response = std::pin::pin!(client.respond_to_agent_request(json!(4),
                    json!({"outcome":{"outcome":"selected","optionId":"grant-42"},
                        "padding":"x".repeat(900_000)})));
                tokio::select! {
                    result = &mut response => panic!("fixture must hold response under backpressure: {result:?}"),
                    () = tokio::time::sleep(Duration::from_millis(10)) => {}
                }
                assert!(client.respond_to_agent_request(json!(4),
                    json!({"outcome":{"outcome":"cancelled"}})).await.is_err());
                // Dropping the in-flight answer must not leave a busy card or
                // permit a retry following a potentially partial frame.
            }
            assert_eq!(stream.next().await.unwrap().unwrap().event_type, "ui_dialog_closed");
            assert!(matches!(stream.next().await.unwrap(), Err(ClientError::ChildExited(_))));
            assert!(stream.next().await.is_none());
        };
        let (result, ()) = tokio::join!(client.prompt(&session, "permission"), consume);
        assert!(matches!(result, Err(ClientError::ChildExited(_))));
        assert!(matches!(client.new_session(".").await, Err(ClientError::ChildExited(_))));
        client.stop().await.unwrap();
    }).await.expect("cancelled ACP answer must close the card and terminate the corrupted transport");
}

#[tokio::test]
async fn rpc_eof_ends_stream_and_rejects_later_requests() {
    timeout(Duration::from_secs(2), async {
        let client = RpiRpcClient::spawn(env!("CARGO_BIN_EXE_rpc-fixture"), ".")
            .await
            .unwrap();
        let mut stream = client.subscribe_events().unwrap();
        assert!(matches!(
            client.request("fixture_crash", Value::Null).await,
            Err(ClientError::ChildExited(_))
        ));
        assert!(matches!(
            stream.next().await.unwrap(),
            Err(ClientError::ChildExited(_))
        ));
        assert!(stream.next().await.is_none());
        let mut late_stream = client.subscribe_events().unwrap();
        assert!(matches!(
            late_stream.next().await.unwrap(),
            Err(ClientError::ChildExited(_))
        ));
        assert!(matches!(
            client.request("get_state", Value::Null).await,
            Err(ClientError::ChildExited(_))
        ));
        client.stop().await.unwrap();
        client.stop().await.unwrap();
    })
    .await
    .expect("RPC EOF must terminate requests and subscribers promptly");
}

#[tokio::test]
async fn rpc_stop_kills_unresponsive_child_and_releases_pending_request() {
    timeout(Duration::from_secs(2), async {
        let client = RpiRpcClient::spawn(env!("CARGO_BIN_EXE_rpc-fixture"), ".").await.unwrap();
        let mut stream = client.subscribe_events().unwrap();
        let mut request = std::pin::pin!(client.request("fixture_hang", Value::Null));
        tokio::select! {
            result = &mut request => panic!("fixture request should be hanging: {result:?}"),
            event = stream.next() => assert_eq!(event.unwrap().unwrap().event_type, "fixture_waiting"),
        }
        client.stop().await.unwrap();
        assert!(matches!(request.await, Err(ClientError::ChildExited(_))));
        assert!(matches!(stream.next().await.unwrap(), Err(ClientError::ChildExited(_))));
        assert!(stream.next().await.is_none());
        client.stop().await.unwrap();
    }).await.expect("RPC stop must not wait for the normal 30s response timeout");
}

#[tokio::test]
async fn acp_crash_and_stop_close_permissions_without_successful_settlement() {
    for crash in [true, false] {
        timeout(Duration::from_secs(2), async {
            let mut args = vec!["--wait-permission".into()];
            if crash {
                args.push("--exit-on-permission".into());
            }
            let client = AcpClient::spawn(env!("CARGO_BIN_EXE_acp-fixture"), &args, ".")
                .await
                .unwrap();
            let session = client.new_session(".").await.unwrap();
            let mut stream = client.subscribe_events().unwrap();
            let consume = async {
                let permission = stream.next().await.unwrap().unwrap();
                assert_eq!(permission.event_type, "ui_dialog_request");
                if !crash {
                    client.stop().await.unwrap();
                }
                let closed = stream.next().await.unwrap().unwrap();
                assert_eq!(closed.event_type, "ui_dialog_closed");
                assert!(matches!(
                    stream.next().await.unwrap(),
                    Err(ClientError::ChildExited(_))
                ));
                assert!(stream.next().await.is_none());
            };
            let (result, ()) = tokio::join!(client.prompt(&session, "needs permission"), consume);
            assert!(matches!(result, Err(ClientError::ChildExited(_))));
            let mut late_stream = client.subscribe_events().unwrap();
            assert!(matches!(
                late_stream.next().await.unwrap(),
                Err(ClientError::ChildExited(_))
            ));
            assert!(late_stream.next().await.is_none());
            assert!(matches!(
                client.new_session(".").await,
                Err(ClientError::ChildExited(_))
            ));
            assert!(
                client
                    .respond_to_agent_request(
                        json!(4),
                        json!({"outcome":{"outcome":"selected","optionId":"grant-42"}})
                    )
                    .await
                    .is_err()
            );
            client.stop().await.unwrap();
            client.stop().await.unwrap();
        })
        .await
        .expect("ACP EOF/stop must clear pending permission and end streaming");
    }
}
