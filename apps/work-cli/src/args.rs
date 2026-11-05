use std::env::VarError;
use url::Url;

pub(crate) const DEFAULT_URL: &str = "http://127.0.0.1:13031/";
pub(crate) const HELP: &str = r"work-cli — existing node client (never starts an agent by default)

Usage: work-cli [--url URL | --node URL] [--password-stdin] COMMAND [ARGS]
       work-cli serve [--shutdown-stdin]

No arguments, help, -h, --help: show this help without connecting or spawning.
Global target: --url and --node are aliases; overrides RPI_WORK_URL.
Default: http://127.0.0.1:13031/ (HTTP or HTTPS origin only).
Authentication: RPI_WORK_PASSWORD or --password-stdin (one UTF-8 line, <=4096
bytes including newline, then EOF; stdin overrides env). Passwords are never
accepted as argv values. Authentication/cookies exist only for this invocation;
auth does NOT save login for later commands. Prefer HTTPS for remote passwords.

HTTP commands (JSON stdout; errors stderr/nonzero):
  health                         Health probe (also checks auth)
  auth                           Auth state: required/authenticated only
  logout                         Logout this invocation's cookie at this target
  capabilities | projects | sessions
  folders [PATH]                 Browse node project folders
  state SESSION | history SESSION (latest 100) | models SESSION | tools SESSION
  watch SESSION                  JSONL events; Ctrl+C detaches, NEVER aborts
  project-create NAME [PATH]     initGit=false
  session-create [PROJECT_ID]
  session-rename SESSION TITLE
  prompt SESSION MESSAGE         HostResponse acceptance ACK, NOT settled
  abort SESSION
  dialog-reply SESSION REQUEST_ID JSON
Quote TITLE/MESSAGE/JSON as one shell argument. Use -- to end option parsing.
Authentication and each operation have a 15s deadline; watch has no deadline.
Timed-out writes have UNKNOWN outcome: inspect state; never blindly resend.

serve starts the shared headless work-host (password env handled by host).
Only explicit --shutdown-stdin enables EOF shutdown. Client options do not apply.

Compatibility only: explicitly set RPI_WORK_TRANSPORT=rpc|acp to start native rpi.
HTTP is the default; unknown transport is an error. Native commands:
  agent-info; load-session ID; resume-session ID; state; messages [LIMIT] [BEFORE_ID]
  history [LIMIT] [AFTER_SEQ] [BEFORE_SEQ]; queue [--all]; models; tools;
  thinking-levels; commands; auth-status; resume-pending; compact [INSTRUCTIONS...];
  set-tools TOOL[,TOOL]...; set-config OPTION VALUE; prompt [--allow] MESSAGE...
Native config: RPI_WORK_RPI_BIN, RPI_WORK_PROJECT. Native prompt retains legacy
streaming semantics. HTTP target/password options are rejected in native mode.
";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Transport {
    Http,
    Rpc,
    Acp,
}
impl Transport {
    pub(crate) fn parse(value: Option<&str>) -> Result<Self, String> {
        match value.unwrap_or("http") {
            "http" => Ok(Self::Http),
            "rpc" => Ok(Self::Rpc),
            "acp" => Ok(Self::Acp),
            _ => Err("RPI_WORK_TRANSPORT must be http, rpc, or acp".into()),
        }
    }
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Rpc => "rpc",
            Self::Acp => "acp",
        }
    }
}

pub(crate) enum Action {
    Help,
    Serve(bool),
    Command { name: String, args: Vec<String> },
}
pub(crate) struct Cli {
    pub(crate) transport: Transport,
    pub(crate) endpoint: String,
    pub(crate) password_stdin: bool,
    pub(crate) action: Action,
}

// Validate the raw origin as well: Url normalizes /a/.. and empty userinfo.
// Never include the supplied URL (which might contain credentials) in errors.
pub(crate) fn endpoint(value: &str) -> Result<String, String> {
    let invalid = || {
        "invalid node URL: expected http(s)://HOST[:PORT]/ without userinfo, path, query, or fragment".to_string()
    };
    if value.trim() != value || value.contains(['\\', '\r', '\n', '\t']) {
        return Err(invalid());
    }
    let url = Url::parse(value).map_err(|_| invalid())?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(invalid());
    }
    let (_, rest) = value.split_once("://").ok_or_else(invalid)?;
    let boundary = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    if rest[..boundary].contains('@') || !matches!(&rest[boundary..], "" | "/") {
        return Err(invalid());
    }
    Ok(url.to_string())
}

#[cfg(test)]
fn parse(
    arguments: Vec<String>,
    transport_env: Option<&str>,
    url_env: Option<&str>,
) -> Result<Cli, String> {
    parse_environment(
        arguments,
        transport_env.map(str::to_owned).ok_or(VarError::NotPresent),
        url_env.map(str::to_owned).ok_or(VarError::NotPresent),
    )
}

// Preserve absence versus invalid Unicode until the parser knows which settings
// this action actually uses. Never format VarError: it contains the raw input.
fn environment_value(
    value: Result<String, VarError>,
    name: &str,
) -> Result<Option<String>, String> {
    match value {
        Ok(value) => Ok(Some(value)),
        Err(VarError::NotPresent) => Ok(None),
        Err(VarError::NotUnicode(_)) => Err(format!("{name} must be UTF-8 (input omitted)")),
    }
}

pub(crate) fn parse_environment(
    arguments: Vec<String>,
    transport_env: Result<String, VarError>,
    url_env: Result<String, VarError>,
) -> Result<Cli, String> {
    let mut target = None;
    let mut password_stdin = false;
    let mut shutdown_stdin = false;
    let mut positional = Vec::new();
    let mut literal = false;
    let mut args = arguments.into_iter();
    while let Some(arg) = args.next() {
        if literal {
            positional.push(arg);
            continue;
        }
        match arg.as_str() {
            "--" => literal = true,
            "--help" | "-h" => {
                return Ok(Cli {
                    transport: Transport::Http,
                    endpoint: DEFAULT_URL.into(),
                    password_stdin: false,
                    action: Action::Help,
                });
            }
            "--url" | "--node" => {
                if target.is_some() {
                    return Err("specify --url or --node only once".into());
                }
                let value = args.next().ok_or("--url/--node requires a URL")?;
                target = Some(endpoint(&value)?);
            }
            "--password-stdin" => {
                if password_stdin {
                    return Err("duplicate --password-stdin".into());
                }
                password_stdin = true;
            }
            "--shutdown-stdin" => shutdown_stdin = true,
            // Legacy native options are validated against their command below.
            "--allow" | "--all" => positional.push(arg),
            _ if arg.starts_with('-') => {
                return Err(
                    "unknown option (see --help); passwords must not be passed on argv".into(),
                );
            }
            _ => positional.push(arg),
        }
    }
    let name = positional.first().map_or("help", String::as_str);
    if name == "help" && positional.len() <= 1 {
        return Ok(Cli {
            transport: Transport::Http,
            endpoint: DEFAULT_URL.into(),
            password_stdin: false,
            action: Action::Help,
        });
    }
    if name == "serve" {
        if positional.len() != 1 || target.is_some() || password_stdin {
            return Err("usage: work-cli serve [--shutdown-stdin]".into());
        }
        return Ok(Cli {
            transport: Transport::Http,
            endpoint: DEFAULT_URL.into(),
            password_stdin: false,
            action: Action::Serve(shutdown_stdin),
        });
    }
    if shutdown_stdin {
        return Err("--shutdown-stdin is only valid with serve".into());
    }
    let transport_env = environment_value(transport_env, "RPI_WORK_TRANSPORT")?;
    let transport = Transport::parse(transport_env.as_deref())?;
    let command_args = &positional[1..];
    if transport == Transport::Http {
        validate_http(name, command_args)?;
    } else {
        if target.is_some() || password_stdin {
            return Err("target/password options require HTTP transport".into());
        }
        validate_native(name, command_args)?;
    }
    let endpoint = if transport == Transport::Http {
        if let Some(target) = target {
            target
        } else {
            let url_env = environment_value(url_env, "RPI_WORK_URL")?;
            endpoint(url_env.as_deref().unwrap_or(DEFAULT_URL))?
        }
    } else {
        DEFAULT_URL.into()
    };
    let name = positional.remove(0);
    Ok(Cli {
        transport,
        endpoint,
        password_stdin,
        action: Action::Command {
            name,
            args: positional,
        },
    })
}

fn arity(args: &[String], min: usize, max: usize, usage: &'static str) -> Result<(), String> {
    if !(min..=max).contains(&args.len()) || args.iter().any(|a| a.trim().is_empty()) {
        return Err(format!("usage: {usage}"));
    }
    Ok(())
}
fn id(value: &str) -> Result<(), String> {
    if value == "."
        || value == ".."
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-.".contains(&b))
    {
        return Err("invalid identifier: use letters, digits, _, -, or .".into());
    }
    Ok(())
}
fn validate_http(name: &str, args: &[String]) -> Result<(), String> {
    match name {
        "health" | "capabilities" | "projects" | "sessions" | "auth" | "logout" => {
            arity(args, 0, 0, "work-cli COMMAND")
        }
        "folders" => arity(args, 0, 1, "work-cli folders [PATH]"),
        "project-create" => arity(args, 1, 2, "work-cli project-create NAME [PATH]"),
        "session-create" => {
            arity(args, 0, 1, "work-cli session-create [PROJECT_ID]")?;
            if let Some(value) = args.first() {
                id(value)?;
            }
            Ok(())
        }
        "state" | "history" | "models" | "tools" | "watch" | "abort" => {
            arity(args, 1, 1, "work-cli COMMAND SESSION")?;
            id(&args[0])
        }
        "session-rename" | "prompt" => {
            arity(
                args,
                2,
                2,
                "work-cli session-rename SESSION TITLE | prompt SESSION MESSAGE",
            )?;
            id(&args[0])
        }
        "dialog-reply" => {
            arity(args, 3, 3, "work-cli dialog-reply SESSION REQUEST_ID JSON")?;
            id(&args[0])?;
            serde_json::from_str::<serde_json::Value>(&args[2])
                .map_err(|_| "dialog-reply requires valid JSON (input omitted)".to_string())?;
            Ok(())
        }
        _ => Err("unknown HTTP command (see --help)".into()),
    }
}
fn validate_native(name: &str, args: &[String]) -> Result<(), String> {
    match name {
        "agent-info" | "state" | "models" | "thinking-levels" | "tools" | "commands"
        | "auth-status" | "resume-pending" => arity(args, 0, 0, "work-cli COMMAND"),
        "load-session" | "resume-session" => {
            arity(args, 1, 1, "work-cli load-session ID | resume-session ID")
        }
        "set-config" => arity(args, 2, 2, "work-cli set-config OPTION VALUE"),
        "messages" | "history" => {
            arity(
                args,
                0,
                if name == "messages" { 2 } else { 3 },
                "work-cli messages [LIMIT] [BEFORE_ID] | history [LIMIT] [AFTER_SEQ] [BEFORE_SEQ]",
            )?;
            for (index, value) in args.iter().enumerate() {
                if (name == "history" || index == 0) && value.parse::<u64>().is_err() {
                    return Err("LIMIT and sequence must be unsigned integers".into());
                }
            }
            Ok(())
        }
        "queue" => {
            arity(args, 0, 1, "work-cli queue [--all]")?;
            if args.first().is_some_and(|s| s != "--all") {
                return Err("usage: work-cli queue [--all]".into());
            }
            Ok(())
        }
        "compact" => arity(args, 0, usize::MAX, "work-cli compact [INSTRUCTIONS...]"),
        "set-tools" => arity(args, 1, usize::MAX, "work-cli set-tools TOOL[,TOOL]..."),
        "prompt" => {
            let text = if args.first().is_some_and(|v| v == "--allow") {
                &args[1..]
            } else {
                args
            };
            arity(text, 1, usize::MAX, "work-cli prompt [--allow] MESSAGE...")
        }
        _ => Err("unknown native command (see --help)".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn parsed(args: &[&str], transport: Option<&str>) -> Result<Cli, String> {
        parse(
            args.iter().map(ToString::to_string).collect(),
            transport,
            None,
        )
    }
    #[test]
    fn default_never_spawns() {
        assert_eq!(Transport::parse(None).unwrap(), Transport::Http);
        for transport in [None, Some("http"), Some("rpc"), Some("acp"), Some("bad")] {
            for args in [&[][..], &["--help"][..], &["help"][..]] {
                assert!(matches!(
                    parsed(args, transport).unwrap().action,
                    Action::Help
                ));
            }
        }
        assert_eq!(parsed(&["health"], None).unwrap().endpoint, DEFAULT_URL);
    }
    #[test]
    fn errors_before_connection() {
        for args in [
            &["unknown"][..],
            &["prompt", "s"][..],
            &["state"][..],
            &["health", "extra"][..],
            &["dialog-reply", "s", "r", "not-json"][..],
            &["state", "../auth"][..],
            &["--password", "secret", "auth"][..],
            &["--url"][..],
        ] {
            assert!(parsed(args, None).is_err());
        }
        for args in [
            &["unknown"][..],
            &["prompt"][..],
            &["prompt", "--allow"][..],
            &["set-config", "option"][..],
            &["history", "bad"][..],
        ] {
            assert!(parsed(args, Some("rpc")).is_err());
        }
    }
    #[test]
    fn endpoint_validation() {
        for url in [
            "http://u:secret@host/",
            "http://@host",
            "http://host/path",
            "http://host/a/..",
            "http://host?",
            "http://host#",
            "ftp://host/",
            "file:///tmp",
            "http://host\\other",
            " http://host/",
        ] {
            let error = endpoint(url).unwrap_err();
            assert!(!error.contains("secret"));
        }
        for url in [DEFAULT_URL, "https://example.org", "http://[::1]:1234/"] {
            assert!(endpoint(url).is_ok());
        }
    }
    #[test]
    fn explicit_transport_and_alias() {
        assert!(parsed(&["health"], Some("invalid")).is_err());
        for t in ["rpc", "acp"] {
            assert_eq!(
                parsed(&["agent-info"], Some(t)).unwrap().transport.name(),
                t
            );
        }
        let cli = parse(
            vec![
                "projects".into(),
                "--node".into(),
                "https://example.org".into(),
            ],
            None,
            Some("bad"),
        )
        .unwrap();
        assert_eq!(cli.endpoint, "https://example.org/");
        assert!(
            parsed(
                &["--url", DEFAULT_URL, "--node", DEFAULT_URL, "projects"],
                None
            )
            .is_err()
        );
    }
    // Construct the error directly so these tests exercise NotUnicode on every
    // platform, independent of how that platform encodes process environments.
    fn non_unicode() -> Result<String, VarError> {
        Err(VarError::NotUnicode(std::ffi::OsString::from(
            "private-env-marker",
        )))
    }
    fn environment(
        args: &[&str],
        transport: Result<String, VarError>,
        url: Result<String, VarError>,
    ) -> Result<Cli, String> {
        parse_environment(
            args.iter().map(ToString::to_string).collect(),
            transport,
            url,
        )
    }
    #[test]
    fn used_non_unicode_environment_fails_closed_and_redacts_input() {
        for args in [
            &["health"][..],
            &["--password-stdin", "project-create", "name"][..],
            &["--url", DEFAULT_URL, "auth"][..],
            &["unknown"][..],
        ] {
            let error = environment(args, non_unicode(), non_unicode())
                .err()
                .unwrap();
            assert_eq!(error, "RPI_WORK_TRANSPORT must be UTF-8 (input omitted)");
        }
        for transport in [Err(VarError::NotPresent), Ok("http".into())] {
            for args in [
                &["health"][..],
                &["--password-stdin", "project-create", "name"][..],
            ] {
                let error = environment(args, transport.clone(), non_unicode())
                    .err()
                    .unwrap();
                assert_eq!(error, "RPI_WORK_URL must be UTF-8 (input omitted)");
            }
        }
    }
    #[test]
    fn non_client_actions_ignore_non_unicode_environment() {
        for args in [&[][..], &["help"][..], &["--help"][..], &["-h"][..]] {
            assert!(matches!(
                environment(args, non_unicode(), non_unicode())
                    .unwrap()
                    .action,
                Action::Help
            ));
        }
        for (args, shutdown) in [
            (&["serve"][..], false),
            (&["serve", "--shutdown-stdin"][..], true),
        ] {
            assert!(matches!(
                environment(args, non_unicode(), non_unicode()).unwrap().action,
                Action::Serve(value) if value == shutdown
            ));
        }
    }
    #[test]
    fn explicit_target_and_native_ignore_unused_non_unicode_url() {
        for flag in ["--url", "--node"] {
            let cli = environment(
                &[flag, "https://example.org", "auth"],
                Err(VarError::NotPresent),
                non_unicode(),
            )
            .unwrap();
            assert_eq!(cli.endpoint, "https://example.org/");
        }
        for transport in ["rpc", "acp"] {
            let cli = environment(&["agent-info"], Ok(transport.into()), non_unicode()).unwrap();
            assert_eq!(cli.transport.name(), transport);
        }
    }
    #[test]
    fn literal_option_text_is_not_an_explicit_target() {
        for text in ["--url", "--node"] {
            let error = environment(
                &["prompt", "s", "--", text],
                Err(VarError::NotPresent),
                non_unicode(),
            )
            .err()
            .unwrap();
            assert_eq!(error, "RPI_WORK_URL must be UTF-8 (input omitted)");
        }
    }
    #[test]
    fn invalid_commands_still_fail_before_using_environment() {
        for args in [&["unknown"][..], &["prompt", "s"][..]] {
            let error = environment(args, Err(VarError::NotPresent), non_unicode())
                .err()
                .unwrap();
            assert!(!error.contains("private-env-marker"));
            assert!(!error.contains("RPI_WORK_URL"));
        }
    }
    #[test]
    fn serve_eof_is_opt_in() {
        assert!(matches!(
            parsed(&["serve"], None).unwrap().action,
            Action::Serve(false)
        ));
        assert!(matches!(
            parsed(&["serve", "--shutdown-stdin"], None).unwrap().action,
            Action::Serve(true)
        ));
        assert!(parsed(&["--shutdown-stdin", "health"], None).is_err());
    }
}
