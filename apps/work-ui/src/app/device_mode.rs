use dioxus::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DeviceMode {
    Desktop,
    Mobile,
}

impl DeviceMode {
    pub const fn is_mobile(self) -> bool {
        matches!(self, Self::Mobile)
    }
}

pub fn use_device_mode() -> Signal<DeviceMode> {
    #[allow(unused_mut)]
    let mut mode = use_signal(|| {
        if cfg!(target_os = "android") {
            DeviceMode::Mobile
        } else {
            DeviceMode::Desktop
        }
    });
    #[cfg(target_arch = "wasm32")]
    use_effect(move || {
        spawn(async move {
            let mut stream = document::eval(
                r"
                window.__rpiDeviceModeDispose?.();
                const narrow=matchMedia('(max-width:700px)');
                const mobile=()=>navigator.userAgentData?.mobile || /Android|iPhone|iPad|iPod/i.test(navigator.userAgent) || (navigator.platform==='MacIntel' && navigator.maxTouchPoints>1) || narrow.matches;
                const update=()=>dioxus.send(Boolean(mobile()));
                narrow.addEventListener('change',update);
                window.__rpiDeviceModeDispose=()=>narrow.removeEventListener('change',update);
                update();
            ",
            );
            while let Ok(mobile) = stream.recv::<bool>().await {
                let next = if mobile {
                    DeviceMode::Mobile
                } else {
                    DeviceMode::Desktop
                };
                if *mode.peek() != next {
                    mode.set(next);
                }
            }
        });
    });
    #[cfg(target_arch = "wasm32")]
    use_drop(|| {
        let _ = document::eval("window.__rpiDeviceModeDispose?.();");
    });
    mode
}
