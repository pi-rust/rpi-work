#![cfg(not(target_arch = "wasm32"))]
//! The native adapter port does not require a workspace, client, or Send futures.
use serde_json::{Value, json};
use std::{cell::Cell, rc::Rc};
use work_core::{AgentDescriptor, CapabilitySet, TransportIdentity};
use work_node_runtime::runtime::{RuntimeError, RuntimeEventStream, RuntimeTransport};

struct LocalRuntime {
    calls: Rc<Cell<usize>>,
}

#[async_trait::async_trait(?Send)]
impl RuntimeTransport for LocalRuntime {
    fn descriptor(&self) -> AgentDescriptor {
        AgentDescriptor {
            identity: TransportIdentity {
                protocol: "fixture".into(),
                version: None,
                transport: "fixture".into(),
                agent_name: None,
            },
            capabilities: CapabilitySet::rpc_default(),
        }
    }

    fn supports_command(&self, command: &str) -> bool {
        command == "get_state"
    }
    fn supports_config_option(&self, _session_id: &str, _config_id: &str) -> bool {
        false
    }

    async fn open_session(&self, _cwd: &str) -> Result<String, RuntimeError> {
        self.calls.set(self.calls.get() + 1);
        Ok("existing-session".into())
    }

    async fn stop(&self) -> Result<(), RuntimeError> {
        Ok(())
    }

    async fn set_config_option(
        &self,
        _session_id: &str,
        _config_id: &str,
        value: Value,
    ) -> Result<Value, RuntimeError> {
        Ok(value)
    }

    async fn request(
        &self,
        _session_id: &str,
        _command: &str,
        payload: Value,
    ) -> Result<Value, RuntimeError> {
        Ok(payload)
    }

    async fn subscribe(
        &self,
        _session_id: &str,
        _after: u64,
    ) -> Result<RuntimeEventStream, RuntimeError> {
        Ok(Box::pin(futures_util::stream::empty()))
    }
}

#[tokio::test]
async fn runtime_port_is_object_safe_lazy_and_preserves_non_send_futures() {
    let calls = Rc::new(Cell::new(0));
    let runtime: Box<dyn RuntimeTransport> = Box::new(LocalRuntime {
        calls: Rc::clone(&calls),
    });
    let future = runtime.open_session(".");
    assert_eq!(calls.get(), 0);
    assert_eq!(future.await.unwrap(), "existing-session");
    assert_eq!(calls.get(), 1);
    assert!(runtime.supports_command("get_state"));
    // Do not infer negotiated command support from the broad descriptor.
    assert!(runtime.descriptor().supports("prompt"));
    assert!(!runtime.supports_command("prompt"));
    assert!(!runtime.supports_config_option("existing-session", "model"));
    assert_eq!(
        runtime.session_config_options("existing-session"),
        Vec::<Value>::new()
    );
}

#[tokio::test]
async fn unimplemented_native_operations_keep_the_existing_unsupported_payloads() {
    let runtime: Box<dyn RuntimeTransport> = Box::new(LocalRuntime {
        calls: Rc::new(Cell::new(0)),
    });
    for (operation, error) in [
        (
            "session/load",
            runtime.load_session("s", ".").await.unwrap_err(),
        ),
        (
            "session/resume",
            runtime.resume_session("s", ".").await.unwrap_err(),
        ),
        (
            "session/request_permission",
            runtime
                .respond_to_agent_request(json!(7), Value::Null)
                .await
                .unwrap_err(),
        ),
    ] {
        assert_eq!(
            error.to_string(),
            format!("operation `{operation}` is unsupported by fixture")
        );
        assert!(
            matches!(error, RuntimeError::Unsupported { operation: actual, protocol }
            if actual == operation && protocol == "fixture")
        );
    }
}
