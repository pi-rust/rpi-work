//! Theme tokens for the prototype, taken from `work-viewmodel::theme`.
//!
//! The Dioxus client injects these tokens as CSS variables; a native renderer
//! can parse the same map into colors. Nothing here duplicates the palette.

use gpui::{Hsla, rgb, rgba};
use work_viewmodel::theme::theme_contract;

#[derive(Clone, Copy)]
pub struct Palette {
    pub bg: Hsla,
    pub side: Hsla,
    pub card: Hsla,
    pub text: Hsla,
    pub muted: Hsla,
    pub line: Hsla,
    pub accent: Hsla,
    pub success: Hsla,
    pub danger: Hsla,
    /// Code token colors, mapped from the same token contract.
    pub keyword: Hsla,
    pub string: Hsla,
    pub comment: Hsla,
    pub number: Hsla,
    pub function: Hsla,
    pub plain: Hsla,
}

impl Palette {
    /// Build the Codex light palette exactly as the shipped client does.
    pub fn codex_light() -> Self {
        let theme = theme_contract::presets()
            .into_iter()
            .find(|theme| theme.id == "codex")
            .expect("codex preset");
        let tokens = theme_contract::build_tokens(&theme, false, false);
        let color = |key: &str, fallback: u32| {
            tokens
                .get(key)
                .and_then(|value| parse_hex(value))
                .unwrap_or_else(|| rgb(fallback).into())
        };
        let accent = color("accent", 0x202020);
        Self {
            bg: color("bg", 0xffffff),
            side: color("side", 0xf7f7f7),
            card: color("card", 0xffffff),
            text: color("text", 0x202020),
            muted: color("muted", 0x6b6b6b),
            line: color("line", 0xe5e5e5),
            accent,
            success: color("success", 0x2d694d),
            danger: color("danger", 0xa03c35),
            keyword: accent,
            string: color("success", 0x2d694d),
            comment: color("muted", 0x6b6b6b),
            number: color("info", 0x285f90),
            function: color("text", 0x202020),
            plain: color("body-text", 0x2b2b2b),
        }
    }

    /// Token color for a lexer class name from `conversation::code_tokens`.
    pub fn code_token(&self, class: &str) -> Hsla {
        match class {
            "code-token-keyword" => self.keyword,
            "code-token-string" | "code-token-string-special" => self.string,
            "code-token-comment" => self.comment,
            "code-token-number" | "code-token-literal" => self.number,
            "code-token-function" | "code-token-type" => self.function,
            _ => self.plain,
        }
    }
}

fn parse_hex(value: &str) -> Option<Hsla> {
    let hex = value.trim().strip_prefix('#')?;
    let byte = |range: std::ops::Range<usize>| {
        hex.get(range)
            .and_then(|part| u8::from_str_radix(part, 16).ok())
            .unwrap_or(0)
    };
    match hex.len() {
        6 => Some(rgba(u32::from_be_bytes([0, byte(0..2), byte(2..4), byte(4..6)])).into()),
        8 => Some(
            rgba(u32::from_be_bytes([
                byte(0..2),
                byte(2..4),
                byte(4..6),
                byte(6..8),
            ]))
            .into(),
        ),
        _ => None,
    }
}
