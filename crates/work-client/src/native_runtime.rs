//! Adapt the node's native runtime to the client compatibility surface.

use futures_util::StreamExt;
use work_node::runtime::{RuntimeError, RuntimeEventStream};

use crate::{ClientError, EventStream};

impl From<RuntimeError> for ClientError {
    fn from(error: RuntimeError) -> Self {
        match error {
            RuntimeError::Json(error) => Self::Json(error),
            RuntimeError::Unsupported {
                operation,
                protocol,
            } => Self::Unsupported {
                operation,
                protocol,
            },
            RuntimeError::Rpc(message) => Self::Rpc(message),
            RuntimeError::Acp(message) => Self::Acp(message),
            RuntimeError::ProtocolMismatch(message) => Self::ProtocolMismatch(message),
            RuntimeError::RequestTimeout(message) => Self::RequestTimeout(message),
            RuntimeError::Process(message) => Self::Process(message),
            RuntimeError::ChildExited(message) => Self::ChildExited(message),
        }
    }
}

pub(crate) fn map_runtime_stream(stream: RuntimeEventStream) -> EventStream {
    Box::pin(stream.map(|event| event.map_err(ClientError::from)))
}

// Always forward the normalized trait surface, including negotiated predicates.
// In particular, RPC's inherent `request` has no session_id parameter.
macro_rules! impl_native_transport {
    ($client:ty) => {
        #[async_trait::async_trait(?Send)]
        impl crate::AgentTransport for $client {
            fn descriptor(&self) -> work_core::AgentDescriptor {
                work_node::runtime::RuntimeTransport::descriptor(&self.runtime)
            }

            fn supports_command(&self, command: &str) -> bool {
                work_node::runtime::RuntimeTransport::supports_command(&self.runtime, command)
            }

            fn supports_config_option(&self, session_id: &str, config_id: &str) -> bool {
                work_node::runtime::RuntimeTransport::supports_config_option(
                    &self.runtime,
                    session_id,
                    config_id,
                )
            }

            fn session_config_options(&self, session_id: &str) -> Vec<serde_json::Value> {
                work_node::runtime::RuntimeTransport::session_config_options(
                    &self.runtime,
                    session_id,
                )
            }

            async fn open_session(&self, cwd: &str) -> Result<String, ClientError> {
                work_node::runtime::RuntimeTransport::open_session(&self.runtime, cwd)
                    .await
                    .map_err(ClientError::from)
            }

            async fn stop(&self) -> Result<(), ClientError> {
                work_node::runtime::RuntimeTransport::stop(&self.runtime)
                    .await
                    .map_err(ClientError::from)
            }

            async fn respond_to_agent_request(
                &self,
                request_id: serde_json::Value,
                result: serde_json::Value,
            ) -> Result<(), ClientError> {
                work_node::runtime::RuntimeTransport::respond_to_agent_request(
                    &self.runtime,
                    request_id,
                    result,
                )
                .await
                .map_err(ClientError::from)
            }

            async fn load_session(
                &self,
                session_id: &str,
                cwd: &str,
            ) -> Result<serde_json::Value, ClientError> {
                work_node::runtime::RuntimeTransport::load_session(&self.runtime, session_id, cwd)
                    .await
                    .map_err(ClientError::from)
            }

            async fn resume_session(
                &self,
                session_id: &str,
                cwd: &str,
            ) -> Result<serde_json::Value, ClientError> {
                work_node::runtime::RuntimeTransport::resume_session(&self.runtime, session_id, cwd)
                    .await
                    .map_err(ClientError::from)
            }

            async fn set_config_option(
                &self,
                session_id: &str,
                config_id: &str,
                value: serde_json::Value,
            ) -> Result<serde_json::Value, ClientError> {
                work_node::runtime::RuntimeTransport::set_config_option(
                    &self.runtime,
                    session_id,
                    config_id,
                    value,
                )
                .await
                .map_err(ClientError::from)
            }

            async fn request(
                &self,
                session_id: &str,
                command: &str,
                payload: serde_json::Value,
            ) -> Result<serde_json::Value, ClientError> {
                work_node::runtime::RuntimeTransport::request(
                    &self.runtime,
                    session_id,
                    command,
                    payload,
                )
                .await
                .map_err(ClientError::from)
            }

            async fn subscribe(
                &self,
                session_id: &str,
                after: u64,
            ) -> Result<EventStream, ClientError> {
                work_node::runtime::RuntimeTransport::subscribe(&self.runtime, session_id, after)
                    .await
                    .map(map_runtime_stream)
                    .map_err(ClientError::from)
            }
        }
    };
}

impl_native_transport!(crate::RpiRpcClient);
impl_native_transport!(crate::AcpClient);

#[cfg(test)]
mod tests {
    use std::error::Error;

    use futures_util::stream;
    use serde_json::{Value, json};

    use super::*;

    #[test]
    fn maps_string_errors_without_losing_variant_or_display() {
        macro_rules! assert_mapping {
            ($variant:ident, $display:literal) => {
                let error = ClientError::from(RuntimeError::$variant("detail".into()));
                assert!(matches!(&error, ClientError::$variant(message) if message == "detail"));
                assert_eq!(error.to_string(), $display);
                assert!(error.source().is_none());
            };
        }

        assert_mapping!(Rpc, "RPC transport error: detail");
        assert_mapping!(Acp, "ACP transport error: detail");
        assert_mapping!(ProtocolMismatch, "agent protocol mismatch: detail");
        assert_mapping!(RequestTimeout, "agent request timed out: detail");
        assert_mapping!(Process, "agent process failed: detail");
        assert_mapping!(ChildExited, "agent process exited: detail");
    }

    #[test]
    fn maps_unsupported_operation_and_protocol() {
        let error = ClientError::from(RuntimeError::Unsupported {
            operation: "session/load".into(),
            protocol: "acp".into(),
        });
        assert!(matches!(
            &error,
            ClientError::Unsupported { operation, protocol }
                if operation == "session/load" && protocol == "acp"
        ));
        assert_eq!(
            error.to_string(),
            "operation `session/load` is unsupported by acp"
        );
        assert!(error.source().is_none());
    }

    #[test]
    fn preserves_json_error_and_source() {
        let json_error = serde_json::from_str::<Value>("{\n  invalid").unwrap_err();
        let message = json_error.to_string();
        let location = (json_error.line(), json_error.column());
        let category = json_error.classify();
        let error = ClientError::from(RuntimeError::Json(json_error));

        let ClientError::Json(source) = &error else {
            panic!("expected ClientError::Json");
        };
        assert_eq!((source.line(), source.column()), location);
        assert_eq!(source.classify(), category);
        assert_eq!(error.to_string(), format!("invalid JSON: {message}"));
        let source = error.source().expect("JSON error source must be retained");
        assert!(source.downcast_ref::<serde_json::Error>().is_some());
        assert_eq!(source.to_string(), message);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn maps_each_stream_error_and_preserves_events_and_order() {
        let first = crate::wire_event(
            Some("event-1".into()),
            Some(7),
            "message_update",
            json!({"text": "hello"}),
        );
        let last = crate::wire_event(None, Some(8), "agent_settled", Value::Null);
        let runtime: RuntimeEventStream = Box::pin(stream::iter(vec![
            Ok(first.clone()),
            Err(RuntimeError::Rpc("bad frame".into())),
            Err(RuntimeError::Json(
                serde_json::from_str::<Value>("{").unwrap_err(),
            )),
            Err(RuntimeError::ChildExited("closed".into())),
            Ok(last.clone()),
        ]));
        let mut mapped = map_runtime_stream(runtime);

        let event = mapped.next().await.unwrap().unwrap();
        assert_eq!(event.id, first.id);
        assert_eq!(event.seq, first.seq);
        assert_eq!(event.event_type, first.event_type);
        assert_eq!(event.data, first.data);
        assert!(matches!(
            mapped.next().await.unwrap(),
            Err(ClientError::Rpc(message)) if message == "bad frame"
        ));
        assert!(matches!(
            mapped.next().await.unwrap(),
            Err(ClientError::Json(_))
        ));
        assert!(matches!(
            mapped.next().await.unwrap(),
            Err(ClientError::ChildExited(message)) if message == "closed"
        ));
        let event = mapped.next().await.unwrap().unwrap();
        assert_eq!(event.id, last.id);
        assert_eq!(event.seq, last.seq);
        assert_eq!(event.event_type, last.event_type);
        assert_eq!(event.data, last.data);
        assert!(mapped.next().await.is_none());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn maps_empty_stream() {
        let runtime: RuntimeEventStream = Box::pin(stream::empty());
        assert!(map_runtime_stream(runtime).next().await.is_none());
    }
}
