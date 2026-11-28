//! Content blocks and explicit attachment links in the native transcript.

use std::{
    io::{Cursor, Write},
    path::Path,
    sync::Arc,
};

use base64::{Engine, engine::general_purpose::STANDARD};
use gpui::{
    App, Context, Div, FontWeight, ObjectFit, RenderImage, RenderOnce, Task, Window, div, img,
    prelude::*, px,
};
use image::{ImageDecoder, ImageFormat, ImageReader};
use serde_json::Value;
use work_viewmodel::conversation::{
    markdown::{self, Node},
    transcript_view::ContentRef,
};

use super::{
    Shell,
    attachments::project_attachment,
    file_format::{FileFormat, has_suffix},
};
use crate::{icons::icon, theme::Palette};

const MAX_IMAGE_BYTES: usize = 10 * 1024 * 1024;
const MAX_FILE_BYTES: usize = 24 * 1024 * 1024;
const MAX_TEXT_PREVIEW: usize = 32 * 1024;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Image,
    Video,
    Audio,
    File(FileFormat),
}

impl Kind {
    fn label(self, name: &str) -> &'static str {
        match self {
            Self::Image => "图片",
            Self::Video => "视频",
            Self::Audio => "音频",
            Self::File(format) => format.label(name),
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Video => "play",
            Self::Audio => "headphones",
            Self::File(format) => format.icon(),
        }
    }

    fn accent(self, palette: Palette) -> Option<gpui::Hsla> {
        match self {
            Self::File(format) => format.accent(palette),
            _ => None,
        }
    }

    fn open_label(self) -> &'static str {
        match self {
            Self::Image => "打开图片",
            Self::Video => "播放视频",
            Self::Audio => "播放音频",
            Self::File(_) => "打开文件",
        }
    }
}

fn block_kind(value: &Value) -> Option<Kind> {
    match value.get("type")?.as_str()? {
        "image" => Some(Kind::Image),
        "video" => Some(Kind::Video),
        "audio" => Some(Kind::Audio),
        "file" | "attachment" => Some(match value["mimeType"].as_str() {
            Some(mime) if mime.starts_with("video/") => Kind::Video,
            Some(mime) if mime.starts_with("audio/") => Kind::Audio,
            _ => Kind::File(FileFormat::detect(
                value["name"]
                    .as_str()
                    .or_else(|| value["filename"].as_str())
                    .unwrap_or_default(),
                value["mimeType"].as_str().unwrap_or_default(),
            )),
        }),
        _ => None,
    }
}

fn legacy_text_file(value: &Value) -> Option<(&str, &str)> {
    // This is the exact standalone text block written by the native picker.
    if value["type"] != "text" {
        return None;
    }
    let (name, text) = value["text"]
        .as_str()?
        .strip_prefix("\n\n附件 · ")?
        .split_once('\n')?;
    (!name.trim().is_empty()).then_some((name, text))
}

fn project_file_block(value: &Value) -> Option<(&str, &str)> {
    if value["type"] != "text" {
        return None;
    }
    project_attachment(value["text"].as_str()?)
}

pub(super) fn claims(value: &Value, role: &str) -> bool {
    block_kind(value).is_some()
        || (role == "user"
            && (legacy_text_file(value).is_some() || project_file_block(value).is_some()))
}

#[derive(Clone)]
struct Block {
    history: Arc<Vec<Value>>,
    reference: ContentRef,
}

impl Block {
    fn value(&self) -> Option<&Value> {
        self.reference.get(&self.history)
    }

    fn encoded(&self) -> Option<&str> {
        self.value()?.get("data")?.as_str()
    }

    fn text(&self) -> Option<&str> {
        let value = self.value()?;
        legacy_text_file(value)
            .map(|(_, text)| text)
            .or_else(|| value["text"].as_str())
    }
}

fn name(raw: &str) -> String {
    raw.rsplit(['/', '\\'])
        .next()
        .unwrap_or_default()
        .chars()
        .map(|ch| {
            if ch.is_control() || matches!(ch, ':' | '<' | '>' | '"' | '|' | '?' | '*') {
                ' '
            } else {
                ch
            }
        })
        .take(160)
        .collect::<String>()
        .trim_matches([' ', '.'])
        .to_owned()
}

fn image_format(mime: &str) -> Option<ImageFormat> {
    match mime {
        "image/png" => Some(ImageFormat::Png),
        "image/jpeg" => Some(ImageFormat::Jpeg),
        "image/gif" => Some(ImageFormat::Gif),
        "image/webp" => Some(ImageFormat::WebP),
        _ => None,
    }
}

fn playback_extension(kind: Kind, mime: &str) -> Option<&'static str> {
    let mime = mime.split(';').next()?.trim();
    match (kind, mime) {
        (Kind::Video, "video/mp4") => Some("mp4"),
        (Kind::Video, "video/webm") => Some("webm"),
        (Kind::Video, "video/quicktime") => Some("mov"),
        (Kind::Video, "video/ogg") => Some("ogv"),
        (Kind::Audio, "audio/mpeg" | "audio/mp3") => Some("mp3"),
        (Kind::Audio, "audio/wav" | "audio/wave" | "audio/x-wav" | "audio/vnd.wave") => Some("wav"),
        (Kind::Audio, "audio/mp4" | "audio/m4a" | "audio/x-m4a") => Some("m4a"),
        (Kind::Audio, "audio/aac" | "audio/aacp") => Some("aac"),
        (Kind::Audio, "audio/ogg") => Some("ogg"),
        (Kind::Audio, "audio/opus") => Some("opus"),
        (Kind::Audio, "audio/flac" | "audio/x-flac") => Some("flac"),
        (Kind::Audio, "audio/webm") => Some("webm"),
        _ => None,
    }
}

fn decode(data: &str, limit: usize) -> Option<Vec<u8>> {
    if data.is_empty() || data.len() > limit.div_ceil(3) * 4 {
        return None;
    }
    let bytes = STANDARD.decode(data).ok()?;
    (bytes.len() <= limit).then_some(bytes)
}

fn raster(encoded: &str, format: ImageFormat) -> Option<Arc<RenderImage>> {
    let bytes = decode(encoded, MAX_IMAGE_BYTES)?;
    if image::guess_format(&bytes).ok()? != format {
        return None;
    }
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(96 * 1024 * 1024);
    reader.limits(limits);
    let decoder = reader.into_decoder().ok()?;
    let (width, height) = decoder.dimensions();
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > 16_000_000 {
        return None;
    }
    let mut pixels = image::DynamicImage::from_decoder(decoder)
        .ok()?
        .into_rgba8();
    if width > 1280 || height > 1280 {
        pixels = image::DynamicImage::ImageRgba8(pixels)
            .thumbnail(1280, 1280)
            .into_rgba8();
    }
    for pixel in pixels.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    Some(Arc::new(RenderImage::new([image::Frame::new(pixels)])))
}

struct ImageLoad {
    encoded: Arc<str>,
    format: ImageFormat,
    result: Option<Option<Arc<RenderImage>>>,
    expanded: bool,
    _task: Task<()>,
}

impl ImageLoad {
    fn new(encoded: &str, format: ImageFormat, cx: &mut Context<Self>) -> Self {
        let encoded: Arc<str> = encoded.into();
        let input = encoded.clone();
        let decode = cx.background_spawn(async move { raster(&input, format) });
        let task = cx.spawn(async move |this, cx| {
            let result = decode.await;
            let _ = this.update(cx, |state, cx| {
                state.result = Some(result);
                cx.notify();
            });
        });
        Self {
            encoded,
            format,
            result: None,
            expanded: false,
            _task: task,
        }
    }
}

#[derive(Default)]
struct FileState {
    expanded: bool,
    saving: bool,
    status: Option<&'static str>,
}

fn save_file(path: &Path, block: &Block, text: bool) -> std::io::Result<()> {
    let invalid = || std::io::Error::from(std::io::ErrorKind::InvalidData);
    let directory = path.parent().ok_or_else(invalid)?;
    let mut file = tempfile::NamedTempFile::new_in(directory)?;
    if text {
        let text = block
            .text()
            .filter(|text| text.len() <= MAX_FILE_BYTES)
            .ok_or_else(invalid)?;
        file.write_all(text.as_bytes())?;
    } else {
        let bytes = block
            .encoded()
            .and_then(|data| decode(data, MAX_FILE_BYTES))
            .ok_or_else(invalid)?;
        file.write_all(&bytes)?;
    }
    file.persist(path)?;
    Ok(())
}

#[derive(IntoElement)]
struct BlockCard {
    source: Block,
    palette: Palette,
    keyboard: bool,
    project: Option<Arc<Path>>,
}

impl RenderOnce for BlockCard {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Some(value) = self.source.value() else {
            return div().into_any_element();
        };
        let mime = value["mimeType"].as_str().unwrap_or_default();
        let project_file = project_file_block(value);
        let filename = project_file
            .or_else(|| legacy_text_file(value))
            .map(|(name, _)| name)
            .or_else(|| value["name"].as_str())
            .or_else(|| value["filename"].as_str())
            .map(name)
            .filter(|name| !name.is_empty());
        let url = value["url"].as_str().and_then(markdown::safe_href);
        let kind = block_kind(value).unwrap_or_else(|| {
            let filename = filename.as_deref().unwrap_or_default();
            if project_file.is_some() {
                attachment_link_kind(filename).unwrap_or(Kind::File(FileFormat::Generic))
            } else {
                Kind::File(FileFormat::detect(filename, mime))
            }
        });
        let label = filename.as_deref().unwrap_or_else(|| kind.label(""));
        let palette = self.palette;
        if let Some((_, relative_path)) = project_file {
            return self
                .project_card(kind, label, relative_path, window, cx)
                .into_any_element();
        }
        if let Some(encoded) = self.source.encoded().filter(|_| kind == Kind::Image) {
            let Some(format) = image_format(mime) else {
                return self
                    .card(kind, label, "图片格式不受支持")
                    .into_any_element();
            };
            if encoded.len() > MAX_IMAGE_BYTES.div_ceil(3) * 4 {
                return self.card(kind, label, "图片超过 10 MiB").into_any_element();
            }
            let state = window.use_state(cx, |_, cx| ImageLoad::new(encoded, format, cx));
            // A streaming snapshot may replace a content block at the same position.
            let previous = state.read(cx);
            if previous.format != format || previous.encoded.as_ref() != encoded {
                state.update(cx, |state, cx| *state = ImageLoad::new(encoded, format, cx));
            }
            let current = state.read(cx);
            return match &current.result {
                None => self.card(kind, label, "正在读取图片").into_any_element(),
                Some(None) => self.card(kind, label, "图片无法预览").into_any_element(),
                Some(Some(image)) => {
                    let expanded = current.expanded;
                    let dimensions = image.size(0);
                    let h = if expanded { 360.0 } else { 180.0 };
                    let aspect = dimensions.width.0 as f32 / dimensions.height.0 as f32;
                    let width = (h * aspect).clamp(100.0, if expanded { 580.0 } else { 360.0 });
                    let picture = img(image.clone())
                        .w_full()
                        .h(px(h.min(width / aspect)))
                        .object_fit(ObjectFit::Contain);
                    card_frame(palette)
                        .w(px(width.max(220.0)))
                        .child(
                            card_title(kind, label, None, palette).child(
                                action(
                                    "image-preview",
                                    if expanded {
                                        "缩小图片"
                                    } else {
                                        "放大图片"
                                    },
                                    palette,
                                    self.keyboard,
                                )
                                .on_click(move |_, _, cx| {
                                    state.update(cx, |state, cx| {
                                        state.expanded = !state.expanded;
                                        cx.notify();
                                    })
                                }),
                            ),
                        )
                        .child(if expanded {
                            super::rich_text::ReadingScroll {
                                id: "attachment-image",
                                viewport: div()
                                    .min_w_0()
                                    .border_1()
                                    .border_color(palette.line)
                                    .child(div().w(px(width)).child(picture)),
                                keyboard: self.keyboard,
                                focus: palette.muted,
                            }
                            .into_any_element()
                        } else {
                            picture.into_any_element()
                        })
                        .into_any_element()
                }
            };
        }
        if (matches!(kind, Kind::File(_)) && self.source.text().is_some())
            || self.source.encoded().is_some() && kind != Kind::Image
        {
            return self
                .inline_file(kind, label, mime, window, cx)
                .into_any_element();
        }
        if let Some(url) = url {
            return link_card(kind, label, url, palette, self.keyboard).into_any_element();
        }
        self.card(kind, label, "没有可用的附件内容")
            .into_any_element()
    }
}

impl BlockCard {
    fn card(&self, kind: Kind, label: &str, status: &str) -> Div {
        card_frame(self.palette).w(px(360.0)).child(card_title(
            kind,
            label,
            Some(status),
            self.palette,
        ))
    }

    fn project_card(
        &self,
        kind: Kind,
        label: &str,
        relative_path: &str,
        window: &mut Window,
        cx: &mut App,
    ) -> Div {
        let palette = self.palette;
        let Some(root) = self.project.clone() else {
            return self.card(kind, label, "附件保存在原本机项目中");
        };
        let relative_path = relative_path.to_owned();
        let state = window.use_state(cx, |_, _| false);
        let missing = *state.read(cx);
        let open_attachment = matches!(kind, Kind::Audio | Kind::Video)
            || has_suffix(
                label,
                &[
                    "pdf", "xls", "xlsx", "csv", "doc", "docx", "ppt", "pptx", "odt", "ods", "odp",
                ],
            );
        card_frame(palette).w(px(400.0)).child(
            card_title(
                kind,
                label,
                Some(if missing {
                    "项目附件不可用"
                } else {
                    kind.label(label)
                }),
                palette,
            )
            .child(
                action(
                    "project-attachment",
                    if open_attachment {
                        kind.open_label()
                    } else {
                        "在文件夹中显示"
                    },
                    palette,
                    self.keyboard,
                )
                .on_click(move |_, _, cx| {
                    let path = root.canonicalize().ok().and_then(|root| {
                        let path = root.join(&relative_path);
                        let canonical = path.canonicalize().ok()?;
                        (canonical == path && path.is_file()).then_some(path)
                    });
                    state.update(cx, |missing, cx| {
                        *missing = path.is_none();
                        cx.notify();
                    });
                    if let Some(path) = path {
                        if open_attachment {
                            cx.open_with_system(&path);
                        } else {
                            cx.reveal_path(&path);
                        }
                    }
                }),
            ),
        )
    }

    fn inline_file(
        &self,
        kind: Kind,
        label: &str,
        mime: &str,
        window: &mut Window,
        cx: &mut App,
    ) -> Div {
        let palette = self.palette;
        let text = (matches!(kind, Kind::File(_)))
            .then(|| self.source.text())
            .flatten();
        let extension = playback_extension(kind, mime);
        let state = window.use_state(cx, |_, _| FileState::default());
        let current = state.read(cx);
        let open = current.expanded;
        let mut title = card_title(kind, label, Some(kind.label(label)), palette);
        if text.is_some() {
            let state = state.clone();
            title = title.child(
                action(
                    "file-preview",
                    if open { "收起" } else { "预览" },
                    palette,
                    self.keyboard,
                )
                .on_click(move |_, _, cx| {
                    state.update(cx, |state, cx| {
                        state.expanded = !state.expanded;
                        cx.notify();
                    })
                }),
            );
        }
        let mut suggested = if label == kind.label(label) {
            "attachment".to_owned()
        } else {
            label.to_owned()
        };
        let suggested_ext = extension.or(text.is_some().then_some("txt"));
        if let Some(ext) = suggested_ext
            && (Path::new(&suggested).extension().is_none()
                || extension.is_some() && !has_extension(Path::new(&suggested), ext))
        {
            suggested.push('.');
            suggested.push_str(ext);
        }
        let save_label = if extension.is_some() {
            "保存并播放"
        } else {
            "保存"
        };
        let source = self.source.clone();
        let is_text = text.is_some();
        let file_status = current.status;
        title = title.child(if current.saving {
            div()
                .flex_none()
                .text_size(px(12.0))
                .text_color(palette.muted)
                .child("正在保存…")
                .into_any_element()
        } else {
            action("file-save", save_label, palette, self.keyboard)
                .on_click(move |_, _, cx| {
                    state.update(cx, |state, cx| {
                        state.saving = true;
                        state.status = None;
                        cx.notify();
                    });
                    let directory = std::env::current_dir().unwrap_or_default();
                    let selected = cx.prompt_for_new_path(&directory, Some(&suggested));
                    let source = source.clone();
                    let executor = cx.background_executor().clone();
                    let state = state.downgrade();
                    cx.spawn(async move |cx| {
                        let result = match selected.await {
                            Ok(Ok(Some(path))) => {
                                let output = path.clone();
                                executor
                                    .spawn(async move { save_file(&output, &source, is_text) })
                                    .await
                                    .map(|()| Some(path))
                            }
                            Ok(Ok(None)) => Ok(None),
                            _ => Err(std::io::Error::other("选择保存位置失败")),
                        };
                        let _ = cx.update(|cx| {
                            let status = match result {
                                Ok(Some(path)) => match extension {
                                    Some(ext) if has_extension(&path, ext) => {
                                        cx.open_with_system(&path);
                                        Some("已保存")
                                    }
                                    Some(_) => Some("已保存，文件扩展名与媒体格式不符"),
                                    None => Some("已保存"),
                                },
                                Ok(None) => None,
                                Err(_) => Some("无法保存附件"),
                            };
                            let _ = state.update(cx, |state, cx| {
                                state.saving = false;
                                state.status = status;
                                cx.notify();
                            });
                        });
                    })
                    .detach();
                })
                .into_any_element()
        });
        card_frame(palette)
            .w(px(400.0))
            .child(title)
            .when_some(file_status, |card, status| {
                card.child(
                    div()
                        .px(px(10.0))
                        .pb(px(8.0))
                        .text_size(px(11.0))
                        .text_color(palette.muted)
                        .child(status),
                )
            })
            .when_some(text.filter(|_| open), |card, text| {
                let mut end = MAX_TEXT_PREVIEW.min(text.len());
                while !text.is_char_boundary(end) {
                    end -= 1;
                }
                let preview = div()
                    .border_1()
                    .border_color(palette.line)
                    .max_h(px(180.0))
                    .min_w_0()
                    .px(px(10.0))
                    .py(px(8.0))
                    .text_size(px(12.0))
                    .line_height(px(18.0))
                    .text_color(palette.text)
                    .child(text[..end].to_owned())
                    .when(end < text.len(), |view| {
                        view.child(div().text_color(palette.muted).child("仅预览前 32 KiB"))
                    });
                card.child(super::rich_text::ReadingScroll {
                    id: "attachment-text",
                    viewport: preview,
                    keyboard: self.keyboard,
                    focus: palette.muted,
                })
            })
    }
}

fn has_extension(path: &Path, extension: &str) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case(extension))
}

fn card_frame(palette: Palette) -> Div {
    div()
        .max_w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .border_1()
        .border_color(palette.line)
        .bg(palette.subtle)
        .rounded(px(10.0))
        .overflow_hidden()
}

fn card_title(kind: Kind, label: &str, subtitle: Option<&str>, palette: Palette) -> Div {
    let accent = kind.accent(palette);
    div()
        .flex()
        .items_center()
        .min_w_0()
        .gap(px(8.0))
        .px(px(10.0))
        .py(px(7.0))
        .child(
            div()
                .flex_none()
                .size(px(28.0))
                .rounded(px(6.0))
                .bg(accent.map_or(palette.hover, |accent| {
                    palette.subtle.blend(accent.opacity(0.14))
                }))
                .flex()
                .items_center()
                .justify_center()
                .child(icon(kind.icon(), 15.0, accent.unwrap_or(palette.text))),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .child(
                    div()
                        .truncate()
                        .text_size(px(13.0))
                        .text_color(palette.text)
                        .font_weight(FontWeight::MEDIUM)
                        .child(label.to_owned()),
                )
                .when_some(
                    subtitle.filter(|subtitle| *subtitle != label),
                    |column, subtitle| {
                        column.child(
                            div()
                                .text_size(px(11.0))
                                .text_color(accent.unwrap_or(palette.muted))
                                .child(subtitle.to_owned()),
                        )
                    },
                ),
        )
}

fn action(
    id: &'static str,
    label: &'static str,
    palette: Palette,
    keyboard: bool,
) -> gpui::Stateful<Div> {
    div()
        .id(id)
        .flex_none()
        .flex()
        .items_center()
        .h(px(29.0))
        .px(px(6.0))
        .rounded(px(5.0))
        .cursor_pointer()
        .tab_index(0)
        .tab_stop(keyboard)
        .focus(move |button| button.bg(palette.hover))
        .hover(move |button| button.bg(palette.hover))
        .text_size(px(12.0))
        .text_color(palette.text)
        .child(label)
}

fn link_card(kind: Kind, label: &str, url: String, palette: Palette, keyboard: bool) -> Div {
    card_frame(palette).w(px(360.0)).child(
        card_title(kind, label, Some(kind.label(label)), palette).child(
            action("attachment-open", kind.open_label(), palette, keyboard)
                .on_click(move |_, _, cx| cx.open_url(&url)),
        ),
    )
}

/// Preserve ordinary links in prose and give standalone download links a card.
pub(super) fn markdown_link(children: &[Node], palette: Palette, keyboard: bool) -> Option<Div> {
    let [
        Node::Link {
            href: Some(url),
            children,
        },
    ] = children
    else {
        return None;
    };
    let path = url
        .split(['?', '#'])
        .next()?
        .split_once("://")?
        .1
        .split_once('/')?
        .1;
    let filename = path.rsplit('/').next()?;
    let kind = attachment_link_kind(filename)?;
    let label = match children.as_slice() {
        [Node::Text { text }] if !text.trim().is_empty() => text,
        _ => filename,
    };
    Some(link_card(kind, label, url.clone(), palette, keyboard))
}

fn attachment_link_kind(filename: &str) -> Option<Kind> {
    let (_, ext) = filename.rsplit_once('.')?;
    Some(match ext.to_ascii_lowercase().as_str() {
        "mp4" | "webm" | "mov" | "ogv" | "m4v" => Kind::Video,
        "mp3" | "wav" | "m4a" | "aac" | "ogg" | "oga" | "opus" | "flac" => Kind::Audio,
        ext => {
            let format = FileFormat::detect(filename, "");
            if format == FileFormat::Generic && !matches!(ext, "txt" | "md") {
                return None;
            }
            Kind::File(format)
        }
    })
}

impl Shell {
    pub(super) fn attachment_cards(&self, attachments: &[ContentRef], max_width: f32) -> Div {
        let palette = self.palette;
        let keyboard = self.navigation_keyboard_enabled() && self.overlay.is_none();
        let project = self
            .node
            .as_ref()
            .filter(|node| node.is_local())
            .and_then(|_| {
                self.project
                    .and_then(|index| self.snapshot.projects.get(index))
                    .map(|project| Arc::<Path>::from(Path::new(&project.path)))
            });
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .min_w_0()
            .max_w(px(max_width))
            .children(attachments.iter().enumerate().map(|(index, reference)| {
                div()
                    .id(("attachment", index))
                    .max_w_full()
                    .min_w_0()
                    .child(BlockCard {
                        source: Block {
                            history: self.transcript_history.clone(),
                            reference: *reference,
                        },
                        palette,
                        keyboard,
                        project: project.clone(),
                    })
            }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use work_viewmodel::{
        conversation::transcript_view::{self, Part},
        transcript::TranscriptOrder,
    };

    #[test]
    fn sent_text_attachment_previews_and_saves_its_original_contents() {
        let history = Arc::new(vec![json!({"role":"user", "content":[
            {"type":"text", "text":"Please review"},
            {"type":"text", "text":"\n\n附件 · notes.md\n# Example\n资料\n"},
        ]})]);
        let rows = TranscriptOrder::default().update("session", &history, &[]);
        let view = transcript_view::present_with_attachments(
            &rows,
            &history,
            &[],
            None,
            None,
            Default::default(),
            Some(claims),
        );
        let [
            Part::Message {
                text, attachments, ..
            },
        ] = view.parts.as_slice()
        else {
            panic!("expected a user message with its attachment");
        };
        assert_eq!(text, "Please review");
        let [reference] = attachments.as_slice() else {
            panic!("expected one attachment")
        };
        let block = Block {
            history,
            reference: *reference,
        };
        assert_eq!(block.text(), Some("# Example\n资料\n"));
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("notes.md");
        save_file(&output, &block, true).unwrap();
        assert!(save_file(&output, &block, false).is_err());
        assert_eq!(
            std::fs::read_to_string(output).unwrap(),
            "# Example\n资料\n"
        );
    }

    #[test]
    fn audio_blocks_keep_their_bytes_and_use_audio_playback_extensions() {
        let bytes = b"RIFF\x24\x00\x00\x00WAVEfmt \x10\x00\x00\x00\x01\x00\x01\x00\x40\x1f\x00\x00\x80\x3e\x00\x00\x02\x00\x10\x00data\x00\x00\x00\x00";
        let history = Arc::new(vec![json!({"role":"assistant", "content":[
            {"type":"audio", "mimeType":"audio/wav", "data": STANDARD.encode(bytes)}
        ]})]);
        let rows = TranscriptOrder::default().update("session", &history, &[]);
        let view = transcript_view::present_with_attachments(
            &rows,
            &history,
            &[],
            None,
            None,
            Default::default(),
            Some(claims),
        );
        let [Part::Message { attachments, .. }] = view.parts.as_slice() else {
            panic!("expected an audio-only message");
        };
        let [reference] = attachments.as_slice() else {
            panic!("expected one audio block")
        };
        let block = Block {
            history,
            reference: *reference,
        };
        assert!(block_kind(block.value().unwrap()) == Some(Kind::Audio));
        assert!(
            block_kind(&json!({"type":"attachment", "mimeType":"audio/mpeg"})) == Some(Kind::Audio)
        );
        assert_eq!(
            playback_extension(Kind::Audio, "audio/webm; codecs=opus"),
            Some("webm")
        );
        assert_eq!(playback_extension(Kind::Audio, "video/webm"), None);
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("clip.wav");
        save_file(&output, &block, false).unwrap();
        assert_eq!(std::fs::read(output).unwrap(), bytes);
    }

    #[test]
    fn raster_preview_checks_the_declared_format_and_bounds() {
        let mut png = Cursor::new(Vec::new());
        image::RgbaImage::from_pixel(2, 2, image::Rgba([240, 100, 20, 255]))
            .write_to(&mut png, ImageFormat::Png)
            .unwrap();
        let encoded = STANDARD.encode(png.into_inner());
        let preview = raster(&encoded, ImageFormat::Png).unwrap();
        assert_eq!(preview.as_bytes(0).unwrap()[..4], [20, 100, 240, 255]);
        assert!(raster(&encoded, ImageFormat::Jpeg).is_none());
        assert!(raster("data:image/png;base64,....", ImageFormat::Png).is_none());
        assert!(decode("AAAA", 2).is_none());
    }
}
