//! Test harness only. A loopback development adapter, not a production node API.
use crate::features::extensions::{CSS, UiWidgets};
use dioxus::prelude::*;
use serde_json::Value;
use work_core::extensions::ExtensionManifest;

#[component]
pub fn ExtensionDemo() -> Element {
    let mut endpoint = use_signal(String::new);
    let mut active_endpoint = use_signal(String::new);
    let mut connected = use_signal(|| false);
    let mut connection_generation = use_signal(|| 0_u64);
    let mut snapshot = use_signal(|| None::<Value>);
    let mut failure = use_signal(String::new);
    let mut visible = use_signal(|| true);
    use_effect(move || {
        spawn(async move {
            let address = document::eval(
                "dioxus.send(new URLSearchParams(location.hash.slice(1)).get('demo') || '')",
            )
            .recv::<String>()
            .await;
            if let Ok(address) = address {
                endpoint.set(address);
            }
        });
    });
    use_drop(|| {
        let _ = document::eval(
            "window.__workExtensionDemo?.close();delete window.__workExtensionDemo;",
        );
    });
    let parsed = snapshot.read().as_ref().and_then(|value| {
        let manifest =
            serde_json::from_value::<ExtensionManifest>(value["manifest"].clone()).ok()?;
        manifest.validate().ok()?;
        Some(manifest)
    });
    let busy = snapshot
        .read()
        .as_ref()
        .is_some_and(|value| value["busy"] == true);
    rsx! {
        style { "{CSS}" }
        style { "html, body {{ height: auto; min-height: 100%; }}" }
        section { class: "extension-contract", style: "max-width:760px;margin:auto;padding:16px", aria_label: "Real extension demo",
            h1 { "真实原生扩展 Demo" }
            p { "真实 rpi → 已安装 DLL → 工具 partial RPC 事件 → 开发适配器 → 受控 Dioxus widget。模型为本地固定 fixture，不调用外部 API。" }
            p { "这是独立测试入口，不代表正式应用已支持通用插件安装或 live UI 协商。地址中的 token 仅用于本次 loopback Demo。" }
            label { "Demo endpoint" input { aria_label: "Demo endpoint", style: "width:100%", r#type: "password", disabled: *connected.read(), value: "{endpoint}", oninput: move |event| endpoint.set(event.value()) } }
            button { class: "button", disabled: *connected.read(), onclick: move |_| {
                let address = endpoint.read().clone();
                active_endpoint.set(address.clone());
                connection_generation += 1;
                let generation = *connection_generation.read();
                snapshot.set(None); failure.set(String::new()); connected.set(true);
                spawn(async move {
                    let address = serde_json::to_string(&address).unwrap();
                    let script = format!(r#"
                        window.__workExtensionDemo?.close();
                        try {{
                            const url = new URL({address});
                            if(url.protocol !== 'http:' || !['127.0.0.1','localhost'].includes(url.hostname) || !url.searchParams.has('token')) throw Error('Expected a token-protected loopback HTTP endpoint');
                            url.pathname='/events';
                            const source = new EventSource(url); window.__workExtensionDemo=source;
                            source.onmessage=event=>{{try{{dioxus.send({{snapshot:JSON.parse(event.data)}})}}catch(_){{source.close();dioxus.send({{error:'Invalid event JSON'}})}}}};
                            source.onerror=()=>{{source.close();dioxus.send({{error:'Demo stream disconnected'}})}};
                        }} catch(error) {{dioxus.send({{error:String(error)}})}}
                    "#);
                    let mut stream = document::eval(&script);
                    while let Ok(value) = stream.recv::<Value>().await {
                        if *connection_generation.read() != generation { break; }
                        if let Some(error) = value["error"].as_str() {
                            failure.set(error.to_owned()); connected.set(false); break;
                        }
                        let frame = &value["snapshot"];
                        match serde_json::from_value::<ExtensionManifest>(frame["manifest"].clone()) {
                            Ok(manifest) if manifest.validate().is_ok() => snapshot.set(Some(frame.clone())),
                            _ => { failure.set("Rejected invalid UI declaration".into()); connected.set(false); let _ = document::eval("window.__workExtensionDemo?.close()"); break; }
                        }
                    }
                    if *connection_generation.read() == generation { connected.set(false); }
                });
            }, "连接真实 Demo" }
            if !failure.read().is_empty() { p { role: "alert", "{failure}" } }
            if let Some(value) = snapshot.read().as_ref() {
                if let Some(error) = value["failure"].as_str() { p { role: "alert", "{error}" } }
                p { role: "status", {format!("原生安装：{} · DLL 加载：{} · Agent busy：{}", value["installed"], value["loaded"], value["busy"])} }
                p { {format!("宿主：{} · 事件序号：{}", value["hostVersion"], value["sequence"])} }
            }
            button { class: "button", disabled: !*connected.read() || snapshot.read().is_none() || busy, onclick: move |_| {
                let address = serde_json::to_string(&active_endpoint.read().clone()).unwrap();
                spawn(async move {
                    let script = format!(r#"const url=new URL({address});if(url.protocol!=='http:'||!['127.0.0.1','localhost'].includes(url.hostname)||!url.searchParams.has('token')){{dioxus.send('Rejected non-loopback endpoint')}}else{{url.pathname='/run';try{{const response=await fetch(url,{{method:'POST'}});dioxus.send(response.ok?'':await response.text())}}catch(error){{dioxus.send(String(error))}}}}"#);
                    if let Ok(error) = document::eval(&script).recv::<String>().await && !error.is_empty() { failure.set(error); }
                });
            }, "运行真实计数工具" }
            button { class: "button", aria_pressed: *visible.read(), onclick: move |_| { let current = *visible.read(); visible.set(!current); }, if *visible.read() { "隐藏实时 UI" } else { "显示实时 UI" } }
            if *visible.read() {
                if let Some(manifest) = parsed {
                    if let Some(ui) = manifest.ui {
                        for contribution in ui.contributions {
                            article { class: "extension-preview", key: "{contribution.id}",
                                h3 { "{contribution.title}" }
                                p { "实时数据源：真实工具事件（非静态预览）" }
                                UiWidgets { widgets: contribution.widgets }
                            }
                        }
                    }
                }
            } else { p { "UI 已隐藏，Agent 继续执行；再次显示会呈现最新事件状态。" } }
        }
    }
}
