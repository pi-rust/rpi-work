//! Validated, read-only transcript pagination. Cursors are persisted entry IDs,
//! never the presentation identities synthesized for live messages.

use crate::{ClientError, Snapshot, unsupported};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashSet;

pub const MESSAGE_PAGE_SIZE: u64 = 48;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MessagePageInfo {
    pub first_id: Option<String>,
    pub has_more: bool,
    pub total: u64,
    pub leaf_id: Option<String>,
}

impl MessagePageInfo {
    #[must_use]
    pub fn from_snapshot(snapshot: &Snapshot) -> Self {
        let has_more = snapshot.has_more || snapshot.truncated;
        let first_id = valid_ids(&snapshot.messages)
            .filter(|ids| {
                !ids.is_empty()
                    && snapshot.first_id.as_deref() == ids.first().copied()
                    && snapshot.last_id.as_deref() == ids.last().copied()
                    && snapshot.total >= ids.len() as u64
                    && (!has_more || snapshot.total > ids.len() as u64)
            })
            .and_then(|ids| ids.first().map(|id| (*id).to_owned()));
        Self {
            first_id,
            has_more,
            total: snapshot.total,
            leaf_id: snapshot.leaf_id.clone(),
        }
    }
}

impl From<&Snapshot> for MessagePageInfo {
    fn from(snapshot: &Snapshot) -> Self {
        Self::from_snapshot(snapshot)
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MessagePage {
    pub messages: Vec<Value>,
    pub total: u64,
    pub first_id: Option<String>,
    pub last_id: Option<String>,
    pub before_id: Option<String>,
    pub has_more: bool,
}

impl MessagePage {
    /// Reject legacy hosts that ignore `beforeId`, or return display-only IDs.
    /// Entry IDs are opaque: their order cannot be inferred lexicographically.
    pub fn validate_before(&self, before_id: &str) -> Result<(), ClientError> {
        let invalid = || unsupported("message_pagination", "get_messages cursor metadata");
        if !real_id(before_id) || self.before_id.as_deref() != Some(before_id) {
            return Err(invalid());
        }
        let ids = valid_ids(&self.messages).ok_or_else(invalid)?;
        if ids.contains(&before_id)
            || self.first_id.as_deref() != ids.first().copied()
            || self.last_id.as_deref() != ids.last().copied()
            // The requested cursor is itself a transcript entry outside this page.
            || self.total <= ids.len() as u64
            || (self.has_more && ids.is_empty())
            || (self.has_more && self.total <= ids.len() as u64 + 1)
        {
            return Err(invalid());
        }
        Ok(())
    }
}

fn real_id(id: &str) -> bool {
    !id.trim().is_empty() && !id.starts_with("anonymous-")
}

fn valid_ids(messages: &[Value]) -> Option<Vec<&str>> {
    let mut seen = HashSet::new();
    messages
        .iter()
        .map(|message| {
            let id = message.get("entryId")?.as_str()?;
            (real_id(id) && seen.insert(id)).then_some(id)
        })
        .collect()
}
