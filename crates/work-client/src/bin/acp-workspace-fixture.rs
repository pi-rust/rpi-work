//! Deterministic, credential-free fixture for local workspace ownership tests.
use serde_json::{Value, json};
use std::{
    io::{self, BufRead, Write},
    path::Path,
    time::Duration,
};

enum Pending {
    Load { id: Value, session: Value },
    Prompt { id: Value, permissions: Vec<Value> },
}

fn emit(value: &Value) {
    let mut out = io::stdout().lock();
    writeln!(out, "{value}").unwrap();
    out.flush().unwrap();
}
fn reply(id: &Value, result: &Value) {
    emit(&json!({"jsonrpc":"2.0","id":id,"result":result}));
}
fn delta(session: &Value, text: &str) {
    emit(
        &json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":session,"update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":text}}}}),
    );
}
fn wait_for(name: &str) {
    while !Path::new(name).exists() {
        std::thread::sleep(Duration::from_millis(5));
    }
}
// One ordered dispatcher deliberately models concurrent outstanding ACP requests.
#[allow(clippy::too_many_lines)]
fn main() {
    std::fs::write("fixture.pid", std::process::id().to_string()).unwrap();
    let mut pending = None;
    let mut selected_config = "balanced".to_owned();
    for line in io::stdin().lock().lines() {
        let request: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let id = &request["id"];
        match request["method"].as_str() {
            Some("initialize") => reply(
                id,
                &json!({"protocolVersion":1,"agentCapabilities":{"loadSession":true},"agentInfo":{"name":"workspace-fixture","version":"1"}}),
            ),
            Some("session/new") => {
                std::fs::write("new.ready", "").unwrap();
                if Path::new("hold-new").exists() {
                    wait_for("release-new");
                }
                reply(
                    id,
                    &json!({"sessionId":"external-fixture","configOptions":config_options(&selected_config)}),
                );
            }
            Some("session/load") => {
                assert!(pending.is_none());
                let session = &request["params"]["sessionId"];
                // Exceed Tokio's cooperative budget so replay spans multiple polls.
                for _ in 0..600 {
                    delta(session, "HISTORIC-REPLAY");
                }
                permission(session, &json!(7), "historical-tool");
                // Do not reply until the manager cancels the historical permission:
                // waiting for load completion before cancelling would deadlock.
                pending = Some(Pending::Load {
                    id: id.clone(),
                    session: session.clone(),
                });
            }
            Some("session/prompt") => {
                let session = &request["params"]["sessionId"];
                let text = request["params"]["prompt"][0]["text"].as_str().unwrap();
                std::fs::write("prompt.ready", "").unwrap();
                match text {
                    "crash" => std::process::exit(17),
                    "hold" => {
                        wait_for("release-prompt");
                        delta(session, "held-answer");
                    }
                    "permission" | "typed-permissions" => {
                        assert!(pending.is_none());
                        permission(session, &json!(7), "tool-number");
                        let mut permissions = vec![json!(7)];
                        if text == "typed-permissions" {
                            permission(session, &json!("7"), "tool-string");
                            permissions.push(json!("7"));
                        }
                        pending = Some(Pending::Prompt {
                            id: id.clone(),
                            permissions,
                        });
                        continue;
                    }
                    "flood" => {
                        for _ in 0..4100 {
                            delta(session, "x");
                            std::thread::sleep(Duration::from_millis(3));
                        }
                    }
                    _ => delta(session, "answer"),
                }
                reply(id, &json!({"stopReason":"end_turn"}));
            }
            Some("session/set_config_option") => {
                assert_eq!(request["params"]["sessionId"], "external-fixture");
                assert_eq!(request["params"]["configId"], "mode-choice");
                let value = request["params"]["value"].as_str().unwrap();
                assert!(matches!(value, "balanced" | "fast"));
                value.clone_into(&mut selected_config);
                reply(
                    id,
                    &json!({"configOptions":config_options(&selected_config), "fixturePid":std::process::id()}),
                );
            }
            Some("session/cancel") => reply(id, &json!({})),
            None if request.get("result").is_some() => {
                match pending.take().expect("response without pending operation") {
                    Pending::Load { id: load, session } => {
                        assert_eq!(id, &json!(7));
                        assert_eq!(request["result"]["outcome"]["outcome"], "cancelled");
                        reply(
                            &load,
                            &json!({"sessionId":session,"configOptions":config_options(&selected_config)}),
                        );
                        // A real non-text notification immediately after the wire boundary
                        // must survive suppression, even before the load caller wakes.
                        emit(
                            &json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":session,"update":{"sessionUpdate":"available_commands_update","availableCommands":[{"name":"post-load","description":"Live command after restore"}]}}}),
                        );
                    }
                    Pending::Prompt {
                        id: prompt,
                        mut permissions,
                    } => {
                        assert_eq!(request["result"]["outcome"]["outcome"], "selected");
                        assert_eq!(request["result"]["outcome"]["optionId"], "original-allow");
                        let position = permissions
                            .iter()
                            .position(|expected| expected == id)
                            .expect("unexpected or duplicate typed permission response");
                        permissions.remove(position);
                        if permissions.is_empty() {
                            reply(&prompt, &json!({"stopReason":"end_turn"}));
                        } else {
                            pending = Some(Pending::Prompt {
                                id: prompt,
                                permissions,
                            });
                        }
                    }
                }
            }
            _ => panic!("unexpected request {request}"),
        }
    }
}
fn permission(session: &Value, id: &Value, tool: &str) {
    emit(
        &json!({"jsonrpc":"2.0","id":id,"method":"session/request_permission","params":{"sessionId":session,"toolCall":{"toolCallId":tool,"title":"Allow?"},"options":[{"optionId":"original-allow","name":"Yes","kind":"allow_once"},{"optionId":"original-deny","name":"No","kind":"reject_once"}]}}),
    );
}

fn config_options(current: &str) -> Value {
    json!([{"id":"mode-choice", "name":"Mode", "type":"select", "currentValue":current,
        "options":[{"name":"Modes", "options":[{"value":"balanced", "name":"Balanced"}, {"value":"fast", "name":"Fast"}]}]}])
}
