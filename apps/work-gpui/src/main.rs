//! GPUI desktop prototype for RPI Work.
//!
//! Everything renderer-independent comes from `work-viewmodel`; this binary only
//! provides windowing, layout and input. See `docs/design/gpui-desktop-prototype.md`.

mod data;
mod shell;
mod theme;

use gpui::{App, AppContext as _, Application, Bounds, WindowBounds, WindowOptions, px, size};
use shell::Shell;

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let snapshot = match parse(&arguments) {
        Ok(Target::Fixture) => data::Snapshot::fixture(),
        Ok(Target::Connect(origin)) => {
            let password = std::env::var("RPI_WORK_PASSWORD").ok();
            match data::Snapshot::connect(&origin, password.as_deref()) {
                Ok(snapshot) => snapshot,
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
        let bounds = Bounds::centered(None, size(px(1280.0), px(820.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |window, cx| {
                let view = cx.new(|cx| Shell::new(snapshot.clone(), cx));
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
        [flag] if flag == "--help" || flag == "-h" => Err(HELP.into()),
        _ => Err(HELP.into()),
    }
}

const HELP: &str = "work-gpui —— GPUI 桌面原型\n\
用法：work-gpui [--connect HTTP(S)_ORIGIN]\n\
默认：使用内置 fixture（与 tests/fixtures/ui-fixture.cjs 同内容）。\n\
--connect：通过 work-client 读取真实节点的项目与会话（只读；密码从 RPI_WORK_PASSWORD 读取）。";
