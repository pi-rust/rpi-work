//! Pure display helpers. Names and arguments are never rewritten in the protocol;
//! returned strings are plain text, not trusted HTML or executable commands.

use serde_json::Value;

fn normalized_name(name: &str) -> String {
    let name = name.trim().to_ascii_lowercase();
    name.strip_prefix("functions.").unwrap_or(&name).to_owned()
}

fn shell_name(name: &str) -> bool {
    matches!(
        name,
        "bash"
            | "shell"
            | "sh"
            | "terminal"
            | "exec"
            | "exec_command"
            | "execute_command"
            | "run_command"
            | "run_shell_command"
            | "shell_command"
            | "powershell"
            | "pwsh"
            | "cmd"
            | "command"
    )
}

fn read_name(name: &str) -> bool {
    matches!(
        name,
        "read" | "read_file" | "readfile" | "cat" | "view_file"
    )
}

fn list_name(name: &str) -> bool {
    matches!(
        name,
        "list" | "ls" | "list_files" | "list_directory" | "list_dir"
    )
}

fn search_name(name: &str) -> bool {
    matches!(
        name,
        "search" | "grep" | "glob" | "find" | "rg" | "search_files" | "websearch" | "web_search"
    )
}

fn fetch_name(name: &str) -> bool {
    matches!(name, "fetch" | "webfetch" | "web_fetch" | "browse")
}

fn edit_name(name: &str) -> bool {
    matches!(name, "edit" | "edit_file" | "multiedit")
}

pub(super) fn is_read(name: &str) -> bool {
    read_name(&normalized_name(name))
}

/// A presentation hint for an actual file-read call, not proof that the model
/// followed the skill. Never infer activation from a catalog, text or directory.
pub(super) fn is_skill_read(name: &str, args: &str) -> bool {
    if !is_read(name) {
        return false;
    }
    let Ok(Value::Object(value)) = serde_json::from_str::<Value>(args) else {
        return false;
    };
    ["path", "filePath", "file_path", "file"]
        .iter()
        .find_map(|key| value.get(*key))
        .and_then(Value::as_str)
        .is_some_and(|path| path.rsplit(['/', '\\']).next() == Some("SKILL.md"))
}

pub(super) fn skill_read_action(label: &str, failed: bool) -> &'static str {
    if failed {
        "读取技能失败"
    } else {
        match label {
            "运行中" | "等待工具结果" => "正在读取技能",
            "已取消" => "已取消读取技能",
            "已完成" => "skills",
            _ => "读取技能",
        }
    }
}

pub(super) fn is_shell(name: &str) -> bool {
    shell_name(&normalized_name(name))
}

pub(super) fn icon(name: &str) -> &'static str {
    let name = normalized_name(name);
    if shell_name(&name) {
        "terminal"
    } else if read_name(&name) {
        "file"
    } else if edit_name(&name) || matches!(name.as_str(), "write" | "write_file" | "apply_patch") {
        "edit"
    } else if search_name(&name) || list_name(&name) {
        "search"
    } else if fetch_name(&name) {
        "globe"
    } else if matches!(name.as_str(), "ask" | "ask_user" | "ask_user_question") {
        "ask"
    } else if matches!(name.as_str(), "delegate_task" | "subagent") {
        "branch"
    } else if matches!(
        name.as_str(),
        "todo" | "plan" | "update_plan" | "todowrite" | "todo_write"
    ) {
        "plan"
    } else {
        "tool"
    }
}

fn collapse(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn display_value(value: &Value) -> Option<String> {
    let text = match value {
        Value::String(text) => collapse(text),
        Value::Array(values) => values
            .iter()
            .filter_map(display_value)
            .collect::<Vec<_>>()
            .join(", "),
        _ => return None,
    };
    (!text.is_empty()).then_some(text)
}

fn field(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(display_value))
}

const PATH_KEYS: &[&str] = &[
    "path",
    "filePath",
    "file_path",
    "file",
    "directory",
    "paths",
];

/// Full, whitespace-normalized summary, also suitable for a plain-text title.
/// No length limit: truncation belongs to CSS, never to the underlying text.
pub(super) fn target(name: &str, args: &str) -> String {
    let Ok(value) = serde_json::from_str::<Value>(args) else {
        return String::new();
    };
    let name = normalized_name(name);
    if shell_name(&name) {
        return field(&value, &["command", "cmd", "script"]).unwrap_or_default();
    }
    if search_name(&name) {
        let query = field(&value, &["query", "pattern", "glob", "search"]);
        let path = field(&value, PATH_KEYS);
        return [query, path]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · ");
    }
    if read_name(&name)
        || list_name(&name)
        || edit_name(&name)
        || matches!(name.as_str(), "write" | "write_file" | "apply_patch")
    {
        return field(&value, PATH_KEYS).unwrap_or_default();
    }
    if fetch_name(&name) {
        return field(&value, &["url"]).unwrap_or_default();
    }
    field(
        &value,
        &["description", "query", "path", "filePath", "url", "action"],
    )
    .unwrap_or_default()
}

fn list_item(line: &str) -> bool {
    let line = line.trim_start();
    if ["- ", "* ", "+ "].iter().any(|prefix| {
        line.strip_prefix(prefix)
            .is_some_and(|rest| !rest.trim().is_empty())
    }) {
        return true;
    }
    let digits = line.bytes().take_while(u8::is_ascii_digit).count();
    digits > 0
        && line[digits..]
            .strip_prefix(". ")
            .or_else(|| line[digits..].strip_prefix(") "))
            .is_some_and(|rest| !rest.trim().is_empty())
}

fn table_separator(line: &str) -> bool {
    let cells: Vec<_> = line.trim().trim_matches('|').split('|').collect();
    cells.len() >= 2
        && cells.iter().all(|cell| {
            let cell = cell.trim().trim_matches(':');
            cell.len() >= 3 && cell.bytes().all(|byte| byte == b'-')
        })
}

/// Intentionally conservative: ordinary source, HTML, and inline punctuation
/// alone do not select Markdown rendering. Fences must have a closing fence.
pub(super) fn is_markdown(text: &str) -> bool {
    let mut previous_list = false;
    let mut previous_pipe = false;
    let mut fence: Option<(u8, usize)> = None;
    for line in text.lines() {
        let line = line.trim();
        if let Some((marker, width)) = fence {
            if line.bytes().take_while(|byte| *byte == marker).count() >= width
                && line.bytes().all(|byte| byte == marker)
            {
                return true;
            }
            continue;
        }
        let first = line.as_bytes().first().copied();
        if let Some(marker @ (b'`' | b'~')) = first {
            let width = line.bytes().take_while(|byte| *byte == marker).count();
            if width >= 3 {
                fence = Some((marker, width));
                continue;
            }
        }
        let hashes = line.bytes().take_while(|byte| *byte == b'#').count();
        if (1..=6).contains(&hashes)
            && line[hashes..]
                .strip_prefix(' ')
                .is_some_and(|rest| !rest.trim().is_empty())
        {
            return true;
        }
        if line
            .strip_prefix("> ")
            .is_some_and(|rest| !rest.trim().is_empty())
        {
            return true;
        }
        let item = list_item(line);
        if item && previous_list {
            return true;
        }
        if previous_pipe && table_separator(line) {
            return true;
        }
        previous_list = item;
        previous_pipe = line.contains('|') && !line.trim_matches('|').trim().is_empty();
    }
    false
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct EditPreview {
    pub path: String,
    pub changes: Vec<(String, String)>,
}

impl EditPreview {
    /// Counts explicit replacement text, not a verified on-disk or Git diff.
    pub(super) fn line_counts(&self) -> (usize, usize) {
        self.changes
            .iter()
            .fold((0, 0), |(added, removed), (old, new)| {
                (added + new.lines().count(), removed + old.lines().count())
            })
    }
}

const PREVIEW_MAX_LINES: usize = 2_000;
const PREVIEW_MAX_BYTES: usize = 128 * 1024;
const OLD_KEYS: &[&str] = &[
    "oldText",
    "old_text",
    "old_string",
    "oldString",
    "old_content",
    "oldContent",
];
const NEW_KEYS: &[&str] = &[
    "newText",
    "new_text",
    "new_string",
    "newString",
    "new_content",
    "newContent",
];

fn raw_field<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_str))
}

/// Preview only explicit replacement pairs. Never executes edits or invents a
/// diff from writes/patches. Validate every pair before cloning any payload.
pub(super) fn edit_preview(name: &str, args: &str) -> Option<EditPreview> {
    if !edit_name(&normalized_name(name)) {
        return None;
    }
    let value: Value = serde_json::from_str(args).ok()?;
    let path = raw_field(&value, PATH_KEYS)?;
    if path.trim().is_empty() || path.len() > PREVIEW_MAX_BYTES {
        return None;
    }
    let entries: &[Value] = match value.get("edits") {
        Some(Value::Array(entries))
            if !entries.is_empty() && entries.len() <= PREVIEW_MAX_LINES =>
        {
            entries
        }
        Some(_) => return None,
        None => std::slice::from_ref(&value),
    };
    let mut bytes = path.len();
    let mut lines: usize = 0;
    let mut changes = Vec::new();
    for entry in entries {
        let old = raw_field(entry, OLD_KEYS)?;
        let new = raw_field(entry, NEW_KEYS)?;
        bytes = bytes.checked_add(old.len())?.checked_add(new.len())?;
        lines = lines
            .checked_add(old.lines().count())?
            .checked_add(new.lines().count())?;
        if bytes > PREVIEW_MAX_BYTES || lines > PREVIEW_MAX_LINES {
            return None;
        }
        changes.push((old, new));
    }
    Some(EditPreview {
        path: path.to_owned(),
        changes: changes
            .into_iter()
            .map(|(old, new)| (old.to_owned(), new.to_owned()))
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn edit_summary_counts_explicit_lines_like_web() {
        for (old, new, expected) in [
            ("", "", (0, 0)),
            ("before\n", "after\nextra\n", (2, 1)),
            ("before\r\n\r\n", "after\r\n", (1, 2)),
            ("", "\n", (1, 0)),
            ("removed", "", (0, 1)),
        ] {
            let preview = edit_preview(
                "edit",
                &json!({
                    "path": "src/main.rs", "oldText": old, "newText": new,
                })
                .to_string(),
            )
            .unwrap();
            assert_eq!(preview.line_counts(), expected);
        }
        let preview = edit_preview(
            "edit",
            &json!({
                "path": "src/main.rs", "edits": [
                    {"oldText": "one\ntwo", "newText": "three"},
                    {"oldText": "", "newText": "four\nfive\n"},
                ],
            })
            .to_string(),
        )
        .unwrap();
        assert_eq!(preview.line_counts(), (3, 2));
        assert!(edit_preview("write", r#"{"path":"a","content":"new"}"#).is_none());
    }

    #[test]
    fn aliases_and_groups_are_display_only() {
        for (names, expected) in [
            (
                vec!["read", "read_file", "readfile", "cat", "view_file"],
                "file",
            ),
            (
                vec![
                    "write",
                    "write_file",
                    "edit",
                    "edit_file",
                    "multiedit",
                    "apply_patch",
                ],
                "edit",
            ),
            (
                vec![
                    "search",
                    "grep",
                    "rg",
                    "glob",
                    "find",
                    "list_files",
                    "websearch",
                    "web_search",
                ],
                "search",
            ),
            (vec!["fetch", "webfetch", "web_fetch", "browse"], "globe"),
            (vec!["ask", "ask_user", "ask_user_question"], "ask"),
            (vec!["delegate_task", "subagent"], "branch"),
            (vec!["todo", "plan", "update_plan", "todo_write"], "plan"),
            (
                vec![
                    "bash",
                    "sh",
                    "shell",
                    "powershell",
                    "pwsh",
                    "cmd",
                    "exec_command",
                    "execute_command",
                    "run_shell_command",
                ],
                "terminal",
            ),
            (vec!["unknown", "", "read_secret", "my_shell"], "tool"),
        ] {
            for name in names {
                let raw = format!("  functions.{} \n", name.to_uppercase());
                assert_eq!(icon(&raw), expected, "{raw}");
                assert!(raw.starts_with("  functions."));
            }
        }
        assert!(is_read(" functions.READFILE "));
        assert!(is_shell(" functions.PowerShell "));
        assert!(!is_read("write"));
        assert!(!is_shell("read"));
    }

    #[test]
    fn targets_choose_semantic_fields() {
        for key in PATH_KEYS {
            for name in ["read", "write", "edit", "list_files"] {
                assert_eq!(
                    target(name, &json!({*key: " a/long file.rs "}).to_string()),
                    "a/long file.rs"
                );
            }
        }
        assert_eq!(target("read", r#"{"paths":["a","b"]}"#), "a, b");
        for key in ["query", "pattern", "glob", "search"] {
            assert_eq!(
                target(
                    "functions.WEBSEARCH",
                    &json!({key: "hello  world", "path":"src"}).to_string()
                ),
                "hello world · src"
            );
        }
        assert_eq!(target("grep", r#"{"path":"src"}"#), "src");
        assert_eq!(
            target("fetch", r#"{"url":"https://example.test/a"}"#),
            "https://example.test/a"
        );
        assert_eq!(
            target("custom", r#"{"description":" ","action":"run"}"#),
            "run"
        );
    }

    #[test]
    fn long_targets_and_titles_are_not_cut() {
        let long = format!("start {} THE_END", "界".repeat(10_000));
        for (name, key) in [
            ("shell", "command"),
            ("read", "path"),
            ("search", "query"),
            ("fetch", "url"),
            ("unknown", "description"),
        ] {
            assert_eq!(target(name, &json!({key: long}).to_string()), long);
        }
        assert_eq!(
            target("bash", r#"{"command":"  echo\t'a b'\n &&  echo done  "}"#),
            "echo 'a b' && echo done"
        );
    }

    #[test]
    fn skill_reads_require_an_explicit_skill_file_path() {
        for name in [
            "read",
            "read_file",
            "readfile",
            "cat",
            "view_file",
            " functions.READ ",
        ] {
            for key in ["path", "filePath", "file_path", "file"] {
                for path in [
                    "SKILL.md",
                    "/skills/demo/SKILL.md",
                    r"C:\skills\demo\SKILL.md",
                    "./skills/技能/SKILL.md",
                ] {
                    assert!(
                        is_skill_read(name, &serde_json::json!({key: path}).to_string()),
                        "{name}: {key}={path}"
                    );
                }
            }
        }
        for args in [
            "",
            "{",
            "null",
            "[]",
            r#"{"path":null}"#,
            r#"{"path":["/skills/demo/SKILL.md"]}"#,
            r#"{"directory":"/skills/demo/SKILL.md"}"#,
            r#"{"paths":["SKILL.md"]}"#,
            r#"{"description":"read SKILL.md"}"#,
            r#"{"path":"/SKILL.md.bak"}"#,
            r#"{"path":"/SKILL.md/"}"#,
            r#"{"path":"/SKILL.md/notes.txt"}"#,
            r#"{"path":"/skills/demo/skill.md"}"#,
            r#"{"path":"/skills/demo/README.md"}"#,
            r#"{"path":"other.txt","filePath":"SKILL.md"}"#,
            r#"{"path":"","filePath":"SKILL.md"}"#,
            r#"{"path":null,"filePath":"SKILL.md"}"#,
            r#"{"path":["SKILL.md"],"filePath":"SKILL.md"}"#,
            r#"{"path":"SKILL.md "}"#,
            r#"{"path":" SKILL.md"}"#,
            r#"{"path":"SKILL.md\n"}"#,
        ] {
            assert!(!is_skill_read("read", args), "{args}");
        }
        for name in ["bash", "write", "skills", "skill", "unknown"] {
            assert!(!is_skill_read(name, r#"{"path":"/skills/demo/SKILL.md"}"#));
        }
    }

    #[test]
    fn skill_read_status_does_not_fabricate_success() {
        for label in ["运行中", "等待工具结果"] {
            assert_eq!(skill_read_action(label, false), "正在读取技能");
        }
        assert_eq!(skill_read_action("已完成", false), "skills");
        assert_eq!(skill_read_action("已取消", false), "已取消读取技能");
        assert_eq!(skill_read_action("未知状态", false), "读取技能");
        for label in ["运行中", "已完成", "执行失败", "已取消"] {
            assert_eq!(skill_read_action(label, true), "读取技能失败");
        }
    }

    #[test]
    fn empty_or_invalid_arguments_have_no_duplicate_name() {
        for args in [
            "",
            "  ",
            "{",
            "null",
            "[]",
            "{}",
            "0",
            r#"{"path":" \n\t "}"#,
            r#"{"path":false}"#,
        ] {
            assert_eq!(target("read", args), "", "{args}");
        }
        assert_eq!(target("unknown", "{}"), "");
        assert_eq!(
            target("read", r#"{"path":" ","filePath":" valid "}"#),
            "valid"
        );
    }

    #[test]
    fn xss_is_only_plain_text_and_raw_edits_are_preserved() {
        let payload = "<img src=x onerror=alert(1)>";
        assert_eq!(
            target("custom", &json!({"description":payload}).to_string()),
            payload
        );
        assert!(!is_markdown(payload));
        let args = json!({"path":" x.rs ", "oldText":"  old\r\n\t", "newText":payload}).to_string();
        let preview = edit_preview("functions.EDIT", &args).unwrap();
        assert_eq!(preview.path, " x.rs ");
        assert_eq!(
            preview.changes,
            vec![("  old\r\n\t".into(), payload.into())]
        );
    }

    #[test]
    fn markdown_requires_explicit_blocks() {
        for text in [
            "# Heading",
            "```rust\nfn main() {}\n```",
            "~~~\nhello\n~~~",
            "- one\n- two",
            "1. one\n2. two",
            "> quote",
            "| a | b |\n| :--- | ---: |\n| c | d |",
        ] {
            assert!(is_markdown(text), "{text}");
        }
        for text in [
            "",
            "plain *text*",
            "a_b.rs",
            "#include <stdio.h>",
            "#[derive(Debug)]\nstruct Foo;",
            "fn main() {\n    println!(\"hi\");\n}",
            "<html><body>Hello</body></html>",
            "const x = a | b;",
            "```unclosed",
            "- alone",
            "a | b\n-- | --",
            "<script>alert(1)</script>",
        ] {
            assert!(!is_markdown(text), "{text}");
        }
    }

    #[test]
    fn previews_accept_only_explicit_edit_shapes() {
        for (old, new) in OLD_KEYS.iter().zip(NEW_KEYS) {
            let args = json!({"filePath":"a", *old:"before", *new:""}).to_string();
            assert_eq!(
                edit_preview("edit_file", &args).unwrap().changes,
                vec![("before".into(), String::new())]
            );
        }
        let args = r#"{"path":"a","edits":[{"oldText":"x","newText":"y"},{"old_string":"","new_string":"z"}]}"#;
        assert_eq!(edit_preview("multiedit", args).unwrap().changes.len(), 2);
        for name in ["write", "write_file", "apply_patch", "unknown", "read"] {
            assert!(edit_preview(name, args).is_none());
        }
        for args in [
            "",
            "{",
            "null",
            "{}",
            r#"{"path":"a","patch":"@@"}"#,
            r#"{"path":"a","oldText":"x"}"#,
            r#"{"path":"a","oldText":1,"newText":"y"}"#,
            r#"{"path":"a","edits":[]}"#,
            r#"{"path":"a","edits":[{"oldText":"a","newText":"b"},{}]}"#,
        ] {
            assert!(edit_preview("edit", args).is_none(), "{args}");
        }
    }

    #[test]
    fn preview_limits_are_aggregate_and_do_not_truncate() {
        let exact = "x\n".repeat(1_000);
        let args = json!({"path":"a", "oldText":exact, "newText":exact}).to_string();
        assert_eq!(edit_preview("edit", &args).unwrap().changes[0].0, exact);
        let args = json!({"path":"a", "oldText":"x\n".repeat(1_001), "newText":exact}).to_string();
        assert!(edit_preview("edit", &args).is_none());
        let args = json!({"path":"a", "edits":[{"oldText":"x\n".repeat(1_000),"newText":""},{"oldText":"x\n".repeat(1_001),"newText":""}]}).to_string();
        assert!(edit_preview("edit", &args).is_none());
        let exact = "x".repeat(PREVIEW_MAX_BYTES - 1);
        let args = json!({"path":"a", "oldText":exact, "newText":""}).to_string();
        assert_eq!(
            edit_preview("edit", &args).unwrap().changes[0].0.len(),
            PREVIEW_MAX_BYTES - 1
        );
        let args = json!({"path":"a", "oldText":exact, "newText":"x"}).to_string();
        assert!(edit_preview("edit", &args).is_none());
        let half = "x".repeat(PREVIEW_MAX_BYTES / 2);
        let args = json!({"path":"a", "edits":[{"oldText":half,"newText":""},{"oldText":"","newText":half}]}).to_string();
        assert!(edit_preview("edit", &args).is_none());
        let args =
            json!({"path":"a", "oldText":"界".repeat(PREVIEW_MAX_BYTES / 3 + 1), "newText":""})
                .to_string();
        assert!(edit_preview("edit", &args).is_none());
        let args = json!({"path":"a", "edits":vec![json!({"oldText":"", "newText":""}); PREVIEW_MAX_LINES + 1]}).to_string();
        assert!(edit_preview("edit", &args).is_none());
    }
}
