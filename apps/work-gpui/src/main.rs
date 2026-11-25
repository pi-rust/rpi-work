//! Native GPUI workbench for RPI Work.
//!
//! Everything renderer-independent comes from `work-viewmodel`; this binary only
//! provides windowing, layout and input.

mod data;
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
    let (snapshot, node) = match parse(&arguments) {
        Ok(Target::Fixture) => (data::Snapshot::fixture(), None),
        Ok(Target::Connect(origin)) => {
            let password = std::env::var("RPI_WORK_PASSWORD").ok();
            match data::Node::connect(&origin, password.as_deref()) {
                Ok((snapshot, node)) => (snapshot, Some(node)),
                Err(error) => {
                    eprintln!("连接 {origin} 失败：{error}");
                    std::process::exit(1);
                }
            }
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };

    Application::new().run(move |cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1440.0), px(940.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(720.0), px(480.0))),
                titlebar: Some(TitlebarOptions {
                    title: Some("RPI Work".into()),
                    appears_transparent: cfg!(target_os = "macos"),
                    traffic_light_position: Some(point(px(12.0), px(17.0))),
                }),
                ..Default::default()
            },
            |window, cx| {
                let view = cx.new(|cx| Shell::new(snapshot, node, cx));
                view.read(cx).focus_composer(window);
                view
            },
        )
        .expect("open window");
        cx.activate(true);
    });
}

enum Target {
    Fixture,
    Connect(String),
}

fn parse(arguments: &[String]) -> Result<Target, String> {
    match arguments {
        [] => Ok(Target::Fixture),
        [flag, origin] if flag == "--connect" => Ok(Target::Connect(origin.clone())),
        _ => Err(HELP.into()),
    }
}

const HELP: &str = "work-gpui —— GPUI 工作台\n\
用法：work-gpui [--connect HTTP(S)_ORIGIN]\n\
默认：打开标注「示例 · 未连接」的布局样例。\n\
--connect：通过 work-client 读取真实节点的项目、会话与消息（密码从 RPI_WORK_PASSWORD 读取）。";
