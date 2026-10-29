use std::io::{self, BufRead, Write};

use serde_json::{Value, json};

fn main() {
    let stdin = io::stdin();
    let mut stdout = io::BufWriter::new(io::stdout());
    let mut hanging = false;
    writeln!(
        stdout,
        "{}",
        json!({
            "type":"ready",
            "protocol":"rpi",
            "piCompatibilityTarget":"1.2.0",
            "supportedCommands":["get_state","get_commands","get_messages","get_history","get_session_branch","get_stats","get_queue","queue_message","cancel_queued","clear_queue","get_available_models","get_auth_status","set_api_key","get_available_thinking_levels","set_model","set_thinking_level","get_available_tools","set_active_tools","prompt","abort","ui_dialog_response","stop"],
            "semantics":{"settlementEvent":"agent_settled"}
        })
    )
    .unwrap();
    stdout.flush().unwrap();
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
            "prompt" => emit_prompt(&mut stdout, &id),
            "stop" => {
                emit(
                    &mut stdout,
                    json!({"type":"response","id":id,"command":"stop","success":true,"data":{}}),
                );
                break;
            }
            "get_state" => emit(
                &mut stdout,
                json!({"type":"response","id":id,"command":"get_state","success":true,"data":{"accepted":true,"sessionId":"fixture-session"}}),
            ),
            "get_messages" => emit(
                &mut stdout,
                json!({"type":"response","id":id,"command":"get_messages","success":true,"data":{"messages":[],"total":0,"hasMore":false}}),
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
