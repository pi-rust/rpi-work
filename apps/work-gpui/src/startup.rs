//! First launch uses the shared local owner and runtime installer.

use std::{path::PathBuf, sync::Arc};

use gpui::{
    App, Context, Div, FocusHandle, FontWeight, IntoElement, PathPromptOptions, Task, Window, div,
    prelude::*, px, relative,
};
use work_host::setup::{DesktopSetup, Phase, Selection, Status};

use super::{Shell, editor::Buffer, icon};
use crate::data::{Node, Snapshot};

pub(crate) fn open(cx: &mut App) -> gpui::Result<()> {
    let setup = DesktopSetup::new();
    let owner = Arc::clone(&setup);
    cx.on_app_quit(move |cx| {
        let owner = Arc::clone(&owner);
        cx.background_executor()
            .spawn(async move { owner.shutdown() })
    })
    .detach();
    cx.open_window(crate::window_options(cx), |window, cx| {
        let view = cx.new(|cx| Shell::new(Snapshot::disconnected(), None, window.appearance(), cx));
        crate::prepare_workbench(&view, window, cx);
        view.update(cx, |shell, cx| shell.start_runtime(setup, window, cx));
        view
    })?;
    Ok(())
}

pub(super) struct Setup {
    owner: Arc<DesktopSetup>,
    status: Status,
    focus: FocusHandle,
    card_focus: FocusHandle,
    path: Buffer,
    selected_path: Option<(u64, PathBuf)>,
    open: bool,
    reveal_failure: bool,
    choosing: bool,
    downloading: bool,
    loading: bool,
    read_error: Option<String>,
    _updates: Task<()>,
}

impl Setup {
    fn busy(&self) -> bool {
        self.status.busy() || self.choosing || self.loading
    }

    fn can_verify(&self) -> bool {
        !self.busy()
            && self.status.phase != Phase::Ready
            && self.path.is_committed()
            && self.executable_path().is_some()
    }

    fn can_download(&self) -> bool {
        !self.busy()
            && self.status.version.is_none()
            && (self.status.can_install
                || self.status.phase == Phase::Cancelled
                || self.downloading)
    }

    fn begin(&mut self, selection: Selection) {
        self.downloading = matches!(selection, Selection::Managed);
        self.owner.begin(selection);
        self.status = self.owner.snapshot();
    }

    fn executable_path(&self) -> Option<PathBuf> {
        if let Some((revision, path)) = &self.selected_path
            && *revision == self.path.revision
        {
            return Some(path.clone());
        }
        let path = self.path.text.trim();
        (!path.is_empty() && !path.chars().any(char::is_control)).then(|| PathBuf::from(path))
    }

    fn select_path(&mut self, path: PathBuf) {
        self.path.clear_after_send();
        self.path.replace_completion(0..0, &path.to_string_lossy());
        self.selected_path = Some((self.path.revision, path));
    }
}

impl Shell {
    fn start_runtime(
        &mut self,
        owner: Arc<DesktopSetup>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut updates = owner.subscribe();
        self.overview_open = false;
        let task = cx.spawn_in(window, async move |this, cx| {
            while updates.changed().await.is_ok() {
                let status = updates.borrow_and_update().clone();
                let result = cx.update(|window, cx| {
                    this.update(cx, |shell, cx| {
                        let Some(setup) = &mut shell.startup else {
                            return false;
                        };
                        if status.phase == Phase::Ready
                            && let Some(path) = status.path.clone()
                        {
                            setup.select_path(path);
                        }
                        setup.downloading |= status.phase == Phase::Resolving;
                        setup.status = status;
                        if setup.reveal_failure
                            && matches!(
                                setup.status.phase,
                                Phase::Resolving | Phase::Downloading | Phase::Failed
                            )
                        {
                            setup.reveal_failure = false;
                            setup.open = true;
                            setup.focus.focus(window);
                        }
                        if setup.status.phase == Phase::Ready && !setup.open {
                            shell.load_local_runtime(window, cx);
                        }
                        cx.notify();
                        true
                    })
                });
                if !matches!(result, Ok(Ok(true))) {
                    break;
                }
            }
        });
        self.startup = Some(Setup {
            status: owner.snapshot(),
            owner,
            focus: cx.focus_handle(),
            card_focus: cx.focus_handle(),
            path: Buffer::default(),
            selected_path: None,
            open: false,
            reveal_failure: true,
            choosing: false,
            downloading: false,
            loading: false,
            read_error: None,
            _updates: task,
        });
        if let Some(setup) = &mut self.startup {
            setup.begin(Selection::Automatic);
        }
    }

    pub(super) fn runtime_dialog_open(&self) -> bool {
        self.startup.as_ref().is_some_and(|setup| setup.open)
    }

    pub(super) fn runtime_path(&self) -> Option<&Buffer> {
        self.startup
            .as_ref()
            .filter(|setup| setup.open)
            .map(|setup| &setup.path)
    }

    pub(super) fn runtime_path_mut(&mut self) -> &mut Buffer {
        &mut self.startup.as_mut().expect("runtime dialog open").path
    }

    pub(super) fn verify_runtime(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(setup) = self.startup.as_mut().filter(|setup| {
            setup.open
                && !setup.busy()
                && setup.status.phase != Phase::Ready
                && setup.path.is_committed()
        }) && let Some(path) = setup.executable_path()
        {
            setup.focus.focus(window);
            setup.begin(Selection::File(path));
            cx.notify();
        }
    }

    fn load_local_runtime(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(setup) = self.startup.as_mut().filter(|setup| !setup.loading) else {
            return;
        };
        setup.loading = true;
        setup.read_error = None;
        cx.notify();
        let read = cx.background_executor().spawn(async { Node::local() });
        cx.spawn_in(window, async move |this, cx| {
            let result = read.await;
            let _ = cx.update(|window, cx| match result {
                Ok((snapshot, node)) => {
                    Self::finish_runtime(snapshot, Some(node), false, window, cx)
                }
                Err(error) => {
                    let _ = this.update(cx, |shell, cx| {
                        if let Some(setup) = &mut shell.startup {
                            setup.loading = false;
                            setup.read_error = Some(error);
                            setup.open = true;
                            setup.focus.focus(window);
                            cx.notify();
                        }
                    });
                }
            });
        })
        .detach();
    }

    fn finish_runtime(
        snapshot: Snapshot,
        node: Option<Node>,
        connect: bool,
        window: &mut Window,
        cx: &mut App,
    ) {
        let view = window.replace_root(cx, |window, cx| {
            Shell::new(snapshot, node, window.appearance(), cx)
        });
        crate::prepare_workbench(&view, window, cx);
        if connect {
            view.update(cx, |shell, cx| shell.open_connection_settings(window, cx));
        }
    }

    fn close_runtime_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(setup) = &mut self.startup else {
            return;
        };
        if setup.choosing || setup.loading || setup.status.phase == Phase::Starting {
            return;
        }
        setup.owner.cancel();
        setup.reveal_failure = false;
        setup.open = false;
        setup.path.finish_composition();
        setup.card_focus.focus(window);
        cx.notify();
    }

    fn choose_runtime_path(
        &mut self,
        directory: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(setup) = self.startup.as_mut().filter(|setup| !setup.busy()) else {
            return;
        };
        setup.choosing = true;
        setup.focus.focus(window);
        cx.notify();
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: !directory,
            directories: directory,
            multiple: false,
            prompt: Some(
                if directory {
                    "选择 rpi 安装目录"
                } else {
                    "选择 rpi 可执行文件"
                }
                .into(),
            ),
        });
        cx.spawn_in(window, async move |this, cx| {
            let selection = picker
                .await
                .ok()
                .and_then(Result::ok)
                .flatten()
                .and_then(|paths| paths.into_iter().next());
            let _ = cx.update(|window, cx| {
                let _ = this.update(cx, |shell, cx| {
                    let Some(setup) = &mut shell.startup else {
                        return;
                    };
                    setup.choosing = false;
                    if let Some(path) = selection {
                        let path = if directory {
                            path.join(if cfg!(windows) { "rpi.exe" } else { "rpi" })
                        } else {
                            path
                        };
                        setup.select_path(path);
                    }
                    shell.focus_composer(window);
                    cx.notify();
                });
            });
        })
        .detach();
    }

    pub(super) fn runtime_card(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let setup = self.startup.as_ref().expect("runtime setup");
        let palette = self.palette;
        let status = match setup.status.phase {
            Phase::Ready => "rpi 已配置",
            Phase::Failed
                if setup.status.can_install
                    && setup.status.path.is_none()
                    && !setup.downloading =>
            {
                "未找到 rpi"
            }
            Phase::Failed | Phase::Cancelled => "rpi 待配置",
            Phase::Checking if !setup.open => "正在检查 rpi…",
            _ => "正在设置 rpi…",
        };
        div()
            .flex()
            .flex_col()
            .gap_1()
            .mx_1()
            .mb_2()
            .p(px(10.0))
            .min_h(px(108.0))
            .rounded(px(9.0))
            .border_1()
            .border_color(palette.line)
            .bg(palette.bg)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(icon("terminal", 14.0, palette.muted))
                    .child(
                        self.text(status, 12.0, palette.text)
                            .font_weight(FontWeight::SEMIBOLD),
                    ),
            )
            .child(self.text(
                if setup.busy() {
                    setup.status.message.clone()
                } else {
                    "下载 rpi，或选择已有程序 / 安装目录。".into()
                },
                11.0,
                palette.muted,
            ))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_1()
                    .child(
                        self.runtime_button(
                            "runtime-open",
                            if setup.status.phase == Phase::Ready {
                                "查看配置"
                            } else {
                                "下载引导"
                            },
                            !setup.busy(),
                            false,
                        )
                        .h(px(26.0))
                        .px(px(6.0))
                        .text_size(px(11.0))
                        .track_focus(&setup.card_focus.clone().tab_stop(!setup.busy()))
                        .on_click(cx.listener(|shell, _, window, cx| {
                            shell.show_runtime_dialog(false, window, cx)
                        })),
                    )
                    .when(setup.status.phase != Phase::Ready, |row| {
                        row.child(
                            self.runtime_button(
                                "runtime-locate",
                                "选择已有 rpi",
                                !setup.busy(),
                                false,
                            )
                            .h(px(26.0))
                            .px(px(6.0))
                            .text_size(px(11.0))
                            .on_click(cx.listener(
                                |shell, _, window, cx| shell.show_runtime_dialog(true, window, cx),
                            )),
                        )
                    }),
            )
    }

    fn show_runtime_dialog(
        &mut self,
        edit_path: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(setup) = self.startup.as_mut().filter(|setup| !setup.busy()) else {
            return;
        };
        setup.open = true;
        setup.reveal_failure = false;
        if edit_path {
            self.focus_composer(window);
        } else {
            setup.focus.focus(window);
        }
        cx.notify();
    }

    fn runtime_button(
        &self,
        id: &'static str,
        label: &'static str,
        enabled: bool,
        primary: bool,
    ) -> gpui::Stateful<Div> {
        self.dialog_button(id, label, enabled, primary)
            .tab_index(0)
            .tab_stop(enabled)
    }

    pub(super) fn runtime_dialog(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let setup = self.startup.as_ref().expect("runtime dialog open");
        let palette = self.palette;
        let busy = setup.busy();
        let ready = setup.status.phase == Phase::Ready;
        let editable = !busy && !ready;
        let closable = !setup.choosing && !setup.loading && setup.status.phase != Phase::Starting;
        let downloadable = setup.can_download();
        let verifiable = setup.can_verify();
        let message = if setup.choosing {
            "正在选择 rpi 安装位置…"
        } else {
            setup.read_error.as_deref().unwrap_or(&setup.status.message)
        };
        let failed = setup.read_error.is_some() || setup.status.phase == Phase::Failed;
        let close = self
            .icon_button("runtime-close", "x", 30.0)
            .h(px(30.0))
            .tab_index(0)
            .tab_stop(closable)
            .when(!closable, |button| button.opacity(0.5))
            .on_click(cx.listener(|shell, _, window, cx| shell.close_runtime_dialog(window, cx)));
        let heading = div()
            .flex()
            .items_center()
            .justify_between()
            .child(
                self.text("配置 rpi", 15.0, palette.text)
                    .font_weight(FontWeight::SEMIBOLD),
            )
            .child(self.navigation_tooltip(close, "关闭 rpi 配置"));
        let phase = div()
            .rounded(px(8.0))
            .border_1()
            .border_color(palette.line)
            .bg(palette.subtle)
            .px_3()
            .py(px(10.0))
            .child(self.text(
                message.to_owned(),
                13.0,
                if failed { palette.danger } else { palette.text },
            ))
            .when_some(setup.status.version.as_ref(), |status, version| {
                status.child(self.text(format!("rpi {version}"), 12.0, palette.muted))
            });
        let path = div()
            .flex()
            .flex_col()
            .flex_1()
            .justify_center()
            .min_w_0()
            .min_h(px(36.0))
            .border_1()
            .border_color(palette.strong_line)
            .rounded(px(6.0))
            .px(px(5.0))
            .py(px(5.0))
            .when(editable, |field| field.child(self.input_element(cx)))
            .when(!editable, |field| {
                let value = if setup.path.text.is_empty() {
                    "选择已有程序，或下载后自动填入".into()
                } else {
                    setup.path.text.clone()
                };
                field.child(self.text(value, 13.0, palette.muted).px_1().truncate())
            });
        let choose_file = self
            .runtime_button("runtime-choose-file", "选择 rpi 程序", editable, false)
            .on_click(
                cx.listener(|shell, _, window, cx| shell.choose_runtime_path(false, window, cx)),
            );
        let options = div()
            .flex()
            .flex_col()
            .gap_1()
            .child(self.text("rpi 路径", 12.0, palette.text))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(path)
                    .child(choose_file),
            )
            .when(!ready, |options| {
                let download = self
                    .runtime_button(
                        "runtime-download",
                        if setup.downloading && failed {
                            "重试下载"
                        } else {
                            "下载并安装 rpi"
                        },
                        downloadable,
                        true,
                    )
                    .when(downloadable, |button| {
                        button.on_click(cx.listener(|shell, _, window, cx| {
                            if let Some(setup) =
                                shell.startup.as_mut().filter(|setup| setup.can_download())
                            {
                                setup.focus.focus(window);
                                setup.begin(Selection::Managed);
                                cx.notify();
                            }
                        }))
                    });
                let choose_dir = self
                    .runtime_button("runtime-choose-dir", "选择 rpi 目录", editable, false)
                    .on_click(cx.listener(|shell, _, window, cx| {
                        shell.choose_runtime_path(true, window, cx)
                    }));
                let releases = self
                    .button("runtime-releases", "官方发布页")
                    .px(px(4.0))
                    .underline()
                    .tab_index(0)
                    .tab_stop(true)
                    .focus(|button| button.bg(palette.hover))
                    .on_click(|_, _, cx| {
                        cx.open_url("https://github.com/bigfish1913/pi-rust/releases")
                    });
                options.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap_2()
                        .mt_1()
                        .child(download)
                        .child(choose_dir)
                        .child(releases),
                )
            });
        let actions = div()
            .flex()
            .flex_wrap()
            .justify_end()
            .gap_2()
            .mt_1()
            .when(closable && !ready, |row| {
                row.child(
                    self.runtime_button("runtime-remote", "连接远程节点", true, false)
                        .on_click(cx.listener(|shell, _, window, cx| {
                            if let Some(setup) = shell.startup.take() {
                                setup.owner.cancel();
                            }
                            window.defer(cx, |window, cx| {
                                Self::finish_runtime(
                                    Snapshot::disconnected(),
                                    None,
                                    true,
                                    window,
                                    cx,
                                );
                            });
                        })),
                )
            })
            .when(!ready, |row| {
                let recheck = self
                    .runtime_button("runtime-recheck", "重新检测", !busy, false)
                    .when(!busy, |button| {
                        button.on_click(cx.listener(|shell, _, window, cx| {
                            if let Some(setup) =
                                shell.startup.as_mut().filter(|setup| !setup.busy())
                            {
                                setup.focus.focus(window);
                                setup.begin(Selection::Automatic);
                                cx.notify();
                            }
                        }))
                    });
                let verify = self
                    .runtime_button("runtime-verify", "验证 rpi", verifiable, true)
                    .when(verifiable, |button| {
                        button.on_click(cx.listener(|shell, _, window, cx| {
                            shell.verify_runtime(window, cx);
                        }))
                    });
                row.child(recheck).child(verify)
            })
            .when(ready, |row| {
                row.child(
                    self.runtime_button(
                        "runtime-done",
                        if setup.loading {
                            "正在打开…"
                        } else {
                            "返回工作台"
                        },
                        !busy,
                        true,
                    )
                    .on_click(
                        cx.listener(|shell, _, window, cx| shell.load_local_runtime(window, cx)),
                    ),
                )
            });
        let dialog = div()
            .id("runtime-dialog")
            .track_focus(&setup.focus)
            .tab_group()
            .w_full()
            .max_w(px(560.0))
            .max_h_full()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_3()
            .p(px(22.0))
            .bg(palette.bg)
            .border_1()
            .border_color(palette.strong_line)
            .rounded(px(13.0))
            .shadow_lg()
            .capture_key_down(
                cx.listener(|shell, event: &gpui::KeyDownEvent, window, cx| {
                    let key = event.keystroke.key.as_str();
                    if key == "escape" && !shell.composer_is_marked() {
                        shell.close_runtime_dialog(window, cx);
                        cx.stop_propagation();
                    } else if key == "tab"
                        && let Some(setup) = &shell.startup
                    {
                        let initial = window.focused(cx);
                        loop {
                            if event.keystroke.modifiers.shift {
                                window.focus_prev();
                            } else {
                                window.focus_next();
                            }
                            if setup.focus.contains_focused(window, cx)
                                || window.focused(cx) == initial
                            {
                                break;
                            }
                        }
                        cx.stop_propagation();
                    }
                }),
            )
            .child(heading)
            .child(self.text("验证 rpi 后打开本机工作区。", 12.0, palette.muted))
            .child(phase)
            .child(options)
            .when(setup.status.phase == Phase::Downloading, |dialog| {
                let progress = setup
                    .status
                    .received
                    .saturating_mul(100)
                    .checked_div(setup.status.total)
                    .map(|percent| percent.min(100));
                let label = progress.map_or_else(
                    || {
                        format!(
                            "正在下载 · {:.1} MB",
                            setup.status.received as f64 / 1_000_000.0
                        )
                    },
                    |percent| format!("正在下载 · {percent}%"),
                );
                dialog
                    .child(self.text(label, 12.0, palette.muted))
                    .when_some(progress, |dialog, percent| {
                        dialog.child(
                            div().h(px(5.0)).rounded(px(3.0)).bg(palette.subtle).child(
                                div()
                                    .h_full()
                                    .w(relative(percent as f32 / 100.0))
                                    .rounded(px(3.0))
                                    .bg(palette.text),
                            ),
                        )
                    })
            })
            .child(actions);
        div()
            .absolute()
            .inset_0()
            .occlude()
            .bg(gpui::rgba(0x00000055))
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .px_3()
            .py_4()
            .child(dialog)
    }
}
