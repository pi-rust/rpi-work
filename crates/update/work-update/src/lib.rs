//! Signed release discovery shared by desktop, Android and the browser.
use base64::{Engine as _, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signature, VerifyingKey};
use futures_util::StreamExt;
use semver::Version;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const REPOSITORY: &str = "pi-rust/rpi-work";
pub const RELEASES_URL: &str = "https://github.com/pi-rust/rpi-work/releases";
pub const PUBLIC_KEY: &str = include_str!("../update-public-key.txt");
const MAX_MANIFEST: usize = 256 * 1024;
pub const MAX_DOWNLOAD: u64 = 256 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    pub name: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema: u32,
    pub version: String,
    pub notes: String,
    pub artifacts: BTreeMap<String, Artifact>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    pub manifest: Manifest,
    pub target: String,
    pub artifact: Artifact,
}
impl Release {
    pub fn download_url(&self) -> String {
        format!(
            "{RELEASES_URL}/download/v{}/{}",
            self.manifest.version, self.artifact.name
        )
    }
}

pub fn verify_manifest(
    bytes: &[u8],
    signature: &str,
    public_key: &str,
) -> Result<Manifest, String> {
    if bytes.len() > MAX_MANIFEST {
        return Err("更新清单过大".into());
    }
    let key: [u8; 32] = STANDARD
        .decode(public_key.trim())
        .map_err(|_| "更新公钥无效")?
        .try_into()
        .map_err(|_| "更新公钥长度无效")?;
    let signature = Signature::from_slice(
        &STANDARD
            .decode(signature.trim())
            .map_err(|_| "更新签名格式无效")?,
    )
    .map_err(|_| "更新签名长度无效")?;
    VerifyingKey::from_bytes(&key)
        .map_err(|_| "更新公钥无效")?
        .verify_strict(bytes, &signature)
        .map_err(|_| "更新签名校验失败，已阻止下载")?;
    let manifest: Manifest = serde_json::from_slice(bytes).map_err(|_| "更新清单格式无效")?;
    let version = Version::parse(&manifest.version).map_err(|_| "更新版本号无效")?;
    if manifest.schema != 1 || !version.pre.is_empty() || !version.build.is_empty() {
        return Err("不支持的更新清单或非稳定版本".into());
    }
    for artifact in manifest.artifacts.values() {
        if artifact.size == 0
            || artifact.size > MAX_DOWNLOAD
            || artifact.sha256.len() != 64
            || !artifact.sha256.bytes().all(|v| v.is_ascii_hexdigit())
            || artifact.name.is_empty()
            || !artifact
                .name
                .bytes()
                .all(|v| v.is_ascii_alphanumeric() || b"._-".contains(&v))
        {
            return Err("更新文件描述无效".into());
        }
    }
    Ok(manifest)
}

pub fn select_release(
    manifest: Manifest,
    current: &str,
    target: &str,
) -> Result<Option<Release>, String> {
    let newer = Version::parse(&manifest.version).map_err(|_| "更新版本无效")?;
    let current = Version::parse(current).map_err(|_| "当前版本无效")?;
    if newer <= current {
        return Ok(None);
    }
    let artifact = manifest
        .artifacts
        .get(target)
        .cloned()
        .ok_or("此版本尚未提供当前平台的更新")?;
    let expected = if target.contains("android") {
        format!("rpi-work-{newer}-{target}.apk")
    } else {
        let extension = if target.contains("windows") || target == "wasm32-unknown-unknown" {
            "zip"
        } else {
            "tar.gz"
        };
        format!("rpi-work-{newer}-{target}.{extension}")
    };
    if artifact.name != expected {
        return Err("更新文件与版本、平台不匹配".into());
    }
    Ok(Some(Release {
        manifest,
        target: target.into(),
        artifact,
    }))
}

pub fn target() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => "x86_64-pc-windows-msvc",
        ("windows", "aarch64") => "aarch64-pc-windows-msvc",
        ("macos", "x86_64") => "x86_64-apple-darwin",
        ("macos", "aarch64") => "aarch64-apple-darwin",
        ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
        ("linux", "aarch64") => "aarch64-unknown-linux-gnu",
        ("android", "aarch64") => "aarch64-linux-android",
        (_, "wasm32") => "wasm32-unknown-unknown",
        _ => "unsupported",
    }
}

pub fn http_client() -> Result<reqwest::Client, String> {
    let builder = reqwest::Client::builder();
    #[cfg(not(target_arch = "wasm32"))]
    let builder = builder
        .user_agent("rpi-work-updater")
        .connect_timeout(std::time::Duration::from_secs(15))
        .timeout(std::time::Duration::from_secs(300))
        .https_only(true);
    builder.build().map_err(|e| e.to_string())
}

async fn bounded(response: reqwest::Response, limit: usize) -> Result<Vec<u8>, String> {
    response.error_for_status_ref().map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        if bytes.len().saturating_add(chunk.len()) > limit {
            return Err("更新响应超出大小限制".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

pub async fn check(current: &str) -> Result<Option<Release>, String> {
    let client = http_client()?;
    let response = client
        .get(format!(
            "https://api.github.com/repos/{REPOSITORY}/releases/latest"
        ))
        .send()
        .await
        .map_err(|e| format!("无法连接更新服务：{e}"))?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let metadata: serde_json::Value =
        serde_json::from_slice(&bounded(response, MAX_MANIFEST).await?)
            .map_err(|e| e.to_string())?;
    let tag = metadata["tag_name"].as_str().ok_or("发布信息缺少版本")?;
    let version = tag.strip_prefix('v').ok_or("发布标签无效")?;
    if Version::parse(version).map_err(|_| "发布版本号无效")?
        <= Version::parse(current).map_err(|_| "当前版本号无效")?
    {
        return Ok(None);
    }
    #[cfg(target_arch = "wasm32")]
    {
        // Browsers discover versions via the CORS-enabled GitHub API. Web assets
        // are deployed by the node owner; the browser never installs release binaries.
        let parsed = Version::parse(version).map_err(|_| "发布版本号无效")?;
        if !parsed.pre.is_empty() || !parsed.build.is_empty() {
            return Err("只支持稳定通道".into());
        }
        let artifact = Artifact {
            name: format!("rpi-work-{version}-wasm32-unknown-unknown.zip"),
            size: 0,
            sha256: String::new(),
        };
        Ok(Some(Release {
            manifest: Manifest {
                schema: 1,
                version: version.into(),
                notes: metadata["body"].as_str().unwrap_or_default().into(),
                artifacts: BTreeMap::new(),
            },
            target: target().into(),
            artifact,
        }))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let base = format!("{RELEASES_URL}/download/{tag}");
        // URLs are constructed from the fixed repository; remote metadata cannot select an arbitrary host.
        let manifest = bounded(
            client
                .get(format!("{base}/update.json"))
                .send()
                .await
                .map_err(|e| e.to_string())?,
            MAX_MANIFEST,
        )
        .await?;
        let signature = bounded(
            client
                .get(format!("{base}/update.json.sig"))
                .send()
                .await
                .map_err(|e| e.to_string())?,
            256,
        )
        .await?;
        let manifest = verify_manifest(
            &manifest,
            std::str::from_utf8(&signature).map_err(|_| "签名编码无效")?,
            PUBLIC_KEY,
        )?;
        if manifest.version != version {
            return Err("发布标签和签名清单版本不一致".into());
        }
        select_release(manifest, current, target())
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub mod native;

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    fn signed() -> (Vec<u8>, String, String) {
        let key = SigningKey::from_bytes(&[42; 32]);
        let manifest = serde_json::json!({"schema":1,"version":"0.2.0","notes":"test","artifacts":{
            "x86_64-pc-windows-msvc":{"name":"rpi-work-0.2.0-x86_64-pc-windows-msvc.zip","size":128,"sha256":"a".repeat(64)}
        }});
        let bytes = serde_json::to_vec(&manifest).unwrap();
        let signature = STANDARD.encode(key.sign(&bytes).to_bytes());
        (
            bytes,
            signature,
            STANDARD.encode(key.verifying_key().to_bytes()),
        )
    }
    #[test]
    fn signed_manifest_rejects_tampering_and_wrong_key() {
        let (mut bytes, signature, key) = signed();
        assert!(verify_manifest(&bytes, &signature, &key).is_ok());
        bytes[0] ^= 1;
        assert!(verify_manifest(&bytes, &signature, &key).is_err());
        let (bytes, signature, _) = signed();
        let wrong = STANDARD.encode(SigningKey::from_bytes(&[43; 32]).verifying_key().to_bytes());
        assert!(verify_manifest(&bytes, &signature, &wrong).is_err());
    }
    #[test]
    fn semver_selection_rejects_downgrade_missing_platform_and_mismatched_artifact() {
        let (bytes, signature, key) = signed();
        let manifest = verify_manifest(&bytes, &signature, &key).unwrap();
        assert!(
            select_release(manifest.clone(), "0.1.9", "x86_64-pc-windows-msvc")
                .unwrap()
                .is_some()
        );
        assert!(
            select_release(manifest.clone(), "0.2.0", "x86_64-pc-windows-msvc")
                .unwrap()
                .is_none()
        );
        assert!(
            select_release(manifest.clone(), "0.10.0", "x86_64-pc-windows-msvc")
                .unwrap()
                .is_none()
        );
        assert!(select_release(manifest.clone(), "0.1.0", "aarch64-linux-android").is_err());
        let mut invalid = manifest;
        invalid.artifacts.values_mut().next().unwrap().name = "other.zip".into();
        assert!(select_release(invalid, "0.1.0", "x86_64-pc-windows-msvc").is_err());
    }
}
