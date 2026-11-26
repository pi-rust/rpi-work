//! Address sharing uses the same QR envelope as the receiving clients.

use std::rc::Rc;

use gpui::{
    Bounds, Context, Div, Pixels, Task, Window, canvas, div, fill, point, prelude::*, px, size,
};
use work_viewmodel::connection::normalize_address;

use super::Shell;

#[derive(Default)]
pub(super) struct Share {
    pub(super) open: bool,
    pending: Option<Task<()>>,
    ready: Option<(String, Qr)>,
    error: Option<String>,
}

#[derive(Clone)]
struct Qr {
    side: usize,
    modules: Rc<[u8]>,
}

#[cfg(target_os = "macos")]
fn address_qr(address: &str) -> Option<Qr> {
    unsafe extern "C" {
        fn work_qr_modules(
            bytes: *const u8,
            length: usize,
            modules: *mut u8,
            capacity: usize,
        ) -> usize;
    }
    let payload = work_viewmodel::qr::seal(
        &serde_json::json!({ "v": 1, "type": "rpi-work-node-address", "address": address })
            .to_string(),
    );
    const MAX_SIDE: usize = 185;
    let mut modules = vec![0; MAX_SIDE * MAX_SIDE];
    // The native encoder fills at most `capacity` bytes and borrows both buffers for this call.
    let side = unsafe {
        work_qr_modules(
            payload.as_ptr(),
            payload.len(),
            modules.as_mut_ptr(),
            modules.len(),
        )
    };
    if !(1..=MAX_SIDE).contains(&side) {
        return None;
    }
    modules.truncate(side * side);
    Some(Qr {
        side,
        modules: modules.into(),
    })
}

#[cfg(not(target_os = "macos"))]
fn address_qr(_: &str) -> Option<Qr> {
    None
}

#[cfg(target_os = "macos")]
pub(super) fn scan_node_address(path: std::path::PathBuf) -> Result<String, String> {
    use std::io::Read;

    unsafe extern "C" {
        fn work_scan_qr(bytes: *const u8, length: usize, text: *mut u8, capacity: usize) -> usize;
    }
    const MAX_IMAGE: u64 = 20 * 1024 * 1024;
    if !std::fs::metadata(&path).is_ok_and(|info| info.is_file()) {
        return Err("请选择一张图片。".into());
    }
    let mut image = Vec::new();
    let file = std::fs::File::open(path).map_err(|_| "无法打开图片。")?;
    file.take(MAX_IMAGE + 1)
        .read_to_end(&mut image)
        .map_err(|_| "无法读取图片。")?;
    if image.len() as u64 > MAX_IMAGE {
        return Err("图片不能超过 20 MB。".into());
    }
    let mut text = [0; 8192];
    // Vision reads the image synchronously and copies at most `capacity` bytes.
    let count = unsafe { work_scan_qr(image.as_ptr(), image.len(), text.as_mut_ptr(), text.len()) };
    let raw = text
        .get(..count)
        .filter(|text| !text.is_empty())
        .and_then(|text| std::str::from_utf8(text).ok())
        .ok_or("请选择含有一个清晰二维码的图片。")?;
    let decoded = work_viewmodel::qr::open(raw)?;
    if !decoded.starts_with('{') {
        return normalize_address(&decoded, true);
    }
    let value: serde_json::Value =
        serde_json::from_str(&decoded).map_err(|_| "无法解析节点地址二维码。")?;
    if value.as_object().map(|object| object.len()) != Some(3)
        || value["v"].as_u64() != Some(1)
        || value["type"].as_str() != Some("rpi-work-node-address")
    {
        return Err("请选择 RPI Work 节点地址二维码。".into());
    }
    normalize_address(
        value["address"].as_str().ok_or("二维码缺少节点地址。")?,
        true,
    )
}

impl Qr {
    fn element(&self, compact: bool) -> impl IntoElement {
        let code = self.clone();
        canvas(
            |_, _, _| (),
            move |bounds: Bounds<Pixels>, (), window, _| {
                let max_side = bounds.size.width.min(bounds.size.height);
                let module = (max_side / (code.side + 8) as f32).floor();
                let side = module * (code.side + 8) as f32;
                let origin = bounds.origin + point((bounds.size.width - side) / 2.0, px(0.0));
                window.paint_quad(fill(Bounds::new(origin, size(side, side)), gpui::white()));
                for (row, modules) in code.modules.chunks_exact(code.side).enumerate() {
                    for (column, _) in modules
                        .iter()
                        .enumerate()
                        .filter(|(_, value)| **value < 128)
                    {
                        let position =
                            origin + point(module * (column + 4) as f32, module * (row + 4) as f32);
                        window.paint_quad(fill(
                            Bounds::new(position, size(module, module)),
                            gpui::black(),
                        ));
                    }
                }
            },
        )
        .w_full()
        .h(px(if compact { 208.0 } else { 280.0 }))
    }
}

impl Shell {
    pub(super) fn show_node_share(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.node_share = Share {
            open: true,
            ..Share::default()
        };
        self.search.clear();
        let address = normalize_address(&self.snapshot.node, false).unwrap_or_default();
        self.search.buffer.replace(None, &address);
        self.search.buffer.select_all();
        self.search.focus.focus(window);
        cx.notify();
    }

    pub(super) fn reset_node_share(&mut self) {
        self.node_share = Share::default();
    }

    pub(super) fn node_share_busy(&self) -> bool {
        self.node_share.pending.is_some()
    }

    pub(super) fn close_node_share(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.reset_node_share();
        self.search.clear();
        self.focus_settings_title(window);
        cx.notify();
    }

    pub(super) fn generate_node_qr(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.node_share.open
            || self.node_share.ready.is_some()
            || self.node_share_busy()
            || !self.search.buffer.is_committed()
        {
            return;
        }
        let (Some(node), Some(expected)) = (&self.node, self.snapshot.node_id.clone()) else {
            return;
        };
        let address = match normalize_address(&self.search.buffer.text, true) {
            Ok(address) => address,
            Err(error) => {
                self.node_share.error = Some(error);
                cx.notify();
                return;
            }
        };
        self.node_share.error = None;
        window.prevent_default();
        let check = node.verify_share_address(address.clone(), expected);
        self.node_share.pending = Some(cx.spawn_in(window, async move |this, cx| {
            let verified = check.await.unwrap_or(false);
            let _ = cx.update(|window, cx| {
                let _ = this.update(cx, |shell, cx| {
                    shell.node_share.pending = None;
                    if !verified {
                        shell.node_share.error =
                            Some("无法在此地址验证当前节点。请检查地址与节点监听设置。".into());
                    } else if let Some(code) = address_qr(&address) {
                        shell.node_share.ready = Some((address, code));
                        shell.search.clear();
                        shell.focus_settings_title(window);
                    } else {
                        shell.node_share.error = Some("无法生成地址二维码。".into());
                    }
                    cx.notify();
                });
            });
        }));
        cx.notify();
    }

    pub(super) fn node_share_form(&self, compact: bool, cx: &mut Context<Self>) -> Div {
        let palette = self.palette;
        let busy = self.node_share_busy();
        let form = if let Some((address, code)) = &self.node_share.ready {
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(self.text(address.clone(), 14.0, palette.text))
                .child(code.element(compact))
                .child(self.text(
                    "用 RPI Work 扫码获取地址，按节点要求登录。",
                    13.0,
                    palette.muted,
                ))
        } else {
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(self.text("对方可访问的节点地址", 14.0, palette.text))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .h(px(38.0))
                        .px_2()
                        .rounded(px(8.0))
                        .border_1()
                        .border_color(palette.strong_line)
                        .child(self.search_input_element(cx)),
                )
                .when(busy, |form| {
                    form.child(self.text("正在验证节点…", 13.0, palette.muted))
                })
                .when_some(self.node_share.error.as_ref(), |form, error| {
                    form.child(self.text(error.clone(), 13.0, palette.danger))
                })
        };
        form.child(
            div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    self.button("close-address-qr", "完成")
                        .tab_index(0)
                        .focus(|button| button.bg(palette.hover))
                        .on_click(
                            cx.listener(|shell, _, window, cx| shell.close_node_share(window, cx)),
                        ),
                )
                .when(!busy && self.node_share.ready.is_none(), |actions| {
                    actions.child(
                        self.button("generate-address-qr", "生成二维码")
                            .border_1()
                            .border_color(palette.line)
                            .tab_index(0)
                            .focus(|button| button.bg(palette.hover))
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.generate_node_qr(window, cx)
                            })),
                    )
                }),
        )
    }
}
