//! Test-only host of the complete production screen. Adapters are synthetic.
#![allow(dead_code, unused_imports)]
use dioxus::prelude::*;
#[path = "../../../../../apps/work-ui/src/shared/components.rs"]
pub(crate) mod components;
#[path = "../../../../../apps/work-ui/src/shared/icons.rs"]
pub(crate) mod icons;
mod shared {
    pub(crate) use crate::components;
    pub(crate) use crate::icons;
}
#[path = "../../../../../apps/work-ui/src/features/settings/connection_panel.rs"]
mod connection_panel;
use connection_panel::ConnectionScreen;

fn main() { dioxus::launch(App); }

#[component]
fn App() -> Element {
    let mut manual = use_signal(|| true);
    let mut address = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(String::new);
    let mut hosted = use_signal(|| false);
    let mut connected = use_signal(|| false);
    let mut remembered = use_signal(|| false);
    let mut native_adapter = use_signal(|| false);
    let mut origin = use_signal(String::new);
    let mut scanning = use_signal(|| false);
    let mut status = use_signal(String::new);
    let mut target = use_signal(String::new);
    let mut submits = use_signal(|| 0usize);
    let mut confirms = use_signal(|| 0usize);
    let mut cancels = use_signal(|| 0usize);
    let mut albums = use_signal(|| 0usize);
    let mut resumes = use_signal(|| 0usize);
    let mut switches = use_signal(|| 0usize);
    let mut forgets = use_signal(|| 0usize);
    let mut shares = use_signal(|| 0usize);
    use_effect(move || {
        spawn(async move {
            if let Ok((url, native)) = document::eval("dioxus.send([location.origin,new URL(location.href).searchParams.get('adapter')==='native'])")
                .recv::<(String, bool)>().await
            { origin.set(url); native_adapter.set(native); }
        });
    });
    rsx! {
        style { {include_str!("../../../../../apps/work-ui/src/shared/theme/styles.css")} }
        style { {concat!(
            include_str!("../../../../../apps/work-ui/src/shared/theme/themes.css"),
            include_str!("../../../../../apps/work-ui/src/shared/theme/codex.css"),
            include_str!("../../../../../apps/work-ui/src/shared/theme/codex-components.css"),
            include_str!("../../../../../apps/work-ui/src/shared/theme/theme-design.css"),
            include_str!("../../../../../apps/work-ui/src/shared/theme/tool-presentation.css")
        )} }
        style { {include_str!("../../../../../apps/work-ui/src/app/entry.css")} }
        nav { aria_label: "Synthetic review controls",
            button { onclick: move |_| { let value = !busy(); busy.set(value); }, "Review busy" }
            button { onclick: move |_| { let value = if error.read().is_empty() { "测试连接错误" } else { "" }; error.set(value.into()); }, "Review error" }
            button { onclick: move |_| { let value = !hosted(); hosted.set(value); password.set(String::new()); }, "Review hosted" }
            button { onclick: move |_| { scanning.set(false); target.set("https://computer.example:13030".into()); status.set("已读取地址，请核对后连接。".into()); }, "Review scanned target" }
            button { onclick: move |_| { let value = !connected(); connected.set(value); password.set(String::new()); }, "Review connected" }
            button { onclick: move |_| { let value = !remembered(); remembered.set(value); }, "Review remembered" }
        }
        div { id: "review-panel",
            ConnectionScreen {
                manual: manual(), show_form: !connected(),
                address: if hosted() { origin() } else { address() },
                password: password(), busy: busy(), error: error(),
                fixed_address: hosted(), fixed_target: hosted(),
                current_address: if connected() { "https://computer.example:13030" } else { "" },
                current_status: if connected() { "已连接" } else { "" }, authenticated: connected(),
                password_label: if hosted() { "节点密码" } else { "电脑节点密码" },
                submit_label: if hosted() { "登录节点" } else { "连接电脑" },
                onaddress: move |value| { address.set(value); password.set(String::new()); },
                onpassword: move |value| password.set(value),
                onsubmit: move |()| { submits += 1; password.set(String::new()); },
                onchange: move |value| { manual.set(value); scanning.set(false); target.set(String::new()); password.set(String::new()); },
                scanning: scanning(), scan_status: status(), target: target(),
                album_input: !native_adapter(),
                oncamera: move |()| { let value = !scanning(); scanning.set(value); },
                onalbum: move |()| { albums += 1; },
                onconfirm: move |()| { confirms += 1; },
                oncancel: move |()| { cancels += 1; scanning.set(false); target.set(String::new()); busy.set(false); },
                oncancel_attempt: move |()| { cancels += 1; busy.set(false); },
                onresume: if remembered() { Some(EventHandler::new(move |()| { resumes += 1; })) } else { None },
                onswitch: move |()| { switches += 1; connected.set(false); manual.set(true); },
                onforget: move |()| { forgets += 1; connected.set(false); remembered.set(false); manual.set(true); },
                onshare: move |()| { shares += 1; },
            }
        }
        output { id: "review-state", "data-adapter": if native_adapter() { "native" } else { "web" },
            "submit:{submits} confirm:{confirms} cancel:{cancels} album:{albums} resume:{resumes} switch:{switches} forget:{forgets} share:{shares}"
        }
    }
}
