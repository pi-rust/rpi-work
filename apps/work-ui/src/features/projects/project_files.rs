use crate::shared::components::Icon;
use dioxus::core::Task;
use dioxus::prelude::*;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use work_client::{FileEntry, WorkspaceClient};

#[derive(Clone, Default)]
struct Tree {
    children: BTreeMap<String, Vec<FileEntry>>,
    expanded: BTreeSet<String>,
    loading: BTreeSet<String>,
    errors: BTreeMap<String, String>,
    revisions: BTreeMap<String, u64>,
}

fn valid_child(parent: &str, entry: &FileEntry) -> bool {
    let path = entry.path.as_str();
    if path.is_empty()
        || path.contains(['\\', ':'])
        || path.chars().any(char::is_control)
        || path.split('/').any(|part| matches!(part, "" | "." | ".."))
    {
        return false;
    }
    let (actual_parent, _) = path.rsplit_once('/').unwrap_or(("", path));
    actual_parent == parent
}

#[component]
pub fn Browser(
    client: Signal<Option<WorkspaceClient>>,
    project: String,
    scope: String,
    selected: Option<String>,
    onselect: EventHandler<String>,
    oninitial: EventHandler<String>,
    onreported: EventHandler<Vec<FileEntry>>,
) -> Element {
    let mut tree = use_signal(Tree::default);
    let mut query = use_signal(String::new);
    let mut initialized = use_signal(|| false);
    let epoch = use_signal(|| 0u64);
    let mut tasks = use_signal(Vec::<Task>::new);

    let mut target = use_signal(|| (scope.clone(), project.clone()));
    if target.peek().0 != scope {
        target.set((scope.clone(), project.clone()));
        for task in tasks.write().drain(..) {
            task.cancel();
        }
        tree.set(Tree::default());
        query.set(String::new());
        initialized.set(false);
        let mut epoch = epoch;
        epoch += 1;
    }
    use_drop(move || {
        for task in tasks.write().drain(..) {
            task.cancel();
        }
    });

    let scope_for_load = scope.clone();
    let load = move |path: String| {
        let Some(node) = client.read().clone() else {
            return;
        };
        let (request_scope, project_id) = target.peek().clone();
        let request_epoch = *epoch.read();
        let revision = {
            let mut state = tree.write();
            let revision = state.revisions.get(&path).copied().unwrap_or(0) + 1;
            state.revisions.insert(path.clone(), revision);
            state.loading.insert(path.clone());
            state.errors.remove(&path);
            revision
        };
        let mut tree_signal = tree;
        let epoch_signal = epoch;
        let task = spawn(async move {
            let result = node.files(&project_id, &path).await;
            if target.peek().0 != request_scope
                || *epoch_signal.read() != request_epoch
                || tree_signal.peek().revisions.get(&path) != Some(&revision)
            {
                return;
            }
            let mut state = tree_signal.write();
            state.loading.remove(&path);
            match result {
                Ok(listing) => {
                    let original_count = listing.entries.len();
                    let mut seen = HashSet::new();
                    let entries: Vec<FileEntry> = listing
                        .entries
                        .into_iter()
                        .filter(|entry| {
                            valid_child(&path, entry) && seen.insert(entry.path.clone())
                        })
                        .collect();
                    if original_count != entries.len() || listing.path != path {
                        state.errors.insert(
                            path.clone(),
                            "节点返回了重复或越界目录条目，已忽略；可重试。".into(),
                        );
                    }
                    let first = if path.is_empty() && !*initialized.peek() {
                        entries
                            .iter()
                            .filter(|entry| entry.kind != "directory")
                            .min_by_key(|entry| {
                                (
                                    !entry.name.to_ascii_lowercase().starts_with("readme"),
                                    entry.name.clone(),
                                )
                            })
                            .map(|entry| entry.path.clone())
                    } else {
                        None
                    };
                    state.children.insert(path.clone(), entries);
                    let reported = state.children.values().flatten().cloned().collect();
                    drop(state);
                    onreported.call(reported);
                    if path.is_empty() {
                        initialized.set(true);
                    }
                    if let Some(first) = first {
                        oninitial.call(first);
                    }
                }
                Err(error) => {
                    state.errors.insert(path, error.to_string());
                }
            }
        });
        tasks.write().push(task);
    };
    let mut initial_load = load;
    use_effect(use_reactive((&scope_for_load,), move |(_scope,)| {
        initial_load(String::new());
    }));
    let snapshot = tree.read().clone();
    let filter = query.read().trim().to_ascii_lowercase();
    let loaded_files = snapshot
        .children
        .values()
        .flatten()
        .filter(|entry| entry.kind != "directory")
        .count();
    let mut rows = Vec::new();
    if filter.is_empty() {
        visible_rows(&snapshot, "", 0, &mut rows, &mut HashSet::new());
    } else {
        let mut seen = HashSet::new();
        for entry in snapshot
            .children
            .values()
            .flatten()
            .filter(|entry| entry.path.to_ascii_lowercase().contains(&filter))
        {
            if seen.insert(entry.path.clone()) {
                rows.push((entry.clone(), 0));
            }
        }
    }
    let mut refresh = load;
    rsx! {
        aside { class: "project-file-browser",
            div { class: "project-file-search", Icon { name: "search" } input { aria_label: "搜索项目文件", placeholder: "搜索已加载文件名或路径…", value: "{query}", oninput: move |event| query.set(event.value()) } }
            div { class: "project-file-caption",
                span { "文件树 · 已加载 {loaded_files} 文件" }
                div { class: "project-file-actions",
                    button { aria_label: "折叠所有目录", onclick: move |_| tree.write().expanded.clear(), "折叠" }
                    button { aria_label: "刷新根目录", onclick: move |_| refresh(String::new()), "刷新" }
                }
            }
            div { class: "project-file-tree",
                if snapshot.loading.contains("") { p { class: "project-no-results", role: "status", "正在加载根目录…" } }
                if let Some(error) = snapshot.errors.get("") { p { role: "alert", class: "project-no-results error-text", "读取目录失败：{error} · 请刷新重试" } }
                if rows.is_empty() && !snapshot.loading.contains("") && !snapshot.errors.contains_key("") { p { class: "project-no-results", "没有已加载的匹配条目。" } }
                for (entry, depth) in rows {
                    {
                        let path = entry.path.clone(); let directory = entry.kind == "directory";
                        let expanded = snapshot.expanded.contains(&path); let active = selected.as_ref() == Some(&path);
                        let mut expand_load = load; let retry_path = path.clone(); let mut retry = load;
                        rsx! {
                            button { key: "tree-{path}", class: if active { "project-file-row selected" } else { "project-file-row" }, style: "padding-left:{14 + depth * 14}px", aria_expanded: if directory { Some(expanded) } else { None },
                                onclick: move |_| if directory {
                                    if tree.peek().expanded.contains(&path) { tree.write().expanded.remove(&path); }
                                    else { tree.write().expanded.insert(path.clone()); if !tree.peek().children.contains_key(&path) && !tree.peek().loading.contains(&path) { expand_load(path.clone()); } }
                                } else { onselect.call(path.clone()) },
                                if directory { span { class: "project-tree-chevron", if expanded { "⌄" } else { "›" } } }
                                Icon { name: if directory { "folder" } else { "file" } } span { "{entry.name}" }
                                if directory && expanded { small { "{snapshot.children.get(&entry.path).map_or(0, Vec::len)} 项" } }
                            }
                            if expanded && snapshot.loading.contains(&entry.path) { p { class: "project-tree-feedback", role: "status", "正在加载目录…" } }
                            if expanded { if let Some(error) = snapshot.errors.get(&entry.path) { div { class: "project-tree-feedback", role: "alert", "{error}" button { onclick: move |_| retry(retry_path.clone()), "重试读取目录" } } } }
                        }
                    }
                }
            }
            footer { class: "project-files-footer", "根目录 {snapshot.children.get(\"\").map_or(0, Vec::len)} 项 · 搜索仅含已加载条目" span { "由执行节点提供目录和内容 · 只读" } }
        }
    }
}

fn visible_rows(
    tree: &Tree,
    path: &str,
    depth: usize,
    rows: &mut Vec<(FileEntry, usize)>,
    visited: &mut HashSet<String>,
) {
    if depth > 128 || !visited.insert(path.to_owned()) {
        return;
    }
    if let Some(entries) = tree.children.get(path) {
        let mut seen = HashSet::new();
        for entry in entries {
            if !valid_child(path, entry) || !seen.insert(entry.path.clone()) {
                continue;
            }
            rows.push((entry.clone(), depth));
            if entry.kind == "directory" && tree.expanded.contains(&entry.path) {
                visible_rows(tree, &entry.path, depth + 1, rows, visited);
            }
        }
    }
}
