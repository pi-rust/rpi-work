//! Scoped image/text attachments and ordinary, independent BTW sessions.
//! Attachment preparation never sends; the workspace owns submission and ACK cleanup.
use dioxus::{core::Task, prelude::*};
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    future::Future,
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
};
use work_client::{ModelDescriptor, SessionConnection, WorkspaceClient};
use work_core::{ConnectionState, MessageBlock, RunPhase};

pub const CSS: &str = include_str!("composer_extras.css");
const MAX_FILES: usize = 8;
const MAX_FILE: usize = 5 * 1024 * 1024;
const MAX_TOTAL: usize = 10 * 1024 * 1024;
const MAX_QUOTE: usize = 32 * 1024;
const MAX_TEXT_BYTES: usize = 32 * 1024;
const MAX_IMAGES: usize = 4;
const MAX_IMAGE_BYTES: usize = 256 * 1024;
const MAX_TOTAL_IMAGE_BYTES: usize = 512 * 1024;
const MAX_IMAGE_BASE64_BYTES: usize = 349_528;
const MAX_CONTENT_BLOCKS: usize = 64;
const MAX_PROMPT_BYTES: usize = 768 * 1024;
const SEND_IMAGE_TARGET: usize = 128 * 1024;
static NEXT_ATTACHMENT_READER: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, PartialEq)]
pub struct AttachmentSubmission {
    pub snapshot: AttachmentSnapshot,
    pub text: String,
    pub content_blocks: Option<Value>,
}

#[derive(Clone, PartialEq, Default)]
pub struct AttachmentStore {
    scopes: BTreeMap<String, AttachmentScopeState>,
    next_id: u64,
}
impl AttachmentStore {
    pub(crate) fn move_to_session(&mut self, from: &str, to: String) {
        if let Some(state) = self.scopes.remove(from) {
            self.scopes.insert(to, state);
        }
    }

    /// Metadata only: pending-state checks must never clone attachment contents.
    pub fn snapshot(&self, scope: &str) -> AttachmentSnapshot {
        self.scopes
            .get(scope)
            .map_or_else(AttachmentSnapshot::default, |state| AttachmentSnapshot {
                files: state
                    .files
                    .iter()
                    .map(|file| (file.id, file.name.clone()))
                    .collect(),
                loading: state.loading,
                failed: !state.error.is_empty(),
            })
    }

    pub fn has_pending_content(&self) -> bool {
        self.scopes
            .values()
            .any(|scope| scope.loading || !scope.files.is_empty() || !scope.error.is_empty())
    }

    /// Build the exact text fallback and ordered native blocks without consuming
    /// or mutating the local files. The caller can acknowledge this snapshot only
    /// after the corresponding send has been accepted.
    pub fn prepare(
        &self,
        scope: &str,
        text: &str,
        ordered_images: bool,
    ) -> Result<AttachmentSubmission, String> {
        let Some(state) = self.scopes.get(scope) else {
            return Ok(AttachmentSubmission {
                snapshot: AttachmentSnapshot::default(),
                text: text.to_owned(),
                content_blocks: None,
            });
        };
        if state.loading {
            return Err("附件仍在读取；请等待读取完成后再发送。".into());
        }
        if !state.error.is_empty() {
            return Err(
                "附件读取或预览存在错误；请重新选择，或明确清除提示后仅发送已加入的附件。".into(),
            );
        }
        if state.files.is_empty() {
            return Ok(AttachmentSubmission {
                snapshot: self.snapshot(scope),
                text: text.to_owned(),
                content_blocks: None,
            });
        }
        let has_images = state
            .files
            .iter()
            .any(|file| raster_mime(&file.bytes).is_some());
        if has_images && !ordered_images {
            return Err("图片已粘贴到本地附件，但当前节点不支持有序图片内容（未声明 prompt_content 能力）；图片仍保留在本地，未发送。请切换到支持 prompt_content 的节点/Agent，或移除图片后发送文本。".into());
        }

        let mut full_text = text.to_owned();
        let mut image_blocks = Vec::new();
        let mut image_count = 0usize;
        let mut total_image_bytes = 0usize;
        for file in &state.files {
            let name = clean_filename(&file.name);
            if let Some(error) = &file.send_error {
                return Err(format!("{name}：{error}"));
            }
            if binary_document(&file.name, &file.bytes) {
                return Err(format!(
                    "{name}：当前仅支持图片和文本/代码附件，暂不支持 PDF、Office、压缩包等原文件发送。"
                ));
            }
            if let Some(mime) = raster_mime(&file.bytes) {
                if !matches!(mime, "image/png" | "image/jpeg") {
                    return Err(format!(
                        "{name}：仅支持 PNG/JPEG 图片发送；{mime} 仅可本地预览，未发送。"
                    ));
                }
                image_count += 1;
                if image_count > MAX_IMAGES {
                    return Err("图片附件最多发送 4 张；未自动丢弃其余图片。".into());
                }
                let (send_mime, encoded) = file.send_data.as_ref().map_or_else(
                    || (mime.to_owned(), base64(&file.bytes)),
                    |(send_mime, encoded)| (send_mime.clone(), encoded.clone()),
                );
                let decoded_size = file
                    .send_data
                    .as_ref()
                    .map_or(file.bytes.len(), |(_, data)| {
                        data.len() * 3 / 4 - data.bytes().rev().take_while(|b| *b == b'=').count()
                    });
                if decoded_size > MAX_IMAGE_BYTES || encoded.len() > MAX_IMAGE_BASE64_BYTES {
                    return Err(format!(
                        "{name}：图片发送副本超过单图 256 KiB 限制；未自动发送。"
                    ));
                }
                total_image_bytes = total_image_bytes.saturating_add(decoded_size);
                if total_image_bytes > MAX_TOTAL_IMAGE_BYTES {
                    return Err("图片发送副本超过合计 512 KiB 限制；未自动发送。".into());
                }
                append_attachment_text(&mut full_text, &format!("[图片附件：{name}]"));
                image_blocks.push(serde_json::json!({
                    "type": "image",
                    "mimeType": send_mime,
                    "data": encoded,
                }));
            } else if let Some(content) = text_attachment(&file.bytes) {
                append_attachment_text(&mut full_text, &format!("[附件：{name}]\n{content}"));
            } else {
                return Err(format!(
                    "{name}：不可发送的二进制文件（PDF/Word/zip 等仅展示，不会上传）。"
                ));
            }
        }
        if full_text.len() > MAX_TEXT_BYTES {
            return Err("附件文字内容超过 32 KiB；完整文件仍保留在本地，未发送。".into());
        }
        if image_blocks.len() + 1 > MAX_CONTENT_BLOCKS {
            return Err("附件内容块超过当前节点限制；未发送。".into());
        }
        let mut blocks = vec![serde_json::json!({"type":"text", "text":full_text})];
        blocks.extend(image_blocks);
        let content_blocks = Value::Array(blocks);
        if serde_json::to_vec(&content_blocks).map_or(true, |bytes| bytes.len() > MAX_PROMPT_BYTES)
        {
            return Err("附件命令超过 768 KiB；未发送。".into());
        }
        Ok(AttachmentSubmission {
            snapshot: self.snapshot(scope),
            text: full_text,
            content_blocks: (image_count > 0).then_some(content_blocks),
        })
    }

    /// Remove only IDs captured by `snapshot`; files added after preparation
    /// deliberately survive acknowledgement.
    pub fn acknowledge(&mut self, scope: &str, snapshot: &AttachmentSnapshot) {
        let remove_scope = {
            let Some(state) = self.scopes.get_mut(scope) else {
                return;
            };
            let ids: std::collections::BTreeSet<u64> =
                snapshot.files.iter().map(|(id, _)| *id).collect();
            state.files.retain(|file| !ids.contains(&file.id));
            state.files.is_empty() && !state.loading && state.error.is_empty()
        };
        if remove_scope {
            self.scopes.remove(scope);
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct AttachmentSnapshot {
    pub files: Vec<(u64, String)>,
    pub loading: bool,
    pub failed: bool,
}

#[cfg(test)]
mod attachment_snapshot_tests {
    use super::*;

    #[test]
    fn snapshots_are_scoped_metadata_and_include_loading() {
        let mut store = AttachmentStore::default();
        let mut file = local_attachment("private.txt".into(), b"never copy this payload");
        file.id = 7;
        store.scopes.insert(
            "node-a/session".into(),
            AttachmentScopeState {
                files: vec![file],
                loading: true,
                error: String::new(),
            },
        );
        assert_eq!(
            store.snapshot("node-b/session"),
            AttachmentSnapshot::default()
        );
        let snapshot = store.snapshot("node-a/session");
        assert_eq!(snapshot.files, vec![(7, "private.txt".into())]);
        assert!(snapshot.loading);
        store.scopes.get_mut("node-a/session").unwrap().loading = false;
        assert_ne!(snapshot, store.snapshot("node-a/session"));
        store.scopes.get_mut("node-a/session").unwrap().files[0].id = 8;
        assert_ne!(snapshot.files, store.snapshot("node-a/session").files);
    }

    fn store_with(files: &[(&str, &[u8])]) -> AttachmentStore {
        let mut store = AttachmentStore::default();
        for (index, (name, bytes)) in files.iter().enumerate() {
            let mut file = local_attachment((*name).into(), bytes);
            file.id = index as u64 + 1;
            store
                .scopes
                .entry("scope".into())
                .or_default()
                .files
                .push(file);
        }
        store
    }

    #[test]
    fn text_files_send_complete_utf8_on_legacy_nodes_without_fake_image_blocks() {
        let text = "完整的代码😀\n".repeat(100);
        let store = store_with(&[("script.rs", text.as_bytes())]);
        let sent = store.prepare("scope", "请检查", false).unwrap();
        assert_eq!(sent.text, format!("请检查\n[附件：script.rs]\n{text}"));
        assert!(sent.content_blocks.is_none());
        assert_eq!(store.snapshot("scope").files.len(), 1);
        assert_eq!(
            store
                .prepare("other", "原文字", true)
                .unwrap()
                .snapshot
                .files,
            [] as [(u64, std::string::String); 0]
        );
    }

    #[test]
    fn image_payload_has_real_inline_bytes_and_never_drops_unsupported_images() {
        let store = store_with(&[
            ("photo.jpg", b"\xff\xd8\xff"),
            ("code.txt", b"full content"),
        ]);
        assert!(
            store
                .prepare("scope", "", false)
                .unwrap_err()
                .contains("不支持")
        );
        let sent = store.prepare("scope", "", true).unwrap();
        let blocks = sent.content_blocks.unwrap();
        assert_eq!(blocks[1]["data"], "/9j/");
        assert_eq!(blocks[1]["mimeType"], "image/jpeg");
        assert_eq!(blocks[0]["text"], sent.text);
        assert!(sent.text.contains("full content"));
        assert!(store.prepare("scope", "", false).is_err());
    }

    #[test]
    fn binary_and_oversized_text_are_blocked_without_silent_truncation() {
        let pdf = store_with(&[("document.pdf", b"%PDF-1.0 ascii is still a PDF")]);
        assert!(
            pdf.prepare("scope", "caption", true)
                .unwrap_err()
                .contains("PDF")
        );
        let text = "中".repeat(MAX_TEXT_BYTES / 3 + 1);
        let store = store_with(&[("long.txt", text.as_bytes())]);
        assert!(store.scopes["scope"].files[0].truncated);
        assert!(
            store
                .prepare("scope", "", true)
                .unwrap_err()
                .contains("32 KiB")
        );
        assert_eq!(store.scopes["scope"].files[0].bytes, text.as_bytes());
    }

    #[test]
    fn acknowledgement_only_removes_the_captured_ids_in_the_same_scope() {
        let mut store = store_with(&[("first.txt", b"first")]);
        let snapshot = store.prepare("scope", "", false).unwrap().snapshot;
        let mut later = local_attachment("later.txt".into(), b"later");
        later.id = 99;
        store
            .scopes
            .get_mut("scope")
            .unwrap()
            .files
            .push(later.clone());
        store.scopes.insert(
            "other".into(),
            AttachmentScopeState {
                files: vec![later],
                ..Default::default()
            },
        );
        store.acknowledge("scope", &snapshot);
        assert_eq!(
            store.snapshot("scope").files,
            vec![(99, "later.txt".into())]
        );
        assert_eq!(store.snapshot("other").files.len(), 1);
        store.acknowledge("scope", &snapshot);
        assert_eq!(store.snapshot("scope").files.len(), 1);
    }

    #[test]
    fn limits_decode_failure_and_loading_fail_before_submission() {
        let mut bytes = vec![0; MAX_IMAGE_BYTES + 1];
        bytes[..3].copy_from_slice(b"\xff\xd8\xff");
        assert!(
            store_with(&[("big.jpg", &bytes)])
                .prepare("scope", "", true)
                .is_err()
        );
        let mut store = store_with(&[("bad.jpg", b"\xff\xd8\xff")]);
        store.scopes.get_mut("scope").unwrap().files[0].send_error = Some("无法解码".into());
        assert!(
            store
                .prepare("scope", "", true)
                .unwrap_err()
                .contains("无法解码")
        );
        store.scopes.get_mut("scope").unwrap().loading = true;
        assert!(
            store
                .prepare("scope", "", true)
                .unwrap_err()
                .contains("仍在读取")
        );
        assert!(!clean_filename("a\n/b\\c.txt").contains(['\n', '/', '\\']));
        let mut partial = store_with(&[("accepted.txt", b"text")]);
        partial.scopes.get_mut("scope").unwrap().error = "another file failed to read".into();
        assert!(partial.snapshot("scope").failed);
        assert!(
            partial.prepare("scope", "caption", true).is_err(),
            "never send a partial selection without acknowledgement"
        );
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum AttachmentPresentation {
    All,
    Button,
    Files,
}
impl AttachmentPresentation {
    fn from_prop(value: &str) -> Self {
        match value {
            "button" => Self::Button,
            "files" => Self::Files,
            _ => Self::All,
        }
    }
}

#[derive(Clone, PartialEq, Default)]
struct AttachmentScopeState {
    files: Vec<LocalAttachment>,
    loading: bool,
    error: String,
}
#[derive(Clone, PartialEq)]
struct LocalAttachment {
    id: u64,
    name: String,
    size: usize,
    kind: String,
    /// Original bytes stay local and are never placed in the metadata snapshot.
    bytes: Vec<u8>,
    /// Optional browser-generated JPEG send copy: (mime, standard base64).
    send_data: Option<(String, String)>,
    send_error: Option<String>,
    image: Option<String>,
    text: Option<String>,
    truncated: bool,
}

pub fn use_attachment_store() -> Signal<AttachmentStore> {
    use_signal(AttachmentStore::default)
}

fn clipped(text: &str, maximum: usize) -> (&str, bool) {
    let mut end = text.len().min(maximum);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    (&text[..end], end < text.len())
}
fn clean_filename(name: &str) -> String {
    let mut cleaned = name
        .chars()
        .map(|character| {
            if character.is_control() || matches!(character, '\u{2028}' | '\u{2029}' | '/' | '\\') {
                ' '
            } else {
                character
            }
        })
        .collect::<String>();
    cleaned = cleaned.trim().chars().take(160).collect();
    if cleaned.is_empty() {
        "未命名附件".into()
    } else {
        cleaned
    }
}

fn binary_document(name: &str, bytes: &[u8]) -> bool {
    bytes.starts_with(b"%PDF-")
        || matches!(
            name.rsplit('.')
                .next()
                .unwrap_or("")
                .to_ascii_lowercase()
                .as_str(),
            "pdf"
                | "doc"
                | "docx"
                | "xls"
                | "xlsx"
                | "ppt"
                | "pptx"
                | "zip"
                | "rar"
                | "7z"
                | "exe"
                | "dll"
                | "wasm"
        )
}

fn text_attachment(bytes: &[u8]) -> Option<&str> {
    std::str::from_utf8(bytes).ok().filter(|text| {
        !text
            .chars()
            .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
    })
}

fn append_attachment_text(target: &mut String, segment: &str) {
    if !target.is_empty() {
        target.push('\n');
    }
    target.push_str(segment);
}

fn size_label(size: usize) -> String {
    let size = f64::from(u32::try_from(size).expect("attachment sizes are bounded to 10 MiB"));
    if size >= 1024.0 * 1024.0 {
        format!("{:.1} MiB", size / (1024.0 * 1024.0))
    } else {
        format!("{:.1} KiB", size / 1024.0)
    }
}
fn raster_mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        Some("image/webp")
    } else {
        None
    }
}
fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let a = chunk[0];
        let b = chunk.get(1).copied().unwrap_or_default();
        let c = chunk.get(2).copied().unwrap_or_default();
        output.push(TABLE[(a >> 2) as usize] as char);
        output.push(TABLE[(((a & 3) << 4) | (b >> 4)) as usize] as char);
        output.push(if chunk.len() > 1 {
            TABLE[(((b & 15) << 2) | (c >> 6)) as usize] as char
        } else {
            '='
        });
        output.push(if chunk.len() > 2 {
            TABLE[(c & 63) as usize] as char
        } else {
            '='
        });
    }
    output
}
fn local_attachment(name: String, bytes: &[u8]) -> LocalAttachment {
    let name = clean_filename(&name);
    let mime = raster_mime(bytes);
    // Only signature-checked raster data becomes an img src. SVG/HTML is never executed.
    let image = mime.map(|mime| format!("data:{mime};base64,{}", base64(bytes)));
    // Extension names are only a hint: UTF-8 text without binary controls is safe
    // to show in <pre>. HTML/SVG are never inserted as markup or used as a URL.
    let extension = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    let text = if image.is_none() {
        text_attachment(bytes)
    } else {
        None
    };
    let (text, truncated) = text.map_or((None, false), |text| {
        let (text, cut) = clipped(text, MAX_QUOTE);
        (Some(text.to_owned()), cut)
    });
    LocalAttachment {
        id: 0,
        name,
        size: bytes.len(),
        bytes: bytes.to_vec(),
        send_data: None,
        send_error: None,
        kind: mime.map_or_else(
            || {
                if text.is_some() {
                    "UTF-8 文本".into()
                } else if extension == "pdf" {
                    "PDF 文档".into()
                } else {
                    "二进制文件".into()
                }
            },
            str::to_owned,
        ),
        image,
        text,
        truncated,
    }
}

fn attachment_label(file: &LocalAttachment, images_supported: bool) -> &'static str {
    if file.send_error.is_some() {
        "图片读取失败，无法发送"
    } else if binary_document(&file.name, &file.bytes) {
        "暂不支持发送此文件类型"
    } else if file.image.is_some() {
        if !images_supported {
            "节点不支持图片发送"
        } else if !matches!(raster_mime(&file.bytes), Some("image/png" | "image/jpeg")) {
            "仅预览，需转换为 PNG/JPEG"
        } else if file.send_data.is_some() {
            "待发送压缩图片，原文件不上传"
        } else {
            "待发送图片"
        }
    } else if file.text.is_some() {
        if file.bytes.len() > MAX_TEXT_BYTES {
            "内容过大，暂不能发送"
        } else {
            "待发送完整文本"
        }
    } else {
        "仅预览，暂不支持发送"
    }
}

// Only JSON-quoted raster bytes enter this fixed script; never a file path, URL
// or source text. Bound the send copy independently of the local preview limit.
async fn prepare_image(file: &mut LocalAttachment) -> Result<(), String> {
    let source = serde_json::to_string(file.image.as_deref().unwrap_or_default()).unwrap();
    let script = format!(
        r"
(async () => {{
  let image;
  try {{
    const src = {source};
    // Inspect bounded headers before allowing the browser to allocate pixels.
    const bytes = Uint8Array.from(atob(src.slice(src.indexOf(',') + 1)), c => c.charCodeAt(0));
    const view = new DataView(bytes.buffer);
    let width = 0, height = 0;
    if (bytes[0] === 137) {{
      if (bytes.length < 33 || view.getUint32(8) !== 13 || view.getUint32(12) !== 0x49484452) throw new Error('png-header');
      width = view.getUint32(16); height = view.getUint32(20);
    }} else {{
      let offset = 2;
      while (offset + 3 < bytes.length) {{
        if (bytes[offset++] !== 255) throw new Error('jpeg-marker');
        while (bytes[offset] === 255) offset++;
        const marker = bytes[offset++];
        if (marker === 217 || marker === 218) break;
        if (marker === 1 || marker >= 208 && marker <= 215) continue;
        if (offset + 2 > bytes.length) throw new Error('jpeg-length');
        const length = view.getUint16(offset);
        if (length < 2 || offset + length > bytes.length) throw new Error('jpeg-length');
        if ([192,193,194,195,197,198,199,201,202,203,205,206,207].includes(marker)) {{
          if (length < 8) throw new Error('jpeg-frame');
          height = view.getUint16(offset + 3); width = view.getUint16(offset + 5); break;
        }}
        offset += length;
      }}
    }}
    if (!width || !height || Math.max(width, height) > 32768 || width * height > 40000000) throw new Error('dimensions');
    image = new Image(); image.src = src;
    await image.decode();
    const w = image.naturalWidth, h = image.naturalHeight;
    if (!w || !h || w * h > 40000000) throw new Error('dimensions');
    if (w <= 2000 && h <= 2000 && src.length <= 170000) {{
      dioxus.send({{ok:true, original:true}}); return;
    }}
    let scale = Math.min(1, 1600 / Math.max(w, h));
    const canvas = document.createElement('canvas');
    const context = canvas.getContext('2d', {{alpha:false}});
    if (!context) throw new Error('canvas');
    for (let attempt = 0; attempt < 7; attempt++) {{
      canvas.width = Math.max(1, Math.round(w * scale));
      canvas.height = Math.max(1, Math.round(h * scale));
      context.fillStyle = '#fff'; context.fillRect(0, 0, canvas.width, canvas.height);
      context.drawImage(image, 0, 0, canvas.width, canvas.height);
      const result = canvas.toDataURL('image/jpeg', Math.max(.4, .86 - attempt * .07));
      if (result.length < 174000) {{
        canvas.width = canvas.height = 0;
        dioxus.send({{ok:true, data:result.slice(result.indexOf(',') + 1)}}); return;
      }}
      scale *= .8;
    }}
    throw new Error('budget');
  }} catch (_) {{ dioxus.send({{ok:false}}); }}
  finally {{ if (image) image.src = ''; }}
}})();"
    );
    let result = document::eval(&script)
        .recv::<Value>()
        .await
        .map_err(|_| "无法准备图片发送副本；请重新选择。".to_owned())?;
    if result["ok"] != true {
        return Err("图片无法解码、像素过大或压缩失败；请换用有效的 PNG/JPEG 图片。".into());
    }
    if result["original"] == true {
        return Ok(());
    }
    let data = result["data"]
        .as_str()
        .filter(|data| !data.is_empty() && data.len() <= SEND_IMAGE_TARGET.div_ceil(3) * 4)
        .ok_or_else(|| "压缩图片超过发送限制；未发送。".to_owned())?
        .to_owned();
    file.image = Some(format!("data:image/jpeg;base64,{data}"));
    file.send_data = Some(("image/jpeg".into(), data));
    Ok(())
}

#[component]
pub fn AttachmentPicker(
    scope: String,
    store: Signal<AttachmentStore>,
    onquote: EventHandler<String>,
    #[props(default = "all".to_owned())] presentation: String,
    #[props(default)] images_supported: bool,
    #[props(default)] disabled: bool,
    #[props(default)] onmore: Option<EventHandler<()>>,
) -> Element {
    // Dioxus reconciles keys on iterator children. A solitary component's key
    // does not give its hooks a new lifetime when its scope prop changes.
    rsx! {
        for attachment_scope in [scope] {
            AttachmentScope {
                key: "{attachment_scope}:{presentation}", scope: attachment_scope, store,
                onquote, presentation: presentation.clone(), images_supported, disabled, onmore,
            }
        }
    }
}

#[component]
fn AttachmentScope(
    scope: String,
    mut store: Signal<AttachmentStore>,
    onquote: EventHandler<String>,
    presentation: String,
    images_supported: bool,
    disabled: bool,
    onmore: Option<EventHandler<()>>,
) -> Element {
    let presentation = AttachmentPresentation::from_prop(&presentation);
    rsx! {
        if matches!(presentation, AttachmentPresentation::All | AttachmentPresentation::Button) {
            AttachmentReader { scope: scope.clone(), store, images_supported, disabled, onmore }
        }
        if matches!(presentation, AttachmentPresentation::All | AttachmentPresentation::Files) {
            AttachmentFiles { scope, store, onquote, images_supported, disabled }
        }
    }
}

#[component]
fn AttachmentReader(
    scope: String,
    mut store: Signal<AttachmentStore>,
    images_supported: bool,
    disabled: bool,
    onmore: Option<EventHandler<()>>,
) -> Element {
    let instance = use_hook(|| NEXT_ATTACHMENT_READER.fetch_add(1, Ordering::Relaxed));
    let input_id = format!("rpi-attachment-input-{instance}");
    let input_id_json = serde_json::to_string(&input_id).unwrap();
    let alive = use_hook(|| Rc::new(Cell::new(true)));
    let pending = use_hook(|| Rc::new(RefCell::new(None::<Task>)));
    let drop_alive = alive.clone();
    let drop_pending = pending.clone();
    let cleanup_scope = scope.clone();
    let paste_cleanup_id = input_id_json.clone();
    let mut cleanup_store = store;
    use_drop(move || {
        drop_alive.set(false);
        if let Some(task) = drop_pending.borrow_mut().take() {
            task.cancel();
        }
        if let Some(state) = cleanup_store.write().scopes.get_mut(&cleanup_scope) {
            if state.loading {
                state.error = "附件读取已中断；请重新选择未完成的文件，或明确放弃。".into();
            }
            state.loading = false;
        }
        let script = format!(
            "window.__rpiAttachmentPasteBindings?.get({paste_cleanup_id})?.cleanup?.();window.__rpiAttachmentPasteBindings?.delete({paste_cleanup_id});"
        );
        spawn(async move {
            let _ = document::eval(&script).await;
        });
    });
    let state = store.read().scopes.get(&scope).cloned().unwrap_or_default();
    let read_scope = scope.clone();
    let paste_input_id = input_id_json.clone();
    rsx! {
        div { class: "composer-extras attachment-picker attachment-button-slot",
            if let Some(onmore) = onmore {
                button { class: "extras-add icon-button", r#type: "button", title: "附件与更多", aria_label: "附件与更多", aria_haspopup: "dialog", onclick: move |_| onmore.call(()),
                    ExtraIcon { kind: "plus" }
                }
            }
            label { class: if onmore.is_some() { "attachment-menu-input" } else { "extras-add icon-button" }, title: if images_supported { "添加待发送图片或文本附件" } else { "添加文本附件；当前节点不支持图片发送" }, aria_label: "添加本地附件",
                ExtraIcon { kind: "plus" }
                input { id: input_id, r#type: "file", multiple: true, disabled: state.loading || disabled, aria_label: "选择本地附件，可多选",
                    onmounted: move |_| {
                        let input_id = paste_input_id.clone();
                        spawn(async move {
                            let script = format!(r#"(() => {{
                                const input = document.getElementById({input_id});
                                if (!input) return false;
                                const bindings = window.__rpiAttachmentPasteBindings ||= new Map();
                                if (bindings.has({input_id})) return true;
                                const installFiles = files => {{
                                    if (!files.length || input.disabled) return false;
                                    try {{
                                        const transfer = new DataTransfer();
                                        files.forEach(file => transfer.items.add(file));
                                        input.files = transfer.files;
                                        input.dispatchEvent(new Event('change', {{bubbles: true}}));
                                        return true;
                                    }} catch (_) {{ return false; }}
                                }};
                                const directFiles = clipboard => {{
                                    const files = [
                                        ...Array.from(clipboard?.files || []),
                                        ...Array.from(clipboard?.items || [])
                                            .filter(item => item.kind === 'file')
                                            .map(item => item.getAsFile())
                                            .filter(Boolean),
                                    ];
                                    const seen = new Set();
                                    return files.filter(file => {{
                                        const key = `${{file.name}}:${{file.size}}:${{file.type}}:${{file.lastModified}}`;
                                        if (seen.has(key)) return false;
                                        seen.add(key);
                                        return true;
                                    }});
                                }};
                                const clipboardItemName = type => {{
                                    const extension = type.split('/')[1]?.split('+')[0] || 'bin';
                                    return `粘贴附件.${{extension === 'jpeg' ? 'jpg' : extension}}`;
                                }};
                                const readNavigatorClipboard = async () => {{
                                    if (!navigator.clipboard?.read) return [];
                                    const files = [];
                                    for (const item of await navigator.clipboard.read()) {{
                                        for (const type of item.types || []) {{
                                            if (type.startsWith('text/')) continue;
                                            const blob = await item.getType(type);
                                            files.push(new File([blob], clipboardItemName(type), {{type}}));
                                        }}
                                    }}
                                    return files;
                                }};
                                const handler = event => {{
                                    // Do not use `instanceof HTMLTextAreaElement` here: native
                                    // WebViews can deliver the event target from another DOM realm.
                                    const target = event.target;
                                    if (!target || target.getAttribute?.('aria-label') !== '发送给 agent 的消息') return;
                                    if (input.disabled) return;
                                    const files = directFiles(event.clipboardData);
                                    const clipboardTypes = Array.from(event.clipboardData?.types || []);
                                    const mayContainFile = files.length > 0 || clipboardTypes.includes('Files') || clipboardTypes.some(type => type.startsWith('image/'));
                                    if (!mayContainFile) return;
                                    event.preventDefault();
                                    if (installFiles(files)) return;
                                    // Desktop WebViews may expose only MIME types on the paste
                                    // event. Read the native clipboard as a fallback instead of
                                    // silently dropping the image/file.
                                    readNavigatorClipboard().then(installFiles).catch(() => {{}});
                                }};
                                document.addEventListener('paste', handler, true);
                                bindings.set({input_id}, {{ cleanup: () => document.removeEventListener('paste', handler, true) }});
                                return true;
                            }})()"#);
                            let _ = document::eval(&script).await;
                        });
                    },
                    onclick: move |_| {
                        spawn(async { let _ = document::eval("document.querySelectorAll('.attachment-picker input[type=file]').forEach(input => input.value = '');").await; });
                    },
                    onchange: move |event| {
                        let chosen = event.files();
                        if disabled || chosen.is_empty() || store.peek().scopes.get(&read_scope).is_some_and(|state| state.loading) { return; }
                        if let Some(state) = store.write().scopes.get_mut(&read_scope) { state.loading = true; state.error.clear(); }
                        else { store.write().scopes.insert(read_scope.clone(), AttachmentScopeState { loading: true, ..Default::default() }); }
                        let scope = read_scope.clone(); let alive = alive.clone(); let mut shared_store = store;
                        let task = spawn(async move {
                            let mut errors = Vec::new();
                            for file in chosen {
                                if !alive.get() { return; }
                                let name = file.name();
                                let declared = file.size();
                                let (count, total) = shared_store.peek().scopes.get(&scope).map_or((0, 0), |state| (state.files.len(), state.files.iter().map(|item| item.size).sum::<usize>()));
                                if count >= MAX_FILES || declared > MAX_FILE as u64 || declared.saturating_add(total as u64) > MAX_TOTAL as u64 {
                                    errors.push(format!("{name}：超出附件限制（8 个 / 单个 5 MiB / 合计 10 MiB）")); continue;
                                }
                                match file.read_bytes().await {
                                    Ok(bytes) => {
                                        if !alive.get() { return; }
                                        let mut attachment = local_attachment(name.clone(), &bytes);
                                        if matches!(raster_mime(&bytes), Some("image/png" | "image/jpeg")) {
                                            if let Err(error) = prepare_image(&mut attachment).await { attachment.image = None; attachment.send_error = Some(error); }
                                        }
                                        if !alive.get() { return; }
                                        let mut state = shared_store.write();
                                        let exceeds_limits = state.scopes.get(&scope).is_some_and(|scope_state| {
                                            scope_state.files.len() >= MAX_FILES
                                                || bytes.len() > MAX_FILE
                                                || scope_state.files.iter().map(|item| item.size).sum::<usize>() + bytes.len() > MAX_TOTAL
                                        });
                                        if exceeds_limits { errors.push(format!("{name}：读取后大小超出限制")); continue; }
                                        state.next_id = state.next_id.wrapping_add(1);
                                        attachment.id = state.next_id;
                                        state.scopes.entry(scope.clone()).or_default().files.push(attachment);
                                    }
                                    Err(problem) => { if !alive.get() { return; } errors.push(format!("{name}：读取失败，{problem}")); }
                                }
                            }
                            if alive.get() { if let Some(state) = shared_store.write().scopes.get_mut(&scope) { state.loading = false; state.error = errors.join("；"); } }
                        });
                        *pending.borrow_mut() = Some(task);
                    }
                }
            }
        }
    }
}

#[component]
fn AttachmentFiles(
    scope: String,
    mut store: Signal<AttachmentStore>,
    onquote: EventHandler<String>,
    images_supported: bool,
    disabled: bool,
) -> Element {
    let mut preview = use_signal(|| None::<u64>);
    let state = store.read().scopes.get(&scope).cloned().unwrap_or_default();
    let files = state.files.clone();
    let selected = files
        .iter()
        .find(|file| Some(file.id) == *preview.read())
        .cloned();
    // No files, error, or load status means this slot contributes zero layout height.
    if files.is_empty() && !state.loading && state.error.is_empty() {
        return rsx! {};
    }
    rsx! {
        div { class: "composer-extras attachment-files-slot",
            if state.loading { small { role: "status", "正在读取本地附件…" } }
            if !files.is_empty() {
                div { class: "draft-attachments",
                    for file in files {
                        { let id = file.id; let remove_scope = scope.clone(); rsx! {
                            div { key: "{id}", class: if file.image.is_some() { "draft-chip is-image" } else { "draft-chip" },
                                button { r#type: "button", aria_label: "预览 {file.name}", onclick: move |_| preview.set(Some(id)),
                                    if let Some(src) = file.image.clone() {
                                        { let image_error_scope = scope.clone();
                                        rsx! { img { src, alt: "{file.name} 的本地预览", onerror: move |_| {
                                            if let Some(state) = store.write().scopes.get_mut(&image_error_scope) {
                                                state.error = "图片预览失败；附件仍保留，可移除后重新选择。".into();
                                            }
                                        } } } }
                                    } else { ExtraIcon { kind: "file" } }
                                    span { title: "{file.name}", span { "{file.name}" } small { "{size_label(file.size)} · {attachment_label(&file, images_supported)}" } }
                                }
                                button { r#type: "button", class: "attachment-remove", disabled, aria_label: "移除 {file.name}", onclick: move |_| { if disabled { return; }
                                    if let Some(state) = store.write().scopes.get_mut(&remove_scope) { state.files.retain(|file| file.id != id); }
                                    if *preview.peek() == Some(id) { preview.set(None); } }, ExtraIcon { kind: "close" } }
                            }
                        } }
                    }
                }
                small { class: "extras-local-note", "待发送 · 点击卡片预览、× 移除。文本发送完整内容；图片发送预览中的视觉副本，原文件不会上传。" }
                if !images_supported && state.files.iter().any(|file| file.image.is_some()) {
                    p { class: "extras-error", role: "alert", "当前节点尚未确认图片输入能力；不会只发送文字或自动漏掉图片。" }
                }
            }
            if !state.error.is_empty() {
                p { class: "extras-error", role: "alert", "{state.error}；有错误时不会发送。" }
                button { r#type: "button", class: "button", disabled, onclick: move |_| {
                    if !disabled { if let Some(state) = store.write().scopes.get_mut(&scope) { state.error.clear(); } }
                }, "清除提示，仅保留已加入的附件" }
            }
            if let Some(file) = selected {
                ExtrasDialog { title: file.name.clone(), onclose: move |()| preview.set(None),
                    p { class: "extras-help", "{attachment_label(&file, images_supported)} · {size_label(file.size)}" }
                    if let Some(src) = file.image { RasterPreview { src, name: file.name.clone() } }
                    else if let Some(text) = file.text {
                        pre { class: "document-preview", "{text}" }
                        if file.truncated { p { class: "extras-help", "文本仅预览和引用前 32 KiB，已按 UTF-8 字符边界截断。" } }
                        button { r#type: "button", class: "button primary", onclick: move |_| { onquote.call(format!("[本地文本引用：{}{}]\n{}", file.name, if file.truncated { "；前 32 KiB，已截断" } else { "" }, text)); preview.set(None); }, "引用文本到草稿" }
                    } else { div { class: "extras-binary", ExtraIcon { kind: "file" } p { "{file.kind} · {size_label(file.size)}" } p { "仅展示类型与大小，不解析或执行文件。" } } }
                    p { class: "extras-help", "尚未发送。支持的附件将随主消息发送；超限或不支持的文件会阻止发送并保留。引用只编辑草稿，不会立即发送。" }
                }
            }
        }
    }
}

#[component]
fn RasterPreview(src: String, name: String) -> Element {
    let mut zoom = use_signal(|| 100u16);
    let mut failed = use_signal(|| false);
    rsx! {
        div { class: "image-preview",
            img { src, alt: "{name} 的本地预览", style: "width:{zoom}%;max-width:none;", onerror: move |_| failed.set(true) }
        }
        if *failed.read() { p { role: "alert", class: "extras-error", "图片解码失败；附件仍保留。" } }
        div { class: "image-controls",
            button { r#type: "button", class: "button", onclick: move |_| { let value = (*zoom.peek()).saturating_sub(25).max(25); zoom.set(value); }, "缩小" }
            button { r#type: "button", class: "button", onclick: move |_| zoom.set(100), "适应窗口" }
            button { r#type: "button", class: "button", onclick: move |_| { let value = (*zoom.peek() + 25).min(300); zoom.set(value); }, "放大" }
        }
    }
}

/// Fixed script; receives no filenames, user HTML, selectors or executable content.
const OPEN_DIALOG: &str = r"
const dialog = document.querySelector('dialog.composer-extra-dialog:not([open])');
if (dialog) {
  const previous = document.activeElement;
  dialog.showModal();
  const observer = new MutationObserver(() => {
    if (!dialog.isConnected) restore();
  });
  const restore = () => {
    observer.disconnect();
    if (previous?.isConnected) previous.focus();
  };
  dialog.addEventListener('close', restore, {once:true});
  observer.observe(document.body, {childList:true, subtree:true});
}
";
#[component]
fn ExtrasDialog(title: String, onclose: EventHandler<()>, children: Element) -> Element {
    rsx! {
        dialog { class: "composer-extras composer-extra-dialog", aria_label: "{title}",
            onmounted: move |_| { spawn(async { let _ = document::eval(OPEN_DIALOG).await; }); },
            oncancel: move |event| { event.prevent_default(); onclose.call(()); },
            onkeydown: move |event| { if event.key() == Key::Escape { event.prevent_default(); event.stop_propagation(); onclose.call(()); } },
            header { h2 { "{title}" } button { r#type: "button", class: "icon-button", aria_label: "关闭预览", autofocus: true, onclick: move |_| onclose.call(()), ExtraIcon { kind: "close" } } }
            div { class: "extras-dialog-body", {children} }
        }
    }
}

/// Create this store in the App (or another owner that outlives `BtwPanel`).
/// It retains every scope's real session IDs, public history and unsent drafts.
#[derive(Clone, PartialEq, Default)]
pub struct BtwStore {
    scopes: BTreeMap<String, BtwScope>,
}
impl BtwStore {
    pub fn has_pending_content(&self) -> bool {
        self.scopes
            .values()
            .flat_map(|scope| &scope.threads)
            .any(|thread| {
                !thread.draft.trim().is_empty()
                    || thread.activity.busy
                    || thread.activity.cancelling
                    || thread.activity.operation
            })
    }
}
#[derive(Clone, PartialEq)]
struct BtwScope {
    threads: Vec<BtwThread>,
    active: usize,
}
impl Default for BtwScope {
    fn default() -> Self {
        Self {
            threads: vec![BtwThread::default()],
            active: 0,
        }
    }
}
#[derive(Clone, PartialEq, Default)]
#[allow(clippy::struct_excessive_bools)]
struct BtwThread {
    session_id: Option<String>,
    renamed: bool,
    draft: String,
    rows: Vec<PublicRow>,
    /// Cached projection row ID -> block slot in the current uninterrupted projection.
    /// Rebuilt snapshots invalidate slots, but retain ID provenance for safe cleanup.
    projection_rows: BTreeMap<String, Option<usize>>,
    /// Completed public owners still need their slot evidence at settlement.
    projection_message_owners: BTreeMap<usize, String>,
    tools: Vec<String>,
    /// Only an explicit empty activeTools readback can confirm this boundary.
    tools_confirmed_empty: bool,
    tools_initialized: bool,
    activity: BtwActivity,
    connected: bool,
    error: String,
    status: String,
    models: Vec<ModelDescriptor>,
    levels: Vec<String>,
    model: String,
    thinking: String,
    usage: String,
}
#[derive(Clone, PartialEq, Default)]
struct BtwActivity {
    busy: bool,
    cancelling: bool,
    operation: bool,
}

#[derive(Clone, PartialEq)]
struct PublicRow {
    id: String,
    role: String,
    text: String,
}

pub fn use_btw_store() -> Signal<BtwStore> {
    use_signal(BtwStore::default)
}

/// Used by explicit `/btw` command routing. It only edits the independent
/// BTW draft; it never sends a message or mutates the main draft.
pub fn set_btw_draft(mut store: Signal<BtwStore>, scope: &str, text: String) {
    let mut state = store.write();
    let scope_state = state.scopes.entry(scope.to_owned()).or_default();
    if let Some(thread) = scope_state.threads.get_mut(scope_state.active) {
        thread.draft = text;
    }
}

#[derive(Clone)]
struct BtwRuntime {
    store: Signal<BtwStore>,
    scope: String,
    index: usize,
    alive: Rc<Cell<bool>>,
    tasks: Rc<RefCell<Vec<Task>>>,
    stream: Rc<RefCell<Option<Task>>>,
    stream_generation: Rc<Cell<u64>>,
}
impl BtwRuntime {
    fn update(&self, change: impl FnOnce(&mut BtwThread)) {
        if !self.alive.get() {
            return;
        }
        let mut store = self.store;
        if let Ok(mut state) = store.try_write() {
            if let Some(thread) = state
                .scopes
                .get_mut(&self.scope)
                .and_then(|scope| scope.threads.get_mut(self.index))
            {
                change(thread);
            }
        }
    }
    fn thread(&self) -> BtwThread {
        self.store
            .peek()
            .scopes
            .get(&self.scope)
            .and_then(|scope| scope.threads.get(self.index))
            .cloned()
            .unwrap_or_default()
    }
    fn stop_stream(&self) {
        self.stream_generation
            .set(self.stream_generation.get().wrapping_add(1));
        if let Some(task) = self.stream.borrow_mut().take() {
            task.cancel();
        }
    }
    fn disconnect(&self) {
        self.stop_stream();
        self.update(|thread| {
            thread.connected = false;
            thread.tools_confirmed_empty = false;
            thread.activity.operation = false;
            thread.status = if thread.activity.busy {
                "面板已关闭；节点会话可能仍在运行，重新打开后同步。".into()
            } else {
                "已断开本地订阅；会话保留在节点。".into()
            };
        });
        self.alive.set(false);
        for task in self.tasks.borrow_mut().drain(..) {
            task.cancel();
        }
    }
    fn task(&self, future: impl Future<Output = ()> + 'static) {
        self.tasks.borrow_mut().push(spawn(future));
    }
}

#[component]
pub fn BtwPanel(
    scope: String,
    project_id: Option<String>,
    client: Signal<Option<WorkspaceClient>>,
    mut store: Signal<BtwStore>,
    onquote: EventHandler<String>,
    onclose: EventHandler<()>,
) -> Element {
    // Initialize without subscribing the setup path to subsequent stream updates.
    if !store.peek().scopes.contains_key(&scope) {
        store
            .write()
            .scopes
            .insert(scope.clone(), BtwScope::default());
    }
    let active = store
        .read()
        .scopes
        .get(&scope)
        .map_or(0, |scope| scope.active);
    // Use a keyed iterator so switching scope or thread actually unmounts the
    // old runtime and creates new hooks, event handlers and task ownership.
    rsx! {
        for thread_scope in [scope] {
            BtwThreadPanel {
                key: "{thread_scope}:{active}", scope: thread_scope, index: active,
                project_id: project_id.clone(), client, store, onquote, onclose,
            }
        }
    }
}

#[component]
fn BtwThreadPanel(
    scope: String,
    index: usize,
    project_id: Option<String>,
    client: Signal<Option<WorkspaceClient>>,
    mut store: Signal<BtwStore>,
    onquote: EventHandler<String>,
    onclose: EventHandler<()>,
) -> Element {
    let runtime = use_hook(|| BtwRuntime {
        store,
        scope: scope.clone(),
        index,
        alive: Rc::new(Cell::new(true)),
        tasks: Rc::new(RefCell::new(Vec::new())),
        stream: Rc::new(RefCell::new(None)),
        stream_generation: Rc::new(Cell::new(0)),
    });
    let drop_runtime = runtime.clone();
    use_drop(move || drop_runtime.disconnect());
    let mut reconnect = use_signal(|| 0u64);
    let mut confirm_new = use_signal(|| false);
    let stream_runtime = runtime.clone();
    use_effect(move || {
        let _ = *reconnect.read();
        let client = client.read().clone();
        stream_runtime.stop_stream();
        let runtime = stream_runtime.clone();
        let Some(id) = runtime.thread().session_id else {
            return;
        };
        let Some(client) = client else {
            runtime.update(|thread| {
                thread.connected = false;
                thread.error = "节点未连接；草稿与已接收的回答保留。".into();
            });
            return;
        };
        let generation = runtime.stream_generation.get();
        let slot = runtime.stream.clone();
        let task = spawn(async move {
            let result = client.connect_session(id.clone()).await;
            if !runtime.alive.get() || runtime.stream_generation.get() != generation {
                return;
            }
            match result {
                Ok(mut connection) => {
                    // Reconnects never silently restore a tool-enabled BTW.
                    // Keep this read-only; initialization is performed only by
                    // send_btw and never while a run is busy.
                    let tools_state = client.active_tools_state(&id).await;
                    if !runtime.alive.get() || runtime.stream_generation.get() != generation {
                        return;
                    }
                    match tools_state {
                        Ok(active) if active.is_empty() => runtime.update(|thread| {
                            thread.tools_confirmed_empty = thread.tools_initialized;
                            thread.tools.clear();
                        }),
                        Ok(_) => runtime.update(|thread| {
                            thread.tools_confirmed_empty = false;
                            thread.error =
                                "旁问节点工具权限已改变；请重试以重新设置为空，未发送消息。".into();
                        }),
                        Err(problem) => runtime.update(|thread| {
                            thread.tools_confirmed_empty = false;
                            thread.error = format!("旁问工具权限未确认：{problem}；请重连后重试。");
                        }),
                    }
                    publish_connection(&runtime, &connection, true, false);
                    loop {
                        let event = connection.next().await;
                        if !runtime.alive.get() || runtime.stream_generation.get() != generation {
                            break;
                        }
                        match event {
                            Ok(Some(event)) => {
                                let continuous = !matches!(
                                    event.wire.event_type.as_str(),
                                    "connection_online" | "resync_required"
                                );
                                publish_connection(&runtime, &connection, false, continuous);
                                if event.wire.event_type == "connection_reconnecting" {
                                    runtime.update(|thread| {
                                        thread.status =
                                            "网络断开，正在重新连接；已接收内容保留。".into();
                                    });
                                }
                            }
                            Ok(None) => {
                                runtime.update(|thread| {
                                    thread.connected = false;
                                    thread.status = "订阅已结束，请重新连接确认节点状态。".into();
                                });
                                break;
                            }
                            Err(problem) => {
                                runtime.update(|thread| {
                                    thread.connected = false;
                                    thread.error = format!(
                                        "旁问订阅失败：{problem}；已接收内容保留，请重新连接。"
                                    );
                                });
                                break;
                            }
                        }
                    }
                }
                Err(problem) => runtime.update(|thread| {
                    thread.connected = false;
                    thread.error = format!("连接旁问失败：{problem}；请重连，草稿和历史已保留。");
                }),
            }
        });
        *slot.borrow_mut() = Some(task);
    });
    let view = store
        .read()
        .scopes
        .get(&scope)
        .and_then(|scope| scope.threads.get(index))
        .cloned()
        .unwrap_or_default();
    let histories = store
        .read()
        .scopes
        .get(&scope)
        .map(|scope| {
            scope
                .threads
                .iter()
                .enumerate()
                .map(|(i, thread)| {
                    (
                        i,
                        thread.session_id.clone(),
                        thread
                            .rows
                            .iter()
                            .find(|row| row.role == "user")
                            .map_or_else(
                                || "未发送的旁问".into(),
                                |row| clipped(&row.text, 60).0.to_owned(),
                            ),
                    )
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let send_runtime = runtime.clone();
    let send_project = project_id.clone();
    let send = Rc::new(move || {
        let runtime = send_runtime.clone();
        let state = runtime.thread();
        let message = state.draft.trim().to_owned();
        if message.is_empty() || state.activity.operation || state.activity.cancelling {
            return;
        }
        let Some(client) = client.peek().clone() else {
            runtime.update(|thread| thread.error = "请先连接节点；旁问草稿已保留。".into());
            return;
        };
        // A known session must be synchronized before deciding prompt versus followUp.
        if state.session_id.is_some() && !state.connected {
            runtime.update(|thread| {
                thread.error = "请先重新连接旁问，确认节点运行状态后再发送；草稿已保留。".into();
            });
            return;
        }
        runtime.update(|thread| {
            thread.activity.operation = true;
            thread.error.clear();
        });
        let project = send_project.clone();
        let task_runtime = runtime.clone();
        runtime.task(async move {
            let runtime = task_runtime;
            let result = send_btw(&runtime, &client, project.as_deref(), &message).await;
            if !runtime.alive.get() {
                return;
            }
            runtime.update(|thread| {
                thread.activity.operation = false;
                match result {
                    Ok(()) => {
                        if thread.draft.trim() == message {
                            thread.draft.clear();
                        }
                        thread.error.clear();
                    }
                    Err(problem) => thread.error = problem,
                }
            });
            reconnect += 1;
        });
    });
    let cancel_runtime = runtime.clone();
    let cancel = move |_| {
        let runtime = cancel_runtime.clone();
        let state = runtime.thread();
        if state.activity.operation || state.activity.cancelling {
            return;
        }
        let (Some(client), Some(id)) = (client.peek().clone(), state.session_id) else {
            return;
        };
        runtime.update(|thread| {
            thread.activity.operation = true;
            thread.error.clear();
        });
        // Replace the previous WebSocket subscription and keep observing until real settlement.
        runtime.stop_stream();
        let task_runtime = runtime.clone();
        runtime.task(async move {
            let runtime = task_runtime;
            let result = client.abort(&id).await;
            if !runtime.alive.get() {
                return;
            }
            runtime.update(|thread| {
                thread.activity.operation = false;
                match result {
                    Ok(response) if response.success => {
                        thread.activity.cancelling = true;
                        thread.activity.busy = true;
                        thread.status = "已请求取消，等待节点真实结算…".into();
                    }
                    Ok(response) => {
                        thread.error = format!(
                            "取消失败：{}",
                            response.error.unwrap_or_else(|| "节点拒绝了请求".into())
                        );
                    }
                    Err(problem) => {
                        thread.error =
                            format!("取消请求失败：{problem}；节点可能仍在运行，请重连确认。");
                    }
                }
            });
            reconnect += 1;
        });
    };
    let config_runtime = runtime.clone();
    let change_config = move |kind: &'static str, value: String| {
        let runtime = config_runtime.clone();
        let state = runtime.thread();
        if state.activity.operation || state.activity.busy || value.is_empty() {
            return;
        }
        let (Some(client), Some(id)) = (client.peek().clone(), state.session_id) else {
            return;
        };
        runtime.update(|thread| {
            thread.activity.operation = true;
            thread.error.clear();
        });
        let task_runtime = runtime.clone();
        runtime.task(async move {
            let runtime = task_runtime;
            let result = if kind == "model" {
                client.set_model(&id, &value).await
            } else {
                client.set_thinking_level(&id, &value).await
            };
            if !runtime.alive.get() {
                return;
            }
            runtime.update(|thread| {
                thread.activity.operation = false;
                match result {
                    Ok(_) => {
                        if kind == "model" {
                            thread.model = value;
                        } else {
                            thread.thinking = value;
                        }
                    }
                    Err(problem) => thread.error = format!("配置未更改：{problem}"),
                }
            });
        });
    };
    let models_runtime = runtime.clone();
    let refresh_models = move |_| {
        let runtime = models_runtime.clone();
        let state = runtime.thread();
        if state.activity.operation {
            return;
        }
        let (Some(client), Some(id)) = (client.peek().clone(), state.session_id) else {
            return;
        };
        runtime.update(|thread| {
            thread.activity.operation = true;
            thread.error.clear();
        });
        let task_runtime = runtime.clone();
        runtime.task(async move {
            load_configuration(&task_runtime, &client, &id).await;
            task_runtime.update(|thread| thread.activity.operation = false);
        });
    };
    let draft_runtime = runtime.clone();
    let close_runtime = runtime.clone();
    let history_runtime = runtime.clone();
    let new_runtime = runtime.clone();
    let new_scope = scope.clone();
    let key_close = runtime.clone();
    let change_model = change_config.clone();
    let change_thinking = change_config;
    let usage_label = if view.usage.is_empty() {
        "—".to_owned()
    } else {
        view.usage.clone()
    };
    rsx! {
        aside { id: "btw-panel", class: "composer-extras btw-panel", aria_label: "BTW 独立旁问会话", tabindex: "-1",
            onmounted: move |_| { spawn(async {
                let _ = document::eval(r"
                    const panel = document.getElementById('btw-panel');
                    if (panel) {
                      const previous = document.activeElement;
                      panel.querySelector('textarea')?.focus();
                      const observer = new MutationObserver(() => {
                        if (!panel.isConnected) {
                          observer.disconnect();
                          if (previous?.isConnected) previous.focus();
                        }
                      });
                      observer.observe(document.body, {childList:true, subtree:true});
                    }
                ").await;
            }); },
            onkeydown: move |event| { if event.key() == Key::Escape && !*confirm_new.peek() { event.prevent_default(); key_close.disconnect(); onclose.call(()); } },
            div { class: "btw-header",
                div { strong { "BTW" span { "旁路对话" } } small { "独立旁问会话 · 主任务继续" } }
                button { r#type: "button", class: "icon-button", aria_label: "关闭旁问面板", onclick: move |_| { close_runtime.disconnect(); onclose.call(()); }, ExtraIcon { kind: "close" } }
            }
            div { class: "btw-boundary-summary", ExtraIcon { kind: "shield" } span {
                "独立历史 · 不带入主对话 · "
                if view.tools_confirmed_empty && view.connected { "执行节点工具为空（已确认）" }
                else { "无工具权限未确认 · 确认前不发送" }
            } }
            details { class: "btw-settings",
                summary { "旁问配置" }
                div { class: "btw-config",
                    label { "模型" select { aria_label: "旁问模型", value: "{view.model}", disabled: view.activity.operation || view.activity.busy || !view.connected || view.models.is_empty(), onchange: move |event| change_model("model", event.value()),
                        option { value: "", disabled: !view.model.is_empty(), "节点默认" }
                        for model in &view.models { option { value: "{model.id}", "{model.name} · {model.provider}" } }
                    } }
                    label { "Thinking" select { aria_label: "旁问思考级别", value: "{view.thinking}", disabled: view.activity.operation || view.activity.busy || !view.connected || view.levels.is_empty(), onchange: move |event| change_thinking("thinking", event.value()),
                        option { value: "", disabled: !view.thinking.is_empty(), "节点默认" }
                        for level in &view.levels { option { value: "{level}", "{level}" } }
                    } }
                    if view.session_id.is_some() { button { r#type: "button", class: "small-link", disabled: view.activity.operation || !view.connected, onclick: refresh_models, "读取配置" } }
                    else { small { "首次发送使用节点默认模型与思考配置，创建后可选择。" } }
                }
                div { class: "btw-boundary", ExtraIcon { kind: "shield" } span { "普通独立 session，不会自动读取主对话内容；只有显式引用公开回答才会写入主草稿。执行节点工具为空，并在发送前通过节点状态确认；未确认时不会发送。关闭面板只断开本地订阅，节点任务可能继续运行；这不是原生 BTW 插件加载。" } }
            }
            div { class: "btw-actions",
                button { r#type: "button", onclick: move |_| confirm_new.set(true), "新旁问" }
                button { r#type: "button", disabled: !view.activity.busy || view.activity.operation || view.activity.cancelling || !view.connected, onclick: cancel, "取消旁问" }
                button { r#type: "button", disabled: view.activity.operation, onclick: move |_| reconnect += 1, "重新连接" }
                span { role: "status", if view.activity.cancelling { "等待结算" } else if view.activity.operation { "请求中" } else if view.activity.busy { "运行中" } else if view.connected && view.tools_confirmed_empty { "就绪" } else if view.connected { "权限未确认" } else { "未连接" } }
            }
            details { class: "btw-sessions", summary { "旁问历史 · {histories.len()} 个话题" }
                p { "只列出本 scope 创建的旁问；新旁问保留旧 session，不删除节点会话。" }
                for (thread_index, id, title) in histories {
                    { let history_scope = scope.clone(); let history_runtime = history_runtime.clone(); let session_label = id.as_deref().unwrap_or("尚未创建 session").to_owned(); rsx! {
                        button { r#type: "button", class: if thread_index == index { "active" } else { "" }, disabled: thread_index == index,
                            onclick: move |_| { history_runtime.disconnect(); if let Some(scope) = store.write().scopes.get_mut(&history_scope) { scope.active = thread_index; } },
                            span { "{title}" } small { "{session_label}" }
                        }
                    } }
                }
            }
            if let Some(id) = &view.session_id { p { class: "btw-session-id", "sessionId: {id}" } }
            div { class: "btw-history", aria_live: "polite",
                if view.rows.is_empty() { div { class: "btw-empty", "顺便问个问题。" p { "独立历史、独立取消，不自动写入主会话。" } button { r#type: "button", class: "button", onclick: move |_| draft_runtime.update(|thread| thread.draft = "为什么把技能与工具分开显示？".into()), "试问这个问题" } } }
                for row in view.rows {
                    article { key: "{row.id}", class: "btw-turn",
                        div { class: if row.role == "user" { "btw-question" } else { "btw-answer" }, "{row.text}" }
                        if row.role == "assistant" { div { class: "btw-turn-meta", "公开回答" button { r#type: "button", onclick: move |_| onquote.call(format!("[引用独立旁问]\n{}", row.text)), "引用到主输入框" } } }
                    }
                }
                for tool in view.tools { p { class: "btw-tool-status", "{tool}" } }
            }
            if !view.error.is_empty() { div { class: "extras-error", role: "alert", "{view.error}" } }
            if !view.status.is_empty() { p { class: "btw-status", role: "status", "{view.status}" } }
            div { class: "btw-composer",
                textarea { rows: 2, aria_label: "旁问消息", placeholder: "旁问，不打断主任务…", value: "{view.draft}",
                    oninput: { let runtime = runtime.clone(); move |event| runtime.update(|thread| thread.draft = event.value()) },
                    onkeydown: { let send = send.clone(); move |event| { if event.key() == Key::Enter && !event.modifiers().shift() && !event.is_composing() { event.prevent_default(); send(); } } }
                }
                div { small { "Enter 发送 · Shift+Enter 换行 · 用量 {usage_label}" }
                    button { r#type: "button", class: "send-button", aria_label: if view.activity.busy { "排队后续旁问" } else { "发送旁问" }, disabled: view.activity.operation || view.activity.cancelling || view.draft.trim().is_empty(), onclick: { let send = send.clone(); move |_| send() }, ExtraIcon { kind: "arrow" } }
                }
                if view.activity.busy { small { "运行期间发送将加入 followUp 后续消息队列。取消会中止当前运行，已接收的排队消息仍以节点队列状态为准。" } }
            }
            if *confirm_new.read() {
                ExtrasDialog { title: "开始新旁问".to_owned(), onclose: move |()| confirm_new.set(false),
                    p { "将切换到空白旁问。当前草稿、公开历史和真实 sessionId 保留在“旁问历史”中，不删除节点会话。当前节点任务可能继续运行；如需停止，请先取消并等待结算。" }
                    button { r#type: "button", class: "button primary", onclick: move |_| {
                        new_runtime.disconnect();
                        if let Some(scope) = store.write().scopes.get_mut(&new_scope) { scope.threads.push(BtwThread::default()); scope.active = scope.threads.len() - 1; }
                    }, "保留旧会话并开始新旁问" }
                    button { r#type: "button", class: "button", onclick: move |_| confirm_new.set(false), "返回" }
                }
            }
        }
    }
}

fn check_btw_operation(runtime: &BtwRuntime, generation: u64) -> Result<(), String> {
    if !runtime.alive.get() || runtime.stream_generation.get() != generation {
        return Err("旁问连接已改变；消息未继续发送，请重连核对后重试。".into());
    }
    Ok(())
}

async fn confirm_btw_tools(
    runtime: &BtwRuntime,
    client: &WorkspaceClient,
    id: &str,
    generation: u64,
) -> Result<(), String> {
    // BTW is deliberately a normal session, so enforce its no-tool boundary
    // through the node RPC before either prompt or followUp. A successful ACK
    // is not enough: the explicit get_state field must be present and empty.
    let state = runtime.thread();
    let needs_initialization = !state.tools_initialized || !state.tools_confirmed_empty;
    let busy = state.activity.busy;
    if busy && !state.tools_initialized {
        return Err("运行中的旁问尚未完成无工具初始化；未发送消息，请等待结算后重试。".into());
    }
    // A busy followUp only reads the tool state; it never reconfigures a run.
    if needs_initialization && !busy {
        client.set_active_tools(id, &[]).await.map_err(|problem| {
            format!("旁问工具权限未设置为空：{problem}；请重试或重连，草稿已保留。")
        })?;
        check_btw_operation(runtime, generation)?;
    }
    let active = client.active_tools_state(id).await.map_err(|problem| {
        runtime.update(|thread| thread.tools_confirmed_empty = false);
        format!("旁问工具权限未确认：{problem}；未发送消息，请重试或重连。")
    })?;
    check_btw_operation(runtime, generation)?;
    if !active.is_empty() {
        runtime.update(|thread| thread.tools_confirmed_empty = false);
        return Err("旁问工具权限未确认为空；未发送消息，请重试或重连。".into());
    }
    runtime.update(|thread| {
        thread.tools.clear();
        thread.tools_confirmed_empty = true;
        thread.tools_initialized = true;
        thread.status = "旁问已确认无工具权限，准备发送…".into();
    });
    Ok(())
}

async fn send_btw(
    runtime: &BtwRuntime,
    client: &WorkspaceClient,
    project: Option<&str>,
    message: &str,
) -> Result<(), String> {
    let generation = runtime.stream_generation.get();
    check_btw_operation(runtime, generation)?;
    let state = runtime.thread();
    let requires_connection = state.session_id.is_some();
    let id = if let Some(id) = state.session_id {
        id
    } else {
        let id = client
            .create_session(project)
            .await
            .map_err(|problem| format!("创建旁问失败：{problem}；草稿已保留。"))?;
        check_btw_operation(runtime, generation)?;
        runtime.update(|thread| thread.session_id = Some(id.clone()));
        id
    };
    check_btw_operation(runtime, generation)?;
    if !runtime.thread().renamed {
        let title = format!("BTW · {}", clipped(message, 96).0);
        client
            .rename_session(&id, &title)
            .await
            .map_err(|problem| {
                format!("旁问已创建（{id}），命名失败：{problem}；草稿保留，重试会复用此会话。")
            })?;
        check_btw_operation(runtime, generation)?;
        runtime.update(|thread| thread.renamed = true);
    }
    confirm_btw_tools(runtime, client, &id, generation).await?;
    check_btw_operation(runtime, generation)?;
    if requires_connection && !runtime.thread().connected {
        runtime.update(|thread| thread.tools_confirmed_empty = false);
        return Err("旁问连接已断开；未发送消息，请重新连接后重试，草稿已保留。".into());
    }
    let queued = runtime.thread().activity.busy;
    runtime.update(|thread| {
        // A request is not an accepted run. In particular, a success:false
        // prompt must not make the next explicit retry a queued followUp.
        thread.status = if queued {
            "正在加入后续消息队列…".into()
        } else {
            "正在发送独立旁问…".into()
        };
    });
    let result = if queued {
        client
            .queue_message(&id, message, "followUp")
            .await
            .map(|_| ())
            .map_err(|problem| problem.to_string())
    } else {
        client
            .prompt(&id, message)
            .await
            .map_err(|problem| problem.to_string())
            .and_then(|response| {
                if response.success {
                    Ok(())
                } else {
                    Err(response.error.unwrap_or_else(|| "节点拒绝了旁问".into()))
                }
            })
    };
    check_btw_operation(runtime, generation)?;
    match result {
        Ok(()) => {
            runtime.update(|thread| {
                thread.activity.busy = true;
                // Optimistic public question persists if a later snapshot/network request fails.
                let id = format!("accepted-local-{}", thread.rows.len());
                thread.rows.push(PublicRow {
                    id,
                    role: "user".into(),
                    text: message.to_owned(),
                });
                thread.status = if queued {
                    "后续消息已被节点接收；等待当前运行收尾。".into()
                } else {
                    "旁问已接收，正在同步公开回答…".into()
                };
            });
            Ok(())
        }
        Err(problem) => Err(format!(
            "旁问发送未确认：{problem}；草稿已保留，请重连核对历史后再重试。"
        )),
    }
}

async fn load_configuration(runtime: &BtwRuntime, client: &WorkspaceClient, id: &str) {
    let models = client.models(id).await;
    if !runtime.alive.get() {
        return;
    }
    let levels = client.thinking_levels(id).await;
    if !runtime.alive.get() {
        return;
    }
    let state = client.runtime_state(id).await;
    if !runtime.alive.get() {
        return;
    }
    runtime.update(|thread| {
        let mut errors = Vec::new();
        match models {
            Ok(models) => thread.models = models,
            Err(problem) => errors.push(format!("模型：{problem}")),
        }
        match levels {
            Ok(levels) => thread.levels = levels,
            Err(problem) => errors.push(format!("思考：{problem}")),
        }
        match state {
            Ok(state) => {
                thread.model = state.model_id.unwrap_or_default();
                thread.thinking = state.thinking_level.unwrap_or_default();
            }
            Err(problem) => errors.push(format!("配置：{problem}")),
        }
        thread.error = errors.join("；");
    });
}

fn public_content(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Array(items) => items
            .iter()
            .filter(|item| item.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|item| item.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    }
}
fn connection_rows(connection: &SessionConnection) -> Vec<PublicRow> {
    let mut rows = Vec::new();
    for (index, message) in connection.messages().iter().enumerate() {
        let role = message
            .get("role")
            .or_else(|| message.get("kind"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !matches!(role, "user" | "assistant") {
            continue;
        }
        let text = message
            .get("content")
            .map(public_content)
            .or_else(|| {
                message
                    .get("text")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .unwrap_or_default();
        if text.is_empty() {
            continue;
        }
        let id = message
            .get("entryId")
            .or_else(|| message.get("messageId"))
            .or_else(|| message.get("id"))
            .and_then(Value::as_str)
            .map_or_else(|| format!("history-{index}"), str::to_owned);
        rows.push(PublicRow {
            id,
            role: role.into(),
            text,
        });
    }
    rows
}

fn merge_btw_rows(thread: &mut BtwThread, rows: Vec<PublicRow>) {
    // A failed/partial restore must not erase received text.
    for row in rows {
        if let Some(existing) = thread
            .rows
            .iter_mut()
            .find(|existing| existing.id == row.id)
        {
            *existing = row;
        } else if row.role == "user" {
            if let Some(existing) = thread.rows.iter_mut().find(|existing| {
                existing.id.starts_with("accepted-local-") && existing.text == row.text
            }) {
                *existing = row;
            } else {
                thread.rows.push(row);
            }
        } else {
            thread.rows.push(row);
        }
    }
}

/// Only the adapter's exact text-block identity convention is ownership evidence.
fn owns_projection_id(message_id: &str, block_id: &str) -> bool {
    block_id == message_id
        || block_id
            .strip_prefix(message_id)
            .and_then(|suffix| suffix.strip_prefix("-text-"))
            .is_some_and(|suffix| !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit()))
}

fn merge_btw_publication(
    thread: &mut BtwThread,
    rows: Vec<PublicRow>,
    blocks: &[MessageBlock],
    continuous: bool,
) {
    if !continuous {
        for slot in thread.projection_rows.values_mut() {
            *slot = None;
        }
        thread.projection_message_owners.clear();
    }
    // Completion may precede canonical settlement. Keep owner evidence even
    // after a cached fragment has already upgraded to a full anonymous row.
    for (index, block) in blocks.iter().enumerate() {
        let MessageBlock::Text { id, .. } = block else {
            continue;
        };
        let Some(owner) = rows
            .iter()
            .find(|row| row.role == "assistant" && owns_projection_id(&row.id, id))
        else {
            continue;
        };
        if let Some(previous) = thread
            .projection_message_owners
            .insert(index, owner.id.clone())
        {
            if previous != owner.id {
                if thread.rows.iter().any(|row| row.id == owner.id) {
                    thread.rows.retain(|row| row.id != previous);
                } else if let Some(row) = thread.rows.iter_mut().find(|row| row.id == previous) {
                    *row = owner.clone();
                }
            }
        }
    }
    // Within one projection, blocks are append-only except for canonical ID
    // reconciliation. A slot therefore carries identity across that rename,
    // even when the old ID no longer appears in either messages or blocks.
    for (index, block) in blocks.iter().enumerate() {
        let MessageBlock::Text { id, .. } = block else {
            continue;
        };
        let previous = thread
            .projection_rows
            .iter()
            .find_map(|(old, slot)| (*slot == Some(index) && old != id).then(|| old.clone()));
        if let Some(old) = previous {
            thread.projection_rows.remove(&old);
            if thread.rows.iter().any(|row| row.id == *id) {
                thread.rows.retain(|row| row.id != old);
            } else if let Some(row) = thread.rows.iter_mut().find(|row| row.id == old) {
                row.id.clone_from(id);
            }
            thread.projection_rows.insert(id.clone(), Some(index));
        }
    }
    for row in rows {
        if row.role == "assistant" {
            let retired: Vec<_> = thread
                .projection_rows
                .keys()
                .filter(|id| owns_projection_id(&row.id, id))
                .cloned()
                .collect();
            // Replace the first cached fragment in place, preserving chronology.
            // Other text fragments owned by this completed message are retired.
            if !thread.rows.iter().any(|existing| existing.id == row.id) {
                if let Some(existing) = thread
                    .rows
                    .iter_mut()
                    .find(|existing| retired.contains(&existing.id))
                {
                    *existing = row.clone();
                }
            }
            thread
                .rows
                .retain(|existing| existing.id == row.id || !retired.contains(&existing.id));
            for id in retired {
                thread.projection_rows.remove(&id);
            }
        }
        merge_btw_rows(thread, vec![row]);
    }
    for (index, block) in blocks.iter().enumerate() {
        let MessageBlock::Text { id, text } = block else {
            continue;
        };
        if text.is_empty()
            || thread.rows.iter().any(|row| {
                row.role == "assistant"
                    && !thread.projection_rows.contains_key(&row.id)
                    && owns_projection_id(&row.id, id)
            })
        {
            continue;
        }
        thread.projection_rows.insert(id.clone(), Some(index));
        merge_btw_rows(
            thread,
            vec![PublicRow {
                id: id.clone(),
                role: "assistant".into(),
                text: text.clone(),
            }],
        );
    }
}

#[cfg(test)]
mod btw_row_tests {
    use super::*;

    fn row(id: &str, text: &str) -> PublicRow {
        PublicRow {
            id: id.into(),
            role: "assistant".into(),
            text: text.into(),
        }
    }
    fn block(id: &str, text: &str) -> MessageBlock {
        MessageBlock::Text {
            id: id.into(),
            text: text.into(),
        }
    }
    fn ids(thread: &BtwThread) -> Vec<&str> {
        thread.rows.iter().map(|row| row.id.as_str()).collect()
    }

    #[test]
    fn btw_completed_anonymous_message_later_canonicalizes_once() {
        let mut thread = BtwThread::default();
        let anonymous = vec![block("anonymous-text-0", "answer")];
        merge_btw_publication(&mut thread, vec![], &anonymous, false);
        merge_btw_publication(
            &mut thread,
            vec![row("anonymous", "answer")],
            &anonymous,
            true,
        );
        assert_eq!(ids(&thread), ["anonymous"]);
        let canonical = vec![block("formal-text-0", "answer")];
        merge_btw_publication(&mut thread, vec![row("formal", "answer")], &canonical, true);
        assert_eq!(
            ids(&thread),
            ["formal"],
            "settlement must also retire an already completed anonymous owner"
        );
    }

    #[test]
    fn btw_projection_completion_reclaims_cached_row() {
        let mut thread = BtwThread::default();
        let blocks = vec![block("message-id-text-0", "answer")];
        merge_btw_publication(&mut thread, vec![], &blocks, false);
        merge_btw_publication(
            &mut thread,
            vec![row("message-id", "answer")],
            &blocks,
            true,
        );
        assert_eq!(
            thread.rows.len(),
            1,
            "completed message must retire its projection row"
        );
        assert_eq!(thread.rows[0].id, "message-id");
        assert!(thread.projection_rows.is_empty());
    }

    #[test]
    fn btw_projection_canonical_rename_keeps_order_and_retires_all_fragments() {
        let mut thread = BtwThread::default();
        merge_btw_publication(
            &mut thread,
            vec![row("older", "same")],
            &[
                block("anonymous-text-0", "sa"),
                block("anonymous-text-1", "me"),
            ],
            false,
        );
        merge_btw_publication(
            &mut thread,
            vec![row("newer", "same")],
            &[
                block("anonymous-text-0", "sa"),
                block("anonymous-text-1", "me"),
            ],
            true,
        );
        let blocks = vec![
            block("canonical-text-0", "sa"),
            block("canonical-text-1", "me"),
        ];
        merge_btw_publication(&mut thread, vec![row("canonical", "same")], &blocks, true);
        assert_eq!(ids(&thread), ["older", "canonical", "newer"]);
        assert_eq!(thread.rows[1].text, "same");
        assert!(thread.projection_rows.is_empty());
        merge_btw_publication(&mut thread, vec![row("canonical", "same")], &blocks, true);
        assert_eq!(ids(&thread), ["older", "canonical", "newer"]);
    }

    #[test]
    fn btw_projection_rebuild_does_not_reuse_slots_or_erase_partial_history() {
        let mut thread = BtwThread::default();
        merge_btw_publication(
            &mut thread,
            vec![row("history", "same")],
            &[block("old-text-0", "same")],
            false,
        );
        merge_btw_publication(
            &mut thread,
            vec![],
            &[block("unrelated-text-0", "same")],
            false,
        );
        assert_eq!(ids(&thread), ["history", "old-text-0", "unrelated-text-0"]);
        merge_btw_publication(&mut thread, vec![row("old", "same")], &[], false);
        assert_eq!(ids(&thread), ["history", "old", "unrelated-text-0"]);
        merge_btw_publication(&mut thread, vec![], &[], false);
        assert_eq!(ids(&thread), ["history", "old", "unrelated-text-0"]);
    }

    #[test]
    fn btw_projection_rename_before_completion_migrates_cache() {
        let mut thread = BtwThread::default();
        merge_btw_publication(
            &mut thread,
            vec![],
            &[block("anonymous-text-0", "partial")],
            false,
        );
        merge_btw_publication(
            &mut thread,
            vec![],
            &[block("canonical-text-0", "full")],
            true,
        );
        assert_eq!(ids(&thread), ["canonical-text-0"]);
        assert_eq!(thread.rows[0].text, "full");
        merge_btw_publication(&mut thread, vec![row("canonical", "full")], &[], true);
        assert_eq!(ids(&thread), ["canonical"]);
        assert!(thread.projection_rows.is_empty());
    }

    #[test]
    fn btw_projection_cleanup_requires_owned_assistant_identity() {
        let mut thread = BtwThread::default();
        // This historical ID resembles a block ID but was never a projection row.
        merge_btw_publication(
            &mut thread,
            vec![row("real-text-0", "same")],
            &[
                block("live-text-0", "same"),
                block("live-text-invalid", "same"),
            ],
            false,
        );
        let mut user = row("live", "same");
        user.role = "user".into();
        merge_btw_publication(&mut thread, vec![row("real", "same"), user], &[], true);
        assert_eq!(
            ids(&thread),
            [
                "real-text-0",
                "live-text-0",
                "live-text-invalid",
                "real",
                "live"
            ]
        );
        merge_btw_publication(&mut thread, vec![row("live", "same")], &[], true);
        assert_eq!(
            ids(&thread),
            ["real-text-0", "live-text-invalid", "real", "live"]
        );
    }
}

fn publish_connection(
    runtime: &BtwRuntime,
    connection: &SessionConnection,
    snapshot: bool,
    continuous: bool,
) {
    let projection = connection.projection();
    let rows = connection_rows(connection);
    let tools = projection
        .blocks
        .iter()
        .filter_map(|block| match block {
            MessageBlock::ToolCall { name, status, .. } => {
                Some(format!("工具 {name} · {status:?}"))
            }
            _ => None,
        })
        .collect();
    runtime.update(|thread| {
        merge_btw_publication(thread, rows, &projection.blocks, continuous);
        thread.tools = tools;
        // A live snapshot is not proof of the no-tool boundary: only the
        // explicit command readback in send_btw may set this flag.
        thread.connected = projection.connection == ConnectionState::Online;
        if !thread.connected {
            thread.tools_confirmed_empty = false;
        }
        if !thread.activity.operation || !snapshot {
            thread.activity.busy =
                !matches!(projection.phase, RunPhase::Ready | RunPhase::Settled(_));
            if matches!(projection.phase, RunPhase::Settled(_)) {
                thread.activity.cancelling = false;
            }
        }
        thread.status = match &projection.phase {
            RunPhase::Settled(outcome) => format!("节点已结算 · {outcome:?}"),
            RunPhase::AwaitingInput => {
                "旁问正在等待节点上的权限或用户输入；主对话不受影响。".into()
            }
            RunPhase::AwaitingSettlement => "正文已结束，等待节点结算…".into(),
            RunPhase::Ready if thread.activity.cancelling => {
                "取消请求已接收，尚未收到结算事件。".into()
            }
            RunPhase::Ready if thread.tools_confirmed_empty => {
                "独立旁问就绪 · 无工具权限已确认".into()
            }
            RunPhase::Ready => "旁问无工具权限未确认；发送前将核对节点状态。".into(),
            _ if thread.activity.cancelling => "已请求取消，等待节点结算…".into(),
            _ => "独立旁问运行中".into(),
        };
        if let Some(usage) = &projection.usage {
            thread.usage = format!(
                "输入 {} / 输出 {} token",
                usage
                    .input_tokens
                    .map_or_else(|| "—".into(), |n| n.to_string()),
                usage
                    .output_tokens
                    .map_or_else(|| "—".into(), |n| n.to_string())
            );
        }
    });
}

#[component]
fn ExtraIcon(kind: &'static str) -> Element {
    rsx! { crate::shared::components::Icon { name: kind } }
}
