//! Node-wide saved-file management. No control host, remote probe or raw commands.
use crate::shared::components::{Icon, Modal};
use dioxus::prelude::*;
use work_client::WorkspaceClient;
use work_core::api::{
    CredentialLocation, ModelDescriptor, NodeApi, NodeError, NodeErrorKind, ProviderAction,
    ProviderApi, ProviderCatalog, ProviderDefinition, ProviderModel, ProviderMutation,
    StoredCredential,
};

#[derive(Clone, Debug, PartialEq)]
struct ProviderContext {
    epoch: u64,
    generation: u64,
    session: Option<String>,
    online: bool,
}
fn fresh(expected: &ProviderContext, current: &ProviderContext, ticket: u64, latest: u64) -> bool {
    expected == current && current.online && ticket == latest
}
fn write_gate(online: bool, observed: bool, busy: bool, pending: bool) -> bool {
    online && observed && !busy && !pending
}
fn valid_text(value: &str, max: usize) -> bool {
    value.len() <= max && !value.chars().any(char::is_control)
}
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 96
        && id != "."
        && id != ".."
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
}
fn parse_tokens(value: &str) -> Result<Option<u64>, String> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    if !value.bytes().all(|c| c.is_ascii_digit()) {
        return Err("Token 数须为正整数，留空表示未指定。".into());
    }
    match value.parse::<u64>() {
        Ok(n @ 1..=100_000_000) => Ok(Some(n)),
        _ => Err("Token 数范围为 1–100000000。".into()),
    }
}
// Conservative local syntax check only. The node performs full URL validation;
// neither side requests the configured endpoint when saving.
fn valid_endpoint(value: &str) -> bool {
    let Some(rest) = value
        .strip_prefix("https://")
        .or_else(|| value.strip_prefix("http://"))
    else {
        return false;
    };
    let authority = rest.split('/').next().unwrap_or_default();
    value.len() <= 2048
        && !authority.is_empty()
        && !authority.contains('@')
        && !value.contains(['?', '#'])
        && !value.chars().any(|c| c.is_control() || c.is_whitespace())
}
#[derive(Clone, Default, PartialEq)]
struct ModelDraft {
    original_id: Option<String>,
    id: String,
    name: String,
    reasoning: bool,
    context: String,
    max: String,
}
#[derive(Clone, PartialEq)]
struct DefinitionDraft {
    id: String,
    name: String,
    api: ProviderApi,
    base_url: String,
    models: Vec<ModelDraft>,
    existing: bool,
}
impl Default for DefinitionDraft {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            api: ProviderApi::OpenaiCompletions,
            base_url: String::new(),
            models: vec![ModelDraft::default()],
            existing: false,
        }
    }
}
impl From<ProviderDefinition> for DefinitionDraft {
    fn from(d: ProviderDefinition) -> Self {
        Self {
            id: d.id,
            name: d.name,
            api: d.api,
            base_url: d.base_url,
            existing: true,
            models: d
                .models
                .into_iter()
                .map(|m| ModelDraft {
                    original_id: Some(m.id.clone()),
                    id: m.id,
                    name: m.name,
                    reasoning: m.reasoning,
                    context: m.context_window.map(|n| n.to_string()).unwrap_or_default(),
                    max: m.max_tokens.map(|n| n.to_string()).unwrap_or_default(),
                })
                .collect(),
        }
    }
}
impl DefinitionDraft {
    fn parse(&self) -> Result<ProviderDefinition, String> {
        if !valid_id(&self.id) {
            return Err("供应商 ID 为 1–96 个字母、数字、-、_、.，不能为 . 或 ..。".into());
        }
        if !valid_text(&self.name, 160) {
            return Err("供应商名称过长或包含控制字符。".into());
        }
        if !valid_endpoint(&self.base_url) {
            return Err("请输入绝对 HTTP(S) Base URL；不得含凭据、查询、片段或空白。".into());
        }
        if self.models.len() > 128 {
            return Err("最多保存 128 个模型。".into());
        }
        let mut ids = std::collections::BTreeSet::new();
        let mut models = Vec::new();
        for (i, row) in self.models.iter().enumerate() {
            if row
                .original_id
                .as_ref()
                .is_some_and(|original| original != &row.id)
            {
                return Err("已有模型 ID 不可直接改名；请显式移除后添加新模型，新模型不会继承旧模型的高级配置。".into());
            }
            if row.id.is_empty()
                || row.id.trim() != row.id
                || !valid_text(&row.id, 200)
                || !valid_text(&row.name, 200)
                || !ids.insert(&row.id)
            {
                return Err(format!(
                    "模型 {}：ID 不能为空、重复或含首尾空白；ID/名称最多 200 字节且不得含控制字符。",
                    i + 1
                ));
            }
            let context_window = parse_tokens(&row.context)?;
            let max_tokens = parse_tokens(&row.max)?;
            if context_window.zip(max_tokens).is_some_and(|(c, m)| m > c) {
                return Err(format!("模型 {}：最大输出不能超过上下文窗口。", i + 1));
            }
            models.push(ProviderModel {
                id: row.id.clone(),
                name: row.name.clone(),
                reasoning: row.reasoning,
                context_window,
                max_tokens,
            });
        }
        Ok(ProviderDefinition {
            id: self.id.clone(),
            name: self.name.clone(),
            api: self.api,
            base_url: self.base_url.clone(),
            models,
        })
    }
}
/// Never holds a key. Presence is the strongest observation available for secrets;
/// the UI cannot verify their value or remote acceptance.
#[derive(Clone)]
enum ExpectedChange {
    Definition(ProviderDefinition),
    Removed(String),
    KeyPresent(String),
    Cleared(String, CredentialLocation),
    HostRelease,
}
impl ExpectedChange {
    fn from_action(action: &ProviderAction) -> Self {
        match action {
            ProviderAction::Upsert { definition } => Self::Definition(definition.clone()),
            ProviderAction::RemoveConfig { id } => Self::Removed(id.clone()),
            ProviderAction::SetKey { id, .. } => Self::KeyPresent(id.clone()),
            ProviderAction::ClearCredential { id, location } => {
                Self::Cleared(id.clone(), *location)
            }
        }
    }
    fn matches(&self, catalog: &ProviderCatalog) -> bool {
        let id = match self {
            Self::Definition(d) => &d.id,
            Self::Removed(id) | Self::KeyPresent(id) | Self::Cleared(id, _) => id,
            Self::HostRelease => return false,
        };
        let entry = catalog.providers.iter().find(|p| p.id == *id);
        match self {
            Self::Definition(d) => entry.and_then(|p| p.definition.as_ref()) == Some(d),
            Self::Removed(_) => entry.is_none_or(|p| !p.configured),
            Self::KeyPresent(_) => {
                entry.is_some_and(|p| p.stored_credential == StoredCredential::ApiKey)
            }
            Self::Cleared(_, CredentialLocation::Stored) => {
                entry.is_none_or(|p| p.stored_credential == StoredCredential::None)
            }
            Self::Cleared(_, CredentialLocation::Config) => {
                entry.is_none_or(|p| !p.config_key_present)
            }
            Self::HostRelease => false,
        }
    }
}
fn expected_readback(
    change: &ExpectedChange,
    receipt_revision: &str,
    catalog: &ProviderCatalog,
) -> bool {
    !receipt_revision.is_empty() && receipt_revision == catalog.revision && change.matches(catalog)
}
#[derive(Clone, PartialEq)]
enum Editor {
    Closed,
    Definition,
    Key(String),
    NewKey(String),
}
#[derive(Clone, PartialEq)]
enum Confirmation {
    Remove(String),
    ClearStored(String),
    ClearConfig(String),
    ReplaceKey(String),
}
impl Confirmation {
    fn id(&self) -> &str {
        match self {
            Self::Remove(id)
            | Self::ClearStored(id)
            | Self::ClearConfig(id)
            | Self::ReplaceKey(id) => id,
        }
    }
    fn message(&self) -> &'static str {
        match self {
            Self::Remove(_) => {
                "仅删除 models.json 中的自定义配置（含其中的配置凭据）；auth.json 已保存的凭据会保留并继续显示。不会卸载内置供应商。"
            }
            Self::ClearStored(_) => {
                "仅删除 auth.json 保存记录（API Key / OAuth / 其他）。环境变量、CLI 或 models.json 仍可能提供凭据。删除 OAuth 本地记录不撤销远端授权；如需撤销，请到供应商管理授权。"
            }
            Self::ClearConfig(_) => {
                "仅删除 models.json 的 apiKey 配置来源，不删除 auth.json 凭据。环境变量或 CLI 仍可能作为回退来源。"
            }
            Self::ReplaceKey(_) => {
                "将替换 auth.json 中已有凭据（包括 OAuth 记录）。不会撤销远端 OAuth 授权。提交后清空输入；不会验证远端认证。"
            }
        }
    }
}
// Input events share the same live write gate as network actions.
enum Edit {
    OpenNew,
    ResumeDefinition,
    UseHostDefinition(ProviderDefinition),
    OpenDefinition(ProviderDefinition),
    OpenKey(String),
    NewKeyId(String),
    Close,
    Id(String),
    Name(String),
    Api(String),
    Endpoint(String),
    Model(usize, &'static str, String),
    Reasoning(usize, bool),
    AddModel,
    ReplaceModels(Vec<ModelDraft>),
    RemoveModel(usize),
    Secret(String),
    Confirm(Confirmation),
    CancelConfirm,
}
/// Remount the entire state owner on same-address reconnect/session/offline changes.
#[component]
pub fn ProviderSettings(
    shared_client: Signal<Option<WorkspaceClient>>,
    connection_epoch: Signal<u64>,
    session_generation: Signal<u64>,
    selected_session: Signal<Option<String>>,
    online: bool,
    onreturn: EventHandler<MouseEvent>,
    oncomponents: EventHandler<MouseEvent>,
) -> Element {
    let mut live_online = use_signal(|| online);
    if *live_online.peek() != online {
        live_online.set(online);
    }
    let context = ProviderContext {
        epoch: connection_epoch(),
        generation: session_generation(),
        session: selected_session(),
        online: online && shared_client.read().is_some(),
    };
    let key = serde_json::to_string(&(
        context.epoch,
        context.generation,
        &context.session,
        context.online,
    ))
    .unwrap_or_default();
    rsx! { ProviderSettingsState { key: "{key}", context, shared_client, connection_epoch, session_generation, selected_session, live_online, onreturn, oncomponents } }
}
#[component]
fn ProviderSettingsState(
    context: ProviderContext,
    shared_client: Signal<Option<WorkspaceClient>>,
    connection_epoch: Signal<u64>,
    session_generation: Signal<u64>,
    selected_session: Signal<Option<String>>,
    live_online: Signal<bool>,
    onreturn: EventHandler<MouseEvent>,
    oncomponents: EventHandler<MouseEvent>,
) -> Element {
    let mounted = use_signal(|| context.clone());
    let mut catalog = use_signal(|| None::<ProviderCatalog>);
    let mut busy = use_signal(|| false);
    let mut pending = use_signal(|| None::<ExpectedChange>);
    let mut ticket = use_signal(|| 0u64);
    let mut query = use_signal(String::new);
    let mut notice = use_signal(String::new);
    let mut error = use_signal(String::new);
    let mut editor = use_signal(|| Editor::Closed);
    let mut draft = use_signal(DefinitionDraft::default);
    let mut retained_definition = use_signal(|| None::<DefinitionDraft>);
    let mut secret = use_signal(String::new);
    let mut confirmation = use_signal(|| None::<Confirmation>);
    let mut discovered_models =
        use_signal(|| std::collections::BTreeMap::<String, Vec<ModelDescriptor>>::new());
    let mut discovery_busy = use_signal(|| None::<String>);
    let mut discovery_error = use_signal(String::new);
    let is_fresh = move |expected: &ProviderContext, request: u64| {
        let current = ProviderContext {
            epoch: *connection_epoch.peek(),
            generation: *session_generation.peek(),
            session: selected_session.peek().clone(),
            online: *live_online.peek() && shared_client.peek().is_some(),
        };
        fresh(expected, &current, request, *ticket.peek())
    };
    let can_write = move || {
        is_fresh(&mounted.peek(), *ticket.peek())
            && write_gate(
                *live_online.peek(),
                catalog.peek().is_some(),
                *busy.peek(),
                pending.peek().is_some(),
            )
            && discovery_busy.peek().is_none()
    };
    let refresh = Callback::new(move |()| {
        secret.set(String::new());
        let expected = mounted.peek().clone();
        if *busy.peek() || discovery_busy.peek().is_some() || !is_fresh(&expected, *ticket.peek()) {
            return;
        }
        let Some(client) = shared_client.peek().clone() else {
            return;
        };
        let next = ticket.peek().wrapping_add(1);
        ticket.set(next);
        busy.set(true);
        error.set(String::new());
        editor.set(Editor::Closed);
        confirmation.set(None);
        catalog.set(None);
        spawn(async move {
            let result = bounded(NodeApi::provider_catalog(&client)).await;
            if !is_fresh(&expected, next) {
                return;
            }
            busy.set(false);
            match result {
                Ok(value) if !value.revision.is_empty() => {
                    if let Some(change) = pending.peek().as_ref() {
                        notice.set(if change.matches(&value) {
                            "已重新读取保存文件，目标字段与期望一致；此前结果仍未确认为成功。凭据仅可核对来源存在，不能核对内容。请检查后再操作。".into()
                        } else { "已重新读取保存文件，但与此前意图不一致或宿主释放状态不可核对；未宣称成功。请检查后重新操作，不会自动重试。".into() });
                    }
                    pending.set(None);
                    catalog.set(Some(value));
                }
                _ => {
                    catalog.set(None);
                    error.set("无法读取供应商目录：节点可能离线、未授权、旧版本或不支持（含 404）。写入已禁用，不会回退或创建控制会话。请连接支持此功能的节点后刷新。".into());
                }
            }
        });
    });
    use_effect(move || {
        refresh.call(());
    });
    use_drop(move || {
        secret.set(String::new());
    });
    let edit = Callback::new(move |action: Edit| {
        if !can_write() {
            secret.set(String::new());
            return;
        }
        if !matches!(action, Edit::Secret(_)) {
            secret.set(String::new());
        }
        error.set(String::new());
        match action {
            Edit::OpenNew => {
                draft.set(DefinitionDraft::default());
                editor.set(Editor::Definition);
                confirmation.set(None);
            }
            Edit::ResumeDefinition => {
                if let Some(saved) = retained_definition.peek().clone() {
                    draft.set(saved);
                    editor.set(Editor::Definition);
                    confirmation.set(None);
                }
            }
            Edit::OpenDefinition(d) => {
                draft.set(d.into());
                editor.set(Editor::Definition);
                confirmation.set(None);
            }
            Edit::UseHostDefinition(d) => {
                let mut value = DefinitionDraft::from(d);
                value.existing = false;
                for model in &mut value.models {
                    model.original_id = None;
                }
                draft.set(value);
                editor.set(Editor::Definition);
                confirmation.set(None);
            }
            Edit::OpenKey(id) => {
                editor.set(Editor::Key(id));
                confirmation.set(None);
            }
            Edit::NewKeyId(id) => {
                if matches!(&*editor.peek(), Editor::NewKey(_)) {
                    editor.set(Editor::NewKey(id));
                }
            }
            Edit::Close => {
                editor.set(Editor::Closed);
                confirmation.set(None);
            }
            Edit::Confirm(c) => {
                editor.set(Editor::Closed);
                confirmation.set(Some(c));
            }
            Edit::CancelConfirm => confirmation.set(None),
            Edit::Id(v) => {
                if !draft.peek().existing {
                    draft.write().id = v;
                }
            }
            Edit::Name(v) => draft.write().name = v,
            Edit::Api(v) => {
                draft.write().api = match v.as_str() {
                    "anthropic-messages" => ProviderApi::AnthropicMessages,
                    "openai-responses" => ProviderApi::OpenaiResponses,
                    _ => ProviderApi::OpenaiCompletions,
                };
            }
            Edit::Endpoint(v) => draft.write().base_url = v,
            Edit::AddModel => {
                if draft.peek().models.len() < 128 {
                    draft.write().models.push(ModelDraft::default());
                }
            }
            Edit::ReplaceModels(models) => draft.write().models = models,
            Edit::RemoveModel(i) => {
                if i < draft.peek().models.len() {
                    draft.write().models.remove(i);
                }
            }
            Edit::Model(i, field, v) => {
                if let Some(m) = draft.write().models.get_mut(i) {
                    match field {
                        "id" if m.original_id.is_none() => m.id = v,
                        "name" => m.name = v,
                        "context" => m.context = v,
                        "max" => m.max = v,
                        _ => {}
                    }
                }
            }
            Edit::Reasoning(i, v) => {
                if let Some(m) = draft.write().models.get_mut(i) {
                    m.reasoning = v;
                }
            }
            Edit::Secret(v) => secret.set(v),
        }
    });
    let mutate = Callback::new(move |action: ProviderAction| {
        secret.set(String::new());
        if !can_write() {
            return;
        }
        let Some(client) = shared_client.peek().clone() else {
            return;
        };
        let Some(observed) = catalog.peek().clone() else {
            return;
        };
        // Never rebuild an unsupported advanced definition or turn Add into an
        // implicit overwrite. Existing edits retain exact ID and observed CAS.
        if let ProviderAction::Upsert { definition } = &action {
            if let Some(entry) = observed.providers.iter().find(|p| p.id == definition.id) {
                if (!draft.peek().existing || entry.definition.is_none()) && entry.configured {
                    error.set(
                        "该 ID 已有配置；请从目录编辑。高级定义只读，不会通过表单重建。".into(),
                    );
                    return;
                }
            }
        }
        let expected = mounted.peek().clone();
        let change = ExpectedChange::from_action(&action);
        if matches!(change, ExpectedChange::Definition(_)) {
            retained_definition.set(Some(draft.peek().clone()));
        }
        let request = ProviderMutation {
            expected_revision: observed.revision,
            action,
        };
        let next = ticket.peek().wrapping_add(1);
        ticket.set(next);
        pending.set(Some(change.clone()));
        busy.set(true);
        error.set(String::new());
        notice.set(String::new());
        editor.set(Editor::Closed);
        confirmation.set(None);
        spawn(async move {
            let result = bounded(NodeApi::provider_mutate(&client, request)).await;
            if !is_fresh(&expected, next) {
                return;
            }
            // Always reread, even after error/timeout. A receipt is not the
            // authoritative observation and errors never imply safe retry.
            let readback = bounded(NodeApi::provider_catalog(&client)).await;
            if !is_fresh(&expected, next) {
                return;
            }
            busy.set(false);
            match (result, readback) {
                (Ok(receipt), Ok(value))
                    if expected_readback(&change, &receipt.catalog.revision, &value) =>
                {
                    catalog.set(Some(value));
                    pending.set(None);
                    if matches!(change, ExpectedChange::Definition(_)) {
                        retained_definition.set(None);
                    }
                    notice.set("已权威回读并核对保存文件的目标字段（凭据仅核对保存来源，不能读取或验证密钥内容）。这不是远端认证或运行时生效证明。新宿主读取新配置，已有活动宿主保持原配置。".into());
                }
                (_, readback) => {
                    catalog.set(readback.ok().filter(|c| !c.revision.is_empty()));
                    error.set("提交失败、超时或回读不匹配，结果待核对。写入保持锁定；请重新进入供应商页面以重新读取宿主目录，不会自动重试，也不代表节点已取消操作。".into());
                }
            }
        });
    });
    let save_definition = Callback::new(move |()| {
        secret.set(String::new());
        if !can_write() || *editor.peek() != Editor::Definition {
            return;
        }
        let parsed = draft.peek().parse();
        match parsed {
            Ok(definition) => mutate.call(ProviderAction::Upsert { definition }),
            Err(e) => error.set(e),
        }
    });
    let submit_key = Callback::new(move |()| {
        let key = std::mem::take(&mut *secret.write());
        if !can_write() {
            return;
        }
        let target = editor.peek().clone();
        let (Editor::Key(id) | Editor::NewKey(id)) = target.clone() else {
            return;
        };
        if !valid_id(&id) {
            error
                .set("请输入有效的原生供应商 ID（字母、数字、-、_、.），不要输入显示名称。".into());
            return;
        }
        if matches!(target, Editor::NewKey(_))
            && catalog.peek().as_ref().is_some_and(|c| {
                c.providers
                    .iter()
                    .any(|p| p.id == id && p.stored_credential != StoredCredential::None)
            })
        {
            error.set(
                "该供应商已有保存凭据。请取消并从对应记录确认替换，不能通过新增入口覆盖已有记录。"
                    .into(),
            );
            return;
        }
        if key.trim().is_empty() || !valid_text(&key, 8192) {
            error.set("请输入非空 API Key，最多 8192 字节且不得含控制字符。".into());
            return;
        }
        mutate.call(ProviderAction::SetKey { id, key });
    });
    let confirm_action = Callback::new(move |()| {
        secret.set(String::new());
        if !can_write() {
            return;
        }
        let Some(c) = confirmation.peek().clone() else {
            return;
        };
        match c {
            Confirmation::Remove(id) => mutate.call(ProviderAction::RemoveConfig { id }),
            Confirmation::ClearStored(id) => mutate.call(ProviderAction::ClearCredential {
                id,
                location: CredentialLocation::Stored,
            }),
            Confirmation::ClearConfig(id) => mutate.call(ProviderAction::ClearCredential {
                id,
                location: CredentialLocation::Config,
            }),
            Confirmation::ReplaceKey(id) => {
                confirmation.set(None);
                editor.set(Editor::Key(id));
            }
        }
    });
    let discover_models = Callback::new(move |provider_id: String| {
        discovery_error.set(String::new());
        let Some(session_id) = selected_session.peek().clone() else {
            discovery_error
                .set("当前没有活动会话；请先返回会话并启动一个会话，再获取模型列表。".into());
            return;
        };
        let Some(client) = shared_client.peek().clone() else {
            discovery_error.set("节点连接已关闭，无法获取模型列表。".into());
            return;
        };
        if discovery_busy.peek().is_some() {
            return;
        }
        let expected = mounted.peek().clone();
        let request = ticket.peek().wrapping_add(1);
        ticket.set(request);
        discovery_busy.set(Some(provider_id.clone()));
        spawn(async move {
            let result = client.models(&session_id).await;
            if !is_fresh(&expected, request) {
                return;
            }
            discovery_busy.set(None);
            match result {
                Ok(models) => {
                    let matching = models
                        .into_iter()
                        .filter(|model| {
                            model.provider == provider_id
                                || model.id.starts_with(&format!("{provider_id}/"))
                        })
                        .collect::<Vec<_>>();
                    discovered_models.write().insert(provider_id, matching);
                }
                Err(error) => discovery_error.set(format!("获取模型列表失败：{error}")),
            }
        });
    });
    let fetch_draft_models = Callback::new(move |()| {
        discovery_error.set(String::new());
        let provider_id = draft.peek().id.trim().to_string();
        if provider_id.is_empty() {
            discovery_error.set("请先填写供应商 ID，再从当前会话获取模型列表。".into());
            return;
        }
        let Some(session_id) = selected_session.peek().clone() else {
            discovery_error.set("当前没有活动会话；请先启动会话，再获取模型列表。".into());
            return;
        };
        let Some(client) = shared_client.peek().clone() else {
            discovery_error.set("节点连接已关闭，无法获取模型列表。".into());
            return;
        };
        if discovery_busy.peek().is_some() {
            return;
        }
        let expected = mounted.peek().clone();
        let request = ticket.peek().wrapping_add(1);
        ticket.set(request);
        discovery_busy.set(Some(provider_id.clone()));
        spawn(async move {
            let result = client.models(&session_id).await;
            if !is_fresh(&expected, request) {
                return;
            }
            discovery_busy.set(None);
            match result {
                Ok(models) => {
                    let matching = models
                        .into_iter()
                        .filter(|model| {
                            model.provider == provider_id
                                || model.id.starts_with(&format!("{provider_id}/"))
                        })
                        .collect::<Vec<_>>();
                    if matching.is_empty() {
                        discovery_error.set("当前会话没有返回该供应商的模型；请检查供应商 ID 或先启动对应会话。".into());
                        return;
                    }
                    let draft_models = matching
                        .iter()
                        .map(|model| ModelDraft {
                            original_id: None,
                            id: model.id.clone(),
                            name: if model.name.is_empty() { model.id.clone() } else { model.name.clone() },
                            reasoning: model.reasoning,
                            context: model.context_window.map(|n| n.to_string()).unwrap_or_default(),
                            max: model.max_tokens.map(|n| n.to_string()).unwrap_or_default(),
                        })
                        .collect::<Vec<_>>();
                    edit.call(Edit::ReplaceModels(draft_models));
                    discovered_models.write().insert(provider_id, matching);
                }
                Err(error) => discovery_error.set(format!("获取模型列表失败：{error}")),
            }
        });
    });
    let use_discovered_model = Callback::new(move |model_id: String| {
        discovery_error.set(String::new());
        notice.set(String::new());
        let Some(session_id) = selected_session.peek().clone() else {
            discovery_error.set("当前没有活动会话，无法切换模型。".into());
            return;
        };
        let Some(client) = shared_client.peek().clone() else {
            discovery_error.set("节点连接已关闭，无法切换模型。".into());
            return;
        };
        if discovery_busy.peek().is_some() {
            return;
        }
        let expected = mounted.peek().clone();
        let request = ticket.peek().wrapping_add(1);
        ticket.set(request);
        discovery_busy.set(Some(model_id.clone()));
        spawn(async move {
            let result = match client.set_model(&session_id, &model_id).await {
                Ok(_) => client.runtime_state(&session_id).await,
                Err(error) => Err(error),
            };
            if !is_fresh(&expected, request) {
                return;
            }
            discovery_busy.set(None);
            match result {
                Ok(state) if state.model_id.as_deref() == Some(model_id.as_str()) => {
                    notice.set(format!(
                        "已将当前会话切换到模型「{model_id}」。API Key 仍只由节点按其凭据来源读取。"
                    ));
                }
                Ok(_) => discovery_error.set(
                    "模型切换请求已返回，但节点回读的模型不一致；请刷新会话状态后再试。".into(),
                ),
                Err(error) => discovery_error.set(format!("切换模型失败：{error}")),
            }
        });
    });
    let release = Callback::new(move |()| {
        secret.set(String::new());
        if !can_write() {
            return;
        }
        let Some(client) = shared_client.peek().clone() else {
            return;
        };
        let expected = mounted.peek().clone();
        let next = ticket.peek().wrapping_add(1);
        ticket.set(next);
        busy.set(true);
        error.set(String::new());
        notice.set(String::new());
        editor.set(Editor::Closed);
        confirmation.set(None);
        spawn(async move {
            let result = bounded(NodeApi::provider_release_hosts(&client)).await;
            if !is_fresh(&expected, next) {
                return;
            }
            busy.set(false);
            match result {
                Ok(report) => notice.set(format!("已释放 {} 个空闲宿主；{} 个受阻。未强制停止活动宿主，也不表示全部重启。新宿主将读取保存配置。", report.released.len(), report.blocked.len())),
                Err(_) => { pending.set(Some(ExpectedChange::HostRelease)); error.set("释放宿主失败或结果未知；不会自动重试。请刷新核对后再操作，未宣称已停止或重启。".into()); }
            }
        });
    });
    let enabled = can_write();
    let filter = query().trim().to_lowercase();
    let selected_session_id = selected_session.read().clone();
    let entries = catalog
        .read()
        .as_ref()
        .map(|c| {
            c.providers
                .iter()
                .filter(|p| {
                    let name = p
                        .definition
                        .as_ref()
                        .map(|d| {
                            format!(
                                "{} {} {}",
                                d.name,
                                d.base_url,
                                d.models
                                    .iter()
                                    .map(|m| format!("{} {}", m.id, m.name))
                                    .collect::<Vec<_>>()
                                    .join(" ")
                            )
                        })
                        .unwrap_or_default();
                    format!("{} {name}", p.id).to_lowercase().contains(&filter)
                })
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let host_definitions = catalog
        .read()
        .as_ref()
        .map(|value| {
            value
                .providers
                .iter()
                .filter_map(|entry| entry.definition.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let draft_value = draft();
    let editor_value = editor();
    let definition_title = if draft_value.existing {
        "编辑供应商定义"
    } else {
        "添加供应商定义"
    };
    rsx! {
        style { {include_str!("providers.css")} }
        section { class: "provider-management", aria_label: "节点供应商配置",
            div { class: "page-heading provider-toolbar",
                div { h1 { "模型供应商" } p { "管理当前节点的模型连接，配置后即可在会话中选择。" } }
                div { class: "provider-actions",
                    button { class: "button provider-extension-button", disabled: !context.online, onclick: move |event| { secret.set(String::new()); if is_fresh(&mounted.peek(), *ticket.peek()) { oncomponents.call(event); } }, Icon { name: "download" } "下载 Provider 扩展" }
                    button { class: "button primary", disabled: !enabled, onclick: move |_| edit.call(Edit::OpenNew), Icon { name: "plus" } "添加供应商" }
                }
            }
            div { class: "provider-overview", role: "note",
                Icon { name: "info" }
                p { "供应商配置保存在当前执行节点。保存成功不代表远端认证或已有宿主立即生效；不会连接测试新 URL。" }
            }
            if !context.online { p { role: "status", "节点尚未在线，所有修改已禁用；不会显示上个连接的结果。" } }
            if *busy.read() { p { role: "status", "正在请求节点并核对保存文件…" } }
            if pending.read().is_some() { p { class: "provider-note", role: "status", "提交待核对：修改和凭据输入已锁定，刷新不会重发写入。" } }
            if !error.read().is_empty() { p { class: "provider-error", role: "alert", "{error}" } }
            if !notice.read().is_empty() { p { class: "provider-note", role: "status", "{notice}" } }
            if !discovery_error.read().is_empty() { p { class: "provider-error", role: "alert", "{discovery_error}" } }
            if retained_definition.read().is_some() && editor_value == Editor::Closed {
                button { class: "button", disabled: !enabled, onclick: move |_| edit.call(Edit::ResumeDefinition), "恢复未保存定义草稿" }
            }
            div { class: "provider-toolbar",
                label { class: "provider-search", r#for: "provider-search", "搜索供应商或模型"
                    input { id: "provider-search", r#type: "search", value: "{query}", placeholder: "ID、名称、地址、模型", oninput: move |event| { if is_fresh(&mounted.peek(), *ticket.peek()) { secret.set(String::new()); query.set(event.value()); } } }
                }
                button { class: "button", disabled: !enabled, onclick: move |_| release.call(()), "释放空闲原生宿主" }
                button { class: "button", onclick: move |event| { secret.set(String::new()); if is_fresh(&mounted.peek(), *ticket.peek()) { onreturn.call(event); } }, "返回会话选择模型" }
            }
            p { class: "provider-meta provider-list-caption", "已保存的供应商可在会话的模型选择器中使用。凭据只显示保存来源，不会回读密钥内容。" }
            div { class: "provider-section-heading", h2 { "已添加" } span { "· {entries.len()}" } }
            div { class: "provider-catalog",
                if entries.is_empty() && catalog.read().is_some() { p { "没有匹配的保存记录。可添加供应商，或调整搜索条件。" } }
                for entry in entries {
                    { let id = entry.id.clone();
                      let edit_def = entry.definition.clone();
                      let stored = entry.stored_credential;
                      let display_name = entry.definition.as_ref().map(|d| if d.name.is_empty() { id.clone() } else { d.name.clone() }).unwrap_or_else(|| id.clone());
                      let mark = display_name.chars().next().map(|c| c.to_uppercase().collect::<String>()).unwrap_or_else(|| "P".into());
                      let credential_ready = stored != StoredCredential::None || entry.config_key_present;
                      rsx! {
                        article { class: if entry.configured { "provider-entry configured" } else { "provider-entry needs-credential" }, key: "{id}",
                            div { class: "provider-row",
                                span { class: "provider-avatar", aria_hidden: "true", "{mark}" }
                                div { class: "provider-row-identity",
                                    div { class: "provider-row-title", h2 { "{display_name}" } span { class: if credential_ready { "provider-row-badge ready" } else { "provider-row-badge" }, if credential_ready { "已配置" } else { "待配置" } } }
                                    if let Some(d) = &entry.definition { div { class: "provider-row-url", span { "{api_label(d.api)}" } code { "{d.base_url}" } } }
                                    else { div { class: "provider-row-url provider-row-muted", "尚未保存供应商定义" } }
                                }
                                div { class: "provider-row-actions",
                                    button { class: "icon-button provider-row-fetch", aria_label: "获取模型列表", title: "获取模型列表", disabled: discovery_busy.read().is_some() || !context.online || selected_session_id.is_none(), onclick: { let id = id.clone(); move |_| discover_models.call(id.clone()) }, Icon { name: "refresh" } }
                                    if let Some(d) = edit_def { button { class: "icon-button", aria_label: "编辑定义", title: "编辑定义", disabled: !enabled, onclick: move |_| edit.call(Edit::OpenDefinition(d.clone())), Icon { name: "edit" } } }
                                    if !entry.configured { button { class: "icon-button", aria_label: "添加自定义定义", title: "添加自定义定义", disabled: !enabled, onclick: { let id = id.clone(); move |_| { edit.call(Edit::OpenNew); edit.call(Edit::Id(id.clone())); } }, Icon { name: "plus" } } }
                                    button { class: "icon-button provider-row-key", aria_label: if stored == StoredCredential::None { "保存独立密钥" } else { "替换已存凭据为密钥" }, title: if stored == StoredCredential::None { "配置 API Key" } else { "替换 API Key" }, disabled: !enabled, onclick: { let id = id.clone(); move |_| { if stored == StoredCredential::None { edit.call(Edit::OpenKey(id.clone())); } else { edit.call(Edit::Confirm(Confirmation::ReplaceKey(id.clone()))); } } }, Icon { name: "shield" } }
                                    details { class: "provider-more-actions",
                                        summary { aria_label: "更多操作", title: "更多操作", Icon { name: "more" } }
                                        div { class: "provider-more-menu",
                                            button { class: "button", aria_label: "{clear_label(stored)}", disabled: !enabled || stored == StoredCredential::None, onclick: { let id = id.clone(); move |_| edit.call(Edit::Confirm(Confirmation::ClearStored(id.clone()))) }, "清除凭据" }
                                            button { class: "button", aria_label: "清除配置凭据", disabled: !enabled || !entry.config_key_present, onclick: { let id = id.clone(); move |_| edit.call(Edit::Confirm(Confirmation::ClearConfig(id.clone()))) }, "清除配置" }
                                            button { class: "button danger", aria_label: "删除自定义配置", disabled: !enabled || !entry.configured, onclick: { let id = id.clone(); move |_| edit.call(Edit::Confirm(Confirmation::Remove(id.clone()))) }, "删除定义" }
                                        }
                                    }
                                }
                            }
                            if let Some(d) = &entry.definition {
                                if !d.models.is_empty() { details { class: "provider-model-details", summary { "模型 · {d.models.len()} 个" }
                                    for m in &d.models { div { class: "provider-saved-model", strong { "{m.name}" } code { "{m.id}" } span { if m.reasoning { "支持推理" } else { "未声明推理" } } span { "上下文：{token_label(m.context_window)} · 最大输出：{token_label(m.max_tokens)}" } } }
                                } }
                            }
                            if let Some(diagnostic) = &entry.diagnostic { details { class: "provider-diagnostic", summary { "高级定义只读" } p { class: "provider-note", "{diagnostic}" } } }
                            if let Some(models) = discovered_models.read().get(&id) { details { class: "provider-discovered-models", open: !models.is_empty(), summary { "最近获取的模型 · {models.len()} 个" }
                                if models.is_empty() { p { class: "provider-meta", "当前会话没有返回该供应商的模型。" } }
                                for model in models { div { class: "provider-discovered-model-row", div { class: "provider-saved-model", strong { "{model.name}" } code { "{model.id}" } }, button { class: "button", disabled: discovery_busy.read().is_some(), onclick: { let model_id = model.id.clone(); move |_| use_discovered_model.call(model_id.clone()) }, "用于当前会话" } } }
                            } }
                        }
                      }
                    }
                }
            }
            div { class: "provider-section-heading provider-section-heading-muted", h2 { "可添加" } span { "· 0" } }
            p { class: "provider-empty-hint", "供应商目录由当前宿主提供；需要新增宿主未内置的供应商时，请先下载 Provider 扩展。" }
            if editor_value == Editor::Definition {
                Modal { id: "provider-definition", title: definition_title, class: "provider-dialog", trap_focus: true, onclose: move |()| edit.call(Edit::Close),
                    div { class: "modal-body provider-management",
                        p { class: "provider-note", "这里只保存供应商定义，不会保存或替换 API Key。已有供应商和模型 ID 不能直接改名；移除模型后保存会删除它的高级配置。" }
                        fieldset { disabled: !enabled,
                            legend { "供应商" }
                            if !host_definitions.is_empty() {
                                label { r#for: "provider-host-template", "宿主支持的供应商模板"
                                    select {
                                        id: "provider-host-template",
                                        aria_label: "宿主支持的供应商模板",
                                        onchange: {
                                            let templates = host_definitions.clone();
                                            move |event| {
                                                if event.value() == "custom" {
                                                    edit.call(Edit::OpenNew);
                                                } else if let Some(definition) = templates.iter().find(|item| item.id == event.value()) {
                                                    edit.call(Edit::UseHostDefinition(definition.clone()));
                                                }
                                            }
                                        },
                                        option { value: "custom", "自定义供应商" }
                                        for definition in host_definitions.iter() {
                                            {
                                                let label = if definition.name.is_empty() { definition.id.clone() } else { definition.name.clone() };
                                                rsx! { option { value: "{definition.id}", "{label} · {definition.id}" } }
                                            }
                                        }
                                    }
                                }
                            }
                            div { class: "provider-fields",
                                label { r#for: "provider-definition-id", "供应商 ID", input { id: "provider-definition-id", value: "{draft_value.id}", disabled: draft_value.existing, oninput: move |e| edit.call(Edit::Id(e.value())) } }
                                label { r#for: "provider-definition-name", "显示名称", input { id: "provider-definition-name", value: "{draft_value.name}", oninput: move |e| edit.call(Edit::Name(e.value())) } }
                                label { r#for: "provider-definition-api", "协议", select { id: "provider-definition-api", aria_label: "协议", value: api_label(draft_value.api), onchange: move |e| edit.call(Edit::Api(e.value())), option { value: "anthropic-messages", "anthropic-messages" } option { value: "openai-completions", "openai-completions" } option { value: "openai-responses", "openai-responses" } } }
                                label { r#for: "provider-definition-url", "Base URL", input { id: "provider-definition-url", r#type: "url", value: "{draft_value.base_url}", placeholder: "https://api.example.com/v1", oninput: move |e| edit.call(Edit::Endpoint(e.value())) } }
                            }
                            p { class: "provider-meta", "模型 Token 数留空为未指定；范围 1–100000000，最大输出不得大于上下文。最多 128 个模型。" }
                            for (index, model) in draft_value.models.iter().enumerate() {
                                fieldset { class: "provider-model-editor", key: "model-{index}",
                                    legend { "模型 {index + 1}" }
                                    div { class: "provider-fields",
                                        label { r#for: "provider-model-id-{index}", "模型 ID", input { id: "provider-model-id-{index}", value: "{model.id}", disabled: model.original_id.is_some(), oninput: move |e| edit.call(Edit::Model(index, "id", e.value())) } }
                                        label { r#for: "provider-model-name-{index}", "模型名称", input { id: "provider-model-name-{index}", value: "{model.name}", oninput: move |e| edit.call(Edit::Model(index, "name", e.value())) } }
                                        label { r#for: "provider-model-context-{index}", "上下文窗口（tokens）", input { id: "provider-model-context-{index}", inputmode: "numeric", value: "{model.context}", oninput: move |e| edit.call(Edit::Model(index, "context", e.value())) } }
                                        label { r#for: "provider-model-max-{index}", "最大输出（tokens）", input { id: "provider-model-max-{index}", inputmode: "numeric", value: "{model.max}", oninput: move |e| edit.call(Edit::Model(index, "max", e.value())) } }
                                    }
                                    label { class: "provider-checkbox", r#for: "provider-model-reasoning-{index}", input { id: "provider-model-reasoning-{index}", r#type: "checkbox", checked: model.reasoning, onchange: move |e| edit.call(Edit::Reasoning(index, e.checked())) } "支持推理" }
                                    button { class: "button", aria_label: "移除模型 {index + 1}", onclick: move |_| edit.call(Edit::RemoveModel(index)), "移除模型" }
                                }
                            }
                            div { class: "provider-model-actions",
                                if !draft_value.existing {
                                    button { class: "button provider-fetch-draft-models", aria_label: "从当前会话获取模型列表", title: "按供应商 ID 从当前会话获取模型列表并填入表单", disabled: discovery_busy.read().is_some() || !context.online || selected_session_id.is_none(), onclick: move |_| fetch_draft_models.call(()), Icon { name: "refresh" } if discovery_busy.read().is_some() { "获取中…" } else { "从当前会话获取模型列表" } }
                                }
                                button { class: "button", disabled: draft_value.models.len() >= 128, onclick: move |_| edit.call(Edit::AddModel), Icon { name: "plus" } "添加模型行" }
                            }
                        }
                        if !error.read().is_empty() { p { class: "provider-error", role: "alert", "{error}" } }
                    }
                    div { class: "modal-footer provider-actions",
                        button { class: "button", disabled: !enabled, onclick: move |_| edit.call(Edit::Close), "取消" }
                        button { class: "button primary", disabled: !enabled, onclick: move |_| save_definition.call(()), "仅保存定义" }
                    }
                }
            }
            if let Editor::Key(id) | Editor::NewKey(id) = &editor_value {
                Modal { id: "provider-key", title: "保存独立密钥", class: "provider-dialog", trap_focus: true, onclose: move |()| edit.call(Edit::Close),
                    div { class: "modal-body provider-management",
                        if matches!(&editor_value, Editor::NewKey(_)) {
                            label { r#for: "provider-credential-id", "凭据供应商 ID", input { id: "provider-credential-id", value: "{id}", disabled: !enabled, autocomplete: "off", spellcheck: "false", oninput: move |e| edit.call(Edit::NewKeyId(e.value())) } }
                            p { class: "provider-note", "输入节点使用的供应商 ID。这里不会联网验证；切换目标时会自动清空已输入的密钥。" }
                        }
                        p { "供应商：{id} · 仅写 auth.json，不修改 Base URL 或模型定义。" }
                        p { class: "provider-note", "密钥只写入节点，不会在客户端保存或回显。保存后不会验证远端认证。" }
                        label { r#for: "provider-secret", "新 API Key", input { id: "provider-secret", r#type: "password", autocomplete: "new-password", spellcheck: "false", value: "{secret}", disabled: !enabled, oninput: move |e| edit.call(Edit::Secret(e.value())) } }
                        if !error.read().is_empty() { p { class: "provider-error", role: "alert", "{error}" } }
                    }
                    div { class: "modal-footer provider-actions",
                        button { class: "button", disabled: !enabled, onclick: move |_| edit.call(Edit::Close), "取消并清空" }
                        button { class: "button primary", disabled: !enabled || secret.read().is_empty(), onclick: move |_| submit_key.call(()), "仅保存 API Key" }
                    }
                }
            }
            if let Some(c) = confirmation.read().as_ref() {
                Modal { id: "provider-confirmation", title: "确认节点文件操作", class: "provider-dialog", trap_focus: true, onclose: move |()| edit.call(Edit::CancelConfirm),
                    div { class: "modal-body provider-management", strong { "供应商：{c.id()}" } p { "{c.message()}" } p { "不会立即更改活动宿主或验证远端认证。" } }
                    div { class: "modal-footer provider-actions",
                        button { class: "button", disabled: !enabled, onclick: move |_| edit.call(Edit::CancelConfirm), "取消" }
                        button { class: "button danger", disabled: !enabled, onclick: move |_| confirm_action.call(()), "确认并继续" }
                    }
                }
            }
        }
    }
}
fn api_label(api: ProviderApi) -> &'static str {
    match api {
        ProviderApi::AnthropicMessages => "anthropic-messages",
        ProviderApi::OpenaiCompletions => "openai-completions",
        ProviderApi::OpenaiResponses => "openai-responses",
    }
}
fn token_label(n: Option<u64>) -> String {
    n.map_or_else(|| "未指定".into(), |n| n.to_string())
}
fn clear_label(c: StoredCredential) -> &'static str {
    match c {
        StoredCredential::OAuth => "清除已存 OAuth",
        StoredCredential::ApiKey => "清除已存 API Key",
        _ => "清除已存凭据",
    }
}
// A local deadline never cancels a node-side write. Keep pending until an
// explicit fresh read after any uncertain result; do not automatically retry.
async fn bounded<T>(
    future: impl std::future::Future<Output = Result<T, NodeError>>,
) -> Result<T, NodeError> {
    use std::{future::poll_fn, pin::pin, task::Poll};
    let mut future = pin!(future);
    let mut deadline = pin!(async {
        #[cfg(target_arch = "wasm32")]
        gloo_timers::future::TimeoutFuture::new(15_000).await;
        #[cfg(not(target_arch = "wasm32"))]
        tokio::time::sleep(std::time::Duration::from_secs(15)).await;
    });
    poll_fn(move |cx| {
        if let Poll::Ready(value) = future.as_mut().poll(cx) {
            return Poll::Ready(value);
        }
        if deadline.as_mut().poll(cx).is_ready() {
            return Poll::Ready(Err(NodeError::new(
                NodeErrorKind::Timeout,
                "provider operation outcome unknown",
            )));
        }
        Poll::Pending
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use work_core::api::ProviderEntry;
    fn definition() -> ProviderDefinition {
        ProviderDefinition {
            id: "example".into(),
            name: "Example".into(),
            api: ProviderApi::OpenaiResponses,
            base_url: "https://example.com/v1".into(),
            models: vec![ProviderModel {
                id: "model-1".into(),
                name: "Model".into(),
                reasoning: true,
                context_window: Some(100),
                max_tokens: Some(20),
            }],
        }
    }
    fn catalog() -> ProviderCatalog {
        ProviderCatalog {
            revision: "opaque-owner-revision".into(),
            providers: vec![ProviderEntry {
                id: "example".into(),
                definition: Some(definition()),
                configured: true,
                config_key_present: true,
                stored_credential: StoredCredential::OAuth,
                diagnostic: None,
            }],
        }
    }
    #[test]
    fn provider_freshness_rejects_reconnect_session_offline_and_late_ticket() {
        let scope = ProviderContext {
            epoch: 3,
            generation: 4,
            session: Some("one".into()),
            online: true,
        };
        assert!(fresh(&scope, &scope, 7, 7));
        for changed in [
            ProviderContext {
                epoch: 4,
                ..scope.clone()
            },
            ProviderContext {
                generation: 5,
                ..scope.clone()
            },
            ProviderContext {
                session: Some("two".into()),
                ..scope.clone()
            },
            ProviderContext {
                online: false,
                ..scope.clone()
            },
        ] {
            assert!(!fresh(&scope, &changed, 7, 7));
        }
        assert!(!fresh(&scope, &scope, 6, 7));
    }
    #[test]
    fn provider_write_gate_fails_closed_for_every_missing_precondition() {
        for online in [false, true] {
            for observed in [false, true] {
                for busy in [false, true] {
                    for pending in [false, true] {
                        assert_eq!(
                            write_gate(online, observed, busy, pending),
                            online && observed && !busy && !pending
                        );
                    }
                }
            }
        }
        assert!(!write_gate(true, false, false, false));
    }
    #[test]
    fn provider_token_parser_rejects_negative_fraction_overflow_zero() {
        assert_eq!(parse_tokens("").unwrap(), None);
        assert_eq!(parse_tokens(" 128 ").unwrap(), Some(128));
        for bad in [
            "0",
            "-1",
            "+1",
            "1.5",
            "1e3",
            "100000001",
            "18446744073709551616",
            "NaN",
        ] {
            assert!(parse_tokens(bad).is_err(), "{bad}");
        }
    }
    #[test]
    fn provider_draft_roundtrips_and_validates_models() {
        let mut draft = DefinitionDraft::from(definition());
        assert_eq!(draft.parse().unwrap(), definition());
        draft.models.push(draft.models[0].clone());
        assert!(draft.parse().is_err());
        draft.models.pop();
        draft.models[0].max = "101".into();
        assert!(draft.parse().is_err());
        draft.models[0].max = "20".into();
        draft.models[0].id = "renamed-model".into();
        assert!(
            draft.parse().is_err(),
            "existing model identity must not silently lose advanced fields"
        );
        draft.models[0].id = " ".into();
        assert!(draft.parse().is_err());
        for url in [
            "file:///tmp",
            "https://user:pass@example.com",
            "https://example.com?token=bad",
            "https://example.com/#x",
            "https://",
            "https://exa mple.com",
        ] {
            assert!(!valid_endpoint(url), "{url}");
        }
        assert!(valid_endpoint("http://localhost:1234/v1"));
        for id in ["", ".", "..", "a/b", "white space"] {
            assert!(!valid_id(id));
        }
    }
    #[test]
    fn provider_expected_readback_requires_exact_definition_and_revision() {
        let mut c = catalog();
        let change = ExpectedChange::Definition(definition());
        assert!(expected_readback(&change, &c.revision, &c));
        assert!(!expected_readback(&change, "another-owner-revision", &c));
        c.providers[0].definition.as_mut().unwrap().models[0].reasoning = false;
        assert!(!expected_readback(&change, &c.revision, &c));
        assert!(!ExpectedChange::Removed("example".into()).matches(&c));
        c.providers[0].configured = false;
        c.providers[0].definition = None;
        assert!(ExpectedChange::Removed("example".into()).matches(&c));
        assert!(!ExpectedChange::KeyPresent("example".into()).matches(&c));
        c.providers[0].stored_credential = StoredCredential::ApiKey;
        assert!(ExpectedChange::KeyPresent("example".into()).matches(&c));
        assert!(!ExpectedChange::Cleared("example".into(), CredentialLocation::Stored).matches(&c));
        c.providers[0].stored_credential = StoredCredential::None;
        assert!(ExpectedChange::Cleared("example".into(), CredentialLocation::Stored).matches(&c));
        assert!(!ExpectedChange::Cleared("example".into(), CredentialLocation::Config).matches(&c));
        c.providers[0].config_key_present = false;
        assert!(ExpectedChange::Cleared("example".into(), CredentialLocation::Config).matches(&c));
        c.providers.clear();
        assert!(ExpectedChange::Removed("example".into()).matches(&c));
        assert!(!ExpectedChange::KeyPresent("example".into()).matches(&c));
    }
}
