use crate::components::Icon;
use crate::connections::{ConnectionStore, NodeHistory, Phase, normalize_address};
use dioxus::core::Task;
use dioxus::prelude::*;
use work_client::{AuthState, Capabilities, WorkClient};

const QR_LIBRARY: &str = include_str!("../../../prototype/vendor/qrcode.js");
const QR_DECODER: &str = include_str!("../../../prototype/vendor/jsQR.js");
const QR_FRAME_DECODER: &str = r"
const decodeFrame=source=>{
    const w=source.videoWidth||source.width,h=source.videoHeight||source.height;
    if(!w||!h)return '';
    const scale=Math.min(1,1200/Math.max(w,h)),canvas=document.createElement('canvas');
    canvas.width=Math.round(w*scale);canvas.height=Math.round(h*scale);
    const context=canvas.getContext('2d',{willReadFrequently:true});
    context.drawImage(source,0,0,canvas.width,canvas.height);
    const pixels=context.getImageData(0,0,canvas.width,canvas.height);
    return window.jsQR(pixels.data,canvas.width,canvas.height)?.data||'';
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
        r"{QR_DECODER};{QR_FRAME_DECODER}
{CAMERA_CLEANUP}
const epoch=window.__rpiWorkCameraEpoch;
const video=document.getElementById('device-camera');
let sent=false;
let cancelPermission;
const cancelled=new Promise(resolve=>{{cancelPermission=()=>resolve(null);}});
const c={{stream:null,frame:0,observer:null,finish:(value)=>{{
    if(sent)return;sent=true;
    cancelPermission();
    c.stream?.getTracks().forEach(t=>t.stop());
    if(c.frame)cancelAnimationFrame(c.frame);
    c.observer?.disconnect();
    document.removeEventListener('visibilitychange',hidden);
    window.removeEventListener('pagehide',close);
    if(video && video.srcObject===c.stream){{video.pause();video.srcObject=null;video.hidden=true;}}
    if(window.__rpiWorkCamera===c)window.__rpiWorkCamera=null;
    dioxus.send(value);
}}}};
const close=()=>{{if(window.__rpiWorkCamera===c)window.__rpiWorkCameraEpoch++;c.finish('');}};
const hidden=()=>{{if(document.hidden)close();}};
const valid=()=>!sent && window.__rpiWorkCameraEpoch===epoch && video?.isConnected && !document.hidden;
window.__rpiWorkCamera=c;
document.addEventListener('visibilitychange',hidden);
window.addEventListener('pagehide',close);
c.observer=new MutationObserver(()=>{{if(!video?.isConnected)close();}});
c.observer.observe(document.documentElement,{{childList:true,subtree:true}});
if(!valid()||!window.isSecureContext||!navigator.mediaDevices?.getUserMedia){{c.finish('ERR:当前环境不能打开相机，请使用相册扫码。');}}
else{{try{{
    const permission=navigator.mediaDevices.getUserMedia({{video:{{facingMode:{{ideal:'environment'}}}},audio:false}}).then(stream=>{{
        if(!valid()){{stream.getTracks().forEach(t=>t.stop());return null;}}
        return stream;
    }});
    const stream=await Promise.race([permission,cancelled]);
    if(!stream||!valid()){{stream?.getTracks().forEach(t=>t.stop());c.finish('');}}
    else{{
        c.stream=stream;video.srcObject=stream;video.hidden=false;
        await video.play();
        if(!valid()){{c.finish('');}}
        else{{
            let detector;try{{if(window.BarcodeDetector)detector=new BarcodeDetector({{formats:['qr_code']}});}}catch(_){{}}
            const loop=async()=>{{
                if(!valid()){{c.finish('');return;}}
                try{{let value='';if(detector){{try{{value=(await detector.detect(video))[0]?.rawValue||'';}}catch(_){{}}}}
                    if(!valid()){{c.finish('');return;}}
                    if(!value)value=decodeFrame(video);
                    if(value){{c.finish(value);return;}}
                }}catch(_){{if(!valid()){{c.finish('');return;}}}}
                c.frame=requestAnimationFrame(loop);
            }};
            c.frame=requestAnimationFrame(loop);
        }}
    }}
}}catch(e){{c.finish(valid()?'ERR:'+(e.name==='NotAllowedError'?'未获得相机权限，请重试或使用相册扫码。':'无法打开相机，请检查设备或使用相册扫码。'):'');}}}}
"
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

#[component]
pub fn DevicesPage(
    shared_client: Signal<Option<WorkClient>>,
    requests: Signal<Vec<Task>>,
    base_url: Signal<String>,
    status: Signal<String>,
    auth_state: Signal<Option<AuthState>>,
    password: Signal<String>,
    capabilities: Signal<Option<Capabilities>>,
    onconnect: EventHandler<MouseEvent>,
    onlogin: EventHandler<MouseEvent>,
    mobile_client: bool,
) -> Element {
    let mut connection = use_context::<ConnectionStore>();
    let mut qr_open = use_signal(|| false);
    let mut remote_open = use_signal(|| false);
    let mut manual_open = use_signal(|| false);
    let mut manual_address = use_signal(String::new);
    let mut phone_address = use_signal(String::new);
    let mut error = use_signal(String::new);
    let mut scanner_status = use_signal(String::new);
    let mut scanning = use_signal(|| false);
    let mut camera_epoch = use_signal(|| 0u64);
    let mut copy_status = use_signal(String::new);
    let mut advertised_address =
        use_signal(|| base_url.read().trim().trim_end_matches('/').to_owned());
    let address = advertised_address
        .read()
        .trim()
        .trim_end_matches('/')
        .to_owned();
    let current_address = connection.address.read().clone();
    let phase = connection.phase.read().clone();
    let connection_status = phase.label().to_owned();
    let busy = phase.busy();
    let online = matches!(phase, Phase::Online);
    let local_node = normalize_address(&current_address, false).is_ok()
        && normalize_address(&current_address, true).is_err();
    use_effect(use_reactive((&mobile_client,), move |(_mobile,)| {
        qr_open.set(false);
        manual_open.set(false);
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
    let connect_address = move |event: MouseEvent| {
        let candidate = phone_address.read().clone();
        match connection_address(&candidate) {
            Ok(value) => {
                base_url.set(format!("{value}/"));
                error.set(String::new());
                onconnect.call(event);
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
        if !*qr_open.read() {
            return;
        }
        let address = advertised_address.read().clone();
        match normalize_address(&address, true) {
            Ok(address) => {
                error.set(String::new());
                spawn(async move {
                    if !matches!(
                        document::eval(&qr_script(&address)).recv::<bool>().await,
                        Ok(true)
                    ) {
                        error.set("无法生成地址二维码，请复制连接信息。".into());
                    }
                });
            }
            Err(message) => {
                error.set(message);
                let _ = document::eval(
                    "document.getElementById('device-address-qr-code')?.replaceChildren();",
                );
            }
        }
    });

    let mut read_album = move |native_picker: bool| {
        manual_open.set(false);
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

    rsx! {
        style { {include_str!("devices.css")} }
        div { class: "devices-page",
            if mobile_client {
                div { class: "page-heading",
                    div { h1 { "连接我的电脑" } p { "扫码连接电脑，也可以手动输入节点地址。" } }
                }
            }
            if !mobile_client {
                div { class: "overline", "YOUR COMPUTER. YOUR AGENT. EVERYWHERE." }
                div { class: "page-heading connection-page-heading",
                    div {
                        h1 { "电脑运行，手机接续" }
                        p { "PC 提供 Agent 服务，手机作为客户端查看、发送和确认。" }
                    }
                    button { class: "button primary", onclick: move |_| qr_open.set(true), Icon { name: "phone".to_owned() } "手机连接这台电脑" }
                }
                div { class: "connection-layout",
                    section { class: "server-overview",
                        div { class: "server-identity",
                            span { class: "connection-node-icon", Icon { name: "monitor".to_owned() } }
                            div { h2 { if local_node { "这台电脑" } else { "当前执行节点" } } p { if local_node { "本机执行节点 · rpi-http" } else { "远程执行节点 · rpi-http" } } }
                            span { class: if online { "status-badge completed" } else { "status-badge" }, i {}, if online { "已连接" } else if busy { "连接中" } else { "未连接" } }
                        }
                        div { class: "connection-topology",
                            span { Icon { name: "monitor".to_owned() } "PC 服务端" small { "模型 / 工具 / 项目文件" } }
                            i { "HTTP + SSE" }
                            span { Icon { name: "phone".to_owned() } "手机客户端" small { "对话 / 变更 / 回答确认" } }
                        }
                        div { class: "node-address", small { "当前执行节点地址" } code { "{current_address}" } }
                        div { class: "server-details",
                            div { span { "服务状态" } strong { role: "status", "{connection_status}" } }
                            div { span { "连接方式" } strong { if local_node { "本机" } else { "远程节点" } } }
                            div { span { "模型凭据" } strong { "保留在节点" } }
                        }
                        div { class: "connection-server-actions",
                            button { class: "button", disabled: busy, onclick: move |event| {
                                if local_node { base_url.set(connection.address.read().clone()); }
                                else { base_url.set("http://127.0.0.1:3030/".into()); }
                                remote_open.set(false);
                                error.set(String::new());
                                onconnect.call(event);
                            }, if local_node { "连接本机节点" } else { "切回本机节点" } }
                            button { class: "button", onclick: move |_| remote_open.set(!remote_open()), "连接其他节点" }
                        }
                        if *remote_open.read() {
                            div { class: "remote-node-form",
                                div { class: "form-field",
                                    label { r#for: "node-url", "节点地址" }
                                    input { id: "node-url", r#type: "url", value: "{base_url}", autocomplete: "url", placeholder: "https://其他电脑地址:3030/", oninput: move |event| base_url.set(event.value()) }
                                }
                                button { class: "button primary", onclick: move |event| {
                                    let candidate = normalize_address(&base_url.read(), false);
                                    match candidate {
                                        Ok(address) => { base_url.set(format!("{address}/")); error.set(String::new()); onconnect.call(event); }
                                        Err(message) => error.set(message),
                                    }
                                }, "连接节点" }
                            }
                        }
                        p { class: "connection-form-error", role: "alert", "{error}" }
                        if let Some(auth) = auth_state.read().as_ref() {
                            if auth.required && !auth.authenticated {
                                div { class: "form-field", label { r#for: "device-node-password", "节点密码" } input { id: "device-node-password", r#type: "password", autocomplete: "current-password", value: "{password}", oninput: move |event| password.set(event.value()) } }
                                button { class: "button primary", disabled: busy || password.read().is_empty(), onclick: move |event| onlogin.call(event), if busy { "正在登录…" } else { "登录" } }
                            } else if auth.authenticated && online { p { class: "meta", "节点已认证。" } }
                        }
                        if let Some(caps) = capabilities.read().as_ref() { p { class: "connection-help", "协议 {caps.protocol} · {caps.supported_commands.len()} 个命令 · {caps.supported_actions.len()} 个动作" } }
                        p { class: "connection-help", "同一局域网使用电脑可达地址；远程连接使用已配置的 HTTPS 或专用网络入口。" }
                    }
                    aside { class: "connection-steps",
                        h3 { "连接只需三步" }
                        div { b { "1" } span { "电脑显示二维码" small { "地址二维码，不包含密码" } } }
                        div { b { "2" } span { "手机扫码并核对电脑" small { "识别后确认节点地址" } } }
                        div { b { "3" } span { "登录并加载项目" small { "需要密码时在手机登录" } } }
                    }
                }
                section { class: "node-history", aria_label: "最近连接的节点",
                    div { class: "connection-clients-heading",
                        h2 { "最近连接" }
                        if !connection.history.read().recent.is_empty() {
                            button { class: "button", onclick: move |_| connection.history.set(NodeHistory::default()), "清除连接记录" }
                        }
                    }
                    p { class: "connection-help", "只保存成功连接的地址；重开应用会尝试连接最近节点，密码不会保存。清除记录不影响当前连接。" }
                    if connection.history.read().recent.is_empty() { div { class: "connection-empty", "成功连接节点后，地址会显示在这里。" } }
                    for address in connection.history.read().recent.clone() {
                        div { class: "node-history-item",
                            code { "{address}" }
                            button { class: "button", aria_label: format!("连接 {address}"), onclick: move |event| {
                                base_url.set(format!("{address}/")); error.set(String::new()); onconnect.call(event);
                            }, "连接" }
                        }
                    }
                }
            } else {
                section { class: "mobile-connect-form",
                        div { class: "pair-scanner",
                            video { id: "device-camera", playsinline: true, muted: true, hidden: true }
                            if !*scanning.read() { div { id: "scanner-placeholder", Icon { name: "image".to_owned() } strong { "扫描电脑屏幕上的地址二维码" } span { "仅在你点击打开相机后申请权限" } } }
                            p { id: "scanner-status", role: "status", "{scanner_status}" }
                        }
                        div { class: "connection-server-actions",
                            button { class: "button primary", disabled: *scanning.read(), onclick: move |_| {
                                manual_open.set(false);
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
                                            Ok(address) => { phone_address.set(address);  scanner_status.set("已读取地址，请核对连接信息后连接。".into()); }
                                            Err(message) => scanner_status.set(message),
                                        },
                                        Ok(_) => scanner_status.set("相机已关闭，可以重新扫码。".into()),
                                        Err(_) => scanner_status.set("相机识别失败，请重新扫描或从图片识别。".into()),
                                    }
                                });
                            }, if *scanning.read() { "正在扫描 / 等待权限" } else { "打开相机" } }
                            if *scanning.read() { button { class: "button", onclick: move |_| {
                                scanning.set(false); camera_epoch += 1; scanner_status.set("相机已关闭。".into());
                                spawn(async move { let _ = document::eval(&stop_camera_script()).recv::<bool>().await; });
                            }, "关闭相机" } }
                        }
                        div { class: "form-field",
                            if cfg!(target_os = "android") {
                                button { class: "button album-scan", onclick: move |_| read_album(true), "相册扫码" }
                            } else {
                            label { r#for: "device-qr-image", class: "button album-scan", "相册扫码" }
                            input { id: "device-qr-image", hidden: true, r#type: "file", accept: "image/*", onchange: move |_| {
                                read_album(false);
                            } }
                            }
                        }

                    button {
                        class: "mobile-manual-toggle",
                        r#type: "button",
                        aria_expanded: *manual_open.read(),
                        aria_controls: "phone-manual-connection",
                        onclick: move |_| {
                            let open = !*manual_open.read();
                            manual_open.set(open);
                            error.set(String::new());
                            if open {
                                scanning.set(false);
                                camera_epoch += 1;
                                phone_address.set(String::new());
                                scanner_status.set(String::new());
                                let _ = document::eval(&format!("{CAMERA_CLEANUP}window.__rpiWorkQrAlbum?.cancel();"));
                            }
                        },
                        Icon { name: if *manual_open.read() { "chevron" } else { "chevron-right" } }
                        "手动输入地址"
                    }
                    if *manual_open.read() {
                        form {
                            id: "phone-manual-connection", class: "mobile-manual-form", novalidate: true,
                            onsubmit: move |event: FormEvent| event.prevent_default(),
                            div { class: "form-field",
                                label { r#for: "phone-node-address", "电脑节点地址" }
                                input {
                                    id: "phone-node-address", r#type: "url", inputmode: "url",
                                    autocomplete: "url", autocapitalize: "none", spellcheck: false,
                                    placeholder: "https://work.example.com:3030",
                                    aria_describedby: "phone-address-help",
                                    value: "{manual_address}",
                                    oninput: move |event| { manual_address.set(event.value()); error.set(String::new()); }
                                }
                            }
                            p { id: "phone-address-help", class: "connection-help", "填写电脑的局域网 IP 或可访问的域名，包含 http:// 或 https://，需要时带端口。" }
                            p { class: "connection-form-error", role: "alert", "{error}" }
                            button {
                                r#type: "submit", class: "button primary",
                                disabled: busy || manual_address.read().trim().is_empty(),
                                onclick: move |event| {
                                    match normalize_address(&manual_address.read(), true) {
                                        Ok(address) => { base_url.set(format!("{address}/")); error.set(String::new()); onconnect.call(event); }
                                        Err(message) => error.set(message),
                                    }
                                },
                                if busy { "正在连接…" } else { "连接电脑" }
                            }
                        }
                    }

                    if !phone_address.read().is_empty() {
                        div { class: "scanned-computer",
                            strong { "已识别电脑" }
                            code { "{phone_address}" }
                            button { class: "button primary", disabled: busy, onclick: connect_address, "连接这台电脑" }
                        }
                    }
                    if normalize_address(&current_address, true).is_ok() && !matches!(phase, Phase::Idle) {
                        div { class: "phone-current-node",
                            strong { "我的电脑" } code { "{current_address}" }
                            p { class: "meta", role: "status", "{connection_status}" }
                            if !matches!(phase, Phase::PasswordRequired | Phase::Authenticating) {
                                button { class: "button", disabled: busy, onclick: move |event| {
                                    base_url.set(connection.address.read().clone()); onconnect.call(event);
                                }, "重新连接" }
                            }
                        }
                    } else { p { class: "meta", role: "status", "{connection_status}" } }

                    if let Some(auth) = auth_state.read().as_ref() {
                        if auth.required && !auth.authenticated {
                            div { class: "form-field", label { r#for: "phone-node-password", "电脑节点密码" } input { id: "phone-node-password", r#type: "password", autocomplete: "current-password", value: "{password}", oninput: move |event| password.set(event.value()) } }
                            button { class: "button primary", disabled: busy || password.read().is_empty(), onclick: move |event| onlogin.call(event), if busy { "正在登录…" } else { "登录电脑" } }
                        } else if auth.authenticated && online { p { class: "meta", "电脑已连接并认证。" } }
                    }
                    if !*manual_open.read() { p { class: "connection-form-error", role: "alert", "{error}" } }
                }
            }
            if !connection.warning.read().is_empty() { p { class: "connection-form-error", role: "status", "{connection.warning}" } }
            if qr_open.read().to_owned() {
                crate::components::Modal { id: "device-address-qr", title: "手机连接这台电脑", class: "device-address-dialog", onclose: move |()| qr_open.set(false),
                    div { class: "modal-body",
                        p { class: "modal-help device-address-intro", "二维码只包含版本化节点地址。" }
                        div { class: "device-address-layout",
                            section { class: "device-address-qr-column",
                                div { class: "pairing-qr", id: "device-address-qr-code", aria_label: "节点地址二维码" }
                                code { class: "qr-address", "{address}" }
                            }
                            section { class: "device-address-details",
                                div { class: "form-field", label { r#for: "qr-node-address", "手机可访问的节点地址" }
                                    input { id: "qr-node-address", r#type: "url", title: "localhost 指向手机本机；分享地址不会更改节点监听配置。", value: "{advertised_address}", oninput: move |event| { advertised_address.set(event.value()); copy_status.set(String::new()); } }
                                }
                                p { class: "connection-form-error", role: "alert", "{error}" }
                                p { class: "modal-help", "手机仍需节点密码；请使用电脑可达地址。" }
                                label { r#for: "device-address-payload", "连接信息 · 可手动复制" }
                                textarea { id: "device-address-payload", rows: "3", readonly: true, value: normalize_address(&address, true).map(|value| qr_payload(&value)).unwrap_or_default() }
                                p { class: "modal-help", role: "status", "{copy_status}" }
                                div { class: "modal-actions",
                                    button {
                                class: "button",
                                onclick: move |_| {
                                    let Ok(value) = normalize_address(&advertised_address.read(), true) else { copy_status.set("请先填写手机可访问的有效地址。".into()); return; };
                                    let payload = qr_payload(&value);
                                    let script = format!("try{{if(!navigator.clipboard?.writeText)throw new Error();await navigator.clipboard.writeText({});dioxus.send(true);}}catch(_){{dioxus.send(false);}}", serde_json::to_string(&payload).expect("JSON strings serialize"));
                                    let mut evaluator = document::eval(&script);
                                    spawn(async move { copy_status.set(if matches!(evaluator.recv::<bool>().await, Ok(true)) { "已复制连接信息。" } else { "无法自动复制，请手动复制上方连接信息。" }.into()); });
                                },
                                "复制连接信息"
                                    }
                                }
                            }
                        }
                    }
                    div { class: "modal-footer", button { class: "button", onclick: move |_| qr_open.set(false), "完成" } }
                }
            }
        }
    }
}

pub fn qr_script(address: &str) -> String {
    let payload = qr_payload(address);
    format!(
        "{};try{{const text={};const q=qrcode(0,'M');q.addData(text,'Byte');q.make();const el=document.getElementById('device-address-qr-code');if(!el)throw new Error();el.innerHTML=q.createSvgTag({{cellSize:4,margin:12,scalable:true,alt:'RPI Work 节点地址二维码'}});dioxus.send(true);}}catch(_){{dioxus.send(false);}}",
        QR_LIBRARY,
        serde_json::to_string(&payload).unwrap_or_else(|_| "\"\"".into())
    )
}

#[cfg(test)]
mod tests {
    use super::*;

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
