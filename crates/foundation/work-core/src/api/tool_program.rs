//! Typed Tool Program read contracts shared by native and remote clients.
//!
//! These DTOs intentionally model only native child metadata and phase bodies. They
//! must not be synthesized from ordinary transcript messages or tool output.
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolProgramCursor {
    pub session_id: String,
    pub lane: String,
    pub leaf_id: String,
    pub direction: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child_id: Option<String>,
    pub after_entry_id: String,
    pub after_seq: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolProgramChildSummary {
    pub version: u64,
    pub child_id: String,
    pub seq: u64,
    pub origin: Value,
    pub origin_verification: String,
    pub root_call_id: String,
    pub root_tool_name: String,
    pub tool_name: String,
    pub ordinal: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_child_id: Option<String>,
    pub state: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolProgramBodyDescriptor {
    pub descriptor_id: String,
    pub version: u64,
    pub body_id: String,
    pub child_id: String,
    pub kind: String,
    pub codec: String,
    pub encoding: String,
    pub byte_length: u64,
    pub chunk_bytes: u64,
    pub chunks: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolProgramPhase {
    pub entry_id: String,
    pub seq: u64,
    pub version: u64,
    pub child_id: String,
    pub index: u64,
    pub phase: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub descriptor: Option<ToolProgramBodyDescriptor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cancelled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failed: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interrupted: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolProgramPhasePage {
    pub leaf_id: String,
    pub child_id: String,
    pub phases: Vec<ToolProgramPhase>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<ToolProgramCursor>,
    pub scanned: u64,
    pub scan_exhausted: bool,
    pub state: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolProgramObservation {
    pub child_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_phase: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub final_phase: Option<ToolProgramPhase>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closed_phase: Option<ToolProgramPhase>,
    pub outcome: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolProgramChildPage {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub leaf_id: Option<String>,
    pub children: Vec<ToolProgramChildSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<ToolProgramCursor>,
    pub scan_exhausted: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolProgramSnapshot {
    pub session_id: String,
    pub lane: String,
    pub page: ToolProgramChildPage,
    pub phases: Vec<ToolProgramPhasePage>,
    pub observations: Vec<ToolProgramObservation>,
    pub start_seq: u64,
    pub last_seq: u64,
    pub events: Vec<Value>,
    pub resync_required: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolProgramBodyChunk {
    pub index: u64,
    pub byte_offset: u64,
    pub byte_length: u64,
    pub data: String,
    pub sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolProgramBodyPage {
    pub descriptor: ToolProgramBodyDescriptor,
    pub leaf_id: String,
    pub byte_offset: u64,
    pub byte_length: u64,
    pub total_bytes: u64,
    pub chunks: Vec<ToolProgramBodyChunk>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<ToolProgramBodyPageCursor>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolProgramBodyPageCursor {
    pub session_id: String,
    pub lane: String,
    pub leaf_id: String,
    pub descriptor_id: String,
    pub next_index: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolProgramBodyCursor {
    pub session_id: String,
    pub lane: String,
    pub leaf_id: String,
    pub child_id: String,
    pub phase_entry_id: String,
    pub body: ToolProgramBodyPageCursor,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolProgramBodyResponse {
    pub child_id: String,
    pub phase_entry_id: String,
    pub body: ToolProgramBodyPage,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<ToolProgramBodyCursor>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolProgramChildrenQuery {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub leaf_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<ToolProgramCursor>,
    pub direction: String,
    pub limit: u64,
    pub max_bytes: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolProgramPhasesQuery {
    pub child_id: String,
    #[serde(flatten)]
    pub children: ToolProgramChildrenQuery,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolProgramBodyQuery {
    pub child_id: String,
    pub phase_entry_id: String,
    pub descriptor_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub leaf_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<ToolProgramBodyCursor>,
    pub limit: u64,
    pub max_bytes: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolProgramSnapshotRequest {
    pub children: ToolProgramChildrenQuery,
    pub phase_limit: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolProgramChildEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(flatten)]
    pub data: Value,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolProgramStreamItem {
    #[serde(rename = "type")]
    pub item_type: String,
    pub seq: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event: Option<ToolProgramChildEvent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observation: Option<ToolProgramObservation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disposition: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn snapshot_contract_round_trips_camel_case_fields() {
        let value = json!({
            "sessionId":"s", "lane":"main", "page":{"children":[],"scanExhausted":true},
            "phases":[], "observations":[], "startSeq":1, "lastSeq":2,
            "events":[], "resyncRequired":false
        });
        let snapshot: ToolProgramSnapshot = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(snapshot).unwrap(), value);
    }
}
