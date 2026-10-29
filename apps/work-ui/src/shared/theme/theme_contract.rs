use serde_json::{Map, Value, json};
use std::{collections::BTreeMap, fmt::Write as _};

const THEME_SCHEMA_VERSION: u64 = 1;
pub const THEME_KIND: &str = "rpi-work-theme";
pub const MAX_CUSTOM_THEMES: usize = 12;
pub const MAX_THEME_JSON_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, PartialEq)]
pub struct Palette {
    pub bg: String,
    pub side: String,
    pub panel: String,
    pub card: String,
    pub text: String,
    pub muted: String,
    pub line: String,
    pub code: String,
    pub accent: String,
    pub accent_bg: String,
    pub selected: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub id: String,
    pub name: String,
    pub description: String,
    pub font: String,
    pub heading_font: String,
    pub shape: String,
    pub light: Palette,
    pub dark: Palette,
    pub brand: Option<String>,
}

const PALETTE_KEYS: [&str; 11] = [
    "bg", "side", "panel", "card", "text", "muted", "line", "code", "accent", "accentBg",
    "selected",
];
const THEME_KEYS: [&str; 8] = [
    "id",
    "name",
    "description",
    "font",
    "headingFont",
    "shape",
    "palettes",
    "brand",
];

fn exact_keys(object: &Map<String, Value>, allowed: &[&str], required: &[&str]) -> bool {
    object.keys().all(|key| allowed.contains(&key.as_str()))
        && required.iter().all(|key| object.contains_key(*key))
}

fn string_field(object: &Map<String, Value>, key: &str, max: usize) -> Result<String, String> {
    let value = object.get(key).and_then(Value::as_str).unwrap_or("");
    if value.trim().is_empty() || value.encode_utf16().count() > max {
        return Err("主题格式不正确，请使用版本 1 的主题 JSON。".into());
    }
    Ok(value.to_owned())
}

fn hex(value: &str) -> bool {
    value.len() == 7
        && value.as_bytes()[0] == b'#'
        && value[1..].bytes().all(|b| b.is_ascii_hexdigit())
}

fn parse_palette(value: &Value) -> Result<Palette, String> {
    let object = value
        .as_object()
        .ok_or("主题必须包含完整的浅色和深色颜色配置。")?;
    if !exact_keys(object, &PALETTE_KEYS, &PALETTE_KEYS) {
        return Err("主题必须包含完整的浅色和深色颜色配置。".into());
    }
    let get = |key: &str| -> Result<String, String> {
        let value = object.get(key).and_then(Value::as_str).unwrap_or("");
        if hex(value) {
            Ok(value.to_owned())
        } else {
            Err("主题必须包含完整的浅色和深色颜色配置。".into())
        }
    };
    Ok(Palette {
        bg: get("bg")?,
        side: get("side")?,
        panel: get("panel")?,
        card: get("card")?,
        text: get("text")?,
        muted: get("muted")?,
        line: get("line")?,
        code: get("code")?,
        accent: get("accent")?,
        accent_bg: get("accentBg")?,
        selected: get("selected")?,
    })
}

fn channel(value: &str, at: usize) -> f64 {
    f64::from(u8::from_str_radix(&value[at..at + 2], 16).unwrap_or(0)) / 255.0
}

pub fn contrast(a: &str, b: &str) -> f64 {
    let luminance = |value: &str| {
        [1usize, 3, 5]
            .map(|at| channel(value, at))
            .map(|v| {
                if v <= 0.04045 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }
            })
            .iter()
            .zip([0.2126, 0.7152, 0.0722])
            .map(|(v, weight)| v * weight)
            .sum::<f64>()
    };
    let (x, y) = (luminance(a), luminance(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

fn validate_contrast(palette: &Palette, dark: bool) -> Result<(), String> {
    for background in [
        &palette.bg,
        &palette.side,
        &palette.panel,
        &palette.card,
        &palette.code,
    ] {
        if contrast(&palette.text, background) < 4.5 || contrast(&palette.muted, background) < 4.5 {
            return Err("主题文字对比度不足，请调整文字、强调色和背景色至至少 4.5:1。".into());
        }
    }
    let on_accent = if dark { "#1c1c20" } else { "#ffffff" };
    if contrast(&palette.accent, &palette.card) < 4.5 || contrast(&palette.accent, on_accent) < 4.5
    {
        return Err("主题文字对比度不足，请调整文字、强调色和背景色至至少 4.5:1。".into());
    }
    Ok(())
}

pub fn parse_theme(value: &Value) -> Result<Theme, String> {
    let object = value
        .as_object()
        .ok_or("主题格式不正确，请使用版本 1 的主题 JSON。")?;
    if !exact_keys(
        object,
        &THEME_KEYS,
        &[
            "id",
            "name",
            "description",
            "font",
            "headingFont",
            "shape",
            "palettes",
        ],
    ) {
        return Err("主题格式不正确，请使用版本 1 的主题 JSON。".into());
    }
    let id = string_field(object, "id", 64)?;
    if !id.bytes().next().is_some_and(|b| b.is_ascii_lowercase())
        || !id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return Err("主题格式不正确，请使用版本 1 的主题 JSON。".into());
    }
    let font = string_field(object, "font", 16)?;
    let heading_font = string_field(object, "headingFont", 16)?;
    if !["system", "humanist", "code"].contains(&font.as_str())
        || !["system", "humanist", "code"].contains(&heading_font.as_str())
    {
        return Err("主题格式不正确，请使用版本 1 的主题 JSON。".into());
    }
    let shape = string_field(object, "shape", 16)?;
    if !["soft", "crisp", "square"].contains(&shape.as_str()) {
        return Err("主题格式不正确，请使用版本 1 的主题 JSON。".into());
    }
    let palettes = object
        .get("palettes")
        .and_then(Value::as_object)
        .ok_or("主题必须包含完整的浅色和深色颜色配置。")?;
    if !exact_keys(palettes, &["light", "dark"], &["light", "dark"]) {
        return Err("主题必须包含完整的浅色和深色颜色配置。".into());
    }
    let light = parse_palette(palettes.get("light").unwrap())?;
    let dark = parse_palette(palettes.get("dark").unwrap())?;
    validate_contrast(&light, false)?;
    validate_contrast(&dark, true)?;
    let brand = match object.get("brand") {
        None => None,
        Some(value) if value.as_str().is_some_and(hex) => Some(value.as_str().unwrap().to_owned()),
        _ => return Err("品牌色必须是六位十六进制颜色。".into()),
    };
    Ok(Theme {
        id,
        name: string_field(object, "name", 40)?,
        description: string_field(object, "description", 160)?,
        font,
        heading_font,
        shape,
        light,
        dark,
        brand,
    })
}

fn palette_value(p: &Palette) -> Value {
    json!({"bg":p.bg,"side":p.side,"panel":p.panel,"card":p.card,"text":p.text,"muted":p.muted,"line":p.line,"code":p.code,"accent":p.accent,"accentBg":p.accent_bg,"selected":p.selected})
}

pub fn theme_value(theme: &Theme) -> Value {
    let mut value = json!({"id":theme.id,"name":theme.name,"description":theme.description,"font":theme.font,"headingFont":theme.heading_font,"shape":theme.shape,"palettes":{"light":palette_value(&theme.light),"dark":palette_value(&theme.dark)}});
    if let Some(brand) = &theme.brand {
        value
            .as_object_mut()
            .unwrap()
            .insert("brand".into(), Value::String(brand.clone()));
    }
    value
}

fn palette(values: [&str; 11]) -> Palette {
    Palette {
        bg: values[0].into(),
        side: values[1].into(),
        panel: values[2].into(),
        card: values[3].into(),
        text: values[4].into(),
        muted: values[5].into(),
        line: values[6].into(),
        code: values[7].into(),
        accent: values[8].into(),
        accent_bg: values[9].into(),
        selected: values[10].into(),
    }
}
fn theme(
    id: &str,
    name: &str,
    description: &str,
    typography: [&str; 3],
    light: Palette,
    dark: Palette,
    brand: Option<&str>,
) -> Theme {
    Theme {
        id: id.into(),
        name: name.into(),
        description: description.into(),
        font: typography[0].into(),
        heading_font: typography[1].into(),
        shape: typography[2].into(),
        light,
        dark,
        brand: brand.map(str::to_owned),
    }
}

pub fn presets() -> Vec<Theme> {
    vec![
        theme(
            "paper",
            "纸与苔绿",
            "暖白底色、赤陶强调，柔和圆角。",
            ["humanist", "humanist", "soft"],
            palette([
                "#fcfcfa", "#f4f4f0", "#f8f8f5", "#ffffff", "#282b27", "#62685d", "#e1e4da",
                "#f1f2ee", "#b14b2a", "#faeee6", "#e9ecdf",
            ]),
            palette([
                "#22261f", "#1d211b", "#272c23", "#292f24", "#e1e5d9", "#afb8a4", "#46503d",
                "#313a2a", "#f2ab82", "#413228", "#35402d",
            ]),
            None,
        ),
        theme(
            "ocean",
            "海湾蓝",
            "冷白与海蓝，清晰边界和利落轮廓。",
            ["system", "system", "crisp"],
            palette([
                "#f7fafd", "#edf3f8", "#eff5fa", "#ffffff", "#1d3246", "#526a7b", "#d9e4ed",
                "#e8f0f6", "#1267a1", "#e4f1fb", "#dcecf8",
            ]),
            palette([
                "#101f2d", "#0d1925", "#142738", "#182e41", "#dfedf8", "#a2bbcf", "#31516b",
                "#1e384c", "#7fc7f5", "#1e405a", "#24445e",
            ]),
            None,
        ),
        theme(
            "graphite",
            "石墨紫",
            "中性灰紫、方正细节与等宽标题。",
            ["system", "code", "square"],
            palette([
                "#f9f9fc", "#f0eff5", "#f3f2f8", "#ffffff", "#2f293d", "#6d637c", "#e1dce9",
                "#eceaf3", "#7550b3", "#eee6fa", "#e9e2f4",
            ]),
            palette([
                "#1e1c26", "#191720", "#25222f", "#292634", "#ebe6f3", "#bdb1ce", "#494057",
                "#342e43", "#c8a6f5", "#423252", "#3d344c",
            ]),
            None,
        ),
        theme(
            "reddit",
            "Reddit 橙",
            "橙红强调、冷灰表面与炭黑夜间配色。",
            ["system", "system", "soft"],
            palette([
                "#ffffff", "#f3f5f6", "#f8f9fa", "#ffffff", "#0f1a1c", "#52646a", "#d6dde0",
                "#e9eef0", "#c93600", "#fff0e8", "#e6ecef",
            ]),
            palette([
                "#0e1113", "#0b1416", "#131b1f", "#181c1f", "#eef1f3", "#adbac1", "#34444b",
                "#20272b", "#ff6a33", "#3b211a", "#253035",
            ]),
            Some("#ff4500"),
        ),
    ]
}

// Inputs are validated hex channels and all callers use weights in [0, 1].
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn mix(a: &str, b: &str, weight: f64) -> String {
    let mut out = String::from("#");
    for at in [1usize, 3, 5] {
        let x = f64::from(u8::from_str_radix(&a[at..at + 2], 16).unwrap_or(0));
        let y = f64::from(u8::from_str_radix(&b[at..at + 2], 16).unwrap_or(0));
        let _ = write!(
            out,
            "{:02x}",
            (x * (1.0 - weight) + y * weight).round() as u8
        );
    }
    out
}
fn font(name: &str) -> &'static str {
    match name {
        "humanist" => "\"Inter\",\"Segoe UI\",\"Microsoft YaHei\",sans-serif",
        "code" => "\"Cascadia Code\",\"SFMono-Regular\",Consolas,\"Microsoft YaHei\",monospace",
        _ => "\"Segoe UI\",\"Microsoft YaHei\",sans-serif",
    }
}

pub fn build_tokens(theme: &Theme, dark: bool, compact: bool) -> BTreeMap<String, String> {
    let p = if dark { &theme.dark } else { &theme.light };
    let (
        success,
        success_bg,
        success_line,
        warning,
        warning_bg,
        warning_line,
        danger,
        danger_bg,
        danger_line,
        info,
        info_bg,
        info_line,
    ) = if dark {
        (
            "#a1d8b6", "#203d31", "#467458", "#f0d294", "#3c3220", "#79633b", "#f1aaa2", "#462b2d",
            "#88504e", "#a6d0f5", "#23394e", "#4c6e90",
        )
    } else {
        (
            "#2d694d", "#eaf4ee", "#bfd8c7", "#855b17", "#fcf4df", "#dfcb99", "#a03c35", "#fff0ed",
            "#e9c5c1", "#285f90", "#eaf2fa", "#c4d8eb",
        )
    };
    let mut t = BTreeMap::new();
    for (key, value) in [
        ("bg", &p.bg),
        ("side", &p.side),
        ("panel", &p.panel),
        ("card", &p.card),
        ("text", &p.text),
        ("muted", &p.muted),
        ("subtle", &p.muted),
        ("line", &p.line),
        ("code", &p.code),
        ("accent", &p.accent),
        ("accent-bg", &p.accent_bg),
        ("selected-bg", &p.selected),
        ("selected-text", &p.text),
        ("canvas", &p.side),
        ("chart-system", &p.accent),
    ] {
        t.insert(key.into(), value.clone());
    }
    let on_accent = if dark { "#1c1c20" } else { "#ffffff" };
    for (key, value) in [
        ("on-accent", on_accent),
        ("shadow-color", if dark { "#00000055" } else { "#202c2012" }),
        ("backdrop", if dark { "#050b1477" } else { "#17252f55" }),
        ("success", success),
        ("success-bg", success_bg),
        ("success-line", success_line),
        ("warning", warning),
        ("warning-bg", warning_bg),
        ("warning-line", warning_line),
        ("danger", danger),
        ("danger-bg", danger_bg),
        ("danger-line", danger_line),
        ("info", info),
        ("info-bg", info_bg),
        ("info-line", info_line),
        ("diff-add", success),
        ("diff-add-bg", success_bg),
        ("diff-del", danger),
        ("diff-del-bg", danger_bg),
        ("terminal-text", "#e1eaf2"),
        ("terminal-line", "#526575"),
        ("terminal-muted", "#b3c4d2"),
        ("terminal-command", "#ffffff"),
        ("terminal-success", "#a8dcba"),
        ("green", success),
        ("green-bg", success_bg),
        ("chart-tools", warning_line),
        ("chart-files", info_line),
        ("qr-paper", "#ffffff"),
        ("qr-ink", "#000000"),
        ("brand", theme.brand.as_deref().unwrap_or(&p.accent)),
        (
            "on-brand",
            if theme.brand.is_some() {
                "#ffffff"
            } else {
                on_accent
            },
        ),
    ] {
        t.insert(key.into(), value.to_owned());
    }
    insert_derived_colors(&mut t, p, dark);
    insert_layout_tokens(&mut t, theme, compact);
    t
}

fn insert_derived_colors(t: &mut BTreeMap<String, String>, p: &Palette, dark: bool) {
    for (key, value) in [
        ("line-strong", mix(&p.line, &p.muted, 0.3)),
        ("body-text", mix(&p.text, &p.muted, 0.15)),
        ("chrome-bg", mix(&p.bg, &p.side, 0.65)),
        ("focus-color", format!("{}66", p.accent)),
        (
            "terminal-bg",
            if dark {
                p.side.clone()
            } else {
                mix(&p.text, "#000000", 0.15)
            },
        ),
        (
            "toast-bg",
            if dark {
                mix(&p.card, &p.accent, 0.18)
            } else {
                p.text.clone()
            },
        ),
        (
            "toast-text",
            if dark { p.text.clone() } else { p.bg.clone() },
        ),
        (
            "toast-line",
            if dark {
                p.line.clone()
            } else {
                p.muted.clone()
            },
        ),
        (
            "thumb",
            if dark {
                p.text.clone()
            } else {
                "#ffffff".into()
            },
        ),
        ("accent-line", mix(&p.line, &p.accent, 0.45)),
        ("chart-history", mix(&p.bg, &p.accent, 0.45)),
    ] {
        t.insert(key.into(), value);
    }
}

fn insert_layout_tokens(t: &mut BTreeMap<String, String>, theme: &Theme, compact: bool) {
    t.insert("sans".into(), font(&theme.font).into());
    t.insert("mono".into(), font("code").into());
    t.insert("font-heading".into(), font(&theme.heading_font).into());
    t.insert("shadow".into(), "0 16px 60px var(--shadow-color)".into());
    t.insert(
        "density-factor".into(),
        if compact { ".86" } else { "1" }.into(),
    );
    t.insert(
        "font-message".into(),
        if compact { "12px" } else { "13px" }.into(),
    );
    t.insert(
        "message-line-height".into(),
        if compact { "1.8" } else { "1.95" }.into(),
    );
    t.insert(
        "control-height".into(),
        if compact { "34px" } else { "39px" }.into(),
    );
    t.insert("motion-fast".into(), "160ms".into());
    t.insert("motion-normal".into(), "220ms".into());
    let radii = match theme.shape.as_str() {
        "crisp" => [2, 3, 5, 8, 10],
        "square" => [1, 2, 3, 5, 7],
        _ => [3, 5, 8, 12, 16],
    };
    for (key, value) in ["xs", "sm", "md", "lg", "xl"].into_iter().zip(radii) {
        t.insert(format!("radius-{key}"), format!("{value}px"));
    }
    t.insert("radius-pill".into(), "999px".into());
    for n in 1..=80 {
        t.insert(
            format!("space-{n}"),
            format!("calc({n}px * var(--density-factor))"),
        );
    }
}

pub fn parse_theme_document(value: &Value) -> Result<(Theme, Option<&str>, Option<&str>), String> {
    let object = value.as_object().ok_or("导入文件必须是 JSON 对象。")?;
    if object.get("schemaVersion").and_then(Value::as_u64) != Some(THEME_SCHEMA_VERSION)
        || object.get("kind").and_then(Value::as_str) != Some(THEME_KIND)
    {
        return Err("主题 JSON 版本或类型不受支持。".into());
    }
    if !exact_keys(
        object,
        &["schemaVersion", "kind", "theme", "preferredMode", "density"],
        &["schemaVersion", "kind", "theme"],
    ) {
        return Err("主题 JSON 含有未知或缺失字段。".into());
    }
    let theme = parse_theme(object.get("theme").unwrap())?;
    let mode = match object.get("preferredMode") {
        None => None,
        Some(Value::String(value)) if ["light", "dark", "system"].contains(&value.as_str()) => {
            Some(value.as_str())
        }
        _ => return Err("preferredMode 必须是 light、dark 或 system。".into()),
    };
    let density = match object.get("density") {
        None => None,
        Some(Value::String(value)) if ["standard", "compact"].contains(&value.as_str()) => {
            Some(value.as_str())
        }
        _ => return Err("density 必须是 standard 或 compact。".into()),
    };
    Ok((theme, mode, density))
}

pub fn theme_document(theme: &Theme, mode: &str, density: &str) -> Value {
    json!({"schemaVersion":THEME_SCHEMA_VERSION,"kind":THEME_KIND,"theme":theme_value(theme),"preferredMode":mode,"density":density})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn presets_are_valid() {
        for theme in presets() {
            assert!(parse_theme(&theme_value(&theme)).is_ok());
        }
    }
    #[test]
    fn contrast_is_checked() {
        for mode in ["light", "dark"] {
            for background in ["bg", "side", "panel", "card", "code"] {
                for foreground in ["text", "muted"] {
                    let mut value = theme_value(&presets()[0]);
                    value["palettes"][mode][foreground] =
                        value["palettes"][mode][background].clone();
                    assert!(parse_theme(&value).is_err());
                }
            }
        }
    }
    #[test]
    fn strict_allowlist_rejects_css_and_unknown_fields() {
        let original = theme_value(&presets()[0]);
        for (field, replacement) in [
            ("font", "url(https://invalid)"),
            ("headingFont", "serif"),
            ("shape", "round"),
            ("id", "a".repeat(65).as_str()),
            ("name", "a".repeat(41).as_str()),
        ] {
            let mut value = original.clone();
            value[field] = json!(replacement);
            assert!(parse_theme(&value).is_err());
        }
        let mut value = original.clone();
        value["extra"] = json!(true);
        assert!(parse_theme(&value).is_err());
        let mut value = original.clone();
        value["palettes"]["dark"]["bg"] = json!("url(test)");
        assert!(parse_theme(&value).is_err());
        let mut value = original.clone();
        value["palettes"]["dark"]["extra"] = json!("#ffffff");
        assert!(parse_theme(&value).is_err());
        let mut value = original;
        value["brand"] = Value::Null;
        assert!(parse_theme(&value).is_err());
    }
    #[test]
    fn tokens_cover_shape_density_and_derived_colors() {
        let tokens = build_tokens(&presets()[0], false, true);
        assert_eq!(tokens["space-80"], "calc(80px * var(--density-factor))");
        assert_eq!(tokens["density-factor"], ".86");
        assert_eq!(tokens["radius-md"], "8px");
        assert_eq!(tokens["terminal-text"], "#e1eaf2");
        assert_eq!(tokens["diff-add"], "#2d694d");
        assert_eq!(tokens["font-heading"], font("humanist"));
        assert_eq!(tokens["accent-line"], mix("#e1e4da", "#b14b2a", 0.45));
    }
}
