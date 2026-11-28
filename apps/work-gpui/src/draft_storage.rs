//! Device-local recovery for committed composer text.

use std::{
    collections::HashMap,
    fs, io,
    path::{Path, PathBuf},
};

use serde_json::json;

use super::{DraftKey, Shell, message_queue, session_storage};
use crate::prompt::Draft;

pub(super) struct Storage {
    directory: PathBuf,
    entries: HashMap<DraftKey, Checkpoint>,
}

struct Checkpoint {
    path: PathBuf,
    saved: (Option<u64>, bool),
    failed: bool,
}

impl Storage {
    pub(super) fn new() -> Option<Self> {
        Some(Self {
            directory: session_storage::directory("gpui-drafts")?,
            entries: HashMap::new(),
        })
    }

    pub(super) fn save_failed(&self, key: DraftKey) -> bool {
        self.entries.get(&key).is_some_and(|entry| entry.failed)
    }

    pub(super) fn persist(
        &mut self,
        key: DraftKey,
        drafts: &HashMap<DraftKey, Draft>,
        queue: &message_queue::Panel,
    ) -> bool {
        self.entries
            .get_mut(&key)
            .is_none_or(|entry| entry.save(drafts.get(&key), enqueue_unconfirmed(queue, key)))
    }
}

fn read(path: &Path) -> io::Result<Option<(String, bool)>> {
    let Some(value) = session_storage::read(path)? else {
        return Ok(None);
    };
    let (Some(text), Some(unconfirmed)) = (value["text"].as_str(), value["unconfirmed"].as_bool())
    else {
        return Err(io::Error::other("invalid draft document"));
    };
    Ok(Some((text.to_owned(), unconfirmed)))
}

fn write(path: &Path, text: &str, unconfirmed: bool) -> io::Result<()> {
    if text.is_empty() {
        return match fs::remove_file(path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            result => result,
        };
    }
    session_storage::write(path, &json!({ "text": text, "unconfirmed": unconfirmed }))
}

impl Shell {
    pub(super) fn restore_draft(&mut self, key: DraftKey) {
        let Some(storage) = self.draft_storage.as_mut() else {
            return;
        };
        if storage.entries.contains_key(&key) {
            return;
        }
        let Some(node) = session_storage::node_scope(&self.snapshot) else {
            return;
        };
        let (project, session) = match key {
            DraftKey::Session(index) => {
                let session = &self.snapshot.sessions[index];
                (session.project_id.as_str(), Some(session.id.as_str()))
            }
            DraftKey::NewConversation(Some(index)) => {
                (self.snapshot.projects[index].id.as_str(), None)
            }
            DraftKey::NewConversation(None) => return,
        };
        let path = session_storage::file(&storage.directory, &node, project, session);
        let mut saved = (None, false);
        if !self.drafts.contains_key(&key) {
            match read(&path) {
                Ok(Some((text, unconfirmed))) => {
                    let draft = self.drafts.entry(key).or_default();
                    draft.input.replace(None, &text);
                    draft.confirm_restored_delivery = unconfirmed;
                    saved = (Some(draft.input.revision), unconfirmed);
                }
                Ok(None) => {}
                Err(error) => eprintln!("Could not restore composer draft: {error}"),
            }
        }
        storage.entries.insert(
            key,
            Checkpoint {
                path,
                saved,
                failed: false,
            },
        );
    }

    pub(super) fn persist_drafts(&mut self) {
        if let Some(storage) = &mut self.draft_storage {
            for (&key, entry) in &mut storage.entries {
                entry.save(self.drafts.get(&key), enqueue_unconfirmed(&self.queue, key));
            }
        }
    }

    pub(super) fn move_draft(&mut self, from: DraftKey, to: DraftKey) {
        if let Some(mut draft) = self.drafts.remove(&from) {
            draft.input.finish_composition();
            self.drafts.insert(to, draft);
            self.restore_draft(to);
            if let Some(storage) = &mut self.draft_storage {
                if storage.persist(to, &self.drafts, &self.queue) {
                    storage.persist(from, &self.drafts, &self.queue);
                } else {
                    // Keep the old checkpoint if its replacement could not be written.
                    storage.entries.remove(&from);
                }
            }
        }
    }
}

fn enqueue_unconfirmed(queue: &message_queue::Panel, key: DraftKey) -> bool {
    match key {
        DraftKey::Session(index) => queue.enqueue_unconfirmed(index),
        _ => false,
    }
}

impl Checkpoint {
    fn save(&mut self, draft: Option<&Draft>, queued: bool) -> bool {
        if draft.is_some_and(|draft| !draft.input.is_committed()) {
            return true;
        }
        let (text, next) = draft.map_or(("", (None, false)), |draft| {
            let text = draft.input.text.as_str();
            let unconfirmed =
                draft.confirm_restored_delivery || draft.delivery_unconfirmed() || queued;
            let next = if text.is_empty() {
                (None, false)
            } else {
                (Some(draft.input.revision), unconfirmed)
            };
            (text, next)
        });
        if self.saved == next {
            self.failed = false;
            return true;
        }
        match write(&self.path, text, next.1) {
            Ok(()) => {
                self.saved = next;
                self.failed = false;
                true
            }
            Err(error) => {
                if !self.failed {
                    eprintln!("Could not save composer draft: {error}");
                }
                self.failed = true;
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_by_node_project_and_session_and_removes_cleared_text() {
        let dir = tempfile::tempdir().unwrap();
        let path = session_storage::file(dir.path(), "node:alpha", "project", Some("session"));
        let other_node = session_storage::file(dir.path(), "node:beta", "project", Some("session"));
        let other_project =
            session_storage::file(dir.path(), "node:alpha", "other", Some("session"));
        let welcome = session_storage::file(dir.path(), "node:alpha", "project", None);
        assert_eq!(read(&path).unwrap(), None);
        write(&path, "中文草稿\nsecond line", true).unwrap();
        write(&welcome, "新会话", false).unwrap();
        assert_eq!(
            read(&path).unwrap(),
            Some(("中文草稿\nsecond line".into(), true))
        );
        assert_eq!(read(&other_node).unwrap(), None);
        assert_eq!(read(&other_project).unwrap(), None);
        write(&path, "", false).unwrap();
        assert_eq!(read(&path).unwrap(), None);
        assert_eq!(read(&welcome).unwrap(), Some(("新会话".into(), false)));
    }
}
