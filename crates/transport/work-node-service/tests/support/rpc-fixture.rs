// Native protocol fixture for the desktop HTTP adapter; never loads user data.
use std::io::{self, BufRead, Write};

use serde_json::{Value, json};

// Opt-in native CLI fragment for browser creation integration only. It reports
// an empty synthetic catalogue, not installed/loaded resources, and advertises
// no install, exact-selection, build, network, or saved-session capabilities.
// All existing RPC-only fixtures keep their Unsupported behaviour unchanged.
fn creation_catalog_fixture() -> bool {
    let mut arguments = std::env::args();
    if !arguments
        .next()
        .is_some_and(|name| name.contains("creation-native-queries-fixture"))
    {
        return false;
    }
    let parameters: Vec<_> = arguments.collect();
    let output = match parameters.as_slice() {
        [flag] if flag == "--help" => "--resource-capabilities".to_owned(),
        [flag] if flag == "--resource-capabilities" => json!({
            "version":1,"type":"resource_capabilities","loadedFacts":false,
            "management":{"version":1,"command":"resource","operations":["list"],
                "scopes":["project","global"],"kinds":[],"explicitScopeRequired":true,
                "build":false,"network":false}
        })
        .to_string(),
        [command, operation, flag, scope]
            if command == "resource"
                && operation == "list"
                && flag == "--scope"
                && matches!(scope.as_str(), "project" | "global") =>
        {
            json!({
                "version":1,"success":true,"operation":"list","scope":scope,
                "kind":null,"name":null,"data":{"resources":[]}
            })
            .to_string()
        }
        _ => return false,
    };
    println!("{output}");
    true
}

#[allow(clippy::too_many_lines)]
fn main() {
    if creation_catalog_fixture() {
        return;
    }
    let stdin = io::stdin();
    let mut stdout = io::BufWriter::new(io::stdout());
    let mut hanging = false;
    let mut held_run = false;
    let mut resume_fixture = None;
    let hosted_creation = std::env::args()
        .next()
        .is_some_and(|name| name.contains("creation-native-queries-fixture"));
    let forked = hosted_creation && std::env::args().any(|arg| arg == "--fork");
    let session_id = if forked {
        format!("fixture-fork-{}", std::process::id())
    } else {
        "fixture-session".into()
    };
    let mut fork_at_answer = false;
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
    if hosted_creation {
        ready["supportedCommands"]
            .as_array_mut()
            .unwrap()
            .extend([json!("navigate"), json!("resume_pending")]);
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
            "prompt" if hosted_creation && request["message"] == "hosted-filter-permission" => {
                held_run = true;
                emit(
                    &mut stdout,
                    json!({"type":"response","id":id,"command":"prompt","success":true,"data":{"accepted":true}}),
                );
                emit(
                    &mut stdout,
                    json!({"type":"agent_start","eventId":"hosted-filter-start"}),
                );
                emit(
                    &mut stdout,
                    json!({"type":"ui_dialog_request","requestId":"hosted-filter-approval","ui":{"purpose":"permission","question":"托管筛选待审批"}}),
                );
            }
            "prompt"
                if hosted_creation
                    && matches!(
                        request["message"].as_str(),
                        Some("hosted-resume-ready" | "hosted-resume-skipped")
                    ) =>
            {
                resume_fixture = Some(request["message"] == "hosted-resume-ready");
                for event in [
                    json!({"type":"response","id":id,"command":"prompt","success":true,"data":{"accepted":true}}),
                    json!({"type":"agent_start","requestId":id}),
                    json!({"type":"agent_settled","requestId":id,"outcome":"aborted","pendingResume":true}),
                ] {
                    emit(&mut stdout, event);
                }
            }
            "resume_pending" if hosted_creation => {
                if resume_fixture.take() == Some(true) {
                    for event in [
                        json!({"type":"response","id":id,"command":kind,"success":true,"data":{"accepted":true}}),
                        json!({"type":"agent_start","requestId":id}),
                        json!({"type":"agent_settled","requestId":id,"outcome":"completed","pendingResume":false}),
                    ] {
                        emit(&mut stdout, event);
                    }
                } else {
                    emit(
                        &mut stdout,
                        json!({"type":"response","id":id,"command":kind,"success":true,"data":{"resumed":false}}),
                    );
                }
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
            "get_state" => {
                let mut data = json!({"accepted":true,"sessionId":session_id,"fixturePid":std::process::id(),"isRunning":held_run,"isStreaming":held_run,"context":{"fixturePid":std::process::id(),"isRunning":held_run,"isStreaming":held_run}});
                if hosted_creation {
                    // Native facts used by the real owner's idle retirement check.
                    data["activeOperation"] = if held_run {
                        json!({"id":"headless-run"})
                    } else {
                        Value::Null
                    };
                    data["queue"] = json!({"steering":[],"followUp":[]});
                }
                emit(
                    &mut stdout,
                    json!({"type":"response","id":id,"command":kind,"success":true,"data":data}),
                );
            }
            "navigate" if forked => {
                fork_at_answer = request["targetId"] == "fixture-fork-answer";
                let data = if fork_at_answer {
                    json!({"outcome":"navigated"})
                } else {
                    json!({"outcome":"failed","error":"unknown fixture fork anchor"})
                };
                emit(
                    &mut stdout,
                    json!({"type":"response","id":id,"command":kind,"success":fork_at_answer,"data":data}),
                );
            }
            "get_messages" => {
                let mut messages = Vec::new();
                if hosted_creation {
                    messages.push(json!({"entryId":"fixture-fork-user","role":"user","content":"托管历史问题"}));
                    messages.push(json!({"entryId":"fixture-fork-answer","role":"assistant","content":"托管历史回答"}));
                    if !fork_at_answer {
                        messages.push(json!({"entryId":"fixture-fork-later","role":"user","content":"托管历史后续"}));
                    }
                }
                emit(
                    &mut stdout,
                    json!({"type":"response","id":id,"command":kind,"success":true,"data":{"total":messages.len(),"messages":messages,"truncated":false,"pendingResume":resume_fixture.is_some(),"hasMore":false}}),
                );
            }
            "get_history" => {
                let mut data = json!({"records":[],"total":0,"hasMore":false});
                if hosted_creation {
                    data["openOperations"] = if held_run {
                        json!([{"id":"headless-run"}])
                    } else {
                        json!([])
                    };
                }
                emit(
                    &mut stdout,
                    json!({"type":"response","id":id,"command":kind,"success":true,"data":data}),
                );
            }
            "get_session_branch" => emit_branch(&mut stdout, &request),
            "get_queue" => {
                let items = if resume_fixture.is_some() {
                    vec![
                        json!({"entryId":"fixture-queued","queue":"followUp","text":"托管待恢复队列项","status":"queued"}),
                    ]
                } else {
                    vec![]
                };
                emit(
                    &mut stdout,
                    json!({"type":"response","id":id,"command":"get_queue","success":true,"data":{"items":items,"count":items.len(),"includeCompleted":false}}),
                );
            }
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
