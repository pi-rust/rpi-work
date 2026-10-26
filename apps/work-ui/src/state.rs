use dioxus::core::Task;
use dioxus::prelude::*;
use std::collections::HashMap;
use std::future::Future;

#[derive(Clone, Default, PartialEq)]
pub struct Draft {
    pub text: String,
    pub revision: u64,
}

pub type Drafts = HashMap<String, Draft>;

/// Draft keys are UI scope keys (node origin + the raw session id).  The raw
/// id is never sent to the node as part of this key.
pub fn update_draft(mut drafts: Signal<Drafts>, scope: &str, text: String) {
    let mut store = drafts.write();
    let draft = store.entry(scope.to_owned()).or_default();
    draft.text = text;
    draft.revision += 1;
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
