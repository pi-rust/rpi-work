use crate::components::{Icon, Modal};
use dioxus::{core::Task, prelude::*};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use work_client::{RuntimeState, WorkClient};

pub const CSS: &str = include_str!("resources.css");
const MAX_PACKAGE_BYTES: usize = 200 * 1024;
#[cfg(any(target_arch = "wasm32", target_os = "android"))]
const STORAGE_KEY: &str = "rpi-work-resources-v1";

#[derive(Clone, Debug, PartialEq)]
pub struct SkillResource {
    pub id: String,
    pub name: String,
    pub description: String,
    pub source: String,
    pub path: String,
    pub content: String,
    pub acquired: bool,
    pub node_installed: bool,
    pub design_sample: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExtensionResource {
    pub id: String,
    pub name: String,
    pub description: String,
    pub source: String,
    pub content: String,
    pub acquired: bool,
    pub node_installed: bool,
    pub design_sample: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PackageSkill {
    pub id: String,
    pub name: String,
    pub description: String,
    pub content: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DesignPackage {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    pub source: String,
    pub resource_kind: String,
    pub skill_contents: Vec<PackageSkill>,
    pub skills: Vec<String>,
    pub tools: Vec<String>,
    pub prompts: Vec<PromptResource>,
    // The entire inert published document, separate from local acquisition state.
    pub manifest: Option<Value>,
    pub acquired: bool,
    pub node_installed: bool,
    pub design_sample: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PromptResource {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub content: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackagePin {
    pub id: String,
    pub version: String,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct SessionBinding {
    pub session_id: String,
    pub package_pins: Vec<PackagePin>,
    pub skills: Vec<String>,
    pub disabled_skills: Vec<String>,
    pub disabled_tools: Vec<String>,
    pub disabled_prompts: Vec<String>,
    pub instructions: String,
    pub revision: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ResourceLibrary {
    pub packages: Vec<DesignPackage>,
    pub skills: Vec<SkillResource>,
    pub extensions: Vec<ExtensionResource>,
    pub sessions: Vec<SessionBinding>,
    pub storage_error: Option<String>,
}

impl Default for ResourceLibrary {
    fn default() -> Self {
        sample_library()
    }
}

impl ResourceLibrary {
    fn from_value(value: &Value) -> Result<Self, String> {
        let object = value.as_object().ok_or("资源库数据不是 JSON 对象")?;
        let mut library = sample_library();
        if let Some(packages) = object.get("packages") {
            library.packages = packages
                .as_array()
                .ok_or("packages 必须是数组")?
                .iter()
                .map(package_from_value)
                .collect::<Result<_, _>>()?;
        }
        if let Some(skills) = object.get("skills") {
            library.skills = skills
                .as_array()
                .ok_or("skills 必须是数组")?
                .iter()
                .map(skill_from_value)
                .collect::<Result<_, _>>()?;
        }
        if let Some(extensions) = object.get("extensions") {
            library.extensions = extensions
                .as_array()
                .ok_or("extensions 必须是数组")?
                .iter()
                .map(extension_from_value)
                .collect::<Result<_, _>>()?;
        }
        if let Some(sessions) = object.get("sessions") {
            library.sessions = sessions
                .as_array()
                .ok_or("sessions 必须是数组")?
                .iter()
                .map(session_from_value)
                .collect::<Result<_, _>>()?;
        }
        let mut keys = BTreeSet::new();
        for package in &library.packages {
            if !keys.insert(package_key(package)) {
                return Err("资源库存在重复的包版本".into());
            }
        }
        Ok(library)
    }

    fn to_value(&self) -> Value {
        json!({
            "version": 2,
            "packages": self.packages.iter().map(package_value).collect::<Vec<_>>(),
            "skills": self.skills.iter().map(skill_value).collect::<Vec<_>>(),
            "extensions": self.extensions.iter().map(extension_value).collect::<Vec<_>>(),
            "sessions": self.sessions.iter().map(session_value).collect::<Vec<_>>()
        })
    }

    fn session_mut(&mut self, session_id: &str) -> &mut SessionBinding {
        if let Some(index) = self
            .sessions
            .iter()
            .position(|item| item.session_id == session_id)
        {
            return &mut self.sessions[index];
        }
        self.sessions.push(SessionBinding {
            session_id: session_id.to_owned(),
            revision: 1,
            ..SessionBinding::default()
        });
        self.sessions.last_mut().expect("session was just inserted")
    }
}

#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
fn resource_path() -> Option<std::path::PathBuf> {
    #[cfg(target_os = "windows")]
    let root = std::env::var_os("LOCALAPPDATA").map(std::path::PathBuf::from);
    #[cfg(target_os = "macos")]
    let root = std::env::var_os("HOME")
        .map(|path| std::path::PathBuf::from(path).join("Library/Application Support"));
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    let root = std::env::var_os("XDG_DATA_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|path| std::path::PathBuf::from(path).join(".local/share"))
        });
    root.map(|path| path.join("rpi-work").join("resources.json"))
}

#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
fn load_library() -> ResourceLibrary {
    let result = (|| {
        let path = resource_path().ok_or("无法确定本地存储路径".to_owned())?;
        match std::fs::read_to_string(path) {
            Ok(text) => {
                let value =
                    serde_json::from_str::<Value>(&text).map_err(|error| error.to_string())?;
                ResourceLibrary::from_value(&value)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(ResourceLibrary::default())
            }
            Err(error) => Err(error.to_string()),
        }
    })();
    result.unwrap_or_else(|error| ResourceLibrary {
        storage_error: Some(format!("资源库读取失败：{error}")),
        ..ResourceLibrary::default()
    })
}

fn set_storage_error(mut library: Signal<ResourceLibrary>, error: Option<String>) {
    if library.peek().storage_error != error {
        library.write().storage_error = error;
    }
}

pub fn use_resource_library() -> Signal<ResourceLibrary> {
    let library = use_signal(|| {
        #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
        {
            load_library()
        }
        #[cfg(any(target_arch = "wasm32", target_os = "android"))]
        {
            ResourceLibrary::default()
        }
    });
    // Error reporting must not trigger another save of identical content.
    let serialized = use_memo(move || library.read().to_value().to_string());
    #[cfg(any(target_arch = "wasm32", target_os = "android"))]
    let mut loaded = use_signal(|| false);
    #[cfg(any(target_arch = "wasm32", target_os = "android"))]
    let mut save_revision = use_signal(|| 0_u64);
    #[cfg(any(target_arch = "wasm32", target_os = "android"))]
    use_effect(move || {
        let mut library = library;
        spawn(async move {
            let key = serde_json::to_string(STORAGE_KEY).expect("static storage key");
            let result = document::eval(&format!(
                "try {{ const text=localStorage.getItem({key}); dioxus.send({{ok:true,value:text===null?null:JSON.parse(text)}}); }} catch (error) {{ dioxus.send({{ok:false,error:String(error)}}); }}"
            )).recv::<Value>().await;
            let result = match result {
                Ok(value) if value["ok"] == true => {
                    if value["value"].is_null() {
                        Ok(None)
                    } else {
                        ResourceLibrary::from_value(&value["value"]).map(Some)
                    }
                }
                Ok(value) => Err(value["error"]
                    .as_str()
                    .unwrap_or("localStorage 不可读")
                    .to_owned()),
                Err(error) => Err(error.to_string()),
            };
            match result {
                Ok(value) => {
                    if let Some(value) = value {
                        library.set(value);
                    }
                    loaded.set(true);
                }
                Err(error) => set_storage_error(
                    library,
                    Some(format!("资源库读取失败：{error}；未覆盖原存储")),
                ),
            }
        });
    });
    use_effect(move || {
        let text = serialized.read().clone();
        #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
        {
            // Preserve unreadable data for recovery instead of overwriting it with samples.
            if library
                .peek()
                .storage_error
                .as_deref()
                .is_some_and(|error| error.starts_with("资源库读取失败"))
            {
                return;
            }
            let result = (|| {
                let path = resource_path().ok_or("无法确定本地存储路径".to_owned())?;
                let parent = path.parent().ok_or("本地存储路径无父目录".to_owned())?;
                std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
                std::fs::write(path, &text).map_err(|error| error.to_string())
            })();
            set_storage_error(
                library,
                result.err().map(|error| format!("资源库保存失败：{error}")),
            );
        }
        #[cfg(any(target_arch = "wasm32", target_os = "android"))]
        if *loaded.read() {
            let revision = *save_revision.peek() + 1;
            save_revision.set(revision);
            let key = serde_json::to_string(STORAGE_KEY).expect("static storage key");
            let value = serde_json::to_string(&text).expect("serialized resource library");
            spawn(async move {
                let result = document::eval(&format!(
                    "try {{ localStorage.setItem({key},{value}); dioxus.send({{ok:true}}); }} catch (error) {{ dioxus.send({{ok:false,error:String(error)}}); }}"
                )).recv::<Value>().await;
                if *save_revision.peek() != revision {
                    return;
                }
                let error = match result {
                    Ok(value) if value["ok"] == true => None,
                    Ok(value) => Some(format!(
                        "资源库保存失败：{}",
                        value["error"].as_str().unwrap_or("localStorage 拒绝写入")
                    )),
                    Err(error) => Some(format!("资源库保存失败：{error}")),
                };
                set_storage_error(library, error);
            });
        }
    });
    library
}

pub fn import_design_package(
    library: &mut ResourceLibrary,
    value: &Value,
) -> Result<String, String> {
    if value.to_string().len() > MAX_PACKAGE_BYTES {
        return Err("设计包不能超过 200 KiB".into());
    }
    let package = package_from_manifest(value)?;
    let key = package_key(&package);
    if let Some(existing) = library
        .packages
        .iter()
        .find(|item| package_key(item) == key)
    {
        if package_manifest_value(existing) != *value {
            return Err("同一 ID 和版本的内容不一致，请使用新版本号".into());
        }
        return Ok(key);
    }
    // Build and validate every derived item before mutating the library.
    let skills = package_skills(&package);
    for skill in &skills {
        if library.skills.iter().any(|item| item.id == skill.id) {
            return Err(format!("技能 {} 已存在，拒绝覆盖", skill.id));
        }
    }
    let extension = if package.resource_kind.eq_ignore_ascii_case("extension") {
        if library.extensions.iter().any(|item| item.id == key) {
            return Err(format!("扩展 {key} 已存在，拒绝覆盖"));
        }
        Some(ExtensionResource {
            id: key.clone(),
            name: package.name.clone(),
            description: package.description.clone(),
            source: key.clone(),
            content: source_text(&value["design"]["sources"]),
            acquired: true,
            node_installed: false,
            design_sample: false,
        })
    } else {
        None
    };
    library.skills.extend(skills);
    if let Some(extension) = extension {
        library.extensions.push(extension);
    }
    // All published kinds retain their immutable document here; pages filter by kind.
    library.packages.push(package);
    Ok(key)
}

fn package_skills(package: &DesignPackage) -> Vec<SkillResource> {
    let owner = package_key(package);
    package
        .skill_contents
        .iter()
        .map(|skill| SkillResource {
            id: format!("{owner}/{}", skill.id),
            name: skill.name.clone(),
            description: skill.description.clone(),
            source: owner.clone(),
            path: format!("{owner}/skills/{}/SKILL.md", skill.id),
            content: skill.content.clone(),
            acquired: package.acquired,
            node_installed: false,
            design_sample: package.design_sample,
        })
        .collect()
}

fn package_selected(binding: &SessionBinding, package: &DesignPackage) -> bool {
    binding
        .package_pins
        .iter()
        .any(|pin| pin.id == package.id && pin.version == package.version)
}

fn set_package_selected(binding: &mut SessionBinding, id: &str, version: &str, selected: bool) {
    binding.package_pins.retain(|pin| pin.id != id);
    if selected {
        binding.package_pins.push(PackagePin {
            id: id.into(),
            version: version.into(),
        });
    }
    binding.revision += 1;
}

fn session_skills(library: &ResourceLibrary, binding: &SessionBinding) -> Vec<SkillResource> {
    let mut seen = BTreeSet::new();
    library
        .packages
        .iter()
        .filter(|package| package.acquired && package_selected(binding, package))
        .flat_map(package_skills)
        .chain(
            library
                .skills
                .iter()
                .filter(|skill| skill.acquired)
                .cloned(),
        )
        .filter(|skill| seen.insert(skill.id.clone()))
        .collect()
}

fn package_skill_selected(library: &ResourceLibrary, binding: &SessionBinding, key: &str) -> bool {
    library.packages.iter().any(|package| {
        package.acquired
            && package_selected(binding, package)
            && package
                .skill_contents
                .iter()
                .any(|skill| format!("{}/{}", package_key(package), skill.id) == key)
    })
}

pub fn effective_skills(library: &ResourceLibrary, binding: &SessionBinding) -> Vec<SkillResource> {
    session_skills(library, binding)
        .into_iter()
        .filter(|skill| {
            if package_skill_selected(library, binding, &skill.id) {
                !binding.disabled_skills.contains(&skill.id)
            } else {
                binding.skills.contains(&skill.id)
            }
        })
        .collect()
}

fn source_text(sources: &Value) -> String {
    if let Some(sources) = sources.as_object() {
        sources
            .iter()
            .map(|(path, content)| {
                format!(
                    "// {path}\n{}",
                    content
                        .as_str()
                        .map_or_else(|| content.to_string(), ToOwned::to_owned)
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    } else {
        serde_json::to_string_pretty(sources).unwrap_or_default()
    }
}

#[component]
pub fn ResourcePage(
    kind: String,
    library: Signal<ResourceLibrary>,
    #[props(default)] session_name: Option<String>,
    #[props(default)] session_scope: Option<String>,
    #[props(default)] on_back: Option<EventHandler<()>>,
) -> Element {
    let mut search = use_signal(String::new);
    let mut status = use_signal(|| "all".to_owned());
    let mut detail = use_signal(|| None::<String>);
    let mut paste_open = use_signal(|| false);
    let mut paste = use_signal(String::new);
    let mut error = use_signal(String::new);
    let mut import_error = use_signal(String::new);
    let mut import_notice = use_signal(String::new);
    let title = match kind.as_str() {
        "skills" => "Skills，清楚地用在每一步",
        "extensions" => "扩展",
        _ => "Packages，给会话一组能力",
    };
    let subtitle = match kind.as_str() {
        "skills" => "找到技能、明确来源，再查看它是否被获取或引用。",
        "extensions" => "设计目录与节点安装状态分开显示。",
        _ => "具名、版本化的 Skills、工具声明与提示词组合。",
    };
    let query = search.read().to_lowercase();
    let status_value = status.read().clone();
    let state = library.read();
    let package_count = state.packages.len();
    let storage_error = state.storage_error.clone();
    let packages = state
        .packages
        .iter()
        .filter(|item| {
            item.resource_kind.eq_ignore_ascii_case("package")
                && format!("{} {} {}", item.name, item.id, item.description)
                    .to_lowercase()
                    .contains(&query)
                && status_matches(&status_value, item.acquired, item.node_installed)
        })
        .cloned()
        .collect::<Vec<_>>();
    let skills = state
        .skills
        .iter()
        .filter(|item| {
            format!("{} {} {}", item.name, item.description, item.source)
                .to_lowercase()
                .contains(&query)
                && status_matches(&status_value, item.acquired, item.node_installed)
        })
        .cloned()
        .collect::<Vec<_>>();
    let extensions = state
        .extensions
        .iter()
        .filter(|item| {
            format!("{} {} {}", item.name, item.description, item.source)
                .to_lowercase()
                .contains(&query)
                && status_matches(&status_value, item.acquired, item.node_installed)
        })
        .cloned()
        .collect::<Vec<_>>();
    drop(state);
    let count = match kind.as_str() {
        "skills" => skills.len(),
        "extensions" => extensions.len(),
        _ => packages.len(),
    };
    let session_name_label = session_name.as_deref().unwrap_or("请先选择会话");
    let import = move |_| {
        let source = paste.read().clone();
        match serde_json::from_str::<Value>(&source) {
            Ok(value) => match import_design_package(&mut library.write(), &value) {
                Ok(key) => {
                    paste.set(String::new());
                    import_error.set(String::new());
                    paste_open.set(false);
                    import_notice.set(format!(
                        "已载入本地资源库：{key}。等待节点安装；存储失败会另行提示。"
                    ));
                }
                Err(message) => import_error.set(message),
            },
            Err(message) => import_error.set(format!("无法解析 JSON：{message}")),
        }
    };
    rsx! {
        div { class: "resources-page",
            div { class: "resource-heading",
                div { span { class: "resource-overline", "RESOURCE LIBRARY" } h1 { "{title}" } p { "{subtitle}" } }
                div { class: "resource-heading-actions",
                    button { class: "button", onclick: move |_| {
                        error.set(String::new());
                        import_error.set(String::new());
                        import_notice.set(String::new());
                        paste_open.set(true);
                    }, Icon { name: "plus" }, "导入资源包" }
                    span { class: "resource-count", "{count} 项" }
                }
            }
            div { class: "resource-notice", "下载后安装，安装后配置会话。状态会区分本地内容、节点安装与会话选择。" }
            if session_name.is_some() || on_back.is_some() {
                div { class: "resource-context-strip",
                    div { Icon { name: "message" } span { "当前会话" } strong { "{session_name_label}" } }
                    if let Some(on_back) = on_back {
                        button { class: "small-link", onclick: move |_| on_back.call(()), "返回对话" }
                    }
                }
            }
            div { class: "resource-filters",
                input { value: "{search}", placeholder: "搜索名称、用途或来源…", aria_label: "搜索资源", oninput: move |event| search.set(event.value()) }
                select { value: "{status}", aria_label: "状态筛选", onchange: move |event| status.set(event.value()),
                    option { value: "all", "全部状态" } option { value: "acquired", "已获取" }
                    option { value: "installed", "节点已安装" } option { value: "pending", "等待节点安装" }
                }
                if kind == "packages" { span { class: "resource-filter-note", "{package_count} 个版本记录" } }
            }
            if let Some(storage_error) = storage_error.as_ref() { p { class: "resource-error", role: "alert", "{storage_error}" } }
            if !error.read().is_empty() { p { class: "resource-error", role: "alert", "{error}" } }
            if !import_notice.read().is_empty() {
                div { class: "resource-import-feedback",
                    if let Some(storage_error) = storage_error.as_ref() {
                        p { class: "resource-error", role: "alert", "{storage_error}。本次内容仅载入内存，尚未确认保存。" }
                    } else {
                        p { class: "resource-import-notice", role: "status", "{import_notice}" }
                    }
                    button { class: "button", aria_label: "关闭导入反馈", onclick: move |_| import_notice.set(String::new()), "关闭" }
                }
            }
            if kind == "packages" {
                if packages.is_empty() { EmptyState { text: "没有匹配的资源包。" } }
                else { div { class: "resource-grid", for package in packages { PackageCard { package, library, detail, error, session_scope: session_scope.clone() } } } }
            } else if kind == "skills" {
                if skills.is_empty() { EmptyState { text: "没有匹配的技能。" } }
                else { div { class: "resource-grid", for skill in skills { SkillCard { skill, library, detail, error, session_scope: session_scope.clone() } } } }
            } else {
                if extensions.is_empty() { EmptyState { text: "暂无扩展设计。" } }
                else { div { class: "resource-grid", for extension in extensions { ExtensionCard { extension, library, detail, error } } } }
            }
            if *paste_open.read() {
                Modal { id: "resource-import", title: "导入设计包", class: "resource-modal", onclose: move |()| paste_open.set(false),
                    div { class: "modal-body resource-import-body",
                        p { "粘贴 schemaVersion 1 的 rpi-work-design-package JSON。最大 200 KiB；完整保存声明、源码与测试快照，不执行其中内容。" }
                        input { r#type: "file", accept: ".json,application/json", onchange: move |event| {
                            let files = event.files();
                            if let Some(file) = files.first() {
                                if file.size() > u64::try_from(MAX_PACKAGE_BYTES).expect("package limit fits u64") { import_error.set("设计包不能超过 200 KiB".into()); }
                                else {
                                    let file = file.clone();
                                    spawn(async move {
                                        match file.read_string().await {
                                            Ok(text) => { paste.set(text); import_error.set(String::new()); }
                                            Err(problem) => import_error.set(format!("读取文件失败：{problem}")),
                                        }
                                    });
                                }
                            }
                        } }
                        textarea { value: "{paste}", rows: 12, placeholder: "粘贴 JSON…", oninput: move |event| paste.set(event.value()) }
                        if !import_error.read().is_empty() { p { class: "resource-error resource-import-error", role: "alert", "{import_error}" } }
                        if let Some(storage_error) = storage_error.as_ref() { p { class: "resource-error", role: "alert", "{storage_error}" } }
                        div { class: "modal-actions",
                            button { class: "secondary", onclick: move |_| paste_open.set(false), "取消" }
                            button { class: "button primary", onclick: import, "校验并导入" }
                        }
                    }
                }
            }
            if let Some(resource_key) = detail.read().clone() {
                ResourceDetail { resource_key, kind: kind.clone(), library, onclose: move |()| detail.set(None) }
            }
        }
    }
}

#[component]
fn PackageCard(
    package: DesignPackage,
    library: Signal<ResourceLibrary>,
    detail: Signal<Option<String>>,
    error: Signal<String>,
    session_scope: Option<String>,
) -> Element {
    let key = package_key(&package);
    let detail_key = key.clone();
    let action_key = key.clone();
    let export = package.clone();
    let state = library.read();
    let desired = state
        .sessions
        .iter()
        .find(|session| Some(&session.session_id) == session_scope.as_ref())
        .is_some_and(|session| package_selected(session, &package));
    drop(state);
    let package_id = package.id.clone();
    let package_version = package.version.clone();
    rsx! {
        article { class: "resource-card package-card",
            div { class: "resource-card-title",
                span { class: "resource-glyph", Icon { name: "blocks" } }
                div { h3 { "{package.name}" } code { "{key}" } }
                span { class: "resource-badge", "{package_state(&package, desired)}" }
            }
            p { "{package.description}" }
            div { class: "resource-summary", span { "{package.skills.len()} Skills" } span { "{package.tools.len()} Tools" } span { "{package.prompts.len()} 提示词" } }
            small { "{package.author} · {package.source}" }
            Steps { acquired: package.acquired, installed: package.node_installed, desired }
            div { class: "resource-actions",
                button { class: "button", onclick: move |_| detail.set(Some(detail_key.clone())), "查看内容" }
                button { class: "button", onclick: move |_| { let package = export.clone(); spawn(async move { if let Err(message) = export_package(package).await { error.set(message); } }); }, "导出 JSON" }
                if !package.acquired {
                    button { class: "button primary", disabled: package.design_sample, onclick: move |_| {
                        if let Some(item) = library.write().packages.iter_mut().find(|item| package_key(item) == action_key) { item.acquired = true; }
                    }, "下载设计包" }
                } else if !package.node_installed {
                    span { class: "resource-ready", "已下载 · 等待节点安装" }
                } else {
                    span { class: "resource-ready", "已下载 · 节点已安装" }
                }
                if package.acquired {
                    if let Some(scope) = session_scope {
                        button { class: "button primary", onclick: move |_| {
                            let mut state = library.write();
                            let session = state.session_mut(&scope);
                            set_package_selected(session, &package_id, &package_version, !desired);
                        }, if desired { "移除当前会话" } else { "用于本会话" } }
                        if desired { small { "本地配置 · 待节点应用" } }
                    } else { small { "请先返回对话选择会话" } }
                }
            }
        }
    }
}

#[component]
fn SkillCard(
    skill: SkillResource,
    library: Signal<ResourceLibrary>,
    detail: Signal<Option<String>>,
    error: Signal<String>,
    session_scope: Option<String>,
) -> Element {
    let detail_id = skill.id.clone();
    let action_id = skill.id.clone();
    let state = library.read();
    let desired = state
        .sessions
        .iter()
        .find(|session| Some(&session.session_id) == session_scope.as_ref())
        .is_some_and(|session| {
            effective_skills(&state, session)
                .iter()
                .any(|item| item.id == skill.id)
        });
    drop(state);
    let owner = library
        .read()
        .packages
        .iter()
        .find(|package| package_key(package) == skill.source)
        .cloned();
    rsx! {
        article { class: "resource-card skill-card",
            div { class: "resource-card-title",
                span { class: "resource-glyph", Icon { name: "skills" } }
                div { h3 { "{skill.name}" } code { "{skill.id}" } }
                span { class: "resource-badge", "{skill_state(&skill, desired)}" }
            }
            p { "{skill.description}" }
            code { class: "resource-path", "{skill.path}" }
            small { "{skill.source}" }
            Steps { acquired: skill.acquired, installed: skill.node_installed, desired }
            div { class: "resource-actions",
                button { class: "button", onclick: move |_| detail.set(Some(detail_id.clone())), "查看全文与来源" }
                if let Some(owner) = owner {
                    button { class: "button", onclick: move |_| { let package = owner.clone(); spawn(async move { if let Err(message) = export_package(package).await { error.set(message); } }); }, "导出 JSON" }
                }
                if !skill.acquired {
                    button { class: "button primary", onclick: move |_| {
                        if let Some(item) = library.write().skills.iter_mut().find(|item| item.id == action_id) { item.acquired = true; }
                    }, "获取本地内容" }
                } else { button { class: "button", disabled: true, "等待节点安装" } }
            }
        }
    }
}

#[component]
fn ExtensionCard(
    extension: ExtensionResource,
    library: Signal<ResourceLibrary>,
    detail: Signal<Option<String>>,
    error: Signal<String>,
) -> Element {
    let detail_id = extension.id.clone();
    let action_id = extension.id.clone();
    let owner = library
        .read()
        .packages
        .iter()
        .find(|package| package_key(package) == extension.source)
        .cloned();
    rsx! {
        article { class: "resource-card extension-card",
            div { class: "resource-card-title",
                span { class: "resource-glyph", Icon { name: "layout" } }
                div { h3 { "{extension.name}" } code { "{extension.id}" } }
                span { class: "resource-badge", "{extension_state(&extension)}" }
            }
            p { "{extension.description}" }
            small { "{extension.source} · 设计资源" }
            Steps { acquired: extension.acquired, installed: extension.node_installed, desired: false }
            div { class: "resource-actions",
                button { class: "button", onclick: move |_| detail.set(Some(detail_id.clone())), "查看设计" }
                if let Some(owner) = owner {
                    button { class: "button", onclick: move |_| { let package = owner.clone(); spawn(async move { if let Err(message) = export_package(package).await { error.set(message); } }); }, "导出 JSON" }
                }
                if !extension.acquired {
                    button { class: "button primary", onclick: move |_| {
                        if let Some(item) = library.write().extensions.iter_mut().find(|item| item.id == action_id) { item.acquired = true; }
                    }, "获取设计" }
                }
                button { class: "button", disabled: true, "等待节点安装" }
            }
        }
    }
}

#[component]
fn Steps(acquired: bool, installed: bool, desired: bool) -> Element {
    rsx! {
        ol { class: "resource-steps",
            li { class: if acquired { "done" } else { "" }, span { if acquired { "✓" } else { "1" } } "获取" }
            li { class: if installed { "done" } else { "" }, span { if installed { "✓" } else { "2" } } "节点安装" }
            li { span { "3" } if desired { "已选 · 待应用" } else { "会话配置" } }
        }
    }
}

#[component]
fn EmptyState(text: &'static str) -> Element {
    rsx! { div { class: "resource-empty", Icon { name: "search" }, strong { "{text}" } p { "调整搜索或状态筛选后再试。" } } }
}

#[component]
fn ResourceDetail(
    resource_key: String,
    kind: String,
    library: Signal<ResourceLibrary>,
    onclose: EventHandler<()>,
) -> Element {
    let state = library.read();
    let package = state
        .packages
        .iter()
        .find(|item| package_key(item) == resource_key)
        .cloned();
    let skill = state
        .skills
        .iter()
        .find(|item| item.id == resource_key)
        .cloned();
    let extension = state
        .extensions
        .iter()
        .find(|item| item.id == resource_key)
        .cloned();
    drop(state);
    rsx! {
        Modal { id: "resource-detail", title: "资源详情", class: "resource-modal", onclose: move |()| onclose.call(()),
            div { class: "modal-body resource-detail-body",
                if kind == "packages" {
                    if let Some(package) = package { PackageDetail { package } }
                } else if kind == "skills" {
                    if let Some(skill) = skill { SkillDetail { skill } }
                } else {
                    if let Some(extension) = extension { ExtensionDetail { extension } }
                }
            }
        }
    }
}

#[component]
fn PackageDetail(package: DesignPackage) -> Element {
    rsx! {
        div { class: "resource-detail-content",
            code { "{package_key(&package)}" } span { "{package.source} · {package.author}" } p { "{package.description}" }
            h3 { "Skills" }
            if package.skill_contents.is_empty() { small { "未包含" } }
            for skill in package.skill_contents {
                details { summary { "{skill.name} · {skill.id}" } p { "{skill.description}" } pre { "{skill.content}" } }
            }
            h3 { "Tools" } div { class: "resource-tool-tags", for tool in package.tools { code { "{tool}" } } }
            h3 { "提示词" }
            for prompt in package.prompts { details { summary { "{prompt.name} · {prompt.kind}" } pre { "{prompt.content}" } } }
            p { class: "resource-boundary", "这是设计包内容；原生 cdylib、节点安装和模型载入仍需宿主协议。" }
        }
    }
}

#[component]
fn SkillDetail(skill: SkillResource) -> Element {
    rsx! {
        div { class: "resource-detail-content",
            code { "{skill.id}" } span { "来源：{skill.source}" } p { "{skill.description}" }
            dl { div { dt { "状态" } dd { "{skill_state(&skill, false)}" } } div { dt { "路径" } dd { "{skill.path}" } } }
            h3 { "SKILL.md 正文" } pre { "{skill.content}" }
            p { class: "resource-boundary", "查看正文只证明本地内容可读；显式引用后才会进入草稿。" }
        }
    }
}

#[component]
fn ExtensionDetail(extension: ExtensionResource) -> Element {
    rsx! {
        div { class: "resource-detail-content",
            code { "{extension.id}" } span { "来源：{extension.source}" } p { "{extension.description}" }
            h3 { "设计正文" } pre { "{extension.content}" }
            p { class: "resource-boundary", "此项是扩展设计资源，不是可执行原生插件；节点安装接口接入前不会显示已安装或已启用。" }
        }
    }
}

#[component]
pub fn SessionResources(
    session_id: String,
    #[props(default = false)] compact: bool,
    #[props(default)] session_title: Option<String>,
    #[props(default)] on_browse: Option<EventHandler<()>>,
    selected_session: Signal<Option<String>>,
    library: Signal<ResourceLibrary>,
    onquote: EventHandler<String>,
    shared_client: Signal<Option<WorkClient>>,
    session_generation: Signal<u64>,
    requests: Signal<Vec<Task>>,
    runtime_state: Signal<Option<RuntimeState>>,
    status: Signal<String>,
    open_requested: Signal<bool>,
) -> Element {
    let mut open = use_signal(|| false);
    let mut tab = use_signal(|| "packages".to_owned());
    use_effect(move || {
        if *open_requested.read() {
            tab.set("skills".into());
            open.set(true);
            open_requested.set(false);
        }
    });
    let mut package_detail = use_signal(|| None::<String>);
    let state = library.read();
    let binding = state
        .sessions
        .iter()
        .find(|session| session.session_id == session_id)
        .cloned()
        .unwrap_or_else(|| SessionBinding {
            session_id: session_id.clone(),
            revision: 1,
            ..SessionBinding::default()
        });
    let packages = state
        .packages
        .iter()
        .filter(|item| {
            binding
                .package_pins
                .iter()
                .any(|pin| pin.id == item.id && pin.version == item.version)
        })
        .cloned()
        .collect::<Vec<_>>();
    let skill_count = effective_skills(&state, &binding).len();
    let storage_error = state.storage_error.clone();
    drop(state);
    let prompt_count = packages
        .iter()
        .map(|package| {
            package
                .prompts
                .iter()
                .filter(|prompt| {
                    !binding
                        .disabled_prompts
                        .contains(&prompt_key(package, prompt))
                })
                .count()
        })
        .sum::<usize>()
        + usize::from(!binding.instructions.trim().is_empty());
    let config_count = binding.package_pins.len();
    let tool_count = packages
        .iter()
        .flat_map(|item| item.tools.iter())
        .collect::<BTreeSet<_>>()
        .len();
    let package_label = if packages.is_empty() {
        "未绑定 Package".into()
    } else {
        format!("{} 个 Package", packages.len())
    };
    let session_title_label = session_title.as_deref().unwrap_or("当前会话");
    rsx! {
        div { class: "session-resource-entry",
            button { class: "session-config-summary", onclick: move |_| { tab.set("packages".into()); open.set(true); }, aria_label: "打开会话资源配置", title: "{package_label} · r{binding.revision} · {skill_count} Skills · {tool_count} Tools · {prompt_count} prompts",
                Icon { name: "blocks" }, span { class: "session-config-label", "会话配置" }, b { "{config_count}" }
            }
            for (key, label, count) in [("skills", "Skills", skill_count), ("tools", "Tools", tool_count), ("prompts", "提示词", prompt_count)] {
                button { class: "session-capability-trigger", aria_label: "打开会话{label}配置", onclick: move |_| { tab.set(key.to_owned()); open.set(true); }, span { "{label}" } b { "{count}" } }
            }
            if !compact {
                span { class: "session-resource-caption", "{package_label} · {skill_count} Skills · {tool_count} Tools · {prompt_count} prompts" }
            }
            if *open.read() {
                Modal { id: "session-resources", title: "会话资源配置", class: "resource-modal", onclose: move |()| open.set(false),
                    div { class: "modal-body session-resource-body",
                        if let Some(storage_error) = storage_error { small { class: "resource-error", role: "alert", "{storage_error}" } }
                        div { class: "resource-session-heading",
                            div { strong { "{session_title_label}" } small { "{package_label}" } }
                            span { "会话配置 r{binding.revision}" }
                        }
                        div { class: "resource-segments",
                            for (key, label) in [("packages", "Packages"), ("skills", "Skills"), ("tools", "Tools"), ("prompts", "提示词")] {
                                button { class: if *tab.read() == key { "active" } else { "" }, onclick: move |_| tab.set(key.to_owned()), "{label}" }
                            }
                        }
                        if *tab.read() == "packages" {
                            SessionPackages {
                                session_id: session_id.clone(), library, binding: binding.clone(),
                                on_browse, package_detail, onclose: move |()| open.set(false),
                            }
                        }
                        if *tab.read() == "skills" { SessionSkills { session_id: session_id.clone(), library, binding: binding.clone(), onquote } }
                        if *tab.read() == "tools" {
                            SessionTools { packages: packages.clone() }
                            crate::session_config::SessionConfig { shared_client, selected_session, session_generation, requests, runtime_state, status, tools_only: true }
                        }
                        if *tab.read() == "prompts" { SessionPrompts { session_id: session_id.clone(), packages: packages.clone(), binding: binding.clone(), library, onquote } }
                    }
                }
                if let Some(resource_key) = package_detail.read().clone() {
                    ResourceDetail {
                        resource_key, kind: "packages".to_owned(), library,
                        onclose: move |()| package_detail.set(None),
                    }
                }
            }
        }
    }
}

#[component]
fn SessionPackages(
    session_id: String,
    library: Signal<ResourceLibrary>,
    binding: SessionBinding,
    on_browse: Option<EventHandler<()>>,
    package_detail: Signal<Option<String>>,
    onclose: EventHandler<()>,
) -> Element {
    let export_error = use_signal(String::new);
    let packages = library
        .read()
        .packages
        .iter()
        .filter(|item| item.acquired && item.resource_kind.eq_ignore_ascii_case("package"))
        .cloned()
        .collect::<Vec<_>>();
    rsx! {
        div { class: "session-resource-list session-package-list",
            div { class: "session-resource-intro",
                p { class: "meta", "选择固定版本用于此会话，等待执行节点应用。新版本不会自动替换当前选择。" }
                if let Some(on_browse) = on_browse {
                    button { class: "small-link", onclick: move |_| { onclose.call(()); on_browse.call(()); }, "浏览与导入 Package" }
                }
            }
            if packages.is_empty() { EmptyState { text: "当前资源库没有已获取的 Package。" } }
            for package in packages {
                {
                    let active = binding.package_pins.iter().any(|pin| pin.id == package.id && pin.version == package.version);
                    let key = package_key(&package);
                    let session_id = session_id.clone();
                    let package_id = package.id.clone();
                    let package_version = package.version.clone();
                    let detail_key = key.clone();
                    rsx! {
                        article { class: "session-resource-row resource-card session-package-card",
                            div { class: "resource-card-title",
                                span { class: "resource-glyph", Icon { name: "blocks" } }
                                div { h3 { "{package.name}" } code { "{key}" } }
                                span { class: "resource-badge", if active { "已选择 · 待节点应用" } else if package.node_installed { "节点已安装 · 未选择" } else { "未选择 · 待节点安装" } }
                            }
                            p { "{package.description}" }
                            div { class: "resource-summary", span { "{package.skills.len()} Skills" } span { "{package.tools.len()} Tools" } span { "{package.prompts.len()} prompts" } }
                            small { "{package.author} · {package.source}" }
                            Steps { acquired: package.acquired, installed: package.node_installed, desired: active }
                            div { class: "resource-actions",
                                button { class: "button", onclick: move |_| package_detail.set(Some(detail_key.clone())), "查看内容" }
                                { let package = package.clone(); rsx! {
                                    button { class: "button", onclick: move |_| { let package = package.clone(); let mut export_error = export_error; spawn(async move { if let Err(message) = export_package(package).await { export_error.set(message); } }); }, "导出 JSON" }
                                } }
                                button { class: "button primary", onclick: move |_| {
                                    let mut state = library.write();
                                    let session = state.session_mut(&session_id);
                                    set_package_selected(session, &package_id, &package_version, !active);
                                }, if active { "移除当前会话" } else { "用于本会话" } }
                            }
                            if !export_error.read().is_empty() { p { class: "resource-error", role: "alert", "{export_error}" } }

                        }
                    }
                }
            }
        }
    }
}

#[component]
fn SessionSkills(
    session_id: String,
    library: Signal<ResourceLibrary>,
    binding: SessionBinding,
    onquote: EventHandler<String>,
) -> Element {
    let skills = library
        .read()
        .skills
        .iter()
        .filter(|item| {
            item.acquired && !package_skill_selected(&library.read(), &binding, &item.id)
        })
        .cloned()
        .collect::<Vec<_>>();
    let package_skills = library
        .read()
        .packages
        .iter()
        .filter(|package| {
            binding
                .package_pins
                .iter()
                .any(|pin| pin.id == package.id && pin.version == package.version)
        })
        .flat_map(|package| {
            package.skill_contents.iter().map(|skill| {
                (
                    format!("{}@{}/{}", package.id, package.version, skill.id),
                    skill.clone(),
                )
            })
        })
        .collect::<Vec<_>>();
    rsx! {
        div { class: "session-resource-list",
            p { class: "meta", "Skills 开关属于独立 desired 配置；包内 Skills 随固定版本选择。节点应用前不代表模型已启用，引用到草稿仅作用于显式消息。" }
            for (key, skill) in package_skills {
                {
                    let quote = format!("请在本条消息中显式参考 Skill {}（{}）：\n\n{}", skill.name, key, skill.content);
                    let active = !binding.disabled_skills.contains(&key);
                    let toggle_key = key.clone();
                    let toggle_session = session_id.clone();
                    rsx! { div { class: "session-resource-row",
                        input { r#type: "checkbox", checked: active, aria_label: "启用包内 Skill {key}", onchange: move |event| {
                            let mut state = library.write();
                            let session = state.session_mut(&toggle_session);
                            session.disabled_skills.retain(|id| id != &toggle_key);
                            if !event.checked() { session.disabled_skills.push(toggle_key.clone()); }
                            session.revision += 1;
                        } }
                        div { strong { "{skill.name}" } small { "{key} · 包内来源 · 待节点应用" } }
                        span { class: "resource-badge", if active { "已选择 · 待节点应用" } else { "已取消 · 待节点应用" } }
                        button { class: "small-link", disabled: !active, onclick: move |_| onquote.call(quote.clone()), "引用到草稿" }
                    } }
                }
            }
            for skill in skills {
                {
                    let active = binding.skills.contains(&skill.id);
                    let session_id = session_id.clone();
                    let skill_id = skill.id.clone();
                    let quote = format!("请在本条消息中显式参考 Skill {}（{}）：\n\n{}", skill.name, skill.source, skill.content);
                    rsx! {
                        div { class: "session-resource-row",
                            input { r#type: "checkbox", checked: active, aria_label: "选择 Skill {skill.name}", onchange: move |event| {
                                let mut state = library.write();
                                let session = state.session_mut(&session_id);
                                session.skills.retain(|id| id != &skill_id);
                                if event.checked() { session.skills.push(skill_id.clone()); }
                                session.revision += 1;
                            } }
                            div { strong { "{skill.name}" } small { "{skill.source} · 全文已获取" } }
                            span { class: "resource-badge", if active { "已选择 · 待节点应用" } else { "未选择" } }
                            button { class: "small-link", onclick: move |_| onquote.call(quote.clone()), "引用到草稿" }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn SessionTools(packages: Vec<DesignPackage>) -> Element {
    let tools = packages
        .iter()
        .flat_map(|package| package.tools.iter().cloned())
        .collect::<BTreeSet<_>>();
    rsx! {
        div { class: "session-resource-list",
            p { class: "meta", "下方复选框来自节点可用工具目录，启停会调用 set_active_tools。Package 仅声明设计意图，不代表工具已安装或启用。" }
            for tool in tools { div { class: "session-resource-row", strong { "{tool}" } span { class: "resource-badge", "Package 声明 · 待节点应用" } } }
            if packages.is_empty() { EmptyState { text: "当前会话没有具名 Package 声明。" } }
        }
    }
}

#[component]
fn SessionPrompts(
    session_id: String,
    packages: Vec<DesignPackage>,
    binding: SessionBinding,
    mut library: Signal<ResourceLibrary>,
    onquote: EventHandler<String>,
) -> Element {
    let instructions_session = session_id.clone();
    let prompts = packages
        .iter()
        .flat_map(|package| {
            package.prompts.iter().map(|prompt| {
                (
                    prompt_key(package, prompt),
                    format!("{} @ {}", package.name, package.version),
                    prompt.clone(),
                )
            })
        })
        .collect::<Vec<_>>();
    rsx! {
        div { class: "session-resource-list",
            p { class: "meta", "提示词与追加指令自动保存到当前会话本地配置，待节点应用；不会自动追加到普通消息。template 只有点击引用后才进入草稿。" }
            for (key, owner, prompt) in prompts {
                {
                    let active = !binding.disabled_prompts.contains(&key);
                    let session_id = session_id.clone();
                    let quote = prompt.content.clone();
                    rsx! {
                        div { class: "session-prompt", key: "{key}",
                            div { class: "session-resource-row",
                                input { r#type: "checkbox", checked: active, aria_label: "启用提示词 {key}", onchange: move |event| {
                                    let mut state = library.write();
                                    let session = state.session_mut(&session_id);
                                    session.disabled_prompts.retain(|id| id != &key);
                                    if !event.checked() { session.disabled_prompts.push(key.clone()); }
                                    session.revision += 1;
                                } }
                                div { strong { "{prompt.name}" } small { "{prompt.kind} · {owner} · 待节点应用" } }
                                if prompt.kind == "template" { button { class: "small-link", disabled: !active, onclick: move |_| onquote.call(quote.clone()), "引用到草稿" } }
                            }
                            pre { "{prompt.content}" }
                        }
                    }
                }
            }
            if packages.is_empty() { EmptyState { text: "当前会话没有 Package 提示词。" } }
            label { class: "session-instructions", span { "追加会话指令 · 待节点应用" }
                textarea { rows: 5, maxlength: 4000, value: "{binding.instructions}", placeholder: "自动保存本地配置，最多 4000 字，待节点应用…", oninput: move |event| {
                    let mut state = library.write();
                    let session = state.session_mut(&instructions_session);
                    let instructions = event.value();
                    if session.instructions != instructions { session.instructions = instructions; session.revision += 1; }
                } }
            }
        }
    }
}

#[component]
pub fn SkillPicker(
    session_id: String,
    library: Signal<ResourceLibrary>,
    onquote: EventHandler<String>,
    open_session_config: Signal<bool>,
) -> Element {
    let mut open = use_signal(|| false);
    let state = library.read();
    let binding = state
        .sessions
        .iter()
        .find(|session| session.session_id == session_id)
        .cloned()
        .unwrap_or_default();
    let skills = session_skills(&state, &binding);
    let selected = effective_skills(&state, &binding);
    let storage_error = state.storage_error.clone();
    drop(state);
    rsx! {
        div { class: "skill-picker",
            button { class: "skill-picker-trigger", onclick: move |_| open.set(true), Icon { name: "skills" }, "Skills", span { "{selected.len()}" } }
            if !selected.is_empty() { div { class: "skill-picker-chips", for skill in selected { span { "{skill.name}" } } } }
            if *open.read() {
                Modal { id: "skill-picker", title: "给下条消息指定技能", class: "resource-modal", onclose: move |()| open.set(false),
                    div { class: "modal-body skill-picker-body",
                        p { class: "meta", "显式引用只写入草稿；全文、来源和当前会话 desired 选择均可核对。" }
                        button { class: "small-link skill-picker-config-link", onclick: move |_| { open.set(false); open_session_config.set(true); }, "打开会话配置以恢复选择" }
                        if let Some(storage_error) = storage_error { p { class: "resource-error", role: "alert", "{storage_error}" } }
                        div { class: "skill-picker-list",
                        for skill in skills {
                            {
                                let active = effective_skills(&library.read(), &binding).iter().any(|item| item.id == skill.id);
                                let quote = format!("请参考 Skill {}（{}）：\n\n{}", skill.name, skill.source, skill.content);
                                rsx! {
                                    div { class: "skill-picker-row",
                                        span { class: "skill-picker-icon", aria_hidden: "true", Icon { name: "skills" } }
                                        div { strong { "{skill.name}" } small { "{skill.source} · {skill.description}" } small { if active { "desired 已选 · 待节点应用" } else { "未选择 · 请先在会话配置中选择" } } }
                                        button { class: "skill-picker-quote", aria_label: "引用到草稿", title: "引用 {skill.name} 到草稿", disabled: !active, onclick: move |_| onquote.call(quote.clone()), Icon { name: "plus" } span { "引用到草稿" } }
                                    }
                                }
                            }
                        }
                        }
                    }
                }
            }
        }
    }
}

fn status_matches(status: &str, acquired: bool, installed: bool) -> bool {
    match status {
        "acquired" => acquired,
        "installed" => installed,
        "pending" => acquired && !installed,
        _ => true,
    }
}
fn package_key(package: &DesignPackage) -> String {
    format!("{}@{}", package.id, package.version)
}
fn package_state(package: &DesignPackage, desired: bool) -> &'static str {
    if desired {
        "desired 已绑定 · 待应用"
    } else if package.node_installed {
        "节点已安装"
    } else if package.acquired {
        "已获取 · 待节点安装"
    } else {
        "未获取"
    }
}
fn skill_state(skill: &SkillResource, desired: bool) -> &'static str {
    if desired {
        "desired 已选 · 待应用"
    } else if skill.node_installed {
        "节点已安装"
    } else if skill.acquired {
        "已获取 · 待节点安装"
    } else {
        "未获取"
    }
}
fn extension_state(extension: &ExtensionResource) -> &'static str {
    if extension.node_installed {
        "节点已安装 · 待启用"
    } else if extension.acquired {
        "已获取 · 待节点安装"
    } else {
        "未获取"
    }
}
fn string_field(value: &Value, field: &str) -> Result<String, String> {
    value[field]
        .as_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| format!("缺少有效的 {field}"))
}
fn bounded_text(value: &Value, field: &str, max: usize) -> Result<String, String> {
    let text = string_field(value, field)?;
    if text.trim().is_empty() || text.len() > max {
        Err(format!("{field} 为空或超过 {max} 字节"))
    } else {
        Ok(text)
    }
}
fn valid_token(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 90
        && text
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        && !matches!(text, "__proto__" | "prototype" | "constructor")
}

fn manifest_tool_ids(tools: &[Value]) -> Result<BTreeSet<String>, String> {
    let allowed = [
        "read",
        "grep",
        "edit",
        "powershell",
        "ask_user",
        "todo",
        "webfetch",
        "docs.search",
        "delegate_task",
        "browser",
    ];
    let mut tool_ids = BTreeSet::new();
    for tool in tools {
        let id = tool.as_str().ok_or("工具声明必须是字符串")?;
        if !allowed.contains(&id) {
            return Err(format!("未知工具声明：{id}"));
        }
        tool_ids.insert(id.to_owned());
    }
    Ok(tool_ids)
}

fn package_from_manifest(value: &Value) -> Result<DesignPackage, String> {
    if value["schemaVersion"] != 1 || value["kind"] != "rpi-work-design-package" {
        return Err("资源包格式不正确：需要 schemaVersion 1 的 rpi-work-design-package".into());
    }
    let id = bounded_text(value, "id", 90)?;
    if !valid_token(&id) {
        return Err("id 只能包含字母、数字、点、下划线或连字符".into());
    }
    let version = bounded_text(value, "version", 32)?;
    if version.split('.').count() != 3
        || version.split('.').any(|part| part.parse::<u64>().is_err())
    {
        return Err("version 必须是三段数字版本号".into());
    }
    let name = bounded_text(value, "name", 80)?;
    let author = bounded_text(value, "author", 80)?;
    let description = bounded_text(value, "description", 1000)?;
    let resource_kind = match value["design"].get("resourceKind") {
        Some(kind) => kind.as_str().ok_or("design.resourceKind 必须是字符串")?,
        None => "Package",
    }
    .to_owned();
    if !matches!(
        resource_kind.to_ascii_lowercase().as_str(),
        "package" | "skill" | "extension"
    ) {
        return Err("design.resourceKind 必须是 Package、Skill 或 Extension".into());
    }
    let resources = value.get("resources").ok_or("缺少 resources")?;
    let skills = resources["skills"].as_array().ok_or("skills 必须是数组")?;
    let tools = resources["tools"].as_array().ok_or("tools 必须是数组")?;
    let prompts = resources["prompts"]
        .as_array()
        .ok_or("prompts 必须是数组")?;
    if skills.len() > 20 || tools.len() > 20 || prompts.len() > 20 {
        return Err("每类资源最多 20 项".into());
    }
    if resource_kind.eq_ignore_ascii_case("skill") && skills.is_empty() {
        return Err("Skill 设计资源缺少技能正文".into());
    }
    if resource_kind.eq_ignore_ascii_case("extension") && value["design"]["sources"].is_null() {
        return Err("Extension 设计资源缺少 design.sources".into());
    }
    let tool_ids = manifest_tool_ids(tools)?;
    let mut skill_ids = BTreeSet::new();
    let mut skill_contents = Vec::new();
    for skill in skills {
        let id = bounded_text(skill, "id", 90)?;
        if !valid_token(&id) || !skill_ids.insert(id.clone()) {
            return Err("技能 ID 无效或重复".into());
        }
        skill_contents.push(PackageSkill {
            id,
            name: bounded_text(skill, "name", 80)?,
            description: bounded_text(skill, "description", 1000)?,
            content: bounded_text(skill, "content", 40_000)?,
        });
    }
    let mut prompt_values = Vec::new();
    let mut prompt_ids = BTreeSet::new();
    for prompt in prompts {
        let id = bounded_text(prompt, "id", 90)?;
        if !valid_token(&id) || !prompt_ids.insert(id.clone()) {
            return Err("提示词 ID 无效或重复".into());
        }
        let kind = bounded_text(prompt, "kind", 20)?;
        if kind != "instruction" && kind != "template" {
            return Err("提示词 kind 必须是 instruction 或 template".into());
        }
        prompt_values.push(PromptResource {
            id,
            name: bounded_text(prompt, "name", 80)?,
            kind,
            content: bounded_text(prompt, "content", 40_000)?,
        });
    }
    Ok(DesignPackage {
        id,
        name,
        version,
        author,
        description,
        source: "本地导入".into(),
        resource_kind,
        skill_contents,
        skills: skill_ids.into_iter().collect(),
        tools: tool_ids.into_iter().collect(),
        prompts: prompt_values,
        manifest: Some(value.clone()),
        acquired: true,
        node_installed: false,
        design_sample: false,
    })
}

fn package_manifest_value(package: &DesignPackage) -> Value {
    package.manifest.clone().unwrap_or_else(|| json!({
        "schemaVersion":1,"kind":"rpi-work-design-package","id":package.id,"name":package.name,
        "version":package.version,"author":package.author,"description":package.description,
        "design":{"resourceKind":package.resource_kind},
        "resources":{
            "skills":package.skill_contents.iter().map(|skill| json!({"id":skill.id,"name":skill.name,"description":skill.description,"content":skill.content})).collect::<Vec<_>>(),
            "tools":package.tools,
            "prompts":package.prompts.iter().map(|prompt| json!({"id":prompt.id,"name":prompt.name,"kind":prompt.kind,"content":prompt.content})).collect::<Vec<_>>()
        }
    }))
}
fn package_value(package: &DesignPackage) -> Value {
    json!({"manifest":package_manifest_value(package),"source":package.source,"acquired":package.acquired,"design_sample":package.design_sample})
}
fn package_from_value(value: &Value) -> Result<DesignPackage, String> {
    // Legacy records mixed local state into the manifest. New records use an envelope.
    let manifest = if let Some(manifest) = value.get("manifest") {
        manifest.clone()
    } else {
        let mut manifest = value.clone();
        if let Some(object) = manifest.as_object_mut() {
            for field in ["source", "acquired", "node_installed", "design_sample"] {
                object.remove(field);
            }
        }
        manifest
    };
    let mut package = package_from_manifest(&manifest)?;
    if let Some(source) = value["source"].as_str() {
        source.clone_into(&mut package.source);
    }
    package.acquired = value["acquired"].as_bool().unwrap_or(package.acquired);
    package.design_sample = value["design_sample"].as_bool().unwrap_or(false);
    // Local persisted booleans cannot attest to an execution node's installation state.
    package.node_installed = false;
    Ok(package)
}
fn skill_value(skill: &SkillResource) -> Value {
    json!({"id":skill.id,"name":skill.name,"description":skill.description,"source":skill.source,"path":skill.path,"content":skill.content,"acquired":skill.acquired,"design_sample":skill.design_sample})
}
fn skill_from_value(value: &Value) -> Result<SkillResource, String> {
    Ok(SkillResource {
        id: string_field(value, "id")?,
        name: string_field(value, "name")?,
        description: string_field(value, "description")?,
        source: string_field(value, "source")?,
        path: string_field(value, "path")?,
        content: string_field(value, "content")?,
        acquired: value["acquired"].as_bool().unwrap_or(false),
        node_installed: false,
        design_sample: value["design_sample"].as_bool().unwrap_or(false),
    })
}
fn extension_value(extension: &ExtensionResource) -> Value {
    json!({"id":extension.id,"name":extension.name,"description":extension.description,"source":extension.source,"content":extension.content,"acquired":extension.acquired,"design_sample":extension.design_sample})
}
fn extension_from_value(value: &Value) -> Result<ExtensionResource, String> {
    Ok(ExtensionResource {
        id: string_field(value, "id")?,
        name: string_field(value, "name")?,
        description: string_field(value, "description")?,
        source: string_field(value, "source")?,
        content: string_field(value, "content")?,
        acquired: value["acquired"].as_bool().unwrap_or(false),
        node_installed: false,
        design_sample: value["design_sample"].as_bool().unwrap_or(false),
    })
}
fn session_value(session: &SessionBinding) -> Value {
    json!({"session_id":session.session_id,"package_pins":session.package_pins.iter().map(|pin| json!({"id":pin.id,"version":pin.version})).collect::<Vec<_>>(),"skills":session.skills,"disabled_skills":session.disabled_skills,"disabled_tools":session.disabled_tools,"disabled_prompts":session.disabled_prompts,"instructions":session.instructions,"revision":session.revision})
}
fn session_from_value(value: &Value) -> Result<SessionBinding, String> {
    let mut package_pins = Vec::new();
    if let Some(items) = value["package_pins"].as_array() {
        for item in items {
            let (Some(id), Some(version)) = (item["id"].as_str(), item["version"].as_str()) else {
                continue;
            };
            if id.trim().is_empty() || version.trim().is_empty() {
                continue;
            }
            if let Some(existing) = package_pins
                .iter_mut()
                .find(|pin: &&mut PackagePin| pin.id == id)
            {
                version.clone_into(&mut existing.version);
            } else {
                package_pins.push(PackagePin {
                    id: id.to_owned(),
                    version: version.to_owned(),
                });
            }
        }
    } else if let (Some(id), Some(version)) = (
        value["package_id"].as_str(),
        value["package_version"].as_str(),
    ) {
        if !id.trim().is_empty() && !version.trim().is_empty() {
            package_pins.push(PackagePin {
                id: id.to_owned(),
                version: version.to_owned(),
            });
        }
    }
    Ok(SessionBinding {
        session_id: string_field(value, "session_id")?,
        package_pins,
        skills: strings(&value["skills"]),
        disabled_skills: strings(&value["disabled_skills"]),
        disabled_tools: strings(&value["disabled_tools"]),
        disabled_prompts: strings(&value["disabled_prompts"]),
        instructions: value["instructions"]
            .as_str()
            .unwrap_or_default()
            .to_owned(),
        revision: value["revision"].as_u64().unwrap_or(1),
    })
}
fn prompt_key(package: &DesignPackage, prompt: &PromptResource) -> String {
    format!("{}@{}/{}", package.id, package.version, prompt.id)
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn sample_library() -> ResourceLibrary {
    let responsive = json!({"id":"global-responsive","name":"responsive-review","description":"检查窄屏布局、触控目标与键盘遮挡。","content":"# Responsive review\n\nInspect navigation, touch targets and safe areas."});
    let review = json!({"id":"global-review","name":"code-review","description":"根据明确的检查范围审阅变更，保留可定位证据。","content":"# Code review\n\nInspect the selected change and explain actionable findings."});
    let notes = json!({"id":"package-notes","name":"release-notes","description":"按用户可见行为整理发布说明。","content":"# Release notes\n\nExplain user-visible changes and relevant limitations."});
    let mobile = json!({"id":"mobile-audit","name":"mobile-audit","description":"审阅手机布局、触控目标与键盘遮挡。","content":"# Mobile audit\n\nInspect touch targets, scrolling and keyboard overlap."});
    let manifests = [
        json!({"schemaVersion":1,"kind":"rpi-work-design-package","id":"rpi.rust-workbench","name":"Rust Workbench","version":"1.2.0","author":"RPI Design","description":"Rust 编程工作台：审阅技能、基础工具和编码约定。","resources":{"skills":[responsive,review,notes],"tools":["read","grep","edit","powershell","ask_user","todo"],"prompts":[{"id":"coding","name":"编码约定","kind":"instruction","content":"优先阅读现有代码，保持修改聚焦，说明实际验证结果。"},{"id":"review","name":"审阅变更","kind":"template","content":"请审阅当前变更，按严重程度列出可定位的问题，并说明验证依据。"}]}}),
        json!({"schemaVersion":1,"kind":"rpi-work-design-package","id":"rpi.research","name":"Research Kit","version":"0.4.0","author":"RPI Design","description":"网页、文档和子任务能力，可独立在会话中启用。","resources":{"skills":[],"tools":["webfetch","docs.search","delegate_task","browser"],"prompts":[{"id":"research","name":"资料检索","kind":"template","content":"请检索并比较相关资料，给出可核对的来源和结论。"}]}}),
        json!({"schemaVersion":1,"kind":"rpi-work-design-package","id":"rpi.mobile-review","name":"Mobile Review","version":"0.3.0","author":"RPI Design","description":"手机界面审阅设计包，组合技能、只读工具和审阅提示词。","resources":{"skills":[mobile],"tools":["read","grep"],"prompts":[{"id":"mobile","name":"手机体验审阅","kind":"template","content":"请检查手机布局、触控目标、滚动和软键盘遮挡，先报告问题，不修改文件。"}]}}),
    ];
    let packages = manifests
        .iter()
        .map(|value| {
            let mut package = package_from_manifest(value).expect("valid built-in design sample");
            package.source = "设计样例".into();
            package.design_sample = true;
            package
        })
        .collect::<Vec<_>>();
    let mut skills = packages.iter().flat_map(package_skills).collect::<Vec<_>>();
    skills.push(SkillResource {
        id: "api-contract-review".into(),
        name: "api-contract-review".into(),
        description: "检查 API 契约、错误响应与版本兼容性。".into(),
        source: "技能目录 · 设计样例".into(),
        path: "<node>/skills/api-contract-review/SKILL.md".into(),
        content: "# API contract review\n\nInspect request/response contracts and compatibility."
            .into(),
        acquired: true,
        node_installed: false,
        design_sample: true,
    });
    ResourceLibrary {
        packages, skills,
        extensions: vec![ExtensionResource {
            id: "path-check".into(), name: "路径检查 Extension 设计".into(), description: "检查工具路径、边界和项目目录约束的扩展设计。".into(),
            source: "扩展目录 · 设计样例".into(), content: "# Path check extension\n\nDesign only: validate paths before a node-provided action.".into(),
            acquired: true, node_installed: false, design_sample: true,
        }],
        sessions: Vec::new(), storage_error: None,
    }
}

async fn export_package(package: DesignPackage) -> Result<(), String> {
    let value = package_manifest_value(&package);
    let data = serde_json::to_string(&value.to_string()).map_err(|error| error.to_string())?;
    let filename = format!("{}-{}.rpi-package.json", package.id, package.version);
    let filename = serde_json::to_string(&filename).map_err(|error| error.to_string())?;
    let script = format!(
        "try {{ const data=JSON.parse({data}); const blob=new Blob([JSON.stringify(data,null,2)+'\\n'],{{type:'application/json'}}); const url=URL.createObjectURL(blob); const a=document.createElement('a'); a.href=url; a.download={filename}; a.click(); setTimeout(()=>URL.revokeObjectURL(url),1000); dioxus.send({{ok:true}}); }} catch (error) {{ dioxus.send({{ok:false,error:String(error)}}); }}"
    );
    let result = document::eval(&script)
        .recv::<Value>()
        .await
        .map_err(|error| format!("导出失败：{error}"))?;
    if result["ok"] == true {
        Ok(())
    } else {
        Err(format!(
            "导出失败：{}",
            result["error"].as_str().unwrap_or("无法生成下载文件")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(kind: &str) -> Value {
        json!({
            "schemaVersion":1,"kind":"rpi-work-design-package","id":"team.review",
            "name":"Review","version":"1.0.0","author":"Team","description":"Review resources",
            "resources":{"skills":[{"id":"review","name":"Review","description":"Description","content":"# Body"}],"tools":["read"],"prompts":[]},
            "design":{"resourceKind":kind,"sources":{"src/lib.rs":"fn check() {}\n"}},
            "testSnapshot":{"revision":3,"passed":true},"source":"Published source","extra":{"preserve":true}
        })
    }

    #[test]
    fn multi_package_storage_migrates_and_normalizes_without_losing_independent_state() {
        let legacy = json!({"session_id":"node/session","package_id":"a","package_version":"1","skills":["independent"],"disabled_tools":["read"],"disabled_prompts":["a@1/p"],"instructions":"keep me","revision":7});
        let migrated = session_from_value(&legacy).unwrap();
        assert_eq!(
            migrated.package_pins,
            vec![PackagePin {
                id: "a".into(),
                version: "1".into()
            }]
        );
        let mut stored = session_value(&migrated);
        assert!(stored.get("package_id").is_none());
        stored["package_pins"] = json!([{"id":"a","version":"1"},{"id":"b","version":"4"},null,{"id":"a","version":"2"},{"id":"b","version":5},{"id":"","version":"9"}]);
        let normalized = session_from_value(&stored).unwrap();
        assert_eq!(
            normalized.package_pins,
            vec![
                PackagePin {
                    id: "a".into(),
                    version: "2".into()
                },
                PackagePin {
                    id: "b".into(),
                    version: "4".into()
                }
            ]
        );
        assert_eq!(normalized.skills, migrated.skills);
        assert_eq!(normalized.disabled_tools, migrated.disabled_tools);
        assert_eq!(normalized.disabled_prompts, migrated.disabled_prompts);
        assert_eq!(normalized.instructions, migrated.instructions);
        assert_eq!(normalized.revision, 7);
        assert_eq!(
            session_from_value(&session_value(&normalized)).unwrap(),
            normalized
        );
        stored["package_pins"] = json!([]);
        assert_eq!(
            session_from_value(&stored).unwrap().package_pins,
            Vec::<PackagePin>::new()
        );
    }

    #[test]
    fn effective_skills_deduplicate_and_disable_only_one_package_key() {
        let mut first = manifest("Package");
        first["id"] = json!("owner.a");
        let mut second = manifest("Package");
        second["id"] = json!("owner.b");
        let mut library = ResourceLibrary::default();
        library.skills.clear();
        library.packages.clear();
        import_design_package(&mut library, &first).unwrap();
        import_design_package(&mut library, &second).unwrap();
        for package in &mut library.packages {
            package.acquired = true;
        }
        let binding = library.session_mut("node/session").clone();
        let mut binding = SessionBinding {
            session_id: "node/session".into(),
            package_pins: library
                .packages
                .iter()
                .map(|package| PackagePin {
                    id: package.id.clone(),
                    version: package.version.clone(),
                })
                .collect(),
            ..binding
        };
        let all = effective_skills(&library, &binding);
        assert_eq!(all.len(), 2);
        assert!(all.iter().all(|skill| skill.id.ends_with("/review")));
        binding.disabled_skills.push("owner.a@1.0.0/review".into());
        let remaining = effective_skills(&library, &binding);
        assert_eq!(
            remaining
                .iter()
                .map(|skill| skill.id.as_str())
                .collect::<Vec<_>>(),
            vec!["owner.b@1.0.0/review"]
        );
        let mut independent = library.skills[0].clone();
        independent.id = "independent".into();
        independent.acquired = true;
        library.skills.push(independent);
        binding.skills.push("independent".into());
        binding.skills.push("owner.a@1.0.0/review".into());
        assert_eq!(effective_skills(&library, &binding).len(), 2);
        let mut newer = first;
        newer["version"] = json!("2.0.0");
        import_design_package(&mut library, &newer).unwrap();
        set_package_selected(&mut binding, "owner.a", "2.0.0", true);
        assert_eq!(effective_skills(&library, &binding).len(), 4);
        assert!(binding.package_pins.iter().any(|pin| pin.id == "owner.b"));
        assert!(
            effective_skills(&library, &binding)
                .iter()
                .any(|skill| skill.id == "owner.a@2.0.0/review")
        );
        let restored = session_from_value(&session_value(&binding)).unwrap();
        assert_eq!(restored.disabled_skills, binding.disabled_skills);
        assert_eq!(restored.skills, binding.skills);
        let old =
            session_from_value(&json!({"session_id":"old","skills":["independent"]})).unwrap();
        assert_eq!(old.disabled_skills, Vec::<String>::new());
    }

    #[test]
    fn complete_manifest_survives_storage_export_and_reimport() {
        for kind in [
            "Package",
            "Skill",
            "Extension",
            "package",
            "sKiLl",
            "EXTENSION",
        ] {
            let original = manifest(kind);
            let mut library = ResourceLibrary::default();
            let key = import_design_package(&mut library, &original).unwrap();
            let mut restored = ResourceLibrary::from_value(&library.to_value()).unwrap();
            let package = restored
                .packages
                .iter()
                .find(|item| package_key(item) == key)
                .unwrap();
            assert_eq!(package_manifest_value(package), original);
            assert_eq!(package.skill_contents[0].description, "Description");
            assert_eq!(package.skill_contents[0].content, "# Body");
            let before = restored.to_value();
            assert_eq!(
                import_design_package(&mut restored, &original).unwrap(),
                key
            );
            assert_eq!(restored.to_value(), before);
            if kind.eq_ignore_ascii_case("extension") {
                assert!(
                    restored
                        .extensions
                        .iter()
                        .any(|item| item.id == key && item.content.contains("fn check() {}\n"))
                );
            }
        }
    }

    #[test]
    fn immutable_version_compares_sources_snapshot_and_unknown_fields() {
        let original = manifest("Extension");
        let mut library = ResourceLibrary::default();
        import_design_package(&mut library, &original).unwrap();
        let mut library = ResourceLibrary::from_value(&library.to_value()).unwrap();
        let before = library.to_value();
        for pointer in [
            "/design/sources/src~1lib.rs",
            "/testSnapshot/passed",
            "/extra/preserve",
        ] {
            let mut changed = original.clone();
            *changed.pointer_mut(pointer).unwrap() = json!("changed");
            assert!(import_design_package(&mut library, &changed).is_err());
            assert_eq!(library.to_value(), before);
        }
    }

    #[test]
    fn skills_and_session_pins_are_isolated_by_owner_and_version() {
        let original = manifest("Skill");
        let mut library = ResourceLibrary::default();
        import_design_package(&mut library, &original).unwrap();
        library.sessions.push(SessionBinding {
            session_id: "session-a".into(),
            package_pins: vec![PackagePin {
                id: "team.review".into(),
                version: "1.0.0".into(),
            }],
            skills: vec!["team.review@1.0.0/review".into()],
            ..SessionBinding::default()
        });
        let pinned = library.sessions.clone();
        let mut newer = original.clone();
        newer["version"] = json!("1.1.0");
        newer["resources"]["skills"][0]["content"] = json!("New version");
        import_design_package(&mut library, &newer).unwrap();
        let mut other = original;
        other["id"] = json!("other.owner");
        other["design"]["resourceKind"] = json!("Package");
        other["resources"]["skills"][0]["content"] = json!("Other owner");
        import_design_package(&mut library, &other).unwrap();
        let restored = ResourceLibrary::from_value(&library.to_value()).unwrap();
        assert_eq!(restored.sessions, pinned);
        for (id, body) in [
            ("team.review@1.0.0/review", "# Body"),
            ("team.review@1.1.0/review", "New version"),
            ("other.owner@1.0.0/review", "Other owner"),
        ] {
            let skill = restored.skills.iter().find(|item| item.id == id).unwrap();
            assert_eq!(skill.content, body);
            assert!(id.starts_with(&format!("{}/", skill.source)));
        }
    }

    #[test]
    fn conflicting_derived_resource_rejects_import_atomically() {
        let mut library = ResourceLibrary::default();
        let mut collision = library.skills[0].clone();
        collision.id = "team.review@1.0.0/review".into();
        collision.content = "Existing source".into();
        library.skills.push(collision);
        let before = library.to_value();
        assert!(import_design_package(&mut library, &manifest("Package")).is_err());
        assert_eq!(library.to_value(), before);
    }

    #[test]
    fn persisted_booleans_cannot_attest_node_installation() {
        let mut saved = ResourceLibrary::default().to_value();
        saved["packages"][0]["node_installed"] = json!(true);
        saved["skills"][0]["node_installed"] = json!(true);
        saved["extensions"][0]["node_installed"] = json!(true);
        let restored = ResourceLibrary::from_value(&saved).unwrap();
        assert!(restored.packages.iter().all(|item| !item.node_installed));
        assert!(restored.skills.iter().all(|item| !item.node_installed));
        assert!(restored.extensions.iter().all(|item| !item.node_installed));
    }
}
