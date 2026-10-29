use serde_json::json;
use std::{env, error::Error, process::ExitCode};
use work_client::{AgentClient, WorkClient};

#[tokio::main]
async fn main() -> Result<ExitCode, Box<dyn Error>> {
    let mut args = env::args().skip(1);
    let base_url = env::var("RPI_WORK_URL").unwrap_or_else(|_| "http://127.0.0.1:3030/".into());
    let transport = env::var("RPI_WORK_TRANSPORT").unwrap_or_else(|_| default_transport().into());
    let command = args
        .next()
        .unwrap_or_else(|| default_command(&transport).into());
    if transport != "http" {
        return run_native_agent(&transport, command, args.collect()).await;
    }
    let client = WorkClient::new(&base_url)?;

    match command.as_str() {
        "health" => print_json(&client.health().await?),
        "capabilities" => print_json(&client.capabilities().await?),
        "projects" => print_json(&client.projects().await?),
        "folders" => {
            let path = args.next().unwrap_or_default();
            print_json(&client.project_folders(&path).await?);
        }
        "sessions" => print_json(&client.sessions().await?),
        "state" => {
            let session_id = args.next().ok_or("state requires a session id")?;
            print_json(&client.runtime_state(&session_id).await?);
        }
        "history" => {
            let session_id = args.next().ok_or("history requires a session id")?;
            print_json(&client.history(&session_id, None, None, 100).await?);
        }
        "models" => {
            let session_id = args.next().ok_or("models requires a session id")?;
            print_json(&client.models(&session_id).await?);
        }
        "tools" => {
            let session_id = args.next().ok_or("tools requires a session id")?;
            print_json(&client.tools(&session_id).await?);
        }
        "watch" => {
            let session_id = args.next().ok_or("watch requires a session id")?;
            let mut connection = client.connect_session(session_id).await?;
            while let Some(event) = connection.next().await? {
                println!(
                    "{}",
                    serde_json::to_string(&json!({
                        "seq": event.wire.seq,
                        "id": event.wire.id,
                        "event": event.wire.event_type,
                        "data": event.wire.data,
                        "applied": format!("{:?}", event.applied),
                    }))?
                );
            }
        }
        _ => {
            eprintln!(
                "usage: work-cli [agent-info|load-session SESSION_ID|resume-session SESSION_ID|set-config OPTION VALUE|prompt MESSAGE] (native default: rpc; use RPI_WORK_TRANSPORT=http for HTTP commands)"
            );
            return Ok(ExitCode::from(2));
        }
    }
    Ok(ExitCode::SUCCESS)
}

#[allow(clippy::too_many_lines)]
async fn run_native_agent(
    transport: &str,
    command: String,
    arguments: Vec<String>,
) -> Result<ExitCode, Box<dyn Error>> {
    let executable = env::var_os("RPI_WORK_RPI_BIN").unwrap_or_else(|| "rpi".into());
    let cwd = env::var_os("RPI_WORK_PROJECT").map_or(env::current_dir()?, std::path::PathBuf::from);
    let client = match transport {
        "rpc" => AgentClient::spawn_rpc(&executable, &cwd).await?,
        "acp" => AgentClient::spawn_acp(&executable, &["--acp".into()], &cwd).await?,
        other => return Err(format!("unsupported RPI_WORK_TRANSPORT `{other}`").into()),
    };
    let result: Result<(), Box<dyn Error>> = match command.as_str() {
        "agent-info" => {
            let descriptor = client.descriptor();
            print_json(&json!({
                "identity": {
                    "protocol": descriptor.identity.protocol,
                    "version": descriptor.identity.version,
                    "transport": descriptor.identity.transport,
                    "agentName": descriptor.identity.agent_name,
                },
                "capabilities": {
                    "sessions": descriptor.capabilities.sessions,
                    "sessionLoad": descriptor.capabilities.session_load,
                    "sessionResume": descriptor.capabilities.session_resume,
                    "prompt": descriptor.capabilities.prompt,
                    "streaming": descriptor.capabilities.streaming,
                    "abort": descriptor.capabilities.abort,
                    "dialogs": descriptor.capabilities.dialogs,
                    "models": descriptor.capabilities.models,
                    "thinkingLevels": descriptor.capabilities.thinking_levels,
                    "tools": descriptor.capabilities.tools,
                    "history": descriptor.capabilities.history,
                }
            }));
            Ok(())
        }
        "load-session" => {
            let session_id = arguments
                .first()
                .ok_or("load-session requires SESSION_ID")?;
            let session = client
                .load_session(session_id, &cwd.to_string_lossy())
                .await?;
            let descriptor = client.descriptor();
            print_json(&json!({
                "sessionId": session.id(),
                "history": descriptor.capabilities.history,
                "models": descriptor.capabilities.models,
                "thinkingLevels": descriptor.capabilities.thinking_levels,
            }));
            Ok(())
        }
        "resume-session" => {
            let session_id = arguments
                .first()
                .ok_or("resume-session requires SESSION_ID")?;
            let session = client
                .resume_session(session_id, &cwd.to_string_lossy())
                .await?;
            let descriptor = client.descriptor();
            print_json(&json!({
                "sessionId": session.id(),
                "sessionResume": descriptor.capabilities.session_resume,
            }));
            Ok(())
        }
        "state" => {
            let session = client.open_session(&cwd.to_string_lossy()).await?;
            print_json(&client.state(session.id()).await?);
            Ok(())
        }
        "messages" => {
            let session = client.open_session(&cwd.to_string_lossy()).await?;
            let limit = arguments.first().and_then(|value| value.parse().ok());
            let before_id = arguments.get(1).map(String::as_str);
            print_json(&client.messages(session.id(), limit, before_id).await?);
            Ok(())
        }
        "history" => {
            let session = client.open_session(&cwd.to_string_lossy()).await?;
            let limit = arguments.first().and_then(|value| value.parse().ok());
            let after_seq = arguments.get(1).and_then(|value| value.parse().ok());
            let before_seq = arguments.get(2).and_then(|value| value.parse().ok());
            print_json(&client.history(session.id(), limit, after_seq, before_seq).await?);
            Ok(())
        }
        "queue" => {
            let session = client.open_session(&cwd.to_string_lossy()).await?;
            let include_completed = arguments.first().is_some_and(|value| value == "--all");
            print_json(&client.queue(session.id(), include_completed).await?);
            Ok(())
        }
        "models" => {
            let session = client.open_session(&cwd.to_string_lossy()).await?;
            print_json(&client.models(session.id()).await?);
            Ok(())
        }
        "thinking-levels" => {
            let session = client.open_session(&cwd.to_string_lossy()).await?;
            print_json(&client.thinking_levels(session.id()).await?);
            Ok(())
        }
        "tools" => {
            let session = client.open_session(&cwd.to_string_lossy()).await?;
            print_json(&client.tools(session.id()).await?);
            Ok(())
        }
        "commands" => {
            let session = client.open_session(&cwd.to_string_lossy()).await?;
            print_json(&client.commands(session.id()).await?);
            Ok(())
        }
        "auth-status" => {
            let session = client.open_session(&cwd.to_string_lossy()).await?;
            print_json(&client.auth_status(session.id()).await?);
            Ok(())
        }
        "resume-pending" => {
            let session = client.open_session(&cwd.to_string_lossy()).await?;
            print_json(&client.request(session.id(), "resume_pending", json!({})).await?);
            Ok(())
        }
        "compact" => {
            let session = client.open_session(&cwd.to_string_lossy()).await?;
            let instructions = arguments.join(" ");
            print_json(&client.request(
                session.id(),
                "compact",
                if instructions.trim().is_empty() {
                    json!({})
                } else {
                    json!({"customInstructions": instructions})
                },
            ).await?);
            Ok(())
        }
        "set-tools" => {
            let session = client.open_session(&cwd.to_string_lossy()).await?;
            let tools = arguments
                .iter()
                .flat_map(|value| value.split(','))
                .filter(|value| !value.trim().is_empty())
                .map(str::to_owned)
                .collect::<Vec<_>>();
            print_json(&client.set_active_tools(session.id(), tools).await?);
            Ok(())
        }
        "set-config" => {
            let config_id = arguments
                .first()
                .ok_or("set-config requires OPTION VALUE")?;
            let raw_value = arguments.get(1).ok_or("set-config requires OPTION VALUE")?;
            let value = serde_json::from_str(raw_value)
                .unwrap_or_else(|_| serde_json::Value::String(raw_value.clone()));
            let session = client.open_session(&cwd.to_string_lossy()).await?;
            print_json(&session.set_config_option(config_id, value).await?);
            Ok(())
        }
        "prompt" => run_native_prompt(&client, transport, &cwd, arguments).await,
        _ => Err(
            "native transport supports: agent-info, state, messages [LIMIT] [BEFORE_ID], history [LIMIT] [AFTER_SEQ] [BEFORE_SEQ], queue [--all], models, thinking-levels, tools, commands, auth-status, resume-pending, compact [INSTRUCTIONS], set-tools TOOL[,TOOL], load-session SESSION_ID, resume-session SESSION_ID, set-config OPTION VALUE, prompt MESSAGE".into(),
        ),
    };
    let stop_result = client.stop().await;
    result?;
    stop_result?;
    Ok(ExitCode::SUCCESS)
}
async fn run_native_prompt(
    client: &AgentClient,
    transport: &str,
    cwd: &std::path::Path,
    arguments: Vec<String>,
) -> Result<(), Box<dyn Error>> {
    let mut prompt_arguments = arguments;
    let allow_permissions = prompt_arguments.first().map(String::as_str) == Some("--allow");
    if allow_permissions {
        prompt_arguments.remove(0);
    }
    let session = client.open_session(&cwd.to_string_lossy()).await?;
    let mut connection = session.connect().await?;
    let prompt = prompt_arguments.join(" ");
    if prompt.trim().is_empty() {
        return Err("prompt requires message text".into());
    }
    let prompt_future = session.prompt(&prompt);
    tokio::pin!(prompt_future);
    let mut prompt_result = None;
    let mut settled = false;
    while !settled || prompt_result.is_none() {
        tokio::select! {
            result = &mut prompt_future, if prompt_result.is_none() => {
                let failed = result.is_err();
                prompt_result = Some(result);
                if failed {
                    settled = true;
                }
            }
            event = connection.next() => {
                let Some(event) = event? else {
                    return Err("native event stream ended before prompt completion".into());
                };
                println!(
                    "{}",
                    serde_json::to_string(&json!({
                        "seq": event.wire.seq,
                        "id": event.wire.id,
                        "event": event.wire.event_type,
                        "data": event.wire.data,
                        "applied": format!("{:?}", event.applied),
                    }))?
                );
                if transport == "acp" && event.wire.event_type == "ui_dialog_request" {
                    if let Some(request_id) = event.wire.data.get("jsonRpcRequestId") {
                        let dialog = work_client::dialog_from_wire(&event.wire)?
                            .ok_or("ACP permission request omitted metadata")?;
                        let result = dialog.acp_permission_policy(allow_permissions)?;
                        client
                            .respond_to_agent_request(request_id.clone(), result)
                            .await?;
                    }
                }
                settled = event.wire.event_type == "agent_settled";
            }
        }
    }
    prompt_result.expect("prompt future completes before native loop exits")?;
    Ok(())
}

fn print_json<T: serde::Serialize>(value: &T) {
    match serde_json::to_string_pretty(value) {
        Ok(json) => println!("{json}"),
        Err(error) => eprintln!("serialization error: {error}"),
    }
}

fn default_transport() -> &'static str {
    "rpc"
}

fn default_command(transport: &str) -> &'static str {
    if transport == "http" {
        "health"
    } else {
        "agent-info"
    }
}

#[cfg(test)]
mod tests {
    use super::{default_command, default_transport};

    #[test]
    fn native_rpc_is_the_default_transport() {
        assert_eq!(default_transport(), "rpc");
        assert_eq!(default_command("rpc"), "agent-info");
        assert_eq!(default_command("acp"), "agent-info");
        assert_eq!(default_command("http"), "health");
    }
}
