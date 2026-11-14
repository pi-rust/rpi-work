use super::ConnectionScreen;
use crate::app::connections::{
    ConnectionStore, NodeHistory, NodePassword, Phase, normalize_address,
};
use crate::shared::components::Icon;
use dioxus::core::Task;
use dioxus::prelude::*;
use work_client::{AuthState, Capabilities, WorkspaceClient};

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

fn connection_address(raw: &str) -> Result<String, String> {
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
        normalize_address(
            value["address"].as_str().ok_or("二维码缺少节点地址。")?,
            true,
        )
    } else {
        normalize_address(raw, true)
    }
}

#[cfg(any(target_arch = "wasm32", target_os = "android"))]
#[derive(Clone, Default)]
struct DesktopSharing {
    listening: bool,
    enabled: bool,
    public_url: String,
    app_url: String,
    address_hint: String,
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

async fn set_desktop_sharing(enabled: bool) -> Result<(), String> {
    let client = crate::app::local_node::client().ok_or("本机节点尚未准备好".to_owned())?;
    client
        .component_capability(
            "node.sharing.write",
            serde_json::json!({"enabled": enabled}),
        )
        .await
        .map(|_| ())
        .map_err(|error| error.to_string())
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

#[derive(Clone, Copy, PartialEq)]
enum RuntimeInstallPhase {
    Idle,
    Downloading,
    Done,
}

#[component]
#[allow(unused_mut)]
fn RuntimeInstallActions() -> Element {
    let mut phase = use_signal(|| RuntimeInstallPhase::Idle);
    let mut progress = use_signal(|| 0u8);
    let mut message = use_signal(String::new);
    let install = move |_| {
        #[cfg(not(any(target_arch = "wasm32", target_os = "android", target_os = "ios")))]
        {
            if *phase.read() == RuntimeInstallPhase::Downloading {
                return;
            }
            phase.set(RuntimeInstallPhase::Downloading);
            progress.set(0);
            message.set("正在从 GitHub 下载并校验 rpi…".into());
            spawn(async move {
                let result =
                    work_update::native::rpi_runtime::install_latest(move |received, total| {
                        let next = u8::try_from(received.saturating_mul(100) / total.max(1))
                            .unwrap_or(100);
                        if *progress.peek() != next {
                            progress.set(next);
                        }
                    })
                    .await;
                match result {
                    Ok(_) => {
                        phase.set(RuntimeInstallPhase::Done);
                        progress.set(100);
                        message.set("rpi 已安装并通过校验，请完全退出后重新启动 RPI Work。".into());
                    }
                    Err(error) => {
                        phase.set(RuntimeInstallPhase::Idle);
                        message.set(format!("下载或安装失败：{error}"));
                    }
                }
            });
        }
    };
    rsx! {
        if *phase.read() == RuntimeInstallPhase::Downloading {
            progress { max: "100", value: "{progress}", aria_label: "rpi 下载进度" }
            small { "{progress}%" }
        }
        if !message.read().is_empty() {
            p { class: "runtime-install-status", role: "status", aria_live: "polite", "{message}" }
        }
        div { class: "runtime-install-actions",
            button {
                class: "button primary",
                disabled: *phase.read() == RuntimeInstallPhase::Downloading,
                onclick: install,
                if *phase.read() == RuntimeInstallPhase::Downloading { "正在下载…" } else if *phase.read() == RuntimeInstallPhase::Done { "重新下载 rpi" } else { "下载并安装 rpi" }
            }
            a { class: "button", href: "https://github.com/bigfish1913/pi-rust/releases/latest", target: "_blank", rel: "noopener noreferrer", "打开 GitHub Release" }
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
    let mut qr_open = use_signal(|| false);
    let mut pairing = use_context::<PairingControl>();
    let mut remote_open = use_signal(|| false);
    let mut plugin_help_open = use_signal(|| false);
    let mut plugin_check_busy = use_signal(|| false);
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
    let local_sharing = pairing.sharing;
    let mut sharing_state = local_sharing;
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
        match connection_address(&candidate) {
            Ok(value) => {
                base_url.set(format!("{value}/"));
                error.set(String::new());
                onconnect.call(());
            }
            Err(message) => error.set(message),
        }
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
                Ok(value) if !value.is_empty() => match connection_address(&value) {
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
                                        input { id: "node-url", r#type: "url", value: "{base_url}", autocomplete: "url", placeholder: "https://其他电脑地址:13031/", oninput: move |event| { base_url.set(event.value()); password.set(NodePassword::default()); } }
                                    }
                                    div { class: "form-field", label { r#for: "node-password", "节点密码" }
                                        input { id: "node-password", r#type: "password", autocomplete: "current-password", value: password.read().value.clone(), placeholder: "连接时输入，不会保存", oninput: move |event| { password.set(NodePassword::new(event.value(), &base_url.read())) } }
                                    }
                                    div { class: "collaboration-connect-actions",
                                        button { class: "button", onclick: move |_| remote_open.set(false), "取消" }
                                        button { class: "button primary", disabled: busy, onclick: move |_| {
                                            let address_input = base_url.read().clone();
                                            match normalize_address(&address_input, false) {
                                                Ok(address) => { base_url.set(format!("{address}/")); error.set(String::new()); onconnect.call(()); }
                                                Err(message) => error.set(message),
                                            }
                                        }, if busy { "正在连接…" } else { "连接节点" } }
                                    }
                                    if !error.read().is_empty() { p { class: "connection-form-error", role: "alert", "{error}" } }
                                }
                            }
                        }
                        section { class: "collaboration-action-card assist-card", aria_label: "允许远程协助",
                            div { class: "collaboration-action-heading",
                                span { class: "collaboration-action-icon assist", Icon { name: "shield" } }
                                div { h2 { "允许远程协助" } p { "开启后，可信设备可以访问当前电脑上的项目、会话和运行任务。" } }
                                button {
                                    class: if sharing.enabled { "sharing-switch enabled" } else { "sharing-switch" },
                                    role: "switch", aria_checked: sharing.enabled, disabled: !sharing.listening || !online,
                                    onclick: move |_| {
                                        let next = !sharing_state.peek().enabled;
                                        spawn(async move {
                                            if set_desktop_sharing(next).await.is_ok() {
                                                sharing_state.with_mut(|state| state.enabled = next);
                                            }
                                        });
                                    },
                                    span { class: "sharing-switch-track", span {} }
                                    span { if sharing.enabled { "已开启" } else { "未开启" } }
                                }
                            }
                            if !sharing.listening {
                                div { class: "plugin-guide", role: "status",
                                    Icon { name: "info" }
                                    div { strong { "需要先安装协同插件" } p { "安装 rpi-http-extension 后，才能开启远程协助。" } }
                                    button { class: "button", onclick: move |_| plugin_help_open.toggle(), if *plugin_help_open.read() { "收起指引" } else { "查看安装指引" } }
                                }
                                if *plugin_help_open.read() {
                                    div { class: "plugin-guide-steps",
                                        div { b { "1" } span { strong { "安装 rpi-http-extension" } small { "为当前节点提供 HTTP / WebSocket 远程入口。" } } }
                                        div { b { "2" } span { strong { "重启 Work 或节点" } small { "让宿主重新加载插件能力。" } } }
                                        div { b { "3" } span { strong { "重新检查插件" } small { "确认后即可使用远程协助开关。" } } }
                                        button { class: "button primary", disabled: *plugin_check_busy.read(), onclick: move |_| {
                                            plugin_check_busy.set(true);
                                            spawn(async move {
                                                sharing_state.set(desktop_sharing().await);
                                                plugin_check_busy.set(false);
                                            });
                                        }, if *plugin_check_busy.read() { "正在检查…" } else { "重新检查插件" } }
                                    }
                                }
                            } else {
                                if !owned_local && auth_state.read().as_ref().is_some_and(|auth| auth.required && !auth.authenticated) {
                                    div { class: "form-field", label { r#for: "current-node-password", "节点密码" }
                                        input { id: "current-node-password", r#type: "password", autocomplete: "current-password", value: password.read().value.clone(), placeholder: "输入节点密码", oninput: move |event| password.set(NodePassword::new(event.value(), &connection.address.read())) }
                                    }
                                    button { class: "button primary", disabled: busy || password.read().is_empty(), onclick: move |_| onlogin.call(()), if busy { "正在登录…" } else { "登录当前节点" } }
                                }
                                div { class: if sharing.enabled { "sharing-ready" } else { "sharing-ready disabled" },
                                    Icon { name: if sharing.enabled { "check" } else { "info" } }
                                    span { if sharing.enabled { "远程协助已开启，可以分享连接信息。" } else { "插件已安装，开启开关后允许其他设备访问。" } }
                                }
                                if sharing.enabled {
                                    button { class: "button", disabled: !online, onclick: move |_| { pairing.request.with_mut(authorization_offer::Request::show); }, "分享连接信息" }
                                }
                            }
                        }
                    section { class: "collaboration-action-card web-access-card", aria_label: "允许 Web 访问",
                        div { class: "collaboration-action-heading",
                            span { class: "collaboration-action-icon", Icon { name: "server" } }
                            div { h2 { "允许 Web 访问" } p { "通过浏览器访问当前工作节点的项目、会话和任务。" } }
                            span { class: "neutral-pill", if owned_local && sharing.enabled && sharing.listening && !sharing.app_url.is_empty() { "已开启" } else { "未开启" } }
                        }
                        div { class: "plugin-guide",
                            Icon { name: "shield" }
                            div {
                                strong { "与远程入口共用访问设置" }
                                p { "Web 访问随当前电脑的远程入口启停，仍需节点鉴权。" }
                            }
                            if owned_local && sharing.enabled && sharing.listening && !sharing.app_url.is_empty() {
                                a { class: "button", href: "{sharing.app_url}", target: "_blank", rel: "noopener noreferrer", "打开 Web 入口" }
                            }
                        }
                    }
                    }
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
                        if !sharing.listening {
                            p { class: "connection-form-error", role: "alert", "未检测到 rpi-http-extension，暂时无法开启远程协作。请先安装或启用宿主扩展。" }
                        }
                        button {
                            class: "sharing-switch",
                            role: "switch",
                            aria_checked: sharing.enabled,
                            disabled: !sharing.listening,
                            onclick: move |_| {
                                let next = !sharing_state.peek().enabled;
                                spawn(async move {
                                    if set_desktop_sharing(next).await.is_ok() {
                                        sharing_state.with_mut(|state| state.enabled = next);
                                    }
                                });
                            },
                            span { class: "sharing-switch-track", span {} }
                            span { if sharing.enabled { "允许手机访问" } else { "禁止手机访问" } }
                        }
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
                        if crate::app::local_node::needs_runtime_install() {
                            div { class: "runtime-install-guide", role: "alert",
                                div { class: "runtime-install-heading",
                                    Icon { name: "download" }
                                    strong { "需要安装 rpi 执行宿主" }
                                }
                                p { "RPI Work 只提供工作界面，不包含 rpi 执行程序。请先安装与系统匹配的 rpi，或将 rpi 放到 RPI Work 可执行文件旁。" }
                                ol {
                                    li { "从 GitHub Release 页面下载对应平台的 rpi；安装了 Rust 的用户也可以执行 ", code { "cargo install rpi-cli" }, "。" }
                                    li { "将 rpi.exe（Windows）或 rpi（Linux/macOS）放到 RPI Work 可执行文件旁；也可以设置 RPI_WORK_RPI_BIN 指向完整路径。" }
                                    li { "如果设置了 RPI_WORK_RPI_BIN，请修正或清除无效路径；显式配置优先级最高。" }
                                    li { "完全退出并重新启动 RPI Work。" }
                                }
                                RuntimeInstallActions {}
                            }
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
                                    input { id: "node-url", r#type: "url", value: "{base_url}", autocomplete: "url", placeholder: "https://其他电脑地址:13030/", oninput: move |event| { base_url.set(event.value()); password.set(NodePassword::default()); } }
                                }
                                div { class: "form-field",
                                    label { r#for: "node-password", "节点密码" }
                                    input { id: "node-password", r#type: "password", autocomplete: "current-password", value: password.read().value.clone(), placeholder: "输入节点密码", oninput: move |event| password.set(NodePassword::new(event.value(), &base_url.read())) }
                                }
                                button { class: "button primary", onclick: move |_| {
                                    let candidate = normalize_address(&base_url.read(), false);
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
                                        match normalize_address(&manual_address.read(), navigation.is_none()) {
                                            Ok(address) => { base_url.set(format!("{address}/")); error.set(String::new()); onconnect.call(()); }
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
                                            Ok(value) if !value.is_empty() => match connection_address(&value) {
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
        assert!(connection_address(&raw).is_err());
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
            connection_address(&payload.to_string()).unwrap(),
            "https://example.test:3030"
        );
    }

    #[test]
    fn local_rpc_identity_is_not_a_shareable_node_address() {
        let local = crate::app::local_node::LOCAL_ADDRESS;
        assert!(connection_address(local).is_err());
        assert!(connection_address(&qr_payload(local)).is_err());
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
            assert!(connection_address(raw).is_err(), "accepted {raw}");
        }
        assert_eq!(
            connection_address("https://[2001:db8::1]:3030/").unwrap(),
            "https://[2001:db8::1]:3030"
        );
    }
}
