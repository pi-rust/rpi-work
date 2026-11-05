//! Android entry restores only encrypted, endpoint-bound login sessions.
use serde::Deserialize;

// Intentionally not Debug or Serialize: the credential stays in memory.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AuthorizationCode {
    v: u64,
    #[serde(rename = "type")]
    kind: String,
    address: String,
    ticket: String,
    #[serde(rename = "expiresAt")]
    expires_at: u64,
    #[serde(rename = "nodeId")]
    node_id: String,
}

fn parse_code(raw: &str, now: u64) -> Result<AuthorizationCode, &'static str> {
    if raw.len() > 4096 {
        return Err("二维码过长，请重新扫码。");
    }
    let mut code: AuthorizationCode =
        serde_json::from_str(raw).map_err(|_| "不是有效的节点授权二维码，请重新扫码。")?;
    if code.v != 1 || code.kind != "rpi-work-node-auth" {
        return Err("二维码类型或版本不受支持。");
    }
    if code.ticket.len() != 64
        || !code
            .ticket
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("授权凭据无效，请重新扫码。");
    }
    // Metadata is only a sanity check; the server is authoritative for TTL.
    if code.expires_at <= now || code.expires_at.saturating_sub(now) > 180 {
        return Err("二维码已过期或时间无效，请刷新后重新扫码。");
    }
    if code.node_id.trim().is_empty() || code.node_id.chars().any(char::is_control) {
        return Err("二维码缺少有效的节点标识。");
    }
    code.address = super::connections::normalize_address(&code.address, true)
        .map_err(|_| "二维码中的节点地址无效。")?;
    Ok(code)
}

#[cfg(any(target_os = "android", test))]
#[cfg_attr(test, allow(dead_code))]
mod native {
    use super::*;
    use crate::app::{
        connection_auth::manual_prepare,
        connection_memory,
        connections::{NodePassword, with_timeout},
        entry::{AuthorizedWorkspace, ConnectionEntryFrame, ConnectionSessionControl, EntryNode},
    };
    use crate::features::settings::connection_panel::ConnectionScreen;
    use dioxus::prelude::*;
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
    };
    use work_client::WorkspaceClient;

    // Desktop tests type-check the native entry but never access a device store.
    #[cfg(target_os = "android")]
    use crate::app::connection_memory_android as secure_store;
    #[cfg(not(target_os = "android"))]
    mod secure_store {
        pub fn load() -> Result<serde_json::Value, ()> {
            Err(())
        }
        pub fn save(_: &str) -> bool {
            false
        }
        pub fn clear(_: bool) -> bool {
            false
        }
    }

    fn now() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }

    #[component]
    pub(crate) fn MobileEntry() -> Element {
        let pending = use_hook(|| Rc::new(RefCell::new(None::<AuthorizationCode>)));
        let epoch = use_hook(|| Rc::new(Cell::new(0_u64)));
        let mut task = use_signal(|| None::<dioxus::core::Task>);
        let mut manual = use_signal(|| true);
        let mut address = use_signal(String::new);
        let mut password = use_signal(NodePassword::default);
        let mut target = use_signal(String::new);
        let mut status = use_signal(String::new);
        let mut busy = use_signal(|| false);
        let mut scanning = use_signal(|| false);
        let mut album_open = use_signal(|| false);
        let mut authorized = use_signal(|| None::<EntryNode>);
        let mut remembered_address = use_signal(String::new);
        let mut memory_warning = use_signal(String::new);
        let mut cancel = {
            let pending = pending.clone();
            let epoch = epoch.clone();
            move || {
                epoch.set(epoch.get().wrapping_add(1));
                pending.borrow_mut().take();
                password.set(NodePassword::default());
                if let Some(old) = task.write().take() {
                    old.cancel();
                }
                target.set(String::new());
                busy.set(false);
                scanning.set(false);
                album_open.set(false);
                status.set(String::new());
                document::eval(&cleanup_script());
            }
        };
        let mut restart = cancel.clone();
        let mut reset_forget = cancel.clone();
        let forget = EventHandler::new(move |()| {
            reset_forget();
            if !secure_store::clear(false) {
                memory_warning.set("无法清除已保存的登录，请重试；尚未退出当前电脑。".into());
                return;
            }
            // Local deletion succeeds before navigation or optional remote logout.
            let previous = authorized.peek().clone();
            remembered_address.set(String::new());
            address.set(String::new());
            memory_warning.set(String::new());
            manual.set(true);
            authorized.set(None);
            if let Some(node) = previous {
                spawn(async move {
                    let _ = with_timeout(node.client.logout()).await;
                });
            }
        });
        use_context_provider(move || ConnectionSessionControl {
            restart: Some(EventHandler::new(move |()| {
                restart();
                manual.set(true);
                address.set(remembered_address.peek().clone());
                authorized.set(None);
            })),
            forget,
            warning: memory_warning,
            busy,
        });
        let mut remember = move |node: &EntryNode, node_id: Option<String>| {
            remembered_address.set(node.address.clone());
            let saved = connection_memory::encode(node, node_id, now());
            let (saved_ok, has_session) = saved
                .map(|(payload, has_session)| (secure_store::save(&payload), has_session))
                .unwrap_or((false, false));
            memory_warning.set(
                if !saved_ok {
                    "本次已连接，但无法安全保存登录；下次可能需要重新登录。"
                } else if !has_session {
                    "已记住电脑地址；此节点的登录状态无法保存，下次需要重新登录。"
                } else {
                    ""
                }
                .into(),
            );
        };
        let mut resume = {
            let epoch = epoch.clone();
            let mut invalidate = cancel.clone();
            move |automatic: bool| {
                invalidate();
                manual.set(true);
                let stored = match secure_store::load() {
                    Ok(value) => connection_memory::decode(value),
                    Err(()) => {
                        memory_warning.set("无法读取已保存的连接，仍可手动登录。".into());
                        return;
                    }
                };
                remembered_address.set(stored.address.clone());
                address.set(stored.address);
                if stored.unavailable {
                    let cleared = secure_store::clear(true);
                    memory_warning.set(
                        if cleared {
                            "已保存的登录不可用，请重新登录；电脑地址仍保留。"
                        } else {
                            "已保存的登录不可用，且暂时无法清除，请重新登录。"
                        }
                        .into(),
                    );
                    return;
                }
                let Some(attempt) = stored.attempt else {
                    if !remembered_address.peek().is_empty() {
                        status.set("已记住电脑地址，请输入密码或扫码登录。".into());
                    }
                    return;
                };
                if automatic
                    && !connection_memory::automatic_restore_allowed(&remembered_address.peek())
                {
                    status.set("已记住地址和登录。HTTP 连接未加密，请确认当前是可信局域网后点击“恢复上次连接”，无需再次输入密码。".into());
                    return;
                }
                busy.set(true);
                status.set("正在恢复上次连接…".into());
                let generation = epoch.get();
                let epoch = epoch.clone();
                task.set(Some(spawn(async move {
                    let result = attempt.resume(now()).await;
                    if epoch.get() != generation {
                        return;
                    }
                    busy.set(false);
                    match result {
                        Ok(node) => {
                            status.set(String::new());
                            memory_warning.set(String::new());
                            // Never recapture/extend a restored session's local expiry.
                            authorized.set(Some(node));
                        }
                        Err(failure) => {
                            if failure.discard && !secure_store::clear(true) {
                                memory_warning.set("登录已失效，但暂时无法清除保存状态。".into());
                            }
                            status.set(failure.message.into());
                        }
                    }
                })));
            }
        };
        let startup_resume = resume.clone();
        let startup_epoch = epoch.clone();
        use_future(move || {
            let mut startup_resume = startup_resume.clone();
            let startup_epoch = startup_epoch.clone();
            async move {
                // A delayed mount effect must not overwrite a user's newer edit.
                if startup_epoch.get() == 0 {
                    startup_resume(true);
                }
            }
        });
        {
            let pending = pending.clone();
            let epoch = epoch.clone();
            use_drop(move || {
                epoch.set(epoch.get().wrapping_add(1));
                pending.borrow_mut().take();
                if let Some(old) = task.peek().as_ref() {
                    old.cancel();
                }
                document::eval(&format!(
                    "{}window.__rpiMobileEntryCleanup?.();",
                    cleanup_script()
                ));
            });
        }
        // No history entries or URLs. Back/background invalidate the current attempt.
        let back_cancel = cancel.clone();
        use_future(move || {
            let mut back_cancel = back_cancel.clone();
            async move {
                let mut listener = document::eval(
                    r#"
            const back=e=>{if(e.type!=='keydown'||['Escape','BrowserBack','GoBack'].includes(e.key)){
                if(e.type==='keydown')e.preventDefault();dioxus.send(e.type);
            }};
            const hidden=()=>{if(document.hidden)back({type:'hidden'});};
            window.addEventListener('keydown',back);window.addEventListener('pagehide',back);
            document.addEventListener('visibilitychange',hidden);
            window.__rpiMobileEntryCleanup=()=>{
                window.removeEventListener('keydown',back);window.removeEventListener('pagehide',back);
                document.removeEventListener('visibilitychange',hidden);delete window.__rpiMobileEntryCleanup;
            };
        "#,
                );
                while let Ok(reason) = listener.recv::<String>().await {
                    // The native photo picker backgrounds the webview. It owns no
                    // credential yet and must be allowed to return its selection.
                    if reason == "hidden" && *album_open.peek() {
                        continue;
                    }
                    back_cancel();
                }
            }
        });
        // Every edit invalidates the in-flight request, not only submission.
        // A changed target can never receive a password entered for its predecessor.
        let onaddress = {
            let mut invalidate = cancel.clone();
            move |value: String| {
                invalidate();
                address.set(value);
            }
        };
        let onpassword = {
            let mut invalidate = cancel.clone();
            move |value: String| {
                invalidate();
                password.set(NodePassword::new(value, &address.peek()));
            }
        };
        let onchange = {
            let mut invalidate = cancel.clone();
            move |value: bool| {
                invalidate();
                manual.set(value);
            }
        };
        let onsubmit = {
            let epoch = epoch.clone();
            let mut invalidate = cancel.clone();
            move |()| {
                if busy() || !manual() {
                    return;
                }
                // Take exactly once and clear the controlled field before validation/I/O.
                let draft = std::mem::take(&mut *password.write());
                let entered_address = address.peek().clone();
                invalidate();
                let attempt = match manual_prepare(&entered_address, draft) {
                    Ok(attempt) => attempt,
                    Err(message) => {
                        status.set(message);
                        return;
                    }
                };
                address.set(attempt.address.clone());
                busy.set(true);
                let generation = epoch.get();
                let epoch = epoch.clone();
                task.set(Some(spawn(async move {
                    let result = attempt.login().await;
                    if epoch.get() != generation {
                        return;
                    }
                    // Probe identity using a fresh transport; the password and its
                    // cookie jar never accompany the unauthenticated identity probe.
                    let node_id = match &result {
                        Ok(node) => connection_memory::identity(&node.address).await,
                        Err(_) => None,
                    };
                    if epoch.get() != generation {
                        return;
                    }
                    busy.set(false);
                    match result {
                        Ok(node) => {
                            remember(&node, node_id);
                            authorized.set(Some(node));
                        }
                        Err(message) => status.set(message),
                    }
                })));
            }
        };
        let mut scan = {
            let pending = pending.clone();
            let epoch = epoch.clone();
            let mut reset = cancel.clone();
            move |album: bool| {
                if manual() || busy() {
                    return;
                }
                reset();
                let attempt = epoch.get();
                let epoch = epoch.clone();
                let pending = pending.clone();
                scanning.set(true);
                album_open.set(album);
                status.set("正在读取授权二维码…".into());
                let script = if album {
                    album_script(true)
                } else {
                    start_camera_script()
                };
                task.set(Some(spawn(async move {
                    let mut evaluator = document::eval(&script);
                    let result = evaluator.recv::<String>().await;
                    if epoch.get() != attempt {
                        return;
                    }
                    scanning.set(false);
                    album_open.set(false);
                    match result {
                        Ok(raw) if !raw.is_empty() && !raw.starts_with("ERR:") => {
                            match parse_code(&raw, now()) {
                                Ok(code) => {
                                    target.set(code.address.clone());
                                    *pending.borrow_mut() = Some(code);
                                    status.set(String::new());
                                }
                                Err(message) => status.set(message.into()),
                            }
                        }
                        Ok(raw) if raw.is_empty() => status.set("已取消，请重新扫码。".into()),
                        _ => status.set("无法识别二维码，请重试相机或相册扫码。".into()),
                    }
                })));
            }
        };
        let mut scan_album = scan.clone();
        let confirm = {
            let pending = pending.clone();
            let epoch = epoch.clone();
            move |()| {
                if busy() || manual() {
                    return;
                }
                // Consume before requesting: replay always requires a fresh scan.
                let Some(code) = pending.borrow_mut().take() else {
                    return;
                };
                target.set(String::new());
                if code.expires_at <= now() {
                    status.set("二维码已过期，请重新扫码。".into());
                    return;
                }
                let Ok(client) = WorkspaceClient::new(&code.address) else {
                    status.set("节点地址无效，请重新扫码。".into());
                    return;
                };
                busy.set(true);
                status.set("正在授权连接…".into());
                let attempt = epoch.get();
                let epoch = epoch.clone();
                task.set(Some(spawn(async move {
                    let result = with_timeout(async {
                        if client.qr_authorization_node().await?.as_deref()
                            != Some(code.node_id.as_str())
                        {
                            return Err(work_client::ClientError::AuthenticationNotConfirmed);
                        }
                        client.redeem_qr_authorization(&code.ticket).await
                    })
                    .await;
                    let address = code.address.clone();
                    let node_id = code.node_id.clone();
                    drop(code);
                    if epoch.get() != attempt {
                        return;
                    }
                    busy.set(false);
                    match result {
                        Ok(Ok(auth)) if auth.authenticated => {
                            let node = EntryNode { address, client };
                            remember(&node, Some(node_id));
                            authorized.set(Some(node));
                        }
                        _ => status.set("授权失败或连接超时，请刷新二维码后重新扫码。".into()),
                    }
                })));
            }
        };
        if let Some(node) = authorized.read().clone() {
            return rsx! { AuthorizedWorkspace { node } };
        }
        let mut cancel_camera = cancel.clone();
        let mut cancel_attempt = cancel.clone();
        let resume_action = if remembered_address.read().is_empty() {
            None
        } else {
            Some(EventHandler::new(move |()| resume(false)))
        };
        rsx! {
            ConnectionEntryFrame {
                ConnectionScreen {
                    manual: manual(), address: address(),
                    password: password.read().value.clone(),
                    busy: busy(), error: status(), warning: memory_warning(),
                    help: "成功连接后会记住电脑地址，并用系统密钥库加密保存登录状态；不会保存密码。登录过期或电脑重启后需重新验证。".to_owned(),
                    onaddress, onpassword, onsubmit, onchange,
                    onresume: resume_action, onforget: forget,
                    scanning: scanning(), scan_status: status(), target: target(),
                    oncamera: move |()| {
                        if scanning() { cancel_camera(); } else { scan(false); }
                    },
                    onalbum: move |()| scan_album(true),
                    onconfirm: confirm,
                    oncancel_attempt: move |()| cancel_attempt(),
                    oncancel: move |()| cancel(),
                }
            }
        }
    }

    const QR_DECODER: &str = include_str!("../../../../assets/vendor/jsQR.js");
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

    // Pure cleanup: never send a bool into the scanner's String channel.
    const CAMERA_CLEANUP: &str = "window.__rpiWorkCameraEpoch=(window.__rpiWorkCameraEpoch||0)+1;window.__rpiWorkCamera?.finish('');";

    fn cleanup_script() -> String {
        format!("{CAMERA_CLEANUP}window.__rpiWorkQrAlbum?.cancel();dioxus.send(true);")
    }

    fn start_camera_script() -> String {
        format!(
            "{QR_DECODER};{QR_FRAME_DECODER}\n{CAMERA_CLEANUP}\n{}",
            include_str!("../features/settings/device_camera.js")
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
}
#[cfg(target_os = "android")]
pub(crate) use native::MobileEntry;

#[cfg(test)]
mod tests {
    use super::*;
    fn payload() -> serde_json::Value {
        serde_json::json!({"v":1,"type":"rpi-work-node-auth","address":"https://node.example:3030/","ticket":"a".repeat(64),"expiresAt":1120,"nodeId":"node-1"})
    }
    #[test]
    fn validates_strict_protocol_and_normalizes_target() {
        let code =
            parse_code(&payload().to_string(), 1000).unwrap_or_else(|_| panic!("valid code"));
        assert_eq!(code.address, "https://node.example:3030");
        for (key, replacement) in [
            ("v", serde_json::json!(2)),
            ("v", serde_json::json!("1")),
            ("type", serde_json::json!("rpi-work-node-address")),
            ("ticket", serde_json::json!("A".repeat(64))),
            ("ticket", serde_json::json!("a".repeat(63))),
            ("expiresAt", serde_json::json!(1000)),
            ("expiresAt", serde_json::json!(1181)),
            ("expiresAt", serde_json::json!("1120")),
            ("nodeId", serde_json::json!(" ")),
            ("address", serde_json::json!("http://localhost:3030")),
            (
                "address",
                serde_json::json!("https://node.example/?ticket=secret"),
            ),
        ] {
            let mut value = payload();
            value[key] = replacement;
            assert!(parse_code(&value.to_string(), 1000).is_err());
        }
        let mut extra = payload();
        extra["password"] = serde_json::json!("unused");
        assert!(parse_code(&extra.to_string(), 1000).is_err());
        for key in ["v", "type", "address", "ticket", "expiresAt", "nodeId"] {
            let mut missing = payload();
            missing.as_object_mut().unwrap().remove(key);
            assert!(parse_code(&missing.to_string(), 1000).is_err());
        }
        for raw in ["https://node.example", "null", "[]", &" ".repeat(4097)] {
            assert!(parse_code(raw, 1000).is_err());
        }
        let duplicate = payload().to_string().replacen("{", "{\"v\":1,", 1);
        assert!(parse_code(&duplicate, 1000).is_err());
    }
}
