//! Workbench controls for rpi's own local Host modes.

use std::path::PathBuf;

#[cfg(not(target_os = "macos"))]
use gpui::ClipboardItem;
use gpui::{Context, Div, FontWeight, PathPromptOptions, div, prelude::*};

use super::Shell;
use control::{Area, Catalog, Launch, Status, Target};

#[path = "rpi_hosts/control.rs"]
mod control;

#[derive(Default)]
pub(super) struct Panel {
    catalog: Option<Catalog>,
    extra_config: Option<PathBuf>,
    refreshing: bool,
    choosing: bool,
    opening: bool,
    notice: Option<(String, bool)>,
}

impl Shell {
    pub(super) fn read_rpi_hosts(&mut self, cx: &mut Context<Self>) {
        let panel = &mut self.rpi_hosts;
        if !cfg!(unix) || panel.refreshing {
            return;
        }
        let config = panel.extra_config.clone();
        panel.refreshing = true;
        panel.notice = None;
        let job = cx
            .background_executor()
            .spawn(async move { control::read_catalog(config) });
        cx.spawn(async move |this, cx| {
            let result = job.await;
            let _ = this.update(cx, |shell, cx| {
                let panel = &mut shell.rpi_hosts;
                panel.refreshing = false;
                match result {
                    Ok(catalog) => panel.catalog = Some(catalog),
                    Err(error) => {
                        panel.catalog = None;
                        panel.notice = Some((error, true));
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn choose_host_config(&mut self, cx: &mut Context<Self>) {
        if self.rpi_hosts.choosing || self.rpi_hosts.refreshing {
            return;
        }
        self.rpi_hosts.choosing = true;
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("选择 rpi Host 的私有配置文件".into()),
        });
        cx.spawn(async move |this, cx| {
            let path = picker
                .await
                .ok()
                .and_then(Result::ok)
                .flatten()
                .and_then(|paths| paths.into_iter().next());
            let _ = this.update(cx, |shell, cx| {
                shell.rpi_hosts.choosing = false;
                if let Some(path) = path.filter(|path| path.is_absolute()) {
                    shell.rpi_hosts.extra_config = Some(path);
                    shell.read_rpi_hosts(cx);
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn enter_rpi_host(
        &mut self,
        area: String,
        native: bool,
        target: Option<Target>,
        workspace: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        let panel = &mut self.rpi_hosts;
        if panel.refreshing || panel.choosing || panel.opening {
            return;
        }
        let Some(catalog) = &panel.catalog else {
            return;
        };
        let workspace = target
            .as_ref()
            .map(|target| target.workspace.clone())
            .or(workspace);
        let launch = Launch {
            executable: catalog.executable.clone(),
            root: catalog.root.clone(),
            area,
            agent: target.map(|target| target.alias),
            native,
        };
        let local_project = self
            .node
            .as_ref()
            .is_some_and(crate::data::Node::is_local)
            .then(|| {
                self.project
                    .and_then(|index| self.snapshot.projects.get(index))
            })
            .flatten()
            .map(|project| PathBuf::from(&project.path));
        if let Some(workspace) = workspace.or(local_project) {
            self.launch_rpi_terminal(launch, workspace, cx);
            return;
        }
        self.rpi_hosts.choosing = true;
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("选择 Host 的本机工作目录".into()),
        });
        cx.spawn(async move |this, cx| {
            let path = picker
                .await
                .ok()
                .and_then(Result::ok)
                .flatten()
                .and_then(|paths| paths.into_iter().next());
            let _ = this.update(cx, |shell, cx| {
                shell.rpi_hosts.choosing = false;
                if let Some(path) = path {
                    shell.launch_rpi_terminal(launch, path, cx);
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn launch_rpi_terminal(&mut self, launch: Launch, workspace: PathBuf, cx: &mut Context<Self>) {
        let Some(command) = workspace
            .is_absolute()
            .then(|| launch.command(&workspace))
            .flatten()
        else {
            self.rpi_hosts.notice = Some(("无法使用此工作目录或 rpi 路径进入 Host。".into(), true));
            cx.notify();
            return;
        };
        #[cfg(target_os = "macos")]
        {
            self.rpi_hosts.opening = true;
            self.rpi_hosts.notice = None;
            let job = cx
                .background_executor()
                .spawn(async move { control::open_terminal(&command) });
            cx.spawn(async move |this, cx| {
                let opened = job.await.is_ok();
                let _ = this.update(cx, |shell, cx| {
                    shell.rpi_hosts.opening = false;
                    shell.rpi_hosts.notice = Some((
                        if opened {
                            "已打开终端。可刷新查看 Host 连接状态。"
                        } else {
                            "无法打开 macOS 终端。"
                        }
                        .into(),
                        !opened,
                    ));
                    cx.notify();
                });
            })
            .detach();
        }
        #[cfg(not(target_os = "macos"))]
        {
            cx.write_to_clipboard(ClipboardItem::new_string(command));
            self.rpi_hosts.notice = Some(("已复制命令。请在本机终端运行。".into(), false));
        }
        cx.notify();
    }

    pub(super) fn rpi_host_settings(&self, cx: &Context<Self>) -> Div {
        let panel = &self.rpi_hosts;
        let palette = self.palette;
        let mut content = div().flex().flex_col().gap_3();
        if !cfg!(unix) {
            return content.child(self.text(
                "rpi Host 模式目前需要 macOS 或 Linux。",
                13.0,
                palette.muted,
            ));
        }
        let enabled = !panel.refreshing && !panel.choosing && !panel.opening;
        let catalog = panel.catalog.as_ref();
        content = content
            .child(self.text(
                "管理此电脑上的 rpi Host area 和 Agent。",
                13.0,
                palette.muted,
            ))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .when(enabled, |row| {
                        row.child(
                            self.setting_choice("rpi-host-refresh", "刷新状态", false)
                                .on_click(cx.listener(|shell, _, _, cx| shell.read_rpi_hosts(cx))),
                        )
                    })
                    .when(panel.refreshing, |row| {
                        row.child(self.text("正在读取 Host…", 13.0, palette.muted))
                    })
                    .when(
                        enabled && catalog.is_some_and(|catalog| catalog.control),
                        |row| {
                            row.child(
                                self.setting_choice("rpi-host-config", "选择 Host 配置", false)
                                    .on_click(
                                        cx.listener(|shell, _, _, cx| shell.choose_host_config(cx)),
                                    ),
                            )
                        },
                    ),
            )
            .when_some(panel.notice.as_ref(), |content, (notice, error)| {
                content.child(self.text(
                    notice.clone(),
                    13.0,
                    if *error { palette.danger } else { palette.text },
                ))
            });
        let Some(catalog) = catalog else {
            return content;
        };
        if catalog.shared_areas.is_empty() && !catalog.native && !catalog.control {
            return content.child(self.text("当前 rpi 不支持 Host 模式。", 13.0, palette.muted));
        }
        content = content.when_some(catalog.notice, |content, notice| {
            content.child(self.text(notice, 13.0, palette.danger))
        });
        if catalog.native {
            content = content.children(
                catalog
                    .areas
                    .iter()
                    .enumerate()
                    .map(|(index, area)| self.host_area_row(index, area, true, enabled, cx)),
            );
        }
        if let Some(area) = &catalog.extra {
            content =
                content.child(self.host_area_row(catalog.areas.len(), area, false, enabled, cx));
        }
        content.children(
            catalog
                .shared_areas
                .iter()
                .enumerate()
                .map(|(index, area)| {
                    let area = area.clone();
                    div()
                        .border_t_1()
                        .border_color(palette.line)
                        .pt_3()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(self.text(format!("共享 Host · {area}"), 14.0, palette.text))
                        .when(enabled, |row| {
                            row.child(
                                div().child(
                                    self.setting_choice(
                                        ("rpi-host-shared", index),
                                        if cfg!(target_os = "macos") {
                                            "进入共享 Host"
                                        } else {
                                            "复制进入命令"
                                        },
                                        false,
                                    )
                                    .on_click(cx.listener(
                                        move |shell, _, _, cx| {
                                            shell.enter_rpi_host(
                                                area.clone(),
                                                false,
                                                None,
                                                None,
                                                cx,
                                            );
                                        },
                                    )),
                                ),
                            )
                        })
                }),
        )
    }

    fn host_area_row(
        &self,
        index: usize,
        area: &Area,
        native: bool,
        enabled: bool,
        cx: &Context<Self>,
    ) -> Div {
        let palette = self.palette;
        let name = &area.name;
        let status = match area.status {
            Status::Uninitialized => "尚未初始化".to_owned(),
            Status::Disconnected => "未连接 · 找不到 Host 端点".to_owned(),
            Status::Unavailable => "无法查询 · 请检查 Host 配置或服务".to_owned(),
            Status::Online {
                registered,
                resident,
                busy,
                stopping,
            } => format!(
                "{} · {registered} 已登记 · {resident} 驻留 · {busy} 忙碌",
                if stopping {
                    "正在停止"
                } else {
                    "已连接"
                }
            ),
        };
        div()
            .border_t_1()
            .border_color(palette.line)
            .pt_3()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                self.text(
                    if native {
                        format!("持久化 Host · {name}")
                    } else {
                        name.clone()
                    },
                    14.0,
                    palette.text,
                )
                .font_weight(FontWeight::MEDIUM),
            )
            .child(self.text(status, 13.0, palette.muted))
            .when(!native, |row| {
                row.when_some(self.rpi_hosts.extra_config.as_ref(), |row, path| {
                    row.child(self.text(path.display().to_string(), 12.0, palette.muted))
                })
            })
            .when(native && enabled && area.can_enter, |row| {
                let name = name.clone();
                let workspace = area.default_workspace.clone();
                row.child(
                    div().child(
                        self.setting_choice(("rpi-host-enter", index), "进入 Host", false)
                            .on_click(cx.listener(move |shell, _, _, cx| {
                                shell.enter_rpi_host(
                                    name.clone(),
                                    true,
                                    None,
                                    workspace.clone(),
                                    cx,
                                );
                            })),
                    ),
                )
            })
            .children(area.agents.iter().enumerate().map(|(agent_index, agent)| {
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child(self.text(agent.name.clone(), 13.0, palette.text))
                    .child(self.text(agent.state, 12.0, palette.muted))
                    .when_some(agent.target.as_ref().filter(|_| enabled), |row, target| {
                        let name = name.clone();
                        let target = target.clone();
                        row.child(
                            self.setting_choice(
                                ("rpi-host-agent", index * 1024 + agent_index),
                                "连接 Agent",
                                false,
                            )
                            .on_click(cx.listener(
                                move |shell, _, _, cx| {
                                    shell.enter_rpi_host(
                                        name.clone(),
                                        true,
                                        Some(target.clone()),
                                        None,
                                        cx,
                                    );
                                },
                            )),
                        )
                    })
            }))
            .when(area.incomplete, |row| {
                row.child(self.text("Agent 列表未读取完整，可刷新后重试。", 12.0, palette.muted))
            })
    }
}
