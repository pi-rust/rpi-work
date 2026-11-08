//! Client-side LAN probe; node policy and lifecycle live in the extension host.
pub use work_http_runtime::public_address;
pub use work_node_host::http::Sharing;
// Read only the compatibility view needed by pairing. Extra fields supplied by
// a newer component (including custom settings) must not break composition.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SharingView {
    listening: bool,
    enabled: bool,
    status: String,
    #[serde(default)]
    warning: String,
    #[serde(default)]
    public_url: String,
    #[serde(default)]
    app_url: String,
    #[serde(default)]
    address_hint: String,
    #[serde(default)]
    address: String,
    #[serde(default)]
    node_id: String,
    #[serde(default)]
    web_hosted: bool,
}
/// Compatibility projection for pairing. Policy lives in the extension.
pub async fn sharing() -> Sharing {
    let Some(client) = crate::client() else {
        return Sharing::default();
    };
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        client.component_capability("node.sharing.read", serde_json::json!({})),
    )
    .await;
    let Some(snapshot) = result
        .ok()
        .and_then(Result::ok)
        .and_then(|v| serde_json::from_value::<SharingView>(v).ok())
    else {
        return Sharing::default();
    };
    Sharing {
        listening: snapshot.listening,
        enabled: snapshot.enabled,
        status: snapshot.status,
        warning: snapshot.warning,
        public_url: snapshot.public_url,
        app_url: snapshot.app_url,
        address_hint: snapshot.address_hint,
        address: snapshot.address,
        node_id: snapshot.node_id,
        web_hosted: snapshot.web_hosted,
        ..Sharing::default()
    }
}
/// Issue an owner-only, one-time delegation through the declared node capability.
/// # Errors
/// Returns an error when the local node is unavailable, sharing is disabled, or issuance fails.
pub async fn issue_qr_authorization() -> Result<work_client::QrAuthorization, String> {
    let client = crate::client().ok_or("本机节点尚未准备好")?;
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        client.component_capability("node.sharing.authorize", serde_json::json!({})),
    )
    .await;
    result
        .ok()
        .and_then(Result::ok)
        .and_then(|v| serde_json::from_value(v).ok())
        .ok_or_else(|| "无法签发本机授权，请确认共享已开启".into())
}
/// Credential-free native LAN probe. `WebView` fetch cannot bypass Origin policy.
/// No redirects, cookies, passwords, tickets or response bodies enter errors.
/// # Errors
/// Returns an error for invalid/unreachable origins or an unavailable sharing entrance.
pub async fn authorization_endpoint_node(url: &str) -> Result<String, String> {
    let (url, _) = public_address(url)?;
    // A fresh transport has no owner/remote credentials. Native rustls handles
    // HTTPS as well as LAN HTTP; redirects are disabled by WorkClient.
    let client = work_client::WorkClient::new(&url).map_err(|_| "无法验证分享地址。".to_owned())?;
    tokio::time::timeout(
        std::time::Duration::from_secs(15),
        client.qr_authorization_node(),
    )
    .await
    .ok()
    .and_then(Result::ok)
    .flatten()
    .ok_or_else(|| "分享地址不可达、共享已关闭或节点无法验证。".into())
}
