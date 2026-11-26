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
                    .on_click(cx.listener(|shell, _, window, cx| {
                        shell.add_browser_tab(window, cx);
                    })),
            );
        tabs
    }

    pub(super) fn open_browser_from_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(target_os = "macos")]
        {
            if let Some(id) = self
                .files
                .tab
                .browser_id()
                .or_else(|| self.browser.items.last().map(|(id, _)| *id))
            {
                self.show_project_tab(Tab::Browser(id), cx);
            } else {
                self.add_browser_tab(window, cx);
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = window;
            self.show_project_tab(super::project_files::Tab::Overview, cx);
        }
    }

    #[cfg(target_os = "macos")]
    fn add_browser_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (sender, mut selection) = tokio::sync::mpsc::channel(4);
        let Some(surface) = mac::Surface::new(sender) else {
            return;
        };
        self.browser.next_id += 1;
        let id = self.browser.next_id;
        self.browser.items.push((id, surface));
        self.show_project_tab(Tab::Browser(id), cx);
        cx.spawn_in(window, async move |this, cx| {
            while let Some(selected) = selection.recv().await {
                let _ = cx.update(|window, cx| {
                    let _ = this.update(cx, |shell, cx| {
                        let docked = window.viewport_size().width > px(900.0);
                        let visible = if docked {
                            shell.overview_open
                        } else {
                            shell.overlay == Some(super::Overlay::Inspector)
                        };
                        if visible
                            && !shell.search_input_open()
                            && !shell.session_context.open
                            && !shell.composer_config.menu_open()
                            && shell.files.tab == Tab::Browser(id)
                            && shell.navigation_revision == selected.scope
                            && shell.frame.is_some()
                            && shell.session.is_some()
                        {
                            shell.open_page_comment(selected.location, window, cx);
                        }
                    });
                });
            }
        })
        .detach();
    }

    pub(super) fn browser_contents(&self, id: u64) -> AnyElement {
        #[cfg(target_os = "macos")]
        if let Some((_, surface)) = self.browser.items.iter().find(|(key, _)| *key == id) {
            if self.search_input_open()
                || self.session_context.open
                || self.composer_config.menu_open()
                || self.comment.is_some()
                || self.project_form.open
                || self.question.is_some()
                || self.session_management.open()
            {
                return gpui::div().flex_1().into_any_element();
            }
            let dark = self.dark;
            let scope = self.navigation_revision;
            let annotatable = self.frame.is_some() && self.session.is_some();
            let surface = surface.clone();
            return gpui::canvas(
                |_, _, _| (),
                move |bounds, (), window, cx| {
                    // AppKit responder changes run after the GPUI element tree has been drawn.
                    window.defer(cx, move |window, _| {
                        surface.mount(window, bounds, dark, scope, annotatable)
                    });
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
    use std::{
        ffi::{CStr, c_char, c_void},
        ptr::NonNull,
        rc::Rc,
    };
    use tokio::sync::mpsc::Sender;

    use gpui::{Bounds, Pixels, Window};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    unsafe extern "C" {
        fn work_browser_create(
            context: *const c_void,
            callback: unsafe extern "C" fn(*const c_void, u64, *const c_char),
        ) -> *mut c_void;
        fn work_browser_mount(
            view: *mut c_void,
            parent: *mut c_void,
            x: f64,
            y: f64,
            width: f64,
            height: f64,
            dark: bool,
            scope: u64,
            annotatable: bool,
        );
        fn work_browser_restore_focus(view: *mut c_void);
        fn work_browser_hide(view: *mut c_void);
        fn work_browser_destroy(view: *mut c_void);
    }

    pub(super) struct Selection {
        pub(super) scope: u64,
        pub(super) location: String,
    }

    unsafe extern "C" fn selected(context: *const c_void, scope: u64, location: *const c_char) {
        // SAFETY: WorkBrowser borrows the boxed sender until destroy, and the string for this call only.
        let sender = unsafe { &*context.cast::<Sender<Selection>>() };
        let location = unsafe { CStr::from_ptr(location) };
        if let Ok(location) = location.to_str() {
            let _ = sender.try_send(Selection {
                scope,
                location: location.to_owned(),
            });
        }
    }

    #[derive(Clone)]
    pub(super) struct Surface(Rc<View>);

    struct View {
        native: NonNull<c_void>,
        _sender: Box<Sender<Selection>>,
    }

    impl Surface {
        pub(super) fn new(sender: Sender<Selection>) -> Option<Self> {
            let sender = Box::new(sender);
            // SAFETY: GPUI invokes the click handler on the AppKit main thread. The allocation
            // stays live until View::drop clears the native callback.
            let native = NonNull::new(unsafe {
                work_browser_create(std::ptr::from_ref(&*sender).cast(), selected)
            })?;
            Some(Self(Rc::new(View {
                native,
                _sender: sender,
            })))
        }

        pub(super) fn mount(
            &self,
            window: &Window,
            bounds: Bounds<Pixels>,
            dark: bool,
            scope: u64,
            annotatable: bool,
        ) {
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
                    scope,
                    annotatable,
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
