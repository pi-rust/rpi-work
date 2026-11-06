use serde_json::{Value, json};
use work_core::api::realtime::*;

fn operation() -> OperationRef {
    OperationRef {
        owner_epoch: "epoch-1".into(),
        operation_id: "op-1".into(),
    }
}
fn hello() -> Hello {
    Hello {
        version: VERSION,
        node_id: "node-1".into(),
        owner_epoch: "epoch-1".into(),
        connection_id: "connection-1".into(),
        capabilities: Capabilities::default(),
    }
}
fn event() -> WireEvent {
    WireEvent {
        id: Some("event-1".into()),
        seq: Some(u64::MAX),
        event_type: "message_update".into(),
        data: json!({"opaque": [null, true, {"password": "private-payload"}]}),
    }
}
fn clients() -> Vec<ClientFrame> {
    vec![
        ClientFrame::Reserve {
            version: VERSION,
            request_id: "r".into(),
            session_id: "s".into(),
            client_instance_id: "client-1".into(),
            kind: RealtimeOperationKind::SessionCommand,
        },
        ClientFrame::Execute {
            version: VERSION,
            request_id: "r".into(),
            operation: operation(),
            payload: json!({"type": "prompt", "unknownFutureField": [1, "private-payload"]}),
        },
        ClientFrame::Status {
            version: VERSION,
            request_id: "r".into(),
            operation: operation(),
        },
        ClientFrame::Subscribe {
            version: VERSION,
            request_id: "r".into(),
            session_id: "s".into(),
            subscription_id: "sub".into(),
            after_seq: Some(u64::MAX),
        },
        ClientFrame::Unsubscribe {
            version: VERSION,
            request_id: "r".into(),
            subscription_id: "sub".into(),
        },
        ClientFrame::Ping {
            version: VERSION,
            request_id: "heartbeat-1".into(),
        },
        ClientFrame::Query {
            version: VERSION,
            request_id: "query-1".into(),
            session_id: "s".into(),
            payload: json!({"type":"get_state", "opaque":"private-payload"}),
        },
    ]
}
fn servers() -> Vec<ServerFrame> {
    vec![
        ServerFrame::Hello {
            version: VERSION,
            hello: hello(),
        },
        ServerFrame::Reserved {
            version: VERSION,
            request_id: "r".into(),
            operation: operation(),
        },
        ServerFrame::Status {
            version: VERSION,
            request_id: "r".into(),
            operation: operation(),
            status: OperationStatus::KnownResult {
                outcome: OperationOutcome::Succeeded {
                    value: json!({"private-payload": true}),
                },
            },
        },
        ServerFrame::Subscribed {
            version: VERSION,
            request_id: "r".into(),
            session_id: "s".into(),
            subscription_id: "sub".into(),
        },
        ServerFrame::Unsubscribed {
            version: VERSION,
            request_id: "r".into(),
            subscription_id: "sub".into(),
        },
        ServerFrame::Events {
            version: VERSION,
            session_id: "s".into(),
            subscription_id: "sub".into(),
            events: vec![event()],
        },
        ServerFrame::Error {
            version: VERSION,
            request_id: None,
            error: RealtimeError {
                code: RealtimeErrorCode::Internal,
                message: "private-payload".into(),
            },
        },
        ServerFrame::Pong {
            version: VERSION,
            request_id: "heartbeat-1".into(),
        },
        ServerFrame::Result {
            version: VERSION,
            request_id: "query-1".into(),
            value: json!({"type":"response", "success":false, "error":"private-payload"}),
        },
    ]
}

#[test]
fn all_frames_roundtrip_and_wire_names() {
    for frame in clients() {
        let text = frame.encode_text().unwrap();
        assert_eq!(ClientFrame::decode_text(&text).unwrap(), frame);
        assert!(!text.contains("request_id"));
    }
    for frame in servers() {
        let text = frame.encode_text().unwrap();
        assert_eq!(ServerFrame::decode_text(&text).unwrap(), frame);
    }
    let subscribe = serde_json::to_value(&clients()[3]).unwrap();
    assert_eq!(subscribe["afterSeq"], json!(u64::MAX));
    assert_eq!(subscribe["subscriptionId"], "sub");
    let events = serde_json::to_value(&servers()[5]).unwrap();
    assert_eq!(events["events"][0]["seq"], json!(u64::MAX));
    // Existing WireEvent shape and payload are deliberately not rewritten.
    assert_eq!(events["events"][0]["event_type"], "message_update");
    assert_eq!(events["events"][0]["data"], event().data);
    assert!(events.get("seq").is_none());
}

#[test]
fn operation_contracts_roundtrip() {
    let statuses = [
        OperationStatus::Reserved,
        OperationStatus::Dispatching,
        OperationStatus::OutcomeUnknown,
        OperationStatus::KnownResult {
            outcome: OperationOutcome::Succeeded { value: Value::Null },
        },
        OperationStatus::KnownResult {
            outcome: OperationOutcome::Succeeded {
                value: json!({"type":"response","command":"prompt","success":false,"data":{"accepted":false},"error":"rejected"}),
            },
        },
        OperationStatus::KnownResult {
            outcome: OperationOutcome::Failed {
                error: RealtimeError {
                    code: RealtimeErrorCode::Conflict,
                    message: "safe error".into(),
                },
            },
        },
    ];
    for status in statuses {
        let text = serde_json::to_string(&status).unwrap();
        assert_eq!(
            serde_json::from_str::<OperationStatus>(&text).unwrap(),
            status
        );
    }
    assert_eq!(
        serde_json::to_value(OperationStatus::OutcomeUnknown).unwrap(),
        json!({"state":"outcomeUnknown"})
    );
    assert_eq!(
        serde_json::to_value(operation()).unwrap(),
        json!({"ownerEpoch":"epoch-1","operationId":"op-1"})
    );
    for kind in [
        RealtimeOperationKind::SessionCommand,
        RealtimeOperationKind::SetConfiguration,
        RealtimeOperationKind::DialogResponse,
        RealtimeOperationKind::CancelDialog,
    ] {
        assert_eq!(
            serde_json::from_value::<RealtimeOperationKind>(serde_json::to_value(kind).unwrap())
                .unwrap(),
            kind
        );
    }
    for code in [
        RealtimeErrorCode::InvalidInput,
        RealtimeErrorCode::NotFound,
        RealtimeErrorCode::AccessDenied,
        RealtimeErrorCode::Conflict,
        RealtimeErrorCode::Unsupported,
        RealtimeErrorCode::UnsupportedContent,
        RealtimeErrorCode::Unavailable,
        RealtimeErrorCode::LimitExceeded,
        RealtimeErrorCode::OutcomeUnknown,
        RealtimeErrorCode::Internal,
    ] {
        assert_eq!(
            serde_json::from_value::<RealtimeErrorCode>(serde_json::to_value(code).unwrap())
                .unwrap(),
            code
        );
    }
}

#[test]
fn strict_unknown_fields_on_every_envelope() {
    for frame in clients() {
        let mut value = serde_json::to_value(frame).unwrap();
        value["private-unknown"] = json!(true);
        let text = value.to_string();
        assert!(serde_json::from_str::<ClientFrame>(&text).is_err());
        assert!(ClientFrame::decode_text(&text).is_err());
    }
    for frame in servers() {
        let mut value = serde_json::to_value(frame).unwrap();
        value["private-unknown"] = json!(true);
        let text = value.to_string();
        assert!(serde_json::from_str::<ServerFrame>(&text).is_err());
        assert!(ServerFrame::decode_text(&text).is_err());
    }
    assert!(
        serde_json::from_value::<OperationRef>(
            json!({"ownerEpoch":"e","operationId":"o","extra":1})
        )
        .is_err()
    );
    assert!(
        serde_json::from_value::<OperationOutcome>(
            json!({"status":"succeeded","value":null,"extra":1})
        )
        .is_err()
    );
    assert!(
        serde_json::from_value::<OperationStatus>(json!({"state":"reserved","extra":1})).is_err()
    );
    assert!(
        serde_json::from_value::<RealtimeError>(
            json!({"code":"invalidInput","message":"x","extra":1})
        )
        .is_err()
    );
    let mut discovery =
        serde_json::to_value(Discovery::new("node-1".into(), "epoch-1".into())).unwrap();
    discovery["extra"] = json!(1);
    assert!(Discovery::decode_text(&discovery.to_string()).is_err());
    let mut greeting = serde_json::to_value(hello()).unwrap();
    greeting["extra"] = json!(1);
    assert!(Hello::decode_text(&greeting.to_string()).is_err());
}

#[test]
fn reject_bad_versions_and_ids_even_for_constructed_frames() {
    for frame in clients() {
        let value = serde_json::to_value(&frame).unwrap();
        let mut bad = value.clone();
        bad["version"] = json!(2);
        let parsed: ClientFrame = serde_json::from_value(bad.clone()).unwrap();
        assert_eq!(
            parsed.validate().unwrap_err().code,
            RealtimeErrorCode::Unsupported
        );
        assert!(parsed.encode_text().is_err());
        assert!(ClientFrame::decode_text(&bad.to_string()).is_err());
        for key in [
            "requestId",
            "sessionId",
            "subscriptionId",
            "clientInstanceId",
        ] {
            if value.get(key).is_none() {
                continue;
            }
            for id in [
                String::new(),
                "   ".into(),
                "secret\nline".into(),
                "x\u{7f}".into(),
                "x".repeat(MAX_ID_BYTES + 1),
            ] {
                let mut bad = value.clone();
                bad[key] = json!(id);
                assert!(ClientFrame::decode_text(&bad.to_string()).is_err());
            }
        }
    }
    for frame in servers() {
        let mut value = serde_json::to_value(frame).unwrap();
        value["version"] = json!(0);
        let frame: ServerFrame = serde_json::from_value(value.clone()).unwrap();
        assert!(frame.validate().is_err());
        assert!(ServerFrame::decode_text(&value.to_string()).is_err());
    }
    for reference in [
        OperationRef {
            owner_epoch: String::new(),
            operation_id: "o".into(),
        },
        OperationRef {
            owner_epoch: "e".into(),
            operation_id: "o\n".into(),
        },
    ] {
        assert!(reference.validate().is_err());
        let frame = ClientFrame::Execute {
            version: VERSION,
            request_id: "r".into(),
            operation: reference,
            payload: Value::Null,
        };
        assert!(frame.validate().is_err());
    }
    let mut greeting = hello();
    greeting.version = 2;
    assert!(greeting.validate().is_err());
    let mut discovery = Discovery::new("node-1".into(), "epoch-1".into());
    discovery.version = 2;
    assert!(discovery.validate().is_err());
}

#[test]
fn opaque_payload_roundtrips_but_debug_and_errors_are_redacted() {
    for frame in clients() {
        let text = frame.encode_text().unwrap();
        assert_eq!(
            serde_json::to_value(ClientFrame::decode_text(&text).unwrap()).unwrap(),
            serde_json::to_value(&frame).unwrap()
        );
        assert!(!format!("{frame:?}").contains("private-payload"));
    }
    for frame in servers() {
        assert!(!format!("{frame:?}").contains("private-payload"));
    }
    let error = ClientFrame::decode_text(r#"{"type":"private-payload"}"#).unwrap_err();
    assert!(!error.message.contains("private-payload"));
    assert!(!format!("{error:?} {error}").contains("private-payload"));
    let outcome = OperationOutcome::Succeeded {
        value: json!("private-payload"),
    };
    assert!(!format!("{outcome:?}").contains("private-payload"));
}

#[test]
fn message_byte_and_event_count_budgets() {
    let oversized = " ".repeat(MAX_MESSAGE_BYTES + 1);
    assert_eq!(
        ClientFrame::decode_text(&oversized).unwrap_err().code,
        RealtimeErrorCode::LimitExceeded
    );
    assert_eq!(
        ServerFrame::decode_text(&oversized).unwrap_err().code,
        RealtimeErrorCode::LimitExceeded
    );
    let mut frame = clients().remove(1);
    if let ClientFrame::Execute { payload, .. } = &mut frame {
        *payload = json!("x".repeat(MAX_MESSAGE_BYTES));
    }
    assert_eq!(
        frame.validate().unwrap_err().code,
        RealtimeErrorCode::LimitExceeded
    );
    assert!(frame.encode_text().is_err());
    for count in [0, MAX_EVENTS_PER_FRAME, MAX_EVENTS_PER_FRAME + 1] {
        let frame = ServerFrame::Events {
            version: VERSION,
            session_id: "s".into(),
            subscription_id: "sub".into(),
            events: vec![event(); count],
        };
        assert_eq!(frame.validate().is_ok(), count == MAX_EVENTS_PER_FRAME);
        let text = serde_json::to_string(&frame).unwrap();
        assert_eq!(
            ServerFrame::decode_text(&text).is_ok(),
            count == MAX_EVENTS_PER_FRAME
        );
    }
}

#[test]
fn discovery_and_hello_advertise_json_only() {
    assert_eq!(VERSION, 1);
    assert_eq!(PATH, "/api/v1/realtime");
    assert_eq!(SUBPROTOCOL, "rpi-work.realtime.v1");
    assert_eq!(MAX_MESSAGE_BYTES, 2 * 1024 * 1024);
    let discovery = Discovery::new("node-1".into(), "epoch-1".into());
    assert_eq!(
        Discovery::decode_text(&discovery.encode_text().unwrap()).unwrap(),
        discovery
    );
    let greeting = hello();
    assert_eq!(
        Hello::decode_text(&greeting.encode_text().unwrap()).unwrap(),
        greeting
    );
    assert!(discovery.capabilities.json_text);
    assert!(!discovery.capabilities.binary);
    assert!(!greeting.capabilities.binary);
    let mut unsupported = greeting;
    unsupported.capabilities.binary = true;
    assert!(unsupported.validate().is_err());
}

#[test]
fn queries_are_whitelisted_and_older_capabilities_do_not_claim_them() {
    for command in [
        "prompt",
        "abort",
        "set_model",
        "ui_dialog_response",
        "get_dangerous_extension",
        "get_state ",
    ] {
        let frame = ClientFrame::Query {
            version: VERSION,
            request_id: "q".into(),
            session_id: "s".into(),
            payload: json!({"type":command}),
        };
        assert!(frame.encode_text().is_err());
        assert!(ClientFrame::decode_text(&serde_json::to_string(&frame).unwrap()).is_err());
    }
    let mut old = serde_json::to_value(Capabilities::default()).unwrap();
    old.as_object_mut().unwrap().remove("commandQueries");
    let old: Capabilities = serde_json::from_value(old).unwrap();
    assert!(!old.command_queries);
    old.validate().unwrap();
    assert!(Capabilities::default().command_queries);
}
