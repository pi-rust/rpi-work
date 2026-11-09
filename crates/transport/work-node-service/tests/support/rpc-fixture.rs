// Native protocol fixture for the desktop HTTP adapter; never loads user data.
use std::io::{self, BufRead, Write};

use serde_json::{Value, json};

fn main() {
    let stdin = io::stdin();
    let mut stdout = io::BufWriter::new(io::stdout());
    let mut hanging = false;
    let mut held_run = false;
    // Only the dedicated seed session opts into bounded native reads. Existing
    // tests retain their original advertisements, including get_commands.
    let native_queries = std::env::args().any(|arg| arg.contains("native-queries-fixture"));
    let mut ready = json!({
        "type":"ready", "protocol":"rpi", "piCompatibilityTarget":"1.2.0",
        "supportedCommands":["get_state","get_commands","get_messages","get_history","get_session_branch","get_stats","get_queue","queue_message","cancel_queued","clear_queue","get_available_models","get_auth_status","set_api_key","get_available_thinking_levels","set_model","set_thinking_level","get_available_tools","set_active_tools","prompt","abort","ui_dialog_response","stop"],
        "semantics":{"settlementEvent":"agent_settled"}
    });
    if native_queries {
        let commands = ready["supportedCommands"].as_array_mut().unwrap();
        commands.extend([json!("get_message_summaries"), json!("get_message_window")]);
    }
    emit(&mut stdout, ready);
    for line in stdin.lock().lines().map_while(Result::ok) {
        let Ok(request) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let kind = request
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        match kind {
            "get_message_summaries" | "get_message_window" if native_queries => {
                // Contract fixture pages; never reads a real transcript or models performance.
                let failure = if request["leafId"] == "conflict" {
                    Some("invalid_query: branch_changed: fixture leaf changed")
                } else if ["beforeId", "anchorId"]
                    .iter()
                    .any(|key| request[*key] == "host-failure")
                {
                    Some("invalid_query: fixture read failed")
                } else {
                    None
                };
                let mut echoed = request.clone();
                echoed.as_object_mut().unwrap().remove("id");
                let mut data = json!({"request":echoed,"fixturePid":std::process::id(),"nativeSentinel":"unchanged"});
                let leaf = request
                    .get("leafId")
                    .cloned()
                    .unwrap_or_else(|| json!("fixture-leaf"));
                let page = if kind == "get_message_summaries" {
                    json!({"filtered":true,"role":"user","messages":[{"entryId":"fixture-user","role":"user","preview":"fixture question","timestamp":0,"hasImages":false}],
                        "firstId":"fixture-user","lastId":"fixture-user","nextBeforeId":null,"hasMore":false,"leafId":leaf})
                } else {
                    let entry = request
                        .get("anchorId")
                        .cloned()
                        .unwrap_or_else(|| json!("fixture-window"));
                    json!({"messages":[{"entryId":entry,"role":"user","content":"fixture question"}],
                        "firstId":entry,"lastId":entry,"nextBeforeId":null,"nextAfterId":null,"hasMoreBefore":false,"hasMoreAfter":false,"leafId":leaf})
                };
                data.as_object_mut()
                    .unwrap()
                    .extend(page.as_object().unwrap().clone());
                emit(
                    &mut stdout,
                    if let Some(error) = failure {
                        json!({"type":"response","id":id,"command":kind,"success":false,"error":error})
                    } else {
                        json!({"type":"response","id":id,"command":kind,"success":true,"data":data})
                    },
                );
            }
            "fixture_pause" => {
                emit(
                    &mut stdout,
                    json!({"type":"response","id":id,"command":kind,"success":true,"data":{}}),
                );
                std::thread::sleep(std::time::Duration::from_secs(2));
            }
            "fixture_crash" => std::process::exit(17),
            "fixture_hang" => {
                hanging = true;
                emit(&mut stdout, json!({"type":"fixture_waiting"}));
            }
            "stop" if hanging => {}
            "prompt" if request["message"] == "headless-owner-hold-fixture" => {
                held_run = true;
                emit(
                    &mut stdout,
                    json!({"type":"response","id":id,"command":"prompt","success":true,"data":{"accepted":true}}),
                );
                emit(
                    &mut stdout,
                    json!({"type":"agent_start","eventId":"headless-run-start"}),
                );
            }
            "abort" if held_run => {
                held_run = false;
                emit(
                    &mut stdout,
                    json!({"type":"response","id":id,"command":"abort","success":true,"data":{"accepted":true}}),
                );
                emit(
                    &mut stdout,
                    json!({"type":"agent_end","eventId":"headless-run-end"}),
                );
                emit(
                    &mut stdout,
                    json!({"type":"agent_settled","eventId":"headless-run-settled","outcome":"cancelled"}),
                );
            }
            "prompt"
                if request["message"]
                    .as_str()
                    .is_some_and(|s| s.starts_with("realtime-auth-gate:")) =>
            {
                // Deterministic test-only gate: parent revokes the HTTP login
                // after observing the write, then releases this temporary file.
                let path = request["message"]
                    .as_str()
                    .unwrap()
                    .strip_prefix("realtime-auth-gate:")
                    .unwrap();
                emit(&mut stdout, json!({"type":"fixture_started"}));
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
                while !std::path::Path::new(path).exists() && std::time::Instant::now() < deadline {
                    std::thread::sleep(std::time::Duration::from_millis(2));
                }
                emit_prompt(&mut stdout, &id);
            }
            "prompt" if request["message"] == "realtime-ws-burst-fixture" => {
                emit(
                    &mut stdout,
                    json!({"type":"response","id":id,"command":"prompt","success":true,"data":{"accepted":true}}),
                );
                let delta = "x".repeat(64 * 1024);
                for i in 0..128 {
                    emit(
                        &mut stdout,
                        json!({"type":"message_update","eventId":format!("ws-burst-{i}"),"eventType":"text_delta","delta":delta,"message":{"role":"assistant","entryId":"ws-burst-message"}}),
                    );
                }
                emit(
                    &mut stdout,
                    json!({"type":"agent_settled","eventId":"ws-burst-settled","outcome":"completed"}),
                );
            }
            "prompt" => emit_prompt(&mut stdout, &id),
            "stop" => {
                emit(
                    &mut stdout,
                    json!({"type":"response","id":id,"command":"stop","success":true,"data":{}}),
                );
                break;
            }
            "set_model" | "set_thinking_level" => emit(
                &mut stdout,
                json!({"type":"response","id":id,"command":kind,"success":true,"data":{"request":request,"fixturePid":std::process::id()}}),
            ),
            "get_state" => emit(
                &mut stdout,
                json!({"type":"response","id":id,"command":"get_state","success":true,"data":{"accepted":true,"sessionId":"fixture-session","fixturePid":std::process::id(),"isRunning":held_run,"isStreaming":held_run,"context":{"fixturePid":std::process::id(),"isRunning":held_run,"isStreaming":held_run}}}),
            ),
            "get_messages" => emit(
                &mut stdout,
                json!({"type":"response","id":id,"command":"get_messages","success":true,"data":{"messages":[],"total":0,"truncated":false,"pendingResume":false,"hasMore":false}}),
            ),
            "get_history" => emit(
                &mut stdout,
                json!({"type":"response","id":id,"command":"get_history","success":true,"data":{"records":[],"total":0,"hasMore":false}}),
            ),
            "get_session_branch" => emit_branch(&mut stdout, &request),
            "get_queue" => emit(
                &mut stdout,
                json!({"type":"response","id":id,"command":"get_queue","success":true,"data":{"items":[],"count":0,"includeCompleted":false}}),
            ),
            "get_available_models" => emit(
                &mut stdout,
                json!({"type":"response","id":id,"command":"get_available_models","success":true,"data":{"models":[]}}),
            ),
            "get_available_thinking_levels" => emit(
                &mut stdout,
                json!({"type":"response","id":id,"command":"get_available_thinking_levels","success":true,"data":{"levels":[]}}),
            ),
            "get_available_tools" => emit(
                &mut stdout,
                json!({"type":"response","id":id,"command":"get_available_tools","success":true,"data":{"tools":[],"activeTools":[]}}),
            ),
            "get_auth_status" => emit(
                &mut stdout,
                json!({"type":"response","id":id,"command":"get_auth_status","success":true,"data":{"providers":[]}}),
            ),
            "get_stats" => emit(
                &mut stdout,
                json!({"type":"response","id":id,"command":"get_stats","success":true,"data":{"messageCount":0}}),
            ),
            "ui_dialog_response" => emit(
                &mut stdout,
                json!({"type":"response","id":id,"command":kind,"success":true,"data":{"accepted":true,"request":request}}),
            ),
            _ => emit(
                &mut stdout,
                json!({"type":"response","id":id,"command":kind,"success":true,"data":{"accepted":true}}),
            ),
        }
    }
}

fn emit_prompt(stdout: &mut impl Write, id: &Value) {
    for event in [
        json!({"type":"response","id":id,"command":"prompt","success":true,"data":{"accepted":true}}),
        json!({"type":"agent_start","eventId":"run-1-start"}),
        json!({"type":"message_update","eventType":"text_delta","delta":"fixture reply","message":{"role":"assistant","entryId":"message-1"}}),
        json!({"type":"agent_end","eventId":"run-1-end"}),
        json!({"type":"agent_settled","eventId":"run-1-settled","outcome":"completed"}),
    ] {
        emit(stdout, event);
    }
}

fn emit_branch(stdout: &mut impl Write, request: &Value) {
    // Match the strict custom-entry-only runtime contract, not an unrestricted
    // full-branch success that could hide incompatible compaction reads.
    let valid_types = request
        .get("customTypes")
        .and_then(Value::as_array)
        .is_some_and(|types| {
            !types.is_empty()
                && types
                    .iter()
                    .all(|value| value.as_str().is_some_and(|value| !value.trim().is_empty()))
        });
    let id = &request["id"];
    let response = if valid_types {
        json!({"type":"response","id":id,"command":"get_session_branch","success":true,
            "data":{"entries":[],"count":0,"filtered":true}})
    } else {
        json!({"type":"response","id":id,"command":"get_session_branch","success":false,
            "error":"get_session_branch: `customTypes` is required and must be a non-empty string array"})
    };
    emit(stdout, response);
}

#[allow(clippy::needless_pass_by_value)]
fn emit(stdout: &mut impl Write, value: Value) {
    writeln!(stdout, "{value}").unwrap();
    stdout.flush().unwrap();
}
