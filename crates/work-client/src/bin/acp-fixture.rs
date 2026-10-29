use std::io::{self, BufRead, Write};

use serde_json::{Value, json};

fn permission_lifecycle_fixture() {
    if std::env::args().any(|arg| arg == "--pause-on-permission") {
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
    if std::env::args().any(|arg| arg == "--exit-on-permission") {
        std::process::exit(17);
    }
}

fn initialize(stdout: &mut impl Write, request: &Value, capabilities: &Value) {
    assert_eq!(
        request.pointer("/params/clientCapabilities"),
        Some(&json!({}))
    );
    emit(
        stdout,
        json!({"jsonrpc":"2.0", "id":request["id"], "result":{
            "protocolVersion":1, "agentInfo":{"name":"fixture-agent","version":"1"},
            "agentCapabilities":capabilities
        }}),
    );
}

fn config_options(session_id: &str) -> Value {
    if session_id == "no-config" {
        return json!([]);
    }
    let thinking_id = if session_id == "alias-session" {
        "thought_level"
    } else {
        "thinking"
    };
    json!([
        {"id":"model","name":"Model","category":"model","type":"select",
            "currentValue":"fixture-model","options":[{"value":"fixture-model","name":"Fixture"}]},
        {"id":thinking_id,"name":"Thinking","category":"thought_level","type":"select",
            "currentValue":"high","options":[{"value":"high","name":"High"}]}
    ])
}

fn session_response(stdout: &mut impl Write, request: &Value) {
    let session_id = request
        .pointer("/params/sessionId")
        .and_then(Value::as_str)
        .unwrap_or("session-fixture");
    emit(
        stdout,
        json!({"jsonrpc":"2.0","id":request["id"],"result":{
            "sessionId":session_id,"configOptions":config_options(session_id)
        }}),
    );
}

fn set_config_response(stdout: &mut impl Write, request: &Value) {
    let session_id = request["params"]["sessionId"].as_str().unwrap();
    let mut options = config_options(session_id);
    let selected = options
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|option| option["id"] == request["params"]["configId"])
        .expect("negotiated config ID");
    selected["currentValue"] = request["params"]["value"].clone();
    if std::env::args().any(|arg| arg == "--drop-config-after-set") {
        options = json!([]);
    }
    emit(
        stdout,
        json!({"jsonrpc":"2.0","id":request["id"],"result":{"configOptions":options}}),
    );
}

fn main() {
    let supports_load = std::env::args().any(|arg| arg == "--load-session");
    let wait_permission = std::env::args().any(|arg| arg == "--wait-permission");
    let permission_id = if std::env::args().any(|arg| arg == "--string-permission-id") {
        json!("4")
    } else {
        json!(4)
    };
    let mut pending_prompt: Option<(Value, Value)> = None;
    let agent_capabilities = if supports_load {
        json!({"loadSession":true,"sessionCapabilities":{"resume":{}}})
    } else {
        json!({"loadSession":false})
    };
    let stdin = io::stdin();
    let mut stdout = io::BufWriter::new(io::stdout());
    for line in stdin.lock().lines().map_while(Result::ok) {
        let Ok(request) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        if wait_permission && request.get("method").is_none() {
            finish_permission(
                &mut stdout,
                &request,
                &permission_id,
                pending_prompt.take().expect("pending prompt"),
            );
            continue;
        }
        match request
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or_default()
        {
            "fixture_hang" => emit(
                &mut stdout,
                json!({"jsonrpc":"2.0","method":"session/update",
                "params":{"sessionId":"session-fixture","update":{"sessionUpdate":"agent_message_chunk",
                    "content":{"type":"text","text":"waiting"}}}}),
            ),
            "fixture_pause" => {
                emit(&mut stdout, json!({"jsonrpc":"2.0","id":id,"result":{}}));
                std::thread::sleep(std::time::Duration::from_secs(2));
            }
            "initialize" => initialize(&mut stdout, &request, &agent_capabilities),
            "session/new" | "session/load" | "session/resume" => {
                session_response(&mut stdout, &request);
            }
            "session/prompt" => {
                let session_id = request["params"]["sessionId"].clone();
                let options = if wait_permission {
                    json!([
                        {"optionId":"deny-7","name":"Reject","kind":"reject_once"},
                        {"optionId":"grant-42","name":"Allow","kind":"allow_once"}
                    ])
                } else {
                    json!([{"optionId":"allow","name":"Allow","kind":"allow_once"}])
                };
                emit(
                    &mut stdout,
                    json!({"jsonrpc":"2.0","id":permission_id,"method":"session/request_permission",
                        "params":{"sessionId":session_id,"toolCall":{"toolCallId":"tool-1","title":"fixture tool"},"options":options}}),
                );
                permission_lifecycle_fixture();
                if wait_permission {
                    assert!(pending_prompt.replace((id, session_id)).is_none());
                } else {
                    finish_prompt(&mut stdout, &id, &session_id, false);
                }
            }
            "session/set_config_option" => set_config_response(&mut stdout, &request),
            "session/cancel" => emit(
                &mut stdout,
                json!({
                    "jsonrpc":"2.0", "id":id, "result":{}
                }),
            ),
            _ => emit(
                &mut stdout,
                json!({
                    "jsonrpc":"2.0", "id":id, "error":{"code":-32601,"message":"unknown method"}
                }),
            ),
        }
    }
}

fn finish_permission(
    stdout: &mut impl Write,
    request: &Value,
    permission_id: &Value,
    (prompt_id, session_id): (Value, Value),
) {
    assert_eq!(
        &request["id"], permission_id,
        "response must preserve JSON-RPC ID type"
    );
    let outcome = &request["result"]["outcome"];
    assert!(
        outcome == &json!({"outcome":"selected","optionId":"grant-42"})
            || outcome == &json!({"outcome":"selected","optionId":"deny-7"})
            || outcome == &json!({"outcome":"cancelled"}),
        "unexpected ACP permission result"
    );
    finish_prompt(
        stdout,
        &prompt_id,
        &session_id,
        outcome["outcome"] == "cancelled",
    );
}

fn finish_prompt(stdout: &mut impl Write, id: &Value, session_id: &Value, cancelled: bool) {
    if !cancelled {
        for text in ["fixture ", "reply"] {
            emit(
                stdout,
                json!({
                    "jsonrpc":"2.0", "method":"session/update",
                    "params":{"sessionId":session_id,"update":{
                        "sessionUpdate":"agent_message_chunk","content":{"type":"text","text":text}
                    }}
                }),
            );
        }
    }
    emit(
        stdout,
        json!({"jsonrpc":"2.0", "id":id,
        "result":{"stopReason":if cancelled { "cancelled" } else { "end_turn" }}}),
    );
}

#[allow(clippy::needless_pass_by_value)]
fn emit(stdout: &mut impl Write, value: Value) {
    writeln!(stdout, "{value}").unwrap();
    stdout.flush().unwrap();
}
