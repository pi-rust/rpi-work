//! Native clipboard and file-picker attachments.

use std::{
    collections::HashSet,
    fs::{self, File},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

use base64::{Engine, engine::general_purpose::STANDARD};
use gpui::{Context, Div, PathPromptOptions, div, prelude::*, px};
use rpi_extcore::prompt::{self, PromptBlock, PromptContent};
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::{DraftKey, Shell, file_format::FileFormat};
use crate::icons::icon;

const PROJECT_ATTACHMENTS: &str = "rpi-attachments";
const MAX_PROJECT_FILE: usize = 24 * 1024 * 1024;
pub(crate) const MAX_PROJECT_TOTAL: usize = 48 * 1024 * 1024;

#[derive(Clone)]
pub(crate) struct Attachment {
    pub(crate) id: u64,
    pub(crate) name: String,
    pub(crate) block: PromptBlock,
    project_file: Option<Arc<[u8]>>,
}

impl Attachment {
    pub(crate) fn inline(name: String, block: PromptBlock) -> Self {
        Self {
            id: 0,
            name,
            block,
            project_file: None,
        }
    }

    pub(crate) fn project_bytes(&self) -> usize {
        self.project_file.as_ref().map_or(0, |bytes| bytes.len())
    }
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

fn attachment_name(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_string_lossy();
    // Keep UTF-8 names and extensions. Escape separators and characters that
    // are invalid in Windows paths or could introduce a second prompt line.
    let name = name
        .chars()
        .map(|ch| {
            if ch.is_control() || matches!(ch, '/' | '\\' | ':' | '<' | '>' | '"' | '|' | '?' | '*')
            {
                '_'
            } else {
                ch
            }
        })
        .collect::<String>();
    let name = name.trim_matches([' ', '.']);
    (!name.is_empty() && name.len() <= 240).then(|| name.to_owned())
}

fn load(mut paths: Vec<PathBuf>, local: bool) -> Result<Vec<Attachment>, String> {
    let mut seen = HashSet::new();
    paths.retain(|path| seen.insert(path.clone()));
    if paths.len() >= prompt::MAX_CONTENT_BLOCKS {
        return Err("请选择少于 64 个附件。".into());
    }
    let mut total = 0;
    paths
        .into_iter()
        .map(|path| load_file(path, local, &mut total))
        .collect()
}

fn load_file(path: PathBuf, local: bool, total: &mut usize) -> Result<Attachment, String> {
    let name = attachment_name(&path).ok_or("文件名无效或过长。")?;
    let read_error = |error| format!("无法读取 {name}：{error}");
    if !path.is_absolute() || !path.metadata().map_err(read_error)?.is_file() {
        return Err(format!("{name} 不是普通文件。"));
    }
    let limit = if local {
        MAX_PROJECT_FILE
    } else {
        prompt::MAX_IMAGE_BYTES
    };
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(read_error)?
        .take((limit + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(read_error)?;
    let remote_error =
        || format!("远程节点支持 PNG、JPEG 和小型 UTF-8 文本附件。{name} 需要在本机项目中发送。");
    if bytes.len() > limit {
        return Err(if local {
            format!("{name} 超过 24 MiB。")
        } else {
            remote_error()
        });
    }
    *total += bytes.len();
    if *total > MAX_PROJECT_TOTAL {
        return Err("附件总大小超过 48 MiB。".into());
    }
    let mime = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        Some("image/jpeg")
    } else {
        None
    };
    if let Some(mime_type) = mime.filter(|_| bytes.len() <= prompt::MAX_IMAGE_BYTES) {
        return Ok(Attachment::inline(
            name,
            PromptBlock::Image {
                mime_type: mime_type.into(),
                data: STANDARD.encode(bytes),
            },
        ));
    }
    let text = format!("\n\n附件 · {name}\n");
    if bytes.len() + text.len() <= prompt::MAX_TEXT_BYTES
        && !bytes.starts_with(b"%PDF-")
        && !bytes.contains(&0)
        && let Ok(body) = std::str::from_utf8(&bytes)
    {
        return Ok(Attachment::inline(
            name,
            PromptBlock::Text { text: text + body },
        ));
    }
    if !local {
        return Err(remote_error());
    }
    let digest = Sha256::digest(&bytes);
    let text = format!("\n\n项目附件 · {name}\n路径：{PROJECT_ATTACHMENTS}/{digest:x}/{name}");
    Ok(Attachment {
        id: 0,
        name,
        block: PromptBlock::Text { text },
        project_file: Some(bytes.into()),
    })
}

pub(super) fn project_attachment(text: &str) -> Option<(&str, &str)> {
    let (name, path) = text
        .strip_prefix("\n\n项目附件 · ")?
        .split_once("\n路径：")?;
    let (digest, basename) = path
        .strip_prefix(PROJECT_ATTACHMENTS)?
        .strip_prefix('/')?
        .split_once('/')?;
    (digest.len() == 64
        && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
        && name == basename
        && !name.contains(['/', '\\'])
        && attachment_name(Path::new(name)).as_deref() == Some(name))
    .then_some((name, path))
}

pub(crate) fn stage_project_attachments(root: &Path, items: &[Attachment]) -> Result<(), String> {
    let root = root
        .canonicalize()
        .map_err(|error| format!("无法打开附件所属项目：{error}"))?;
    for item in items.iter().filter(|item| item.project_file.is_some()) {
        let PromptBlock::Text { text } = &item.block else {
            unreachable!("project attachment uses a text reference")
        };
        let (_, path) = project_attachment(text).ok_or("项目附件路径无效。")?;
        let bytes = item.project_file.as_deref().unwrap();
        stage_file(&root, Path::new(path), bytes)
            .map_err(|error| format!("无法保存 {} 到项目：{error}", item.name))?;
    }
    Ok(())
}

fn stage_file(root: &Path, path: &Path, bytes: &[u8]) -> io::Result<()> {
    let directory = path.parent().expect("project attachments have a parent");
    let mut target = root.to_owned();
    for component in directory.components() {
        target.push(component);
        match fs::create_dir(&target) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
        if !fs::symlink_metadata(&target)?.is_dir() || !target.canonicalize()?.starts_with(root) {
            return Err(io::Error::other("附件目录不是项目内的普通目录"));
        }
    }
    let destination = root.join(path);
    let mut file = tempfile::NamedTempFile::new_in(target)?;
    file.write_all(bytes)?;
    match file.persist_noclobber(&destination) {
        Ok(_) => Ok(()),
        Err(error) if error.error.kind() == io::ErrorKind::AlreadyExists => {
            if fs::symlink_metadata(&destination)?.is_file()
                && destination.canonicalize()?.starts_with(root)
            {
                let mut stored = Vec::new();
                File::open(destination)?
                    .take((bytes.len() + 1) as u64)
                    .read_to_end(&mut stored)?;
                if stored == bytes {
                    return Ok(());
                }
            }
            Err(io::Error::other("同名附件已被修改"))
        }
        Err(error) => Err(error.into()),
    }
}

impl Shell {
    pub(super) fn pick_attachments(&mut self, cx: &mut Context<Self>) {
        let key = self.draft_key();
        if !self.supports_attachments() || !self.selecting_attachments.insert(key) {
            return;
        }
        let selection = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("选择附件".into()),
        });
        self.load_attachments(
            key,
            async move {
                match selection.await {
                    Ok(result) => result.map_err(|error| error.to_string()),
                    Err(error) => Err(error.to_string()),
                }
            },
            cx,
        );
    }

    pub(super) fn paste_attachments(&mut self, cx: &mut Context<Self>) -> bool {
        let paths = arboard::Clipboard::new().and_then(|mut clipboard| clipboard.get().file_list());
        let Ok(paths) = paths else { return false };
        if paths.is_empty() {
            return false;
        }
        let key = self.draft_key();
        if !self.supports_attachments() {
            self.drafts.entry(key).or_default().attachment_notice =
                Some("当前会话尚未确认支持附件。".into());
            cx.notify();
        } else if self.selecting_attachments.insert(key) {
            self.load_attachments(key, async move { Ok(Some(paths)) }, cx);
        }
        true
    }

    fn load_attachments(
        &self,
        key: DraftKey,
        selection: impl Future<Output = Result<Option<Vec<PathBuf>>, String>> + 'static,
        cx: &mut Context<Self>,
    ) {
        let background = cx.background_executor().clone();
        let local = self.node.as_ref().is_some_and(crate::data::Node::is_local);
        cx.spawn(async move |this, cx| {
            let result = match selection.await {
                Ok(Some(paths)) => background
                    .spawn(async move { load(paths, local) })
                    .await
                    .map(Some),
                Ok(None) => Ok(None),
                Err(error) => Err(error),
            };
            let _ = this.update(cx, |shell, cx| {
                shell.selecting_attachments.remove(&key);
                let draft = shell.drafts.entry(key).or_default();
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
        if self.session.is_none() {
            return self.node.as_ref().is_some_and(crate::data::Node::is_local)
                && self.project.is_some()
                && self.creating_session.is_none();
        }
        self.frame
            .as_ref()
            .and_then(|frame| frame.access.as_ref())
            .is_some_and(|access| {
                access.supports_prompt_content() && access.supports_command("prompt")
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
                let format = FileFormat::detect(&item.name, "");
                let shape = if matches!(item.block, PromptBlock::Image { .. }) {
                    "image"
                } else {
                    format.icon()
                };
                let accent = format.accent(palette).unwrap_or(palette.muted);
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .max_w_full()
                    .pl_2()
                    .rounded(px(5.0))
                    .bg(palette.subtle)
                    .child(icon(shape, 13.0, accent).flex_none())
                    .child(
                        self.text(item.name.clone(), 12.0, palette.text)
                            .min_w_0()
                            .truncate(),
                    )
                    .child(
                        self.navigation_tooltip(
                            self.navigation_control(
                                self.button(("remove-attachment", id), "")
                                    .w(px(22.0))
                                    .px_0()
                                    .child(icon("x", 14.0, palette.muted)),
                                self.overlay.is_none(),
                            )
                            .h(px(24.0)),
                            format!("移除附件 {}", item.name),
                        )
                        .on_click(cx.listener(
                            move |shell, _, window, cx| {
                                if let Some(draft) = shell.drafts.get_mut(&shell.draft_key()) {
                                    draft.attachments.retain(|item| item.id != id);
                                    draft.attachment_notice = None;
                                }
                                shell.focus_composer(window);
                                cx.notify();
                            },
                        )),
                    )
            }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pdf_is_staged_as_an_original_file_and_retries_reuse_it() {
        let source = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        let file = source.path().join("预算 报告.pdf");
        let bytes = b"%PDF-1.7\n%%EOF\n";
        fs::write(&file, bytes).unwrap();
        assert!(load(vec![file.clone()], false).is_err());
        let items = load(vec![file.clone(), file.clone()], true).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].project_bytes(), bytes.len());
        let blocks = content_blocks(String::new(), &items).unwrap();
        let (name, relative) = project_attachment(blocks[0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(name, "预算 报告.pdf");
        let destination = project.path().join(relative);
        assert!(!destination.exists());
        fs::remove_file(file).unwrap();
        stage_project_attachments(project.path(), &items).unwrap();
        stage_project_attachments(project.path(), &items).unwrap();
        assert_eq!(fs::read(&destination).unwrap(), bytes);
        fs::write(&destination, b"project edit").unwrap();
        assert!(stage_project_attachments(project.path(), &items).is_err());
        assert_eq!(fs::read(destination).unwrap(), b"project edit");
        assert!(
            project_attachment(&format!(
                "\n\n项目附件 · ../a.pdf\n路径：rpi-attachments/{}/../a.pdf",
                "0".repeat(64)
            ))
            .is_none()
        );
    }
}
