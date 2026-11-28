//! Device preferences and the workbench settings dialog.

#[path = "settings/file.rs"]
mod file;

use std::{path::PathBuf, time::Duration};

use gpui::{
    AnyElement, ClipboardItem, Context, Div, ElementId, FocusHandle, FontWeight, ScrollHandle,
    Task, Window, WindowAppearance, div, point, prelude::*, px,
};
use serde_json::{Value, json};
use work_core::api::NativeKind;

use self::file::SettingsFile;
use super::{
    Shell, desktop_plugins,
    feature_entries::{self, Entry, Visibility},
    message_queue::Delivery,
};
use crate::{
    icons::icon,
    theme::Palette,
    updates::{Status as UpdateStatus, Updates, VERSION},
};

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
    automatic_updates: bool,
    pub(super) follow_up: Delivery,
    pub(super) detached_review: bool,
    pub(super) notify_finish: bool,
    pub(super) navigation: super::navigation::Preferences,
    pub(super) entries: Visibility,
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
            automatic_updates: true,
            follow_up: Delivery::default(),
            detached_review: false,
            notify_finish: false,
            navigation: super::navigation::Preferences::default(),
            entries: Visibility::default(),
        }
    }
}

impl Preferences {
    fn path() -> Option<PathBuf> {
        work_viewmodel::connection::native_path()
            .map(|path| path.with_file_name("gpui-settings.json"))
    }

    fn from_value(value: &Value) -> Self {
        let default = Self::default();
        Self {
            color: match value["color"].as_str() {
                Some("system") => ColorMode::System,
                Some("dark") => ColorMode::Dark,
                _ => ColorMode::Light,
            },
            theme: value["theme"]
                .as_str()
                .map(str::to_owned)
                .unwrap_or(default.theme),
            classic: value["classic"].as_bool().unwrap_or(default.classic),
            large_text: value["largeText"].as_bool().unwrap_or(default.large_text),
            compact: value["compact"].as_bool().unwrap_or(default.compact),
            code_numbers: value["codeNumbers"]
                .as_bool()
                .unwrap_or(default.code_numbers),
            show_context: value["showContext"]
                .as_bool()
                .unwrap_or(default.show_context),
            automatic_updates: value["automaticUpdates"]
                .as_bool()
                .unwrap_or(default.automatic_updates),
            follow_up: Delivery::from_wire(value["followUp"].as_str().unwrap_or_default()),
            detached_review: value["detachedReview"]
                .as_bool()
                .unwrap_or(default.detached_review),
            notify_finish: value["notifyFinish"].as_bool().unwrap_or_default(),
            navigation: super::navigation::Preferences::load(&value["navigation"]),
            entries: Visibility::load(&value["featureEntries"]),
        }
    }

    fn value(&self) -> Value {
        json!({
            "color": match self.color { ColorMode::System => "system", ColorMode::Light => "light", ColorMode::Dark => "dark" },
            "theme": self.theme,
            "classic": self.classic,
            "largeText": self.large_text,
            "compact": self.compact,
            "codeNumbers": self.code_numbers,
            "showContext": self.show_context,
            "automaticUpdates": self.automatic_updates,
            "followUp": self.follow_up.wire(),
            "detachedReview": self.detached_review,
            "notifyFinish": self.notify_finish,
            "navigation": self.navigation.value(),
            "featureEntries": self.entries.value(),
        })
    }

    pub(super) fn font_size(&self) -> f32 {
        if self.large_text { 18.0 } else { 16.0 }
    }

    pub(super) fn line_height(&self) -> f32 {
        self.font_size() * if self.compact { 1.55 } else { 1.65 }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    General,
    Appearance,
    Notifications,
    Configuration,
    Shortcuts,
    Archives,
    Plugins,
    Skills,
    FeatureEntries,
    ChatIntegrations,
    RpiHost,
    Collaboration,
    Review,
}

type SectionLink = (Section, &'static str, &'static str);

impl Section {
    const NAVIGATION: [(&str, &[SectionLink]); 4] = [
        (
            "应用",
            &[
                (Self::General, "settings-general", "settings"),
                (Self::Appearance, "settings-appearance", "appearance"),
                (Self::Notifications, "settings-notifications", "bell"),
                (Self::Configuration, "settings-configuration", "code-xml"),
                (Self::Shortcuts, "settings-shortcuts", "keyboard"),
            ],
        ),
        (
            "数据与隐私",
            &[(Self::Archives, "settings-archives", "archive-box")],
        ),
        (
            "集成",
            &[
                (Self::Plugins, "settings-plugins", "blocks"),
                (Self::Skills, "settings-skills", "list-checks"),
                (Self::FeatureEntries, "settings-feature-entries", "settings"),
                (
                    Self::ChatIntegrations,
                    "settings-chat-integrations",
                    "message-circle",
                ),
                (Self::RpiHost, "settings-rpi-host", "code-xml"),
            ],
        ),
        (
            "编码",
            &[
                (Self::Collaboration, "settings-collaboration", "globe"),
                (Self::Review, "settings-review", "file-diff"),
            ],
        ),
    ];

    fn label(self) -> &'static str {
        match self {
            Self::General => "常规",
            Self::Appearance => "外观",
            Self::Notifications => "通知",
            Self::Configuration => "配置",
            Self::Shortcuts => "键盘快捷键",
            Self::Archives => "已归档的聊天",
            Self::Plugins => "插件",
            Self::Skills => "技能",
            Self::FeatureEntries => "功能入口",
            Self::ChatIntegrations => "接入聊天软件",
            Self::RpiHost => "rpi Host 管理",
            Self::Collaboration => "工作协同",
            Self::Review => "代码审查",
        }
    }

    fn visible(self, entries: &feature_entries::Visibility) -> bool {
        match self {
            Self::ChatIntegrations => entries.shows(Entry::Chat),
            Self::RpiHost => entries.shows(Entry::RpiHost),
            _ => true,
        }
    }

    fn search_terms(self) -> &'static str {
        match self {
            Self::General => "general",
            Self::Appearance => "appearance",
            Self::Notifications => "notification",
            Self::Configuration => "供应商 模型 API 密钥 Base URL provider key",
            Self::Shortcuts => "快捷键 shortcut keyboard",
            Self::Archives => "归档 恢复 archive",
            Self::Plugins => "扩展 plugin extension",
            Self::Skills => "skill",
            Self::FeatureEntries => "目标模式 goal 功能开关 发现 MCP Node Web 工作协同 菜单",
            Self::ChatIntegrations => "飞书 企业微信 机器人 feishu lark wecom im-message",
            Self::RpiHost => "共享宿主 持久化 area Agent native-host",
            Self::Collaboration => "节点 地址 局域网 连接 分享 二维码 node connection",
            Self::Review => "代码 审查 review",
        }
    }

    fn resource_kinds(self) -> &'static [NativeKind] {
        match self {
            Self::Plugins => &[NativeKind::Extension],
            Self::Skills => &[NativeKind::Skill],
            _ => &[],
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
    SettingsFile,
    AppUpdate,
}

impl Field {
    const ALL: [Self; 13] = [
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
        Self::SettingsFile,
        Self::AppUpdate,
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
            Self::SettingsFile => "设置文件",
            Self::AppUpdate => "应用更新",
        }
    }

    fn section(self) -> Section {
        match self {
            Self::Context
            | Self::FollowUp
            | Self::ProjectGrouping
            | Self::SettingsFile
            | Self::AppUpdate => Section::General,
            Self::ReviewDelivery => Section::Review,
            Self::NotifyFinish => Section::Notifications,
            Self::Color
            | Self::Theme
            | Self::Layout
            | Self::FontSize
            | Self::Density
            | Self::CodeNumbers => Section::Appearance,
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::Context => "在输入框下方显示当前会话的上下文用量。",
            Self::FollowUp => "任务运行时，新消息可以排队等候，也可以引导当前任务。",
            Self::ProjectGrouping => "调整侧栏项目列表的排列方式。",
            Self::ReviewDelivery => "在当前聊天中查看结果，或为审查创建独立会话。",
            Self::Color => "跟随系统时，工作台会随系统外观自动切换。",
            Self::Theme => "选中的配色会立即应用到整个工作台。",
            Self::Layout => "在适合长文阅读的布局与经典聊天布局间切换。",
            Self::FontSize => "调整聊天正文的字号。",
            Self::Density => "调整消息的行距。",
            Self::CodeNumbers => "在聊天的代码块中显示行号。",
            Self::NotifyFinish => "在工作台内显示任务完成、失败或停止的提示。",
            Self::SettingsFile => "在默认应用中编辑本机设置，保存后自动应用。",
            Self::AppUpdate => "后台检查并下载稳定版更新，重启前由你确认。",
        }
    }

    fn search_terms(self) -> &'static str {
        match self {
            Self::Context => "上下文 context",
            Self::FollowUp => "排队 加入队列 引导当前任务 queue steer",
            Self::ProjectGrouping => "项目 分组 平铺 project",
            Self::ReviewDelivery => "代码审查 独立会话 review",
            Self::Color => "跟随系统 浅色 深色 暗色 黑暗 夜间 dark light system",
            Self::Theme => "配色 色板 theme",
            Self::Layout => "经典 阅读 layout",
            Self::FontSize => "字体 文字 大小 font size",
            Self::Density => "行距 舒适 紧凑 density",
            Self::CodeNumbers => "代码块 line numbers",
            Self::NotifyFinish => "通知 提醒 完成 失败 停止 notification",
            Self::SettingsFile => "编辑设置 JSON settings file gpui-settings.json",
            Self::AppUpdate => "自动更新 检查 下载 安装 重启 版本 update version",
        }
    }

    fn first_control(self) -> ElementId {
        match self {
            Self::Context => "settings-context".into(),
            Self::FollowUp => "settings-follow-up".into(),
            Self::ProjectGrouping => "settings-projects-grouped".into(),
            Self::ReviewDelivery => "settings-review-inline".into(),
            Self::Color => "settings-system".into(),
            Self::Theme => ("settings-preset", 0usize).into(),
            Self::Layout => ("settings-layout", 0usize).into(),
            Self::FontSize => ("settings-font-size", 0usize).into(),
            Self::Density => ("settings-density", 0usize).into(),
            Self::CodeNumbers => "settings-code-numbers".into(),
            Self::NotifyFinish => "settings-notify-finish".into(),
            Self::SettingsFile => "settings-edit-json".into(),
            Self::AppUpdate => "settings-check-update".into(),
        }
    }
}

#[derive(Clone, Copy)]
struct Destination {
    section: Section,
    field: Option<Field>,
}

fn search_destinations(query: &str) -> Vec<Destination> {
    let query = query.to_lowercase();
    let matches = |label: &str, terms: &str| {
        let label = label.to_lowercase();
        let terms = terms.to_lowercase();
        query
            .split_whitespace()
            .all(|word| label.contains(word) || terms.contains(word))
    };
    let mut results = Vec::new();
    for (_, links) in Section::NAVIGATION {
        for &(section, _, _) in links {
            if section.label() == query || matches(section.label(), section.search_terms()) {
                results.push(Destination {
                    section,
                    field: None,
                });
            }
            results.extend(Field::ALL.into_iter().filter_map(|field| {
                (field.section() == section
                    && (matches(field.label(), field.search_terms())
                        || (field == Field::Theme
                            && Palette::presets()
                                .iter()
                                .any(|(id, name)| matches(name, id)))))
                .then_some(Destination {
                    section,
                    field: Some(field),
                })
            }));
        }
    }
    results.sort_by_key(|destination| match destination.field {
        Some(field) if field.label() == query => 0,
        None if destination.section.label() == query => 0,
        Some(_) => 1,
        None => 2,
    });
    results
}

struct SettingsRow(Option<Field>, AnyElement);

#[derive(Default)]
enum SaveStatus {
    #[default]
    Idle,
    Saved,
    Reloaded,
    Failed,
}

pub(super) struct Panel {
    pub(super) open: bool,
    section: Section,
    dialog_focus: FocusHandle,
    end_focus: FocusHandle,
    close_focus: FocusHandle,
    compact: bool,
    form_open: bool,
    browse: bool,
    browse_focus: FocusHandle,
    navigation_focus: Vec<(Section, usize, FocusHandle)>,
    navigation_container: FocusHandle,
    navigation_scroll: ScrollHandle,
    search_focus: Vec<FocusHandle>,
    search_results: Vec<Destination>,
    search_scroll: ScrollHandle,
    last_query: String,
    last_entry_visibility: (bool, bool),
    title_focus: FocusHandle,
    content_focus: FocusHandle,
    row_focus: Vec<(Field, FocusHandle)>,
    focus_rows: Vec<(usize, FocusHandle)>,
    target: Option<Field>,
    target_focus: FocusHandle,
    reveal: bool,
    content_scroll: ScrollHandle,
    save_status: SaveStatus,
    file: SettingsFile,
    _file_watcher: Option<Task<()>>,
    file_open_error: bool,
    appearance_dirty: bool,
}

impl Panel {
    pub(super) fn new(cx: &Context<Shell>) -> (Self, Preferences) {
        let (file, preferences) = SettingsFile::load(Preferences::path());
        let watcher = file.path.clone().map(|path| {
            let executor = cx.background_executor().clone();
            cx.spawn(async move |shell, cx| {
                loop {
                    executor.timer(Duration::from_secs(1)).await;
                    let Ok(revision) = shell.update(cx, |shell, _| shell.settings.file.revision)
                    else {
                        break;
                    };
                    let path = path.clone();
                    let read = executor
                        .spawn(async move { SettingsFile::read(&path) })
                        .await;
                    if shell
                        .update(cx, |shell, cx| {
                            if shell.settings.file.revision == revision {
                                shell.reload_settings_file(read, cx);
                            }
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })
        });
        let panel = Self {
            open: false,
            section: Section::General,
            dialog_focus: cx.focus_handle(),
            end_focus: cx.focus_handle(),
            close_focus: cx.focus_handle().tab_stop(true),
            compact: false,
            form_open: false,
            browse: false,
            browse_focus: cx.focus_handle().tab_stop(true),
            navigation_focus: Section::NAVIGATION
                .into_iter()
                .flat_map(|(_, links)| std::iter::once(None).chain(links.iter().map(Some)))
                .enumerate()
                .filter_map(|(index, link)| {
                    link.map(|(section, _, _)| (*section, index, cx.focus_handle().tab_stop(true)))
                })
                .collect(),
            navigation_container: cx.focus_handle(),
            navigation_scroll: ScrollHandle::new(),
            search_focus: Vec::new(),
            search_results: Vec::new(),
            search_scroll: ScrollHandle::new(),
            last_query: String::new(),
            last_entry_visibility: (
                preferences.entries.shows(Entry::Chat),
                preferences.entries.shows(Entry::RpiHost),
            ),
            title_focus: cx.focus_handle(),
            content_focus: cx.focus_handle(),
            row_focus: Field::ALL.map(|field| (field, cx.focus_handle())).into(),
            focus_rows: Vec::new(),
            target: None,
            target_focus: cx.focus_handle().tab_stop(true),
            reveal: false,
            content_scroll: ScrollHandle::new(),
            save_status: SaveStatus::Idle,
            file,
            _file_watcher: watcher,
            file_open_error: false,
            appearance_dirty: false,
        };
        (panel, preferences)
    }

    fn update_search(
        &mut self,
        query: &str,
        entries: &feature_entries::Visibility,
        cx: &Context<Shell>,
    ) {
        let visibility = (entries.shows(Entry::Chat), entries.shows(Entry::RpiHost));
        if self.last_query == query && self.last_entry_visibility == visibility {
            return;
        }
        self.last_query = query.to_owned();
        self.last_entry_visibility = visibility;
        self.search_results = if query.is_empty() {
            Vec::new()
        } else {
            search_destinations(query)
                .into_iter()
                .filter(|result| result.section.visible(entries))
                .collect()
        };
        self.search_focus
            .resize_with(self.search_results.len(), || {
                cx.focus_handle().tab_stop(true)
            });
        self.search_scroll.set_offset(point(px(0.0), px(0.0)));
    }

    fn reveal_section(&self) {
        if let Some((_, index, _)) = self
            .navigation_focus
            .iter()
            .find(|(section, _, _)| *section == self.section)
        {
            self.navigation_scroll.scroll_to_item(*index);
        }
    }

    fn tab(&self, backwards: bool, window: &mut Window, cx: &Context<Shell>) {
        if backwards {
            window.focus_prev();
        } else {
            window.focus_next();
        }
        if !self.dialog_focus.contains_focused(window, cx) {
            if backwards {
                // Start just after the dialog's last visible control.
                self.end_focus.focus(window);
                window.focus_prev();
            } else if self.dialog_focus.contains(&self.browse_focus, window) {
                self.browse_focus.focus(window);
            } else {
                self.close_focus.focus(window);
            }
        }
        self.reveal_focus(window, cx);
    }

    fn reveal_focus(&self, window: &Window, cx: &Context<Shell>) {
        if let Some(index) = self
            .search_focus
            .iter()
            .position(|focus| focus.is_focused(window))
        {
            self.search_scroll.scroll_to_item(index);
        } else if let Some((_, index, _)) = self
            .navigation_focus
            .iter()
            .find(|(_, _, focus)| focus.is_focused(window))
        {
            self.navigation_scroll.scroll_to_item(*index);
        } else if let Some((index, _)) = self
            .focus_rows
            .iter()
            .find(|(_, focus)| focus.contains_focused(window, cx))
        {
            self.content_scroll.scroll_to_item(*index);
        }
    }
}

impl Shell {
    pub(crate) fn automatic_updates(&self) -> bool {
        self.preferences.automatic_updates
    }

    pub(super) fn apply_appearance(&mut self, appearance: WindowAppearance, force: bool) {
        let dark = match self.preferences.color {
            ColorMode::System => matches!(
                appearance,
                WindowAppearance::Dark | WindowAppearance::VibrantDark
            ),
            ColorMode::Light => false,
            ColorMode::Dark => true,
        };
        let dirty = std::mem::take(&mut self.settings.appearance_dirty);
        if force || dirty || self.dark != dark {
            self.dark = dark;
            self.palette =
                Palette::from_preset(&self.preferences.theme, dark, self.preferences.compact);
        }
    }

    pub(super) fn save_preferences(&mut self, window: &Window, cx: &mut Context<Self>) {
        self.save_device_preferences();
        self.apply_appearance(window.appearance(), true);
        cx.notify();
    }

    pub(super) fn save_device_preferences(&mut self) {
        self.settings.save_status = match self.settings.file.save(&mut self.preferences) {
            Ok(()) => {
                self.device_preferences_changed();
                if self.settings.open {
                    SaveStatus::Saved
                } else {
                    SaveStatus::Idle
                }
            }
            Err(_) => SaveStatus::Failed,
        };
    }

    fn reload_settings_file(
        &mut self,
        read: std::io::Result<Option<Vec<u8>>>,
        cx: &mut Context<Self>,
    ) {
        if !self.settings.file.reload(read, &mut self.preferences) {
            return;
        }
        if self.settings.file.problem.is_none() {
            self.device_preferences_changed();
            Updates::set_automatic(self.preferences.automatic_updates, cx);
            if !self.settings.file.has_pending_changes(&self.preferences) {
                self.settings.save_status = if self.settings.open {
                    SaveStatus::Reloaded
                } else {
                    SaveStatus::Idle
                };
            }
        }
        cx.notify();
    }

    fn device_preferences_changed(&mut self) {
        self.settings.appearance_dirty = true;
        if !self.preferences.notify_finish {
            self.finish_notice = None;
        }
        self.apply_feature_visibility();
    }

    pub(super) fn edit_settings_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.settings.file_open_error = match self.settings.file.open(&self.preferences) {
            Ok(path) => {
                cx.open_with_system(&path);
                false
            }
            Err(_) => {
                if !self.settings.open {
                    self.settings.section = Section::General;
                    self.open_settings(window, cx);
                }
                true
            }
        };
        cx.notify();
    }

    pub(super) fn open_feature_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.navigation_keyboard_enabled() {
            return;
        }
        self.dismiss_drawer(window);
        self.settings.section = Section::FeatureEntries;
        self.open_settings(window, cx);
        self.settings.browse = false;
        self.focus_settings_title(window);
        cx.notify();
    }

    fn read_feature_status(&mut self, cx: &mut Context<Self>) {
        if [Entry::Goal, Entry::Mcp, Entry::Web]
            .into_iter()
            .any(|entry| self.preferences.entries.shows(entry))
        {
            self.read_resource_catalog(cx);
        }
        self.refresh_local_sharing(None, cx);
    }

    pub(super) fn open_collaboration_settings(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.settings.section = Section::Collaboration;
        self.open_settings(window, cx);
        self.settings.browse = false;
        self.focus_settings_title(window);
        cx.notify();
    }

    pub(crate) fn open_connection_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_collaboration_settings(window, cx);
        self.show_node_connection(None, window, cx);
    }

    pub(super) fn open_provider_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.settings.section = Section::Configuration;
        self.settings
            .content_scroll
            .set_offset(point(px(0.0), px(0.0)));
        self.open_settings(window, cx);
    }

    pub(super) fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings.open || !self.navigation_keyboard_enabled() {
            return;
        }
        self.close_resource_page();
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
        self.settings
            .content_scroll
            .set_offset(point(px(0.0), px(0.0)));
        self.settings.reveal_section();
        if self.settings.section == Section::Archives {
            self.read_session_management_capabilities(cx);
        }
        if self.settings.section == Section::Collaboration {
            self.read_connection_history();
            self.refresh_local_sharing(None, cx);
        }
        if !self.settings.section.resource_kinds().is_empty() {
            self.read_resource_catalog(cx);
        }
        if self.settings.section == Section::Configuration {
            self.read_provider_catalog(cx);
        }
        if self.settings.section == Section::FeatureEntries {
            self.read_feature_status(cx);
        }
        if self.settings.section == Section::ChatIntegrations {
            self.read_chat_integrations(cx);
        }
        if self.settings.section == Section::RpiHost {
            self.read_rpi_hosts(cx);
        }
        self.settings.compact = window.viewport_size().width <= px(super::COMPACT_NAVIGATION_WIDTH);
        if self.settings.compact {
            self.settings.browse_focus.focus(window);
        } else {
            self.search.focus.focus(window);
        }
        cx.notify();
    }

    fn close_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.reset_node_connection();
        self.reset_node_share();
        self.reset_resource_catalog();
        self.provider_settings.leave();
        self.chat_integrations = Default::default();
        self.settings.open = false;
        self.settings.browse = false;
        self.settings.target = None;
        self.settings.reveal = false;
        self.settings.file_open_error = false;
        if matches!(
            self.settings.save_status,
            SaveStatus::Saved | SaveStatus::Reloaded
        ) {
            self.settings.save_status = SaveStatus::Idle;
        }
        self.search.clear();
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
        if self.settings.section != destination.section {
            self.clear_resource_detail();
        }
        let panel = &mut self.settings;
        panel.section = destination.section;
        panel.target = destination.field;
        panel.reveal = true;
        panel.browse = false;
        panel.reveal_section();
        panel.content_scroll.set_offset(point(px(0.0), px(0.0)));
        self.search.clear();
        if destination.section == Section::Archives {
            self.read_session_management_capabilities(cx);
        }
        if destination.section == Section::Collaboration {
            self.read_connection_history();
            self.refresh_local_sharing(None, cx);
        }
        if !destination.section.resource_kinds().is_empty() {
            self.read_resource_catalog(cx);
        }
        if destination.section == Section::FeatureEntries {
            self.read_feature_status(cx);
        } else if destination.section == Section::ChatIntegrations {
            self.read_chat_integrations(cx);
        } else if destination.section.resource_kinds().is_empty() {
            self.reset_resource_catalog();
        }
        if destination.section != Section::ChatIntegrations {
            self.chat_integrations = Default::default();
        }
        if destination.section == Section::RpiHost {
            self.read_rpi_hosts(cx);
        }
        if destination.section == Section::Configuration {
            self.read_provider_catalog(cx);
        } else {
            self.provider_settings.leave();
        }
        // Finish dispatching the result's key release before focusing a writable control.
        window.prevent_default();
        cx.on_next_frame(window, move |shell, window, _| {
            let panel = &shell.settings;
            if panel.open
                && panel.section == destination.section
                && panel.target == destination.field
            {
                if destination.field.is_some() {
                    panel.target_focus.focus(window);
                } else {
                    panel.title_focus.focus(window);
                }
            }
        });
        cx.notify();
    }

    pub(super) fn show_settings_form(&mut self) {
        self.settings.browse = false;
        self.settings.target = None;
        self.settings.reveal = false;
        self.settings
            .content_scroll
            .set_offset(point(px(0.0), px(0.0)));
    }

    pub(super) fn focus_settings_title(&self, window: &mut Window) {
        self.settings.title_focus.focus(window);
    }

    pub(super) fn activate_settings_search(
        &mut self,
        select: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let query = self.search.buffer.text.trim();
        if query.is_empty() || !self.search.buffer.is_committed() {
            return;
        }
        self.settings
            .update_search(query, &self.preferences.entries, cx);
        if let Some(&first) = self.settings.search_results.first() {
            if select {
                self.select_settings_destination(first, window, cx);
            } else {
                self.settings.search_focus[0].focus(window);
                self.settings.search_scroll.scroll_to_item(0);
                cx.notify();
            }
        }
    }

    fn toggle_settings_browse(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.settings.browse = !self.settings.browse;
        if self.settings.browse {
            self.settings.reveal_section();
            self.search.focus.focus(window);
        } else {
            self.search.clear();
            self.settings.browse_focus.focus(window);
        }
        cx.notify();
    }

    fn settings_navigation(
        &self,
        compact: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = self.palette;
        let section = self.settings.section;
        let query = self.search.buffer.text.trim();
        let mut list = div()
            .id("settings-navigation-list")
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .flex_col();
        if query.is_empty() {
            list = list.track_scroll(&self.settings.navigation_scroll);
            let mut focus = self.settings.navigation_focus.iter();
            for (index, (label, links)) in Section::NAVIGATION.into_iter().enumerate() {
                list = list.child(
                    self.text(label, 12.0, palette.muted)
                        .px_2()
                        .pb_1()
                        .when(index > 0, |heading| heading.pt_3()),
                );
                list = list.children(links.iter().copied().zip(focus.by_ref()).map(
                    |((tab, id, shape), (_, _, focus))| {
                        if !tab.visible(&self.preferences.entries) {
                            return div().into_any_element();
                        }
                        self.setting_navigation_button(id)
                            .track_focus(focus)
                            .when(section == tab, |button| button.bg(palette.hover))
                            .child(icon(
                                shape,
                                16.0,
                                if section == tab {
                                    palette.text
                                } else {
                                    palette.muted
                                },
                            ))
                            .child(
                                self.text(tab.label(), 13.0, palette.text)
                                    .when(section == tab, |label| {
                                        label.font_weight(FontWeight::SEMIBOLD)
                                    }),
                            )
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
                            .into_any_element()
                    },
                ));
            }
        } else {
            list = list.track_scroll(&self.settings.search_scroll);
            if self.settings.search_results.is_empty() {
                list = list.child(
                    self.text("未找到设置。试试搜索‘字体’或‘节点’。", 13.0, palette.muted)
                        .px_2()
                        .py_3(),
                );
            }
            list = list.children(
                self.settings
                    .search_results
                    .iter()
                    .copied()
                    .zip(&self.settings.search_focus)
                    .enumerate()
                    .map(|(index, (destination, focus))| {
                        let title = destination
                            .field
                            .map_or(destination.section.label(), Field::label);
                        self.setting_navigation_button(("settings-match", index))
                            .min_h(px(if destination.field.is_some() {
                                48.0
                            } else {
                                36.0
                            }))
                            .track_focus(focus)
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
                    }),
            );
        }
        div()
            .id("settings-navigation")
            .track_focus(&self.settings.navigation_container)
            .flex()
            .flex_1()
            .flex_col()
            .min_h_0()
            .gap_2()
            .child(
                div()
                    .id("settings-search")
                    .flex()
                    .items_center()
                    .gap_1()
                    .px_2()
                    .h(px(if compact { 36.0 } else { 32.0 }))
                    .flex_none()
                    .bg(palette.bg)
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(palette.line)
                    .when(self.search.focus.is_focused(window), |field| {
                        field.border_color(palette.strong_line)
                    })
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|shell, _, window, _| shell.search.focus.focus(window)),
                    )
                    .child(icon("search", 15.0, palette.muted))
                    .child(self.search_input_element(cx))
                    .when(!self.search.buffer.text.is_empty(), |field| {
                        field.child(
                            self.navigation_tooltip(
                                self.icon_button("settings-search-clear", "x", 28.0)
                                    .tab_index(0)
                                    .focus(|button| button.bg(palette.hover))
                                    .on_click(cx.listener(|shell, _, window, cx| {
                                        shell.search.clear();
                                        shell.search.focus.focus(window);
                                        cx.notify();
                                    })),
                                "清除搜索",
                            ),
                        )
                    }),
            )
            .child(list)
    }

    fn setting_navigation_button(&self, id: impl Into<ElementId>) -> gpui::Stateful<Div> {
        let palette = self.palette;
        self.button(id, "")
            .w_full()
            .h(px(if self.settings.compact { 44.0 } else { 32.0 }))
            .gap_2()
            .justify_start()
            .tab_index(0)
            .border_1()
            .border_color(gpui::transparent_black())
            .focus(|button| button.border_color(palette.strong_line))
    }

    fn settings_form_open(&self) -> bool {
        self.node_connection.open
            || self.node_share.open
            || self.provider_settings.editing()
            || self.chat_integrations.editing()
    }

    fn dismiss_settings_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.node_connection.open {
            self.cancel_node_connection(window, cx);
        } else if self.node_share.open {
            self.close_node_share(window, cx);
        } else if self.provider_settings.editing() {
            self.cancel_provider_edit(window, cx);
        } else if self.chat_integrations.editing() {
            self.cancel_chat_edit(window, cx);
        }
    }

    fn settings_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = &event.keystroke;
        match key.key.as_str() {
            "escape" => {
                if self.search.focus.is_focused(window) && !self.search.buffer.is_committed() {
                    return;
                }
                if self.settings_form_open() {
                    self.dismiss_settings_form(window, cx);
                } else if !self.search.buffer.text.is_empty() {
                    self.search.clear();
                    self.search.focus.focus(window);
                    cx.notify();
                } else if self.settings.compact && self.settings.browse {
                    self.toggle_settings_browse(window, cx);
                } else {
                    self.close_settings(window, cx);
                }
            }
            "tab" if !key.modifiers.platform && !key.modifiers.control && !key.modifiers.alt => {
                self.settings.tab(key.modifiers.shift, window, cx);
                cx.notify();
            }
            "up" | "down" if !key.modifiers.modified() => {
                let Some(index) = self
                    .settings
                    .search_focus
                    .iter()
                    .position(|focus| focus.is_focused(window))
                else {
                    return;
                };
                let next = if key.key == "up" {
                    index.checked_sub(1)
                } else {
                    Some(index + 1)
                };
                if let Some(focus) = next.and_then(|index| self.settings.search_focus.get(index)) {
                    focus.focus(window);
                    self.settings.reveal_focus(window, cx);
                } else if next.is_none() {
                    self.search.focus.focus(window);
                }
                cx.notify();
            }
            "pageup" | "pagedown"
                if !key.modifiers.modified()
                    && !self.search.focus.is_focused(window)
                    && self.settings.content_focus.contains_focused(window, cx) =>
            {
                let scroll = &self.settings.content_scroll;
                let direction = if key.key == "pageup" { 1.0 } else { -1.0 };
                scroll.set_offset(point(
                    px(0.0),
                    scroll.offset().y + scroll.bounds().size.height * direction * 0.9,
                ));
                cx.notify();
            }
            _ => return,
        }
        window.prevent_default();
        cx.stop_propagation();
    }

    pub(super) fn settings_dialog(
        &mut self,
        width: f32,
        height: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = self.palette;
        let compact = width <= super::COMPACT_NAVIGATION_WIDTH;
        let section = self.settings.section;
        let form_open = self.settings_form_open();
        if self.settings.form_open != form_open {
            self.settings.form_open = form_open;
            self.settings
                .content_scroll
                .set_offset(point(px(0.0), px(0.0)));
        }
        let stacked = compact || form_open;
        let title = if self.node_share.open && self.node_share.authorization {
            "一次性授权"
        } else if self.node_share.open {
            "节点地址二维码"
        } else {
            section.label()
        };
        if self.settings.compact != compact {
            self.settings.compact = compact;
            if compact {
                self.settings.browse = !form_open
                    && self
                        .settings
                        .navigation_container
                        .contains_focused(window, cx);
            } else if self.settings.browse_focus.is_focused(window) {
                cx.on_next_frame(window, |shell, window, _| {
                    if shell.settings.open && !shell.settings.compact {
                        shell.search.focus.focus(window);
                    }
                });
            }
        }
        let browse = compact && self.settings.browse && !form_open;
        let title_in_header = stacked && !browse;
        let query = if form_open {
            ""
        } else {
            self.search.buffer.text.trim()
        };
        self.settings
            .update_search(query, &self.preferences.entries, cx);
        let dialog_width = if compact {
            width
        } else {
            (width * 0.92).min(if form_open { 820.0 } else { 1200.0 })
        };
        let dialog_height = if compact {
            height
        } else {
            (height * 0.85).min(784.0)
        };
        let rows = if browse {
            Vec::new()
        } else {
            match section {
                Section::General => self.general_settings(cx),
                Section::Appearance => self.appearance_settings(cx),
                Section::Notifications => self.notification_settings(cx),
                Section::Shortcuts => self.shortcut_settings(),
                Section::Configuration => vec![SettingsRow(
                    None,
                    self.provider_settings(cx).into_any_element(),
                )],
                Section::Review => vec![
                    self.settings_heading("结果", false),
                    self.review_delivery_setting(cx),
                ],
                Section::Plugins => vec![
                    self.settings_heading("桌面插件", false),
                    SettingsRow(None, self.desktop_plugin_settings(cx).into_any_element()),
                    self.settings_heading("节点插件", true),
                    SettingsRow(
                        None,
                        self.resource_settings(section.resource_kinds(), cx)
                            .into_any_element(),
                    ),
                ],
                Section::Skills => vec![SettingsRow(
                    None,
                    self.resource_settings(section.resource_kinds(), cx)
                        .into_any_element(),
                )],
                Section::FeatureEntries => vec![SettingsRow(
                    None,
                    self.feature_entry_settings(cx).into_any_element(),
                )],
                Section::ChatIntegrations => vec![SettingsRow(
                    None,
                    self.chat_integration_settings(cx).into_any_element(),
                )],
                Section::RpiHost => vec![SettingsRow(
                    None,
                    self.rpi_host_settings(cx).into_any_element(),
                )],
                Section::Collaboration => vec![SettingsRow(
                    None,
                    self.collaboration_settings(compact, window, cx)
                        .into_any_element(),
                )],
                Section::Archives => vec![SettingsRow(
                    None,
                    self.archive_settings(&self.settings.title_focus, cx)
                        .into_any_element(),
                )],
            }
        };
        self.settings.focus_rows.clear();
        let target = if std::mem::take(&mut self.settings.reveal) {
            self.settings.target
        } else {
            None
        };
        let rows = rows
            .into_iter()
            .enumerate()
            .map(|(index, SettingsRow(field, row))| {
                let Some((_, focus)) = self
                    .settings
                    .row_focus
                    .iter()
                    .find(|(candidate, _)| Some(*candidate) == field)
                else {
                    return row;
                };
                if target == field {
                    self.settings.content_scroll.scroll_to_item(index + 1);
                }
                self.settings.focus_rows.push((index + 1, focus.clone()));
                div()
                    .id(("settings-field", index))
                    .track_focus(focus)
                    .tab_stop(false)
                    .child(row)
                    .into_any_element()
            })
            .collect::<Vec<_>>();
        let nav =
            div()
                .flex()
                .flex_col()
                .flex_none()
                .min_h_0()
                .gap_2()
                .p_3()
                .bg(palette.side)
                .when(browse, |nav| nav.flex_1())
                .when(!stacked, |nav| {
                    nav.w(px((dialog_width / 3.0).min(275.0)))
                        .rounded_l(px(19.0))
                        .border_r_1()
                        .border_color(palette.line)
                })
                .when(stacked, |nav| nav.border_b_1().border_color(palette.line))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .child(
                            self.text(
                                if title_in_header { title } else { "设置" },
                                if title_in_header { 18.0 } else { 16.0 },
                                palette.text,
                            )
                            .font_weight(FontWeight::SEMIBOLD)
                            .pl_2(),
                        )
                        .when(form_open, |bar| {
                            bar.child(
                                self.button("settings-form-back", "返回设置")
                                    .h(px(if compact { 44.0 } else { 34.0 }))
                                    .tab_index(0)
                                    .focus(|button| button.bg(palette.hover))
                                    .on_click(cx.listener(|shell, _, window, cx| {
                                        shell.dismiss_settings_form(window, cx);
                                    })),
                            )
                        })
                        .when(compact && !form_open, |bar| {
                            bar.child(
                                self.button(
                                    "settings-browse",
                                    if browse {
                                        "返回当前页"
                                    } else {
                                        "浏览分类"
                                    },
                                )
                                .h(px(44.0))
                                .gap_2()
                                .track_focus(&self.settings.browse_focus)
                                .tab_index(0)
                                .focus(|button| button.bg(palette.hover))
                                .child(icon(
                                    if browse {
                                        "chevron-right"
                                    } else {
                                        "chevron-down"
                                    },
                                    14.0,
                                    palette.muted,
                                ))
                                .on_click(cx.listener(
                                    |shell, _, window, cx| shell.toggle_settings_browse(window, cx),
                                )),
                            )
                        })
                        .child(
                            self.navigation_tooltip(
                                self.icon_button(
                                    "settings-close",
                                    "x",
                                    if compact { 44.0 } else { 30.0 },
                                )
                                .h(px(if compact { 44.0 } else { 30.0 }))
                                .track_focus(&self.settings.close_focus)
                                .tab_index(0)
                                .border_1()
                                .border_color(gpui::transparent_black())
                                .focus(|button| button.border_color(palette.strong_line))
                                .on_click(cx.listener(
                                    |shell, _, window, cx| shell.close_settings(window, cx),
                                )),
                                "关闭设置",
                            ),
                        ),
                )
                .when(!form_open && (!compact || browse), |nav| {
                    nav.child(self.settings_navigation(compact, window, cx))
                });
        div()
            .id("settings-scrim")
            .absolute()
            .inset_0()
            .occlude()
            .bg(gpui::rgba(if self.dark { 0x00000070 } else { 0x00000030 }))
            .flex()
            .items_center()
            .justify_center()
            .capture_key_down(cx.listener(Self::settings_key))
            .child(
                div()
                    .id("settings-dialog")
                    .track_focus(&self.settings.dialog_focus)
                    .tab_group()
                    .flex()
                    .when(stacked, |dialog| dialog.flex_col())
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
                    .when(!browse, |dialog| {
                        dialog.child(
                            div()
                                .flex()
                                .flex_col()
                                .flex_1()
                                .min_h_0()
                                .min_w_0()
                                .child(
                                    div()
                                        .id(("settings-content", section as usize))
                                        .track_focus(&self.settings.content_focus)
                                        .flex_1()
                                        .min_h_0()
                                        .overflow_y_scroll()
                                        .track_scroll(&self.settings.content_scroll)
                                        .px(px(if compact { 18.0 } else { 28.0 }))
                                        .py(px(if stacked { 20.0 } else { 32.0 }))
                                        .child(
                                            div()
                                                .id("settings-title")
                                                .track_focus(&self.settings.title_focus)
                                                .tab_index(0)
                                                .tab_stop(false)
                                                .when(!title_in_header, |title| {
                                                    title.mb_5().child(
                                                        self.text(
                                                            section.label(),
                                                            28.0,
                                                            palette.text,
                                                        )
                                                        .font_weight(FontWeight::SEMIBOLD),
                                                    )
                                                }),
                                        )
                                        .children(rows),
                                )
                                .when(!form_open, |body| {
                                    body.children(self.settings_save_status(cx))
                                }),
                        )
                    })
                    .child(
                        div()
                            .id("settings-tab-end")
                            .track_focus(&self.settings.end_focus)
                            .size(px(0.0)),
                    ),
            )
    }

    fn settings_save_status(&self, cx: &mut Context<Self>) -> Option<Div> {
        let palette = self.palette;
        let retry = matches!(self.settings.save_status, SaveStatus::Failed);
        let (message, color, success) = if let Some(problem) = &self.settings.file.problem {
            (
                format!("设置文件读取失败。{problem}"),
                palette.danger,
                false,
            )
        } else if self.settings.file_open_error {
            ("无法打开或创建本机设置文件。".into(), palette.danger, false)
        } else {
            match self.settings.save_status {
                SaveStatus::Idle => return None,
                SaveStatus::Saved => ("已保存到本机".into(), palette.success, true),
                SaveStatus::Reloaded => ("已应用设置文件的更改".into(), palette.success, true),
                SaveStatus::Failed => ("设置已应用，本机保存失败。".into(), palette.danger, false),
            }
        };
        Some(
            div()
                .flex()
                .flex_none()
                .items_center()
                .justify_between()
                .gap_2()
                .px_4()
                .min_h(px(44.0))
                .border_t_1()
                .border_color(palette.line)
                .bg(palette.side)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .min_w_0()
                        .when(success, |status| status.child(icon("check", 14.0, color)))
                        .child(self.text(message, 12.0, color)),
                )
                .when(retry, |status| {
                    status.child(
                        self.setting_choice("settings-save-retry", "重试保存", false)
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.save_preferences(window, cx);
                                if matches!(shell.settings.save_status, SaveStatus::Saved) {
                                    shell.settings.title_focus.focus(window);
                                }
                            })),
                    )
                }),
        )
    }

    fn general_settings(&self, cx: &mut Context<Self>) -> Vec<SettingsRow> {
        vec![
            self.settings_heading("聊天", false),
            self.settings_row(
                Field::FollowUp,
                div().flex().flex_wrap().gap_1().children(
                    [
                        ("settings-follow-up", "加入队列", Delivery::FollowUp),
                        ("settings-steer", "引导当前任务", Delivery::Steer),
                    ]
                    .map(|(id, label, delivery)| {
                        self.setting_option(id, label, self.preferences.follow_up == delivery, 94.0)
                            .on_click(cx.listener(move |shell, _, window, cx| {
                                shell.preferences.follow_up = delivery;
                                shell.save_preferences(window, cx);
                            }))
                    }),
                ),
            ),
            self.settings_row(
                Field::Context,
                self.setting_switch("settings-context", self.preferences.show_context)
                    .on_click(cx.listener(|shell, _, _, cx| {
                        shell.preferences.show_context = !shell.preferences.show_context;
                        shell.save_device_preferences();
                        cx.notify();
                    })),
            ),
            self.settings_heading("工作区", true),
            self.settings_row(
                Field::ProjectGrouping,
                div().flex().flex_wrap().gap_1().children(
                    [
                        ("settings-projects-grouped", "按节点分组", false),
                        ("settings-projects-flat", "平铺", true),
                    ]
                    .map(|(id, label, flat)| {
                        self.setting_option(
                            id,
                            label,
                            self.preferences.navigation.flat == flat,
                            82.0,
                        )
                        .on_click(cx.listener(move |shell, _, _, cx| {
                            shell.preferences.navigation.flat = flat;
                            shell.project_menu = None;
                            shell.save_device_preferences();
                            cx.notify();
                        }))
                    }),
                ),
            ),
            self.review_delivery_setting(cx),
            self.update_setting(cx),
            self.settings_heading("高级", true),
            self.settings_file_row(cx),
        ]
    }

    fn update_setting(&self, cx: &mut Context<Self>) -> SettingsRow {
        let update = cx.global::<Updates>();
        let (message, error) = match &update.status {
            UpdateStatus::Idle if !update.enabled => ("请通过发布页更新此构建。".into(), false),
            UpdateStatus::Idle => ("尚未检查更新。".into(), false),
            UpdateStatus::Checking => ("正在检查更新…".into(), false),
            UpdateStatus::Current => ("已是最新版本。".into(), false),
            UpdateStatus::Available(release) => {
                (format!("可更新至 v{}。", release.manifest.version), false)
            }
            UpdateStatus::Downloading { version, percent } => {
                (format!("正在下载 v{version} · {percent}%"), false)
            }
            UpdateStatus::Ready { version, .. } => match &update.install_problem {
                Some(problem) => (format!("安装失败：{problem}"), true),
                None => (format!("v{version} 已下载并通过校验。"), false),
            },
            UpdateStatus::Installing => ("正在准备安装…".into(), false),
            UpdateStatus::Failed(problem) => (problem.clone(), true),
        };
        let actions = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .when(update.enabled, |actions| {
                actions.child(
                    self.setting_choice("settings-check-update", "检查更新", false)
                        .when(
                            !update.busy() && !matches!(update.status, UpdateStatus::Ready { .. }),
                            |button| button.on_click(cx.listener(|_, _, _, cx| Updates::check(false, cx))),
                        )
                        .when(update.busy() || matches!(update.status, UpdateStatus::Ready { .. }), |button| {
                            button.opacity(0.5).tab_stop(false)
                        }),
                )
            })
            .when(matches!(update.status, UpdateStatus::Available(_)), |actions| {
                actions.child(self.setting_choice("settings-download-update", "下载更新", true)
                    .on_click(cx.listener(|_, _, _, cx| Updates::download(cx))))
            })
            .when(matches!(update.status, UpdateStatus::Ready { .. }), |actions| {
                actions.child(self.setting_choice("settings-install-update", "重启并安装", true)
                    .on_click(cx.listener(|_, _, window, cx| {
                        let answer = window.prompt(
                            gpui::PromptLevel::Warning,
                            "重启并安装更新？",
                            Some("重启会关闭所有工作台窗口。请先发送或保存草稿，并等待正在运行的任务结束。"),
                            &[gpui::PromptButton::ok("重启并安装"), gpui::PromptButton::cancel("稍后")],
                            cx,
                        );
                        cx.spawn(async move |_, cx| {
                            if answer.await == Ok(0) {
                                let _ = cx.update(Updates::install);
                            }
                        }).detach();
                    })))
            })
            .child(self.setting_choice("settings-update-releases", "发布记录", false)
                .on_click(cx.listener(|_, _, _, cx| cx.open_url(work_update::RELEASES_URL))));
        self.settings_row(
            Field::AppUpdate,
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(self.text(format!("当前版本 v{VERSION}"), 13.0, self.palette.text))
                .child(self.text(
                    message,
                    12.0,
                    if error {
                        self.palette.danger
                    } else {
                        self.palette.muted
                    },
                ))
                .when(update.enabled, |control| {
                    control.child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(self.text("自动检查和下载更新", 13.0, self.palette.text))
                            .child(
                                self.setting_switch(
                                    "settings-automatic-updates",
                                    self.preferences.automatic_updates,
                                )
                                .on_click(cx.listener(
                                    |shell, _, _, cx| {
                                        shell.preferences.automatic_updates =
                                            !shell.preferences.automatic_updates;
                                        shell.save_device_preferences();
                                        Updates::set_automatic(
                                            shell.preferences.automatic_updates,
                                            cx,
                                        );
                                        cx.notify();
                                    },
                                )),
                            ),
                    )
                })
                .child(actions),
        )
    }

    pub(super) fn update_notice(&self, cx: &mut Context<Self>) -> Option<gpui::Stateful<Div>> {
        if self.settings.open || !self.navigation_keyboard_enabled() {
            return None;
        }
        let status = &cx.global::<Updates>().status;
        let label = match status {
            UpdateStatus::Ready { version, .. } => format!("v{version} 已下载 · 查看更新"),
            UpdateStatus::Failed(_)
                if std::env::var_os(work_update::native::gpui::RESULT_ENV).is_some() =>
            {
                "应用更新失败 · 查看详情".into()
            }
            _ => return None,
        };
        Some(
            self.setting_choice("workbench-update", &label, false)
                .on_click(cx.listener(|shell, _, window, cx| {
                    shell.open_settings(window, cx);
                    shell.select_settings_destination(
                        Destination {
                            section: Section::General,
                            field: Some(Field::AppUpdate),
                        },
                        window,
                        cx,
                    );
                })),
        )
    }

    pub(super) fn apply_feature_visibility(&mut self) {
        if !self.preferences.entries.shows(Entry::Discovery) {
            self.stop_nearby_discovery();
        }
        if !self.preferences.entries.shows(Entry::Mcp) {
            self.hide_mcp_tab();
        }
        if !self.settings.section.visible(&self.preferences.entries) {
            self.chat_integrations = Default::default();
            self.search.clear();
            self.settings.section = Section::FeatureEntries;
        }
    }

    fn feature_entry_settings(&self, cx: &mut Context<Self>) -> Div {
        let palette = self.palette;
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(self.text(
                "入口开关保存在本机。节点共享和 Web 访问权限在「工作协同」中管理。",
                13.0,
                palette.muted,
            ))
            .children(Entry::ALL.into_iter().map(|entry| {
                let visible = self.preferences.entries.shows(entry);
                let detail = match entry {
                    Entry::Goal => "在侧栏显示目标模式的检查和安装入口。",
                    Entry::Collaboration => self.node_access_summary(),
                    Entry::Discovery => "在工作协同中显示附近节点与刷新发现。",
                    Entry::Mcp => "在「技能与插件」中显示 MCP 服务页签。",
                    Entry::Web => self.web_access_summary(),
                    Entry::Chat => "在设置中显示飞书和企业微信机器人配置。",
                    Entry::RpiHost => "在设置中显示本机 rpi Host area 和 Agent。",
                };
                div()
                    .border_t_1()
                    .border_color(palette.line)
                    .pt_3()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .justify_between()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .child(
                                        self.text(entry.label(), 14.0, palette.text)
                                            .font_weight(FontWeight::MEDIUM),
                                    )
                                    .child(self.text(detail, 12.0, palette.muted).mt_1()),
                            )
                            .child(
                                self.setting_switch(entry.key(), visible)
                                    .flex_none()
                                    .on_click(cx.listener(move |shell, _, _, cx| {
                                        shell.preferences.entries.toggle(entry);
                                        shell.apply_feature_visibility();
                                        shell.save_device_preferences();
                                        if shell.preferences.entries.shows(entry) {
                                            match entry {
                                                Entry::Goal | Entry::Mcp | Entry::Web => {
                                                    shell.read_feature_status(cx)
                                                }
                                                Entry::Collaboration
                                                | Entry::Discovery
                                                | Entry::Chat
                                                | Entry::RpiHost => {}
                                            }
                                        }
                                        cx.notify();
                                    })),
                            ),
                    )
                    .when(visible && entry == Entry::Goal, |row| {
                        row.child(self.goal_entry_status(cx))
                    })
                    .when(visible, |row| {
                        let dependencies: &[&str] = match entry {
                            Entry::Mcp => &feature_entries::MCP_PACKAGES,
                            Entry::Web => &[feature_entries::WEB_PACKAGE],
                            _ => &[],
                        };
                        row.children(dependencies.iter().map(|package| {
                            self.text(
                                format!("{package} · {}", self.extension_status(package)),
                                12.0,
                                palette.muted,
                            )
                            .mt_2()
                        }))
                    })
            }))
            .child(
                div().child(
                    self.setting_choice("feature-status-refresh", "重新检查依赖", false)
                        .on_click(cx.listener(|shell, _, _, cx| shell.read_feature_status(cx))),
                ),
            )
    }

    fn goal_entry_status(&self, cx: &Context<Self>) -> Div {
        let palette = self.palette;
        let installed = self.confirmed_extension(feature_entries::GOAL_PACKAGE);
        div()
            .flex()
            .flex_col()
            .gap_2()
            .mt_2()
            .child(self.text(
                format!(
                    "{} · {}",
                    feature_entries::GOAL_PACKAGE,
                    self.extension_status(feature_entries::GOAL_PACKAGE)
                ),
                13.0,
                palette.text,
            ))
            .child(self.text(
                if installed {
                    "在执行节点重新启动 rpi，确认会话已加载扩展，再输入 /goal status。"
                } else {
                    "在当前节点安装 rpi-goal，完成后重新检查依赖。"
                },
                12.0,
                palette.muted,
            ))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .when(!installed, |row| {
                        row.child(
                            self.setting_choice("goal-copy-install", "复制安装命令", false)
                                .on_click(cx.listener(|_, _, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(
                                        feature_entries::GOAL_INSTALL.into(),
                                    ));
                                })),
                        )
                    })
                    .child(
                        self.setting_choice(
                            "goal-guide",
                            if installed {
                                "目标模式用法"
                            } else {
                                "安装说明"
                            },
                            false,
                        )
                        .on_click(|_, _, cx| cx.open_url(feature_entries::GOAL_GUIDE)),
                    ),
            )
    }

    fn desktop_plugin_settings(&self, cx: &mut Context<Self>) -> Div {
        let palette = self.palette;
        let catalog = &self.desktop_plugins;
        let Some(directory) = catalog.directory.clone() else {
            return div().child(self.text("本机插件目录不可用。", 13.0, palette.muted));
        };
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(self.text("在输入框键入 /，选择模板填入草稿。", 13.0, palette.muted))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        self.setting_choice("desktop-plugin-example", "安装示例", false)
                            .on_click(cx.listener(|shell, _, _, cx| {
                                if let Some(directory) = &shell.desktop_plugins.directory {
                                    match desktop_plugins::install_example(directory) {
                                        Ok(()) => {
                                            shell.desktop_plugins =
                                                desktop_plugins::Catalog::load_local()
                                        }
                                        Err(error) => shell
                                            .desktop_plugins
                                            .problems
                                            .push(format!("无法安装示例：{error}")),
                                    }
                                    cx.notify();
                                }
                            })),
                    )
                    .child(
                        self.setting_choice("desktop-plugin-directory", "打开插件目录", false)
                            .on_click(cx.listener(move |shell, _, _, cx| {
                                match std::fs::create_dir_all(&directory) {
                                    Ok(()) => cx.open_with_system(&directory),
                                    Err(error) => {
                                        shell
                                            .desktop_plugins
                                            .problems
                                            .push(format!("无法打开插件目录：{error}"));
                                        cx.notify();
                                    }
                                }
                            })),
                    )
                    .child(
                        self.setting_choice("desktop-plugin-reload", "重新加载桌面插件", false)
                            .on_click(cx.listener(|shell, _, _, cx| {
                                shell.desktop_plugins = desktop_plugins::Catalog::load_local();
                                cx.notify();
                            })),
                    ),
            )
            .when(catalog.plugins.is_empty(), |panel| {
                panel.child(self.text("尚未加载桌面插件。", 13.0, palette.muted))
            })
            .children(catalog.plugins.iter().map(|plugin| {
                div()
                    .border_t_1()
                    .border_color(palette.line)
                    .py_2()
                    .child(self.text(plugin.name.clone(), 14.0, palette.text))
                    .child(
                        div().flex().flex_wrap().gap_2().children(
                            plugin
                                .prompts
                                .iter()
                                .map(|prompt| self.text(prompt.token.clone(), 12.0, palette.muted)),
                        ),
                    )
            }))
            .children(
                catalog
                    .problems
                    .iter()
                    .map(|problem| self.text(problem.clone(), 13.0, palette.danger).min_w_0()),
            )
    }

    fn settings_file_row(&self, cx: &mut Context<Self>) -> SettingsRow {
        let control = div()
            .flex()
            .flex_wrap()
            .gap_1()
            .when_some(self.settings.file.path.clone(), |control, path| {
                control
                    .child(
                        self.setting_choice("settings-edit-json", "编辑设置 JSON", false)
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.edit_settings_file(window, cx)
                            })),
                    )
                    .child(
                        self.navigation_tooltip(
                            self.setting_choice("settings-copy-json-path", "复制路径", false)
                                .on_click(cx.listener({
                                    let path = path.to_string_lossy().into_owned();
                                    move |_, _, _, cx| {
                                        cx.write_to_clipboard(ClipboardItem::new_string(
                                            path.clone(),
                                        ))
                                    }
                                })),
                            path.to_string_lossy().into_owned(),
                        ),
                    )
            })
            .when(self.settings.file.path.is_none(), |control| {
                control.child(self.text("本机设置目录不可用。", 12.0, self.palette.muted))
            });
        self.settings_row(Field::SettingsFile, control)
    }

    fn review_delivery_setting(&self, cx: &mut Context<Self>) -> SettingsRow {
        self.settings_row(
            Field::ReviewDelivery,
            div().flex().flex_wrap().gap_1().children(
                [
                    ("settings-review-inline", "当前会话", false),
                    ("settings-review-detached", "独立会话", true),
                ]
                .map(|(id, label, detached)| {
                    self.setting_option(
                        id,
                        label,
                        self.preferences.detached_review == detached,
                        68.0,
                    )
                    .on_click(cx.listener(move |shell, _, _, cx| {
                        shell.preferences.detached_review = detached;
                        shell.save_device_preferences();
                        cx.notify();
                    }))
                }),
            ),
        )
    }

    fn appearance_settings(&self, cx: &mut Context<Self>) -> Vec<SettingsRow> {
        let mut rows = vec![
            self.settings_heading("主题", false),
            self.settings_row(
                Field::Color,
                div().flex().flex_wrap().gap_1().children(
                    [
                        ("settings-system", "跟随系统", ColorMode::System),
                        ("settings-light", "浅色", ColorMode::Light),
                        ("settings-dark", "深色", ColorMode::Dark),
                    ]
                    .map(|(id, label, color)| {
                        self.setting_option(id, label, self.preferences.color == color, 68.0)
                            .on_click(cx.listener(move |shell, _, window, cx| {
                                shell.preferences.color = color;
                                shell.save_preferences(window, cx);
                            }))
                    }),
                ),
            ),
            self.settings_row(Field::Theme, self.theme_choices(cx)),
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
            ]
            .map(|(id, title, first, second, active, apply)| {
                self.settings_row(
                    title,
                    div().flex().flex_wrap().gap_1().children(
                        [(first, false), (second, true)].map(|(label, value)| {
                            self.setting_option(
                                (id, usize::from(value)),
                                label,
                                value == active,
                                56.0,
                            )
                            .on_click(cx.listener(
                                move |shell, _, window, cx| {
                                    apply(&mut shell.preferences, value);
                                    shell.save_preferences(window, cx);
                                },
                            ))
                        }),
                    ),
                )
            }),
        );
        rows.push(
            self.settings_row(
                Field::CodeNumbers,
                self.setting_switch("settings-code-numbers", self.preferences.code_numbers)
                    .on_click(cx.listener(|shell, _, _, cx| {
                        shell.preferences.code_numbers = !shell.preferences.code_numbers;
                        shell.save_device_preferences();
                        cx.notify();
                    })),
            ),
        );
        rows
    }

    fn theme_choices(&self, cx: &mut Context<Self>) -> Div {
        let palette = self.palette;
        div()
            .flex()
            .flex_wrap()
            .gap_2()
            .children(
                Palette::presets()
                    .into_iter()
                    .enumerate()
                    .map(|(index, (id, label))| {
                        let preview =
                            Palette::from_preset(&id, self.dark, self.preferences.compact);
                        let selected = self.preferences.theme == id;
                        self.setting_choice(("settings-preset", index), "", selected)
                            .w(px(if self.settings.compact { 140.0 } else { 104.0 }))
                            .h(px(62.0))
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .w_full()
                                    .h(px(24.0))
                                    .px_2()
                                    .rounded(px(4.0))
                                    .bg(preview.bg)
                                    .border_1()
                                    .border_color(preview.line)
                                    .child(div().flex().gap_1().children(
                                        [preview.side, preview.keyword, preview.text].map(
                                            |color| div().size(px(9.0)).rounded_full().bg(color),
                                        ),
                                    ))
                                    .when(selected, |swatch| {
                                        swatch.child(icon("check", 13.0, preview.text))
                                    }),
                            )
                            .child(self.text(label, 12.0, palette.text))
                            .on_click(cx.listener(move |shell, _, window, cx| {
                                shell.preferences.theme.clone_from(&id);
                                shell.save_preferences(window, cx);
                            }))
                    }),
            )
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
            ("浏览设置搜索结果", &["↑", "↓"]),
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
        vec![
            self.settings_heading("工作台提示", false),
            self.settings_row(
                Field::NotifyFinish,
                self.setting_switch("settings-notify-finish", self.preferences.notify_finish)
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
        let stacked = matches!(field, Field::Theme | Field::AppUpdate)
            || (self.settings.compact
                && field != Field::Context
                && field != Field::CodeNumbers
                && field != Field::NotifyFinish);
        SettingsRow(
            Some(field),
            div()
                .flex()
                .gap_3()
                .min_h(px(64.0))
                .py_3()
                .border_t_1()
                .border_color(self.palette.line)
                .when(stacked, |row| row.flex_col().items_start())
                .when(!stacked, |row| row.items_center().justify_between())
                .child(
                    div()
                        .when(!stacked, |label| label.flex_1())
                        .min_w_0()
                        .child(
                            self.text(field.label(), 14.0, self.palette.text)
                                .font_weight(FontWeight::MEDIUM),
                        )
                        .child(
                            self.text(field.description(), 12.0, self.palette.muted)
                                .mt_1(),
                        ),
                )
                .child(
                    div()
                        .when(stacked, |control| control.w_full())
                        .when(!stacked, |control| control.flex_none())
                        .child(control),
                )
                .into_any_element(),
        )
    }

    pub(super) fn setting_switch(&self, id: &'static str, enabled: bool) -> gpui::Stateful<Div> {
        let palette = self.palette;
        self.setting_choice(id, "", false)
            .border_color(gpui::transparent_black())
            .gap_2()
            .child(self.text(if enabled { "开启" } else { "关闭" }, 12.0, palette.muted))
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
                    .child(div().size(px(14.0)).rounded_full().bg(palette.bg)),
            )
    }

    fn setting_option(
        &self,
        id: impl Into<ElementId>,
        label: &str,
        selected: bool,
        width: f32,
    ) -> gpui::Stateful<Div> {
        self.setting_choice(id, label, selected)
            .when(!self.settings.compact, |button| button.w(px(width)))
    }

    pub(super) fn setting_choice(
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
        self.button(id, "")
            .when(!label.is_empty(), |button| {
                button.child(self.text(label.to_owned(), 13.0, palette.text))
            })
            .when(target, |button| {
                button.track_focus(&self.settings.target_focus)
            })
            .h(px(if self.settings.compact { 44.0 } else { 34.0 }))
            .gap_1()
            .tab_index(0)
            .border_1()
            .border_color(palette.line)
            .when(selected, |button| {
                button
                    .bg(palette.hover)
                    .font_weight(FontWeight::SEMIBOLD)
                    .border_color(palette.muted)
            })
            .focus(|button| button.border_color(palette.text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_search_matches_aliases_and_prioritizes_exact_names() {
        let cases = [
            ("通知", Section::Notifications, None),
            ("FONT SIZE", Section::Appearance, Some(Field::FontSize)),
            ("海湾蓝", Section::Appearance, Some(Field::Theme)),
            ("REDDIT", Section::Appearance, Some(Field::Theme)),
            ("API KEY", Section::Configuration, None),
            ("代码行号", Section::Appearance, Some(Field::CodeNumbers)),
            ("SETTINGS JSON", Section::General, Some(Field::SettingsFile)),
            ("自动更新", Section::General, Some(Field::AppUpdate)),
            ("飞书", Section::ChatIntegrations, None),
            ("wecom", Section::ChatIntegrations, None),
            ("native-host", Section::RpiHost, None),
        ];
        for (query, section, field) in cases {
            let results = search_destinations(query);
            assert!(
                results
                    .first()
                    .is_some_and(|result| { result.section == section && result.field == field }),
                "unexpected first result for {query}"
            );
        }
        assert!(search_destinations("font missingword").is_empty());
    }
}
