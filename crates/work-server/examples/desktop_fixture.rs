//! Isolated native owner for the browser E2E. Reads test commands on stdin;
//! never opens a user directory or installed rpi. Shutdown is cooperative.
//! Usage: `desktop_fixture` <rpc-fixture> <Web-assets-directory|--web-proxy>.
//! Proxy mode declares hosted metadata without serving assets on this listener.
#[path = "../../../tests/support/legacy_fixture.rs"]
mod legacy_fixture;
use serde_json::{Value, json};
use std::{
    io::{BufRead, Write},
    path::PathBuf,
    sync::Arc,
};
use work_node::workspace::LocalWorkspace;
use work_server::{HttpServer, ServerConfig};
#[allow(clippy::needless_pass_by_value)]
fn emit(value: Value) {
    println!("{value}");
    let _ = std::io::stdout().flush();
}
#[tokio::main(flavor = "current_thread")]
async fn main() {
    tokio::task::LocalSet::new().run_until(async{
        let args:Vec<_>=std::env::args_os().skip(1).collect();
        assert_eq!(args.len(),2,"expected fixture binary and Web assets directory or --web-proxy");
        let web_proxy = args[1] == "--web-proxy";
        let assets = (!web_proxy).then(|| PathBuf::from(&args[1]));
        let root=tempfile::tempdir().unwrap();
        let owner=Arc::new(LocalWorkspace::with_session_dir(PathBuf::from(&args[0]),root.path().join("data"),Some(root.path().join("transcripts"))).unwrap());
        let project=owner.create_project("Desktop owner fixture",None,false).await.unwrap();
        // Seed an explicit legacy fixture, as the protocol-only child does not
        // advertise the native CLI resource capability needed for new sessions.
        let session=legacy_fixture::seed_legacy(&root.path().join("data"),&project.id,"browser-owner-session");
        owner.rename_session(&session,"Shared desktop session").await.unwrap();
        let password=std::env::var("WORK_FIXTURE_PASSWORD").unwrap_or_else(|_| "111111".into());
        let node_id=std::env::var("WORK_FIXTURE_NODE_ID").unwrap_or_else(|_| "browser-native-owner-fixture".into());
        let mut server=HttpServer::start(owner.clone(),ServerConfig{bind:"127.0.0.1:0".parse().unwrap(),password,allowed_hosts:vec![],node_id:node_id.clone(),allow_sharing:true,assets,web_proxy}).unwrap();
        let sharing = server.sharing_control();
        emit(json!({"ready":true,"url":format!("http://{}/",server.address()),"nodeId":node_id,"sessionId":session,"projectId":project.id,"webProxy":web_proxy}));
        let (send,mut commands)=tokio::sync::mpsc::unbounded_channel();
        std::thread::spawn(move||{for line in std::io::stdin().lock().lines().map_while(Result::ok){if send.send(line).is_err(){break;}}});
        while let Some(line)=commands.recv().await{
            let Ok(command)=serde_json::from_str::<Value>(&line)else{continue;};
            let result=match command["op"].as_str().unwrap_or(""){
                "sharing"=>{sharing.set_enabled(command["enabled"].as_bool().unwrap());json!({"enabled":sharing.enabled()})},
                "catalog"=>json!({"sessions":owner.sessions().await.unwrap()}),
                "rename"=>json!(owner.rename_session(&session,command["title"].as_str().unwrap()).await.unwrap()),
                "shutdown"=>{break;},
                _=>json!({"error":"unknown fixture control"}),
            };
            emit(json!({"id":command["id"],"result":result}));
        }
        server.drain().await.unwrap();owner.shutdown().await;drop(server);
    }).await;
}
