//! Presentation only: controllers own validation, credentials and scanner lifetimes.
use crate::shared::components::Icon;
use dioxus::prelude::*;

/// Complete connection presentation, shared by entry and Settings on every host.
/// Controllers supply state and capability callbacks; no platform selects markup.
#[component]
pub(crate) fn ConnectionScreen(
    address: String,
    password: String,
    busy: bool,
    error: String,
    onaddress: EventHandler<String>,
    onpassword: EventHandler<String>,
    onsubmit: EventHandler<()>,
    #[props(default = true)] manual: bool,
    #[props(default = true)] show_form: bool,
    #[props(default = false)] fixed_address: bool,
    #[props(default = false)] fixed_target: bool,
    #[props(default = true)] show_current_node: bool,
    #[props(default = true)] show_connection_summary: bool,
    #[props(default = true)] show_password: bool,
    #[props(default)] home_address: String,
    #[props(default)] onhome: Option<EventHandler<()>>,
    #[props(default)] current_address: String,
    #[props(default)] current_status: String,
    #[props(default = false)] authenticated: bool,
    #[props(default)] warning: String,
    #[props(default)] help: Option<String>,
    #[props(default = "电脑节点密码".to_owned())] password_label: String,
    #[props(default = "连接电脑".to_owned())] submit_label: String,
    #[props(default = "DEVICES & COLLABORATION".to_owned())] eyebrow: String,
    #[props(default = "连接其他设备".to_owned())] heading: String,
    #[props(default = "连接电脑或其他工作节点，继续项目、会话和 Agent 任务。".to_owned())] description: String,
    #[props(default = false)] scanning: bool,
    #[props(default)] scan_status: String,
    #[props(default)] target: String,
    #[props(default = false)] album_input: bool,
    #[props(default)] onchange: Option<EventHandler<bool>>,
    #[props(default)] oncamera: Option<EventHandler<()>>,
    #[props(default)] onalbum: Option<EventHandler<()>>,
    #[props(default)] onconfirm: Option<EventHandler<()>>,
    #[props(default)] oncancel: Option<EventHandler<()>>,
    #[props(default)] oncancel_attempt: Option<EventHandler<()>>,
    #[props(default)] onresume: Option<EventHandler<()>>,
    #[props(default)] onforget: Option<EventHandler<()>>,
    #[props(default)] onswitch: Option<EventHandler<()>>,
    #[props(default)] onreauthenticate: Option<EventHandler<()>>,
    #[props(default)] onshare: Option<EventHandler<()>>,
    #[props(default)] children: Option<Element>,
) -> Element {
    rsx! {
        ConnectionPanel { fixed_address: fixed_target, eyebrow, heading, description,
            if show_current_node && !current_address.is_empty() {
                div { class: "phone-current-node",
                    strong { "当前连接的工作节点" } code { "{current_address}" }
                    p { class: "meta", role: "status", "{current_status}" }
                    if authenticated { p { class: "meta", "节点已连接并认证。" } }
                    p { class: "connection-help", "切换连接不会停止节点上的任务；请先保留尚未提交的输入。" }
                    div { class: "phone-current-actions",
                        if let Some(action) = onswitch {
                            button { r#type: "button", class: "button", disabled: busy,
                                onclick: move |_| action.call(()),
                                if fixed_target { "重新连接" } else { "连接其他设备" }
                            }
                        }
                        if let Some(action) = onreauthenticate {
                            button { r#type: "button", class: "button primary", disabled: busy,
                                onclick: move |_| action.call(()), "重新登录当前节点"
                            }
                        }
                        if let Some(action) = onforget {
                            button { r#type: "button", class: "button", disabled: busy,
                                onclick: move |_| action.call(()), "退出并清除登录"
                            }
                        }
                        if let Some(action) = onshare {
                            button { r#type: "button", class: "button", disabled: busy,
                                onclick: move |_| action.call(()), Icon { name: "phone" } "分享连接信息"
                            }
                        }
                    }
                    if let Some(action) = onhome {
                        div { class: "connection-home",
                            p { class: "connection-help", "入口节点：" code { "{home_address}" } }
                            p { class: "connection-help", "返回地址仅是导航提示，请核对后访问，不代表节点身份或授权。" }
                            button { r#type: "button", class: "button", disabled: busy,
                                onclick: move |_| action.call(()), "返回入口节点"
                            }
                        }
                    }
                }
            }
            if show_connection_summary && !show_current_node && !current_address.is_empty() {
                div { class: "phone-current-node collaboration-current-connection",
                    strong { "当前连接的工作节点" }
                    code { "{current_address}" }
                    p { class: "meta", role: "status", "连接已建立，可继续连接其他设备。" }
                    if let Some(action) = onswitch {
                        button { r#type: "button", class: "button", disabled: busy,
                            onclick: move |_| action.call(()), "连接其他设备"
                        }
                    }
                }
            }
            if current_address.is_empty() && !current_status.is_empty() {
                // Connection attempts can fail before any node is authorized.
                p { class: "meta", role: "status", "{current_status}" }
            }
            if show_form {
                if !fixed_target {
                    if let Some(action) = onchange { ConnectionTabs { manual, onchange: action } }
                }
                if manual || fixed_target {
                    PasswordConnectionForm {
                        address, password, busy, error,
                        fixed_address, show_password, password_label, submit_label,
                        help: help.unwrap_or_else(|| CONNECTION_HELP.to_owned()),
                        onaddress, onpassword, onsubmit,
                    }
                    if let Some(action) = onresume {
                        div { class: "phone-current-actions",
                            button { r#type: "button", class: "button", disabled: busy,
                                onclick: move |_| action.call(()), "恢复上次连接"
                            }
                            if let Some(forget) = onforget {
                                button { r#type: "button", class: "button", disabled: busy,
                                    onclick: move |_| forget.call(()), "忘记已保存的电脑"
                                }
                            }
                        }
                    }
                    if busy {
                        if let Some(action) = oncancel_attempt {
                            button { r#type: "button", class: "button",
                                onclick: move |_| action.call(()), "取消连接"
                            }
                        }
                    }
                } else {
                    ConnectionScanner {
                        scanning, status: scan_status, target, busy, album_input,
                        oncamera: move |()| { if let Some(action) = oncamera { action.call(()); } },
                        onalbum: move |()| { if let Some(action) = onalbum { action.call(()); } },
                        onconfirm: move |()| { if let Some(action) = onconfirm { action.call(()); } },
                        oncancel: move |()| { if let Some(action) = oncancel { action.call(()); } },
                    }
                    if !error.is_empty() { p { class: "connection-form-error", role: "alert", "{error}" } }
                }
            }
            if !warning.is_empty() { p { class: "connection-help", role: "status", "{warning}" } }
            {children}
        }
    }
}

const CONNECTION_HELP: &str = "填写电脑可达的完整 HTTP(S) 地址，需要时包含端口。仅连接你信任的电脑；节点要求密码时再输入，密码不会保存或放入二维码。";

#[component]
pub(crate) fn ConnectionPanel(
    children: Element,
    #[props(default = false)] fixed_address: bool,
    #[props(default = "DEVICES & COLLABORATION".to_owned())] eyebrow: String,
    #[props(default = "连接其他设备".to_owned())] heading: String,
    #[props(default = "连接电脑或其他工作节点，继续项目、会话和 Agent 任务。".to_owned())] description: String,
) -> Element {
    rsx! {
        style { {include_str!("connection_panel.css")} }
        div { class: "connection-panel-shell",
            if !eyebrow.trim().is_empty() { div { class: "overline", "{eyebrow}" } }
            if !heading.trim().is_empty() || !description.trim().is_empty() {
                div { class: "page-heading connection-panel-heading",
                    div {
                        if !heading.trim().is_empty() { h1 { "{heading}" } }
                        if !description.trim().is_empty() { p { if fixed_address { "登录当前电脑，接续项目与会话。" } else { "{description}" } } }
                    }
                }
            }
            section { class: "mobile-connect-form connection-panel", {children} }
        }
    }
}

#[component]
pub(crate) fn ConnectionTabs(manual: bool, onchange: EventHandler<bool>) -> Element {
    rsx! {
        div { class: "mobile-connect-tabs", role: "group", aria_label: "连接方式",
            button {
                r#type: "button", class: if manual { "active" } else { "" },
                aria_pressed: manual, onclick: move |_| onchange.call(true),
                Icon { name: "monitor" } "手动输入地址"
            }
            button {
                r#type: "button", class: if !manual { "active" } else { "" },
                aria_pressed: !manual, onclick: move |_| onchange.call(false),
                Icon { name: "image" } "扫码连接"
            }
        }
    }
}

#[component]
pub(crate) fn PasswordConnectionForm(
    address: String,
    password: String,
    busy: bool,
    error: String,
    onaddress: EventHandler<String>,
    onpassword: EventHandler<String>,
    onsubmit: EventHandler<()>,
    #[props(default = false)] fixed_address: bool,
    #[props(default = true)] show_password: bool,
    #[props(default = "电脑节点密码".to_owned())] password_label: String,
    #[props(default = "连接电脑".to_owned())] submit_label: String,
    #[props(default = "填写电脑可达的完整 HTTP(S) 地址，需要时包含端口。仅连接你信任的电脑；节点要求密码时再输入，密码不会保存或放入二维码。".to_owned())]
    help: String,
) -> Element {
    let disabled = busy || address.trim().is_empty();
    rsx! {
        form {
            id: "phone-manual-connection", class: "mobile-manual-form", novalidate: true,
            "aria-busy": busy,
            onsubmit: move |event: FormEvent| {
                event.prevent_default();
                if !disabled { onsubmit.call(()); }
            },
            div { class: "form-field",
                label { r#for: "phone-node-address", "电脑节点地址" }
                input {
                    id: "phone-node-address", r#type: "url", inputmode: "url",
                    autocomplete: "url", autocapitalize: "none", spellcheck: false,
                    required: true, readonly: fixed_address, disabled: busy,
                    placeholder: "https://work.example.com:13030",
                    aria_describedby: "phone-address-help", value: address,
                    oninput: move |event| { if !fixed_address { onaddress.call(event.value()); } }
                }
            }
            if show_password {
                div { class: "form-field",
                    label { r#for: "phone-node-password", "{password_label}" }
                    input {
                        id: "phone-node-password", r#type: "password", autocomplete: "current-password",
                        disabled: busy, value: password, placeholder: "输入电脑节点密码",
                        oninput: move |event| onpassword.call(event.value())
                    }
                }
            }
            p { id: "phone-address-help", class: "connection-help", "{help}" }
            if !error.is_empty() { p { class: "connection-form-error", role: "alert", "{error}" } }
            button { r#type: "submit", class: "button primary", disabled,
                if busy { "正在连接…" } else { "{submit_label}" }
            }
            if busy { p { role: "status", "aria-live": "polite", "正在验证连接，请稍候。" } }
        }
    }
}

#[component]
pub(crate) fn ConnectionScanner(
    scanning: bool,
    status: String,
    target: String,
    busy: bool,
    oncamera: EventHandler<()>,
    onalbum: EventHandler<()>,
    onconfirm: EventHandler<()>,
    oncancel: EventHandler<()>,
    #[props(default = false)] album_input: bool,
) -> Element {
    rsx! {
        div { class: "connection-scanner",
            div { class: "pair-scanner",
                video { id: "device-camera", playsinline: true, muted: true, hidden: true }
                if !scanning {
                    div { id: "scanner-placeholder",
                        Icon { name: "image" }
                        strong { "扫描电脑屏幕上的二维码" }
                        span { "仅在你点击打开相机后申请权限" }
                    }
                }
                p { id: "scanner-status", role: "status", "{status}" }
            }
            div { class: "connection-server-actions",
                button { r#type: "button", class: "button primary", disabled: busy,
                    onclick: move |_| oncamera.call(()),
                    if scanning { "关闭相机" } else { "打开相机" }
                }
                button { r#type: "button", class: "button album-scan", disabled: busy,
                    onclick: move |_| {
                        if album_input {
                            // The picker is an adapter detail; both hosts expose the same button.
                            document::eval("document.getElementById('device-qr-image')?.click();");
                        } else { onalbum.call(()); }
                    }, "相册扫码"
                }
                if album_input {
                    input { id: "device-qr-image", hidden: true,
                        r#type: "file", accept: "image/*", disabled: busy,
                        aria_label: "从相册选择二维码", onchange: move |_| onalbum.call(())
                    }
                }
            }
            if !target.is_empty() {
                div { class: "scanned-computer",
                    strong { "已识别电脑" }
                    code { "{target}" }
                    button { r#type: "button", class: "button primary", disabled: busy || target.trim().is_empty(),
                        onclick: move |_| onconfirm.call(()), "连接这台电脑"
                    }
                    button { r#type: "button", class: "button", disabled: busy,
                        onclick: move |_| oncancel.call(()), "取消"
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn password_form_submits_without_mouse_coupling() {
        let source = include_str!("connection_panel.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        let form = source
            .split("pub(crate) fn PasswordConnectionForm(")
            .nth(1)
            .unwrap()
            .split("pub(crate) fn ConnectionScanner(")
            .next()
            .unwrap();
        assert!(form.contains("onsubmit: EventHandler<()>"));
        assert!(form.contains("event.prevent_default()"));
        assert!(form.contains("onsubmit.call(())"));
        assert!(!form.contains("onclick"));
        assert!(form.contains("busy || address.trim().is_empty()"));
        assert!(!form.contains("password.trim().is_empty()"));
        assert!(form.contains("readonly: fixed_address"));
    }

    #[test]
    fn scanner_exposes_only_display_target_not_credentials() {
        let source = include_str!("connection_panel.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        let scanner = source
            .split("pub(crate) fn ConnectionScanner(")
            .nth(1)
            .unwrap();
        assert!(!scanner.contains("password"));
        assert!(!scanner.contains("ticket"));
        for id in ["device-camera", "device-qr-image", "scanner-status"] {
            assert!(scanner.contains(id));
        }
        assert!(scanner.contains("album_input: bool"));
        assert!(scanner.contains("onconfirm: EventHandler<()>"));
    }

    #[test]
    fn shared_styles_supply_mobile_controls_without_devices_css() {
        let css = include_str!("connection_panel.css");
        assert!(css.contains("min-height:44px"));
        assert!(css.contains("font-size:16px"));
        assert!(css.contains(":focus-visible"));
        assert!(css.contains("overflow-wrap:anywhere"));
        assert!(css.contains("var(--accent)"));
    }
}
