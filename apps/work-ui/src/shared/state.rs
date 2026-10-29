use dioxus::core::Task;
use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
#[cfg(target_arch = "wasm32")]
use serde_json::Value;
use std::collections::HashMap;
use std::future::Future;
#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;

#[derive(Clone, Default, Deserialize, PartialEq, Serialize)]
pub struct Draft {
    pub text: String,
    pub revision: u64,
}

pub type Drafts = HashMap<String, Draft>;

#[cfg(target_arch = "wasm32")]
const DRAFT_STORAGE_KEY: &str = "rpi-work-drafts-v1";

/// Load only the local text drafts. Keys already contain the normalized node
/// origin and session id, so a draft cannot bleed between nodes or sessions.
pub async fn load_persisted_drafts() -> Result<Drafts, String> {
    #[cfg(target_arch = "wasm32")]
    {
        let value = document::eval(&format!(
            "try{{dioxus.send(JSON.parse(localStorage.getItem('{DRAFT_STORAGE_KEY}')||'{{}}'));}}catch(error){{dioxus.send({{error:String(error)}});}}"
        ))
        .recv::<Value>()
        .await
        .map_err(|error| format!("读取本地草稿失败：{error}"))?;
        if let Some(error) = value.get("error").and_then(Value::as_str) {
            return Err(format!("读取本地草稿失败：{error}"));
        }
        serde_json::from_value(value).map_err(|error| format!("本地草稿格式无效：{error}"))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::future::ready(()).await;
        let Some(path) = draft_path()? else {
            return Err("没有可用的本地草稿目录。".into());
        };
        match std::fs::read_to_string(path) {
            Ok(value) => {
                serde_json::from_str(&value).map_err(|error| format!("本地草稿格式无效：{error}"))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Drafts::new()),
            Err(error) => Err(format!("读取本地草稿失败：{error}")),
        }
    }
}

/// Persist drafts atomically where the host permits it. An error is returned
/// to the UI instead of silently treating an in-memory draft as durable.
pub async fn persist_drafts(drafts: &Drafts) -> Result<(), String> {
    #[cfg(target_arch = "wasm32")]
    {
        let payload =
            serde_json::to_string(drafts).map_err(|error| format!("编码本地草稿失败：{error}"))?;
        let literal = serde_json::to_string(&payload)
            .map_err(|error| format!("编码本地草稿失败：{error}"))?;
        let saved = document::eval(&format!(
            "try{{localStorage.setItem('{DRAFT_STORAGE_KEY}',{literal});dioxus.send(true);}}catch(error){{dioxus.send(false);}}"
        ))
        .recv::<bool>()
        .await
        .map_err(|error| format!("保存本地草稿失败：{error}"))?;
        if saved {
            Ok(())
        } else {
            Err("浏览器拒绝保存本地草稿。".into())
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::future::ready(()).await;
        let Some(path) = draft_path()? else {
            return Err("没有可用的本地草稿目录。".into());
        };
        let parent = path
            .parent()
            .ok_or_else(|| "本地草稿目录无效。".to_owned())?;
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("创建本地草稿目录失败：{error}"))?;
        let temporary = path.with_extension("json.tmp");
        let payload =
            serde_json::to_string(drafts).map_err(|error| format!("编码本地草稿失败：{error}"))?;
        std::fs::write(&temporary, payload)
            .map_err(|error| format!("写入本地草稿失败：{error}"))?;
        std::fs::rename(&temporary, &path).map_err(|error| format!("保存本地草稿失败：{error}"))
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[allow(clippy::unnecessary_wraps)] // Android JNI failures must reach the visible persistence error.
fn draft_path() -> Result<Option<PathBuf>, String> {
    #[cfg(target_os = "android")]
    {
        #[allow(unsafe_code)]
        fn android_draft_directory() -> Result<PathBuf, String> {
            let context = ndk_context::android_context();
            // ndk-context owns these pointers for the native activity lifetime.
            let vm = unsafe { jni::JavaVM::from_raw(context.vm().cast()) }
                .map_err(|error| format!("读取 Android 应用目录失败：{error}"))?;
            let mut env = vm
                .attach_current_thread()
                .map_err(|error| format!("读取 Android 应用目录失败：{error}"))?;
            let activity = unsafe { jni::objects::JObject::from_raw(context.context().cast()) };
            let value = env
                .call_method(&activity, "draftsDirectory", "()Ljava/lang/String;", &[])
                .and_then(|value| value.l())
                .map_err(|error| format!("读取 Android 应用目录失败：{error}"))?;
            let value = jni::objects::JString::from(value);
            let path = env
                .get_string(&value)
                .map_err(|error| format!("读取 Android 应用目录失败：{error}"))?
                .to_string_lossy()
                .into_owned();
            Ok(PathBuf::from(path).join("drafts.json"))
        }
        return android_draft_directory().map(Some);
    }

    #[cfg(not(target_os = "android"))]
    {
        #[cfg(target_os = "windows")]
        let root = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
        #[cfg(target_os = "macos")]
        let root = std::env::var_os("HOME")
            .map(|path| PathBuf::from(path).join("Library/Application Support"));
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        let root = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|path| PathBuf::from(path).join(".config")));
        Ok(root.map(|path| path.join("rpi-work").join("drafts.json")))
    }
}

pub async fn draft_persist_delay() {
    #[cfg(target_arch = "wasm32")]
    gloo_timers::future::TimeoutFuture::new(180).await;
    #[cfg(not(target_arch = "wasm32"))]
    tokio::time::sleep(std::time::Duration::from_millis(180)).await;
}

#[cfg(target_arch = "wasm32")]
fn sync_drafts_for_unload(drafts: &Drafts) {
    let Ok(payload) = serde_json::to_string(drafts) else {
        return;
    };
    let Ok(literal) = serde_json::to_string(&payload) else {
        return;
    };
    let _ = document::eval(&format!(
        "try{{window.__rpiLatestDrafts={literal};const save=()=>{{try{{if(window.__rpiLatestDrafts!==undefined) localStorage.setItem('{DRAFT_STORAGE_KEY}',window.__rpiLatestDrafts)}}catch(_){{}}}};if(!window.__rpiDraftFlushInstalled){{window.addEventListener('pagehide',save);window.addEventListener('beforeunload',save);window.__rpiDraftFlushInstalled=true}};save()}}catch(_){{}}"
    ));
}

/// Update the in-memory draft and synchronously stage the latest web snapshot
/// for pagehide/beforeunload. The debounced durable write remains responsible
/// for normal operation, while the unload path closes the short-loss window.
pub fn update_draft(mut drafts: Signal<Drafts>, scope: &str, text: String) {
    let mut store = drafts.write();
    let draft = store.entry(scope.to_owned()).or_default();
    draft.text = text;
    draft.revision += 1;
    #[cfg(target_arch = "wasm32")]
    let snapshot = store.clone();
    #[cfg(target_arch = "wasm32")]
    sync_drafts_for_unload(&snapshot);
}

pub fn draft_for(drafts: &Drafts, scope: &str) -> Draft {
    drafts.get(scope).cloned().unwrap_or_default()
}

pub fn cancel_requests(mut tasks: Signal<Vec<Task>>) {
    for task in tasks.write().drain(..) {
        task.cancel();
    }
}

pub fn scoped_spawn(
    mut tasks: Signal<Vec<Task>>,
    future: impl Future<Output = ()> + 'static,
) -> Task {
    let task_id = std::rc::Rc::new(std::cell::Cell::new(None::<Task>));
    let completed_id = task_id.clone();
    let task = spawn(async move {
        future.await;
        if let Some(id) = completed_id.get() {
            tasks.write().retain(|task| *task != id);
        }
    });
    task_id.set(Some(task));
    tasks.write().push(task);
    task
}

pub fn directory_key(path: &str) -> Option<(String, String)> {
    let path = path.trim().replace('\\', "/");
    if path.chars().any(char::is_control) {
        return None;
    }
    let windows = path.as_bytes().get(1) == Some(&b':') || path.starts_with("//");
    if !(path.starts_with('/') || (windows && path.as_bytes().get(2) == Some(&b'/'))) {
        return None;
    }
    let mut parts = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            _ => parts.push(part),
        }
    }
    let name = parts.last()?.to_string();
    if name.ends_with(':') {
        return None;
    }
    let normalized = format!(
        "{}{}",
        if path.starts_with("//") {
            "//"
        } else if path.starts_with('/') {
            "/"
        } else {
            ""
        },
        parts.join("/")
    );
    Some((
        if windows {
            normalized.to_lowercase()
        } else {
            normalized
        },
        name,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn directory_identity_respects_platform() {
        assert_eq!(
            directory_key("D:\\Projects\\demo\\").unwrap().0,
            directory_key("d:/projects/./demo").unwrap().0
        );
        assert_ne!(
            directory_key("/Projects/demo").unwrap().0,
            directory_key("/projects/demo").unwrap().0
        );
        assert!(directory_key("../demo").is_none());
    }
}
