//! Lucide 0.468.0 marks from the confirmed prototype. See `assets/icons/LICENSE`.

use std::borrow::Cow;

use gpui::{AssetSource, Hsla, SharedString, Svg, prelude::*, px, svg};

pub struct Icons;

impl AssetSource for Icons {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        let shapes = match path {
            "history-back" => r#"<path d="m11 5-7 7 7 7M4 12h16"/>"#,
            "history-forward" => r#"<path d="m13 5 7 7-7 7M20 12H4"/>"#,
            "panel-left" => {
                r#"<g transform="translate(24 0) scale(-1 1)"><rect width="18" height="18" x="3" y="3" rx="2"/><path d="M15 3v18"/></g>"#
            }
            "panel-right" => {
                r#"<rect width="18" height="18" x="3" y="3" rx="2"/><path d="M15 3v18"/>"#
            }
            "terminal" => {
                r#"<polyline points="4 17 10 11 4 5"/><line x1="12" x2="20" y1="19" y2="19"/>"#
            }
            "folder" => {
                r#"<path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z"/>"#
            }
            "globe" => {
                r#"<circle cx="12" cy="12" r="10"/><path d="M12 2a14.5 14.5 0 0 0 0 20 14.5 14.5 0 0 0 0-20"/><path d="M2 12h20"/>"#
            }
            "code-xml" => {
                r#"<path d="m18 16 4-4-4-4"/><path d="m6 8-4 4 4 4"/><path d="m14.5 4-5 16"/>"#
            }
            "blocks" => {
                r#"<rect width="7" height="7" x="14" y="3" rx="1"/><path d="M10 21V8a1 1 0 0 0-1-1H4a1 1 0 0 0-1 1v12a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-5a1 1 0 0 0-1-1H3"/>"#
            }
            "flag" => r#"<path d="M4 22V3m0 1c6-5 10 5 16 0v11c-6 5-10-5-16 0"/>"#,
            "file-diff" => {
                r#"<path d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z"/><path d="M9 10h6"/><path d="M12 13V7"/><path d="M9 17h6"/>"#
            }
            "file-text" => {
                r#"<path d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z"/><path d="M14 2v6h6M8 13h8m-8 4h8"/>"#
            }
            "image" => {
                r#"<rect x="3" y="3" width="18" height="18" rx="2"/><circle cx="9" cy="9" r="2"/><path d="m21 15-4-4L7 21"/>"#
            }
            "play" => r#"<path d="M8 5v14l11-7Z"/>"#,
            "file-pdf" => {
                r#"<path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8zm0 0v6h6M8 17c2-1 4-4 4-7 0 2 1 5 4 6-3-1-6 0-8 1Z"/>"#
            }
            "file-spreadsheet" => {
                r#"<path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8zm0 0v6h6"/><rect x="7" y="11" width="10" height="7" rx="1"/><path d="M12 11v7m-5-3.5h10"/>"#
            }
            "presentation" => {
                r#"<rect x="3" y="4" width="18" height="13" rx="2"/><path d="m7 13 3-3 3 2 4-4m-5 9v4m-3 0h6"/>"#
            }
            "headphones" => {
                r#"<path d="M3 14v-2a9 9 0 0 1 18 0v2"/><rect x="3" y="13" width="4" height="7" rx="1.5"/><rect x="17" y="13" width="4" height="7" rx="1.5"/>"#
            }
            "ellipsis" => {
                r#"<circle cx="5" cy="12" r="1"/><circle cx="12" cy="12" r="1"/><circle cx="19" cy="12" r="1"/>"#
            }
            "pencil" => r#"<path d="m14 6 4 4M3 21l1-5L16 4a2.8 2.8 0 0 1 4 4L8 20z"/>"#,
            "info" => r#"<circle cx="12" cy="12" r="9"/><path d="M12 11v5m0-8h.01"/>"#,
            "pin" => {
                r#"<path d="M16 9V4h1a1 1 0 0 0 0-2H7a1 1 0 0 0 0 2h1v5l-3 4v2h14v-2zM12 15v7"/>"#
            }
            "plus" => r#"<path d="M12 5v14M5 12h14"/>"#,
            "archive-box" => {
                r#"<rect width="20" height="5" x="2" y="3" rx="1"/><path d="M4 8v12h16V8M10 12h4"/>"#
            }
            "keyboard" => {
                r#"<rect width="20" height="14" x="2" y="5" rx="2"/><path d="M6 9h.01M10 9h.01M14 9h.01M18 9h.01M6 13h.01M10 13h.01M14 13h.01M18 13h.01M8 16h8"/>"#
            }
            "bell" => {
                r#"<path d="M10.268 21a2 2 0 0 0 3.464 0M3.262 15.326A1 1 0 0 0 4 17h16a1 1 0 0 0 .74-1.673C19.41 13.956 18 12.499 18 8A6 6 0 0 0 6 8c0 4.499-1.411 5.956-2.738 7.326"/>"#
            }
            "message-circle" => r#"<path d="M7.9 20a9 9 0 1 0-3.9-3.9L2 22Z"/>"#,
            "search" => r#"<circle cx="11" cy="11" r="8"/><path d="m21 21-4.35-4.35"/>"#,
            "appearance" => r#"<circle cx="12" cy="12" r="9"/><path d="M12 3v18"/>"#,
            "chevron-right" => r#"<path d="m9 6 6 6-6 6"/>"#,
            "chevron-down" => r#"<path d="m6 9 6 6 6-6"/>"#,
            "check" => r#"<path d="m5 12 4 4L19 6"/>"#,
            "x" => r#"<path d="M18 6 6 18"/><path d="m6 6 12 12"/>"#,
            "arrow-up" => r#"<path d="m5 12 7-7 7 7"/><path d="M12 19V5"/>"#,
            "square" => r#"<rect x="5" y="5" width="14" height="14" rx="2"/>"#,
            "settings" => {
                r#"<path d="M20 7h-9"/><path d="M14 17H5"/><circle cx="17" cy="17" r="3"/><circle cx="7" cy="7" r="3"/>"#
            }
            "list-checks" => r#"<path d="m3 7 2 2 3-3M3 17l2 2 3-3M12 8h9m-9 10h9"/>"#,
            "git-branch" => {
                r#"<line x1="6" x2="6" y1="3" y2="15"/><circle cx="18" cy="6" r="3"/><circle cx="6" cy="18" r="3"/><path d="M18 9a9 9 0 0 1-9 9"/>"#
            }
            "copy" => {
                r#"<rect width="14" height="14" x="8" y="8" rx="2" ry="2"/><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"/>"#
            }
            "thumbs-up" => {
                r#"<path d="M7 10v12M15 5.88 14 10h5.83a2 2 0 0 1 1.92 2.56l-2.33 8a2 2 0 0 1-1.92 1.44H4a2 2 0 0 1-2-2v-8a2 2 0 0 1 2-2h2.76a2 2 0 0 0 1.79-1.11L12 2a3.13 3.13 0 0 1 3 3.88Z"/>"#
            }
            "thumbs-down" => {
                r#"<g transform="translate(24 24) rotate(180)"><path d="M7 10v12M15 5.88 14 10h5.83a2 2 0 0 1 1.92 2.56l-2.33 8a2 2 0 0 1-1.92 1.44H4a2 2 0 0 1-2-2v-8a2 2 0 0 1 2-2h2.76a2 2 0 0 0 1.79-1.11L12 2a3.13 3.13 0 0 1 3 3.88Z"/></g>"#
            }
            "laptop" => {
                r#"<path d="M20 16V7a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v9m-2 0h20v2a1 1 0 0 1-1 1H3a1 1 0 0 1-1-1Z"/>"#
            }
            "workspace" => {
                r#"<rect x="3" y="4" width="18" height="13" rx="2"/><path d="M8 21h8m-4-4v4"/>"#
            }
            "compose" => {
                r#"<path d="M12 4H6a3 3 0 0 0-3 3v11a3 3 0 0 0 3 3h11a3 3 0 0 0 3-3v-6M10 14l1-4 8-8 3 3-8 8z"/>"#
            }
            _ => return Ok(None),
        };
        Ok(Some(Cow::Owned(format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="1.65" stroke-linecap="round" stroke-linejoin="round">{shapes}</svg>"#
        ).into_bytes())))
    }

    fn list(&self, _: &str) -> gpui::Result<Vec<SharedString>> {
        Ok(Vec::new())
    }
}

pub fn icon(path: &'static str, size: f32, color: Hsla) -> Svg {
    svg()
        .path(path)
        .size(px(size))
        .flex_none()
        .text_color(color)
}
