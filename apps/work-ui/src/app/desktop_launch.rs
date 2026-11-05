//! Parse desktop host capabilities before opening a window or acquiring a lock.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Mode {
    Gui(Option<String>),
    Headless { shutdown_stdin: bool },
    Help,
}

pub(super) const HELP: &str = "RPI Work\n\
Usage: work-ui [--connect HTTP(S)_ORIGIN]\n\
       work-ui --headless [--shutdown-stdin]\n\
Default: own the local workspace and open its GUI.\n\
--connect (or remote RPI_WORK_NODE_URL): client only; never starts a local owner.\n\
--headless: same owner without a window. Requires an explicit HTTP password env.\n\
--shutdown-stdin: only in headless mode; stop cooperatively on stdin EOF.\n\
For servers without GUI libraries use work-cli serve (no WebView dependency).";

pub(super) fn parse(args: &[String], configured: Option<&str>) -> Result<Mode, String> {
    match args {
        [help] if matches!(help.as_str(), "--help" | "-h") => Ok(Mode::Help),
        [headless] if headless == "--headless" => Ok(Mode::Headless {
            shutdown_stdin: false,
        }),
        [headless, stdin] if headless == "--headless" && stdin == "--shutdown-stdin" => {
            Ok(Mode::Headless {
                shutdown_stdin: true,
            })
        }
        [connect, address] if connect == "--connect" => {
            remote(address).map(|address| Mode::Gui(Some(address)))
        }
        [] => match configured {
            Some(address) if super::local_node::is_local(address) => {
                Ok(Mode::Gui(Some(super::local_node::LOCAL_ADDRESS.into())))
            }
            Some(address) => remote(address).map(|address| Mode::Gui(Some(address))),
            None => Ok(Mode::Gui(None)),
        },
        _ => Err("启动参数无效，请使用 --help；未启动本机节点。".into()),
    }
}

fn remote(address: &str) -> Result<String, String> {
    super::connections::normalize_address(address, false)
        .map(|address| format!("{address}/"))
        .map_err(|_| {
            "远程节点地址无效：仅接受不含凭据、路径、查询和片段的 HTTP(S) origin；未启动本机节点。"
                .into()
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).into()).collect()
    }
    #[test]
    fn gui_owner_is_default_and_remote_is_explicit() {
        assert_eq!(parse(&[], None).unwrap(), Mode::Gui(None));
        assert_eq!(
            parse(&[], Some("rpi-work://local")).unwrap(),
            Mode::Gui(Some(super::super::local_node::LOCAL_ADDRESS.into()))
        );
        assert_eq!(
            parse(&[], Some("http://127.0.0.1:45678")).unwrap(),
            Mode::Gui(Some("http://127.0.0.1:45678/".into()))
        );
        assert_eq!(
            parse(
                &args(&["--connect", "https://node.example"]),
                Some("invalid")
            )
            .unwrap(),
            Mode::Gui(Some("https://node.example/".into()))
        );
    }
    #[test]
    fn headless_and_pipe_supervision_are_separate_explicit_capabilities() {
        assert_eq!(
            parse(&args(&["--headless"]), None).unwrap(),
            Mode::Headless {
                shutdown_stdin: false
            }
        );
        assert_eq!(
            parse(&args(&["--headless", "--shutdown-stdin"]), None).unwrap(),
            Mode::Headless {
                shutdown_stdin: true
            }
        );
        assert_eq!(parse(&args(&["--help"]), None).unwrap(), Mode::Help);
        for values in [
            &["--shutdown-stdin"][..],
            &["--connect"][..],
            &["--headless", "--connect", "http://localhost"][..],
        ] {
            assert!(parse(&args(values), None).is_err());
        }
    }
    #[test]
    fn invalid_explicit_target_never_falls_back_to_local_or_echoes_credentials() {
        for target in [
            "",
            "about:blank",
            "https://u:secret@node.example/",
            "https://node.example/a",
            "https://node.example/#rpi-entry=x",
        ] {
            let error = parse(&args(&["--connect", target]), None).unwrap_err();
            assert!(!error.contains("secret"));
            assert!(parse(&[], Some(target)).is_err());
        }
    }
}
