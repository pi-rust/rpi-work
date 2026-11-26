//! Device preferences and the workbench settings dialog.

use std::{fs, io, path::PathBuf};

use gpui::{Context, Div, FocusHandle, FontWeight, Window, WindowAppearance, div, prelude::*, px};
use serde_json::{Value, json};

use super::{Shell, message_queue::Delivery};
use crate::{icons::icon, theme::Palette};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum ColorMode {
    System,
    #[default]
    Light,
    Dark,
}

pub(super) struct Preferences {
    color: ColorMode,
    theme: String,
    pub(super) classic: bool,
    large_text: bool,
    pub(super) compact: bool,
    pub(super) code_numbers: bool,
    pub(super) show_context: bool,
    pub(super) follow_up: Delivery,
    pub(super) detached_review: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            color: ColorMode::default(),
            theme: "codex".into(),
            classic: false,
            large_text: false,
            compact: false,
            code_numbers: true,
            show_context: true,
            follow_up: Delivery::default(),
            detached_review: false,
        }
    }
}

impl Preferences {
    fn path() -> Option<PathBuf> {
        work_viewmodel::connection::native_path()
            .map(|path| path.with_file_name("gpui-settings.json"))
    }

    pub(super) fn load() -> Self {
        let value: Value = Self::path()
            .and_then(|path| fs::read(path).ok())
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        let default = Self::default();
        let theme = value["theme"]
            .as_str()
            .filter(|id| Palette::presets().iter().any(|(preset, _)| preset == id));
        Self {
            color: match value["color"].as_str() {
                Some("system") => ColorMode::System,
                Some("dark") => ColorMode::Dark,
                _ => ColorMode::Light,
            },
            theme: theme.map(str::to_owned).unwrap_or(default.theme),
            classic: value["classic"].as_bool().unwrap_or(default.classic),
            large_text: value["largeText"].as_bool().unwrap_or(default.large_text),
            compact: value["compact"].as_bool().unwrap_or(default.compact),
            code_numbers: value["codeNumbers"]
                .as_bool()
                .unwrap_or(default.code_numbers),
            show_context: value["showContext"]
                .as_bool()
                .unwrap_or(default.show_context),
            follow_up: Delivery::from_wire(value["followUp"].as_str().unwrap_or_default()),
            detached_review: value["detachedReview"]
                .as_bool()
                .unwrap_or(default.detached_review),
        }
    }

    fn persist(&self) -> io::Result<()> {
        let path = Self::path().ok_or(io::ErrorKind::NotFound)?;
        fs::create_dir_all(path.parent().ok_or(io::ErrorKind::NotFound)?)?;
        let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
        let value = json!({
            "color": match self.color { ColorMode::System => "system", ColorMode::Light => "light", ColorMode::Dark => "dark" },
            "theme": self.theme,
            "classic": self.classic,
            "largeText": self.large_text,
            "compact": self.compact,
            "codeNumbers": self.code_numbers,
            "showContext": self.show_context,
            "followUp": self.follow_up.wire(),
            "detachedReview": self.detached_review,
        });
        fs::write(&temporary, value.to_string())?;
        if let Err(error) = fs::rename(&temporary, &path) {
            let _ = fs::remove_file(&temporary);
            return Err(error);
        }
        Ok(())
    }

    pub(super) fn font_size(&self) -> f32 {
        if self.large_text { 18.0 } else { 16.0 }
    }

    pub(super) fn line_height(&self) -> f32 {
        self.font_size() * if self.compact { 1.4 } else { 1.65 }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    General,
    Appearance,
}

impl Section {
    fn label(self) -> &'static str {
        match self {
            Self::General => "常规",
            Self::Appearance => "外观",
        }
    }
}

pub(super) struct Panel {
    pub(super) open: bool,
    section: Section,
    close_focus: FocusHandle,
    save_failed: bool,
}

impl Panel {
    pub(super) fn new(cx: &Context<Shell>) -> Self {
        Self {
            open: false,
            section: Section::General,
            close_focus: cx.focus_handle(),
            save_failed: false,
        }
    }
}

impl Shell {
    pub(super) fn apply_appearance(&mut self, appearance: WindowAppearance, force: bool) {
        let dark = match self.preferences.color {
            ColorMode::System => matches!(
                appearance,
                WindowAppearance::Dark | WindowAppearance::VibrantDark
            ),
            ColorMode::Light => false,
            ColorMode::Dark => true,
        };
        if force || self.dark != dark {
            self.dark = dark;
            self.palette =
                Palette::from_preset(&self.preferences.theme, dark, self.preferences.compact);
        }
    }

    pub(super) fn save_preferences(&mut self, window: &Window, cx: &mut Context<Self>) {
        self.apply_appearance(window.appearance(), true);
        self.settings.save_failed = self.preferences.persist().is_err();
        cx.notify();
    }

    pub(super) fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(draft) = self.drafts.get_mut(&self.session) {
            draft.input.finish_composition();
        }
        self.composer_config.close_menu();
        self.input_dragging = false;
        self.browser.restore_composer_focus();
        self.settings.open = true;
        self.settings.close_focus.focus(window);
        cx.notify();
    }

    fn close_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.settings.open = false;
        self.focus_composer(window);
        cx.notify();
    }

    pub(super) fn settings_dialog(
        &self,
        width: f32,
        height: f32,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = self.palette;
        let compact = width <= 700.0;
        let section = self.settings.section;
        let dialog_width = if compact {
            width
        } else {
            (width * 0.92).min(1200.0)
        };
        let dialog_height = if compact {
            height
        } else {
            (height * 0.85).min(784.0)
        };
        let nav =
            div()
                .flex()
                .flex_col()
                .flex_none()
                .gap_2()
                .p_3()
                .bg(palette.side)
                .when(!compact, |nav| {
                    nav.w(px((dialog_width / 3.0).min(275.0)))
                        .border_r_1()
                        .border_color(palette.line)
                })
                .when(compact, |nav| nav.border_b_1().border_color(palette.line))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            self.text("设置", 16.0, palette.text)
                                .font_weight(FontWeight::SEMIBOLD)
                                .pl_2(),
                        )
                        .child(
                            self.icon_button("settings-close", "x", 44.0)
                                .h(px(44.0))
                                .track_focus(&self.settings.close_focus)
                                .tab_index(0)
                                .border_1()
                                .border_color(gpui::transparent_black())
                                .focus(|button| button.border_color(palette.strong_line))
                                .on_click(cx.listener(|shell, _, window, cx| {
                                    shell.close_settings(window, cx)
                                })),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .gap_1()
                        .when(!compact, |tabs| tabs.flex_col())
                        .children(
                            [
                                (Section::General, "settings-general", "settings"),
                                (Section::Appearance, "settings-appearance", "appearance"),
                            ]
                            .into_iter()
                            .map(|(tab, id, shape)| {
                                self.button(id, "")
                                    .h(px(44.0))
                                    .justify_start()
                                    .gap_2()
                                    .tab_index(0)
                                    .border_1()
                                    .border_color(gpui::transparent_black())
                                    .focus(|button| button.border_color(palette.strong_line))
                                    .when(section == tab, |button| button.bg(palette.hover))
                                    .child(icon(shape, 16.0, palette.muted))
                                    .child(self.text(tab.label(), 13.0, palette.text))
                                    .on_click(cx.listener(move |shell, _, _, cx| {
                                        shell.settings.section = tab;
                                        cx.notify();
                                    }))
                            }),
                        ),
                );
        div()
            .id("settings-scrim")
            .absolute()
            .inset_0()
            .occlude()
            .bg(gpui::rgba(0x00000030))
            .flex()
            .items_center()
            .justify_center()
            .capture_key_down(
                cx.listener(|shell, event: &gpui::KeyDownEvent, window, cx| {
                    match event.keystroke.key.as_str() {
                        "escape" => shell.close_settings(window, cx),
                        "tab" if event.keystroke.modifiers.shift => window.focus_prev(),
                        "tab" => window.focus_next(),
                        _ => return,
                    }
                    cx.stop_propagation();
                }),
            )
            .child(
                div()
                    .id("settings-dialog")
                    .tab_group()
                    .flex()
                    .when(compact, |dialog| dialog.flex_col())
                    .w(px(dialog_width))
                    .h(px(dialog_height))
                    .overflow_hidden()
                    .bg(palette.bg)
                    .when(!compact, |dialog| {
                        dialog
                            .rounded(px(20.0))
                            .border_1()
                            .border_color(palette.line)
                            .shadow_lg()
                    })
                    .child(nav)
                    .child(
                        div()
                            .id(("settings-content", section as usize))
                            .flex_1()
                            .min_h(px(0.0))
                            .min_w(px(0.0))
                            .overflow_y_scroll()
                            .px(px(if compact { 18.0 } else { 24.0 }))
                            .py(px(if compact { 20.0 } else { 40.0 }))
                            .child(
                                self.text(
                                    section.label(),
                                    if compact { 24.0 } else { 28.0 },
                                    palette.text,
                                )
                                .mb_5(),
                            )
                            .child(match section {
                                Section::General => self.general_settings(cx),
                                Section::Appearance => self.appearance_settings(cx),
                            })
                            .when(self.settings.save_failed, |body| {
                                body.child(
                                    self.text(
                                        "当前窗口已应用设置，本机保存失败。",
                                        12.0,
                                        palette.danger,
                                    )
                                    .mt_3(),
                                )
                            }),
                    ),
            )
    }

    fn general_settings(&self, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .flex_col()
            .child(self.settings_heading("聊天"))
            .child(
                self.settings_row(
                    "显示上下文窗口使用情况",
                    self.setting_choice(
                        "settings-context",
                        if self.preferences.show_context {
                            "开"
                        } else {
                            "关"
                        },
                        false,
                    )
                    .on_click(cx.listener(|shell, _, window, cx| {
                        shell.preferences.show_context = !shell.preferences.show_context;
                        shell.save_preferences(window, cx);
                    })),
                ),
            )
            .child(
                self.settings_row(
                    "后续消息处理方式",
                    div().flex().gap_1().children(
                        [
                            ("settings-follow-up", "加入队列", Delivery::FollowUp),
                            ("settings-steer", "引导当前任务", Delivery::Steer),
                        ]
                        .map(|(id, label, delivery)| {
                            self.setting_choice(id, label, self.preferences.follow_up == delivery)
                                .on_click(cx.listener(move |shell, _, window, cx| {
                                    shell.preferences.follow_up = delivery;
                                    shell.save_preferences(window, cx);
                                }))
                        }),
                    ),
                ),
            )
            .child(self.settings_heading("工作区").mt_4())
            .child(
                self.settings_row(
                    "审查结果呈现方式",
                    div().flex().gap_1().children(
                        [
                            ("settings-review-inline", "当前会话", false),
                            ("settings-review-detached", "独立会话", true),
                        ]
                        .map(|(id, label, detached)| {
                            self.setting_choice(
                                id,
                                label,
                                self.preferences.detached_review == detached,
                            )
                            .on_click(cx.listener(
                                move |shell, _, window, cx| {
                                    shell.preferences.detached_review = detached;
                                    shell.save_preferences(window, cx);
                                },
                            ))
                        }),
                    ),
                ),
            )
    }

    fn appearance_settings(&self, cx: &mut Context<Self>) -> Div {
        let palette = self.palette;
        let selected_theme = self.preferences.theme.as_str();
        let presets = Palette::presets();
        let preset = presets
            .iter()
            .position(|(id, _)| id == selected_theme)
            .unwrap_or_default();
        let label = presets[preset].1.as_str();
        let next = presets[(preset + 1) % presets.len()].0.clone();
        div()
            .flex()
            .flex_col()
            .child(self.settings_heading("主题"))
            .child(
                self.settings_row(
                    "颜色模式",
                    div().flex().gap_1().children(
                        [
                            ("settings-system", "跟随系统", ColorMode::System),
                            ("settings-light", "浅色", ColorMode::Light),
                            ("settings-dark", "深色", ColorMode::Dark),
                        ]
                        .map(|(id, label, color)| {
                            self.setting_choice(id, label, self.preferences.color == color)
                                .on_click(cx.listener(move |shell, _, window, cx| {
                                    shell.preferences.color = color;
                                    shell.save_preferences(window, cx);
                                }))
                        }),
                    ),
                ),
            )
            .child(
                self.settings_row(
                    "主题方案",
                    self.setting_choice("settings-preset", label, false)
                        .child(icon("chevron-right", 12.0, palette.muted))
                        .on_click(cx.listener(move |shell, _, window, cx| {
                            shell.preferences.theme.clone_from(&next);
                            shell.save_preferences(window, cx);
                        })),
                ),
            )
            .child(self.settings_heading("字体与阅读").mt_4())
            .children(
                [
                    (
                        "settings-layout",
                        "消息布局",
                        "阅读",
                        "经典",
                        self.preferences.classic,
                        (|p: &mut Preferences, v| p.classic = v) as fn(&mut Preferences, bool),
                    ),
                    (
                        "settings-font-size",
                        "消息字号",
                        "16 px",
                        "18 px",
                        self.preferences.large_text,
                        |p, v| p.large_text = v,
                    ),
                    (
                        "settings-density",
                        "阅读密度",
                        "舒适",
                        "紧凑",
                        self.preferences.compact,
                        |p, v| p.compact = v,
                    ),
                    (
                        "settings-code-numbers",
                        "代码行号",
                        "隐藏",
                        "显示",
                        self.preferences.code_numbers,
                        |p, v| p.code_numbers = v,
                    ),
                ]
                .map(|(id, title, first, second, active, apply)| {
                    self.settings_row(
                        title,
                        div()
                            .flex()
                            .gap_1()
                            .children([(first, false), (second, true)].map(|(label, value)| {
                                self.setting_choice(
                                    (id, usize::from(value)),
                                    label,
                                    value == active,
                                )
                                .on_click(cx.listener(
                                    move |shell, _, window, cx| {
                                        apply(&mut shell.preferences, value);
                                        shell.save_preferences(window, cx);
                                    },
                                ))
                            })),
                    )
                }),
            )
    }

    fn settings_heading(&self, label: &'static str) -> Div {
        self.text(label, 15.0, self.palette.text)
            .font_weight(FontWeight::SEMIBOLD)
            .mb_2()
    }

    fn settings_row(&self, label: &'static str, control: impl IntoElement) -> Div {
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .min_h(px(64.0))
            .border_t_1()
            .border_color(self.palette.line)
            .child(
                self.text(label, 14.0, self.palette.text)
                    .flex_1()
                    .min_w(px(0.0)),
            )
            .child(div().flex_none().child(control))
    }

    fn setting_choice(
        &self,
        id: impl Into<gpui::ElementId>,
        label: &str,
        selected: bool,
    ) -> gpui::Stateful<Div> {
        let palette = self.palette;
        self.button(id, label)
            .h(px(44.0))
            .gap_1()
            .tab_index(0)
            .border_1()
            .border_color(palette.line)
            .focus(|button| button.border_color(palette.text))
            .when(selected, |button| {
                button
                    .bg(palette.subtle)
                    .font_weight(FontWeight::SEMIBOLD)
                    .border_color(palette.strong_line)
            })
    }
}
