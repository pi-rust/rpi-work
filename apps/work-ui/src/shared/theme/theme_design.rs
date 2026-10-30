//! Bounded design tokens, never arbitrary CSS, font URLs, or component layout.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FontFamily {
    System,
    Humanist,
    Code,
    Serif,
}
impl FontFamily {
    pub fn legacy(value: &str) -> Self {
        match value {
            "humanist" => Self::Humanist,
            "code" => Self::Code,
            _ => Self::System,
        }
    }
    pub fn stack(self) -> &'static str {
        match self {
            Self::System => "\"Segoe UI\",\"Microsoft YaHei\",sans-serif",
            Self::Humanist => "\"Inter\",\"Segoe UI\",\"Microsoft YaHei\",sans-serif",
            Self::Code => {
                "\"Cascadia Code\",\"SFMono-Regular\",Consolas,\"Microsoft YaHei\",monospace"
            }
            Self::Serif => "Georgia,\"Noto Serif SC\",\"Songti SC\",SimSun,serif",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Typography {
    pub body_font: FontFamily,
    pub heading_font: FontFamily,
    pub code_font: FontFamily,
    pub scale: f64,
    pub body_size: u16,
    pub nav_size: u16,
    pub assist_size: u16,
    pub title_size: u16,
    pub message_size: u16,
    pub code_size: u16,
    pub body_line_height: f64,
    pub message_line_height: f64,
    pub body_weight: u16,
    pub heading_weight: u16,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Geometry {
    pub spacing_scale: f64,
    /// xs, sm, md, lg, xl; ascending pixel values, with zero allowed.
    pub radii: [u16; 5],
    pub control_height: u16,
    pub hit_size: u16,
    pub border_width: u16,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Shadow {
    None,
    Subtle,
    Soft,
    Lifted,
}
impl Shadow {
    fn css(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Subtle => "0 2px 8px var(--shadow-color)",
            Self::Soft => "0 16px 60px var(--shadow-color)",
            Self::Lifted => "0 24px 80px var(--shadow-color)",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Motion {
    #[serde(rename = "fastMs")]
    pub fast: u16,
    #[serde(rename = "normalMs")]
    pub normal: u16,
    #[serde(rename = "pressMs")]
    pub press: u16,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ThemeDesign {
    pub typography: Typography,
    pub geometry: Geometry,
    pub shadow: Shadow,
    pub motion: Motion,
}
impl ThemeDesign {
    /// Built-in Codex's visual defaults; explicit portable v2 designs still win.
    pub fn codex() -> Self {
        let mut design = Self::legacy("system", "system", "soft");
        design.typography.nav_size = 14;
        design.typography.title_size = 16;
        design.typography.message_size = 15;
        design.typography.code_size = 12;
        design.typography.body_line_height = 1.65;
        design.typography.message_line_height = 1.7;
        design.typography.heading_weight = 600;
        design.geometry.radii = [4, 8, 12, 16, 20];
        design.geometry.control_height = 40;
        design.shadow = Shadow::Subtle;
        design.motion = Motion {
            fast: 140,
            normal: 200,
            press: 100,
        };
        design
    }

    pub fn legacy(body: &str, heading: &str, shape: &str) -> Self {
        Self {
            typography: Typography {
                body_font: FontFamily::legacy(body),
                heading_font: FontFamily::legacy(heading),
                code_font: FontFamily::Code,
                scale: 1.0,
                body_size: 15,
                nav_size: 13,
                assist_size: 12,
                title_size: 18,
                message_size: 13,
                code_size: 11,
                body_line_height: 1.8,
                message_line_height: 1.55,
                body_weight: 400,
                heading_weight: 550,
            },
            geometry: Geometry {
                spacing_scale: 1.0,
                radii: match shape {
                    "crisp" => [2, 3, 5, 8, 10],
                    "square" => [1, 2, 3, 5, 7],
                    _ => [3, 5, 8, 12, 16],
                },
                control_height: 39,
                hit_size: 44,
                border_width: 1,
            },
            shadow: Shadow::Soft,
            motion: Motion {
                fast: 160,
                normal: 220,
                press: 120,
            },
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        let t = &self.typography;
        let g = &self.geometry;
        let within =
            |value: f64, min: f64, max: f64| value.is_finite() && (min..=max).contains(&value);
        if !within(t.scale, 0.85, 1.5)
            || !within(t.body_line_height, 1.3, 2.2)
            || !within(t.message_line_height, 1.3, 2.2)
            || ![
                (t.body_size, 12, 22),
                (t.nav_size, 11, 20),
                (t.assist_size, 10, 18),
                (t.title_size, 16, 32),
                (t.message_size, 12, 24),
                (t.code_size, 10, 20),
            ]
            .iter()
            .all(|&(value, min, max)| (min..=max).contains(&value))
            || ![t.body_weight, t.heading_weight]
                .iter()
                .all(|value| (300..=800).contains(value) && value % 50 == 0)
        {
            return Err("字体规格越界：请检查字号倍率、角色字号、行高和字重。".into());
        }
        if !within(g.spacing_scale, 0.75, 1.5)
            || g.radii.iter().any(|value| *value > 32)
            || g.radii.windows(2).any(|pair| pair[0] > pair[1])
            || !(32..=56).contains(&g.control_height)
            || !(44..=64).contains(&g.hit_size)
            || !(1..=3).contains(&g.border_width)
        {
            return Err("几何规格越界：间距、递增圆角、控件高度、触控尺寸或边框不合法。".into());
        }
        if [self.motion.fast, self.motion.normal, self.motion.press]
            .iter()
            .any(|value| *value > 600)
            || self.motion.fast > self.motion.normal
        {
            return Err("动效时长必须在 0–600ms 内，快速时长不能大于常规时长。".into());
        }
        Ok(())
    }

    pub fn tokens(&self, compact: bool) -> BTreeMap<String, String> {
        let t = &self.typography;
        let g = &self.geometry;
        let mut out = BTreeMap::new();
        for (key, value) in [
            ("sans", t.body_font.stack()),
            ("mono", t.code_font.stack()),
            ("font-heading", t.heading_font.stack()),
            ("shadow", self.shadow.css()),
            ("density-factor", if compact { ".86" } else { "1" }),
        ] {
            out.insert(key.into(), value.into());
        }
        for (key, value) in [
            ("type-scale", t.scale),
            ("spacing-scale", g.spacing_scale),
            ("body-line-height", t.body_line_height),
            ("message-line-height", t.message_line_height),
            ("reading-line-height", t.message_line_height),
        ] {
            out.insert(key.into(), value.to_string());
        }
        for (key, value) in [
            ("font-body", t.body_size),
            ("font-nav", t.nav_size),
            ("font-assist", t.assist_size),
            ("font-title", t.title_size),
            ("font-message", t.message_size - u16::from(compact)),
            ("font-code", t.code_size),
        ] {
            out.insert(key.into(), format!("calc({value}px * var(--type-scale))"));
        }
        for (key, value) in [
            (
                "control-height",
                g.control_height - if compact { 5 } else { 0 },
            ),
            ("hit-size", g.hit_size),
            ("border-width", g.border_width),
        ] {
            out.insert(key.into(), format!("{value}px"));
        }
        for (key, value) in [
            ("body-weight", t.body_weight),
            ("heading-weight", t.heading_weight),
        ] {
            out.insert(key.into(), value.to_string());
        }
        for (key, value) in [
            ("motion-fast", self.motion.fast),
            ("motion-normal", self.motion.normal),
            ("press-duration", self.motion.press),
        ] {
            out.insert(key.into(), format!("{value}ms"));
        }
        for (key, value) in ["xs", "sm", "md", "lg", "xl"].into_iter().zip(g.radii) {
            out.insert(format!("radius-{key}"), format!("{value}px"));
        }
        out.insert(
            "shadow-control".into(),
            match self.shadow {
                Shadow::None => "none",
                Shadow::Subtle => "0 1px 4px var(--shadow-color)",
                Shadow::Soft => "0 3px 12px var(--shadow-color)",
                Shadow::Lifted => "0 6px 20px var(--shadow-color)",
            }
            .into(),
        );
        out.insert("radius-pill".into(), "999px".into());
        for n in 1..=80 {
            out.insert(
                format!("space-{n}"),
                format!("calc({n}px * var(--density-factor) * var(--spacing-scale))"),
            );
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_are_valid_and_density_composes_without_mutating_design() {
        let design = ThemeDesign::legacy("humanist", "code", "crisp");
        design.validate().unwrap();
        let standard = design.tokens(false);
        let compact = design.tokens(true);
        assert_eq!(standard["radius-md"], "5px");
        assert_eq!(compact["density-factor"], ".86");
        assert_eq!(compact["control-height"], "34px");
        assert_eq!(standard["type-scale"], "1");
        assert_eq!(standard["sans"], FontFamily::Humanist.stack());
        assert_eq!(
            serde_json::from_value::<ThemeDesign>(serde_json::to_value(&design).unwrap()).unwrap(),
            design
        );
    }
    #[test]
    fn rejects_unknown_fields_external_fonts_and_out_of_range_values() {
        let original =
            serde_json::to_value(ThemeDesign::legacy("system", "system", "soft")).unwrap();
        for (section, key, value) in [
            (
                "typography",
                "bodyFont",
                serde_json::json!("url(https://fonts.invalid/font)"),
            ),
            ("geometry", "radii", serde_json::json!([1, 2, 3])),
            ("motion", "extra", serde_json::json!(0)),
        ] {
            let mut v = original.clone();
            v[section][key] = value;
            assert!(serde_json::from_value::<ThemeDesign>(v).is_err());
        }
        for (section, key, value) in [
            ("typography", "scale", serde_json::json!(3)),
            ("typography", "bodySize", serde_json::json!(2)),
            ("typography", "messageLineHeight", serde_json::json!(0.5)),
            ("geometry", "spacingScale", serde_json::json!(8)),
            ("geometry", "hitSize", serde_json::json!(12)),
            ("geometry", "radii", serde_json::json!([8, 1, 2, 3, 4])),
            ("motion", "normalMs", serde_json::json!(1000)),
        ] {
            let mut v = original.clone();
            v[section][key] = value;
            assert!(
                serde_json::from_value::<ThemeDesign>(v)
                    .unwrap()
                    .validate()
                    .is_err()
            );
        }
    }
}
