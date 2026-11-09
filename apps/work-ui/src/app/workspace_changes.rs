use dioxus::prelude::*;
use work_client::{ChangedFile, Diff, GitStatus};

use crate::features::projects::review::{change_status_label, render_diff};
use crate::shared::components::Icon;

#[derive(Props, Clone, PartialEq)]
pub(crate) struct ChangesPanelProps {
    pub(crate) repository: Option<GitStatus>,
    pub(crate) visible_changes: Vec<ChangedFile>,
    pub(crate) git_loading: bool,
    pub(crate) git_error: Option<String>,
    pub(crate) change_mobile_preview: Signal<bool>,
    pub(crate) change_count: String,
    pub(crate) reviewed_count: usize,
    pub(crate) change_query: Signal<String>,
    pub(crate) change_directory_filter: Signal<String>,
    pub(crate) change_status_filter: Signal<String>,
    pub(crate) changes_unreviewed: Signal<bool>,
    pub(crate) git_refresh: Signal<u64>,
    pub(crate) reviewed_changes: Signal<std::collections::HashSet<String>>,
    pub(crate) selected_change_file: Signal<Option<String>>,
    pub(crate) change_staged: Signal<bool>,
    pub(crate) change_diff: Signal<Option<Diff>>,
    pub(crate) change_error: Signal<Option<String>>,
    pub(crate) onopen: EventHandler<(String, bool)>,
}

#[component]
pub(crate) fn ChangesPanel(props: ChangesPanelProps) -> Element {
    let ChangesPanelProps {
        repository,
        visible_changes,
        git_loading,
        git_error,
        mut change_mobile_preview,
        change_count,
        reviewed_count,
        mut change_query,
        mut change_directory_filter,
        mut change_status_filter,
        mut changes_unreviewed,
        mut git_refresh,
        mut reviewed_changes,
        selected_change_file,
        change_staged,
        change_diff,
        change_error,
        onopen,
    } = props;

    rsx! {
        div {
            id: "changes-panel",
            role: "tabpanel",
            aria_busy: git_loading,
            aria_labelledby: "project-changes-tab",
            class: if *change_mobile_preview.read() { "change-workspace show-diff" } else { "change-workspace" },
            div { class: "review-overview",
                div { class: "review-heading",
                    h2 { "变更" span { class: "review-total", "{change_count}" } }
                    if let Some(branch) = repository.as_ref().and_then(|repo| repo.branch.as_ref()) {
                        p { class: "review-branch", title: "{branch}", "{branch}" }
                    }
                }
                div { class: "review-actions",
                    span { class: "review-progress", title: "仅记录本次阅读进度，不代表 Git 暂存或提交；刷新后重置。", "{reviewed_count}/{change_count} 已审阅" }
                }
            }
            div { class: "change-layout",
                aside { class: "change-browser",
                    div { class: "change-filters",
                        div { class: "change-search",
                            Icon { name: "search".to_owned() }
                            input { aria_label: "搜索变更文件", placeholder: "搜索文件…", value: "{change_query}", oninput: move |event| change_query.set(event.value()) }
                        }
                        div { class: "change-filter-row",
                            input { aria_label: "筛选目录", placeholder: "所有目录", value: "{change_directory_filter}", oninput: move |event| change_directory_filter.set(event.value()) }
                            select { aria_label: "筛选变更类型", value: "{change_status_filter}", onchange: move |event| change_status_filter.set(event.value()),
                                option { value: "all", "全部类型" }
                                option { value: "M", "已修改 M" }
                                option { value: "A", "新增 A" }
                                option { value: "D", "删除 D" }
                                option { value: "R", "重命名 R" }
                                option { value: "C", "复制 C" }
                                option { value: "U", "冲突 U" }
                                option { value: "T", "类型变化 T" }
                            }
                        }
                        div { class: "change-filter-footer",
                            label { input { r#type: "checkbox", checked: *changes_unreviewed.read(), onchange: move |event| changes_unreviewed.set(event.checked()) } "仅未审阅" }
                            button {
                                class: "change-clear-filters",
                                disabled: change_query.read().is_empty() && change_directory_filter.read().is_empty() && change_status_filter.read().as_str() == "all" && !*changes_unreviewed.read(),
                                onclick: move |_| {
                                    change_query.set(String::new());
                                    change_directory_filter.set(String::new());
                                    change_status_filter.set("all".into());
                                    changes_unreviewed.set(false);
                                },
                                "清除筛选"
                            }
                        }
                    }
                    div { class: "change-list-caption", span { "文件" } span { "{visible_changes.len()} / {change_count}" } }
                    if git_loading {
                        div { class: "change-no-results", role: "status", "正在读取 Git 状态…" }
                    } else if let Some(error) = git_error.as_ref() {
                        div { class: "change-no-results error-text", role: "alert", strong { "Git 状态读取失败" } p { "{error}" }
                            button { onclick: move |_| git_refresh += 1, "重试读取变更" }
                        }
                    } else if repository.is_none() {
                        div { class: "change-no-results", "连接项目后加载 Git 状态。" }
                    } else if repository.as_ref().is_some_and(|repo| !repo.is_repository) {
                        div { class: "change-no-results", "当前项目不是 Git 仓库。" }
                    } else if repository.as_ref().is_some_and(|repo| repo.files.is_empty()) {
                        div { class: "change-no-results", "工作区干净，没有变更。" }
                    } else if visible_changes.is_empty() {
                        div { class: "change-no-results", strong { "没有匹配的变更" } p { "试试其他路径、类型或审阅筛选。" } }
                    } else {
                        div { class: "change-file-list",
                            for changed in visible_changes.iter() {
                                {
                                    let path = changed.path.clone();
                                    let status_code = change_status_label(&changed.status);
                                    let (directory, filename) = changed.path.rsplit_once(['/', '\\']).unwrap_or(("", &changed.path));
                                    let selected = selected_change_file.read().as_ref() == Some(&path);
                                    let staged = changed.staged && !changed.unstaged;
                                    let reviewed = reviewed_changes.read().contains(&path);
                                    rsx! {
                                        button {
                                            key: "review-file-{path}",
                                            "data-change-path": "{path}", title: "{path}", aria_pressed: selected,
                                            class: if selected { "change-file selected" } else { "change-file" },
                                            onclick: move |_| onopen.call((path.clone(), staged)),
                                            Icon { name: "file".to_owned() }
                                            span { class: "change-file-name",
                                                strong { "{filename}" }
                                                small { class: "change-file-context",
                                                    if !directory.is_empty() { span { class: "change-file-directory", "{directory}" } span { "·" } }
                                                    span { class: "change-file-stage", if changed.staged && changed.unstaged { "已暂存 + 工作区变更" } else if changed.staged { "已暂存" } else if changed.status == "??" { "未追踪" } else { "工作区变更" } }
                                                }
                                                if let Some(original) = changed.original_path.as_ref() { small { class: "change-file-origin", title: "原路径：{original}", "← {original}" } }
                                            }
                                            span { class: "file-change-status status-{status_code}", title: "Git 状态：{changed.status}", "{status_code}" }
                                            span { class: if reviewed { "file-review-mark reviewed" } else { "file-review-mark" }, aria_label: if reviewed { "已审阅" } else { "未审阅" }, if reviewed { "✓" } else { "·" } }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                section { class: "change-preview",
                    if let Some(path) = selected_change_file.read().clone() {
                        {
                            let reviewed_path = path.clone();
                            let unstaged_path = path.clone();
                            let staged_path = path.clone();
                            let changed = repository.as_ref().and_then(|repo| repo.files.iter().find(|file| file.path == path));
                            let has_staged = changed.is_some_and(|file| file.staged);
                            let has_unstaged = changed.is_some_and(|file| file.unstaged);
                            rsx! {
                                div { class: "selected-file-heading",
                                    button { class: "back-to-files", aria_label: "返回文件列表", onclick: move |_| change_mobile_preview.set(false), Icon { name: "back".to_owned() } span { "文件列表" } }
                                    div { class: "review-file-title", strong { title: "{path}", "{path}" }
                                        small { if *change_staged.read() { "HEAD → 暂存区" } else { "暂存区 → 工作区" } }
                                    }
                                }
                                div { class: "review-file-toolbar",
                                    div { class: "review-diff-switch", role: "group", aria_label: "Diff 基准",
                                        button { disabled: !has_unstaged, aria_label: "工作区 diff", aria_pressed: !*change_staged.read(), onclick: move |_| onopen.call((unstaged_path.clone(), false)), "工作区" }
                                        button { disabled: !has_staged, aria_label: "已暂存 diff", aria_pressed: *change_staged.read(), onclick: move |_| onopen.call((staged_path.clone(), true)), "已暂存" }
                                    }
                                    label { class: "review-mark-control", title: "本地阅读进度，刷新变更后重置。", input { r#type: "checkbox", aria_label: "已审阅（仅本地）", checked: reviewed_changes.read().contains(&reviewed_path), onchange: move |event| {
                                        if event.checked() { reviewed_changes.write().insert(reviewed_path.clone()); } else { reviewed_changes.write().remove(&reviewed_path); }
                                    } } "标记已审阅" }
                                }
                            }
                        }
                        if let Some(patch) = change_diff.read().as_ref() {
                            div { class: "selected-diff-scroll", {render_diff(&patch.patch)} }
                        } else if let Some(error) = change_error.read().as_ref() {
                            div { class: "change-preview-empty error-text", strong { "Git patch 读取失败" } p { "{error}" }
                                button { onclick: move |_| onopen.call((path.clone(), *change_staged.peek())), "重试读取 diff" }
                            }
                        } else {
                            div { class: "change-preview-empty", "正在加载 Git patch…" }
                        }
                    } else {
                        div { class: "change-preview-empty", "从左侧选择一个文件查看 diff" }
                    }
                }
            }
        }
    }
}
