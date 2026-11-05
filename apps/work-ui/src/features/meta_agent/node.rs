//! Durable node-side authoring UI; editor buffers are never replaced on failed saves.
use super::MetaStore;
use dioxus::prelude::*;
use std::collections::BTreeMap;
use work_client::WorkspaceClient;
use work_core::api::{AuthoringRequest, AuthoringState, NativeCreate, NativeKind, NodeApi};

#[component]
pub fn NodeAuthoring(
    project: String,
    shared_client: Signal<Option<WorkspaceClient>>,
    connection_epoch: Signal<u64>,
    legacy: Signal<MetaStore>,
    onopen: EventHandler<String>,
) -> Element {
    let project_key = use_signal(|| project.clone());
    let mut state = use_signal(|| None::<AuthoringState>);
    let mut selected = use_signal(String::new);
    let mut buffer = use_signal(BTreeMap::<String, String>::new);
    let mut owner = use_signal(|| None::<(String, u64, String)>);
    let mut active_file = use_signal(String::new);
    let mut dirty = use_signal(|| false);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(String::new);
    let mut notice = use_signal(String::new);
    let mut new_id = use_signal(String::new);
    let mut kind = use_signal(|| "package".to_owned());
    let mut version = use_signal(|| "1.0.0".to_owned());
    let mut prompt = use_signal(|| {
        "Explain the selected Package instructions and perform a small safe project inspection. Report observed results.".to_owned()
    });
    let mut refresh = use_signal(|| 0u64);
    let mut ticket = use_signal(|| 0u64);
    let load_project = project.clone();
    use_effect(use_reactive((&load_project,), move |(project,)| {
        let _ = refresh.read();
        let epoch = *connection_epoch.read();
        let t = ticket.peek().wrapping_add(1);
        ticket.set(t);
        let Some(client) = shared_client.read().clone() else {
            state.set(None);
            return;
        };
        spawn(async move {
            let result = NodeApi::authoring(&client, &project, AuthoringRequest::Read).await;
            if *connection_epoch.peek() != epoch || *ticket.peek() != t {
                return;
            }
            match result {
                Ok(value) => {
                    if !*dirty.peek() {
                        if let Some(draft) = value.drafts.iter().find(|d| d.id == *selected.peek())
                        {
                            buffer.set(draft.sources.clone());
                            owner.set(Some((
                                draft.id.clone(),
                                draft.revision,
                                draft.digest.clone(),
                            )));
                            if !draft.sources.contains_key(&*active_file.peek()) {
                                active_file
                                    .set(draft.sources.keys().next().cloned().unwrap_or_default());
                            }
                        }
                    }
                    state.set(Some(value));
                }
                Err(e) => {
                    error.set(e.to_string());
                    state.set(None);
                }
            }
        });
    }));
    let mut action = move |request: AuthoringRequest| {
        if *busy.peek() {
            return;
        }
        let Some(client) = shared_client.peek().clone() else {
            return;
        };
        let project = project_key.peek().clone();
        let epoch = *connection_epoch.peek();
        let saved = matches!(&request, AuthoringRequest::Save { .. });
        let buffer_at_submit = buffer.peek().clone();
        busy.set(true);
        error.set(String::new());
        notice.set("正在处理，请稍候…".into());
        spawn(async move {
            let result = NodeApi::authoring(&client, &project, request).await;
            if *connection_epoch.peek() != epoch {
                return;
            }
            busy.set(false);
            match result {
                Ok(value) => {
                    // Only clear the submitted buffer version, not text typed while awaiting.
                    if saved && *buffer.peek() == buffer_at_submit {
                        dirty.set(false);
                        if let Some(draft) = value.drafts.iter().find(|d| d.id == *selected.peek())
                        {
                            owner.set(Some((
                                draft.id.clone(),
                                draft.revision,
                                draft.digest.clone(),
                            )));
                        }
                    }
                    if !*dirty.peek() {
                        if let Some(draft) = value.drafts.iter().find(|d| d.id == *selected.peek())
                        {
                            buffer.set(draft.sources.clone());
                            owner.set(Some((
                                draft.id.clone(),
                                draft.revision,
                                draft.digest.clone(),
                            )));
                        }
                    }
                    notice.set(
                        "已保存节点记录。静态校验不等于测试；测试成功且源码未变化后可发布。".into(),
                    );
                    state.set(Some(value));
                }
                Err(e) => {
                    error.set(e.to_string());
                    notice.set("操作未完成，未保存的编辑已保留。请回读节点内容后手工合并；不会自动覆盖或重试。".into());
                }
            }
        });
    };
    let mut load_editor = move |id: String| {
        if *dirty.peek() {
            error.set("有未保存源码；先保存或显式放弃后再切换草稿。".into());
            return;
        }
        if let Some(draft) = state
            .peek()
            .as_ref()
            .and_then(|s| s.drafts.iter().find(|d| d.id == id))
        {
            selected.set(id);
            buffer.set(draft.sources.clone());
            owner.set(Some((
                draft.id.clone(),
                draft.revision,
                draft.digest.clone(),
            )));
            active_file.set(draft.sources.keys().next().cloned().unwrap_or_default());
        }
    };
    let develop = move |_| {
        if *busy.peek() {
            return;
        }
        let Some(client) = shared_client.peek().clone() else {
            return;
        };
        let project = project_key.peek().clone();
        let epoch = *connection_epoch.peek();
        busy.set(true);
        spawn(async move {
            let result = NodeApi::native_create(
                &client,
                &project,
                NativeCreate {
                    developer: true,
                    selection: None,
                },
            )
            .await;
            if *connection_epoch.peek() != epoch {
                return;
            }
            busy.set(false);
            match result {
                Ok(id) => onopen.call(id),
                Err(e) => error.set(e.to_string()),
            }
        });
    };
    let current = state
        .read()
        .as_ref()
        .and_then(|s| s.drafts.iter().find(|d| d.id == *selected.read()))
        .cloned();
    let owner_value = owner.read().clone();
    let path = active_file();
    rsx! {
        section { class: "page-view native-authoring", aria_label: "元 Agent 节点源码开发",
            style { {include_str!("../resources/native.css")} }
            header { class: "page-heading", h1 { "元Agent · 资源开发" } p { "编辑项目源码，测试后发布为本地资源。" } }
            p { class: "meta", "开发助手可直接修改项目草稿；也可在下方编辑。静态校验不等于测试。测试成功且源码未变化后可发布。" }
            details { summary { "保存与测试的安全边界" } p { "保存和快照受节点执行锁保护；ACP 会话存在时拒绝不安全的修改。模型与编辑器读取相同文件。外部程序的写入不能被锁住，回读检测变化；冲突与不确定错误保留恢复字节。" } }
            div { class: "native-actions",
                button { class: "button", disabled: busy(), onclick: move |_| refresh += 1, "刷新草稿（保留未保存编辑）" }
                button { class: "button", disabled: busy() || dirty(), onclick: develop, "新建开发助手会话" }
            }
            if let Some(message) = &legacy.read().storage_error { p { role: "alert", "{message}" } }
            details { summary { "旧惰性草稿：保留原始字节 / 显式迁移" }
                p { "旧存储不再自动写入。读失败不会 reset；原始文件/localStorage 保持不变。导入只把原文保存为 references/legacy-inert.json，不转换为 native 权限。新草稿使用单独 canonical 源码模板。" }
                if let Some(original) = legacy.read().original.clone() {
                    pre { "{original}" }
                    button { class: "button", disabled: busy() || new_id().is_empty(), onclick: move |_| action(AuthoringRequest::ImportLegacy { id: new_id(), original: original.clone() }), "以新 ID 显式导入原始字节为惰性参考" }
                }
            }
            label { "新资源 ID" input { aria_label: "节点草稿 ID", value: "{new_id}", oninput: move |e| new_id.set(e.value()) } }
            label { "类型" select { aria_label: "节点草稿类型", value: "{kind}", onchange: move |e| kind.set(e.value()), option { value: "package", "Package" } option { value: "skill", "Skill" } option { value: "extension", "Extension 源码（需单独构建）" } } }
            button { class: "button primary", disabled: busy() || new_id().is_empty(), onclick: move |_| action(AuthoringRequest::Create { id: new_id(), kind: match kind().as_str() { "skill" => NativeKind::Skill, "extension" => NativeKind::Extension, _ => NativeKind::Package } }), "创建源码草稿" }
            if let Some(s) = state.read().as_ref() {
                for d in &s.drafts { { let id = d.id.clone(); rsx! { button { class: "button", disabled: busy(), onclick: move |_| load_editor(id.clone()), "{d.id} · {d.kind:?} · r{d.revision}" } } } }
                for diagnostic in &s.diagnostics { p { "{diagnostic}" } }
            } else { p { "草稿尚未观察；读取失败不会创建空 store 覆盖节点。" } }
            if let Some(draft) = current {
                h2 { "{draft.id} · r{draft.revision}" }
                p { class: "meta", if dirty() { "有未保存编辑" } else { "编辑已保存" } }
                details { summary { "源码位置与版本摘要" } p { "源码目录：{draft.path}" } p { "digest: {draft.digest} · 编辑版本：{owner_value:?}" } }
                label { "源码文件" select { aria_label: "节点源码文件", value: "{active_file}", onchange: move |e| active_file.set(e.value()), for path in buffer.read().keys() { option { value: "{path}", "{path}" } } } }
                if let Some(text) = buffer.read().get(&path).cloned() {
                    textarea { class: "native-source-editor", aria_label: "节点草稿源码", value: "{text}", spellcheck: "false", oninput: move |e| { buffer.write().insert(active_file(),e.value()); dirty.set(true); } }
                }
                button { class: "button primary", disabled: busy() || owner.read().is_none(), onclick: move |_| { if let Some((id,revision,digest)) = owner.peek().clone() { action(AuthoringRequest::Save { id,revision,digest,sources:buffer.peek().clone() }); } }, "保存源码" }
                button { class: "button danger", disabled: busy(), onclick: { let id = draft.id.clone(); move |_| { dirty.set(false); load_editor(id.clone()); } }, "放弃编辑并载入已回读内容" }
                button { class: "button", disabled: busy() || dirty(), onclick: { let d = draft.clone(); move |_| action(AuthoringRequest::Validate { id:d.id.clone(),revision:d.revision,digest:d.digest.clone() }) }, "静态校验（不执行构建）" }
                if let Some(v) = &draft.validation { p { "校验 r{v.revision} · valid: {v.valid} · {v.diagnostics:?}" } }
                button { class: "button", disabled: busy() || dirty(), onclick: { let d = draft.clone(); move |_| action(AuthoringRequest::Test { id:d.id.clone(),revision:d.revision,digest:d.digest.clone() }) }, "创建测试快照与会话" }
                label { "测试请求（将调用已配置的模型）" textarea { aria_label: "真实测试请求", value: "{prompt}", oninput: move |e| prompt.set(e.value()) } }
                label { "个人发布版本" input { aria_label: "个人发布版本", value: "{version}", oninput: move |e| version.set(e.value()) } }
                for test in &draft.tests {
                    { let execute_id = draft.id.clone(); let record_id = execute_id.clone(); let tid = test.id.clone(); let record_tid = tid.clone(); let publish_draft = draft.clone(); let publish_tid = tid.clone(); let sid = test.session_id.clone(); rsx! {
                        article { class: "native-resource-card",
                            h3 { "测试 · r{test.revision}" }
                            p { if test.evidence.is_some() { "测试已成功完成" } else { "尚未记录成功的执行测试" } }
                            details { summary { "测试快照与执行记录" } p { "{test.id} · {test.digest} · {test.snapshot} · 会话 {test.session_id}" } pre { "{test.evidence:?}" } }
                            button { class: "button", disabled: busy() || dirty(), onclick: move |_| onopen.call(sid.clone()), "打开真实测试会话" }
                            button { class: "button primary", disabled: busy() || dirty(), onclick: move |_| action(AuthoringRequest::ExecuteTest { id:execute_id.clone(),test_id:tid.clone(),prompt:prompt() }), "执行测试" }
                            button { class: "button", disabled: busy(), onclick: move |_| action(AuthoringRequest::RecordEvidence { id:record_id.clone(),test_id:record_tid.clone() }), "核对执行记录" }
                            button { class: "button primary", disabled: busy() || dirty(), onclick: move |_| action(AuthoringRequest::Publish { id:publish_draft.id.clone(),revision:publish_draft.revision,digest:publish_draft.digest.clone(),test_id:publish_tid.clone(),version:version() }), "发布本地版本（不上传）" }
                            details { summary { "实际加载详情" } pre { "{test.readback}" } }
                        }
                    } }
                }
                for release in &draft.releases {
                    p { "已发布 {release.version} · digest {release.digest} · tested by {release.test_id}。来源：{release.source}；已测试安装 opaque ref: {release.installed_reference}" }
                    p { "其他普通会话通过原生资源页从此来源安装 / 选择。发布不会更改运行会话或旧测试。" }
                }
            }
            if busy() { p { role: "status", "节点操作中…" } }
            if !notice().is_empty() { p { role: "status", "{notice}" } }
            if !error().is_empty() { p { role: "alert", class: "resource-error", "{error}" } }
        }
    }
}
