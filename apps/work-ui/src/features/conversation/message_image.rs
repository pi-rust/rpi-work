use dioxus::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};

const RENDERER: &str = include_str!("message_image.js");
const CSS: &str = include_str!("message_image.css");
const MAX_DECODED_BYTES: usize = 10 * 1024 * 1024;
const INVALID_IMAGE: &str = "图片无效或格式不受支持。";
static NEXT_INSTANCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ImageBlock {
    pub mime_type: String,
    pub data: String,
}

/// Only explicit protocol image blocks are claimed, including malformed blocks.
/// Errors deliberately contain neither field values nor the encoded payload.
pub(super) fn image_block(value: &serde_json::Value) -> Option<Result<ImageBlock, String>> {
    if value.get("type").and_then(serde_json::Value::as_str) != Some("image") {
        return None;
    }
    let mime = value.get("mimeType").and_then(serde_json::Value::as_str);
    let data = value.get("data").and_then(serde_json::Value::as_str);
    Some(match (mime, data) {
        (Some(mime_type), Some(data))
            if matches!(
                mime_type,
                "image/png" | "image/jpeg" | "image/gif" | "image/webp"
            ) && valid_base64(data) =>
        {
            Ok(ImageBlock {
                mime_type: mime_type.to_owned(),
                data: data.to_owned(),
            })
        }
        _ => Err(INVALID_IMAGE.to_owned()),
    })
}

fn base64_digit(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

fn valid_base64(data: &str) -> bool {
    let bytes = data.as_bytes();
    if bytes.is_empty() || bytes.len() % 4 != 0 || bytes.len() > MAX_DECODED_BYTES.div_ceil(3) * 4 {
        return false;
    }
    let padding = if bytes.ends_with(b"==") {
        2
    } else {
        usize::from(bytes.ends_with(b"="))
    };
    let content = &bytes[..bytes.len() - padding];
    if bytes.len() / 4 * 3 - padding > MAX_DECODED_BYTES
        || !content.iter().all(|byte| base64_digit(*byte).is_some())
    {
        return false;
    }
    // RFC 4648 canonical padding: unused low bits must be zero.
    let last = base64_digit(content[content.len() - 1]).unwrap_or(0);
    match padding {
        2 => last.trailing_zeros() >= 4,
        1 => last.trailing_zeros() >= 2,
        _ => true,
    }
}

/// Project bytes have already passed the authenticated owner's bounded read.
/// Reuse the same raster decoder/viewer, never a file:// or credential-bearing URL.
pub(crate) fn project_image(image: work_client::ProjectImage) -> Result<ImageBlock, String> {
    use base64::Engine as _;
    if image.bytes.is_empty()
        || image.bytes.len() > work_client::PROJECT_IMAGE_MAX_BYTES
        || !matches!(image.media_type.as_str(), "image/png" | "image/jpeg" | "image/gif" | "image/webp")
    {
        return Err("项目图片预览支持不超过 20 MiB 的 PNG、JPEG、GIF、WebP。".into());
    }
    Ok(ImageBlock {
        mime_type: image.media_type,
        data: base64::engine::general_purpose::STANDARD.encode(image.bytes),
    })
}

#[component]
pub(crate) fn MessageImage(image: Result<ImageBlock, String>, #[props(default)] project: bool) -> Element {
    let instance = use_hook(|| NEXT_INSTANCE.fetch_add(1, Ordering::Relaxed));
    let id = format!("rpi-message-image-{instance}");
    let mut status = use_signal(|| "正在加载图片…".to_owned());
    let mut eval_task = use_signal(|| None::<dioxus::core::Task>);
    let effect_id = id.clone();
    // Parsing is done by the caller once, not repeated during signal rerenders.
    use_effect(use_reactive((&image,), move |(image,)| {
        if let Some(task) = eval_task.write().take() {
            task.cancel();
        }
        status.set("正在加载图片…".to_owned());
        let script = mount_script(&effect_id, &image, project);
        eval_task.set(Some(spawn(async move {
            let installed = document::eval("dioxus.send(!!window.__rpiMessageImages);")
                .recv::<bool>()
                .await
                .unwrap_or(false);
            if !installed {
                let bootstrap =
                    format!("if(!window.__rpiMessageImages){{{RENDERER}}}dioxus.send(true);");
                let _ = document::eval(&bootstrap).recv::<bool>().await;
            }
            let mut evaluator = document::eval(&script);
            while let Ok(value) = evaluator.recv::<String>().await {
                status.set(value);
            }
        })));
    }));
    let cleanup_id = serde_json::to_string(&id).unwrap();
    use_drop(move || {
        if let Some(task) = eval_task.write().take() {
            task.cancel();
        }
        let _ = document::eval(&format!(
            "window.__rpiMessageImages?.dispose({cleanup_id});"
        ));
    });
    let ready = status.read().is_empty();
    let viewer_id = serde_json::to_string(&id).unwrap();
    rsx! {
        style { {CSS} }
        figure { class: "conversation-message-image", id: "{id}", tabindex: "-1",
            figcaption { class: "message-image-heading",
                span { if project { "项目图片" } else { "消息图片" } }
                button { r#type: "button", class: "button", aria_label: if project { "放大查看项目图片" } else { "放大查看消息图片" },
                    aria_haspopup: "dialog", disabled: !ready,
                    onclick: move |_| {
                        let _ = document::eval(&format!(
                            "window.__rpiMessageImages?.openViewer({viewer_id});"
                        ));
                    },
                    "放大查看"
                }
            }
            p { class: "message-image-status", role: "status", hidden: ready, "{status}" }
            div { class: "message-image-preview", hidden: !ready,
                img { alt: if project { "项目文件图片预览" } else { "对话中的消息图片（未提供文字描述）" }, decoding: "async" }
            }
        }
    }
}

fn mount_script(id: &str, image: &Result<ImageBlock, String>, project: bool) -> String {
    let id = serde_json::to_string(id).unwrap();
    if let Ok(image) = image {
        let mime = serde_json::to_string(&image.mime_type).unwrap();
        let data = serde_json::to_string(&image.data).unwrap();
        format!(
            "window.__rpiMessageImages.mount({id},{mime},{data},value=>{{try{{dioxus.send(value);}}catch(_){{}}}},{project});"
        )
    } else {
        format!(
            "window.__rpiMessageImages.mount({id},null,null,value=>{{try{{dioxus.send(value);}}catch(_){{}}}});"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn project_rasters_are_encoded_without_exposing_urls_or_credentials() {
        let image = project_image(work_client::ProjectImage {
            bytes: vec![0, 1, 255], media_type: "image/png".into(),
        }).unwrap();
        assert_eq!(image.data, "AAH/");
        let script = mount_script("project", &Ok(image), true);
        assert!(script.ends_with("},true);"));
        for mime in ["image/svg+xml", "text/html", "image/bmp"] {
            assert!(project_image(work_client::ProjectImage {
                bytes: vec![1], media_type: mime.into(),
            }).is_err());
        }
        assert!(project_image(work_client::ProjectImage {
            bytes: vec![0; work_client::PROJECT_IMAGE_MAX_BYTES + 1],
            media_type: "image/png".into(),
        }).is_err());
    }

    #[test]
    fn only_explicit_images_are_claimed() {
        for value in [
            json!(null),
            json!({}),
            json!({"type":"text"}),
            json!({"type":"Image"}),
        ] {
            assert!(image_block(&value).is_none());
        }
        assert!(image_block(&json!({"type":"image"})).unwrap().is_err());
    }

    #[test]
    fn image_fields_and_mime_are_exact() {
        for mime in ["image/png", "image/jpeg", "image/gif", "image/webp"] {
            let value = json!({"type":"image","mimeType":mime,"data":"AA=="});
            let block = image_block(&value).unwrap().unwrap();
            assert_eq!(block.mime_type, mime);
            assert_eq!(block.data, "AA==");
        }
        for value in [
            json!({"type":"image","mimeType":"image/svg+xml","data":"AA=="}),
            json!({"type":"image","mimeType":"IMAGE/PNG","data":"AA=="}),
            json!({"type":"image","mimeType":"image/png;foo=bar","data":"AA=="}),
            json!({"type":"image","mime_type":"image/png","data":"AA=="}),
            json!({"type":"image","mimeType":false,"data":"AA=="}),
            json!({"type":"image","mimeType":"image/png","data":123}),
        ] {
            assert_eq!(image_block(&value), Some(Err(INVALID_IMAGE.to_owned())));
        }
    }

    #[test]
    fn base64_requires_standard_alphabet_canonical_padding_and_no_whitespace() {
        for data in ["AA==", "AAA=", "AAAA", "+/8=", "////", "SGVsbG8="] {
            assert!(valid_base64(data), "valid encoding rejected");
        }
        for data in [
            "",
            "A",
            "AA",
            "AAA",
            "A===",
            "====",
            "AA=A",
            "AA==AAAA",
            "AB==",
            "AAB=",
            "AA__",
            "AA--",
            "AA==\n",
            " AA==",
            "data:image/png;base64,AA==",
            "中文",
        ] {
            assert!(!valid_base64(data), "invalid encoding accepted");
        }
    }

    #[test]
    fn decoded_limit_includes_padding_boundary() {
        let full_groups = MAX_DECODED_BYTES / 3;
        let mut exact = "AAAA".repeat(full_groups);
        exact.push_str("AA==");
        assert!(valid_base64(&exact));
        exact.truncate(full_groups * 4);
        exact.push_str("AAA=");
        assert!(!valid_base64(&exact));
        exact.push_str("AAAA");
        assert!(!valid_base64(&exact));
    }

    #[test]
    fn eval_arguments_are_json_encoded_and_errors_never_echo_payloads() {
        let image = Ok(ImageBlock {
            mime_type: "image/png".to_owned(),
            data: "\";alert('no');\\\n</script>".to_owned(),
        });
        let script = mount_script("id\"", &image, false);
        let Ok(block) = image else { unreachable!() };
        assert!(script.contains(&serde_json::to_string(&block.data).unwrap()));
        assert!(script.contains(&serde_json::to_string("id\"").unwrap()));
        let error = mount_script("safe", &Err("secret payload".to_owned()), false);
        assert!(!error.contains("secret payload"));
        assert!(!RENDERER.contains("innerHTML"));
        assert!(!RENDERER.contains("data:image"));
    }
}
