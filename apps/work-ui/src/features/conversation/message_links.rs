//! Message links are navigation only: project-relative reads via the existing file API.
//! Never turn model output into shell commands, editor invocations or file:// URLs.
use dioxus::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FileLink {
    pub path: String,
    pub lines: Option<(usize, usize)>,
}

#[derive(Clone, Copy)]
pub(crate) struct LinkContext {
    pub root: Signal<Option<String>>,
    pub scope: Memo<String>,
    pub onopen: EventHandler<(String, FileLink)>,
}

fn decode(value: &str) -> Option<String> {
    let mut bytes = Vec::new();
    let mut input = value.bytes();
    while let Some(byte) = input.next() {
        bytes.push(if byte == b'%' {
            let hi = (input.next()? as char).to_digit(16)?;
            let lo = (input.next()? as char).to_digit(16)?;
            (hi * 16 + lo) as u8
        } else {
            byte
        });
    }
    String::from_utf8(bytes).ok()
}

fn drive(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && &bytes[1..3] == b":/"
}

fn location(value: &str) -> Option<(&str, Option<(usize, usize)>)> {
    let parse = |s: &str| s.parse::<usize>().ok().filter(|n| *n > 0);
    if let Some((path, lines)) = value.rsplit_once("#L") {
        let (start, end) = lines.split_once("-L").unwrap_or((lines, lines));
        let (start, end) = (parse(start)?, parse(end)?);
        return (start <= end).then_some((path, Some((start, end))));
    }
    if let Some((path, line)) = value.rsplit_once(':') {
        if let Some(line) = parse(line) {
            return Some((path, Some((line, line))));
        }
    }
    Some((value, None))
}

/// Lexical containment is intentionally platform-independent (WASM runs on Unix
/// and Windows projects). The server remains responsible for symlink/sensitive-file checks.
pub(crate) fn resolve_file(root: &str, target: &str) -> Option<FileLink> {
    let decoded = decode(target)?;
    if decoded.is_empty()
        || decoded
            .chars()
            .any(|c| c.is_control() || matches!(c, '?' | '<' | '>' | '"' | '|' | '%'))
    {
        return None;
    }
    let (path, lines) = location(&decoded)?;
    if path.contains('#') {
        return None;
    }
    let path = path.replace('\\', "/");
    let root = root.replace('\\', "/");
    let root = if root == "/" || (drive(&root) && root.len() == 3) {
        root.as_str()
    } else {
        root.trim_end_matches('/')
    };
    if path.starts_with("//")
        || root.starts_with("//")
        || root.is_empty()
        || !(root.starts_with('/') || drive(root))
    {
        return None;
    }
    if path.split('/').any(|part| part == "..") || root.split('/').any(|part| part == "..") {
        return None;
    }
    let relative = if path.starts_with('/') || drive(&path) {
        let prefix = if root.ends_with('/') {
            root.to_owned()
        } else {
            format!("{root}/")
        };
        if drive(&path) && drive(root) {
            if !path
                .get(..prefix.len())
                .is_some_and(|p| p.eq_ignore_ascii_case(&prefix))
            {
                return None;
            }
        } else if !path.starts_with(&prefix) {
            return None;
        }
        path.get(prefix.len()..)?
    } else {
        &path
    };
    if relative.contains(':') {
        return None;
    }
    let path = relative
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect::<Vec<_>>()
        .join("/");
    if path.is_empty() {
        return None;
    }
    Some(FileLink { path, lines })
}

pub(crate) fn looks_like_file(target: &str) -> bool {
    let Some(decoded) = decode(target) else {
        return false;
    };
    let Some((path, _)) = location(&decoded) else {
        return false;
    };
    path.contains('/')
        || path.contains('\\')
        || path.rsplit('.').next().is_some_and(|ext| {
            ext.len() <= 12
                && !ext.is_empty()
                && ext != path
                && ext.chars().all(|c| c.is_alphanumeric())
        })
        || matches!(path, "README" | "LICENSE" | "Makefile" | "Dockerfile")
}

pub(crate) fn safe_web_url(url: &str) -> bool {
    if url
        .chars()
        .any(|c| c.is_whitespace() || c.is_control() || matches!(c, '<' | '>' | '"' | '\'' | '\\'))
    {
        return false;
    }
    let lower = url.to_ascii_lowercase();
    let Some(rest) = lower
        .strip_prefix("https://")
        .or_else(|| lower.strip_prefix("http://"))
    else {
        return false;
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    !host.is_empty() && host != "." && !host.contains('@')
}

/// Consume only a URL at a token boundary. Preserve balanced parentheses and
/// leave prose punctuation outside the anchor (including Chinese punctuation).
pub(crate) fn bare_url(text: &str) -> Option<&str> {
    if !(text
        .get(..8)
        .is_some_and(|s| s.eq_ignore_ascii_case("https://"))
        || text
            .get(..7)
            .is_some_and(|s| s.eq_ignore_ascii_case("http://")))
    {
        return None;
    }
    let candidate = text
        .split(|c: char| {
            c.is_whitespace()
                || matches!(
                    c,
                    '<' | '>'
                        | '"'
                        | '\''
                        | '`'
                        | '，'
                        | '。'
                        | '！'
                        | '？'
                        | '；'
                        | '、'
                        | '）'
                        | '】'
                )
        })
        .next()?;
    let mut url = candidate.trim_end_matches(['.', ',', '!', '?', ';', ':', ']']);
    while url.ends_with(')') && url.matches(')').count() > url.matches('(').count() {
        url = &url[..url.len() - 1];
    }
    safe_web_url(url).then_some(url)
}

#[component]
pub(crate) fn MessageFileLink(label: String, target: String, code: bool) -> Element {
    let context = try_use_context::<LinkContext>();
    let resolved = context.and_then(|ctx| {
        ctx.root
            .read()
            .as_deref()
            .and_then(|root| resolve_file(root, &target))
            .map(|file| (ctx, ctx.scope.read().clone(), file))
    });
    if let Some((context, scope, file)) = resolved {
        rsx! { button { class: "message-file-link", r#type: "button", title: "在项目内只读预览", "data-file-path": "{file.path}", onclick: move |_| context.onopen.call((scope.clone(), file.clone())),
            if code { code { "{label}" } } else { "{label}" }
        } }
    } else if code {
        rsx! { code { "{label}" } }
    } else {
        rsx! { "{label}" }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn project_paths_and_locations() {
        for (root, input, expected) in [
            ("/work/demo", "./src/main.rs:12", "src/main.rs"),
            (
                "/work/demo",
                "/work/demo/src/main.rs#L12-L20",
                "src/main.rs",
            ),
            ("D:\\Work\\Demo", "d:/work/DEMO/src/main.rs", "src/main.rs"),
            (
                "/work/demo",
                "docs/%E4%B8%AD%E6%96%87%20name.md",
                "docs/中文 name.md",
            ),
        ] {
            assert_eq!(resolve_file(root, input).unwrap().path, expected);
        }
        assert_eq!(
            resolve_file("/w", "x.rs#L12-L20").unwrap().lines,
            Some((12, 20))
        );
        assert_eq!(resolve_file("/w", "x.rs:12").unwrap().lines, Some((12, 12)));
        assert_eq!(
            resolve_file("/", "/src/main.rs").unwrap().path,
            "src/main.rs"
        );
        assert_eq!(
            resolve_file("D:/", "d:/src/main.rs").unwrap().path,
            "src/main.rs"
        );
        assert_eq!(resolve_file("/w", "config").unwrap().path, "config");
    }
    #[test]
    fn rejects_unsafe_or_outside_paths() {
        for input in [
            "../secret",
            "src/../secret",
            "%2e%2e/secret",
            "%252e%252e/secret",
            "/work/demo-other/x",
            "/Work/demo/x",
            "C:/other/x",
            "C:relative",
            "//host/share/x",
            "\\\\host\\share\\x",
            "file:///work/demo/x",
            "javascript:alert(1)",
            "data:text/plain,x",
            "https://example.org/x",
            "x.rs#L0",
            "x.rs#L20-L12",
            "x.rs:0",
            "x%00.rs",
            "x%0a.rs",
            "x%ZZ",
            "#anchor",
            "",
        ] {
            assert!(resolve_file("/work/demo", input).is_none(), "{input}");
        }
    }
    #[test]
    fn web_urls_and_punctuation() {
        assert_eq!(
            bare_url("https://example.org/a_(b). 后续"),
            Some("https://example.org/a_(b)")
        );
        assert_eq!(
            bare_url("https://example.org/a)，后续"),
            Some("https://example.org/a")
        );
        for url in [
            "javascript:x",
            "https://user@host/a",
            "https://",
            "https://x/\\bad",
            "https://x/\n",
        ] {
            assert!(!safe_web_url(url));
        }
        assert!(looks_like_file("src/main.rs:12"));
        assert!(!looks_like_file("not a path"));
    }
}
