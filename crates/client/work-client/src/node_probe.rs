use futures_util::StreamExt;
use reqwest::{StatusCode, Url};
use rpi_node_core::{NodeDiscoveryDocument, NodeTransportKind};
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::time::Duration;

use crate::{AuthState, ClientError, Health, WorkClient};

const PROBE_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_DISCOVERY_BYTES: usize = 128 * 1024;
const MAX_HEALTH_BYTES: usize = 64 * 1024;
const MAX_AUTH_BYTES: usize = 64 * 1024;

/// Public, credential-free identity probe for a Work node.
///
/// A probe deliberately creates a fresh HTTP client. It never reuses the
/// caller's cookie jar, bearer token, or private control capability, so a
/// node switch cannot accidentally authenticate a request against the old
/// node. Protected capabilities and transport endpoints are validated later
/// by the authenticated session handshake.
#[derive(Debug, Clone)]
pub struct NodeProbe {
    pub endpoint: String,
    pub discovery: NodeDiscoveryDocument,
    pub health: Health,
    pub auth: AuthState,
}

impl NodeProbe {
    /// Probe a root HTTP(S) origin without sending credentials.
    pub async fn probe(base_url: &str) -> Result<Self, ClientError> {
        let endpoint = normalize_endpoint(base_url)?;
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()?;

        let discovery: NodeDiscoveryDocument = get_json(
            &http,
            endpoint.join("/.well-known/rpi-node")?,
            MAX_DISCOVERY_BYTES,
        )
        .await?;
        discovery
            .validate()
            .map_err(|error| ClientError::ProtocolMismatch(format!("node discovery: {error}")))?;
        validate_discovery(&endpoint, &discovery)?;

        let health_value: Value =
            get_json(&http, endpoint.join("api/v1/health")?, MAX_HEALTH_BYTES).await?;
        let node_id = health_value
            .get("nodeId")
            .and_then(Value::as_str)
            .filter(|node_id| !node_id.is_empty())
            .ok_or_else(|| ClientError::ProtocolMismatch("node health omitted nodeId".into()))?;
        if node_id != discovery.node_id {
            return Err(ClientError::ProtocolMismatch(
                "node identity changed during probe".into(),
            ));
        }
        let health = serde_json::from_value(health_value)?;

        let auth: AuthState =
            get_json(&http, endpoint.join("api/v1/auth")?, MAX_AUTH_BYTES).await?;
        Ok(Self {
            endpoint: endpoint.to_string(),
            discovery,
            health,
            auth,
        })
    }

    #[must_use]
    pub fn node_id(&self) -> &str {
        &self.discovery.node_id
    }
}

impl WorkClient {
    /// Probe this client's origin with a fresh, unauthenticated transport.
    pub async fn probe(&self) -> Result<NodeProbe, ClientError> {
        NodeProbe::probe(self.base_url.as_str()).await
    }

    /// Probe a node before constructing an authenticated client for it.
    pub async fn probe_node(base_url: &str) -> Result<NodeProbe, ClientError> {
        NodeProbe::probe(base_url).await
    }
}

async fn get_json<T: DeserializeOwned>(
    http: &reqwest::Client,
    url: Url,
    max_bytes: usize,
) -> Result<T, ClientError> {
    let response = http
        .get(url)
        .timeout(PROBE_TIMEOUT)
        .header("x-rpi-web", "1")
        .send()
        .await
        .map_err(map_probe_http_error)?;
    let status = response.status();
    if !status.is_success() {
        return Err(ClientError::Api {
            status,
            message: if matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) {
                "node probe was not authorized".into()
            } else {
                "node probe request failed".into()
            },
        });
    }

    let mut stream = response.bytes_stream();
    let mut body = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(map_probe_http_error)?;
        if body.len().saturating_add(chunk.len()) > max_bytes {
            return Err(ClientError::ProtocolMismatch(
                "node probe response exceeded its size limit".into(),
            ));
        }
        body.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&body).map_err(ClientError::from)
}

fn map_probe_http_error(error: reqwest::Error) -> ClientError {
    if error.is_timeout() {
        ClientError::RequestTimeout("node probe".into())
    } else {
        ClientError::Http(error)
    }
}

fn normalize_endpoint(value: &str) -> Result<Url, ClientError> {
    if value.len() > 2048
        || value
            .bytes()
            .any(|byte| byte.is_ascii_whitespace() || byte.is_ascii_control())
        || value.contains(['\\', '?', '#', '@'])
    {
        return Err(ClientError::ProtocolMismatch(
            "node probe requires a clean origin".into(),
        ));
    }
    let url = Url::parse(value)?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(ClientError::ProtocolMismatch(
            "node probe requires an HTTP(S) origin".into(),
        ));
    }
    Ok(url)
}

fn validate_discovery(
    endpoint: &Url,
    discovery: &NodeDiscoveryDocument,
) -> Result<(), ClientError> {
    if discovery.node_id.is_empty() || discovery.owner_epoch.is_empty() {
        return Err(ClientError::ProtocolMismatch(
            "node discovery omitted identity".into(),
        ));
    }
    let expected_origin = endpoint.origin().ascii_serialization();
    let has_realtime = discovery.bindings.iter().any(|binding| {
        binding.protocol.as_str() == "rpi-work"
            && binding.transport.kind == NodeTransportKind::WebSocket
            && binding.transport.origin.as_deref() == Some(expected_origin.as_str())
            && binding.transport.path.as_deref() == Some("/api/v1/realtime")
            && binding.transport.subprotocol.as_deref() == Some("rpi-work.realtime.v1")
    });
    if !has_realtime {
        return Err(ClientError::ProtocolMismatch(
            "node discovery omitted the Work realtime binding".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::normalize_endpoint;

    #[test]
    fn probe_accepts_only_clean_root_http_origins() {
        assert!(normalize_endpoint("http://localhost:8080/").is_ok());
        for value in [
            "http://localhost:8080/api",
            "http://user@localhost:8080/",
            "http://localhost:8080/?ticket=secret",
            "file:///tmp/node",
        ] {
            assert!(normalize_endpoint(value).is_err(), "accepted {value}");
        }
    }
}
