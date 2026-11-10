#![cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
//! A freshly compiled std-only echo process, never a real agent/model runtime.
use futures_util::StreamExt;
use serde_json::{Value, json};
use work_node_runtime::workspace::LocalWorkspace;

fn profile() -> Value {
    json!({"contentBlocksVersion":1,"image":true,"imageMimeTypes":["image/png","image/jpeg"],
        "maxImages":4,"maxImageBytes":1_048_576,"maxTotalImageBytes":2_097_152,
        "maxImageBase64Bytes":1_398_104,"maxPromptBytes":3_145_728,"maxTextBytes":32768,
        "maxContentBlocks":64,"maxImageSide":2000,"maxImagePixels":4_000_000,
        "imageDecodeAllocBudgetBytes":67_108_864,"frameBytes":4_194_304})
}

#[tokio::test]
async fn native_owner_forwards_blocks_exactly_and_rejects_before_host_write() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("echo.rs");
    let executable = root
        .path()
        .join(if cfg!(windows) { "echo.exe" } else { "echo" });
    // Echo the complete raw command as response data. No JSON dependencies or
    // external network/environment state in the child, and args are ignored.
    let ready = json!({"type":"ready","protocol":"rpi","supportedCommands":["prompt","abort"],"promptContent":profile()});
    let code = r#"
use std::io::{self, BufRead, Write};
fn main() {
    println!("{}", READY);
    io::stdout().flush().unwrap();
    for line in io::stdin().lock().lines() {
        let line = line.unwrap();
        let Some(tail) = line.split("\"id\":\"").nth(1) else { continue };
        let id = tail.split('"').next().unwrap();
        let data = format!("{},\"accepted\":true}}", &line[..line.len()-1]);
        println!("{{\"type\":\"response\",\"id\":\"{}\",\"command\":\"prompt\",\"success\":true,\"data\":{}}}", id, data);
        println!("{{\"type\":\"agent_settled\",\"outcome\":\"completed\"}}");
        io::stdout().flush().unwrap();
    }
}
"#.replace("READY", &serde_json::to_string(&ready.to_string()).unwrap());
    std::fs::write(&source, code).unwrap();
    assert!(
        std::process::Command::new("rustc")
            .arg("--edition=2024")
            .arg(&source)
            .arg("-o")
            .arg(&executable)
            .status()
            .unwrap()
            .success()
    );
    let owner = LocalWorkspace::with_session_dir(
        executable,
        root.path().join("data"),
        Some(root.path().join("sessions")),
    )
    .unwrap();
    // Seed an isolated legacy registry rather than the unrelated shipped-resource
    // installer. Opening this known session launches only our echo executable.
    let project =
        rpi_extcore::workspace::Project::new(&root.path().join("data/rpc/control")).unwrap();
    let registry_path = root
        .path()
        .join("data/rpc/projects")
        .join(&project.id)
        .join("sessions.json");
    std::fs::create_dir_all(registry_path.parent().unwrap()).unwrap();
    let mut registry = rpi_extcore::registry::Registry::load(registry_path).unwrap();
    let id = "prompt-test".to_owned();
    registry
        .upsert(rpi_extcore::registry::RegistryEntry {
            session_id: id.clone(),
            project_id: Some(project.id),
            title: None,
            parent_session_id: None,
            fork_entry_id: None,
            relation: None,
            created_at: 0,
            updated_at: 0,
        })
        .unwrap();
    let access = owner.session_access(&id).await.unwrap();
    assert_eq!(access.prompt_content, Some(profile()));
    assert!(access.supports_prompt_content());
    let blocks = json!([{"type":"text","text":"caption"},{"type":"image","mimeType":"image/jpeg","data":"/9j/"},{"type":"text","text":"full text attachment"}]);
    let mut events = owner.subscribe(&id, 0).await.unwrap();
    let response = owner.prompt_content(&id, &blocks).await.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        while let Some(event) = events.next().await {
            if event.unwrap().event_type == "agent_settled" {
                return;
            }
        }
        panic!("stub settlement stream ended");
    })
    .await
    .expect("stub must settle before a second prompt");
    let sent = response.data.unwrap();
    assert_eq!(sent["contentBlocks"], blocks);
    assert!(sent.get("message").is_none());
    assert_eq!(sent["id"].as_str().unwrap().len(), 36);
    // Direct raw commands cannot bypass the owner's validator.
    for payload in [
        json!({"type":"prompt","message":"bad","contentBlocks":blocks}),
        json!({"type":"prompt","unknown":true,"contentBlocks":blocks}),
        json!({"type":"prompt","contentBlocks":[{"type":"image","mimeType":"image/jpeg","data":"not-base64"}]}),
    ] {
        assert!(owner.command::<Value>(&id, payload).await.is_err());
    }
    let legacy = owner
        .prompt(&id, "legacy unchanged")
        .await
        .unwrap()
        .data
        .unwrap();
    assert_eq!(legacy["message"], "legacy unchanged");
    assert!(legacy.get("contentBlocks").is_none());
    owner.shutdown().await;
}
