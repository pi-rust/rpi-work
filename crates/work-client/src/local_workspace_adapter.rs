//! Converts application handles/errors at the local access boundary only.
use crate::{AgentClient, AgentTransport, ClientError, EventStream};
use futures_util::StreamExt;
use serde_json::Value;
use work_core::AgentDescriptor;
use work_node::workspace::{PreparedAgent, WorkspaceError, WorkspaceEventStream};

impl From<WorkspaceError> for ClientError {
    fn from(error: WorkspaceError) -> Self {
        match error {
            WorkspaceError::Local(error) => Self::Local(error),
            WorkspaceError::Runtime(error) => error.into(),
        }
    }
}
pub(crate) fn map_workspace_stream(stream: WorkspaceEventStream) -> EventStream {
    Box::pin(stream.map(|item| item.map_err(ClientError::from)))
}
pub(crate) fn into_agent_client(agent: PreparedAgent) -> AgentClient {
    AgentClient::from_transport(PreparedAgentTransport(agent))
}
struct PreparedAgentTransport(PreparedAgent);
#[async_trait::async_trait(?Send)]
impl AgentTransport for PreparedAgentTransport {
    fn descriptor(&self) -> AgentDescriptor {
        self.0.descriptor()
    }
    fn supports_command(&self, command: &str) -> bool {
        self.0.supports_command(command)
    }
    fn supports_config_option(&self, id: &str, config: &str) -> bool {
        self.0.supports_config_option(id, config)
    }
    fn session_config_options(&self, id: &str) -> Vec<Value> {
        self.0.session_config_options(id)
    }
    async fn open_session(&self, _cwd: &str) -> Result<String, ClientError> {
        self.0.open_session(_cwd).await.map_err(Into::into)
    }
    async fn load_session(&self, id: &str, _cwd: &str) -> Result<Value, ClientError> {
        self.0.load_session(id, _cwd).await.map_err(Into::into)
    }
    async fn resume_session(&self, id: &str, cwd: &str) -> Result<Value, ClientError> {
        self.0.resume_session(id, cwd).await.map_err(Into::into)
    }
    async fn stop(&self) -> Result<(), ClientError> {
        self.0.stop().await.map_err(Into::into)
    }
    async fn respond_to_agent_request(
        &self,
        request_id: Value,
        result: Value,
    ) -> Result<(), ClientError> {
        self.0
            .respond_to_agent_request(request_id, result)
            .await
            .map_err(Into::into)
    }
    async fn set_config_option(
        &self,
        id: &str,
        config: &str,
        value: Value,
    ) -> Result<Value, ClientError> {
        self.0
            .set_config_option(id, config, value)
            .await
            .map_err(Into::into)
    }
    async fn request(&self, id: &str, command: &str, payload: Value) -> Result<Value, ClientError> {
        self.0
            .request(id, command, payload)
            .await
            .map_err(Into::into)
    }
    async fn subscribe(&self, id: &str, after: u64) -> Result<EventStream, ClientError> {
        self.0
            .subscribe(id, after)
            .await
            .map(map_workspace_stream)
            .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LocalError, WireEvent};
    use futures_util::stream;
    use serde_json::json;
    use std::error::Error;
    use work_node::runtime::RuntimeError;

    #[test]
    fn workspace_local_errors_keep_their_variants_and_diagnostics() {
        let cases = [
            LocalError::InvalidInput("bad name".into()),
            LocalError::NotFound("private/path".into()),
            LocalError::DuplicateProject("private/path".into()),
            LocalError::Initialization("private stderr".into()),
            LocalError::Unsupported("operation".into()),
            LocalError::Shutdown,
            LocalError::Poisoned,
            LocalError::Io(std::io::Error::other("private/path")),
            LocalError::Json(serde_json::from_str::<Value>("{").unwrap_err()),
        ];
        for original in cases {
            let variant = std::mem::discriminant(&original);
            let display = original.to_string();
            let has_source = original.source().is_some();
            let converted = ClientError::from(WorkspaceError::Local(original));
            assert_eq!(converted.to_string(), display);
            assert_eq!(converted.source().is_some(), has_source);
            let ClientError::Local(actual) = converted else {
                panic!("lost local classification")
            };
            assert_eq!(std::mem::discriminant(&actual), variant);
        }
    }

    #[test]
    fn workspace_runtime_errors_keep_client_classification_and_json_source() {
        macro_rules! check {
            ($variant:ident) => {
                let original = RuntimeError::$variant("diagnostic".into());
                let display = original.to_string();
                let converted = ClientError::from(WorkspaceError::Runtime(original));
                assert_eq!(converted.to_string(), display);
                assert!(matches!(converted, ClientError::$variant(message) if message == "diagnostic"));
            };
        }
        check!(Rpc);
        check!(Acp);
        check!(ProtocolMismatch);
        check!(RequestTimeout);
        check!(Process);
        check!(ChildExited);
        let converted = ClientError::from(WorkspaceError::Runtime(RuntimeError::Unsupported {
            operation: "snapshot".into(),
            protocol: "acp".into(),
        }));
        assert!(
            matches!(converted, ClientError::Unsupported { operation, protocol } if operation == "snapshot" && protocol == "acp")
        );
        let original = serde_json::from_str::<Value>("{").unwrap_err();
        let display = original.to_string();
        let converted = ClientError::from(WorkspaceError::Runtime(RuntimeError::Json(original)));
        assert!(matches!(&converted, ClientError::Json(_)));
        assert_eq!(converted.source().unwrap().to_string(), display);
    }

    #[tokio::test]
    async fn workspace_stream_mapping_preserves_data_errors_and_eof_without_a_reader() {
        let event = WireEvent {
            id: Some("7".into()),
            seq: Some(7),
            event_type: "fixture".into(),
            data: json!({"message":"same"}),
        };
        let source: WorkspaceEventStream = Box::pin(stream::iter([
            Ok(event.clone()),
            Err(WorkspaceError::Local(LocalError::Shutdown)),
            Err(WorkspaceError::Runtime(RuntimeError::ChildExited(
                "stopped".into(),
            ))),
            Ok(event.clone()),
        ]));
        let mut mapped = map_workspace_stream(source);
        assert_eq!(mapped.next().await.unwrap().unwrap(), event);
        assert!(matches!(
            mapped.next().await.unwrap(),
            Err(ClientError::Local(LocalError::Shutdown))
        ));
        assert!(
            matches!(mapped.next().await.unwrap(), Err(ClientError::ChildExited(message)) if message == "stopped")
        );
        assert_eq!(mapped.next().await.unwrap().unwrap(), event);
        assert!(mapped.next().await.is_none());
    }
}
