use serde_json::json;
use std::{env, error::Error, process::ExitCode};
use work_client::WorkClient;

#[tokio::main]
async fn main() -> Result<ExitCode, Box<dyn Error>> {
    let mut args = env::args().skip(1);
    let base_url = env::var("RPI_WORK_URL").unwrap_or_else(|_| "http://127.0.0.1:3030/".into());
    let client = WorkClient::new(&base_url)?;
    let command = args.next().unwrap_or_else(|| "health".into());

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
                "usage: work-cli [health|capabilities|projects|folders [PATH]|sessions|state SESSION|history SESSION|models SESSION|tools SESSION|watch SESSION]"
            );
            return Ok(ExitCode::from(2));
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn print_json<T: serde::Serialize>(value: &T) {
    match serde_json::to_string_pretty(value) {
        Ok(json) => println!("{json}"),
        Err(error) => eprintln!("serialization error: {error}"),
    }
}
