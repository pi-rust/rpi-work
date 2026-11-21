//! Connection UI glue: reactive store, effects and platform storage transports.
//!
//! Everything that does not need a signal or a hook lives in
//! `work_viewmodel::connection`; this module mounts it in the Dioxus tree.
use dioxus::prelude::*;
use serde_json::Value;

pub(crate) use work_viewmodel::connection::*;

#[derive(Clone, Copy)]
pub struct ConnectionStore {
    pub history: Signal<NodeHistory>,
    pub ready: Signal<bool>,
    pub warning: Signal<String>,
    pub phase: Signal<Phase>,
    pub address: Signal<String>,
    pub epoch: Signal<u64>,
}

pub fn use_connections(initial_address: &str) -> ConnectionStore {
    let mut store = ConnectionStore {
        history: use_signal(NodeHistory::default),
        ready: use_signal(|| false),
        warning: use_signal(String::new),
        phase: use_signal(Phase::default),
        address: use_signal(|| initial_address.to_owned()),
        epoch: use_signal(|| 0),
    };
    use_context_provider(|| store);
    use_effect(move || {
        spawn(async move {
            #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
            let value = native_path()
                .and_then(|path| std::fs::read(path).ok())
                .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
            #[cfg(any(target_arch = "wasm32", target_os = "android"))]
            let value = document::eval("try{dioxus.send(JSON.parse(localStorage.getItem('rpi-work-nodes-v1')||'null'));}catch(_){dioxus.send(null);}")
                .recv::<Value>().await.ok();
            let restored = value.as_ref().map(NodeHistory::restore).unwrap_or_default();
            // An authenticated hosted/native entry owns its exact address. Merge
            // storage history without letting a late restore discard that owner.
            let mut store = store;
            store.history.write().merge_restore(restored);
            store.ready.set(true);
        });
    });
    use_effect(move || {
        let ready = *store.ready.read();
        let history = store.history.read().clone();
        if !ready {
            return;
        }
        spawn(async move {
            #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
            let saved = persist_native(&history);
            #[cfg(any(target_arch = "wasm32", target_os = "android"))]
            let saved = {
                let literal = serde_json::to_string(&history.value().to_string()).unwrap();
                matches!(document::eval(&format!("try{{localStorage.setItem('rpi-work-nodes-v1',{literal});dioxus.send(true);}}catch(_){{dioxus.send(false);}}" )).recv::<bool>().await, Ok(true))
            };
            store.warning.set(
                if saved {
                    ""
                } else {
                    "无法保存连接地址；本次连接仍可使用。"
                }
                .into(),
            );
        });
    });
    store
}
