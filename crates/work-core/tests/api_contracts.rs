//! Contract tests deliberately depend on work-core alone, not a client or runtime.
use serde_json::{Value, json};
use std::{
    cell::Cell,
    rc::Rc,
    task::{Context, Poll, Waker},
};
use work_core::api::{
    Capabilities, Health, HostResponse, NodeApi, NodeError, NodeErrorKind, NodeFuture, Project,
    RuntimeState, SessionAgentBinding, SessionSummary, Snapshot, SnapshotEvent, ToolCatalog,
    UiDialogPayload,
};

#[test]
fn protocol_names_and_sparse_defaults_remain_compatible() {
    let health: Health =
        serde_json::from_value(json!({"service":"rpi-web","protocol":"rpi"})).unwrap();
    assert!(health.ok);
    assert_eq!(
        serde_json::to_value(health).unwrap(),
        json!({
            "ok":true,"service":"rpi-web","version":null,"protocol":"rpi"
        })
    );
    let capabilities: Capabilities = serde_json::from_value(json!({
        "protocol":"rpi","supportedCommands":["get_state"],"supportedActions":["open"]
    }))
    .unwrap();
    assert_eq!(
        serde_json::to_value(capabilities).unwrap(),
        json!({
            "protocol":"rpi","supportedCommands":["get_state"],"supportedActions":["open"],
            "piCompatibilityTarget":null,"semantics":null,"limits":null,"parityComplete":false
        })
    );
    let summary: SessionSummary = serde_json::from_value(json!({
        "sessionId":"s","projectId":"p","title":"Session","status":"idle",
        "createdAt":1,"updatedAt":2,"lastSeq":3
    }))
    .unwrap();
    assert_eq!(
        serde_json::to_value(summary).unwrap(),
        json!({
            "sessionId":"s","projectId":"p","title":"Session","status":"idle",
            "createdAt":1,"updatedAt":2,"lastSeq":3,"error":null,"pendingResume":false,
            "latestSettlement":null,"archived":false,"pinned":false
        })
    );
    let binding: SessionAgentBinding = serde_json::from_value(json!({
        "backend":"codex","externalSessionId":"opaque-id"
    }))
    .unwrap();
    assert_eq!(
        serde_json::to_value(binding).unwrap(),
        json!({
            "backend":"codex","externalSessionId":"opaque-id"
        })
    );
    let state: RuntimeState = serde_json::from_value(json!({"modelId":"model"})).unwrap();
    assert_eq!(state.active_tools, Vec::<String>::new());
    let tools: ToolCatalog = serde_json::from_value(json!({"tools":["read"]})).unwrap();
    assert_eq!(
        serde_json::to_value(tools).unwrap(),
        json!({"tools":["read"],"activeTools":[]})
    );
}

#[test]
fn dialog_payloads_preserve_unknown_runtime_content() {
    let dialog = json!({
        "question":"Choose", "options":["Rust",{"label":"Other","value":7,"extra":true}],
        "allow_multiple":true,"allow_custom":true,
        "questions":[{"question":"Nested"}],"future":{"version":2}
    });
    let ui: UiDialogPayload = serde_json::from_value(dialog.clone()).unwrap();
    assert!(ui.allow_multiple && ui.allow_freeform);
    assert_eq!(serde_json::to_value(ui).unwrap(), dialog);
    let event: SnapshotEvent = serde_json::from_value(json!({
        "seq":9,"event":{"type":"ready","opaque":true}
    }))
    .unwrap();
    assert_eq!(
        serde_json::to_value(event).unwrap(),
        json!({
            "seq":9,"event":{"type":"ready","opaque":true}
        })
    );
}

#[test]
fn generic_host_responses_keep_ids_and_data_types() {
    let value = json!({"type":"response","id":12,"command":"get_messages",
        "success":true,"data":{"messages":[],"total":0},"error":null});
    let response: HostResponse<Value> = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(response).unwrap(), value);
    let error = NodeError::new(NodeErrorKind::Unsupported, "not supported");
    assert_eq!(error.to_string(), "not supported");
    assert!(std::error::Error::source(&error).is_none());
}

// Rc/Cell intentionally make the fake !Send. A node service port must remain
// usable on the same local executors as the existing native adapters.
struct MemoryNode {
    calls: Rc<Cell<usize>>,
    summary: SessionSummary,
    snapshot: Snapshot,
}

impl MemoryNode {
    fn record(&self) {
        self.calls.set(self.calls.get() + 1);
    }

    fn check(&self, session_id: &str) -> Result<(), NodeError> {
        if session_id == self.summary.session_id {
            Ok(())
        } else {
            Err(NodeError::new(NodeErrorKind::NotFound, "not found"))
        }
    }
}

impl NodeApi for MemoryNode {
    fn capabilities(&self) -> NodeFuture<'_, Capabilities> {
        Box::pin(async move {
            self.record();
            Ok(serde_json::from_value(json!({"protocol":"fixture"})).unwrap())
        })
    }

    fn projects(&self) -> NodeFuture<'_, Vec<Project>> {
        Box::pin(async move {
            self.record();
            Ok(vec![Project {
                id: "p".into(),
                name: "Project".into(),
                path: "/fixture".into(),
            }])
        })
    }

    fn sessions(&self) -> NodeFuture<'_, Vec<SessionSummary>> {
        Box::pin(async move {
            self.record();
            Ok(vec![self.summary.clone()])
        })
    }

    fn open_session<'a>(&'a self, session_id: &'a str) -> NodeFuture<'a, SessionSummary> {
        Box::pin(async move {
            self.record();
            self.check(session_id)?;
            Ok(self.summary.clone())
        })
    }

    fn snapshot<'a>(&'a self, session_id: &'a str) -> NodeFuture<'a, Snapshot> {
        Box::pin(async move {
            self.record();
            self.check(session_id)?;
            Ok(self.snapshot.clone())
        })
    }
}

fn ready<T>(mut future: NodeFuture<'_, T>) -> Result<T, NodeError> {
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("the memory fixture has no asynchronous work"),
    }
}

#[test]
fn node_port_is_object_safe_lazy_and_does_not_require_send() {
    let calls = Rc::new(Cell::new(0));
    let node: Rc<dyn NodeApi> = Rc::new(MemoryNode {
        calls: calls.clone(),
        summary: serde_json::from_value(json!({"sessionId":"s","status":"idle",
            "createdAt":1,"updatedAt":1,"lastSeq":0}))
        .unwrap(),
        snapshot: serde_json::from_value(json!({"sessionId":"s"})).unwrap(),
    });
    let future = node.projects();
    assert_eq!(
        calls.get(),
        0,
        "creating a future must not start the operation"
    );
    assert_eq!(ready(future).unwrap()[0].id, "p");
    assert_eq!(calls.get(), 1);
    assert_eq!(ready(node.capabilities()).unwrap().protocol, "fixture");
    let session = ready(node.sessions()).unwrap().remove(0);
    assert_eq!(ready(node.open_session("s")).unwrap(), session);
    assert_eq!(ready(node.snapshot("s")).unwrap().session_id, "s");
    assert_eq!(
        ready(node.open_session("missing")).unwrap_err().kind,
        NodeErrorKind::NotFound
    );
    assert_eq!(
        ready(node.snapshot("missing")).unwrap_err().kind,
        NodeErrorKind::NotFound
    );
    assert_eq!(calls.get(), 7);
}

#[test]
fn shared_command_payload_constructor_keeps_overwrite_and_non_object_semantics() {
    use work_core::api::payload_with_type;
    assert_eq!(
        payload_with_type("prompt", json!({"type":"old", "message":"same"})),
        json!({"type":"prompt", "message":"same"})
    );
    for payload in [
        Value::Null,
        json!("ignored"),
        json!([1]),
        json!(false),
        json!(1),
    ] {
        assert_eq!(
            payload_with_type("get_state", payload),
            json!({"type":"get_state"})
        );
    }
}
