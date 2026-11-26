//! Native file selection and bounded inline prompt attachments.

use std::{fs::File, io::Read, path::PathBuf};

use base64::{Engine, engine::general_purpose::STANDARD};
use gpui::{Context, Div, PathPromptOptions, div, prelude::*, px};
use rpi_extcore::prompt::{self, PromptBlock, PromptContent};
use serde_json::Value;

use super::Shell;

pub(crate) struct Attachment {
    pub(crate) id: u64,
    pub(crate) name: String,
    pub(crate) block: PromptBlock,
}

pub(crate) fn content_blocks(message: String, items: &[Attachment]) -> Result<Value, String> {
    let text = (!message.trim().is_empty()).then_some(PromptBlock::Text { text: message });
    let blocks = text
        .into_iter()
        .chain(items.iter().map(|file| file.block.clone()))
        .collect();
    let content = PromptContent::new(blocks)?;
    serde_json::to_value(content).map_err(|error| error.to_string())
}

fn load(paths: Vec<PathBuf>) -> Result<Vec<Attachment>, String> {
    if paths.len() >= prompt::MAX_CONTENT_BLOCKS {
        return Err("请选择少于 64 个附件。".into());
    }
    let mut total_read = 0;
    paths
        .into_iter()
        .map(|path| {
            let name = path
                .file_name()
                .ok_or("无法读取文件名称。")?
                .to_string_lossy()
                .chars()
                .map(|ch| if ch.is_control() { ' ' } else { ch })
                .collect::<String>();
            let read_error = |error| format!("无法读取 {name}：{error}");
            if !path.metadata().map_err(read_error)?.is_file() {
                return Err(format!("{name} 不是普通文件。"));
            }
            let mut bytes = Vec::new();
            File::open(path)
                .map_err(read_error)?
                .take((prompt::MAX_IMAGE_BYTES + 1) as u64)
                .read_to_end(&mut bytes)
                .map_err(read_error)?;
            if bytes.is_empty() || bytes.len() > prompt::MAX_IMAGE_BYTES {
                return Err(format!("{name} 为空或超过 1 MiB。"));
            }
            total_read += bytes.len();
            if total_read > prompt::MAX_TOTAL_IMAGE_BYTES + prompt::MAX_TEXT_BYTES {
                return Err("附件超过单条消息的大小上限。".into());
            }
            let block = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
                PromptBlock::Image {
                    mime_type: "image/png".into(),
                    data: STANDARD.encode(bytes),
                }
            } else if bytes.starts_with(b"\xff\xd8\xff") {
                PromptBlock::Image {
                    mime_type: "image/jpeg".into(),
                    data: STANDARD.encode(bytes),
                }
            } else {
                if bytes.contains(&0) || bytes.len() > prompt::MAX_TEXT_BYTES {
                    return Err(format!("{name} 不符合 PNG、JPEG 或 UTF-8 文本附件要求。"));
                }
                let text =
                    String::from_utf8(bytes).map_err(|_| format!("{name} 不是 UTF-8 文本。"))?;
                PromptBlock::Text {
                    text: format!("\n\n附件 · {name}\n{text}"),
                }
            };
            Ok(Attachment { id: 0, name, block })
        })
        .collect()
}

impl Shell {
    fn pick_attachments(&mut self, cx: &mut Context<Self>) {
        let Some(index) = self.session.filter(|_| self.supports_attachments()) else {
            return;
        };
        if !self.selecting_attachments.insert(index) {
            return;
        }
        let selection = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("选择图片或文本附件".into()),
        });
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = match selection.await {
                Ok(Ok(Some(paths))) => background.spawn(async move { load(paths) }).await.map(Some),
                Ok(Ok(None)) => Ok(None),
                Ok(Err(error)) => Err(error.to_string()),
                Err(error) => Err(error.to_string()),
            };
            let _ = this.update(cx, |shell, cx| {
                shell.selecting_attachments.remove(&index);
                let draft = shell
                    .drafts
                    .entry(super::DraftKey::Session(index))
                    .or_default();
                match result {
                    Ok(Some(items)) if !items.is_empty() => draft.add_attachments(items),
                    Ok(_) => {}
                    Err(error) => draft.attachment_notice = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn supports_attachments(&self) -> bool {
        self.frame
            .as_ref()
            .and_then(|frame| frame.access.as_ref())
            .is_some_and(|access| {
                access.supports_prompt_content() && access.supports_command("prompt")
            })
    }

    pub(super) fn add_attachments_button(&self, cx: &Context<Self>) -> impl IntoElement {
        let ready = self.supports_attachments()
            && self
                .session
                .is_some_and(|index| !self.selecting_attachments.contains(&index));
        self.icon_button("add-attachments", "plus", 24.0)
            .when(!ready, |button| button.opacity(0.4).cursor_default())
            .when(ready, |button| {
                button.on_click(cx.listener(|shell, _, _, cx| shell.pick_attachments(cx)))
            })
    }

    pub(super) fn attachment_chips(
        &self,
        items: &[Attachment],
        cx: &Context<Self>,
    ) -> gpui::Stateful<Div> {
        let palette = self.palette;
        div()
            .id("draft-attachments")
            .when(!items.is_empty(), |list| {
                list.flex()
                    .flex_wrap()
                    .gap_1()
                    .max_h(px(88.0))
                    .overflow_y_scroll()
                    .pb_2()
            })
            .children(items.iter().map(|item| {
                let id = item.id;
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .max_w_full()
                    .pl_2()
                    .rounded(px(5.0))
                    .bg(palette.subtle)
                    .child(self.text(item.name.clone(), 11.0, palette.text).truncate())
                    .child(
                        self.icon_button("remove-attachment", "x", 16.0)
                            .id(("remove-attachment", id))
                            .flex_none()
                            .h(px(22.0))
                            .on_click(cx.listener(move |shell, _, window, cx| {
                                if let Some(draft) = shell.drafts.get_mut(&shell.draft_key()) {
                                    draft.attachments.retain(|item| item.id != id);
                                    draft.attachment_notice = None;
                                }
                                shell.focus_composer(window);
                                cx.notify();
                            })),
                    )
            }))
    }
}
