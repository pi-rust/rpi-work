mod args;
mod compat;
mod password;

use args::{Action, Cli, Transport};
use serde::Serialize;
use serde_json::{Value, json};
use std::{env, future::Future, io::Write, process::ExitCode, time::Duration};
use work_client::{AuthState, ClientError, WorkClient};

pub(crate) const DEADLINE: Duration = Duration::from_secs(15);

#[tokio::main]
async fn main() -> ExitCode {
    // Parsing and command validation always precede stdin, client construction,
    // network traffic, or explicit compatibility-mode process creation.
    let arguments: Result<Vec<String>, _> = env::args_os()
        .skip(1)
        .map(|arg| {
            arg.into_string()
                .map_err(|_| "arguments must be UTF-8 (input omitted)".to_string())
        })
        .collect();
    let cli = match arguments.and_then(|arguments| {
        args::parse_environment(
            arguments,
            env::var("RPI_WORK_TRANSPORT"),
            env::var("RPI_WORK_URL"),
        )
    }) {
        Ok(cli) => cli,
        Err(error) => {
            work_host::report_error(&error);
            return ExitCode::from(2);
        }
    };
    match Box::pin(run(cli)).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            work_host::report_error(&error);
            ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli) -> Result<(), String> {
    match cli.action {
        Action::Help => {
            println!("{}", args::HELP);
            Ok(())
        }
        Action::Serve(shutdown_stdin) => work_host::run_headless(shutdown_stdin).await,
        Action::Command { name, args } if cli.transport != Transport::Http => {
            // Compatibility mode is explicitly opt-in. Do not echo native errors:
            // an agent can include private content in its error text.
            compat::run_native_agent(cli.transport.name(), name, args)
                .await
                .map(|_| ())
                .map_err(|_| "native compatibility operation failed (details omitted)".into())
        }
        Action::Command { name, args } => {
            let client = WorkClient::new(&cli.endpoint)
                .map_err(|_| "could not construct HTTP client".to_string())?;
            let password = if cli.password_stdin {
                Some(password::stdin_password().await?)
            } else {
                match env::var("RPI_WORK_PASSWORD") {
                    Ok(value) => Some(password::Password::from_env(value)?),
                    Err(env::VarError::NotPresent) => None,
                    Err(_) => return Err("RPI_WORK_PASSWORD must be UTF-8 (input omitted)".into()),
                }
            };
            let state = authenticate(&client, password.as_ref()).await;
            drop(password); // No saved draft, credential file, or persistent jar.
            let state = state?;
            if name == "auth" {
                return output(&state);
            }
            if name == "logout" {
                let state = operation(client.logout(), true).await?;
                return output(&state);
            }
            // required=false alone is NOT an authorization grant. Standalone
            // nodes legitimately report {required:false, authenticated:true}.
            require_authenticated(&state)?;
            if name == "watch" {
                return watch(&client, &args[0]).await;
            }
            Box::pin(execute(&client, &name, &args)).await
        }
    }
}

async fn authenticate(
    client: &WorkClient,
    password: Option<&password::Password>,
) -> Result<AuthState, String> {
    // login itself performs a confirming auth read; one deadline covers both.
    let result = tokio::time::timeout(DEADLINE, async {
        if let Some(password) = password {
            client.login(password.as_str()).await
        } else {
            client.auth().await
        }
    })
    .await;
    match result {
        Ok(Ok(state)) => Ok(state),
        Ok(Err(_)) => {
            Err("authentication failed; verify node and password (details omitted)".into())
        }
        Err(_) => Err("authentication deadline exceeded (15s); no command was sent".into()),
    }
}

fn require_authenticated(state: &AuthState) -> Result<(), String> {
    if state.authenticated {
        return Ok(());
    }
    if state.required {
        Err("node requires a password: use RPI_WORK_PASSWORD or --password-stdin (no login is saved across commands)".into())
    } else {
        Err("node has not authorized this client (required=false is not authorization); provide a password or check node sharing policy".into())
    }
}

async fn operation<T>(
    future: impl Future<Output = Result<T, ClientError>>,
    write: bool,
) -> Result<T, String> {
    match tokio::time::timeout(DEADLINE, future).await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(error)) => Err(safe_error(&error, write)),
        Err(_) => Err(timeout_error(write)),
    }
}
fn timeout_error(write: bool) -> String {
    if write {
        "write deadline exceeded (15s): result UNKNOWN; inspect state before retrying; command was not resent".into()
    } else {
        "operation deadline exceeded (15s)".into()
    }
}
fn safe_error(error: &ClientError, write: bool) -> String {
    // Never format server bodies, URLs, JSON decoder errors or source chains:
    // they may echo credentials, private prompts or dialog values.
    match error {
        ClientError::Api { status, .. } if status.as_u16() == 401 || status.as_u16() == 403 =>
            "node denied access; provide a password or check node sharing policy".into(),
        ClientError::Api { status, .. } => format!("node returned HTTP {} (details omitted){}", status.as_u16(), if write { "; write result may be UNKNOWN; inspect state, do not blindly resend" } else { "" }),
        ClientError::Unsupported { .. } => "operation unsupported by this node (details omitted)".into(),
        _ if write => "write failed: result UNKNOWN; inspect state before retrying; command was not resent (details omitted)".into(),
        _ => "node operation failed (details omitted)".into(),
    }
}

async fn execute(client: &WorkClient, name: &str, args: &[String]) -> Result<(), String> {
    match name {
        "health" => output(&operation(client.health(), false).await?),
        "capabilities" => output(&operation(client.capabilities(), false).await?),
        "projects" => output(&operation(client.projects(), false).await?),
        "folders" => output(
            &operation(
                client.project_folders(args.first().map_or("", String::as_str)),
                false,
            )
            .await?,
        ),
        "sessions" => output(&operation(client.sessions(), false).await?),
        "state" => output(&Box::pin(operation(client.runtime_state(&args[0]), false)).await?),
        "history" => {
            output(&Box::pin(operation(client.history(&args[0], None, None, 100), false)).await?)
        }
        "models" => output(&Box::pin(operation(client.models(&args[0]), false)).await?),
        "tools" => output(&Box::pin(operation(client.tools(&args[0]), false)).await?),
        "project-create" => output(
            &operation(
                client.create_project(&args[0], args.get(1).map(String::as_str), false),
                true,
            )
            .await?,
        ),
        "session-create" => {
            let id = operation(
                client.create_session(args.first().map(String::as_str)),
                true,
            )
            .await?;
            output(&json!({"sessionId": id}))
        }
        "session-rename" => {
            output(&operation(client.rename_session(&args[0], &args[1]), true).await?)
        }
        "prompt" => {
            let ack = Box::pin(operation(client.prompt(&args[0], &args[1]), true)).await?;
            output(&ack)?;
            if ack.success {
                Ok(())
            } else {
                Err(
                    "node rejected prompt (details omitted); ACK above is not a settled event"
                        .into(),
                )
            }
        }
        "abort" => {
            let ack = Box::pin(operation(client.abort(&args[0]), true)).await?;
            output(&ack)?;
            if ack.success {
                Ok(())
            } else {
                Err("node rejected abort (details omitted)".into())
            }
        }
        "dialog-reply" => {
            // Parsed before auth/client creation; never fall back to raw requests.
            let value: Value = serde_json::from_str(&args[2])
                .map_err(|_| "invalid dialog JSON (input omitted)".to_string())?;
            output(
                &Box::pin(operation(
                    client.dialog_response(&args[0], &args[1], value),
                    true,
                ))
                .await?,
            )
        }
        _ => Err("unsupported command".into()),
    }
}

async fn watch(client: &WorkClient, session: &str) -> Result<(), String> {
    // A single pinned Ctrl+C future covers stream setup and every subsequent
    // read. Dropping only detaches the HTTP stream: no abort RPC is ever sent.
    let interrupt = tokio::signal::ctrl_c();
    tokio::pin!(interrupt);
    let mut connection = tokio::select! {
        result = &mut interrupt => return result.map_err(|_| "could not listen for Ctrl+C".into()),
        result = client.connect_session(session) => result.map_err(|e| safe_error(&e, false))?,
    };
    loop {
        tokio::select! {
            result = &mut interrupt => return result.map_err(|_| "could not listen for Ctrl+C".into()),
            event = connection.next() => {
                let Some(event) = event.map_err(|e| safe_error(&e, false))? else { return Ok(()); };
                output(&json!({
                    "seq": event.wire.seq, "id": event.wire.id,
                    "event": event.wire.event_type, "data": event.wire.data,
                    "applied": format!("{:?}", event.applied),
                }))?;
            }
        }
    }
}

fn output<T: Serialize>(value: &T) -> Result<(), String> {
    let mut out = std::io::stdout().lock();
    serde_json::to_writer(&mut out, value)
        .map_err(|_| "could not write JSON output".to_string())?;
    writeln!(out)
        .and_then(|()| out.flush())
        .map_err(|_| "could not write JSON output".into())
}
// Retained solely for legacy compatibility output.
fn print_json<T: Serialize>(value: &T) {
    if let Err(error) = output(value) {
        work_host::report_error(&error);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn auth_policy_requires_authenticated_not_just_optional_password() {
        assert!(
            require_authenticated(&AuthState {
                required: false,
                authenticated: true
            })
            .is_ok()
        );
        assert!(
            require_authenticated(&AuthState {
                required: false,
                authenticated: false
            })
            .is_err()
        );
        let error = require_authenticated(&AuthState {
            required: true,
            authenticated: false,
        })
        .unwrap_err();
        assert!(error.contains("password"));
    }
    #[test]
    fn errors_omit_secrets_and_writes_warn_unknown() {
        for error in [
            ClientError::Sse("secret-password".into()),
            ClientError::Process("secret-password".into()),
        ] {
            assert!(!safe_error(&error, false).contains("secret-password"));
            assert!(safe_error(&error, true).contains("UNKNOWN"));
        }
        assert!(timeout_error(true).contains("not resent"));
    }
}
