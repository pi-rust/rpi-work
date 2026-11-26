//! Device preferences and the workbench settings dialog.

use std::{fs, io, path::PathBuf};

use gpui::{
    AnyElement, Context, Div, ElementId, FocusHandle, FontWeight, ScrollHandle, Window,
    WindowAppearance, div, point, prelude::*, px,
};
use serde_json::{Value, json};

use super::{
    Shell,
    editor::{Buffer, Layout},
    message_queue::Delivery,
};
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
    pub(super) notify_finish: bool,
    pub(super) navigation: super::navigation::Preferences,
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
            notify_finish: false,
            navigation: super::navigation::Preferences::default(),
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
            notify_finish: value["notifyFinish"].as_bool().unwrap_or_default(),
            navigation: super::navigation::Preferences::load(&value["navigation"]),
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
            "notifyFinish": self.notify_finish,
            "navigation": self.navigation.value(),
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
    Notifications,
    Shortcuts,
    Archives,
}

impl Section {
    const NAVIGATION: [(Self, &str, &str); 5] = [
        (Self::General, "settings-general", "settings"),
        (Self::Appearance, "settings-appearance", "appearance"),
        (Self::Notifications, "settings-notifications", "bell"),
        (Self::Shortcuts, "settings-shortcuts", "keyboard"),
        (Self::Archives, "settings-archives", "archive-box"),
    ];

    fn label(self) -> &'static str {
        match self {
            Self::General => "常规",
            Self::Appearance => "外观",
            Self::Notifications => "通知",
            Self::Shortcuts => "键盘快捷键",
            Self::Archives => "已归档的聊天",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Field {
    Context,
    FollowUp,
    ProjectGrouping,
    ReviewDelivery,
    Color,
    Theme,
    Layout,
    FontSize,
    Density,
    CodeNumbers,
    NotifyFinish,
}

impl Field {
    const ALL: [Self; 11] = [
        Self::Context,
        Self::FollowUp,
        Self::ProjectGrouping,
        Self::ReviewDelivery,
        Self::Color,
        Self::Theme,
        Self::Layout,
        Self::FontSize,
        Self::Density,
        Self::CodeNumbers,
        Self::NotifyFinish,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Context => "显示上下文窗口使用情况",
            Self::FollowUp => "后续消息处理方式",
            Self::ProjectGrouping => "项目列表显示",
            Self::ReviewDelivery => "审查结果呈现方式",
            Self::Color => "颜色模式",
            Self::Theme => "主题方案",
            Self::Layout => "消息布局",
            Self::FontSize => "消息字号",
            Self::Density => "阅读密度",
            Self::CodeNumbers => "代码行号",
            Self::NotifyFinish => "任务完成提示",
        }
    }

    fn section(self) -> Section {
        match self {
            Self::Context | Self::FollowUp | Self::ProjectGrouping | Self::ReviewDelivery => {
                Section::General
            }
            Self::NotifyFinish => Section::Notifications,
            Self::Color
            | Self::Theme
            | Self::Layout
            | Self::FontSize
            | Self::Density
            | Self::CodeNumbers => Section::Appearance,
        }
    }

    fn first_control(self) -> ElementId {
        match self {
            Self::Context => "settings-context".into(),
            Self::FollowUp => "settings-follow-up".into(),
            Self::ProjectGrouping => "settings-projects-grouped".into(),
            Self::ReviewDelivery => "settings-review-inline".into(),
            Self::Color => "settings-system".into(),
            Self::Theme => "settings-preset".into(),
            Self::Layout => ("settings-layout", 0usize).into(),
            Self::FontSize => ("settings-font-size", 0usize).into(),
            Self::Density => ("settings-density", 0usize).into(),
            Self::CodeNumbers => ("settings-code-numbers", 0usize).into(),
            Self::NotifyFinish => "settings-notify-finish".into(),
        }
    }
}

#[derive(Clone, Copy)]
struct Destination {
    section: Section,
    field: Option<Field>,
}

fn search_destinations(query: &str) -> impl Iterator<Item = Destination> {
    Section::NAVIGATION
        .into_iter()
        .flat_map(move |(section, _, _)| {
            let pane_match = section.label().contains(query);
            pane_match
                .then_some(Destination {
                    section,
                    field: None,
                })
                .into_iter()
                .chain(
                    Field::ALL
                        .into_iter()
                        .filter(move |field| {
                            !pane_match
                                && field.section() == section
                                && field.label().contains(query)
                        })
                        .map(move |field| Destination {
                            section,
                            field: Some(field),
                        }),
                )
        })
}

struct SettingsRow(Option<Field>, AnyElement);

pub(super) struct Panel {
    pub(super) open: bool,
    section: Section,
    close_focus: FocusHandle,
    browse: bool,
    browse_focus: FocusHandle,
    pub(super) search: Buffer,
    pub(super) search_enter: bool,
    pub(super) search_layout: Option<Layout>,
    pub(super) search_focus: FocusHandle,
    first_result: FocusHandle,
    target: Option<Field>,
    target_focus: FocusHandle,
    reveal: bool,
    content_scroll: ScrollHandle,
    save_failed: bool,
}

impl Panel {
    pub(super) fn new(cx: &Context<Shell>) -> Self {
        Self {
            open: false,
            section: Section::General,
            close_focus: cx.focus_handle(),
            browse: false,
            browse_focus: cx.focus_handle(),
            search: Buffer::default(),
            search_enter: false,
            search_layout: None,
            search_focus: cx.focus_handle(),
            first_result: cx.focus_handle(),
            target: None,
            target_focus: cx.focus_handle(),
            reveal: false,
            content_scroll: ScrollHandle::new(),
            save_failed: false,
        }
    }

    fn clear_search(&mut self) {
        self.search = Buffer::default();
        self.search_enter = false;
        self.search_layout = None;
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
        self.save_device_preferences();
        cx.notify();
    }

    pub(super) fn save_device_preferences(&mut self) {
        self.settings.save_failed = self.preferences.persist().is_err();
    }

    pub(super) fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings.open || !self.navigation_keyboard_enabled() {
            return;
        }
        self.project_menu = None;
        self.close_session_filter_menu();
        self.welcome_project_picker = false;
        if let Some(draft) = self.drafts.get_mut(&self.draft_key()) {
            draft.input.finish_composition();
        }
        self.composer_config.close_menu();
        self.input_dragging = false;
        self.browser.restore_composer_focus();
        self.settings.open = true;
        if self.settings.section == Section::Archives {
            self.read_session_management_capabilities(cx);
        }
        if window.viewport_size().width <= px(700.0) {
            self.settings.browse_focus.focus(window);
        } else {
            self.settings.search_focus.focus(window);
        }
        cx.notify();
    }

    fn close_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.settings.open = false;
        self.settings.browse = false;
        self.settings.clear_search();
        self.input_dragging = false;
        self.focus_composer(window);
        cx.notify();
    }

    fn select_settings_destination(
        &mut self,
        destination: Destination,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let panel = &mut self.settings;
        panel.section = destination.section;
        panel.target = destination.field;
        panel.reveal = true;
        panel.browse = false;
        panel.clear_search();
        panel.content_scroll.set_offset(point(px(0.0), px(0.0)));
        if destination.section == Section::Archives {
            self.read_session_management_capabilities(cx);
        }
        // Finish dispatching the result's key release before focusing a writable control.
        window.prevent_default();
        cx.on_next_frame(window, move |shell, window, _| {
            let panel = &shell.settings;
            if panel.open
                && panel.section == destination.section
                && panel.target == destination.field
            {
                panel.target_focus.focus(window);
            }
        });
        cx.notify();
    }

    pub(super) fn activate_settings_search(
        &mut self,
        select: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let query = self.settings.search.text.trim();
        if query.is_empty() || !self.settings.search.is_committed() {
            return;
        }
        let first = search_destinations(query).next();
        if let Some(first) = first {
            if select {
                self.select_settings_destination(first, window, cx);
            } else {
                self.settings.first_result.focus(window);
            }
        }
    }

    fn settings_navigation(
        &self,
        compact: bool,
        height: f32,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let palette = self.palette;
        let section = self.settings.section;
        let query = self.settings.search.text.trim();
        let mut list = div()
            .id("settings-navigation-list")
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .flex_col();
        if query.is_empty() {
            list = list.children(Section::NAVIGATION.map(|(tab, id, shape)| {
                self.setting_navigation_button(id)
                    .when(section == tab, |button| button.bg(palette.hover))
                    .child(icon(shape, 16.0, palette.muted))
                    .child(self.text(tab.label(), 13.0, palette.text))
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        shell.select_settings_destination(
                            Destination {
                                section: tab,
                                field: None,
                            },
                            window,
                            cx,
                        );
                    }))
            }));
        } else {
            let mut results = search_destinations(query).peekable();
            if results.peek().is_none() {
                list = list.child(self.text("未找到结果", 13.0, palette.muted).px_2().py_3());
            }
            list = list.children(results.enumerate().map(|(index, destination)| {
                let title = destination
                    .field
                    .map_or(destination.section.label(), Field::label);
                self.setting_navigation_button(("settings-match", index))
                    .when(index == 0, |button| {
                        button.track_focus(&self.settings.first_result)
                    })
                    .child(
                        div()
                            .min_w_0()
                            .child(self.text(title, 13.0, palette.text))
                            .when(destination.field.is_some(), |label| {
                                label.child(self.text(
                                    destination.section.label(),
                                    11.0,
                                    palette.muted,
                                ))
                            }),
                    )
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        shell.select_settings_destination(destination, window, cx);
                    }))
            }));
        }
        div()
            .flex()
            .flex_col()
            .min_h_0()
            .gap_2()
            .when(compact, |nav| nav.max_h(px((height * 0.5).min(400.0))))
            .child(
                div()
                    .id("settings-search")
                    .flex()
                    .items_center()
                    .gap_1()
                    .px_2()
                    .h(px(36.0))
                    .flex_none()
                    .bg(palette.bg)
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(palette.line)
                    .when(self.settings.search_focus.is_focused(window), |field| {
                        field.border_color(palette.strong_line)
                    })
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|shell, _, window, _| {
                            shell.settings.search_focus.focus(window)
                        }),
                    )
                    .child(icon("search", 15.0, palette.muted))
                    .child(self.settings_input_element(cx)),
            )
            .child(list)
    }

    fn setting_navigation_button(&self, id: impl Into<ElementId>) -> gpui::Stateful<Div> {
        let palette = self.palette;
        self.button(id, "")
            .w_full()
            .h(px(44.0))
            .gap_2()
            .justify_start()
            .tab_index(0)
            .border_1()
            .border_color(gpui::transparent_black())
            .focus(|button| button.border_color(palette.strong_line))
    }

    pub(super) fn settings_dialog(
        &mut self,
        width: f32,
        height: f32,
        window: &Window,
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
        let rows = match section {
            Section::General => self.general_settings(cx),
            Section::Appearance => self.appearance_settings(cx),
            Section::Notifications => self.notification_settings(cx),
            Section::Shortcuts => self.shortcut_settings(),
            Section::Archives => vec![SettingsRow(
                None,
                self.archive_settings(&self.settings.target_focus, cx)
                    .into_any_element(),
            )],
        };
        if std::mem::take(&mut self.settings.reveal)
            && let Some(target) = self.settings.target
            && let Some(index) = rows.iter().position(|row| row.0 == Some(target))
        {
            self.settings.content_scroll.scroll_to_item(index + 1);
        }
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
                        .gap_2()
                        .child(
                            self.text("设置", 16.0, palette.text)
                                .font_weight(FontWeight::SEMIBOLD)
                                .pl_2(),
                        )
                        .when(compact, |bar| {
                            bar.child(
                                self.button("settings-browse", "浏览分类")
                                    .h(px(44.0))
                                    .gap_1()
                                    .track_focus(&self.settings.browse_focus)
                                    .tab_index(0)
                                    .focus(|button| button.bg(palette.hover))
                                    .child(icon("chevron-down", 14.0, palette.muted))
                                    .on_click(cx.listener(|shell, _, window, cx| {
                                        shell.settings.browse = !shell.settings.browse;
                                        if shell.settings.browse {
                                            shell.settings.search_focus.focus(window);
                                        } else {
                                            shell.settings.clear_search();
                                            shell.settings.browse_focus.focus(window);
                                        }
                                        cx.notify();
                                    })),
                            )
                        })
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
                .when(!compact || self.settings.browse, |nav| {
                    nav.child(self.settings_navigation(compact, height, window, cx))
                });
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
                        "escape"
                            if shell.settings.search_focus.is_focused(window)
                                && !shell.settings.search.is_committed() =>
                        {
                            return;
                        }
                        "escape" if !shell.settings.search.text.is_empty() => {
                            shell.settings.clear_search();
                            if window.viewport_size().width > px(700.0) || shell.settings.browse {
                                shell.settings.search_focus.focus(window);
                            } else {
                                shell.settings.browse_focus.focus(window);
                            }
                            cx.notify();
                        }
                        "escape" => shell.close_settings(window, cx),
                        "tab" => {
                            if event.keystroke.modifiers.shift {
                                window.focus_prev();
                            } else {
                                window.focus_next();
                            }
                            cx.notify();
                        }
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
                            .track_scroll(&self.settings.content_scroll)
                            .px(px(if compact { 18.0 } else { 24.0 }))
                            .py(px(if compact { 20.0 } else { 40.0 }))
                            .child(
                                div()
                                    .id("settings-title")
                                    .when(self.settings.target.is_none(), |title| {
                                        title
                                            .track_focus(&self.settings.target_focus)
                                            .tab_index(0)
                                            .tab_stop(false)
                                    })
                                    .child(self.text(
                                        section.label(),
                                        if compact { 24.0 } else { 28.0 },
                                        palette.text,
                                    ))
                                    .mb_5(),
                            )
                            .children(rows.into_iter().map(|row| row.1))
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

    fn general_settings(&self, cx: &mut Context<Self>) -> Vec<SettingsRow> {
        vec![
            self.settings_heading("聊天", false),
            self.settings_row(
                Field::Context,
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
            self.settings_row(
                Field::FollowUp,
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
            self.settings_heading("工作区", true),
            self.settings_row(
                Field::ProjectGrouping,
                div().flex().gap_1().children(
                    [
                        ("settings-projects-grouped", "按节点分组", false),
                        ("settings-projects-flat", "平铺", true),
                    ]
                    .map(|(id, label, flat)| {
                        self.setting_choice(id, label, self.preferences.navigation.flat == flat)
                            .on_click(cx.listener(move |shell, _, _, cx| {
                                shell.preferences.navigation.flat = flat;
                                shell.project_menu = None;
                                shell.save_device_preferences();
                                cx.notify();
                            }))
                    }),
                ),
            ),
            self.settings_row(
                Field::ReviewDelivery,
                div().flex().gap_1().children(
                    [
                        ("settings-review-inline", "当前会话", false),
                        ("settings-review-detached", "独立会话", true),
                    ]
                    .map(|(id, label, detached)| {
                        self.setting_choice(id, label, self.preferences.detached_review == detached)
                            .on_click(cx.listener(move |shell, _, window, cx| {
                                shell.preferences.detached_review = detached;
                                shell.save_preferences(window, cx);
                            }))
                    }),
                ),
            ),
        ]
    }

    fn appearance_settings(&self, cx: &mut Context<Self>) -> Vec<SettingsRow> {
        let palette = self.palette;
        let selected_theme = self.preferences.theme.as_str();
        let presets = Palette::presets();
        let preset = presets
            .iter()
            .position(|(id, _)| id == selected_theme)
            .unwrap_or_default();
        let label = presets[preset].1.as_str();
        let next = presets[(preset + 1) % presets.len()].0.clone();
        let mut rows = vec![
            self.settings_heading("主题", false),
            self.settings_row(
                Field::Color,
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
            self.settings_row(
                Field::Theme,
                self.setting_choice("settings-preset", label, false)
                    .child(icon("chevron-right", 12.0, palette.muted))
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        shell.preferences.theme.clone_from(&next);
                        shell.save_preferences(window, cx);
                    })),
            ),
            self.settings_heading("字体与阅读", true),
        ];
        rows.extend(
            [
                (
                    "settings-layout",
                    Field::Layout,
                    "阅读",
                    "经典",
                    self.preferences.classic,
                    (|p: &mut Preferences, v| p.classic = v) as fn(&mut Preferences, bool),
                ),
                (
                    "settings-font-size",
                    Field::FontSize,
                    "16 px",
                    "18 px",
                    self.preferences.large_text,
                    |p, v| p.large_text = v,
                ),
                (
                    "settings-density",
                    Field::Density,
                    "舒适",
                    "紧凑",
                    self.preferences.compact,
                    |p, v| p.compact = v,
                ),
                (
                    "settings-code-numbers",
                    Field::CodeNumbers,
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
                            self.setting_choice((id, usize::from(value)), label, value == active)
                                .on_click(cx.listener(move |shell, _, window, cx| {
                                    apply(&mut shell.preferences, value);
                                    shell.save_preferences(window, cx);
                                }))
                        })),
                )
            }),
        );
        rows
    }

    fn shortcut_settings(&self) -> Vec<SettingsRow> {
        let palette = self.palette;
        let shortcuts: [(&str, &[&str]); 7] = [
            ("关闭对话框或抽屉", &["Esc"]),
            ("移至下一个控件", &["Tab"]),
            ("移至上一个控件", &["Shift", "Tab"]),
            ("发送消息", &["Enter"]),
            ("消息换行", &["Shift", "Enter"]),
            ("定位首个设置搜索结果", &["Enter"]),
            ("聚焦设置搜索结果", &["↓"]),
        ];
        super::WorkbenchShortcut::BINDINGS
            .into_iter()
            .map(|(_, key, shift, label)| {
                let keys = [
                    super::PRIMARY_MODIFIER_LABEL,
                    if shift { "Shift" } else { "" },
                    key,
                ];
                (
                    label,
                    keys.into_iter()
                        .filter(|key| !key.is_empty())
                        .collect::<Vec<_>>(),
                )
            })
            .chain(
                shortcuts
                    .into_iter()
                    .map(|(label, keys)| (label, keys.to_vec())),
            )
            .map(|(label, keys)| {
                SettingsRow(
                    None,
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_3()
                        .min_h(px(46.0))
                        .border_t_1()
                        .border_color(palette.line)
                        .child(self.text(label, 13.0, palette.text))
                        .child(
                            div()
                                .flex()
                                .flex_none()
                                .gap_1()
                                .children(keys.iter().map(|key| {
                                    self.text(*key, 12.0, palette.text)
                                        .min_w(px(23.0))
                                        .px_1()
                                        .text_center()
                                        .rounded(px(4.0))
                                        .bg(palette.subtle)
                                        .border_1()
                                        .border_color(palette.line)
                                })),
                        )
                        .into_any_element(),
                )
            })
            .collect()
    }

    fn notification_settings(&self, cx: &mut Context<Self>) -> Vec<SettingsRow> {
        let enabled = self.preferences.notify_finish;
        let palette = self.palette;
        vec![
            self.settings_heading("桌面应用", false),
            self.settings_row(
                Field::NotifyFinish,
                self.setting_choice("settings-notify-finish", "", false)
                    .border_color(gpui::transparent_black())
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .w(px(34.0))
                            .h(px(20.0))
                            .px(px(2.0))
                            .rounded_full()
                            .border_1()
                            .border_color(if enabled {
                                palette.text
                            } else {
                                palette.strong_line
                            })
                            .bg(if enabled { palette.text } else { palette.hover })
                            .when(enabled, |track| track.justify_end())
                            .child(
                                div()
                                    .size(px(14.0))
                                    .rounded_full()
                                    .border_1()
                                    .border_color(if enabled {
                                        palette.bg
                                    } else {
                                        palette.strong_line
                                    })
                                    .bg(palette.bg),
                            ),
                    )
                    .on_click(cx.listener(|shell, _, window, cx| {
                        shell.preferences.notify_finish = !shell.preferences.notify_finish;
                        if !shell.preferences.notify_finish {
                            shell.finish_notice = None;
                        }
                        shell.save_preferences(window, cx);
                    })),
            ),
        ]
    }

    fn settings_heading(&self, label: &'static str, after_group: bool) -> SettingsRow {
        SettingsRow(
            None,
            self.text(label, 15.0, self.palette.text)
                .font_weight(FontWeight::SEMIBOLD)
                .mb_2()
                .when(after_group, |heading| heading.mt_4())
                .into_any_element(),
        )
    }

    fn settings_row(&self, field: Field, control: impl IntoElement) -> SettingsRow {
        SettingsRow(
            Some(field),
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .min_h(px(64.0))
                .border_t_1()
                .border_color(self.palette.line)
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .child(self.text(field.label(), 14.0, self.palette.text))
                        .when(field == Field::NotifyFinish, |label| {
                            label.child(
                                self.text(
                                    "在工作台内显示完成、失败或停止的提示。",
                                    12.0,
                                    self.palette.muted,
                                )
                                .mt_1(),
                            )
                        }),
                )
                .child(div().flex_none().child(control))
                .into_any_element(),
        )
    }

    fn setting_choice(
        &self,
        id: impl Into<gpui::ElementId>,
        label: &str,
        selected: bool,
    ) -> gpui::Stateful<Div> {
        let palette = self.palette;
        let id = id.into();
        let target = self
            .settings
            .target
            .is_some_and(|field| field.first_control() == id);
        self.button(id, label)
            .when(target, |button| {
                button.track_focus(&self.settings.target_focus)
            })
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
