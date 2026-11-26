//! Workbench browser tabs backed by AppKit child views on macOS.

#[cfg(target_os = "macos")]
use gpui::px;
use gpui::{AnyElement, Context, Window, prelude::*};

use super::Shell;
#[cfg(target_os = "macos")]
use super::project_files::Tab;
#[cfg(target_os = "macos")]
use crate::icons::icon;

#[derive(Default)]
pub(super) struct Tabs {
    #[cfg(target_os = "macos")]
    next_id: u64,
    #[cfg(target_os = "macos")]
    items: Vec<(u64, mac::Surface)>,
}

impl Tabs {
    pub(super) fn restore_composer_focus(&self) {
        #[cfg(target_os = "macos")]
        for (_, surface) in &self.items {
            surface.restore_focus();
        }
    }

    pub(super) fn hide_inactive(
        &self,
        active: Option<u64>,
        window: &Window,
        cx: &mut Context<Shell>,
    ) {
        #[cfg(target_os = "macos")]
        for (id, surface) in &self.items {
            if Some(*id) != active {
                let surface = surface.clone();
                window.defer(cx, move |_, _| surface.hide());
            }
        }
        #[cfg(not(target_os = "macos"))]
        let _ = (active, window, cx);
    }
}

impl Shell {
    pub(super) fn browser_tabs(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = self.palette;
        let tabs = gpui::div().flex().items_center().gap_1();
        #[cfg(not(target_os = "macos"))]
        let _ = (cx, palette);
        #[cfg(target_os = "macos")]
        let tabs = tabs
            .children(self.browser.items.iter().map(|(id, _)| {
                let id = *id;
                let tab = Tab::Browser(id);
                gpui::div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .rounded(px(7.0))
                    .when(self.files.tab == tab, |row| row.bg(palette.subtle))
                    .child(
                        self.button("browser-tab", "")
                            .id(("browser-tab", id))
                            .gap(px(5.0))
                            .child(icon("globe", 13.0, palette.muted))
                            .child(self.text(format!("网页 {id}"), 12.0, palette.text))
                            .on_click(
                                cx.listener(move |shell, _, _, cx| shell.show_project_tab(tab, cx)),
                            ),
                    )
                    .child(
                        self.icon_button("close-browser-tab", "x", 20.0)
                            .id(("close-browser-tab", id))
                            .on_click(cx.listener(move |shell, _, window, cx| {
                                if let Some(index) =
                                    shell.browser.items.iter().position(|(key, _)| *key == id)
                                {
                                    let (_, surface) = shell.browser.items.remove(index);
                                    window.defer(cx, move |_, _| drop(surface));
                                    if shell.files.tab == tab {
                                        let next = shell
                                            .browser
                                            .items
                                            .get(
                                                index.min(
                                                    shell.browser.items.len().saturating_sub(1),
                                                ),
                                            )
                                            .map_or(Tab::Overview, |(key, _)| Tab::Browser(*key));
                                        shell.show_project_tab(next, cx);
                                    }
                                    cx.notify();
                                }
                            })),
                    )
            }))
            .child(
                self.icon_button("add-browser-tab", "plus", 24.0)
                    .on_click(cx.listener(|shell, _, _, cx| {
                        if let Some(surface) = mac::Surface::new() {
                            shell.browser.next_id += 1;
                            let id = shell.browser.next_id;
                            shell.browser.items.push((id, surface));
                            shell.show_project_tab(Tab::Browser(id), cx);
                        }
                    })),
            );
        tabs
    }

    pub(super) fn browser_contents(&self, id: u64) -> AnyElement {
        #[cfg(target_os = "macos")]
        if let Some((_, surface)) = self.browser.items.iter().find(|(key, _)| *key == id) {
            if self.composer_config.menu_open() || self.project_form.open {
                return gpui::div().flex_1().into_any_element();
            }
            let dark = self.dark;
            let surface = surface.clone();
            return gpui::canvas(
                |_, _, _| (),
                move |bounds, (), window, cx| {
                    // AppKit responder changes run after the GPUI element tree has been drawn.
                    window.defer(cx, move |window, _| surface.mount(window, bounds, dark));
                },
            )
            .flex_1()
            .min_h(px(0.0))
            .w_full()
            .into_any_element();
        }
        let _ = id;
        gpui::div().into_any_element()
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use std::{ffi::c_void, ptr::NonNull, rc::Rc};

    use gpui::{Bounds, Pixels, Window};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    unsafe extern "C" {
        fn work_browser_create() -> *mut c_void;
        fn work_browser_mount(
            view: *mut c_void,
            parent: *mut c_void,
            x: f64,
            y: f64,
            width: f64,
            height: f64,
            dark: bool,
        );
        fn work_browser_restore_focus(view: *mut c_void);
        fn work_browser_hide(view: *mut c_void);
        fn work_browser_destroy(view: *mut c_void);
    }

    #[derive(Clone)]
    pub(super) struct Surface(Rc<View>);

    struct View {
        native: NonNull<c_void>,
    }

    impl Surface {
        pub(super) fn new() -> Option<Self> {
            // SAFETY: GPUI invokes the click handler on the AppKit main thread.
            NonNull::new(unsafe { work_browser_create() })
                .map(|view| Self(Rc::new(View { native: view })))
        }

        pub(super) fn mount(&self, window: &Window, bounds: Bounds<Pixels>, dark: bool) {
            let Ok(handle) = HasWindowHandle::window_handle(window) else {
                return;
            };
            let RawWindowHandle::AppKit(host) = handle.as_raw() else {
                return;
            };
            if bounds.size.width <= gpui::px(0.0) || bounds.size.height <= gpui::px(0.0) {
                self.hide();
                return;
            }
            // SAFETY: The handle borrows the NSView from this live GPUI window. The surface
            // is owned by the Rc and all AppKit access is on the main thread.
            unsafe {
                work_browser_mount(
                    self.0.native.as_ptr(),
                    host.ns_view.as_ptr(),
                    bounds.origin.x.into(),
                    bounds.origin.y.into(),
                    bounds.size.width.into(),
                    bounds.size.height.into(),
                    dark,
                );
            }
        }

        pub(super) fn restore_focus(&self) {
            // SAFETY: This surface is retained and focus changes run on AppKit's main thread.
            unsafe { work_browser_restore_focus(self.0.native.as_ptr()) };
        }

        pub(super) fn hide(&self) {
            // SAFETY: The Rc retains the AppKit view and its use stays on the main thread.
            unsafe { work_browser_hide(self.0.native.as_ptr()) };
        }
    }

    impl Drop for View {
        fn drop(&mut self) {
            // SAFETY: Only the final Rc drop releases the retained native view.
            unsafe { work_browser_destroy(self.native.as_ptr()) };
        }
    }
}
