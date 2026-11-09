//! Remote and native-facing Tool Program queries.
use crate::{ClientError, WorkClient};
use work_core::api::{
    ToolProgramBodyQuery, ToolProgramBodyResponse, ToolProgramChildPage, ToolProgramChildrenQuery,
    ToolProgramPhasePage, ToolProgramPhasesQuery, ToolProgramSnapshot, ToolProgramSnapshotRequest,
};

const DEFAULT_CHILD_LIMIT: u64 = 8;
const DEFAULT_PAGE_BYTES: u64 = 32 * 1024;

fn children_query(query: ToolProgramChildrenQuery) -> ToolProgramChildrenQuery {
    ToolProgramChildrenQuery {
        direction: if query.direction.is_empty() {
            "newestFirst".into()
        } else {
            query.direction
        },
        limit: if query.limit == 0 {
            DEFAULT_CHILD_LIMIT
        } else {
            query.limit
        },
        max_bytes: if query.max_bytes == 0 {
            DEFAULT_PAGE_BYTES
        } else {
            query.max_bytes
        },
        ..query
    }
}

impl WorkClient {
    pub async fn tool_program_children(
        &self,
        session_id: &str,
        query: ToolProgramChildrenQuery,
    ) -> Result<ToolProgramChildPage, ClientError> {
        self.post(
            &format!("api/v1/sessions/{session_id}/tool-program/children/query"),
            serde_json::to_value(children_query(query))?,
        )
        .await
    }

    pub async fn tool_program_phases(
        &self,
        session_id: &str,
        query: ToolProgramPhasesQuery,
    ) -> Result<ToolProgramPhasePage, ClientError> {
        self.post(
            &format!("api/v1/sessions/{session_id}/tool-program/phases/query"),
            serde_json::to_value(ToolProgramPhasesQuery {
                children: children_query(query.children),
                ..query
            })?,
        )
        .await
    }

    pub async fn tool_program_body(
        &self,
        session_id: &str,
        query: ToolProgramBodyQuery,
    ) -> Result<ToolProgramBodyResponse, ClientError> {
        let query = ToolProgramBodyQuery {
            limit: if query.limit == 0 { 8 } else { query.limit },
            max_bytes: if query.max_bytes == 0 {
                64 * 1024
            } else {
                query.max_bytes
            },
            ..query
        };
        self.post(
            &format!("api/v1/sessions/{session_id}/tool-program/body/query"),
            serde_json::to_value(query)?,
        )
        .await
    }

    pub async fn tool_program_snapshot(
        &self,
        session_id: &str,
    ) -> Result<ToolProgramSnapshot, ClientError> {
        let request = ToolProgramSnapshotRequest {
            children: children_query(ToolProgramChildrenQuery {
                direction: "newestFirst".into(),
                limit: DEFAULT_CHILD_LIMIT,
                max_bytes: 128 * 1024,
                ..Default::default()
            }),
            phase_limit: DEFAULT_CHILD_LIMIT,
        };
        self.post(
            &format!("api/v1/sessions/{session_id}/tool-program/snapshot"),
            serde_json::to_value(request)?,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_defaults_match_web_bounded_windows() {
        let query = children_query(ToolProgramChildrenQuery::default());
        assert_eq!(query.direction, "newestFirst");
        assert_eq!(query.limit, DEFAULT_CHILD_LIMIT);
        assert_eq!(query.max_bytes, DEFAULT_PAGE_BYTES);
    }
}
