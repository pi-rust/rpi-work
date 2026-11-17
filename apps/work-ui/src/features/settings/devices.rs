use super::ConnectionScreen;
use crate::app::connections::{
    ConnectionStore, NodeHistory, NodePassword, Phase, normalize_address, normalize_remote_address,
};
use crate::shared::{
    app_settings::{self, AppSettingsState, DEFAULT_REMOTE_PORT},
    components::Icon,
};
use dioxus::core::Task;
use dioxus::prelude::*;
use work_client::{AuthState, Capabilities, NodeProbe, WorkspaceClient};

#[path = "authorization_offer.rs"]
mod authorization_offer;

#[derive(Clone, Copy)]
struct PairingControl {
    sharing: Signal<DesktopSharing>,
    request: Signal<authorization_offer::Request>,
    address_draft: Signal<String>,
}

/// One workspace-lifetime instance, shared by startup and all Settings layouts.
pub(crate) fn use_pairing() {
    let mut sharing = use_signal(DesktopSharing::default);
    use_effect(move || {
        spawn(async move {
            let mut events = document::eval(
                "window.addEventListener('rpi-component-state-changed',()=>dioxus.send(true));",
            );
            sharing.set(desktop_sharing().await);
            while events.recv::<bool>().await.is_ok() {
                sharing.set(desktop_sharing().await);
            }
        });
    });
    let request = use_signal(authorization_offer::Request::default);
    let address_draft = use_signal(String::new);
    use_context_provider(|| PairingControl {
        sharing,
        request,
        address_draft,
    });
}

#[component]
pub(crate) fn DesktopPairing(
    shared_client: Signal<Option<WorkspaceClient>>,
    base_url: Signal<String>,
    auth_state: Signal<Option<AuthState>>,
    view: String,
) -> Element {
    let control = use_context::<PairingControl>();
    let connection = use_context::<ConnectionStore>();
    let owned_local = crate::app::local_node::is_local(&connection.address.read());
    let sharing = control.sharing.read();
    let available = matches!(*connection.phase.read(), Phase::Online)
        && if owned_local {
            crate::app::local_node::client().is_some() && sharing.listening && sharing.enabled
        } else {
            auth_state
                .read()
                .as_ref()
                .is_some_and(|auth| auth.authenticated)
                && matches!(
                    shared_client.read().as_ref(),
                    Some(WorkspaceClient::Remote(_))
                )
        };
    rsx! {
        // The modal also exists before the Settings page is ever mounted.
        style { {include_str!("authorization_qr.css")} }
        AuthorizationQr {
            shared_client, base_url, owned_local, available,
            public_url: sharing.app_url.clone(),
            address_draft: control.address_draft,
            // Do not open an authorization QR modal just by entering the page;
            // generate it only after the user explicitly requests sharing.
            automatic: false,
            request: control.request,
            view,
        }
    }
}

const QR_LIBRARY: &str = include_str!("../../../../../assets/vendor/qrcode.js");
const QR_DECODER: &str = include_str!("../../../../../assets/vendor/jsQR.js");
const QR_FRAME_DECODER: &str = r"
let qrCanvas,qrContext;
const decodeFrame=source=>{
    const w=source.videoWidth||source.width,h=source.videoHeight||source.height;
    if(!w||!h)return '';
    const scale=Math.min(1,1200/Math.max(w,h));
    qrCanvas ||= document.createElement('canvas');
    const width=Math.max(1,Math.round(w*scale)),height=Math.max(1,Math.round(h*scale));
    if(qrCanvas.width!==width)qrCanvas.width=width;
    if(qrCanvas.height!==height)qrCanvas.height=height;
    qrContext ||= qrCanvas.getContext('2d',{willReadFrequently:true});
    qrContext.drawImage(source,0,0,width,height);
    const pixels=qrContext.getImageData(0,0,width,height);
    return window.jsQR(pixels.data,width,height)?.data||'';
};";

fn connection_address(raw: &str, port: u16) -> Result<String, String> {
    let raw = raw.trim();
    if raw.len() > 4096 {
        return Err("连接信息过长。".into());
    }
    if raw.starts_with('{') {
        let value: serde_json::Value =
            serde_json::from_str(raw).map_err(|_| "无法解析地址二维码。")?;
        let object = value.as_object().ok_or("地址二维码必须是 JSON 对象。")?;
        if object.len() != 3
            || value["v"].as_u64() != Some(1)
            || value["type"].as_str() != Some("rpi-work-node-address")
        {
            return Err("二维码类型或版本不受支持，请扫描节点地址二维码。".into());
        }
        normalize_remote_address(
            value["address"].as_str().ok_or("二维码缺少节点地址。")?,
            true,
            port,
        )
    } else {
        normalize_remote_address(raw, true, port)
    }
}

#[derive(Clone, Debug, PartialEq)]
struct NodePreview {
    endpoint: String,
    node_id: String,
    protocol: String,
    version: u32,
    health: String,
    authentication: String,
    transports: String,
}

impl NodePreview {
    fn from_probe(probe: NodeProbe) -> Self {
        let transports = probe
            .discovery
            .bindings
            .iter()
            .map(|binding| {
                format!(
                    "{:?} {}",
                    binding.transport.kind,
                    binding.transport.path.as_deref().unwrap_or("/")
                )
            })
            .collect::<Vec<_>>()
            .join(" · ");
        let node_id = probe.node_id().to_owned();
        Self {
            endpoint: probe.endpoint,
            node_id,
            protocol: probe.discovery.protocol.to_string(),
            version: probe.discovery.version,
            health: if probe.health.ok {
                "健康检查通过"
            } else {
                "节点报告异常"
            }
            .into(),
            authentication: if probe.auth.required {
                if probe.auth.authenticated {
                    "已认证"
                } else {
                    "需要节点密码"
                }
            } else {
                "无需密码"
            }
            .into(),
            transports,
        }
    }
}

async fn probe_work_node(address: &str) -> Result<NodePreview, String> {
    NodeProbe::probe(address)
        .await
        .map(NodePreview::from_probe)
        .map_err(|error| error.to_string())
}

#[cfg(not(target_arch = "wasm32"))]
async fn discover_work_nodes() -> Result<Vec<NodePreview>, String> {
    work_client::discover_nodes()
        .await
        .map(|nodes| {
            nodes
                .into_iter()
                .map(|node| NodePreview::from_probe(node.probe))
                .collect()
        })
        .map_err(|error| error.to_string())
}

#[cfg(target_arch = "wasm32")]
async fn discover_work_nodes() -> Result<Vec<NodePreview>, String> {
    Err("浏览器无法直接发现局域网节点，请输入节点地址。".into())
}

#[cfg(any(target_arch = "wasm32", target_os = "android"))]
#[derive(Clone, Default)]
struct DesktopSharing {
    listening: bool,
    enabled: bool,
    public_url: String,
    app_url: String,
    address_hint: String,
    status: String,
    warning: String,
    web: DesktopWeb,
}
#[cfg(any(target_arch = "wasm32", target_os = "android"))]
#[derive(Clone, Default)]
struct DesktopWeb {
    listening: bool,
    enabled: bool,
    url: String,
    status: String,
}
#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
type DesktopSharing = crate::app::local_http::Sharing;
async fn desktop_sharing() -> DesktopSharing {
    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    {
        crate::app::local_http::sharing().await
    }
    #[cfg(any(target_arch = "wasm32", target_os = "android"))]
    {
        DesktopSharing::default()
    }
}

async fn set_desktop_access(web: bool, enabled: bool) -> Result<(), String> {
    let client = crate::app::local_node::client().ok_or("本机节点尚未准备好".to_owned())?;
    crate::app::connections::with_timeout(client.component_capability(
        if web {
            "node.web.write"
        } else {
            "node.sharing.write"
        },
        serde_json::json!({"enabled": enabled}),
    ))
    .await?
    .map(|_| ())
    .map_err(|error| error.to_string())
}

#[component]
fn LocalAccessCards(
    mut sharing: Signal<DesktopSharing>,
    available: bool,
    onshare: EventHandler<()>,
) -> Element {
    let mut busy = use_signal(|| false);
    let mut error = use_signal(String::new);
    let mut guide = use_signal(|| false);
    let snapshot = sharing.read().clone();
    let mut change = move |web: bool| {
        if *busy.peek() || !available {
            return;
        }
        let enabled = if web {
            !sharing.peek().web.enabled
        } else {
            !sharing.peek().enabled
        };
        busy.set(true);
        error.set(String::new());
        spawn(async move {
            if let Err(message) = set_desktop_access(web, enabled).await {
                error.set(message);
            }
            sharing.set(desktop_sharing().await);
            busy.set(false);
        });
    };
    rsx! {
        section { class: "collaboration-action-card assist-card", aria_label: "允许远程协作",
            div { class: "collaboration-action-heading",
                span { class: "collaboration-action-icon assist", Icon { name: "shield" } }
                div { h2 { "允许远程协作" } p { "授权其他设备连接这台电脑，继续处理项目、会话和任务。" } }
                button {
                    class: if snapshot.enabled { "sharing-switch enabled" } else { "sharing-switch" },
                    role: "switch", aria_label: "允许远程协作", aria_checked: snapshot.enabled,
                    disabled: !available || !snapshot.listening || *busy.read(),
                    onclick: move |_| change(false),
                    span { class: "sharing-switch-track", span {} }
                    span { if snapshot.enabled { "已开启" } else { "未开启" } }
                }
            }
            if !available {
                p { class: "connection-help", "请在拥有该工作节点的电脑上管理远程协作。" }
            } else if !snapshot.listening {
                p { class: "connection-form-error", role: "alert", "{snapshot.status}" }
            } else {
                div { class: "sharing-ready",
                    Icon { name: if snapshot.enabled { "check" } else { "info" } }
                    span { if snapshot.enabled { "远程协作已开启，可以分享连接信息。" } else { "节点入口已就绪，开启后可授权其他设备。" } }
                }
                if snapshot.enabled {
                    button { class: "button", onclick: move |_| onshare.call(()), "分享连接信息" }
                }
            }
            if !snapshot.warning.is_empty() {
                p { class: "connection-form-error", role: "alert", "{snapshot.warning}" }
            }
        }
        section { class: "collaboration-action-card web-access-card", aria_label: "允许 Web 访问",
            div { class: "collaboration-action-heading",
                span { class: "collaboration-action-icon", Icon { name: "server" } }
                div { h2 { "允许 Web 访问" } p { "使用 Work 自带的网页界面，在浏览器中处理同一节点的工作。" } }
                button {
                    class: if snapshot.web.enabled { "sharing-switch enabled" } else { "sharing-switch" },
                    role: "switch", aria_label: "允许 Web 访问", aria_checked: snapshot.web.enabled,
                    disabled: !available || !snapshot.web.listening || *busy.read(),
                    onclick: move |_| change(true),
                    span { class: "sharing-switch-track", span {} }
                    span { if snapshot.web.enabled { "已开启" } else { "未开启" } }
                }
            }
            if available {
                if !snapshot.web.listening {
                    p { class: "connection-form-error", role: "alert", "{snapshot.web.status}" }
                    button { class: "button", onclick: move |_| guide.toggle(), if *guide.read() { "收起指引" } else { "查看准备指引" } }
                    if *guide.read() {
                        p { class: "connection-help", "安装包含 Web 资源的完整 Work 桌面包，或将 Work Web 文件放在程序旁的 web 目录，然后重新启动 Work。自定义部署可通过 RPI_WORK_HTTP_ASSETS 指定目录。" }
                    }
                } else {
                    p { class: "connection-help", if snapshot.web.enabled { "Web 访问已开启，浏览器仍需登录节点。" } else { "Web 入口已就绪，开启后可使用浏览器访问。" } }
                    if snapshot.web.enabled && !snapshot.web.url.is_empty() {
                        a { class: "button web-entry-button", href: "{snapshot.web.url}", target: "_blank", rel: "noopener noreferrer", aria_label: "打开 Web 入口", title: "在浏览器中打开 Web 入口", "打开 Web 入口" }
                        code { class: "web-entry-address", "{snapshot.web.url}" }
                    }
                    if !snapshot.web.status.is_empty() {
                        p { class: "connection-form-error", role: "alert", "{snapshot.web.status}" }
                    }
                }
            } else {
                p { class: "connection-help", "请在拥有该工作节点的电脑上管理 Web 访问。" }
            }
        }
        if available {
            div { class: "collaboration-host-actions",
                button { class: "button", disabled: *busy.read(), onclick: move |_| {
                    busy.set(true);
                    error.set(String::new());
                    spawn(async move {
                        sharing.set(desktop_sharing().await);
                        busy.set(false);
                    });
                }, Icon { name: "refresh" } "重新检测入口" }
                crate::app::desktop_background::BackgroundRunButton {}
                if !error.read().is_empty() {
                    p { class: "connection-form-error", role: "alert", "{error}" }
                }
            }
        }
    }
}

fn phone_share_address(value: &str, owned_local: bool) -> Result<String, String> {
    if value.trim().is_empty() {
        return Err("尚未配置可分享地址，无法生成二维码。请填写手机可达的完整 HTTP(S) 地址，并确认该入口已启动。".into());
    }
    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    if owned_local {
        return work_host::http::public_address(value)
            .map(|(url, _)| url.trim_end_matches('/').to_owned());
    }
    let _ = owned_local;
    normalize_address(value, true)
}

fn qr_payload(address: &str) -> String {
    serde_json::json!({ "v": 1, "type": "rpi-work-node-address", "address": address }).to_string()
}

// Pure cleanup: never send a bool into the scanner's String channel.
const CAMERA_CLEANUP: &str = "window.__rpiWorkCameraEpoch=(window.__rpiWorkCameraEpoch||0)+1;window.__rpiWorkCamera?.finish('');";

fn stop_camera_script() -> String {
    format!("{CAMERA_CLEANUP}dioxus.send(true);")
}

fn start_camera_script() -> String {
    format!(
        "{QR_DECODER};{QR_FRAME_DECODER}\n{CAMERA_CLEANUP}\n{}",
        include_str!("device_camera.js")
    )
}

fn album_script(native_picker: bool) -> String {
    let file_source = if native_picker {
        r"const file=await new Promise(resolve=>{
    window.__rpiWorkQrAlbum?.cancel();
    const input=document.createElement('input');
    input.type='file';input.accept='image/*';input.hidden=true;
    let done=false;
    const finish=file=>{if(done)return;done=true;input.remove();if(window.__rpiWorkQrAlbum?.input===input)window.__rpiWorkQrAlbum=null;resolve(file);};
    input.onchange=()=>finish(input.files?.[0]||null);
    input.oncancel=()=>finish(null);
    window.__rpiWorkQrAlbum={input,cancel:()=>finish(null)};
    // Outside the Dioxus root, so its desktop-only file-dialog interceptor does
    // not consume this input. Wry opens Android's system photo picker instead.
    document.body.appendChild(input);input.click();
});"
    } else {
        "const file=document.getElementById('device-qr-image')?.files?.[0];document.getElementById('device-qr-image').value='';"
    };
    format!(
        r"{QR_DECODER};{QR_FRAME_DECODER}
                                    {CAMERA_CLEANUP}
                                    {file_source}
                                    let bitmap;
                                    try {{
                                        if(!file) {{ dioxus.send(''); }}
                                        else {{
                                            if(file.size>10*1024*1024)throw new Error('image too large');
                                            bitmap=await createImageBitmap(file);
                                            dioxus.send(decodeFrame(bitmap)||'ERR:图片中未找到二维码，请重新选择或重新扫码。');
                                        }}
                                    }} catch(_) {{ dioxus.send('ERR:无法识别图片，请选择清晰的地址二维码。'); }}
                                    finally {{ bitmap?.close(); }}"
    )
}

/// Device-collaboration advanced settings. The remote connection port is a
/// device-local preference: it completes remote addresses that omit an HTTP
/// port and can also be written directly as `connection.remotePort` in the
/// client settings JSON.
#[component]
fn CollaborationAdvancedSettings(
    store: Option<Signal<AppSettingsState>>,
    default_port: u16,
) -> Element {
    let mut draft = use_signal(|| default_port.to_string());
    let mut error = use_signal(String::new);
    let mut saved = use_signal(|| false);
    use_effect(use_reactive((&default_port,), move |(port,)| {
        if draft.peek().trim() != port.to_string() {
            draft.set(port.to_string());
            saved.set(false);
        }
    }));
    let editable = store.is_some();
    let storage = app_settings::storage_label();
    let apply = move |port: u16| -> Result<(), String> {
        let Some(store) = store else {
            return Err("此设备无法保存全局设置。".into());
        };
        if !store.peek().ready {
            return Err("全局设置尚未加载完成，请稍后重试。".into());
        }
        if app_settings::update_remote_port(store, port).is_none() {
            return Err("无法保存端口：请先在高级设置中修正全局 JSON。".into());
        }
        app_settings::request_write(store);
        Ok(())
    };
    let mut commit = move |port: u16| match apply(port) {
        Ok(()) => {
            error.set(String::new());
            saved.set(true);
            draft.set(port.to_string());
        }
        Err(message) => {
            saved.set(false);
            error.set(message);
        }
    };
    let mut save = move |(): ()| {
        let raw = draft.read().trim().to_string();
        match raw.parse::<u16>().ok().filter(|port| *port != 0) {
            Some(port) => commit(port),
            None => {
                saved.set(false);
                error.set("端口必须是 1 到 65535 之间的整数。".into());
            }
        }
    };
    let mut reset_default = move |(): ()| commit(app_settings::DEFAULT_REMOTE_PORT);
    rsx! {
        details { class: "collaboration-advanced", aria_label: "连接高级设置",
            summary { Icon { name: "server" } span { "高级设置" } small { "远程连接端口" } }
            div { class: "collaboration-advanced-body",
                div { class: "collaboration-advanced-row",
                    label { r#for: "remote-connection-port",
                        strong { "远程连接默认端口" }
                        small { "只补全未写端口的 HTTP 地址；已写端口或 HTTPS 地址保持不变。" }
                    }
                    div { class: "collaboration-port-control",
                        input {
                            id: "remote-connection-port",
                            r#type: "number",
                            min: "1",
                            max: "65535",
                            inputmode: "numeric",
                            autocomplete: "off",
                            value: "{draft}",
                            disabled: !editable,
                            oninput: move |event| { draft.set(event.value()); error.set(String::new()); saved.set(false); },
                        }
                        button { class: "button primary", r#type: "button", disabled: !editable, onclick: move |_| save(()), "保存端口" }
                        button { class: "button", r#type: "button", disabled: !editable, onclick: move |_| reset_default(()), "恢复默认" }
                    }
                }
                p { class: "collaboration-advanced-hint",
                    "例如输入 http://10.100.100.9 会按当前设置补全为 http://10.100.100.9:{default_port}。"
                }
                p { class: "collaboration-advanced-hint",
                    "也可直接编辑全局设置 JSON 的 "
                    code { "connection.remotePort" }
                    " · {storage}"
                }
                if !editable { p { class: "collaboration-advanced-hint", "当前布局无法读取本设备全局设置。" } }
                if !error.read().is_empty() { p { class: "connection-form-error", role: "alert", "{error}" } }
                if *saved.read() { p { class: "collaboration-advanced-saved", role: "status", "已保存远程连接端口 {draft}。" } }
            }
        }
    }
}

#[component]
pub fn DevicesPage(
    shared_client: Signal<Option<WorkspaceClient>>,
    requests: Signal<Vec<Task>>,
    base_url: Signal<String>,
    status: Signal<String>,
    auth_state: Signal<Option<AuthState>>,
    password: Signal<NodePassword>,
    capabilities: Signal<Option<Capabilities>>,
    onconnect: EventHandler<()>,
    onlogin: EventHandler<()>,
    mobile_client: bool,
) -> Element {
    let mut connection = use_context::<ConnectionStore>();
    // Device-collaboration advanced settings own the remote connection port.
    let settings_store = use_hook(try_consume_context::<Signal<AppSettingsState>>);
    let remote_port = settings_store.map_or(DEFAULT_REMOTE_PORT, |store| {
        let state = store.read();
        app_settings::remote_port(&state.value)
    });
    let mut qr_open = use_signal(|| false);
    let mut pairing = use_context::<PairingControl>();
    let mut remote_open = use_signal(|| false);
    let mut manual_open = use_signal(|| true);
    let mut manual_address = use_signal(String::new);
    let mut phone_address = use_signal(String::new);
    let mut error = use_signal(String::new);
    let mut scanner_status = use_signal(String::new);
    let mut scanning = use_signal(|| false);
    let mut camera_epoch = use_signal(|| 0u64);
    let mut copy_status = use_signal(String::new);
    let mut share_error = use_signal(String::new);
    let mut share_epoch = use_signal(|| 0u64);
    let mut discovery_results = use_signal(Vec::<NodePreview>::new);
    let mut discovery_busy = use_signal(|| false);
    let mut discovery_status = use_signal(String::new);
    let mut preview = use_signal(|| None::<NodePreview>);
    let mut preview_busy = use_signal(|| false);
    let mut preview_epoch = use_signal(|| 0u64);
    let local_sharing = pairing.sharing;
    let sharing_state = local_sharing;
    let sharing = local_sharing.read().clone();
    let owns_desktop = crate::app::local_node::client().is_some();
    let current_address = connection.address.read().clone();
    let connection_epoch = *connection.epoch.read();
    let initial_address = current_address.clone();
    let initial_share = sharing.public_url.clone();
    let mut advertised_address = use_signal(move || {
        if crate::app::local_node::is_local(&initial_address) {
            initial_share
        } else {
            initial_address.trim().trim_end_matches('/').to_owned()
        }
    });
    let address = advertised_address
        .read()
        .trim()
        .trim_end_matches('/')
        .to_owned();
    let phase = connection.phase.read().clone();
    let busy = phase.busy();
    let online = matches!(phase, Phase::Online);
    let owned_local = crate::app::local_node::is_local(&current_address)
        || (current_address == "about:blank" && crate::app::local_node::startup_error().is_some());
    let share_available = if owned_local {
        sharing.listening && sharing.enabled
    } else {
        online
    };
    let share_title = if owned_local {
        "手机连接这台电脑"
    } else {
        "分享连接信息"
    };
    let share_address_valid = phone_share_address(&address, owned_local).is_ok();
    let connection_status = if owned_local {
        match &phase {
            Phase::Online => "本机运行中",
            Phase::Idle | Phase::Connecting | Phase::Authenticating => "正在准备本机工作空间…",
            Phase::PasswordRequired => "本机认证正在恢复…",
            Phase::Failed(message) => message,
        }
    } else {
        phase.label()
    }
    .to_owned();
    let local_public_url = sharing.public_url.clone();
    use_effect(use_reactive(
        (&current_address, &connection_epoch),
        move |(node, _epoch)| {
            qr_open.set(false);
            share_epoch += 1;
            copy_status.set(String::new());
            share_error.set(String::new());
            advertised_address.set(if crate::app::local_node::is_local(&node) {
                local_public_url.clone()
            } else {
                node.trim().trim_end_matches('/').to_owned()
            });
        },
    ));
    use_effect(use_reactive((&mobile_client,), move |(_mobile,)| {
        qr_open.set(false);
        manual_open.set(true);
        manual_address.set(String::new());
        scanning.set(false);
        camera_epoch += 1;
        phone_address.set(String::new());
        scanner_status.set(String::new());
        error.set(String::new());
        let _ = document::eval(&format!(
            "{CAMERA_CLEANUP}window.__rpiWorkQrAlbum?.cancel();"
        ));
    }));
    // Editing the ordinary share address invalidates any outstanding delegation.
    use_effect(move || {
        let draft = advertised_address.read().clone();
        if *pairing.address_draft.peek() != draft {
            pairing.address_draft.set(draft);
        }
    });
    let mut open_share = move |(): ()| {
        pairing
            .request
            .with_mut(authorization_offer::Request::close);
        // Use the connected node, never the unsubmitted connection form draft.
        advertised_address.set(if owned_local {
            local_sharing.read().public_url.clone()
        } else {
            connection
                .address
                .read()
                .trim()
                .trim_end_matches('/')
                .to_owned()
        });
        share_epoch += 1;
        copy_status.set(String::new());
        share_error.set(String::new());
        qr_open.set(true);
    };
    let mut copy_share = move |as_address: bool| {
        let Ok(value) = phone_share_address(&advertised_address.read(), owned_local) else {
            copy_status.set("请先填写对方可访问的有效地址。".into());
            return;
        };
        let payload = if as_address {
            value
        } else {
            qr_payload(&value)
        };
        let attempt = *share_epoch.peek();
        let node_epoch = *connection.epoch.peek();
        let script = format!(
            "try{{if(!navigator.clipboard?.writeText)throw new Error();await navigator.clipboard.writeText({});dioxus.send(true);}}catch(_){{dioxus.send(false);}}",
            serde_json::to_string(&payload).expect("JSON strings serialize")
        );
        let mut evaluator = document::eval(&script);
        spawn(async move {
            let copied = matches!(evaluator.recv::<bool>().await, Ok(true));
            if *share_epoch.peek() != attempt
                || *connection.epoch.peek() != node_epoch
                || !*qr_open.peek()
            {
                return;
            }
            copy_status.set(
                if !copied {
                    "无法自动复制，请手动复制上方地址或连接信息。"
                } else if as_address {
                    "已复制连接地址。"
                } else {
                    "已复制连接信息。"
                }
                .into(),
            );
        });
    };
    let mut connect_address = move |(): ()| {
        let candidate = phone_address.read().clone();
        match connection_address(&candidate, remote_port) {
            Ok(value) => {
                let endpoint = format!("{value}/");
                preview_epoch += 1;
                let attempt = *preview_epoch.read();
                preview_busy.set(true);
                preview.set(None);
                error.set(String::new());
                scanner_status.set("正在读取节点信息…".into());
                spawn(async move {
                    let result = probe_work_node(&endpoint).await;
                    if *preview_epoch.peek() != attempt {
                        return;
                    }
                    preview_busy.set(false);
                    match result {
                        Ok(candidate) => {
                            phone_address.set(candidate.endpoint.clone());
                            preview.set(Some(candidate));
                            scanner_status.set("已读取节点信息，请核对后确认连接。".into());
                        }
                        Err(message) => scanner_status.set(format!("无法读取节点信息：{message}")),
                    }
                });
            }
            Err(message) => error.set(message),
        }
    };
    let mut discover_lan = move |(): ()| {
        if *discovery_busy.peek() {
            return;
        }
        preview_epoch += 1;
        discovery_busy.set(true);
        discovery_status.set("正在搜索局域网节点…".into());
        discovery_results.set(Vec::new());
        let attempt = *preview_epoch.read();
        spawn(async move {
            let result = discover_work_nodes().await;
            if *preview_epoch.peek() != attempt {
                return;
            }
            discovery_busy.set(false);
            match result {
                Ok(nodes) if nodes.is_empty() => {
                    discovery_status.set(
                        "未发现可用节点。请确认节点已开启远程协作，并检查局域网或防火墙设置。"
                            .into(),
                    );
                }
                Ok(nodes) => {
                    discovery_status.set(format!("发现 {} 个可用节点。", nodes.len()));
                    discovery_results.set(nodes);
                }
                Err(message) => discovery_status.set(message),
            }
        });
    };
    use_drop(move || {
        let _ = document::eval(&format!(
            "{CAMERA_CLEANUP}window.__rpiWorkQrAlbum?.cancel();"
        ));
    });
    use_effect(move || {
        if !*qr_open.read() || !share_available {
            return;
        }
        let address = advertised_address.read().clone();
        let attempt = *share_epoch.read();
        let node_epoch = *connection.epoch.peek();
        match phone_share_address(&address, owned_local) {
            Ok(address) => {
                share_error.set(String::new());
                spawn(async move {
                    let generated = matches!(
                        document::eval(&qr_script(&address)).recv::<bool>().await,
                        Ok(true)
                    );
                    if *share_epoch.peek() != attempt
                        || *connection.epoch.peek() != node_epoch
                        || !*qr_open.peek()
                    {
                        return;
                    }
                    if !generated {
                        share_error.set("无法生成地址二维码，请复制连接信息。".into());
                    }
                });
            }
            Err(message) => {
                share_error.set(message);
                let _ = document::eval(
                    "document.getElementById('device-address-qr-code')?.replaceChildren();",
                );
            }
        }
    });

    let mut read_album = move |native_picker: bool| {
        manual_open.set(false);
        password.set(NodePassword::default());
        error.set(String::new());
        phone_address.set(String::new());
        scanning.set(false);
        camera_epoch += 1;
        let epoch = *camera_epoch.read();
        scanner_status.set("正在识别图片…".into());
        let script = album_script(native_picker);
        let mut evaluator = document::eval(&script);
        spawn(async move {
            let result = evaluator.recv::<String>().await;
            if *camera_epoch.read() != epoch {
                return;
            }
            match result {
                Ok(value) if value.starts_with("ERR:") => scanner_status.set(value[4..].to_owned()),
                Ok(value) if !value.is_empty() => match connection_address(&value, remote_port) {
                    Ok(address) => {
                        password.set(NodePassword::default());
                        phone_address.set(address);
                        scanner_status.set("已读取图片地址，请核对后连接。".into());
                    }
                    Err(message) => scanner_status.set(message),
                },
                Ok(_) => scanner_status.set("未选择图片。".into()),
                Err(_) => scanner_status.set("图片识别失败，请重新扫描或从图片识别。".into()),
            }
        });
    };

    let session_control =
        use_hook(try_consume_context::<crate::app::entry::ConnectionSessionControl>);
    let scoped_entry = session_control.is_some();
    // Desktop browser and native desktop share the collaboration surface. Mobile only
    // exposes the connected-node list and the action for sharing the current node.
    let mobile_collaboration = mobile_client && !scoped_entry;
    let collaboration_hub = !mobile_client && !scoped_entry;
    let navigation = use_hook(try_consume_context::<crate::app::node_navigation::NodeNavigation>);
    let home_address = navigation
        .map(|nav| nav.home_address.read().clone())
        .unwrap_or_default();
    let return_home = navigation.filter(|_| home_address != current_address.trim_end_matches('/'));
    let memory_warning = session_control
        .map(|control| control.warning.read().clone())
        .unwrap_or_default();
    let busy = busy || session_control.is_some_and(|control| *control.busy.read());
    let mobile_authorized = online
        && auth_state
            .read()
            .as_ref()
            .is_some_and(|auth| auth.authenticated);
    let reauthenticate = navigation.filter(|_| !mobile_authorized).map(|nav| {
        let address = current_address.clone();
        EventHandler::new(move |()| nav.navigate.call(address.clone()))
    });
    // Native password recovery is in-place; browser recovery reenters its own gate.
    let login_current = navigation.is_none()
        && matches!(phase, Phase::PasswordRequired)
        && manual_address.read().is_empty()
        && phone_address.read().is_empty();
    let mobile_form_address = if login_current {
        current_address.clone()
    } else if phone_address.read().is_empty() {
        manual_address.read().clone()
    } else {
        phone_address.read().clone()
    };

    rsx! {
        style { {include_str!("devices.css")} }
        div { class: "devices-page",
            if mobile_collaboration {
                section { class: "collaboration-hub mobile-collaboration-hub", aria_label: "工作协同",
                    div { class: "page-heading collaboration-page-heading",
                        div {
                            div { class: "overline", "DEVICES & COLLABORATION" }
                            h2 { "工作协同" }
                            p { "查看已连接的工作节点，并分享当前节点。" }
                        }
                        span { class: if online { "status-badge completed" } else { "status-badge" }, i {}, if online { "当前节点已连接" } else if busy { "节点连接中" } else { "当前节点未连接" } }
                    }
                    div { class: "collaboration-actions",
                        section { class: "collaboration-action-card connect-card", aria_label: "分享当前节点",
                            div { class: "collaboration-action-heading",
                                span { class: "collaboration-action-icon", Icon { name: "cloud" } }
                                div { h2 { "分享当前节点" } p { "让其他设备连接当前工作节点，继续项目、会话和 Agent 任务。" } }
                            }
                            div { class: "collaboration-action-footer",
                                AuthorizationQrButton {
                                    owned_local: false, target: current_address.clone(), available: online && mobile_authorized,
                                    onopen: move |()| { qr_open.set(false); pairing.request.with_mut(authorization_offer::Request::show); },
                                }
                            }
                        }
                    }
                    section { class: "collaboration-list-panel", aria_label: "连接列表",
                        div { class: "collaboration-list-heading", div { h2 { "连接列表" } p { "本设备连接过的工作节点" } }
                            if !connection.history.read().recent.is_empty() { button { class: "button ghost", onclick: move |_| connection.history.set(NodeHistory::default()), "清除记录" } }
                        }
                        if connection.history.read().recent.is_empty() {
                            div { class: "collaboration-empty", Icon { name: "clock" } strong { "还没有连接记录" } span { "连接工作节点后，会显示在这里。" } }
                        } else {
                            for address in connection.history.read().recent.clone() {
                                div { class: "collaboration-list-row",
                                    span { class: "collaboration-device-icon", Icon { name: "monitor" } }
                                    div { class: "collaboration-device-copy", strong { "{address}" } small { "工作节点" } }
                                    button { class: "button", disabled: busy, aria_label: "连接 {address}", onclick: { let address = address.clone(); move |_| { base_url.set(format!("{address}/")); password.set(NodePassword::default()); error.set(String::new()); onconnect.call(()); } }, "连接" }
                                }
                            }
                        }
                    }
                }
            } else if collaboration_hub {
                section { class: "collaboration-hub", aria_label: "工作协同",
                    div { class: "page-heading collaboration-page-heading",
                        div {
                            div { class: "overline", "DEVICES & COLLABORATION" }
                            h2 { "工作协同" }
                            p { "连接其他设备，或允许可信设备访问这台电脑上的工作空间。" }
                        }
                        span { role: "status", class: if online { "status-badge completed" } else { "status-badge" }, i {}, if online { if owned_local { "本机节点在线" } else { "当前节点已连接" } } else if busy { "节点连接中" } else { "当前节点未连接" } }
                    }
                    div { class: "collaboration-actions",
                        section { class: "collaboration-action-card discovery-card", aria_label: "发现局域网节点",
                            div { class: "collaboration-action-heading",
                                span { class: "collaboration-action-icon", Icon { name: "search" } }
                                div { h2 { "发现节点" } p { "搜索同一局域网中已开启远程协作的 Work 节点。发现后仍会读取节点信息并等待确认。" } }
                            }
                            div { class: "collaboration-action-footer",
                                button { class: "button", disabled: *discovery_busy.read() || busy, onclick: move |_| discover_lan(()), if *discovery_busy.read() { "正在发现…" } else { "刷新局域网节点" } }
                                if !discovery_status.read().is_empty() { span { class: "collaboration-action-hint", role: "status", "{discovery_status}" } }
                            }
                            if !discovery_results.read().is_empty() {
                                div { class: "discovered-node-list",
                                    for node in discovery_results.read().clone() {
                                        div { class: "discovered-node-row",
                                            div { class: "collaboration-device-copy",
                                                strong { "{node.node_id}" }
                                                small { "{node.endpoint} · {node.authentication}" }
                                            }
                                            button { class: "button", disabled: *preview_busy.read(), onclick: { let node = node.clone(); move |_| preview.set(Some(node.clone())) }, "查看信息" }
                                        }
                                    }
                                }
                            }
                        }
                        section { class: "collaboration-action-card connect-card", aria_label: "连接其他设备",
                            div { class: "collaboration-action-heading",
                                span { class: "collaboration-action-icon", Icon { name: "cloud" } }
                                div { h2 { "连接其他设备" } p { "连接另一台电脑或工作节点，继续项目、会话和 Agent 任务。" } }
                            }
                            div { class: "collaboration-action-footer",
                                button { class: "button primary", disabled: busy, onclick: move |_| { error.set(String::new()); remote_open.toggle(); }, "连接设备" }
                                span { class: "collaboration-action-hint", "扫码或输入地址" }
                            }
                            if *remote_open.read() {
                                div { class: "collaboration-connect-form",
                                    div { class: "form-field", label { r#for: "node-url", "节点地址" }
                                        input { id: "node-url", r#type: "url", value: "{base_url}", autocomplete: "url", placeholder: "https://其他电脑地址:{remote_port}/", oninput: move |event| { base_url.set(event.value()); preview_epoch += 1; preview.set(None); password.set(NodePassword::default()); } }
                                    }
                                    div { class: "form-field", label { r#for: "node-password", "节点密码" }
                                        input { id: "node-password", r#type: "password", autocomplete: "current-password", value: password.read().value.clone(), placeholder: "连接时输入，不会保存", oninput: move |event| { password.set(NodePassword::new(event.value(), &base_url.read())) } }
                                    }
                                    div { class: "collaboration-connect-actions",
                                        button { class: "button", onclick: move |_| remote_open.set(false), "取消" }
                                        button { class: "button primary", disabled: busy || *preview_busy.read(), onclick: move |_| {
                                            let address_input = base_url.read().clone();
                                            match normalize_remote_address(&address_input, false, remote_port) {
                                                Ok(address) => {
                                                    let endpoint = format!("{address}/");
                                                    preview_epoch += 1;
                                                    let attempt = *preview_epoch.read();
                                                    preview_busy.set(true);
                                                    preview.set(None);
                                                    error.set(String::new());
                                                    spawn(async move {
                                                        let result = probe_work_node(&endpoint).await;
                                                        if *preview_epoch.peek() != attempt { return; }
                                                        preview_busy.set(false);
                                                        match result {
                                                            Ok(node) => { base_url.set(node.endpoint.clone()); preview.set(Some(node)); }
                                                            Err(message) => error.set(format!("无法读取节点信息：{message}")),
                                                        }
                                                    });
                                                }
                                                Err(message) => error.set(message),
                                            }
                                        }, if *preview_busy.read() { "正在检查…" } else { "检查节点" } }
                                    }
                                    if !error.read().is_empty() { p { class: "connection-form-error", role: "alert", "{error}" } }
                                }
                            }
                        }
                        LocalAccessCards {
                            sharing: sharing_state,
                            available: owns_desktop && owned_local,
                            onshare: move |()| pairing.request.with_mut(authorization_offer::Request::show),
                        }
                    }
                    CollaborationAdvancedSettings { store: settings_store, default_port: remote_port }
                    div { class: "collaboration-lists",
                        section { class: "collaboration-list-panel", aria_label: "最近连接",
                            div { class: "collaboration-list-heading", div { h2 { "最近连接" } p { "本设备主动连接过的工作节点" } }
                                if !connection.history.read().recent.is_empty() { button { class: "button ghost", onclick: move |_| connection.history.set(NodeHistory::default()), "清除记录" } }
                            }
                            if connection.history.read().recent.is_empty() {
                                div { class: "collaboration-empty", Icon { name: "clock" } strong { "还没有最近连接" } span { "连接其他设备后，会显示在这里。" } }
                            } else {
                                for address in connection.history.read().recent.clone() {
                                    div { class: "collaboration-list-row",
                                        span { class: "collaboration-device-icon", Icon { name: "monitor" } }
                                        div { class: "collaboration-device-copy", strong { "{address}" } small { "远程工作节点" } }
                                        button { class: "button", disabled: busy, aria_label: "连接 {address}", onclick: { let address = address.clone(); move |_| { base_url.set(format!("{address}/")); password.set(NodePassword::default()); error.set(String::new()); onconnect.call(()); } }, "连接" }
                                    }
                                }
                            }
                        }
                        section { class: "collaboration-list-panel visited-panel", aria_label: "被访问的设备",
                            div { class: "collaboration-list-heading", div { h2 { "被访问的设备" } p { "曾经访问过这台电脑的设备" } } }
                            div { class: "collaboration-empty", Icon { name: "shield" } strong { "暂无可用记录" } span { "此页面暂不提供设备访问记录。" } }
                        }
                    }
                }
            }
            // Keep node diagnostics and host controls available without appending a
            // second settings page to the collaboration surface.
            if owned_local || (!mobile_client && !scoped_entry) {
                details { class: "connection-details", open: !collaboration_hub,
                summary { "当前节点与连接详情" }
                div { class: "page-heading connection-page-heading",
                    div {
                        h2 { if owned_local { "本机节点详情" } else { "当前节点详情" } }
                        p { "查看当前连接的工作节点，或切换到其他设备。" }
                    }
                    if owned_local {
                        span { class: "neutral-pill", if sharing.enabled { "远程协作已开启" } else { "远程协作已关闭" } }
                    }
                }
                section { class: "device-auth-entry",
                    AuthorizationQrButton {
                        owned_local,
                        target: if owned_local { sharing.app_url.clone() } else { current_address.clone() },
                        available: online && if owned_local {
                            owns_desktop && sharing.listening && sharing.enabled
                        } else {
                            auth_state.read().as_ref().is_some_and(|auth| auth.authenticated)
                                && matches!(shared_client.read().as_ref(), Some(WorkspaceClient::Remote(_)))
                        },
                        onopen: move |()| {
                            qr_open.set(false);
                            pairing.request.with_mut(authorization_offer::Request::show);
                        },
                    }
                }
                div { class: "connection-layout",
                    section { class: "server-overview",
                        div { class: "server-identity",
                            span { class: "connection-node-icon", Icon { name: "monitor".to_owned() } }
                            div { h2 { if owned_local { "这台电脑" } else { "当前执行节点" } } p { if owned_local { "本机工作空间 · RPC" } else { "远程执行节点 · HTTP/WebSocket" } } }
                            span { class: if online { "status-badge completed" } else { "status-badge" }, i {}, if owned_local { if online { "运行中" } else if busy { "准备中" } else { "未就绪" } } else if online { "已连接" } else if busy { "连接中" } else { "未连接" } }
                        }
                        div { class: "connection-topology",
                            if owned_local {
                                span { Icon { name: "monitor".to_owned() } "桌面应用" small { "对话 / 变更 / 回答确认" } }
                                i { "本机 RPC" }
                                span { Icon { name: "monitor".to_owned() } "本机工作空间" small { "模型 / 工具 / 项目文件" } }
                            } else {
                                span { Icon { name: "monitor".to_owned() } "PC 服务端" small { "模型 / 工具 / 项目文件" } }
                                i { "HTTP + WebSocket" }
                                span { Icon { name: "phone".to_owned() } "手机客户端" small { "对话 / 变更 / 回答确认" } }
                            }
                        }
                        div { class: "node-address", small { "当前执行节点地址" } code { "{current_address}" } }
                        div { class: "server-details",
                            div { span { "服务状态" } strong { role: "status", "{connection_status}" } }
                            div { span { "连接方式" } strong { if owned_local { "本机 RPC" } else { "远程节点" } } }
                            div { span { "模型凭据" } strong { "保留在节点" } }
                        }
                        div { class: "connection-server-actions",
                            if !owned_local && (owns_desktop || crate::app::local_node::startup_error().is_some()) {
                            button { class: "button", disabled: busy, onclick: move |_| {
                                // The parent handles this stable identity before HTTP normalization,
                                // including reporting a local runtime startup failure.
                                base_url.set(crate::app::local_node::address()
                                    .unwrap_or(crate::app::local_node::LOCAL_ADDRESS).into());
                                password.set(NodePassword::default());
                                remote_open.set(false);
                                error.set(String::new());
                                onconnect.call(());
                            }, "使用这台电脑" }
                            }
                            if !collaboration_hub {
                                button { class: "button", onclick: move |_| remote_open.set(!remote_open()), "连接其他节点" }
                            }
                        }
                        if owned_local { p { class: "connection-help", "本机 RPC 和桌面 HTTP 共用这一份工作空间；远程协作默认关闭。开启并保存后，手机可通过二维码访问相同会话和任务；共享开关与 HTTP 配置会记住，不会自动修改防火墙。" } }
                        if *remote_open.read() && !collaboration_hub {
                            div { class: "remote-node-form",
                                div { class: "form-field",
                                    label { r#for: "node-url", "节点地址" }
                                    input { id: "node-url", r#type: "url", value: "{base_url}", autocomplete: "url", placeholder: "https://其他电脑地址:{remote_port}/", oninput: move |event| { base_url.set(event.value()); password.set(NodePassword::default()); } }
                                }
                                div { class: "form-field",
                                    label { r#for: "node-password", "节点密码" }
                                    input { id: "node-password", r#type: "password", autocomplete: "current-password", value: password.read().value.clone(), placeholder: "输入节点密码", oninput: move |event| password.set(NodePassword::new(event.value(), &base_url.read())) }
                                }
                                button { class: "button primary", onclick: move |_| {
                                    let candidate = normalize_remote_address(&base_url.read(), false, remote_port);
                                    match candidate {
                                        Ok(address) => { base_url.set(format!("{address}/")); error.set(String::new()); onconnect.call(()); }
                                        Err(message) => error.set(message),
                                    }
                                }, "连接节点" }
                            }
                        }
                        p { class: "connection-form-error", role: "alert", "{error}" }
                        if !owned_local {
                        if let Some(auth) = auth_state.read().as_ref() {
                            if auth.required && !auth.authenticated {
                                if !*remote_open.read() {
                                    div { class: "form-field", label { r#for: "device-node-password", "节点密码" } input { id: "device-node-password", r#type: "password", autocomplete: "current-password", value: password.read().value.clone(), oninput: move |event| password.set(NodePassword::new(event.value(), &connection.address.read())) } }
                                }
                                button { class: "button primary", disabled: busy || password.read().is_empty(), onclick: move |_| onlogin.call(()), if busy { "正在登录…" } else { "登录" } }
                            } else if auth.authenticated && online { p { class: "meta", "节点已认证。" } }
                        }
                        }
                        if let Some(caps) = capabilities.read().as_ref() { p { class: "connection-help", "协议 {caps.protocol} · {caps.supported_commands.len()} 个命令 · {caps.supported_actions.len()} 个动作" } }
                        if !owned_local { p { class: "connection-help", "同一局域网使用电脑可达地址；远程连接使用已配置的 HTTPS 或专用网络入口。" } }
                    }
                    aside { class: "connection-steps",
                        if owned_local {
                            h3 { "需要跨设备接续？" }
                            p { class: "connection-help", "开启这台电脑的共享并配置可达的局域网或 HTTPS 入口后，可分享普通地址供浏览器密码登录，或使用手机 App 一次性扫码授权。二维码不会更改监听或防火墙。" }
                        } else {
                            h3 { "手机接续工作" }
                            div { b { "1" } span { "电脑显示二维码" small { "地址二维码，不包含密码" } } }
                            div { b { "2" } span { "手机扫码并核对电脑" small { "识别后确认节点地址" } } }
                            div { b { "3" } span { "登录并加载项目" small { "需要密码时在手机登录" } } }
                        }
                    }
                }
                section { class: "node-history", aria_label: "最近连接的节点",
                    div { class: "connection-clients-heading",
                        h2 { "最近连接" }
                        if !connection.history.read().recent.is_empty() {
                            button { class: "button", onclick: move |_| connection.history.set(NodeHistory::default()), "清除连接记录" }
                        }
                    }
                    p { class: "connection-help", "只保存成功连接的远程地址；桌面重开默认使用本机节点，远程节点需手动切换。密码不会保存。" }
                    if connection.history.read().recent.is_empty() { div { class: "connection-empty", "成功连接节点后，地址会显示在这里。" } }
                    for address in connection.history.read().recent.clone() {
                        div { class: "node-history-item",
                            code { "{address}" }
                            button { class: "button", aria_label: format!("连接 {address}"), onclick: move |_| {
                                base_url.set(format!("{address}/")); error.set(String::new()); password.set(NodePassword::default()); onconnect.call(());
                            }, "连接" }
                        }
                    }
                }
                }
            } else if !mobile_collaboration && !collaboration_hub {
                ConnectionScreen {
                    current_address: if mobile_authorized || scoped_entry { current_address.clone() } else { String::new() },
                    current_status: connection_status,
                    show_current_node: false,
                    show_connection_summary: false,
                    eyebrow: String::new(),
                    heading: String::new(),
                    description: String::new(),
                    authenticated: mobile_authorized,
                    warning: memory_warning,
                    show_password: navigation.is_none(),
                    home_address: home_address.clone(),
                    onhome: return_home.map(|nav| EventHandler::new(move |()| nav.navigate.call(nav.home_address.peek().clone()))),
                    help: navigation.map(|_| "填写目标节点提供的 Web 入口地址。将在目标页面独立登录，不转发当前密码或授权票据；仅提供 API 的节点需先配置 Web 入口。离开不会停止节点任务，请先保留未提交的输入。".to_owned()),
                    show_form: (!scoped_entry && !mobile_authorized) || *remote_open.read(),
                    manual: *manual_open.read(),
                    address: mobile_form_address.clone(),
                    password: password.read().value.clone(), busy, error: error.read().clone(),
                    fixed_address: login_current || !phone_address.read().is_empty(),
                    submit_label: if navigation.is_some() { "连接节点" } else if login_current { "登录" } else { "连接设备" },
                    onswitch: move |()| {
                        if let Some(restart) = session_control.and_then(|control| control.restart) { restart.call(()); }
                        else {
                            remote_open.set(!remote_open()); manual_open.set(true);
                            password.set(NodePassword::default()); phone_address.set(String::new()); error.set(String::new());
                        }
                    },
                    onforget: session_control.map(|control| control.forget),
                    onreauthenticate: reauthenticate,
                    onshare: if share_available { Some(EventHandler::new(move |()| open_share(()))) } else { None },
                    onchange: move |manual| {
                        manual_open.set(manual); password.set(NodePassword::default()); error.set(String::new());
                        scanning.set(false); camera_epoch += 1; phone_address.set(String::new()); scanner_status.set(String::new());
                        let _ = document::eval(&format!("{CAMERA_CLEANUP}window.__rpiWorkQrAlbum?.cancel();"));
                    },
                    onaddress: move |value| { manual_address.set(value); password.set(NodePassword::default()); error.set(String::new()); },
                    onpassword: move |value| { password.set(NodePassword::new(value, &mobile_form_address)); },
                                onsubmit: move |()| {
                                    if login_current {
                                        if !password.read().is_empty() { onlogin.call(()); }
                                    } else if !phone_address.read().is_empty() { connect_address(()); }
                                    else {
                                        match normalize_remote_address(
                                        &manual_address.read(),
                                        navigation.is_none(),
                                        remote_port,
                                    ) {
                                            Ok(address) => {
                                                let endpoint = format!("{address}/");
                                                preview_epoch += 1;
                                                let attempt = *preview_epoch.read();
                                                preview_busy.set(true);
                                                preview.set(None);
                                                error.set(String::new());
                                                spawn(async move {
                                                    let result = probe_work_node(&endpoint).await;
                                                    if *preview_epoch.peek() != attempt { return; }
                                                    preview_busy.set(false);
                                                    match result {
                                                        Ok(node) => { base_url.set(node.endpoint.clone()); preview.set(Some(node)); }
                                                        Err(message) => error.set(format!("无法读取节点信息：{message}")),
                                                    }
                                                });
                                            }
                                            Err(message) => error.set(message),
                                        }
                                    }
                                },
                    scanning: *scanning.read(), scan_status: scanner_status.read().clone(), target: phone_address.read().clone(), album_input: true,
                                oncamera: move |()| {
                                    if *scanning.read() {
                                        scanning.set(false); camera_epoch += 1; scanner_status.set("相机已关闭。".into());
                                        spawn(async move { let _ = document::eval(&stop_camera_script()).recv::<bool>().await; });
                                        return;
                                    }
                                    manual_open.set(false);
                                    password.set(NodePassword::default());
                                    error.set(String::new());
                                    phone_address.set(String::new());
                                    scanning.set(true);
                                    camera_epoch += 1;
                                    let epoch = *camera_epoch.read();
                                    scanner_status.set("正在申请相机权限；可随时关闭。".into());
                                    let mut evaluator = document::eval(&start_camera_script());
                                    spawn(async move {
                                        let result = evaluator.recv::<String>().await;
                                        if *camera_epoch.read() != epoch { return; }
                                        scanning.set(false);
                                        match result {
                                            Ok(value) if value.starts_with("ERR:") => scanner_status.set(value[4..].to_owned()),
                                            Ok(value) if !value.is_empty() => match connection_address(&value, remote_port) {
                                                Ok(address) => { password.set(NodePassword::default()); phone_address.set(address); scanner_status.set("已读取地址，请核对连接信息后连接。".into()); }
                                                Err(message) => scanner_status.set(message),
                                            },
                                            Ok(_) => scanner_status.set("相机已关闭，可以重新扫码。".into()),
                                            Err(_) => scanner_status.set("相机识别失败，请重新扫描或从图片识别。".into()),
                                        }
                                    });

                                },
                                onalbum: move |()| read_album(false),
                                onconfirm: move |()| {
                                    // Only explicit confirmation starts a connection. A protected
                                    // node subsequently uses the shared password form.
                                    manual_open.set(true); scanning.set(false); camera_epoch += 1;
                                    let _ = document::eval(&stop_camera_script());
                                    connect_address(());
                                },
                                oncancel: move |()| { phone_address.set(String::new()); password.set(NodePassword::default()); error.set(String::new()); },
                    AuthorizationQrButton {
                        owned_local: false, target: current_address.clone(),
                        available: mobile_authorized && matches!(shared_client.read().as_ref(), Some(WorkspaceClient::Remote(_))),
                        onopen: move |()| { qr_open.set(false); pairing.request.with_mut(authorization_offer::Request::show); },
                    }
                }
            }
            if let Some(node) = preview.read().clone() {
                NodePreviewCard {
                    preview: node.clone(),
                    onconfirm: move |()| {
                        base_url.set(node.endpoint.clone());
                        preview.set(None);
                        preview_busy.set(false);
                        error.set(String::new());
                        onconnect.call(());
                    },
                    oncancel: move |()| {
                        preview.set(None);
                        preview_busy.set(false);
                    },
                }
            }
            if !connection.warning.read().is_empty() { p { class: "connection-form-error", role: "status", "{connection.warning}" } }
            if share_available && qr_open.read().to_owned() {
                crate::shared::components::Modal { id: "device-address-qr", title: share_title, class: "device-address-dialog", onclose: move |()| { qr_open.set(false); share_epoch += 1; },
                    div { class: "modal-body",
                        p { class: "modal-help device-address-intro", "分享仅包含节点地址，不包含密码或登录凭据；接收方仍需输入节点密码。" }
                        div { class: "connection-share-source", small { "当前已连接节点" } code { "{current_address}" } }
                        div { class: "device-address-layout",
                            section { class: "device-address-qr-column",
                                div { class: "pairing-qr", id: "device-address-qr-code", aria_label: "节点地址二维码" }
                                if !share_address_valid { p { class: "modal-help", role: "status", "配置可达地址后生成二维码" } }
                                code { class: "qr-address", "{address}" }
                            }
                            section { class: "device-address-details",
                                div { class: "form-field", label { r#for: "qr-node-address", "对方可访问的节点地址" }
                                    input { id: "qr-node-address", r#type: "url", placeholder: "http://电脑局域网IP:Web入口端口", title: "localhost 指向接收方本机；分享地址不会切换当前连接或更改节点监听配置。", value: "{advertised_address}", oninput: move |event| { advertised_address.set(event.value()); share_epoch += 1; copy_status.set(String::new()); } }
                                }
                                p { class: "connection-form-error", role: "alert", "{share_error}" }
                                if owned_local { p { class: "modal-help", "{sharing.address_hint}" } }
                                p { class: "modal-help", "默认分享当前节点连接。若地址为 localhost 或回环，请改为同一节点可达的局域网或 HTTPS 同源 Web/代理入口。填写地址不会切换节点或开启节点共享。" }
                                label { r#for: "device-address-payload", "连接信息 · 可手动复制" }
                                textarea { id: "device-address-payload", rows: "3", readonly: true, value: phone_share_address(&address, owned_local).map(|value| qr_payload(&value)).unwrap_or_default() }
                                p { class: "modal-help", role: "status", "{copy_status}" }
                                div { class: "modal-actions",
                                    button { class: "button", disabled: !share_address_valid, onclick: move |_| { share_epoch += 1; copy_status.set(String::new()); }, "重新生成" }
                                    button { class: "button", disabled: !share_address_valid, onclick: move |_| copy_share(true), "复制地址" }
                                    button { class: "button", disabled: !share_address_valid, onclick: move |_| copy_share(false), "复制连接信息" }
                                }
                            }
                        }
                    }
                    div { class: "modal-footer", button { class: "button", onclick: move |_| { qr_open.set(false); share_epoch += 1; }, "完成" } }
                }
            }
        }
    }
}

#[component]
fn NodePreviewCard(
    preview: NodePreview,
    onconfirm: EventHandler<()>,
    oncancel: EventHandler<()>,
) -> Element {
    rsx! {
        section { class: "node-discovery-preview", aria_label: "节点信息预览",
            div { class: "node-discovery-preview-heading",
                div { h3 { "节点信息" } p { "请核对节点身份和协议后再连接。" } }
                span { class: "status-badge completed", "已验证" }
            }
            code { class: "node-discovery-endpoint", "{preview.endpoint}" }
            div { class: "node-discovery-details",
                div { small { "节点 ID" } strong { "{preview.node_id}" } }
                div { small { "协议" } strong { "{preview.protocol} · v{preview.version}" } }
                div { small { "健康状态" } strong { "{preview.health}" } }
                div { small { "认证状态" } strong { "{preview.authentication}" } }
            }
            if !preview.transports.is_empty() {
                p { class: "connection-help", "传输：{preview.transports}" }
            }
            div { class: "node-discovery-preview-actions",
                button { class: "button", onclick: move |_| oncancel.call(()), "取消" }
                button { class: "button primary", onclick: move |_| onconfirm.call(()), "确认连接" }
            }
        }
    }
}

// Bearer material stays in transient component state, never Debug/logs/storage.
#[derive(Clone)]
struct AuthorizationQrData {
    payload: String,
    expires_at: u64,
}

fn unix_seconds() -> u64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

fn authorization_payload(
    address: &str,
    ticket: &str,
    expires_at: u64,
    node_id: &str,
    expected_node: &str,
    now: u64,
) -> Result<String, String> {
    let address = normalize_address(address, true)?;
    if expected_node.is_empty() || node_id != expected_node {
        return Err("分享地址与当前授权节点不一致，请检查网络入口。".into());
    }
    if ticket.is_empty() || expires_at <= now || expires_at > now.saturating_add(120) {
        return Err("授权已过期或有效期无效，请重新生成。".into());
    }
    Ok(serde_json::json!({
        "v": 1, "type": "rpi-work-node-auth", "address": address,
        "ticket": ticket, "expiresAt": expires_at, "nodeId": node_id,
    })
    .to_string())
}

async fn owner_authorization() -> Result<(String, u64, String), String> {
    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    {
        let grant = crate::app::local_http::issue_qr_authorization().await?;
        Ok((grant.ticket, grant.expires_at, grant.node_id))
    }
    #[cfg(any(target_arch = "wasm32", target_os = "android"))]
    {
        Err("只能在节点桌面签发本机授权。".into())
    }
}

async fn authorization_endpoint_node(address: &str) -> Result<String, String> {
    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    {
        return crate::app::local_http::authorization_endpoint_node(address).await;
    }
    #[cfg(any(target_arch = "wasm32", target_os = "android"))]
    {
        let health_url =
            serde_json::to_string(&format!("{address}/api/v1/health")).unwrap_or_default();
        let script = format!(
            "try{{const r=await fetch({health_url},{{cache:'no-store',signal:AbortSignal.timeout(8000)}});if(!r.ok)throw new Error();const h=await r.json();dioxus.send(h.sharingEnabled===true&&typeof h.nodeId==='string'?h.nodeId:'');}}catch(_){{dioxus.send('');}}"
        );
        document::eval(&script)
            .recv::<String>()
            .await
            .map_err(|_| "无法验证分享地址，请检查局域网或 HTTPS 入口。".to_owned())
    }
}

#[component]
fn AuthorizationQrButton(
    available: bool,
    target: String,
    owned_local: bool,
    onopen: EventHandler<()>,
) -> Element {
    let valid = phone_share_address(&target, owned_local).is_ok();
    rsx! {
        if available {
            button { class: "button", disabled: !valid, onclick: move |_| onopen.call(()), "手机 App 扫码授权" }
            p { class: "connection-help", "Android 原生 App：一次性扫码授权，无需输入密码。浏览器仍使用普通地址和密码登录。" }
            if !valid { p { class: "connection-help", "请先配置可达的局域网或 HTTPS 地址；回环地址不能用于手机授权。" } }
        }
    }
}

type AuthorizationScope = (String, u64, String, String, String, bool, bool, String);

#[component]
fn AuthorizationQr(
    shared_client: Signal<Option<WorkspaceClient>>,
    base_url: Signal<String>,
    owned_local: bool,
    public_url: String,
    available: bool,
    address_draft: Signal<String>,
    automatic: bool,
    request: Signal<authorization_offer::Request>,
    view: String,
) -> Element {
    let connection = use_context::<ConnectionStore>();
    let mut open = use_signal(|| false);
    let mut request_epoch = use_signal(|| 0u64);
    let mut secret = use_signal(|| None::<AuthorizationQrData>);
    let mut remaining = use_signal(|| 0u64);
    let mut pending = use_signal(|| false);
    let mut message = use_signal(String::new);
    let node = connection.address.read().clone();
    let node_epoch = *connection.epoch.read();
    let draft = address_draft.read().trim().to_owned();
    let target = if draft.is_empty() {
        if owned_local {
            public_url.clone()
        } else {
            node.clone()
        }
    } else {
        draft.clone()
    };
    // Show the exact normalized origin encoded into the authorization payload,
    // not the desktop-only rpi-work identity or a trailing-slash variant.
    let target = phone_share_address(&target, owned_local).unwrap_or(target);
    let target_valid = phone_share_address(&target, owned_local).is_ok();
    let scope = (
        node,
        node_epoch,
        draft.clone(),
        base_url.read().clone(),
        public_url,
        available,
        owned_local,
        view,
    );
    let mut previous_scope = use_signal(|| None::<AuthorizationScope>);
    let mut offer = use_signal(authorization_offer::Offer::default);
    let intent = *request.read();
    use_effect(move || {
        let Some(data) = secret.read().clone() else {
            return;
        };
        let attempt = *request_epoch.peek();
        let epoch = *connection.epoch.peek();
        let guard = format!(
            "if(el.dataset.request!=='{attempt}'||Date.now()>={expiry}*1000)throw new Error();setTimeout(()=>{{if(el.dataset.request==='{attempt}')el.replaceChildren();}},Math.max(0,{expiry}*1000-Date.now()));",
            expiry = data.expires_at,
        );
        let script = qr_payload_script(
            &data.payload,
            "device-auth-qr-code",
            "手机 App 一次性授权二维码",
            &guard,
        );
        spawn(async move {
            let generated = matches!(document::eval(&script).recv::<bool>().await, Ok(true));
            if *request_epoch.peek() != attempt || *connection.epoch.peek() != epoch {
                return;
            }
            if !generated {
                secret.set(None);
                message.set("无法生成授权二维码，请重试。".into());
                return;
            }
            loop {
                if *request_epoch.peek() != attempt || *connection.epoch.peek() != epoch {
                    return;
                }
                let left = data.expires_at.saturating_sub(unix_seconds());
                remaining.set(left);
                if left == 0 {
                    secret.set(None);
                    message.set("授权二维码已过期，已隐藏；请重新生成。".into());
                    return;
                }
                #[cfg(target_arch = "wasm32")]
                gloo_timers::future::TimeoutFuture::new(1000).await;
                #[cfg(not(target_arch = "wasm32"))]
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        });
    });
    let mut close = move || {
        request_epoch += 1;
        secret.set(None);
        remaining.set(0);
        pending.set(false);
        open.set(false);
        let _ =
            document::eval("document.getElementById('device-auth-qr-code')?.replaceChildren();");
    };
    let issue_target = target.clone();
    let issue = use_callback(move |automatic: bool| {
        request_epoch += 1;
        let attempt = *request_epoch.peek();
        let epoch = *connection.epoch.peek();
        // Capture the current client each time, not the client from first mount.
        let client = shared_client.peek().clone();
        let address = issue_target.clone();
        secret.set(None);
        remaining.set(0);
        message.set(String::new());
        open.set(!automatic);
        pending.set(true);
        spawn(async move {
            if automatic {
                // Never stack the startup offer over another active modal.
                let unobstructed = matches!(
                    document::eval("dioxus.send(!document.querySelector('dialog[open]'))")
                        .recv::<bool>()
                        .await,
                    Ok(true)
                );
                if *request_epoch.peek() != attempt || *connection.epoch.peek() != epoch {
                    return;
                }
                if !unobstructed {
                    pending.set(false);
                    return;
                }
                open.set(true);
            }
            let result = async {
                let address = phone_share_address(&address, owned_local)?;
                // Verify reachability and identity without ever sending a ticket
                // to the advertised endpoint. A different/draft address cannot issue.
                let expected_node = authorization_endpoint_node(&address).await?;
                if expected_node.is_empty() {
                    return Err("分享地址不可达或未开启共享，无法签发授权。".into());
                }
                if *request_epoch.peek() != attempt || *connection.epoch.peek() != epoch {
                    return Err(String::new());
                }
                let (ticket, expires_at, node_id) = if owned_local {
                    owner_authorization().await?
                } else {
                    let Some(client @ WorkspaceClient::Remote(_)) = client else {
                        return Err("请先登录当前 HTTP 节点。".into());
                    };
                    let grant = client
                        .issue_qr_authorization()
                        .await
                        .map_err(|_| "签发失败，请确认当前节点已登录且共享已开启。".to_owned())?;
                    (grant.ticket, grant.expires_at, grant.node_id)
                };
                let payload = authorization_payload(
                    &address,
                    &ticket,
                    expires_at,
                    &node_id,
                    &expected_node,
                    unix_seconds(),
                )?;
                Ok(AuthorizationQrData {
                    payload,
                    expires_at,
                })
            }
            .await;
            if *request_epoch.peek() != attempt
                || *connection.epoch.peek() != epoch
                || !*open.peek()
            {
                return;
            }
            pending.set(false);
            match result {
                Ok(data) => {
                    remaining.set(data.expires_at.saturating_sub(unix_seconds()));
                    secret.set(Some(data));
                }
                Err(error) => message.set(error),
            }
        });
    });
    // Invalidation and the initial offer run in one effect, so mount-time
    // invalidation cannot cancel a just-started automatic request. This instance
    // survives Settings/layout remounts; only explicit requests can reissue.
    use_effect(use_reactive(
        (&scope, &intent, &automatic),
        move |(scope, intent, automatic)| {
            if previous_scope.peek().as_ref() != Some(&scope) {
                previous_scope.set(Some(scope));
                close();
                message.set(String::new());
            }
            let action =
                offer.with_mut(|offer| offer.next(intent, automatic && available && target_valid));
            match action {
                Some(false) => close(),
                Some(true)
                    if available
                        && target_valid
                        && !(*open.peek() && (*pending.peek() || secret.peek().is_some())) =>
                {
                    issue.call(automatic && intent.revision == 0);
                }
                _ => {}
            }
        },
    ));
    rsx! {
        if available && *open.read() {
            crate::shared::components::Modal {
                id: "device-auth-qr", title: "手机 App 扫码授权", class: "device-address-dialog device-auth-dialog",
                onclose: move |()| close(),
                div { class: "modal-body",
                    p { class: "device-auth-warning", "任何人在过期前扫描此码，都可访问当前工作空间（项目、会话与运行任务）。只向你信任的 Android 手机展示。" }
                    p { class: "modal-help", "120 秒有效，仅可使用一次；二维码不含密码。请用 Android 原生 App 扫描并确认授权，浏览器仍需密码登录。" }
                    div { class: "connection-share-source", small { "手机访问地址 · 当前授权节点" } code { "{target}" } }
                    div { class: "form-field device-auth-address-field",
                        label { r#for: "device-auth-address", "手机访问地址（可填写自定义域名）" }
                        input {
                            id: "device-auth-address", r#type: "url", value: "{target}",
                            placeholder: "https://work.example.com",
                            oninput: move |event| address_draft.set(event.value()),
                        }
                    }
                    if secret.read().is_some() && *remaining.read() > 0 {
                        div { class: "pairing-qr", id: "device-auth-qr-code", "data-request": "{request_epoch}", aria_label: "手机 App 一次性授权二维码" }
                        p { class: "device-auth-countdown", role: "status", "{remaining} 秒后过期 · 仅可使用一次" }
                    }
                    if *pending.read() { p { class: "modal-help", role: "status", "正在验证节点并签发授权…" } }
                    p { class: "connection-form-error", role: "alert", "{message}" }
                    p { class: "modal-help", "关闭窗口即隐藏此码；已签发的授权在使用或过期前仍可能有效。关闭共享可撤销授权。" }
                }
                div { class: "modal-footer",
                    button { class: "button", disabled: *pending.read(), onclick: move |_| issue.call(false), "重新生成授权码" }
                    button { class: "button", onclick: move |_| close(), "关闭授权码" }
                }
            }
        }
    }
}

pub fn qr_script(address: &str) -> String {
    qr_payload_script(
        &qr_payload(address),
        "device-address-qr-code",
        "RPI Work 节点地址二维码",
        "",
    )
}

fn qr_payload_script(payload: &str, target: &str, label: &str, guard: &str) -> String {
    format!(
        "{};try{{const text={};const q=qrcode(0,'M');q.addData(text,'Byte');q.make();const el=document.getElementById('{}');if(!el)throw new Error();{}el.innerHTML=q.createSvgTag({{cellSize:4,margin:12,scalable:true,alt:'{}'}});dioxus.send(true);}}catch(_){{dioxus.send(false);}}",
        QR_LIBRARY,
        serde_json::to_string(payload).unwrap_or_else(|_| "\"\"".into()),
        target,
        guard,
        label,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn form_submit_callbacks_are_platform_independent() {
        let source = include_str!("devices.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        assert!(source.contains("onconnect: EventHandler<()>"));
        assert!(source.contains("onlogin: EventHandler<()>"));
        assert!(!source.contains("SerializedMouseData"));
    }

    #[test]
    fn authorization_payload_is_exact_and_password_free() {
        let raw = authorization_payload(
            "https://example.test",
            "test-only-ticket",
            1120,
            "node-a",
            "node-a",
            1000,
        )
        .unwrap();
        let value: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(value.as_object().unwrap().len(), 6);
        assert_eq!(value["v"], 1);
        assert_eq!(value["type"], "rpi-work-node-auth");
        assert_eq!(value["address"], "https://example.test");
        assert_eq!(value["expiresAt"], 1120);
        assert_eq!(value["nodeId"], "node-a");
        assert_eq!(value["ticket"].as_str(), Some("test-only-ticket"));
        assert!(!raw.contains("password"));
        // Ordinary browser scanning must not treat an authorization as an address.
        assert!(connection_address(&raw, DEFAULT_REMOTE_PORT).is_err());
    }

    #[test]
    fn authorization_rejects_wrong_node_expiry_and_unreachable_addresses() {
        for (address, expiry, node, expected) in [
            ("https://example.test", 1120, "node-b", "node-a"),
            ("https://example.test", 1000, "node-a", "node-a"),
            ("https://example.test", 1121, "node-a", "node-a"),
            ("https://example.test", 1120, "", ""),
            ("http://localhost:3031", 1120, "node-a", "node-a"),
            ("http://127.0.0.1:3031", 1120, "node-a", "node-a"),
            ("http://[::1]:3031", 1120, "node-a", "node-a"),
            ("local://desktop", 1120, "node-a", "node-a"),
        ] {
            assert!(
                authorization_payload(address, "test-only-ticket", expiry, node, expected, 1000)
                    .is_err()
            );
        }
        assert!(
            authorization_payload("https://example.test", "", 1120, "node-a", "node-a", 1000)
                .is_err()
        );
    }

    #[test]
    fn authorization_renderer_has_expiry_pixel_removal() {
        let script = qr_payload_script(
            "test-only-payload",
            "device-auth-qr-code",
            "授权二维码",
            "if(Date.now()>=1120000)throw new Error();setTimeout(()=>el.replaceChildren(),1000);",
        );
        assert!(script.contains("device-auth-qr-code"));
        assert!(script.contains("replaceChildren()"));
        assert!(qr_script("https://example.test").contains("device-address-qr-code"));
    }

    #[test]
    fn empty_share_address_explains_missing_qr_on_all_clients() {
        for owned in [false, true] {
            for value in ["", "  "] {
                let error = phone_share_address(value, owned).unwrap_err();
                assert!(error.contains("尚未配置可分享地址"));
                assert!(error.contains("无法生成二维码"));
                assert!(error.contains("HTTP(S)"));
                assert!(!error.contains("invalid public HTTP address"));
            }
        }
    }

    #[test]
    fn address_validation_rejects_credentials_and_scripts() {
        assert!(normalize_address("javascript:alert(1)", true).is_err());
        assert!(normalize_address("https://user:pass@example.test:3030", true).is_err());
        assert!(normalize_address("https://localhost:3030", true).is_err());
        assert_eq!(
            normalize_address("https://example.test:3030/", true).unwrap(),
            "https://example.test:3030"
        );
    }

    #[test]
    fn qr_contains_only_versioned_address_data() {
        let payload: serde_json::Value =
            serde_json::from_str(&qr_payload("https://example.test:3030")).unwrap();
        assert_eq!(payload["v"], 1);
        assert_eq!(payload["type"], "rpi-work-node-address");
        assert_eq!(payload["address"], "https://example.test:3030");
        assert!(!qr_payload("https://example.test:3030").contains("password"));
        assert_eq!(
            connection_address(&payload.to_string(), DEFAULT_REMOTE_PORT).unwrap(),
            "https://example.test:3030"
        );
    }

    #[test]
    fn local_rpc_identity_is_not_a_shareable_node_address() {
        let local = crate::app::local_node::LOCAL_ADDRESS;
        assert!(connection_address(local, DEFAULT_REMOTE_PORT).is_err());
        assert!(connection_address(&qr_payload(local), DEFAULT_REMOTE_PORT).is_err());
    }

    #[test]
    fn connection_data_rejects_unknown_versions_and_local_phone_hosts() {
        for raw in [
            r#"{"v":2,"type":"rpi-work-node-address","address":"https://example.test"}"#,
            r#"{"v":1,"type":"rpi-work-node-address","address":"https://example.test","password":"secret"}"#,
            "https://example.test:65536",
            "https://example.test:0",
            "https://example.test:bad",
            "https://localhost./",
            "http://127.1/",
            "http://2130706433/",
            "http://[::1]/",
            "https://user@example.test/",
            "https://example.test/path",
        ] {
            assert!(
                connection_address(raw, DEFAULT_REMOTE_PORT).is_err(),
                "accepted {raw}"
            );
        }
        assert_eq!(
            connection_address("https://[2001:db8::1]:3030/", DEFAULT_REMOTE_PORT).unwrap(),
            "https://[2001:db8::1]:3030"
        );
    }

    #[test]
    fn configured_remote_port_completes_scanned_and_typed_addresses() {
        assert_eq!(
            connection_address("http://10.100.100.9", 18443).unwrap(),
            "http://10.100.100.9:18443"
        );
        assert_eq!(
            connection_address("http://10.100.100.9:13031", 18443).unwrap(),
            "http://10.100.100.9:13031"
        );
        assert_eq!(
            connection_address(&qr_payload("http://node.test"), 18443).unwrap(),
            "http://node.test:18443"
        );
        assert_eq!(
            connection_address("https://node.test", 18443).unwrap(),
            "https://node.test"
        );
        assert_eq!(
            connection_address("http://node.test", DEFAULT_REMOTE_PORT).unwrap(),
            "http://node.test:13031"
        );
    }
}
