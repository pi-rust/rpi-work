//! Explicit legacy fixture records. Not an application creation API or exact fallback.
use std::path::Path;

pub fn seed_legacy(data: &Path, project: &str, id: &str) -> String {
    let path = data
        .join("rpc/projects")
        .join(project)
        .join("sessions.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut value = if path.exists() {
        serde_json::from_slice::<serde_json::Value>(&std::fs::read(&path).unwrap()).unwrap()
    } else {
        serde_json::json!({"sessions":[]})
    };
    value["sessions"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({"sessionId":id,"projectId":project}));
    std::fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
    id.into()
}
