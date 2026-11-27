//! Native GPUI workbench for RPI Work.
//!
//! Everything renderer-independent comes from `work-viewmodel`; this binary only
//! provides windowing, layout and input.

mod data;
mod icons;
mod prompt;
mod shell;
mod theme;

use gpui::{
    App, AppContext as _, Application, Bounds, TitlebarOptions, WindowBounds, WindowOptions, point,
    px, size,
};
use shell::Shell;

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if matches!(arguments.as_slice(), [flag] if flag == "--help" || flag == "-h") {
        println!("{HELP}");
        return;
    }
    let target = match parse(&arguments) {
        Ok(target) => target,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };
    let workspace = match target {
        Target::Local => None,
        Target::Fixture => Some((data::Snapshot::fixture(), None)),
        Target::Connect(origin) => {
            let password = std::env::var("RPI_WORK_PASSWORD").ok();
            match data::Node::connect(&origin, password.as_deref()) {
                Ok((snapshot, node)) => Some((snapshot, Some(node))),
                Err(error) => {
                    eprintln!("连接 {origin} 失败：{error}");
                    std::process::exit(1);
                }
            }
        }
    };

    Application::new()
        .with_assets(icons::Icons)
        .run(move |cx: &mut App| {
            if let Some((snapshot, node)) = workspace {
                open_workbench(snapshot, node, cx).expect("open workbench window");
            } else {
                shell::startup::open(cx).expect("open startup window");
            }
            cx.activate(true);
        });
}

fn open_workbench(
    snapshot: data::Snapshot,
    node: Option<data::Node>,
    cx: &mut App,
) -> gpui::Result<gpui::WindowHandle<Shell>> {
    cx.open_window(window_options(cx), |window, cx| {
        let view = cx.new(|cx| Shell::new(snapshot, node, window.appearance(), cx));
        prepare_workbench(&view, window, cx);
        view
    })
}

fn window_options(cx: &App) -> WindowOptions {
    let bounds = Bounds::centered(None, size(px(1440.0), px(940.0)), cx);
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        window_min_size: Some(size(px(520.0), px(480.0))),
        titlebar: Some(TitlebarOptions {
            title: Some("RPI Work".into()),
            appears_transparent: cfg!(target_os = "macos"),
            traffic_light_position: Some(point(px(12.0), px(16.0))),
        }),
        ..Default::default()
    }
}

fn prepare_workbench(view: &gpui::Entity<Shell>, window: &mut gpui::Window, cx: &mut App) {
    let workbench = view.downgrade();
    window
        .observe_window_appearance(move |_, cx| {
            let _ = workbench.update(cx, |_, cx| cx.notify());
        })
        .detach();
    view.read(cx).focus_composer(window);
}

enum Target {
    Local,
    Fixture,
    Connect(String),
}

fn parse(arguments: &[String]) -> Result<Target, String> {
    match arguments {
        [] => Ok(Target::Local),
        [flag] if flag == "--sample" => Ok(Target::Fixture),
        [flag, origin] if flag == "--connect" => Ok(Target::Connect(origin.clone())),
        _ => Err(HELP.into()),
    }
}

const HELP: &str = "work-gpui —— GPUI 工作台\n\
用法：work-gpui [--sample | --connect HTTP(S)_ORIGIN]\n\
默认：检测 rpi 并打开本机工作区。\n\
--sample：打开布局样例。\n\
--connect：连接已运行的节点（密码从 RPI_WORK_PASSWORD 读取）。";
