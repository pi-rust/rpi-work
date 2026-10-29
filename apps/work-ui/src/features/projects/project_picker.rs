use dioxus::prelude::*;
use work_client::{ClientError, Project, ProjectFolders, SessionSummary, WorkspaceClient};

use crate::shared::components::{Icon, Modal};

pub const CSS: &str = include_str!("project_picker.css");

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ProjectKind {
    Normal,
    MetaAgent,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ProjectMode {
    Open,
    Create,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ProjectIntent {
    pub mode: ProjectMode,
    pub path: String,
    pub name: String,
    pub kind: ProjectKind,
    pub init_git: bool,
}

/// Lexically normalizes Windows drive, UNC, and Unix absolute directories.
/// It does not touch the filesystem; the execution node remains authoritative.
pub fn normalize_directory(raw: &str) -> Option<String> {
    let mut path = raw.trim().replace('\\', "/");
    if path.is_empty() || path.chars().any(char::is_control) {
        return None;
    }

    let (root, rest, windows) = if path.len() >= 3
        && path.as_bytes()[0].is_ascii_alphabetic()
        && path.as_bytes()[1] == b':'
        && path.as_bytes()[2] == b'/'
    {
        let drive = path[..1].to_ascii_uppercase();
        (format!("{drive}:/"), path[3..].to_owned(), true)
    } else if let Some(unc) = path.strip_prefix("//") {
        let mut parts = unc.split('/').filter(|part| !part.is_empty());
        let server = parts.next()?;
        let share = parts.next()?;
        if server == "." || server == ".." || share == "." || share == ".." {
            return None;
        }
        let rest = parts.collect::<Vec<_>>().join("/");
        (format!("//{server}/{share}/"), rest, true)
    } else {
        let rest = path.strip_prefix('/')?;
        ("/".to_owned(), rest.to_owned(), false)
    };

    let mut segments = Vec::new();
    for part in rest.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            segments.pop()?;
            continue;
        }
        if windows && part.chars().any(|character| "<>:\"|?*".contains(character)) {
            return None;
        }
        segments.push(part);
    }

    path.clear();
    path.push_str(&root);
    path.push_str(&segments.join("/"));
    if path.len() > root.len() && path.ends_with('/') {
        path.pop();
    }
    Some(path)
}

/// Append exactly one portable directory component, never a path or traversal.
fn new_project_directory(parent: &str, name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty()
        || matches!(name, "." | "..")
        || name.ends_with(['.', ' '])
        || name
            .chars()
            .any(|c| c.is_control() || "\\/<>:\"|?*".contains(c))
    {
        return Err("请输入有效的新目录名称（不能包含路径分隔符或 ..）。".to_owned());
    }
    let stem = name
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    if matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && matches!(stem.as_bytes()[3], b'1'..=b'9'))
    {
        return Err("此目录名称是系统保留名称，请使用其他名称。".to_owned());
    }
    let parent = normalize_directory(parent).ok_or("请输入有效的绝对父目录。")?;
    let target = normalize_directory(&format!("{}/{name}", parent.trim_end_matches('/')))
        .ok_or("无法生成新项目目录。")?;
    if direct_child_name(&parent, &target).as_deref() != Some(name) {
        return Err("新目录必须是所选父目录的直接子目录。".to_owned());
    }
    Ok(target)
}

fn selected_project_directory(
    selected: &str,
    mode: ProjectMode,
    directory_name: &str,
) -> Result<String, String> {
    if mode == ProjectMode::Create {
        new_project_directory(selected, directory_name)
    } else {
        normalize_directory(selected).ok_or_else(|| "请选择有效的绝对项目目录。".to_owned())
    }
}

fn directory_basename(path: &str) -> String {
    let normalized = normalize_directory(path).unwrap_or_default();
    normalized
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or("项目目录")
        .to_owned()
}

fn project_at<'a>(projects: &'a [Project], path: &str) -> Option<&'a Project> {
    projects
        .iter()
        .find(|project| same_directory(&project.path, path))
}

fn is_windows_path(path: &str) -> bool {
    path.as_bytes().get(1) == Some(&b':') || path.starts_with("//")
}

fn project_label(projects: &[Project], id: Option<&str>) -> String {
    id.and_then(|project_id| projects.iter().find(|project| project.id == project_id))
        .map_or_else(|| "独立会话".to_owned(), |project| project.name.clone())
}

pub fn directory_parent(path: &str) -> Option<String> {
    folder_parent(path)
}

pub fn direct_child_name(root: &str, target: &str) -> Option<String> {
    let root = normalize_directory(root)?;
    let target = normalize_directory(target)?;
    let parent = folder_parent(&target)?;
    if !same_directory(&root, &parent) {
        return None;
    }
    directory_basename(&target).into()
}

fn folder_parent(path: &str) -> Option<String> {
    let normalized = normalize_directory(path)?;
    let root = if let Some(unc) = normalized.strip_prefix("//") {
        let mut pieces = unc.split('/');
        format!("//{}/{}", pieces.next()?, pieces.next()?)
    } else if normalized.len() >= 3 && normalized.as_bytes()[1] == b':' {
        normalized[..3].to_owned()
    } else {
        "/".to_owned()
    };
    let trimmed = normalized.trim_end_matches('/');
    if trimmed == root.trim_end_matches('/') {
        return None;
    }
    let parent = trimmed
        .rsplit_once('/')
        .map_or(root.clone(), |(parent, _)| {
            if parent.is_empty() {
                "/".to_owned()
            } else if normalized.len() >= 3 && normalized.as_bytes()[1] == b':' && parent.len() == 2
            {
                root.clone()
            } else {
                parent.to_owned()
            }
        });
    normalize_directory(&parent)
}

fn same_directory(left: &str, right: &str) -> bool {
    let (Some(left), Some(right)) = (normalize_directory(left), normalize_directory(right)) else {
        return false;
    };
    if is_windows_path(&left) && is_windows_path(&right) {
        left.eq_ignore_ascii_case(&right)
    } else {
        left == right
    }
}

fn folder_matches(folder: &work_client::ProjectFolder, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    query.is_empty()
        || folder.name.to_lowercase().contains(&query)
        || folder.path.to_lowercase().contains(&query)
}

// The API browses paths relative to its configured root. The UI shows absolute
// paths for selection; keep that distinction at this boundary, on every OS.
fn folder_relative(root: &str, absolute: &str) -> Result<String, String> {
    if absolute.is_empty() {
        return Ok(String::new());
    }
    let root = normalize_directory(root).ok_or("节点返回了无效的项目根目录。")?;
    let absolute = normalize_directory(absolute).ok_or("请输入有效的绝对目录路径。")?;
    if same_directory(&root, &absolute) {
        return Ok(String::new());
    }
    let prefix = format!("{}/", root.trim_end_matches('/'));
    let inside = absolute.get(..prefix.len()).is_some_and(|head| {
        if is_windows_path(&root) {
            head.eq_ignore_ascii_case(&prefix)
        } else {
            head == prefix
        }
    });
    if !inside {
        return Err(format!(
            "目录不在当前项目根目录内：{root}。请选择根目录中的文件夹。"
        ));
    }
    Ok(absolute[prefix.len()..].to_owned())
}

fn absolute_folder_listing(mut listing: ProjectFolders) -> Result<ProjectFolders, String> {
    let root = normalize_directory(&listing.root).ok_or("节点返回了无效的项目根目录。")?;
    let absolute = |raw: &str| -> Result<String, String> {
        let path = normalize_directory(raw)
            .or_else(|| normalize_directory(&format!("{}/{raw}", root.trim_end_matches('/'))))
            .ok_or("节点返回了无效的目录路径。")?;
        folder_relative(&root, &path)?;
        Ok(path)
    };
    listing.path = absolute(&listing.path)?;
    for folder in &mut listing.folders {
        folder.path = absolute(&folder.path)?;
    }
    listing.root = root;
    Ok(listing)
}

enum FolderError {
    Request(ClientError),
    Path(String),
}
impl From<ClientError> for FolderError {
    fn from(error: ClientError) -> Self {
        Self::Request(error)
    }
}

#[component]
pub fn ProjectPicker(
    shared_client: Signal<Option<WorkspaceClient>>,
    projects: Vec<Project>,
    busy: bool,
    error: Option<String>,
    onsubmit: EventHandler<ProjectIntent>,
    onclose: EventHandler<()>,
    onconnectionrequired: EventHandler<()>,
) -> Element {
    let mut mode = use_signal(|| ProjectMode::Open);
    let mut path = use_signal(String::new);
    let mut name = use_signal(String::new);
    let mut name_auto = use_signal(|| true);
    let mut directory_name = use_signal(|| "new-project".to_owned());
    let mut selected_parent = use_signal(|| None::<String>);
    let mut kind = use_signal(|| ProjectKind::Normal);
    let mut init_git = use_signal(|| false);
    let mut local_error = use_signal(String::new);
    let mut folder_open = use_signal(|| false);
    let mut folder_busy = use_signal(|| false);
    let mut folder_error = use_signal(String::new);
    let mut folder_auth_required = use_signal(|| false);
    let mut folder_query = use_signal(String::new);
    let mut folder_path = use_signal(String::new);
    let mut folder_root = use_signal(|| None::<String>);
    let mut folders = use_signal(|| None::<ProjectFolders>);
    let mut folder_generation = use_signal(|| 0u64);

    let mut load_folders = move |requested_path: String, error_prefix: &'static str| {
        let generation = *folder_generation.read() + 1;
        folder_generation.set(generation);
        folder_path.set(requested_path.clone());
        folder_busy.set(true);
        folder_error.set(String::new());
        folder_auth_required.set(false);
        let known_root = folder_root.peek().clone();
        folders.set(None);
        let Some(client) = shared_client.read().clone() else {
            folder_error.set("请先连接并登录执行节点，再浏览项目目录。".to_owned());
            folder_auth_required.set(true);
            folder_busy.set(false);
            return;
        };
        let mut folder_error = folder_error;
        let mut folders = folders;
        let mut folder_busy = folder_busy;
        let folder_generation = folder_generation;
        spawn(async move {
            let result: Result<ProjectFolders, FolderError> = async {
                let root_listing = if known_root.is_none() {
                    Some(client.project_folders("").await?)
                } else {
                    None
                };
                let root = known_root
                    .as_deref()
                    .or_else(|| root_listing.as_ref().map(|data| data.root.as_str()))
                    .ok_or_else(|| FolderError::Path("无法读取项目根目录。".to_owned()))?;
                let relative = folder_relative(root, &requested_path).map_err(FolderError::Path)?;
                let listing = if relative.is_empty() {
                    if let Some(listing) = root_listing {
                        listing
                    } else {
                        client.project_folders("").await?
                    }
                } else {
                    client.project_folders(&relative).await?
                };
                absolute_folder_listing(listing).map_err(FolderError::Path)
            }
            .await;
            if *folder_generation.read() != generation {
                return;
            }
            match result {
                Ok(result) => {
                    folder_root.set(Some(result.root.clone()));
                    folder_path.set(result.path.clone());
                    folders.set(Some(result));
                    folder_error.set(String::new());
                }
                Err(FolderError::Request(ClientError::Api { status, .. }))
                    if status.as_u16() == 401 =>
                {
                    folder_auth_required.set(true);
                    folder_error.set("节点认证未完成或已失效，请连接并登录节点后重试。".to_owned());
                }
                Err(FolderError::Path(message)) => folder_error.set(message),
                Err(FolderError::Request(error)) => {
                    folder_error.set(format!("{error_prefix}：{error}"));
                }
            }
            folder_busy.set(false);
        });
    };

    let mut invalidate_folder_request = move || {
        folder_generation += 1;
        folder_busy.set(false);
        folder_auth_required.set(false);
    };

    use_effect(move || {
        let _ = shared_client.read().clone();
        folder_root.set(None);
        folder_generation += 1;
        folder_busy.set(false);
        folder_open.set(false);
        folders.set(None);
    });

    let normalized_path = normalize_directory(path.read().as_str());
    let existing_name = normalized_path
        .as_deref()
        .and_then(|candidate| project_at(&projects, candidate))
        .map(|project| project.name.clone());
    let existing = existing_name.is_some();
    let title = if *mode.read() == ProjectMode::Open {
        "添加项目"
    } else {
        "新建项目目录"
    };
    let folder_title = if *mode.read() == ProjectMode::Create {
        "选择新项目的父目录"
    } else {
        "选择项目文件夹"
    };
    let submit_label = if *mode.read() == ProjectMode::Open {
        "打开项目"
    } else {
        "创建项目"
    };
    let mode_for_submit = *mode.read();
    let path_for_submit = normalized_path.clone();
    let name_for_submit = name.read().trim().to_owned();
    let kind_for_submit = *kind.read();
    let init_git_for_submit = *init_git.read();

    let open_folder = move |_| {
        let selection_mode = *mode.read();
        let child_name = directory_name.read().clone();
        if selection_mode == ProjectMode::Create {
            if let Err(error) = new_project_directory("/", &child_name) {
                local_error.set(error);
                return;
            }
        }
        let initial = if selection_mode == ProjectMode::Create {
            selected_parent
                .peek()
                .clone()
                .or_else(|| folder_parent(path.read().as_str()))
        } else {
            normalize_directory(path.read().as_str())
        }
        .unwrap_or_default();
        #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
        if shared_client
            .read()
            .as_ref()
            .is_some_and(WorkspaceClient::is_local)
        {
            let generation = *folder_generation.peek() + 1;
            folder_generation.set(generation);
            folder_busy.set(true);
            spawn(async move {
                let mut dialog = rfd::AsyncFileDialog::new().set_title(
                    if selection_mode == ProjectMode::Create {
                        "选择新项目的父目录"
                    } else {
                        "选择项目目录"
                    },
                );
                if !initial.is_empty() {
                    dialog = dialog.set_directory(&initial);
                }
                let selected = dialog.pick_folder().await;
                if *folder_generation.read() != generation {
                    return;
                }
                folder_busy.set(false);
                let Some(selected) = selected else {
                    return;
                };
                let Some(selected) = selected.path().to_str() else {
                    local_error.set("所选目录不是有效的 UTF-8 路径，请手动输入。".to_owned());
                    return;
                };
                match selected_project_directory(selected, selection_mode, &child_name) {
                    Ok(target) => {
                        if selection_mode == ProjectMode::Create {
                            selected_parent.set(normalize_directory(selected));
                        }
                        if *name_auto.read() {
                            name.set(directory_basename(&target));
                        }
                        path.set(target);
                        local_error.set(String::new());
                    }
                    Err(error) => local_error.set(error),
                }
            });
            return;
        }
        folder_query.set(String::new());
        folder_open.set(true);
        load_folders(initial, "无法读取节点目录");
    };

    let submit = move |_| {
        local_error.set(String::new());
        let Some(path) = path_for_submit.clone() else {
            local_error.set("请输入有效的绝对项目目录（Windows、UNC 或 Unix 路径）。".to_owned());
            return;
        };
        if name_for_submit.is_empty() {
            local_error.set("请输入显示名称。".to_owned());
            return;
        }
        if existing && mode_for_submit == ProjectMode::Create {
            local_error.set("这个目录已经登记，请切换到“打开已有目录”。".to_owned());
            return;
        }
        invalidate_folder_request();
        onsubmit.call(ProjectIntent {
            mode: mode_for_submit,
            path,
            name: name_for_submit.clone(),
            kind: kind_for_submit,
            init_git: mode_for_submit == ProjectMode::Create && init_git_for_submit,
        });
    };

    rsx! {
        Modal {
            id: "project-picker",
            title,
            class: "project-picker-modal",
            onclose: move |()| {
                invalidate_folder_request();
                onclose.call(());
            },
            div { class: "modal-body project-picker-body",
                p { class: "project-picker-intro", "打开已有项目，或新建一个项目目录。" }
                div { class: "project-picker-modes", role: "group", aria_label: "项目添加方式",
                    button {
                        class: if *mode.read() == ProjectMode::Open { "project-picker-mode active" } else { "project-picker-mode" },
                        aria_pressed: (*mode.read() == ProjectMode::Open).to_string(),
                        onclick: move |_| { invalidate_folder_request(); folder_open.set(false); selected_parent.set(None); mode.set(ProjectMode::Open); init_git.set(false); local_error.set(String::new()); },
                        Icon { name: "folder".to_owned() } span { "打开已有目录" }
                    }
                    button {
                        class: if *mode.read() == ProjectMode::Create { "project-picker-mode active" } else { "project-picker-mode" },
                        aria_pressed: (*mode.read() == ProjectMode::Create).to_string(),
                        onclick: move |_| { invalidate_folder_request(); folder_open.set(false); selected_parent.set(None); mode.set(ProjectMode::Create); local_error.set(String::new()); },
                        Icon { name: "plus".to_owned() } span { "新建目录" }
                    }
                }
                div { class: "project-picker-node", Icon { name: "monitor".to_owned() } span { "这台电脑" } small { "执行节点" } }
                if *mode.read() == ProjectMode::Create {
                    div { class: "form-field",
                        label { r#for: "project-picker-directory-name", "新目录名称" }
                        input {
                            id: "project-picker-directory-name",
                            value: "{directory_name}",
                            maxlength: "120",
                            oninput: move |event| {
                                invalidate_folder_request();
                                let value = event.value();
                                directory_name.set(value.clone());
                                if let Some(parent) = selected_parent.peek().clone() {
                                    match new_project_directory(&parent, &value) {
                                        Ok(target) => {
                                            if *name_auto.read() { name.set(directory_basename(&target)); }
                                            path.set(target);
                                            local_error.set(String::new());
                                        }
                                        Err(error) => {
                                            path.set(String::new());
                                            local_error.set(error);
                                        }
                                    }
                                }
                            }
                        }
                        small { class: "field-help", "浏览时选择父目录，将在其中创建上述新目录；也可直接输入下方完整项目路径。" }
                    }
                }
                div { class: "form-field project-path-field",
                    label { r#for: "project-picker-path", if *mode.read() == ProjectMode::Open { "已有项目目录" } else { "新项目目录" } }
                    div { class: "project-path-control",
                        input {
                            id: "project-picker-path",
                            value: "{path}",
                            placeholder: "输入完整绝对路径",
                            maxlength: "4096",
                            autocomplete: "off",
                            oninput: move |event| {
                                invalidate_folder_request();
                                selected_parent.set(None);
                                let value = event.value();
                                path.set(value.clone());
                                if *name_auto.read() {
                                    name.set(normalize_directory(&value).map_or_else(String::new, |value| directory_basename(&value)));
                                }
                                local_error.set(String::new());
                            }
                        }
                        button { class: "project-browse-button", disabled: *folder_busy.read(), onclick: open_folder, Icon { name: "folder".to_owned() } "浏览…" }
                    }
                }
                div { class: "form-field",
                    label { r#for: "project-picker-name", "显示名称" span { class: "field-note", "可自定义" } }
                    input {
                        id: "project-picker-name",
                        value: "{name}",
                        placeholder: "默认使用目录名",
                        maxlength: "120",
                        autocomplete: "off",
                        oninput: move |event| {
                            let value = event.value();
                            name_auto.set(value.trim().is_empty());
                            name.set(value);
                        }
                    }
                    small { class: "field-help", "只影响项目列表中的名称，项目目录保持不变。" }
                    if !*name_auto.read() {
                        button { class: "text-button", onclick: move |_| {
                            if let Some(path) = normalize_directory(path.read().as_str()) {
                                name.set(directory_basename(&path));
                                name_auto.set(true);
                            }
                        }, "恢复自动名称" }
                    }
                }
                fieldset { class: "project-purpose",
                    legend { "项目用途" }
                    div { class: "project-kind-grid", role: "radiogroup", aria_label: "项目用途",
                        label { class: if *kind.read() == ProjectKind::Normal { "project-kind selected" } else { "project-kind" },
                            input { r#type: "radio", name: "project-kind", checked: *kind.read() == ProjectKind::Normal, disabled: existing, onchange: move |_| kind.set(ProjectKind::Normal) }
                            Icon { name: "folder".to_owned() } span { strong { "普通项目" } small { "日常开发和项目任务" } }
                        }
                        label { class: if *kind.read() == ProjectKind::MetaAgent { "project-kind selected" } else { "project-kind" },
                            input { r#type: "radio", name: "project-kind", checked: *kind.read() == ProjectKind::MetaAgent, disabled: existing, onchange: move |_| kind.set(ProjectKind::MetaAgent) }
                            Icon { name: "sparkles".to_owned() } span { strong { "元Agent" } small { "编写 Skill、Package 与扩展" } }
                        }
                    }
                }
                if let Some(existing_name) = existing_name.as_deref() {
                    p { class: "project-existing-note", "已登记项目将复用原有名称、用途和会话（{existing_name}）。" }
                }
                if *mode.read() == ProjectMode::Create {
                    label { class: "project-git-option",
                        input { r#type: "checkbox", checked: *init_git.read(), onchange: move |event| init_git.set(event.checked()) }
                        span { "初始化 Git 仓库" }
                    }
                }
                if !local_error.read().is_empty() { p { class: "project-form-error", role: "alert", "{local_error}" } }
                if let Some(error) = error.as_deref() { p { class: "project-form-error", role: "alert", "{error}" } }
            }
            div { class: "modal-actions project-picker-footer",
                button { class: "secondary", disabled: busy, onclick: move |_| { invalidate_folder_request(); onclose.call(()); }, "取消" }
                button { class: "button primary", disabled: busy, onclick: submit, if busy { "处理中…" } else { "{submit_label}" } }
            }
        }
        if *folder_open.read() {
            Modal {
                id: "project-folder-picker",
                title: folder_title,
                class: "project-folder-modal",
                onclose: move |()| { invalidate_folder_request(); folder_open.set(false); },
                div {
                    class: "modal-body project-folder-body",
                    onkeydown: move |event| {
                        if event.key() == Key::Escape {
                            event.stop_propagation();
                            invalidate_folder_request();
                            folder_open.set(false);
                        }
                    },
                    p { class: "project-folder-caption", "这台电脑 · 目录来自执行节点，可授权范围由节点决定。" }
                    div { class: "project-folder-search",
                        Icon { name: "search".to_owned() }
                        input { aria_label: "搜索目录名称或路径", placeholder: "搜索目录名称或路径…", value: "{folder_query}", autofocus: true, oninput: move |event| folder_query.set(event.value()) }
                    }
                    if !folder_error.read().is_empty() { p { class: "project-form-error", role: "alert", "{folder_error}" } }
                    if *folder_auth_required.read() {
                        button { class: "button primary", onclick: move |_| onconnectionrequired.call(()), "连接或登录节点" }
                    }
                    if *folder_busy.read() {
                        p { class: "project-folder-state", "正在读取节点目录…" }
                    } else if let Some(data) = folders.read().as_ref() {
                        div { class: "project-folder-toolbar",
                            button { class: "folder-up", disabled: same_directory(&data.path, &data.root), onclick: move |_| {
                                let parent = folders.peek().as_ref().and_then(|data| folder_parent(&data.path));
                                if let Some(parent) = parent {
                                    load_folders(parent, "无法读取上级目录");
                                }
                            }, "上级目录" }
                            code { "{data.path}" }
                        }
                        div { class: "project-folder-list", aria_label: "节点目录",
                            for folder in data.folders.iter().filter(|folder| folder_matches(folder, folder_query.read().as_str())) {
                                {
                                    let folder_path_value = folder.path.clone();
                                    let selected = same_directory(&folder_path_value, folder_path.read().as_str());
                                    let registered = projects.iter().any(|project| same_directory(&project.path, &folder_path_value));
                                    let entered_path = folder_path_value.clone();
                                    rsx! {
                                        div { class: if selected { "project-folder-row selected" } else { "project-folder-row" },
                                            button { class: "project-folder-select", aria_pressed: selected.to_string(), onclick: move |_| folder_path.set(folder_path_value.clone()),
                                                Icon { name: "folder".to_owned() }
                                                span { strong { "{folder.name}" } small { "{folder.path}" }
                                                    if registered { em { "已登记 · 打开将复用" } }
                                                }
                                            }
                                            button { class: "project-folder-enter", aria_label: "进入此目录", title: "进入此目录", disabled: *folder_busy.read(), onclick: move |_| load_folders(entered_path.clone(), "无法读取此目录"),
                                                Icon { name: "chevron".to_owned() }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if data.folders.is_empty() { p { class: "project-folder-state", "这个目录没有可浏览的子目录。仍可选择当前目录。" } }
                        div { class: "project-folder-selection", small { "选中的目录" } code { "{folder_path}" } }
                    } else {
                        p { class: "project-folder-state", "没有目录数据。请返回后输入路径，或重试浏览。" }
                    }
                    div { class: "modal-actions",
                        button { class: "secondary", onclick: move |_| { invalidate_folder_request(); folder_open.set(false); }, "取消" }
                        button { class: "button primary", disabled: *folder_busy.read() || !folder_error.read().is_empty(), onclick: move |_| {
                            let selected = folder_path.read().clone();
                            let target = match selected_project_directory(&selected, *mode.read(), directory_name.read().as_str()) {
                                Ok(target) => target,
                                Err(error) => { folder_error.set(error); return; }
                            };
                            if *mode.read() == ProjectMode::Create {
                                selected_parent.set(normalize_directory(&selected));
                            }
                            if *name_auto.read() { name.set(directory_basename(&target)); }
                            path.set(target);
                            local_error.set(String::new());
                            invalidate_folder_request();
                            folder_open.set(false);
                        }, "使用此目录" }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod directory_tests {
    use super::{folder_parent, normalize_directory, project_at, same_directory};
    use work_client::Project;

    #[test]
    fn create_selection_appends_one_component_on_every_supported_path_style() {
        for (parent, expected) in [
            ("/", "/new-project"),
            ("/work/parent", "/work/parent/new-project"),
            ("c:\\work", "C:/work/new-project"),
            ("C:/", "C:/new-project"),
            ("\\\\server\\share", "//server/share/new-project"),
        ] {
            assert_eq!(
                super::new_project_directory(parent, "new-project").unwrap(),
                expected
            );
            assert_eq!(
                super::selected_project_directory(
                    parent,
                    super::ProjectMode::Create,
                    "new-project"
                )
                .unwrap(),
                expected
            );
        }
        assert_eq!(
            super::selected_project_directory("c:\\work\\app", super::ProjectMode::Open, "ignored")
                .unwrap(),
            "C:/work/app"
        );
    }

    #[test]
    fn create_selection_rejects_paths_traversal_and_nonportable_names() {
        for invalid in [
            "",
            " ",
            ".",
            "..",
            "../escape",
            "a/b",
            "a\\b",
            "/absolute",
            "C:/absolute",
            "a\nb",
            "name.",
            "a:b",
            "NUL",
            "con.txt",
            "COM1",
            "LPT9.log",
        ] {
            assert!(
                super::new_project_directory("/parent", invalid).is_err(),
                "{invalid:?}"
            );
        }
        assert!(super::new_project_directory("relative", "valid").is_err());
        assert_eq!(
            super::new_project_directory("/parent", " 项目 ").unwrap(),
            "/parent/项目"
        );
    }

    #[test]
    fn windows_drive_and_unc_roots_have_no_parent() {
        assert_eq!(folder_parent("c:/"), None);
        assert_eq!(folder_parent("//server/share"), None);
        assert_eq!(folder_parent("C:/work"), Some("C:/".to_owned()));
        assert_eq!(
            folder_parent("//server/share/work"),
            Some("//server/share/".to_owned())
        );
    }

    #[test]
    fn normalization_collapses_segments_and_rejects_crossing_root() {
        assert_eq!(
            normalize_directory("C:\\work\\a\\..\\b\\"),
            Some("C:/work/b".to_owned())
        );
        assert_eq!(normalize_directory("//server/share/../../outside"), None);
        assert_eq!(normalize_directory("/../../outside"), None);
    }

    #[test]
    fn directory_identity_respects_platform_case_rules_and_ignores_aliases() {
        assert!(!same_directory("/Work/App", "/work/app"));
        assert!(same_directory("c:/Work/App", "C:/work/app"));
        let projects = [Project {
            id: "p1".to_owned(),
            name: "Custom alias".to_owned(),
            path: "/Work/App".to_owned(),
        }];
        assert_eq!(
            project_at(&projects, "/Work/App").map(|project| project.id.as_str()),
            Some("p1")
        );
        assert_eq!(project_at(&projects, "/work/app"), None);
        assert_eq!(project_at(&projects, "Custom alias"), None);
    }

    #[test]
    fn new_project_must_be_a_direct_child_of_the_node_root() {
        assert_eq!(
            super::direct_child_name("D:/Projects", "d:/projects/review-meta"),
            Some("review-meta".to_owned())
        );
        assert_eq!(
            super::direct_child_name("/Projects", "/Projects/nested/review-meta"),
            None
        );
        assert_eq!(super::direct_child_name("/Projects", "/Projects"), None);
    }

    #[test]
    fn directory_queries_are_relative_and_do_not_escape_the_configured_root() {
        assert_eq!(
            super::folder_relative("D:/Projects", "d:\\projects\\rpi-work\\src").unwrap(),
            "rpi-work/src"
        );
        assert_eq!(
            super::folder_relative("/workspace", "/workspace/app").unwrap(),
            "app"
        );
        assert_eq!(
            super::folder_relative("//server/share", "//SERVER/share/app").unwrap(),
            "app"
        );
        assert_eq!(
            super::folder_relative("D:/Projects", "D:/Projects").unwrap(),
            ""
        );
        for invalid in [
            "D:/Projects-old/app",
            "C:/Projects/app",
            "D:/Projects/../private",
        ] {
            assert!(super::folder_relative("D:/Projects", invalid).is_err());
        }
        assert!(super::folder_relative("/Workspace", "/workspace/app").is_err());
    }

    #[test]
    fn relative_node_results_become_absolute_selections() {
        let listing = work_client::ProjectFolders {
            root: "D:\\Projects".into(),
            path: "rpi-work".into(),
            folders: vec![work_client::ProjectFolder {
                name: "src".into(),
                path: "rpi-work/src".into(),
            }],
        };
        let listing = super::absolute_folder_listing(listing).unwrap();
        assert_eq!(listing.path, "D:/Projects/rpi-work");
        assert_eq!(listing.folders[0].path, "D:/Projects/rpi-work/src");
        assert_eq!(
            super::folder_relative(&listing.root, &listing.folders[0].path).unwrap(),
            "rpi-work/src"
        );
    }
}

#[component]
pub fn ProjectNameDialog(
    project: Project,
    busy: bool,
    error: Option<String>,
    onsave: EventHandler<String>,
    onclose: EventHandler<()>,
) -> Element {
    let mut name = use_signal(|| project.name.clone());
    rsx! {
        Modal { id: "project-name", title: "项目名称与目录", class: "project-name-modal", onclose,
            div { class: "modal-body",
                div { class: "form-field", label { r#for: "project-display-name", "显示名称" } input { id: "project-display-name", value: "{name}", maxlength: "80", autofocus: true, disabled: busy,
                    onkeydown: move |event| { if event.key() == Key::Enter { event.prevent_default(); if !busy && !name.read().trim().is_empty() { onsave.call(name.read().trim().to_owned()); } } },
                    oninput: move |event| name.set(event.value()) } }
                div { class: "form-field", label { r#for: "project-display-path", "项目目录" } input { id: "project-display-path", readonly: true, value: "{project.path}" } }
                p { class: "field-help", "保存只修改显示名称，不重命名文件夹，也不改变项目用途或会话绑定。" }
                if let Some(error) = error.as_deref() { p { class: "project-form-error", role: "alert", "{error}" } }
                div { class: "modal-actions", button { class: "secondary", disabled: busy, onclick: move |_| onclose.call(()), "取消" } button { class: "button primary", disabled: busy || name.read().trim().is_empty(), onclick: move |_| onsave.call(name.read().trim().to_owned()), if busy { "保存中…" } else { "保存名称" } } }
            }
        }
    }
}

#[component]
pub fn SearchDialog(
    projects: Vec<Project>,
    sessions: Vec<SessionSummary>,
    onproject: EventHandler<String>,
    onsession: EventHandler<String>,
    onclose: EventHandler<()>,
) -> Element {
    let mut query = use_signal(String::new);
    let needle = query.read().trim().to_lowercase();
    let matching_projects = projects.iter().filter(|project| {
        format!("{} {}", project.name, project.path)
            .to_lowercase()
            .contains(&needle)
    });
    let matching_sessions = sessions.iter().filter(|session| {
        let title = session.title.as_deref().unwrap_or("未命名会话");
        let project = projects
            .iter()
            .find(|project| session.project_id.as_deref() == Some(project.id.as_str()));
        format!(
            "{} {} {}",
            title,
            project.map_or("", |project| &project.name),
            project.map_or("", |project| &project.path)
        )
        .to_lowercase()
        .contains(&needle)
    });
    rsx! {
        Modal { id: "global-search", title: "找到你的工作", class: "search-dialog-modal", onclose,
            div { class: "modal-body",
                div { class: "global-search-input", Icon { name: "search".to_owned() } input { aria_label: "搜索项目、目录或会话", placeholder: "搜索项目、目录或会话…", value: "{query}", autofocus: true, oninput: move |event| query.set(event.value()) } }
                div { class: "search-result-groups",
                    section { class: "search-result-group", h3 { "项目" }
                        for project in matching_projects {
                            {
                                let project_id = project.id.clone();
                                let project_name = project.name.clone();
                                let project_path = project.path.clone();
                                rsx! {
                                    button { class: "search-result-row", onclick: move |_| onproject.call(project_id.clone()), Icon { name: "folder".to_owned() } span { strong { "{project_name}" } small { "{project_path}" } } }
                                }
                            }
                        }
                        if projects.iter().all(|project| !format!("{} {}", project.name, project.path).to_lowercase().contains(&needle)) { p { class: "search-empty", "没有匹配的项目" } }
                    }
                    section { class: "search-result-group", h3 { "会话" }
                        for session in matching_sessions {
                            {
                                let session_id = session.session_id.clone();
                                let title = session.title.as_deref().unwrap_or("未命名会话").to_owned();
                                rsx! { button { class: "search-result-row", onclick: move |_| onsession.call(session_id.clone()), Icon { name: "message".to_owned() } span { strong { "{title}" } small { "{project_label(&projects, session.project_id.as_deref())}" } } } }
                            }
                        }
                        if sessions.iter().all(|session| { let title = session.title.as_deref().unwrap_or("未命名会话"); !format!("{} {}", title, project_label(&projects, session.project_id.as_deref())).to_lowercase().contains(&needle) }) { p { class: "search-empty", "没有匹配的会话" } }
                    }
                }
            }
        }
    }
}

#[component]
pub fn SessionSwitcher(
    projects: Vec<Project>,
    sessions: Vec<SessionSummary>,
    current_session: Option<String>,
    onsession: EventHandler<String>,
    onclose: EventHandler<()>,
) -> Element {
    let mut query = use_signal(String::new);
    let needle = query.read().trim().to_lowercase();
    rsx! {
        Modal { id: "session-switcher", title: "切换会话", class: "session-switcher-modal", onclose,
            div { class: "modal-body",
                div { class: "session-switcher-toolbar", span { "所有项目" } }
                div { class: "global-search-input", Icon { name: "search".to_owned() } input { aria_label: "搜索会话或项目", placeholder: "搜索会话或项目…", value: "{query}", autofocus: true, oninput: move |event| query.set(event.value()) } }
                div { class: "session-switcher-results",
                    for session in sessions.iter().filter(|session| {
                        let title = session.title.as_deref().unwrap_or("未命名会话");
                        let project = projects.iter().find(|project| session.project_id.as_deref() == Some(project.id.as_str()));
                        format!("{} {} {}", title, project.map_or("", |project| &project.name), project.map_or("", |project| &project.path)).to_lowercase().contains(&needle)
                    }) {
                        {
                            let session_id = session.session_id.clone();
                            let current = current_session.as_deref() == Some(session.session_id.as_str());
                            let title = session.title.as_deref().unwrap_or("未命名会话").to_owned();
                            rsx! {
                                button { class: if current { "session-switcher-row current" } else { "session-switcher-row" }, aria_current: if current { "true" } else { "false" }, onclick: move |_| onsession.call(session_id.clone()),
                                    Icon { name: "message".to_owned() }
                                    span { strong { "{title}" } small { "{project_label(&projects, session.project_id.as_deref())}" } }
                                    if current { em { "当前" } } else { Icon { name: "chevron".to_owned() } }
                                }
                            }
                        }
                    }
                    if sessions.iter().all(|session| {
                        let title = session.title.as_deref().unwrap_or("未命名会话");
                        let project = projects.iter().find(|project| session.project_id.as_deref() == Some(project.id.as_str()));
                        !format!("{} {} {}", title, project.map_or("", |project| &project.name), project.map_or("", |project| &project.path)).to_lowercase().contains(&needle)
                    }) { p { class: "search-empty", "没有匹配的会话，试试其他名称或项目。" } }
                }
            }
        }
    }
}
