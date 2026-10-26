use crate::components::Icon;
use dioxus::core::Task;
use dioxus::prelude::*;
use work_client::{AuthState, Capabilities, WorkClient};

const QR_LIBRARY: &str = include_str!("../../../prototype/vendor/qrcode.js");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DeviceRole {
    Pc,
    Phone,
}

fn normalize_address(raw: &str, phone: bool) -> Result<String, String> {
    let value = raw.trim();
    if value.len() > 2048 || value.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("服务地址过长或包含无效字符。".into());
    }
    let Some((scheme, rest)) = value.split_once("://") else {
        return Err("请输入完整的 HTTP(S) 服务地址，例如 https://电脑地址:3030。".into());
    };
    let scheme = scheme.to_ascii_lowercase();
    if !matches!(scheme.as_str(), "http" | "https") {
        return Err("服务地址只支持 HTTP 或 HTTPS。".into());
    }
    let rest = rest.strip_suffix('/').unwrap_or(rest);
    if rest.is_empty() || rest.contains(['/', '?', '#', '@', '\\', '%']) {
        return Err("服务地址只包含协议、主机和端口，不包含路径、查询参数或账号密码。".into());
    }
    let (host, port) = if let Some(ipv6) = rest.strip_prefix('[') {
        let (host, suffix) = ipv6.split_once(']').ok_or("IPv6 地址需要完整方括号。")?;
        host.parse::<std::net::Ipv6Addr>()
            .map_err(|_| "IPv6 地址无效。")?;
        let port = if suffix.is_empty() {
            None
        } else {
            Some(suffix.strip_prefix(':').ok_or("端口格式无效。")?)
        };
        (host, port)
    } else {
        let (host, port) = rest
            .split_once(':')
            .map_or((rest, None), |(h, p)| (h, Some(p)));
        if host.is_empty()
            || !host
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.'))
            || host
                .trim_end_matches('.')
                .split('.')
                .any(|part| part.is_empty() || part.starts_with('-') || part.ends_with('-'))
        {
            return Err("服务地址的主机名无效。".into());
        }
        (host, port)
    };
    if let Some(port) = port {
        if port.is_empty()
            || !port.bytes().all(|c| c.is_ascii_digit())
            || port.parse::<u16>().ok().is_none_or(|p| p == 0)
        {
            return Err("端口必须是 1 到 65535 之间的数字。".into());
        }
    }
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    let loopback = host == "localhost"
        || host.ends_with(".localhost")
        || host == "0.0.0.0"
        || host.starts_with("127.")
        || host.parse::<std::net::Ipv6Addr>().is_ok_and(|ip| {
            ip.is_loopback()
                || ip.is_unspecified()
                || ip
                    .to_ipv4_mapped()
                    .is_some_and(|v4| v4.is_loopback() || v4.is_unspecified())
        })
        || (host.bytes().all(|c| c.is_ascii_digit())
            && host
                .parse::<u32>()
                .is_ok_and(|ip| std::net::Ipv4Addr::from(ip).is_loopback() || ip == 0));
    if phone && loopback {
        return Err(
            "手机上的 localhost / 127.0.0.1 指向手机本机，请填写电脑可达的局域网或远程地址。"
                .into(),
        );
    }
    Ok(format!("{scheme}://{rest}"))
}

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
        r"{CAMERA_CLEANUP}
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
if(!window.BarcodeDetector){{c.finish('ERR:此浏览器不支持相机识别，请粘贴连接信息或手工输入地址。');}}
else if(!valid()||!window.isSecureContext||!navigator.mediaDevices?.getUserMedia){{c.finish('ERR:当前环境不能打开相机，请粘贴连接信息。');}}
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
            const detector=new BarcodeDetector({{formats:['qr_code']}});
            const loop=async()=>{{
                if(!valid()){{c.finish('');return;}}
                try{{const codes=await detector.detect(video);
                    if(!valid()){{c.finish('');return;}}
                    if(codes[0]?.rawValue){{c.finish(codes[0].rawValue);return;}}
                }}catch(_){{if(!valid()){{c.finish('');return;}}}}
                c.frame=requestAnimationFrame(loop);
            }};
            c.frame=requestAnimationFrame(loop);
        }}
    }}
}}catch(e){{c.finish(valid()?'ERR:'+(e.name==='NotAllowedError'?'未获得相机权限，请粘贴地址。':'无法打开相机，请检查设备或粘贴地址。'):'');}}}}
"
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
) -> Element {
    let mut role = use_signal(|| DeviceRole::Pc);
    let mut qr_open = use_signal(|| false);
    let mut phone_address = use_signal(String::new);
    let mut pasted = use_signal(String::new);
    let mut error = use_signal(String::new);
    let mut scanner_status = use_signal(String::new);
    let mut scanning = use_signal(|| false);
    let mut camera_epoch = use_signal(|| 0u64);
    let mut advanced = use_signal(|| false);
    let mut copy_status = use_signal(String::new);
    let mut advertised_address =
        use_signal(|| base_url.read().trim().trim_end_matches('/').to_owned());

    let address = advertised_address
        .read()
        .trim()
        .trim_end_matches('/')
        .to_owned();
    let is_phone = *role.read() == DeviceRole::Phone;
    let mut set_role = move |next: DeviceRole| {
        role.set(next);
        scanning.set(false);
        camera_epoch += 1;
        error.set(String::new());
        spawn(async move {
            let _ = document::eval(&stop_camera_script()).recv::<bool>().await;
        });
    };
    let connect_address = move |event: MouseEvent| {
        let candidate = if *advanced.read() {
            phone_address.read().clone()
        } else {
            pasted.read().clone()
        };
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
        let _ = document::eval(CAMERA_CLEANUP);
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

    rsx! {
        style { {include_str!("devices.css")} }
        div { class: "devices-page",
            div { class: "overline", "YOUR COMPUTER. YOUR AGENT. EVERYWHERE." }
            div { class: "page-heading",
                div { h1 { if is_phone { "连接你的电脑" } else { "电脑运行，手机接续" } }
                    p { if is_phone { "扫描或粘贴电脑地址，然后使用真实节点密码登录。" } else { "查看当前节点地址并为手机提供地址二维码；本客户端不会启动电脑服务。" } }
                }
                if !is_phone { button { class: "button primary", onclick: move |_| qr_open.set(true), Icon { name: "phone".to_owned() } "显示地址二维码" } }
            }
            div { class: "connection-role-switch", aria_label: "连接角色",
                button { class: if is_phone { "" } else { "active" }, onclick: move |_| set_role(DeviceRole::Pc), Icon { name: "monitor".to_owned() } "电脑 · 服务端" }
                button { class: if is_phone { "active" } else { "" }, onclick: move |_| set_role(DeviceRole::Phone), Icon { name: "phone".to_owned() } "手机 · 客户端" }
                span { "选择此设备的连接方式" }
            }
            div { class: "connection-demo-note", role: "note", Icon { name: "info".to_owned() }
                span { "地址二维码，仍需节点密码。当前不支持单独设备授权。" }
            }
            if !is_phone {
                div { class: "connection-layout",
                    section { class: "server-overview",
                        div { class: "server-identity",
                            span { class: "connection-node-icon", Icon { name: "monitor".to_owned() } }
                            div { h2 { "这台电脑" } p { "当前浏览器客户端 · rpi-web" } }
                            span { class: "status-badge stopped", i {}, "本客户端未启动服务" }
                        }
                        div { class: "connection-topology",
                            span { Icon { name: "monitor".to_owned() } "PC 节点" small { "模型 / 工具 / 项目" } }
                            i { "HTTP + SSE" }
                            span { Icon { name: "phone".to_owned() } "手机客户端" small { "连接后使用节点 API" } }
                        }
                        div { class: "node-address",
                            label { "手机可访问的节点地址" }
                            code { if address.is_empty() { "未设置分享地址" } else { "{address}" } }
                            small { "只读展示；分享地址不会更改真实服务监听。" }
                        }
                        div { class: "server-details",
                            div { span { "服务状态" } strong { "依赖外部 rpi-web" } }
                            div { span { "密码" } strong { "连接时验证" } }
                            div { span { "设备授权" } strong { "未支持" } }
                        }
                        div { class: "connection-server-actions",
                            button { class: "button", onclick: move |_| qr_open.set(true), "设置服务地址" }
                            button { class: "button", onclick: move |_| { advanced.set(true); set_role(DeviceRole::Phone); }, "手工连接 · 高级" }
                        }
                        p { class: "connection-help", "二维码只编码版本化节点地址和类型，不编码密码。手机必须能够访问该地址；localhost / 127.0.0.1 只代表当前设备。" }
                    }
                    aside { class: "connection-steps",
                        h3 { "连接只需三步" }
                        div { b { "1" } span { "电脑显示地址二维码" small { "二维码不代表授权或邀请" } } }
                        div { b { "2" } span { "手机扫码或粘贴地址" small { "先验证协议、主机和端口" } } }
                        div { b { "3" } span { "连接节点并输入密码" small { "真实登录由节点处理" } } }
                    }
                }
                div { class: "device-status-panel",
                    h2 { "当前节点连接" }
                    div { class: "form-field",
                        label { r#for: "node-url", "节点地址" }
                        input { id: "node-url", r#type: "url", value: "{base_url}", autocomplete: "url", placeholder: "http://localhost:3030/", oninput: move |event| base_url.set(event.value()) }
                    }
                    p { class: "meta", "此电脑客户端实际连接的执行节点，与提供给手机的分享地址分别设置。" }
                    button { class: "button primary", onclick: move |event| onconnect.call(event), "连接节点" }
                    p { class: "meta", role: "status", "{status}" }
                    if let Some(auth) = auth_state.read().as_ref() {
                        p { class: "meta", if auth.required && !auth.authenticated { "节点需要密码认证。" } else if auth.authenticated { "节点已认证。" } else { "节点无需密码认证。" } }
                        if auth.required && !auth.authenticated {
                            div { class: "form-field", label { r#for: "device-node-password", "节点密码" } input { id: "device-node-password", r#type: "password", autocomplete: "current-password", value: "{password}", oninput: move |event| password.set(event.value()) } }
                            button { class: "button primary", onclick: move |event| onlogin.call(event), "登录" }
                        }
                    }
                    if let Some(caps) = capabilities.read().as_ref() { p { class: "meta", "协议 {caps.protocol} · {caps.supported_commands.len()} 个命令 · {caps.supported_actions.len()} 个动作" } }
                }
            } else {
                section { class: "mobile-connect-form",
                    div { class: "mobile-connect-tabs",
                        button { class: if *advanced.read() { "" } else { "active" }, onclick: move |_| advanced.set(false), "扫码 / 粘贴连接信息" }
                        button { class: if *advanced.read() { "active" } else { "" }, onclick: move |_| {
                            advanced.set(true); scanning.set(false); camera_epoch += 1;
                            spawn(async move { let _ = document::eval(&stop_camera_script()).recv::<bool>().await; });
                        }, "高级 · 手工地址" }
                    }
                    if *advanced.read() {
                        div { class: "field", label { r#for: "phone-address", "电脑节点地址" } input { id: "phone-address", value: "{phone_address}", inputmode: "url", autocomplete: "url", placeholder: "https://电脑地址:3030", oninput: move |event| phone_address.set(event.value()) } }
                    } else {
                        div { class: "pair-scanner",
                            video { id: "device-camera", playsinline: true, muted: true, hidden: true }
                            if !*scanning.read() { div { id: "scanner-placeholder", Icon { name: "image".to_owned() } strong { "扫描电脑屏幕上的地址二维码" } span { "仅在你点击打开相机后申请权限" } } }
                            p { id: "scanner-status", role: "status", "{scanner_status}" }
                        }
                        div { class: "connection-server-actions",
                            button { class: "button", disabled: *scanning.read(), onclick: move |_| {
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
                                            Ok(address) => { phone_address.set(address); pasted.set(value); scanner_status.set("已读取地址，请核对连接信息后连接。".into()); }
                                            Err(message) => scanner_status.set(message),
                                        },
                                        Ok(_) => scanner_status.set("相机已关闭，可粘贴连接信息或重新打开。".into()),
                                        Err(_) => scanner_status.set("相机识别失败，请粘贴连接信息。".into()),
                                    }
                                });
                            }, if *scanning.read() { "正在扫描 / 等待权限" } else { "打开相机" } }
                            button { class: "button", onclick: move |_| {
                                scanning.set(false); camera_epoch += 1; scanner_status.set("相机已关闭。".into());
                                spawn(async move { let _ = document::eval(&stop_camera_script()).recv::<bool>().await; });
                            }, "关闭相机" }
                        }
                        div { class: "form-field",
                            label { r#for: "device-qr-image", "从图片识别" }
                            input { id: "device-qr-image", class: "qr-image-input", r#type: "file", accept: "image/*", onchange: move |_| {
                                scanning.set(false); camera_epoch += 1;
                                let epoch = *camera_epoch.read();
                                scanner_status.set("正在识别图片…".into());
                                let script = format!(r"{CAMERA_CLEANUP}
                                    const file=document.getElementById('device-qr-image')?.files?.[0];
                                    let bitmap;
                                    try {{
                                        if(!file) {{ dioxus.send(''); }}
                                        else if(!window.BarcodeDetector) {{ dioxus.send('ERR:此浏览器不支持图片识别，请粘贴连接信息。'); }}
                                        else {{
                                            bitmap=await createImageBitmap(file);
                                            const codes=await new BarcodeDetector({{formats:['qr_code']}}).detect(bitmap);
                                            dioxus.send(codes[0]?.rawValue||'ERR:图片中未找到二维码，请重新选择或粘贴地址。');
                                        }}
                                    }} catch(_) {{ dioxus.send('ERR:无法识别图片，请选择清晰的地址二维码。'); }}
                                    finally {{ bitmap?.close(); }}");
                                let mut evaluator = document::eval(&script);
                                spawn(async move {
                                    let result = evaluator.recv::<String>().await;
                                    if *camera_epoch.read() != epoch { return; }
                                    match result {
                                        Ok(value) if value.starts_with("ERR:") => scanner_status.set(value[4..].to_owned()),
                                        Ok(value) if !value.is_empty() => match connection_address(&value) {
                                            Ok(address) => { phone_address.set(address); pasted.set(value); scanner_status.set("已读取图片地址，请核对后连接。".into()); }
                                            Err(message) => scanner_status.set(message),
                                        },
                                        Ok(_) => scanner_status.set("未选择图片。".into()),
                                        Err(_) => scanner_status.set("图片识别失败，请粘贴连接信息。".into()),
                                    }
                                });
                            } }
                        }
                        details { class: "pair-link-entry", open: true, summary { "粘贴连接信息" }, textarea { rows: "3", value: "{pasted}", placeholder: "粘贴地址二维码中的 JSON 或节点地址…", oninput: move |event| pasted.set(event.value()) } }
                    }
                    div { class: "connection-server-actions", button { class: "button primary", onclick: connect_address, "验证地址并连接" } }
                    p { class: "meta", role: "status", "{status}" }
                    if let Some(auth) = auth_state.read().as_ref() {
                        if auth.required && !auth.authenticated {
                            div { class: "form-field", label { r#for: "phone-node-password", "节点密码" } input { id: "phone-node-password", r#type: "password", autocomplete: "current-password", value: "{password}", oninput: move |event| password.set(event.value()) } }
                            button { class: "button primary", onclick: move |event| onlogin.call(event), "登录节点" }
                        } else if auth.authenticated { p { class: "meta", "节点已认证。" } }
                    }
                    p { class: "connection-help", "手机上的 localhost 指向手机本机，请使用电脑可达地址。连接后仍需节点密码。" }
                    p { class: "connection-form-error", role: "alert", "{error}" }
                }
            }
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
