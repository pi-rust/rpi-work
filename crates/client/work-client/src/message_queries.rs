//! Bounded native timeline queries. Entry IDs and page order belong to the host.

use crate::{ClientError, WorkClient, unsupported};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;

/// User-message metadata query. Pin subsequent pages to the returned `leaf_id`.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct MessageSummaryQuery {
    pub limit: u64,
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub leaf_id: Option<String>,
}

impl Default for MessageSummaryQuery {
    fn default() -> Self {
        Self {
            limit: 10,
            role: "user".into(),
            before_id: None,
            leaf_id: None,
        }
    }
}

/// A bounded window with exactly one of anchor, before, or after set.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct MessageWindowQuery {
    pub limit: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anchor_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub leaf_id: Option<String>,
}

impl Default for MessageWindowQuery {
    fn default() -> Self {
        Self {
            limit: 48,
            anchor_id: None,
            before_id: None,
            after_id: None,
            leaf_id: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MessageSummary {
    pub entry_id: String,
    pub role: String,
    pub preview: String,
    pub timestamp: i64,
    pub has_images: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MessageSummaryPage {
    pub filtered: bool,
    pub role: String,
    pub messages: Vec<MessageSummary>,
    pub first_id: Option<String>,
    pub last_id: Option<String>,
    pub next_before_id: Option<String>,
    pub has_more: bool,
    pub leaf_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MessageWindowPage {
    pub messages: Vec<Value>,
    pub first_id: Option<String>,
    pub last_id: Option<String>,
    pub next_before_id: Option<String>,
    pub next_after_id: Option<String>,
    pub has_more_before: bool,
    pub has_more_after: bool,
    pub leaf_id: Option<String>,
}

fn valid_id(id: &str) -> bool {
    !id.trim().is_empty() && id.len() <= 512 && !id.chars().any(char::is_control)
}

fn invalid(context: &str) -> ClientError {
    ClientError::ProtocolMismatch(format!("invalid native message query {context}"))
}

fn validate_query(limit: u64, ids: &[Option<&str>]) -> Result<(), ClientError> {
    if !(1..=100).contains(&limit) || ids.iter().flatten().any(|id| !valid_id(id)) {
        return Err(invalid("limit or entry cursor"));
    }
    Ok(())
}

impl MessageSummaryQuery {
    pub fn validate(&self) -> Result<(), ClientError> {
        validate_query(
            self.limit,
            &[self.before_id.as_deref(), self.leaf_id.as_deref()],
        )?;
        if self.role != "user" {
            return Err(invalid("summary role"));
        }
        Ok(())
    }
}

impl MessageWindowQuery {
    pub fn validate(&self) -> Result<(), ClientError> {
        let cursors = [
            self.anchor_id.as_deref(),
            self.before_id.as_deref(),
            self.after_id.as_deref(),
        ];
        validate_query(
            self.limit,
            &[cursors[0], cursors[1], cursors[2], self.leaf_id.as_deref()],
        )?;
        if cursors.iter().flatten().count() != 1 {
            return Err(invalid("window cursor selection"));
        }
        Ok(())
    }
}

fn validate_edges(
    ids: &[&str],
    first: Option<&str>,
    last: Option<&str>,
    leaf: Option<&str>,
    pinned: Option<&str>,
) -> Result<(), ClientError> {
    let unique: HashSet<_> = ids.iter().copied().collect();
    if unique.len() != ids.len()
        || ids.iter().any(|id| !valid_id(id))
        || first != ids.first().copied()
        || last != ids.last().copied()
        || leaf.is_some_and(|id| !valid_id(id))
        || (!ids.is_empty() && leaf.is_none())
        || pinned.is_some_and(|pin| Some(pin) != leaf)
    {
        return Err(invalid("page edges or leaf pin"));
    }
    Ok(())
}

impl MessageSummaryPage {
    /// Validate host metadata without sorting opaque IDs or inventing cursors.
    pub fn validate(&self, query: &MessageSummaryQuery) -> Result<(), ClientError> {
        query.validate()?;
        let ids: Vec<_> = self.messages.iter().map(|m| m.entry_id.as_str()).collect();
        validate_edges(
            &ids,
            self.first_id.as_deref(),
            self.last_id.as_deref(),
            self.leaf_id.as_deref(),
            query.leaf_id.as_deref(),
        )?;
        if !self.filtered
            || self.role != "user"
            || ids.len() as u64 > query.limit
            || self
                .messages
                .iter()
                .any(|m| m.role != "user" || m.preview.chars().count() > 140)
            || self.next_before_id.as_ref()
                != if self.has_more {
                    self.first_id.as_ref()
                } else {
                    None
                }
            || (self.has_more && ids.is_empty())
            || query
                .before_id
                .as_deref()
                .is_some_and(|id| ids.contains(&id))
        {
            return Err(invalid("summary page"));
        }
        Ok(())
    }
}

impl MessageWindowPage {
    pub fn validate(&self, query: &MessageWindowQuery) -> Result<(), ClientError> {
        query.validate()?;
        let ids: Vec<_> = self
            .messages
            .iter()
            .map(|m| m["entryId"].as_str().unwrap_or(""))
            .collect();
        validate_edges(
            &ids,
            self.first_id.as_deref(),
            self.last_id.as_deref(),
            self.leaf_id.as_deref(),
            query.leaf_id.as_deref(),
        )?;
        if ids.len() as u64 > query.limit
            || query
                .anchor_id
                .as_deref()
                .is_some_and(|id| !ids.contains(&id))
            || query
                .before_id
                .as_deref()
                .is_some_and(|id| ids.contains(&id))
            || query
                .after_id
                .as_deref()
                .is_some_and(|id| ids.contains(&id))
            || self.next_before_id.as_ref()
                != if self.has_more_before {
                    self.first_id.as_ref()
                } else {
                    None
                }
            || self.next_after_id.as_ref()
                != if self.has_more_after {
                    self.last_id.as_ref()
                } else {
                    None
                }
            || (ids.is_empty() && (self.has_more_before || self.has_more_after))
        {
            return Err(invalid("window page"));
        }
        Ok(())
    }
}

pub(crate) fn compatibility_error(error: ClientError, operation: &str) -> ClientError {
    match error {
        ClientError::Api {
            status: reqwest::StatusCode::NOT_FOUND | reqwest::StatusCode::NOT_IMPLEMENTED,
            ..
        } => unsupported(operation, "legacy native message queries"),
        error => error,
    }
}

impl WorkClient {
    async fn message_query<Q: Serialize>(
        &self,
        session: &str,
        endpoint: &str,
        query: &Q,
    ) -> Result<Value, ClientError> {
        let mut url = self.url(&format!("api/v1/sessions/{session}/{endpoint}"))?;
        // The same camelCase, null-free object is used by native command_data.
        let data = serde_json::to_value(query)?;
        for (key, value) in data.as_object().ok_or_else(|| invalid("parameters"))? {
            let value = match value {
                Value::String(s) => s.clone(),
                value => value.to_string(),
            };
            url.query_pairs_mut().append_pair(key, &value);
        }
        self.get_url(url).await
    }

    pub async fn message_summaries(
        &self,
        session: &str,
        query: &MessageSummaryQuery,
    ) -> Result<MessageSummaryPage, ClientError> {
        query.validate()?;
        let data = self
            .message_query(session, "message-summaries", query)
            .await
            .map_err(|e| compatibility_error(e, "message_summaries"))?;
        let page: MessageSummaryPage =
            serde_json::from_value(data).map_err(|_| invalid("summary response"))?;
        page.validate(query)?;
        Ok(page)
    }

    pub async fn message_window(
        &self,
        session: &str,
        query: &MessageWindowQuery,
    ) -> Result<MessageWindowPage, ClientError> {
        query.validate()?;
        let data = self
            .message_query(session, "message-window", query)
            .await
            .map_err(|e| compatibility_error(e, "message_window"))?;
        let page: MessageWindowPage =
            serde_json::from_value(data).map_err(|_| invalid("window response"))?;
        page.validate(query)?;
        Ok(page)
    }
}

#[cfg(test)]
mod message_queries_tests {
    use super::*;
    use serde_json::json;

    fn summaries() -> Value {
        json!({"filtered":true,"role":"user","messages":[
            {"entryId":"z","role":"user","preview":"first","timestamp":2,"hasImages":false},
            {"entryId":"a","role":"user","preview":"second","timestamp":1,"hasImages":true}],
            "firstId":"z","lastId":"a","nextBeforeId":"z","hasMore":true,"leafId":"leaf"})
    }
    fn window() -> Value {
        json!({"messages":[{"entryId":"z","id":"presentation"},{"entryId":"a"}],
            "firstId":"z","lastId":"a","nextBeforeId":"z","nextAfterId":"a",
            "hasMoreBefore":true,"hasMoreAfter":true,"leafId":"leaf"})
    }
    fn sq() -> MessageSummaryQuery {
        MessageSummaryQuery {
            leaf_id: Some("leaf".into()),
            ..Default::default()
        }
    }
    fn wq() -> MessageWindowQuery {
        MessageWindowQuery {
            anchor_id: Some("a".into()),
            leaf_id: Some("leaf".into()),
            ..Default::default()
        }
    }
    #[test]
    fn defaults_serialization_and_exclusive_cursor() {
        let q: MessageSummaryQuery = serde_json::from_value(json!({})).unwrap();
        assert_eq!(q.limit, 10);
        assert_eq!(q.role, "user");
        assert_eq!(
            serde_json::to_value(q).unwrap(),
            json!({"limit":10,"role":"user"})
        );
        assert_eq!(wq().limit, 48);
        assert!(MessageWindowQuery::default().validate().is_err());
        let mut q = wq();
        assert!(q.validate().is_ok());
        q.before_id = Some("cursor".into());
        assert!(q.validate().is_err());
        for id in [
            "", " ", "bad
",
        ] {
            let mut q = sq();
            q.before_id = Some(id.into());
            assert!(q.validate().is_err(), "{id:?}");
        }
        let mut opaque = sq();
        opaque.before_id = Some("anonymous-host-owned".into());
        assert!(
            opaque.validate().is_ok(),
            "host IDs are opaque; prefix is not provenance"
        );
        for limit in [0, 101] {
            let mut q = sq();
            q.limit = limit;
            assert!(q.validate().is_err());
        }
    }
    #[test]
    fn preserves_host_order_and_leaf_pin() {
        let page: MessageSummaryPage = serde_json::from_value(summaries()).unwrap();
        page.validate(&sq()).unwrap();
        assert_eq!(
            page.messages
                .iter()
                .map(|m| m.entry_id.as_str())
                .collect::<Vec<_>>(),
            ["z", "a"]
        );
        let page: MessageWindowPage = serde_json::from_value(window()).unwrap();
        page.validate(&wq()).unwrap();
        assert_eq!(page.messages[0]["entryId"], "z");
        let mut q = wq();
        q.leaf_id = Some("changed".into());
        assert!(page.validate(&q).is_err());
    }
    #[test]
    fn rejects_malformed_summary_metadata() {
        for (key, value) in [
            ("filtered", json!(false)),
            ("role", json!("assistant")),
            ("firstId", json!("a")),
            ("nextBeforeId", json!("a")),
            ("leafId", Value::Null),
            ("messages", json!([])),
        ] {
            let mut data = summaries();
            data[key] = value;
            let page: MessageSummaryPage = serde_json::from_value(data).unwrap();
            assert!(page.validate(&sq()).is_err(), "{key}");
        }
        let mut data = summaries();
        data["messages"][1]["entryId"] = json!("z");
        assert!(
            serde_json::from_value::<MessageSummaryPage>(data)
                .unwrap()
                .validate(&sq())
                .is_err()
        );
        let mut q = sq();
        q.before_id = Some("a".into());
        assert!(
            serde_json::from_value::<MessageSummaryPage>(summaries())
                .unwrap()
                .validate(&q)
                .is_err()
        );
        let mut q = sq();
        q.limit = 1;
        assert!(
            serde_json::from_value::<MessageSummaryPage>(summaries())
                .unwrap()
                .validate(&q)
                .is_err()
        );
    }
    #[test]
    fn rejects_malformed_window_and_non_entry_ids() {
        for (key, value) in [
            ("firstId", json!("a")),
            ("nextAfterId", json!("z")),
            ("leafId", json!("changed")),
            ("messages", json!([])),
            ("messages", json!([{"id":"z"},{"id":"a"}])),
        ] {
            let mut data = window();
            data[key] = value;
            assert!(
                serde_json::from_value::<MessageWindowPage>(data)
                    .unwrap()
                    .validate(&wq())
                    .is_err(),
                "{key}"
            );
        }
        let page: MessageWindowPage = serde_json::from_value(window()).unwrap();
        for field in ["anchorId", "beforeId", "afterId"] {
            let id = if field == "anchorId" { "missing" } else { "z" };
            let q = serde_json::from_value(json!({field:id,"leafId":"leaf"})).unwrap();
            assert!(page.validate(&q).is_err());
        }
    }
    #[test]
    fn empty_terminal_pages_and_snapshot_leaf() {
        let data = json!({"messages":[],"firstId":null,"lastId":null,"nextBeforeId":null,
            "nextAfterId":null,"hasMoreBefore":false,"hasMoreAfter":false,"leafId":"leaf"});
        let page: MessageWindowPage = serde_json::from_value(data).unwrap();
        let q = MessageWindowQuery {
            before_id: Some("cursor".into()),
            leaf_id: Some("leaf".into()),
            ..Default::default()
        };
        page.validate(&q).unwrap();
        let snapshot: crate::Snapshot =
            serde_json::from_value(json!({"sessionId":"fixture","leafId":"leaf"})).unwrap();
        assert_eq!(snapshot.leaf_id.as_deref(), Some("leaf"));
        let old: crate::Snapshot = serde_json::from_value(json!({"sessionId":"fixture"})).unwrap();
        assert_eq!(old.leaf_id, None);
    }
    #[test]
    fn compatibility_errors_only_map_404_and_501() {
        for status in [404, 501] {
            assert!(matches!(
                compatibility_error(
                    ClientError::Api {
                        status: reqwest::StatusCode::from_u16(status).unwrap(),
                        message: "fixture".into()
                    },
                    "message_summaries"
                ),
                ClientError::Unsupported { .. }
            ));
        }
        for status in [400, 409, 500] {
            assert!(
                matches!(compatibility_error(ClientError::Api {status:reqwest::StatusCode::from_u16(status).unwrap(),message:"fixture".into()},"message_window"),ClientError::Api {status:s,..} if s.as_u16()==status)
            );
        }
    }
    #[test]
    fn native_payloads_are_null_free_camel_case_and_host_response_is_not_a_page() {
        let request =
            crate::payload_with_type("get_message_summaries", serde_json::to_value(sq()).unwrap());
        assert_eq!(
            request,
            json!({"type":"get_message_summaries","limit":10,"role":"user","leafId":"leaf"})
        );
        let request =
            crate::payload_with_type("get_message_window", serde_json::to_value(wq()).unwrap());
        assert_eq!(
            request,
            json!({"type":"get_message_window","limit":48,"anchorId":"a","leafId":"leaf"})
        );
        let envelope = json!({"type":"response","command":"get_message_summaries","success":true,"data":summaries()});
        let response: crate::HostResponse<MessageSummaryPage> =
            serde_json::from_value(envelope.clone()).unwrap();
        assert!(response.success);
        response.data.unwrap().validate(&sq()).unwrap();
        assert!(serde_json::from_value::<MessageSummaryPage>(envelope).is_err());
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn http_fixture(
        status: u16,
        body: Value,
    ) -> (crate::WorkspaceClient, std::thread::JoinHandle<String>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut bytes = Vec::new();
            let mut buf = [0; 1024];
            while !bytes.windows(4).any(|w| w == b"\r\n\r\n") {
                let n = socket.read(&mut buf).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buf[..n]);
            }
            let body = body.to_string();
            write!(socket,"HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
            String::from_utf8(bytes).unwrap()
        });
        (
            WorkClient::new(&format!("http://{address}/"))
                .unwrap()
                .into(),
            server,
        )
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[tokio::test]
    async fn remote_get_routes_encode_cursors_and_preserve_page_order() {
        let (client, server) = http_fixture(200, summaries());
        let mut q = sq();
        q.before_id = Some("cursor /?&".into());
        let page = client.message_summaries("fixture", &q).await.unwrap();
        assert_eq!(page.messages[0].entry_id, "z");
        let request = server.join().unwrap();
        let line = request.lines().next().unwrap();
        assert!(
            line.starts_with("GET /api/v1/sessions/fixture/message-summaries?"),
            "{line}"
        );
        let url = reqwest::Url::parse(&format!(
            "http://fixture{}",
            line.split_whitespace().nth(1).unwrap()
        ))
        .unwrap();
        let params: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(params.get("beforeId").unwrap(), "cursor /?&");
        assert_eq!(params.get("leafId").unwrap(), "leaf");
        assert_eq!(params.get("limit").unwrap(), "10");
        assert_eq!(params.get("role").unwrap(), "user");
        assert_eq!(params.len(), 4);
        assert!(!request.to_ascii_lowercase().contains("authorization:"));
        let (client, server) = http_fixture(200, window());
        client.message_window("fixture", &wq()).await.unwrap();
        let request = server.join().unwrap();
        assert!(request.starts_with("GET /api/v1/sessions/fixture/message-window?"));
        assert!(request.contains("anchorId=a"));
        assert!(request.contains("leafId=leaf"));
        assert!(request.contains("limit=48"));
        assert!(!request.contains("beforeId="));
        assert!(!request.contains("afterId="));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[tokio::test]
    async fn remote_compatibility_conflict_and_host_envelope_errors() {
        for status in [404, 501, 409] {
            let (client, server) = http_fixture(status, json!({"error":"fixture"}));
            let error = client
                .message_summaries("fixture", &sq())
                .await
                .unwrap_err();
            if status == 409 {
                assert!(matches!(error,ClientError::Api{status,..} if status.as_u16()==409));
            } else {
                assert!(matches!(error, ClientError::Unsupported { .. }));
            }
            server.join().unwrap();
            let (client, server) = http_fixture(status, json!({"error":"fixture"}));
            let error = client.message_window("fixture", &wq()).await.unwrap_err();
            assert!(if status == 409 {
                matches!(error, ClientError::Api { .. })
            } else {
                matches!(error, ClientError::Unsupported { .. })
            });
            server.join().unwrap();
        }
        let (client, server) = http_fixture(
            200,
            json!({"success":false,"data":summaries(),"error":"native failed"}),
        );
        assert!(matches!(
            client.message_summaries("fixture", &sq()).await,
            Err(ClientError::ProtocolMismatch(_))
        ));
        server.join().unwrap();
    }
}
