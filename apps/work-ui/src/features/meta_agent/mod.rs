use dioxus::prelude::*;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};

pub const CSS: &str = include_str!("meta_agent.css");
static NEXT_NOTICE_INSTANCE: AtomicU64 = AtomicU64::new(1);
const MAX_BYTES: usize = 200 * 1024;
const NOTICE_DURATION_MS: u32 = 3300;

fn notice_timer_cancel_script(key: u64) -> String {
    let key = serde_json::to_string(&format!("rpi-meta-notice-{key}"))
        .unwrap_or_else(|_| "\"\"".to_owned());
    format!(
        "try{{const timers=window.__rpiMetaNoticeTimers;const timer=timers?.get({key});if(timer){{clearTimeout(timer.handle);timers.delete({key});timer.resolve(false);}}}}catch(_){{}}"
    )
}

fn sorted_source_paths(sources: &BTreeMap<String, String>) -> Vec<String> {
    let mut paths: Vec<String> = sources.keys().cloned().collect();
    paths.sort_by_key(|path| {
        let preferred = if path == "manifest.json" || path == "package.json" {
            0
        } else if path.ends_with("/SKILL.md") || path == "SKILL.md" {
            1
        } else if path == "extension.design.json" {
            2
        } else {
            3
        };
        (preferred, path.clone())
    });
    paths
}

fn set_meta_notice(
    mut notice: Signal<String>,
    mut notice_error: Signal<bool>,
    mut notice_epoch: Signal<u64>,
    mut notice_task: Signal<Option<dioxus::core::Task>>,
    timer_key: u64,
    message: impl Into<String>,
    is_error: bool,
) {
    if let Some(task) = notice_task.take() {
        task.cancel();
    }
    let cancel_script = notice_timer_cancel_script(timer_key);
    let _ = document::eval(&cancel_script);
    let next_epoch = notice_epoch() + 1;
    notice_epoch.set(next_epoch);
    notice_error.set(is_error);
    notice.set(message.into());
    if is_error {
        return;
    }
    let mut notice = notice;
    let epoch = notice_epoch;
    let key_literal = serde_json::to_string(&format!("rpi-meta-notice-{timer_key}"))
        .unwrap_or_else(|_| "\"\"".to_owned());
    let script = format!(
        "(()=>{{const timers=window.__rpiMetaNoticeTimers||(window.__rpiMetaNoticeTimers=new Map());const key={key_literal};const previous=timers.get(key);if(previous){{clearTimeout(previous.handle);timers.delete(key);previous.resolve(false);}}return new Promise(resolve=>{{const timer={{handle:0,resolve}};timer.handle=setTimeout(()=>{{timers.delete(key);resolve(true);}}, {NOTICE_DURATION_MS});timers.set(key,timer);}});}})().then(expired=>{{if(expired)dioxus.send(true);}});"
    );
    let task = spawn(async move {
        let _ = document::eval(&script).recv::<bool>().await;
        if *epoch.read() == next_epoch {
            notice.set(String::new());
        }
    });
    notice_task.set(Some(task));
}

#[cfg(any(target_arch = "wasm32", target_os = "android"))]
const STORAGE_KEY: &str = "rpi-work-meta-agent-v1";

#[derive(Clone, Debug, PartialEq)]
pub struct MetaCheck {
    pub name: String,
    pub ok: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MetaValidation {
    pub revision: u64,
    pub ok: bool,
    pub duration_ms: Option<u128>,
    pub checks: Vec<MetaCheck>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MetaSnapshot {
    pub revision: u64,
    pub version: String,
    pub sources: BTreeMap<String, String>,
    pub passed: bool,
    pub duration_ms: Option<u128>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MetaRelease {
    pub version: String,
    pub revision: u64,
    pub sources: BTreeMap<String, String>,
    pub test: MetaSnapshot,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MetaResource {
    pub project_id: String,
    pub id: String,
    pub name: String,
    pub kind: String,
    pub version: String,
    pub revision: u64,
    pub sources: BTreeMap<String, String>,
    pub baseline: BTreeMap<String, String>,
    pub validation: Option<MetaValidation>,
    pub tests: Vec<MetaSnapshot>,
    pub releases: Vec<MetaRelease>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MetaStore {
    pub resources: Vec<MetaResource>,
    pub selected_by_project: BTreeMap<String, String>,
}

fn map_string(value: &Value) -> BTreeMap<String, String> {
    value
        .as_object()
        .map(|object| {
            object
                .iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_owned())))
                .collect()
        })
        .unwrap_or_default()
}
fn source_json(sources: &BTreeMap<String, String>) -> Value {
    Value::Object(
        sources
            .iter()
            .map(|(k, v)| (k.clone(), Value::String(v.clone())))
            .collect(),
    )
}
impl MetaStore {
    fn from_json(value: &Value) -> Self {
        let Some(items) = value.get("resources").and_then(Value::as_array) else {
            return Self::default();
        };
        let resources = items
            .iter()
            .map(|item| {
                let get = |key: &str| {
                    item.get(key)
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned()
                };
                let sources = map_string(&item["sources"]);
                let baseline = map_string(&item["baseline"]);
                let tests = item["tests"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|v| MetaSnapshot {
                        revision: v["revision"].as_u64().unwrap_or_default(),
                        version: v["version"].as_str().unwrap_or_default().into(),
                        sources: map_string(&v["sources"]),
                        passed: v["passed"].as_bool().unwrap_or(false),
                        duration_ms: v["durationMs"].as_u64().map(u128::from),
                    })
                    .collect();
                let releases = item["releases"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|v| {
                        let snap = &v["test"];
                        MetaRelease {
                            version: v["version"].as_str().unwrap_or_default().into(),
                            revision: v["revision"].as_u64().unwrap_or_default(),
                            sources: map_string(&v["sources"]),
                            test: MetaSnapshot {
                                revision: snap["revision"].as_u64().unwrap_or_default(),
                                version: snap["version"].as_str().unwrap_or_default().into(),
                                sources: map_string(&snap["sources"]),
                                passed: snap["passed"].as_bool().unwrap_or(false),
                                duration_ms: snap["durationMs"].as_u64().map(u128::from),
                            },
                        }
                    })
                    .collect();
                MetaResource {
                    project_id: get("projectId"),
                    id: get("id"),
                    name: get("name"),
                    kind: get("kind"),
                    version: get("version"),
                    revision: item["revision"].as_u64().unwrap_or(1),
                    sources,
                    baseline,
                    validation: None,
                    tests,
                    releases,
                }
            })
            .collect();
        let selected_by_project = value["selected"]
            .as_object()
            .map(|m| {
                m.iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.into())))
                    .collect()
            })
            .unwrap_or_default();
        Self {
            resources,
            selected_by_project,
        }
    }
    fn to_json(&self) -> Value {
        let resources: Vec<Value> = self.resources.iter().map(|r| {
            let tests: Vec<Value> = r.tests.iter().map(snapshot_json).collect();
            let releases: Vec<Value> = r.releases.iter().map(|rel| json!({"version":rel.version,"revision":rel.revision,"sources":source_json(&rel.sources),"test":snapshot_json(&rel.test)})).collect();
            json!({"projectId":r.project_id,"id":r.id,"name":r.name,"kind":r.kind,"version":r.version,"revision":r.revision,"sources":source_json(&r.sources),"baseline":source_json(&r.baseline),"tests":tests,"releases":releases})
        }).collect();
        json!({"resources":resources,"selected":self.selected_by_project})
    }
}
fn snapshot_json(s: &MetaSnapshot) -> Value {
    json!({"revision":s.revision,"version":s.version,"sources":source_json(&s.sources),"passed":s.passed,"durationMs":s.duration_ms})
}

fn duration_text(duration_ms: Option<u128>) -> String {
    duration_ms.map_or_else(|| "耗时未知".into(), |duration| format!("{duration} ms"))
}

async fn browser_now_ms() -> Option<f64> {
    document::eval("try { dioxus.send(performance.now()); } catch (_) { dioxus.send(null); }")
        .recv::<Value>()
        .await
        .ok()
        .and_then(|value| value.as_f64())
}

// The caller validates ordered finite samples; handle overflow before truncating milliseconds.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn elapsed_millis(start: f64, finish: f64) -> u128 {
    let elapsed = finish - start;
    if !elapsed.is_finite() || elapsed >= 2_f64.powi(128) {
        return u128::MAX;
    }
    debug_assert!(elapsed >= 0.0);
    elapsed as u128
}

async fn validate_with_duration(resource: MetaResource) -> (bool, Vec<MetaCheck>, Option<u128>) {
    let started = browser_now_ms().await;
    let (ok, checks) = validate(&resource);
    let finished = browser_now_ms().await;
    let duration_ms = match (started, finished) {
        (Some(start), Some(finish))
            if start.is_finite() && finish.is_finite() && finish >= start =>
        {
            Some(elapsed_millis(start, finish))
        }
        _ => None,
    };
    (ok, checks, duration_ms)
}

#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
fn data_path() -> Option<std::path::PathBuf> {
    #[cfg(target_os = "windows")]
    let root = std::env::var_os("LOCALAPPDATA").map(std::path::PathBuf::from);
    #[cfg(target_os = "macos")]
    let root = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .map(|p| p.join("Library/Application Support"));
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    let root = std::env::var_os("XDG_DATA_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .map(std::path::PathBuf::from)
                .map(|p| p.join(".local/share"))
        });
    root.map(|p| p.join("rpi-work").join("meta-agent.json"))
}
fn load_store() -> MetaStore {
    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    if let Some(store) = data_path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .as_ref()
        .map(MetaStore::from_json)
    {
        return store;
    }
    MetaStore::default()
}

pub fn use_meta_store() -> Signal<MetaStore> {
    let store = use_signal(load_store);
    #[cfg(any(target_arch = "wasm32", target_os = "android"))]
    {
        let mut ready = use_signal(|| false);
        use_effect(move || {
            let mut store = store;
            spawn(async move {
                if let Ok(value) = document::eval(&format!("try {{ dioxus.send(JSON.parse(localStorage.getItem('{STORAGE_KEY}') || 'null')); }} catch (_) {{ dioxus.send(null); }}")).recv::<Value>().await {
                    if !value.is_null() { store.set(MetaStore::from_json(&value)); }
                }
                ready.set(true);
            });
        });
        use_effect(move || {
            let serialized = store.read().to_json().to_string();
            if *ready.read() {
                spawn(async move {
                    let script = format!(
                        "try {{ localStorage.setItem('{STORAGE_KEY}', {serialized:?}); }} catch (_) {{}} dioxus.send(true);"
                    );
                    let _ = document::eval(&script).recv::<bool>().await;
                });
            }
        });
    }
    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    use_effect(move || {
        let serialized = store.read().to_json().to_string();
        if let Some(path) = data_path() {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(path, serialized);
        }
    });
    store
}

fn slug(id: &str) -> &str {
    id.rsplit('.').next().unwrap_or(id)
}
fn templates(kind: &str, id: &str, name: &str, version: &str) -> BTreeMap<String, String> {
    let key = slug(id);
    let skill = format!(
        "---\nname: {key}\ndescription: Review an explicit scope and report evidence.\n---\n\n# {name}\n\n先确认任务范围，再检查项目资料。输出结论、文件依据和待确认项。\n"
    );
    let mut files = BTreeMap::new();
    match kind {
        "Skill" => {
            files.insert(format!("skills/{key}/SKILL.md"), skill);
            files.insert(
                format!("skills/{key}/examples.md"),
                "# 试用问题\n\n审阅一个包含错误处理变更的任务，给出可定位的依据。\n".into(),
            );
        }
        "Extension" => {
            files.insert("extension.design.json".into(), serde_json::to_string_pretty(&json!({"kind":"rpi-work-extension-design","id":id,"name":name,"version":version,"entry":"src/lib.rs","description":"设计草稿，尚未适配原生扩展宿主。"})).unwrap_or_default());
            files.insert("src/lib.rs".into(), "// 设计源码草稿；尚未注册到原生宿主。\npub fn is_relative_project_path(path: &str) -> bool {\n    !path.is_empty() && !path.starts_with('/') && !path.contains(\"..\")\n}\n".into());
            files.insert("tests/cases.md".into(), "# 本地结构测试场景\n\n- 接受 src/lib.rs\n- 拒绝 ../secrets\n- 不代表 Rust 编译或运行测试\n".into());
        }
        _ => {
            files.insert("package.json".into(), serde_json::to_string_pretty(&json!({"schemaVersion":1,"kind":"rpi-work-design-package","id":id,"name":name,"version":version,"author":"个人","description":"组合技能、已知工具声明与提示词的设计样例。","resources":{"skills":[{"id":key,"name":key,"description":"审阅明确范围并提供文件依据。","content":skill}],"tools":["read","grep"],"prompts":[{"id":"review","name":"审阅任务","kind":"template","content":"请确认任务范围，再审阅相关文件，给出结论与依据。"}]}})).unwrap_or_default());
            files.insert(
                "README.md".into(),
                format!("# {name}\n\n个人资源包设计草稿。工具声明不代表节点已安装或可用。\n"),
            );
            files.insert("tests/cases.md".into(), "# 本地结构测试场景\n\n- Package 清单符合 schemaVersion 1\n- 工具声明均为已知工具\n- 模型行为仍需在真实宿主验证\n".into());
        }
    }
    files
}
fn valid_id(id: &str) -> bool {
    (3..=80).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.' || b == b'-')
        && id.as_bytes()[0].is_ascii_lowercase()
}
fn validate(resource: &MetaResource) -> (bool, Vec<MetaCheck>) {
    let total: usize = resource.sources.values().map(String::len).sum();
    let all_files = !resource.sources.is_empty()
        && resource
            .sources
            .values()
            .all(|s| !s.trim().is_empty() && s.len() <= MAX_BYTES)
        && total <= MAX_BYTES;
    let mut checks = vec![MetaCheck {
        name: format!("源码文件非空，合计不超过 200 KiB（{total} 字节）"),
        ok: all_files,
    }];
    let has_tests = resource
        .sources
        .keys()
        .any(|p| p.contains("tests") || p.contains("examples"));
    validate_kind(resource, has_tests, &mut checks);
    checks.push(MetaCheck {
        name: "包含 tests/ 或 examples/ 说明".into(),
        ok: has_tests,
    });
    let ok = checks.iter().all(|c| c.ok);
    (ok, checks)
}
fn validate_kind(resource: &MetaResource, has_tests: bool, checks: &mut Vec<MetaCheck>) {
    match resource.kind.as_str() {
        "Package" => {
            let parsed = resource
                .sources
                .get("package.json")
                .and_then(|s| serde_json::from_str::<Value>(s).ok());
            let schema = parsed.as_ref().is_some_and(|v| {
                v["schemaVersion"] == 1
                    && v["kind"] == "rpi-work-design-package"
                    && v["id"] == resource.id
                    && v["name"].as_str().is_some()
                    && v["version"] == resource.version
                    && v["resources"].is_object()
            });
            let tools_ok = parsed
                .as_ref()
                .and_then(|v| v["resources"]["tools"].as_array())
                .is_some_and(|tools| {
                    tools.iter().all(|t| {
                        matches!(
                            t.as_str(),
                            Some("read" | "grep" | "edit" | "ask_user" | "todo")
                        )
                    })
                });
            let references = parsed
                .as_ref()
                .and_then(|v| v["resources"]["skills"].as_array())
                .is_some_and(|skills| {
                    skills.iter().all(|s| {
                        s["id"].as_str().is_some()
                            && s["content"].as_str().is_some_and(|c| !c.trim().is_empty())
                    })
                });
            checks.push(MetaCheck {
                name: "Package schema、ID、版本与资源结构".into(),
                ok: schema,
            });
            checks.push(MetaCheck {
                name: "工具声明均在已知工具目录内".into(),
                ok: tools_ok,
            });
            checks.push(MetaCheck {
                name: "技能引用含 ID 与正文".into(),
                ok: references,
            });
        }
        "Skill" => {
            let body = resource
                .sources
                .iter()
                .find(|(p, _)| p.ends_with("SKILL.md"))
                .map_or("", |(_, s)| s.as_str());
            let valid = body.starts_with("---\n")
                && body.contains("\nname:")
                && body.contains("\ndescription:")
                && body.contains("\n---\n")
                && body
                    .split("\n---\n")
                    .nth(1)
                    .is_some_and(|s| !s.trim().is_empty());
            checks.push(MetaCheck {
                name: "SKILL.md frontmatter 与正文".into(),
                ok: valid,
            });
            checks.push(MetaCheck {
                name: "引用示例文件存在".into(),
                ok: resource.sources.keys().any(|p| p.contains("examples")),
            });
        }
        _ => {
            let manifest = resource
                .sources
                .get("extension.design.json")
                .and_then(|s| serde_json::from_str::<Value>(s).ok());
            let entry = manifest.as_ref().and_then(|v| v["entry"].as_str());
            let valid = manifest.as_ref().is_some_and(|v| {
                v["kind"] == "rpi-work-extension-design"
                    && v["id"] == resource.id
                    && v["version"] == resource.version
                    && entry.is_some_and(|e| resource.sources.contains_key(e))
            });
            checks.push(MetaCheck {
                name: "扩展设计清单与源码入口引用".into(),
                ok: valid,
            });
            checks.push(MetaCheck {
                name: "测试场景文件存在".into(),
                ok: has_tests,
            });
        }
    }
}
fn validation_status(ok: bool) -> &'static str {
    if ok { "通过" } else { "未通过" }
}

fn same_resource_snapshot(
    current: &MetaResource,
    project_id: &str,
    snapshot: &MetaResource,
) -> bool {
    current.project_id == project_id
        && current.project_id == snapshot.project_id
        && current.id == snapshot.id
        && current.revision == snapshot.revision
        && current.version == snapshot.version
        && current.sources == snapshot.sources
}

fn check_status(ok: bool) -> &'static str {
    if ok { "通过" } else { "失败" }
}

fn snapshot_status(passed: bool) -> &'static str {
    if passed {
        "结构检查通过"
    } else {
        "失败"
    }
}

fn resource_status(resource: &MetaResource) -> &'static str {
    if resource
        .releases
        .iter()
        .any(|release| release.revision == resource.revision)
    {
        "已发布"
    } else if resource
        .validation
        .as_ref()
        .is_some_and(|validation| validation.revision == resource.revision && validation.ok)
    {
        "已校验"
    } else {
        "草稿"
    }
}

fn design_package(resource: &MetaResource) -> Value {
    let manifest = resource
        .sources
        .get("package.json")
        .and_then(|text| serde_json::from_str::<Value>(text).ok());
    let skills = if resource.kind == "Package" {
        manifest
            .as_ref()
            .and_then(|v| v["resources"]["skills"].as_array().cloned())
            .unwrap_or_default()
    } else if resource.kind == "Skill" {
        resource.sources.iter().find(|(path, _)| path.ends_with("SKILL.md")).map(|(path, content)| vec![json!({"id":slug(path.trim_end_matches("/SKILL.md")),"name":resource.name,"description":"个人 Skill 设计资源。","content":content})]).unwrap_or_default()
    } else {
        Vec::new()
    };
    let tools = manifest
        .as_ref()
        .and_then(|v| v["resources"]["tools"].as_array().cloned())
        .unwrap_or_default();
    let prompts = manifest
        .as_ref()
        .and_then(|v| v["resources"]["prompts"].as_array().cloned())
        .unwrap_or_default();
    json!({"schemaVersion":1,"kind":"rpi-work-design-package","id":resource.id,"name":resource.name,"version":resource.version,"author":"个人","description":format!("{} 个人设计资源快照 r{}；不代表已在节点安装或运行。",resource.kind,resource.revision),"resources":{"skills":skills,"tools":tools,"prompts":prompts},"design":{"resourceKind":resource.kind,"revision":resource.revision,"sources":source_json(&resource.sources)}})
}

#[component]
pub fn MetaAgentPage(
    project_id: String,
    project_name: String,
    store: Signal<MetaStore>,
    ondevelop: EventHandler<MetaResource>,
    onpublish: Callback<Value, Result<String, String>>,
) -> Element {
    let mut filter = use_signal(String::new);
    let mut kind_filter = use_signal(|| "全部类型".to_owned());
    let mut selected = use_signal(|| None::<String>);
    let mut editing = use_signal(|| false);
    // The editor is bound to the resource snapshot it opened, not to the current selection.
    let mut editor_owner = use_signal(|| None::<(String, String, u64)>);
    let mut tab = use_signal(|| "drafts".to_owned());
    let mut new_open = use_signal(|| false);
    let mut new_kind = use_signal(|| "Package".to_owned());
    let mut new_name = use_signal(String::new);
    let mut new_id = use_signal(String::new);
    let mut edit_files = use_signal(BTreeMap::<String, String>::new);
    let mut active_file = use_signal(String::new);
    let mut notice = use_signal(String::new);
    let mut notice_error = use_signal(|| false);
    let notice_epoch = use_signal(|| 0_u64);
    let notice_task = use_signal(|| None::<dioxus::core::Task>);
    let notice_instance = use_hook(|| NEXT_NOTICE_INSTANCE.fetch_add(1, Ordering::Relaxed));
    let notice_timer_key = notice_instance;
    let mut scope_notice_task = notice_task;
    let mut scope_notice_epoch = notice_epoch;
    let scope_timer_key = notice_timer_key;
    use_effect(use_reactive((&project_id,), move |_| {
        if let Some(task) = scope_notice_task.write().take() {
            task.cancel();
        }
        scope_notice_epoch.with_mut(|epoch| *epoch += 1);
        let script = notice_timer_cancel_script(scope_timer_key);
        let _ = document::eval(&script);
        notice.set(String::new());
        notice_error.set(false);
    }));
    let mut drop_notice_task = notice_task;
    let drop_timer_key = notice_timer_key;
    use_drop(move || {
        if let Some(task) = drop_notice_task.write().take() {
            task.cancel();
        }
        let script = notice_timer_cancel_script(drop_timer_key);
        let _ = document::eval(&script);
    });
    let mut new_error = use_signal(String::new);
    let mut editor_error = use_signal(String::new);
    let mut show_diff = use_signal(|| false);
    let resources: Vec<MetaResource> = store
        .read()
        .resources
        .iter()
        .filter(|r| r.project_id == project_id)
        .cloned()
        .collect();
    let selected_id = selected
        .read()
        .clone()
        .or_else(|| store.read().selected_by_project.get(&project_id).cloned());
    let selected_resource = selected_id
        .as_ref()
        .and_then(|id| resources.iter().find(|r| &r.id == id))
        .cloned()
        .or_else(|| resources.first().cloned());
    let shown: Vec<MetaResource> = resources
        .iter()
        .filter(|r| {
            (kind_filter.read().as_str() == "全部类型" || r.kind == *kind_filter.read())
                && (filter.read().is_empty()
                    || r.name
                        .to_lowercase()
                        .contains(&filter.read().to_lowercase())
                    || r.id.contains(&*filter.read()))
        })
        .cloned()
        .collect();
    let release_total: usize = resources.iter().map(|r| r.releases.len()).sum();
    let tests_total: usize = resources.iter().map(|r| r.tests.len()).sum();
    let editor_resource =
        editor_owner
            .read()
            .as_ref()
            .and_then(|(owner_project, owner_id, owner_revision)| {
                resources.iter().find(|resource| {
                    resource.project_id == *owner_project
                        && resource.id == *owner_id
                        && resource.revision == *owner_revision
                })
            });
    let diff_files: Vec<(String, String, String)> = editor_resource
        .or(selected_resource.as_ref())
        .map(|resource| {
            resource
                .sources
                .iter()
                .filter_map(|(path, source)| {
                    edit_files.read().get(path).and_then(|current| {
                        let baseline = resource.baseline.get(path).unwrap_or(source);
                        (current != baseline)
                            .then(|| (path.clone(), baseline.clone(), current.clone()))
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let editor_paths = sorted_source_paths(&edit_files.read());
    let editor_path = editor_paths
        .iter()
        .find(|p| **p == *active_file.read())
        .cloned()
        .or_else(|| editor_paths.first().cloned());
    let editor_snapshot = editor_owner.read().clone();
    let editor_save_project = editor_snapshot
        .as_ref()
        .map(|(project, _, _)| project.clone());
    let editor_save_id = editor_snapshot.as_ref().map(|(_, id, _)| id.clone());
    let editor_save_revision = editor_snapshot.as_ref().map(|(_, _, revision)| *revision);
    rsx! {
    style { {CSS} }
    main { class: "meta-page", "data-project": "{project_id}",
        header { class: "meta-heading",
                div { span { class: "meta-overline", "META AGENT / PERSONAL" } h1 { "元Agent" } p { "{project_name} · 个人能力源码、结构校验与版本" } }
                button { class: "meta-button primary", onclick: move |_| { new_open.set(true); new_name.set(String::new()); new_id.set(String::new()); new_error.set(String::new()); }, "+ 新建能力" }
            }
            div { class: "meta-overview", span { "{resources.len()} 个源码资源" } span { "{tests_total} 个测试快照" } span { "{release_total} 个个人版本" } }
            nav { class: "meta-tabs", "aria-label": "元Agent视图",
                button { aria_pressed: *tab.read() == "drafts", onclick: move |_| tab.set("drafts".into()), "开发中" }
                button { aria_pressed: *tab.read() == "library", onclick: move |_| tab.set("library".into()), "个人资源库 · {release_total}" }
                if *tab.read() == "drafts" {
                    details { class: "meta-filter-toggle",
                        summary { if filter.read().is_empty() && *kind_filter.read() == "全部类型" { "筛选" } else { "筛选 · 已应用" } }
                        div { class: "meta-filters",
                            input { aria_label: "筛选资源", placeholder: "筛选名称或资源 ID", value: "{filter}", oninput: move |e| filter.set(e.value()) }
                            select { aria_label: "按类型筛选", value: "{kind_filter}", onchange: move |e| kind_filter.set(e.value()), option { "全部类型" } option { "Package" } option { "Skill" } option { "Extension" } }
                        }
                    }
                }
            }
            if *tab.read() == "drafts" {
                div { class: "meta-layout",
                    aside { class: "meta-resource-list", aria_label: "源码资源列表",
                        for resource in shown.iter() {
                            button { class: if selected_resource.as_ref().is_some_and(|r| r.id == resource.id) { "meta-resource selected" } else { "meta-resource" }, aria_current: selected_resource.as_ref().is_some_and(|r| r.id == resource.id), onclick: {
                                let id = resource.id.clone();
                                let project_id = project_id.clone();
                                move |_| { selected.set(Some(id.clone())); store.write().selected_by_project.insert(project_id.clone(), id.clone()); }
                            },
                                strong { "{resource.name}" } span { class: "meta-kind", "{resource.kind}" } code { "{resource.id}" }
                                small { "{resource.version} · r{resource.revision} · {resource_status(resource)}" }
                            }
                        }
                        if shown.is_empty() { p { class: "meta-empty", "没有匹配的源码资源。" } }
                    }
                    section { class: "meta-detail",
                        if let Some(resource) = selected_resource {
                            header { class: "meta-resource-head", div { span { class: "meta-kind", "{resource.kind} · {resource_status(&resource)}" } h2 { "{resource.name}" } code { "{resource.id} @{resource.version} · r{resource.revision}" } } button { class: "meta-button", onclick: {
                                    let develop_resource = resource.clone();
                                    move |_| { ondevelop.call(develop_resource.clone()); }
                                }, "开发会话" } }
                            div { class: "meta-lifecycle", span { class: "done", "✓ 编写源码" } i { "→" } span { class: if resource.validation.as_ref().is_some_and(|v| v.revision == resource.revision && v.ok) { "done" } else { "" }, "结构校验" } i { "→" } span { class: if resource.tests.iter().any(|t| t.revision == resource.revision && t.passed) { "done" } else { "" }, "测试快照" } i { "→" } span { class: if resource.releases.iter().any(|r| r.revision == resource.revision) { "done" } else { "" }, "发布版本" } }
                            div { class: "meta-file-strip", div { strong { "源码与清单" } span { "{resource.sources.len()} 个文件 · 在源码编辑器中预览" } } button { class: "meta-button", onclick: {
                                    let edit_resource = resource.clone();
                                    move |_| {
                                        edit_files.set(edit_resource.sources.clone());
                                        let first = ["package.json", "SKILL.md", "extension.design.json"].iter().find_map(|preferred| edit_resource.sources.keys().find(|path| path.ends_with(preferred)).cloned()).or_else(|| edit_resource.sources.keys().next().cloned()).unwrap_or_default();
                                        active_file.set(first); editor_owner.set(Some((edit_resource.project_id.clone(), edit_resource.id.clone(), edit_resource.revision))); editor_error.set(String::new()); show_diff.set(false); editing.set(true);
                                    }
                                }, "编辑源码" } div { class: "meta-source-paths", for path in sorted_source_paths(&resource.sources) { code { "{path}" } } } }
                            div { class: "meta-actions",
                                button { class: "meta-button", onclick: {
                                    let resource_snapshot = resource.clone();
                                    let project_id_snapshot = project_id.clone();
                                    move |_| {
                                        let resource_snapshot = resource_snapshot.clone();
                                        let project_id_snapshot = project_id_snapshot.clone();
                                        let mut store = store;
                                        spawn(async move {
                                            let (ok, checks, duration_ms) = validate_with_duration(resource_snapshot.clone()).await;
                                            let mut data = store.write();
                                            if let Some(current) = data.resources.iter_mut().find(|current| same_resource_snapshot(current, &project_id_snapshot, &resource_snapshot)) {
                                                current.validation = Some(MetaValidation { revision: resource_snapshot.revision, ok, duration_ms, checks });
                                            }
                                        });
                                    }
                                }, "结构校验" }
                                button { class: "meta-button", disabled: !resource.validation.as_ref().is_some_and(|v| v.revision == resource.revision && v.ok), onclick: {
                                    let resource_snapshot = resource.clone();
                                    let project_id_snapshot = project_id.clone();
                                    move |_| {
                                        let can_test = store.read().resources.iter().any(|current| {
                                            same_resource_snapshot(current, &project_id_snapshot, &resource_snapshot)
                                                && current.validation.as_ref().is_some_and(|v| v.revision == current.revision && v.ok)
                                                && !current.tests.iter().any(|test| test.revision == current.revision && test.version == current.version && test.sources == current.sources && test.passed)
                                        });
                                        if !can_test {
                                            set_meta_notice(notice, notice_error, notice_epoch, notice_task, notice_timer_key, "当前 revision 已有通过的测试快照，或源码已变化；请确认当前校验状态。", true);
                                            return;
                                        }
                                        let resource_snapshot = resource_snapshot.clone();
                                        let project_id_snapshot = project_id_snapshot.clone();
                                        let mut store = store;
                                        let notice = notice;
                                        spawn(async move {
                                            let (ok, _, duration_ms) = validate_with_duration(resource_snapshot.clone()).await;
                                            if !ok {
                                                return;
                                            }
                                            let snapshot = MetaSnapshot { revision: resource_snapshot.revision, version: resource_snapshot.version.clone(), sources: resource_snapshot.sources.clone(), passed: true, duration_ms };
                                            let mut data = store.write();
                                            let applied = data.resources.iter_mut().find(|current| {
                                                same_resource_snapshot(current, &project_id_snapshot, &resource_snapshot)
                                                    && current.validation.as_ref().is_some_and(|v| v.revision == current.revision && v.ok)
                                                    && !current.tests.iter().any(|test| test.revision == current.revision && test.version == current.version && test.sources == current.sources && test.passed)
                                            }).map(|current| {
                                                current.tests.push(snapshot);
                                            }).is_some();
                                            drop(data);
                                            if applied {
                                                set_meta_notice(notice, notice_error, notice_epoch, notice_task, notice_timer_key, format!("已创建不可变 revision {} 结构测试快照（{}）", resource_snapshot.revision, duration_text(duration_ms)), false);
                                            }
                                        });
                                    }
                                }, "创建测试快照" }
                                button { class: "meta-button", disabled: !resource.validation.as_ref().is_some_and(|v| v.revision == resource.revision && v.ok) || !resource.tests.iter().any(|t| t.revision == resource.revision && t.passed) || resource.releases.iter().any(|v| v.version == resource.version), onclick: {
                                    let publish_resource = resource.clone();
                                    let publish_project_id = project_id.clone();
                                    move |_| {
                                        let current = store.read().resources.iter().find(|current| same_resource_snapshot(current, &publish_project_id, &publish_resource)).cloned();
                                        let Some(current) = current else {
                                            set_meta_notice(notice, notice_error, notice_epoch, notice_task, notice_timer_key, "源码已变化，请重新校验并创建测试快照后发布。", true);
                                            return;
                                        };
                                        if current.releases.iter().any(|release| release.version == current.version) {
                                            set_meta_notice(notice, notice_error, notice_epoch, notice_task, notice_timer_key, "该版本已有不可变发布快照，请继续迭代后再发布。", true);
                                            return;
                                        }
                                        if !current.validation.as_ref().is_some_and(|v| v.revision == current.revision && v.ok) {
                                            set_meta_notice(notice, notice_error, notice_epoch, notice_task, notice_timer_key, "当前 revision 尚未通过结构校验。", true);
                                            return;
                                        }
                                        let test = current.tests.iter().find(|test| test.revision == current.revision && test.version == current.version && test.sources == current.sources && test.passed).cloned();
                                        let Some(test) = test else {
                                            set_meta_notice(notice, notice_error, notice_epoch, notice_task, notice_timer_key, "请先为当前源码创建通过的测试快照。", true);
                                            return;
                                        };
                                        let mut package = design_package(&current);
                                        package["testSnapshot"] = snapshot_json(&test);
                                        match onpublish.call(package) {
                                            Ok(receipt) => {
                                                let release = MetaRelease { version: current.version.clone(), revision: current.revision, sources: current.sources.clone(), test };
                                                let mut data = store.write();
                                                let recorded = data.resources.iter_mut().find(|resource| {
                                                    same_resource_snapshot(resource, &publish_project_id, &current)
                                                        && !resource.releases.iter().any(|release| release.version == current.version)
                                                }).map(|resource| resource.releases.push(release)).is_some();
                                                drop(data);
                                                if recorded {
                                                    set_meta_notice(notice, notice_error, notice_epoch, notice_task, notice_timer_key, format!("个人设计版本 {} 已发布：{}", current.version, receipt), false);
                                                } else {
                                                    set_meta_notice(notice, notice_error, notice_epoch, notice_task, notice_timer_key, "资源库已接收设计快照；当前草稿已变化或该版本已登记，请检查版本记录。", true);
                                                }
                                            }
                                            Err(error) => set_meta_notice(notice, notice_error, notice_epoch, notice_task, notice_timer_key, format!("发布失败：{error}"), true),
                                        }
                                    }
                                }, "发布个人版本" }
                                if resource.releases.iter().any(|v| v.version == resource.version) {
                                    button { class: "meta-button", onclick: {
                                        let bump_project_id = project_id.clone();
                                        let bump_resource_id = resource.id.clone();
                                        move |_| {
                                            let mut data = store.write();
                                            if let Some(r) = data.resources.iter_mut().find(|r| r.project_id == bump_project_id && r.id == bump_resource_id) {
                                                r.baseline = r.sources.clone();
                                                let parts: Vec<u64> = r.version.split('.').filter_map(|n| n.parse().ok()).collect();
                                                let next = if parts.len() == 3 { format!("{}.{}.{}", parts[0], parts[1], parts[2] + 1) } else { "0.1.1".into() };
                                                r.version.clone_from(&next); r.revision += 1; r.validation = None;
                                                let path = if r.kind == "Package" { Some("package.json") } else if r.kind == "Extension" { Some("extension.design.json") } else { None };
                                                if let Some(path) = path { if let Some(text) = r.sources.get(path).cloned() { if let Ok(mut manifest) = serde_json::from_str::<Value>(&text) { manifest["version"] = Value::String(next); r.sources.insert(path.into(), serde_json::to_string_pretty(&manifest).unwrap_or(text)); } } }
                                            }
                                        }
                                    }, "继续迭代 · bump patch" }
                                }
                                button { class: "meta-button", onclick: {
                                    let export_resource = resource.clone();
                                    move |_| {
                                        let value = design_package(&export_resource);
                                        let encoded = value.to_string();
                                        spawn(async move { let script = format!("const blob=new Blob([{}],{{type:'application/json'}});const a=document.createElement('a');a.href=URL.createObjectURL(blob);a.download='design-package.json';a.click();URL.revokeObjectURL(a.href);dioxus.send(true);", serde_json::to_string(&encoded).unwrap_or_default()); let _ = document::eval(&script).recv::<bool>().await; });
                                    }
                                }, "导出 JSON" }
                            }
                            if let Some(validation) = resource.validation.as_ref().filter(|v| v.revision == resource.revision) {
                                div { class: "meta-checks", strong { "本地结构校验 · {duration_text(validation.duration_ms)} · {validation_status(validation.ok)}" }
                                    for check in validation.checks.iter() { p { class: if check.ok { "pass" } else { "fail" }, "{check_status(check.ok)} · {check.name}" } }
                                }
                            }
                            p { class: "meta-boundary", "测试只检查 schema、文件引用、已知工具声明和版本，并固定不可变源码快照；不编译 Rust、不执行源码、不验证模型行为。发布不会安装或启用节点插件。" }
                            section { class: "meta-history", h3 { "测试快照与发布版本" }
                                if resource.tests.is_empty() && resource.releases.is_empty() { p { class: "meta-empty", "尚未创建测试快照或发布版本。结构校验只检查设计结构；测试快照固定当前源码，发布还需要本地库确认。" } }
                                for snapshot in resource.tests.iter().rev() { div { "测试快照 · revision {snapshot.revision} · {snapshot.version} · {duration_text(snapshot.duration_ms)} · {snapshot.sources.len()} 个固定文件 · {snapshot_status(snapshot.passed)}" } }
                                for release in resource.releases.iter().rev() { div { "个人设计版本 {release.version} · revision {release.revision} · {release.sources.len()} 个固定源码文件 · 已收到本地库确认" } }
                            }
                        } else { p { class: "meta-empty", "该项目尚无个人源码资源。新建 Package、Skill 或 Extension 设计草稿。" } }
                    }
                }
            } else {
                section { class: "meta-library", h2 { "个人资源库" } p { "发布物是不可变设计快照。消费项目需要显式获取并启用；本页面不执行节点安装。" }
                    for resource in resources.iter().cloned() {
                        for release in resource.releases.iter().cloned() {
                            article { class: "meta-release",
                                span { class: "meta-kind", "{resource.kind} · 个人设计" }
                                h3 { "{resource.name}" }
                                code { "{resource.id} @ {release.version}" }
                                p { "来源项目 {project_name} · revision {release.revision} · 测试快照 r{release.test.revision}" }
                                button { class: "meta-button", onclick: {
                                    let export_resource = resource.clone();
                                    let export_release = release.clone();
                                    move |_| {
                                        let mut published = export_resource.clone();
                                        published.version.clone_from(&export_release.version);
                                        published.revision = export_release.revision;
                                        published.sources = export_release.sources.clone();
                                        let mut value = design_package(&published);
                                        value["testSnapshot"] = snapshot_json(&export_release.test);
                                        let encoded = value.to_string();
                                        spawn(async move {
                                            let script = format!("const blob=new Blob([{}],{{type:'application/json'}});const a=document.createElement('a');a.href=URL.createObjectURL(blob);a.download='design-package.json';a.click();URL.revokeObjectURL(a.href);dioxus.send(true);", serde_json::to_string(&encoded).unwrap_or_default());
                                            let _ = document::eval(&script).recv::<bool>().await;
                                        });
                                    }
                                }, "导出设计 JSON" }
                            }
                        }
                    }
                    if release_total == 0 { p { class: "meta-empty", "还没有发布版本。" } }
                }
            }
            if !notice.read().is_empty() { p { class: if *notice_error.read() { "meta-notice error" } else { "meta-notice" }, role: if *notice_error.read() { "alert" } else { "status" }, "{notice}" } }
            if *editing.read() {
                crate::shared::components::Modal { id: "meta-editor", title: "编写能力源码", class: "meta-editor-modal", onclose: move |()| { editing.set(false); editor_owner.set(None); },
                            div { class: "modal-body meta-editor-body",
                                p { class: "meta-editor-caption",
                                    if let Some(resource) = editor_resource { "{resource.name} · 草稿 r{resource.revision} · 保存后重新校验和测试" }
                                    else { "编辑中的草稿 · 保存后重新校验和测试" }
                                }
                                if !editor_error.read().is_empty() { p { class: "meta-editor-error", role: "alert", "{editor_error}" } }
                                if *show_diff.read() {
                                    section { class: "meta-diff", aria_label: "源码差异",
                                        for (path, baseline, current) in diff_files.iter() {
                                            div { class: "meta-diff-file", strong { "{path}" } pre { "基线：
{baseline}
当前：
{current}" } }
                                        }
                                        if diff_files.is_empty() { p { "当前源码与版本基线一致。" } }
                                    }
                                } else {
                                    div { class: "meta-editor-grid",
                                        nav { class: "meta-files", aria_label: "源码文件",
                                            for path in editor_paths.iter() {
                                                button { class: if editor_path.as_ref() == Some(path) { "active" } else { "" }, title: "{path}", onclick: { let path = path.clone(); move |_| active_file.set(path.clone()) }, "{path}" }
                                            }
                                        }
                                        div { class: "meta-editor-work",
                                            if let Some(path) = editor_path.clone() {
                                                div { class: "meta-file-name", "{path}" }
                                                textarea { class: "meta-code", aria_label: "源码编辑器", value: edit_files.read().get(&path).cloned().unwrap_or_default(), spellcheck: "false", oninput: { let path = path.clone(); move |e| { edit_files.write().insert(path.clone(), e.value()); } } }
                                            }
                                            small { class: "meta-editor-help", "本地设计源稿，不直接修改真实项目文件。" }
                                        }
                                    }
                                }
                            }
                            div { class: "modal-footer meta-editor-actions",
                                button { class: "meta-diff-toggle", aria_pressed: *show_diff.read(), onclick: move |_| { let next = !*show_diff.read(); show_diff.set(next); }, if *show_diff.read() { "返回源码" } else { "查看差异" } }
                                button { class: "meta-button", onclick: move |_| { editing.set(false); editor_owner.set(None); }, "取消" }
                                button { class: "meta-button primary", onclick: {
                                    let save_project = editor_save_project.clone(); let save_id = editor_save_id.clone(); let save_revision = editor_save_revision;
                                    move |_| {
                                        let drafts = edit_files.read().clone();
                                        if drafts.values().map(String::len).sum::<usize>() > MAX_BYTES { editor_error.set("源码总大小超过 200 KiB，请缩减后再保存。".into()); return; }
                                        let (Some(save_project), Some(save_id), Some(save_revision)) = (save_project.clone(), save_id.clone(), save_revision) else { editor_error.set("编辑目标已失效，请重新打开源码编辑器。".into()); return; };
                                        let mut data = store.write();
                                        let target = data.resources.iter_mut().find(|r| r.project_id == save_project && r.id == save_id && r.revision == save_revision);
                                        let Some(target) = target else { editor_error.set("资源已变化，草稿未覆盖新源码；请复制草稿后重新打开。".into()); return; };
                                        if drafts != target.sources { target.sources = drafts; target.revision += 1; target.validation = None; }
                                        editing.set(false); editor_owner.set(None);
                                    }
                                }, "保存草稿" }
                            }
                        }
            }
            if *new_open.read() {
                crate::shared::components::Modal { id: "meta-new-resource", title: "新建个人能力", class: "meta-new-modal", onclose: move |()| new_open.set(false),
                    div { class: "modal-body",
                        label { r#for: "meta-kind-select", "能力类型" }
                        select { id: "meta-kind-select", value: "{new_kind}", onchange: move |e| new_kind.set(e.value()), option { "Package" } option { "Skill" } option { "Extension" } }
                        label { r#for: "meta-name-input", "名称" } input { id: "meta-name-input", maxlength: "60", value: "{new_name}", placeholder: "例如：API 审阅包", oninput: move |e| new_name.set(e.value()) }
                        label { r#for: "meta-id-input", "资源 ID" } input { id: "meta-id-input", maxlength: "80", value: "{new_id}", placeholder: "personal.api-review", oninput: move |e| new_id.set(e.value()) }
                        p { "从 0.1.0 开始的设计样例；按项目首次创建，不代表已有节点安装。" }
                        if !new_error.read().is_empty() { p { class: "meta-editor-error", role: "alert", "{new_error}" } }
                        div { class: "modal-actions", button { class: "meta-button primary", onclick: {
                            let new_project_id = project_id.clone();
                            move |_| {
                                let kind=new_kind.read().clone(); let name=new_name.read().trim().to_owned(); let id=new_id.read().trim().to_owned();
                                if name.is_empty() || !valid_id(&id) { new_error.set("请填写名称和合法的小写资源 ID".into()); return; }
                                let mut data=store.write(); if data.resources.iter().any(|r| r.id==id) { new_error.set("此资源 ID 已存在".into()); return; }
                                let sources=templates(&kind,&id,&name,"0.1.0"); data.resources.push(MetaResource { project_id:new_project_id.clone(),id:id.clone(),name,kind,version:"0.1.0".into(),revision:1,baseline:sources.clone(),sources,validation:None,tests:Vec::new(),releases:Vec::new() }); data.selected_by_project.insert(new_project_id.clone(),id.clone()); selected.set(Some(id)); new_open.set(false); set_meta_notice(notice, notice_error, notice_epoch, notice_task, notice_timer_key, "已创建设计草稿", false);
                            }
                        }, "创建设计草稿" } }
                    }
                }
            }
        }
    }
}
