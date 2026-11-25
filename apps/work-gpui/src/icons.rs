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
            "file-diff" => {
                r#"<path d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z"/><path d="M9 10h6"/><path d="M12 13V7"/><path d="M9 17h6"/>"#
            }
            "plus" => r#"<path d="M12 5v14M5 12h14"/>"#,
            "chevron-down" => r#"<path d="m6 9 6 6 6-6"/>"#,
            "x" => r#"<path d="M18 6 6 18"/><path d="m6 6 12 12"/>"#,
            "arrow-up" => r#"<path d="m5 12 7-7 7 7"/><path d="M12 19V5"/>"#,
            "square" => r#"<rect x="5" y="5" width="14" height="14" rx="2"/>"#,
            "settings" => {
                r#"<path d="M20 7h-9"/><path d="M14 17H5"/><circle cx="17" cy="17" r="3"/><circle cx="7" cy="7" r="3"/>"#
            }
            "git-branch" => {
                r#"<line x1="6" x2="6" y1="3" y2="15"/><circle cx="18" cy="6" r="3"/><circle cx="6" cy="18" r="3"/><path d="M18 9a9 9 0 0 1-9 9"/>"#
            }
            "copy" => {
                r#"<rect width="14" height="14" x="8" y="8" rx="2" ry="2"/><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"/>"#
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
