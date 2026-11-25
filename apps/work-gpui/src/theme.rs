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
    pub titlebar: Hsla,
    pub subtle: Hsla,
    pub hover: Hsla,
    pub strong_line: Hsla,
    pub text: Hsla,
    pub muted: Hsla,
    pub line: Hsla,
    pub success: Hsla,
    pub danger: Hsla,
    pub diff_add_bg: Hsla,
    pub diff_remove_bg: Hsla,
    /// Code token colors, mapped from the same token contract.
    pub keyword: Hsla,
    pub string: Hsla,
    pub comment: Hsla,
    pub number: Hsla,
    pub function: Hsla,
    pub plain: Hsla,
}

impl Palette {
    /// Build any preset exactly as the shipped client does.
    pub fn from_preset(id: &str, dark: bool, compact: bool) -> Self {
        let theme = theme_contract::presets()
            .into_iter()
            .find(|theme| theme.id == id)
            .or_else(|| theme_contract::presets().into_iter().next())
            .expect("at least one preset");
        let tokens = theme_contract::build_tokens(&theme, dark, compact);
        let color = |key: &str, fallback: u32| {
            tokens
                .get(key)
                .and_then(|value| parse_hex(value))
                .unwrap_or_else(|| rgb(fallback).into())
        };
        let chrome = |key, light, dark_value| {
            if id == "codex" {
                rgb(if dark { dark_value } else { light }).into()
            } else {
                color(key, light)
            }
        };
        let accent = color("accent", 0x202020);
        Self {
            bg: color("bg", 0xffffff),
            side: chrome("side", 0xfafafa, 0x1b1b1b),
            titlebar: chrome("accent-bg", 0xf0f0f0, 0x191919),
            subtle: chrome("code", 0xf2f2f2, 0x2c2c2c),
            hover: color("selected-bg", 0xeaeaea),
            strong_line: color("line-strong", 0xd8d8d8),
            text: color("text", 0x202020),
            muted: color("muted", 0x6b6b6b),
            line: chrome("line", 0xe3e3e3, 0x373737),
            success: color("success", 0x2d694d),
            danger: color("danger", 0xa03c35),
            diff_add_bg: chrome("diff-add-bg", 0xedf8ef, 0x263c2d),
            diff_remove_bg: chrome("diff-del-bg", 0xfff0f0, 0x412c2c),
            keyword: accent,
            string: color("success", 0x2d694d),
            comment: color("muted", 0x6b6b6b),
            number: color("info", 0x285f90),
            function: color("text", 0x202020),
            plain: color("body-text", 0x2b2b2b),
        }
    }

    /// Preset ids and names for the header chip.
    pub fn presets() -> Vec<(String, String)> {
        theme_contract::presets()
            .into_iter()
            .map(|theme| (theme.id, theme.name))
            .collect()
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
        // `rgb` takes 0xRRGGBB; `rgba` takes 0xRRGGBBAA. Feeding an RGB triple to
        // `rgba` reads it as 0x00RRGGBB, which turns white into cyan.
        6 => Some(
            rgb((u32::from(byte(0..2)) << 16)
                | (u32::from(byte(2..4)) << 8)
                | u32::from(byte(4..6)))
            .into(),
        ),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_tokens_keep_their_channels() {
        // #ffffff must stay white; a stray alpha read made the whole window cyan.
        assert_eq!(parse_hex("#ffffff"), Some(rgb(0xffffff).into()));
        assert_eq!(parse_hex("#202020"), Some(rgb(0x202020).into()));
        assert_eq!(
            parse_hex("  #e5e5e5 "),
            Some(rgb(0xe5e5e5).into()),
            "surrounding whitespace is tolerated"
        );
        assert_eq!(
            parse_hex("#17252f55"),
            Some(rgba(0x17252f55).into()),
            "8-digit tokens keep their alpha"
        );
    }

    #[test]
    fn codex_light_palette_is_not_cyan() {
        let palette = Palette::from_preset("codex", false, false);
        assert_eq!(palette.bg, rgb(0xffffff).into());
        assert_eq!(palette.text, rgb(0x1f1f1f).into());
    }
}
