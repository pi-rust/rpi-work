//! Loaded pages are presentation state, never runtime events or a replay cursor.
use serde_json::Value;
use std::collections::HashSet;
use work_client::{MessagePage, MessagePageInfo};

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct HistoryWindow {
    pub scope: String,
    pub generation: u64,
    pub revision: u64,
    pub cursor: Option<String>,
    pub has_more: bool,
    pub enabled: bool,
    pub busy: bool,
    pub error: Option<String>,
    pub pages: usize,
    pub leaf_id: Option<String>,
    pub snapshot_ready: bool,
    older: Vec<Value>,
}

impl HistoryWindow {
    pub fn reset(
        &mut self,
        scope: String,
        generation: u64,
        info: Option<&MessagePageInfo>,
        supported: bool,
    ) {
        let revision = self.revision.saturating_add(1);
        *self = Self {
            scope,
            generation,
            revision,
            cursor: info.and_then(|i| i.first_id.clone()),
            has_more: info.is_some_and(|i| i.has_more),
            enabled: supported && info.is_some(),
            leaf_id: info.and_then(|info| info.leaf_id.clone()),
            snapshot_ready: info.is_some(),
            ..Self::default()
        };
    }

    pub fn matches(&self, scope: &str, generation: u64) -> bool {
        self.scope == scope && self.generation == generation
    }

    pub fn claim(&mut self) -> Option<String> {
        if !self.enabled || !self.has_more || self.busy {
            return None;
        }
        let cursor = self.cursor.clone()?;
        self.busy = true;
        self.error = None;
        Some(cursor)
    }

    pub fn fail(&mut self, error: String) {
        self.busy = false;
        self.error = Some(error);
    }

    pub fn apply(
        &mut self,
        cursor: &str,
        page: MessagePage,
        tail: &[Value],
    ) -> Result<Vec<Value>, String> {
        if !self.busy || self.cursor.as_deref() != Some(cursor) {
            return Err("历史请求已失效".into());
        }
        page.validate_before(cursor).map_err(|e| e.to_string())?;
        let existing: HashSet<_> = self.older.iter().chain(tail).filter_map(entry_id).collect();
        if page.has_more
            && page
                .messages
                .iter()
                .all(|m| entry_id(m).is_none_or(|id| existing.contains(id)))
        {
            return Err("节点返回的历史页没有前进，请重新连接会话。".into());
        }
        let mut older = page.messages;
        older.extend(std::mem::take(&mut self.older));
        self.older = deduplicate(older);
        self.cursor = page.first_id;
        self.has_more = page.has_more;
        self.busy = false;
        self.error = None;
        self.pages += 1;
        Ok(self.merge_tail(tail))
    }

    pub fn merge_tail(&self, tail: &[Value]) -> Vec<Value> {
        // Runtime tail wins on matching IDs, preserving corrected live text/results.
        let tail_ids: HashSet<_> = tail.iter().filter_map(entry_id).collect();
        let mut messages: Vec<_> = self
            .older
            .iter()
            .filter(|m| entry_id(m).is_none_or(|id| !tail_ids.contains(id)))
            .cloned()
            .collect();
        messages.extend_from_slice(tail);
        deduplicate(messages)
    }
}

fn entry_id(message: &Value) -> Option<&str> {
    // Compare proven identities, but retain the winning row's physical ID so
    // existing live projections and timeline DOM anchors remain stable.
    work_client::canonical_entry_id(message).or_else(|| message.get("entryId")?.as_str())
}

fn deduplicate(messages: Vec<Value>) -> Vec<Value> {
    let mut seen = HashSet::new();
    messages
        .into_iter()
        .filter(|m| entry_id(m).is_none_or(|id| seen.insert(id.to_owned())))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn message(id: &str, text: &str) -> Value {
        json!({"entryId":id,"role":"user","content":text})
    }
    fn window() -> HistoryWindow {
        let mut w = HistoryWindow::default();
        w.reset(
            "node/session".into(),
            2,
            Some(&MessagePageInfo {
                first_id: Some("c".into()),
                has_more: true,
                total: 10,
                leaf_id: None,
            }),
            true,
        );
        w
    }
    fn page() -> MessagePage {
        serde_json::from_value(json!({"messages":[message("a","old"),message("b","old")],"firstId":"a","lastId":"b","beforeId":"c","hasMore":false,"total":10})).unwrap()
    }
    #[test]
    fn paging_canonical_alias_keeps_one_owner_and_the_updated_tail() {
        let w = HistoryWindow {
            older: vec![message("formal", "persisted")],
            ..HistoryWindow::default()
        };
        let live = json!({"entryId":"anonymous-1-1","role":"user","content":"newer", "__rpiClientAnonymousId":true,"__rpiClientCanonicalId":"formal"});
        let merged = w.merge_tail(&[live.clone()]);
        assert_eq!(
            merged,
            vec![live],
            "pagination must recognize a proven canonical alias without renaming its live DOM owner"
        );
    }

    #[test]
    fn paging_preserves_distinct_ids_and_unproven_aliases_even_with_identical_text() {
        let w = HistoryWindow {
            older: vec![message("formal", "same")],
            ..HistoryWindow::default()
        };
        for tail in [
            message("different", "same"),
            json!({"entryId":"anonymous-1-1","role":"user","content":"same"}),
            json!({"entryId":"different","role":"user","content":"same","__rpiClientCanonicalId":"formal"}),
            json!({"entryId":"anonymous-1-1","role":"user","content":"same","__rpiClientAnonymousId":true}),
        ] {
            assert_eq!(w.merge_tail(&[tail]).len(), 2);
        }
    }

    #[test]
    fn paging_page_overlap_uses_proven_alias_as_identity() {
        let mut w = window();
        w.older = vec![
            json!({"entryId":"anonymous-1-1","role":"user","content":"live", "__rpiClientAnonymousId":true,"__rpiClientCanonicalId":"a"}),
        ];
        let cursor = w.claim().unwrap();
        let merged = w.apply(&cursor, page(), &[message("c", "tail")]).unwrap();
        assert_eq!(
            merged.len(),
            3,
            "overlapping page entries share their proven identity"
        );
    }

    #[test]
    fn claims_once_and_retry_keeps_cursor() {
        let mut w = window();
        assert_eq!(w.claim().as_deref(), Some("c"));
        assert_eq!(w.claim(), None);
        w.fail("offline".into());
        assert_eq!(w.claim().as_deref(), Some("c"));
        assert!(w.error.is_none());
    }
    #[test]
    fn merged_tail_wins_and_realtime_updates_keep_older_pages() {
        let mut w = window();
        w.claim();
        let first = w.apply("c", page(), &[message("c", "tail")]).unwrap();
        assert_eq!(first.len(), 3);
        let merged = w.merge_tail(&[
            message("b", "corrected"),
            message("c", "tail"),
            message("d", "new"),
        ]);
        assert_eq!(merged.len(), 4);
        assert_eq!(merged[1]["content"], "corrected");
        assert!(!w.has_more);
    }
    #[test]
    fn reset_isolates_nodes_generations_and_disables_missing_capability() {
        let mut w = window();
        w.claim();
        w.apply("c", page(), &[]).unwrap();
        w.reset("other/session".into(), 3, None, false);
        assert!(!w.matches("node/session", 2));
        assert!(w.matches("other/session", 3));
        assert_eq!(w.merge_tail(&[]), Vec::<serde_json::Value>::new());
        assert_eq!(w.claim(), None);
    }
    #[test]
    fn invalid_or_nonadvancing_pages_do_not_change_cursor_or_older_data() {
        let mut w = window();
        w.claim();
        let mut p = page();
        p.has_more = true;
        assert!(
            w.apply("c", p, &[message("a", "fresh"), message("b", "fresh")])
                .is_err()
        );
        assert_eq!(w.cursor.as_deref(), Some("c"));
        assert_eq!(w.pages, 0);
        w.fail("bad page".into());
        assert_eq!(w.claim().as_deref(), Some("c"));
    }
}
