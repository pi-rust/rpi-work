use crate::shared::components::Icon;
use dioxus::prelude::*;
pub(crate) fn change_status_label(status: &str) -> &'static str {
    match status.to_ascii_uppercase().as_str() {
        "A" | "ADDED" | "ADD" => "A",
        "D" | "DELETED" | "DELETE" => "D",
        "R" | "RENAMED" | "RENAME" => "R",
        _ => "M",
    }
}

pub(crate) fn filter_changed_files<'a>(
    files: &'a [work_client::ChangedFile],
    query: &str,
    status: &str,
    directory: &str,
    unreviewed: bool,
    reviewed: &std::collections::HashSet<String>,
) -> Vec<&'a work_client::ChangedFile> {
    let query = query.trim().to_ascii_lowercase();
    files
        .iter()
        .filter(|file| {
            let matches_query = query.is_empty()
                || file.path.to_ascii_lowercase().contains(&query)
                || file
                    .original_path
                    .as_deref()
                    .is_some_and(|old| old.to_ascii_lowercase().contains(&query));
            let matches_status = status == "all" || change_status_label(&file.status) == status;
            let matches_directory =
                directory.trim().is_empty() || file.path.starts_with(directory.trim_matches('/'));
            matches_query
                && matches_status
                && matches_directory
                && (!unreviewed || !reviewed.contains(&file.path))
        })
        .collect()
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ParsedDiffLine {
    kind: &'static str,
    old: Option<u32>,
    new: Option<u32>,
    text: String,
}

fn parse_diff_range(value: &str) -> Option<u32> {
    value
        .split_once(',')
        .map_or(value, |(start, _)| start)
        .trim()
        .parse()
        .ok()
}

fn parse_hunk_header(line: &str) -> Option<(u32, u32)> {
    let mut fields = line.split_whitespace();
    fields.next()?;
    let old = fields.next()?.strip_prefix('-')?;
    let new = fields.next()?.strip_prefix('+')?;
    Some((parse_diff_range(old)?, parse_diff_range(new)?))
}

fn parse_unified_diff(patch: &str) -> Vec<ParsedDiffLine> {
    let mut old_line = None;
    let mut new_line = None;
    let mut parsed = Vec::new();
    for line in patch.lines() {
        if line.starts_with("@@") {
            if let Some((old, new)) = parse_hunk_header(line) {
                old_line = Some(old);
                new_line = Some(new);
                parsed.push(ParsedDiffLine {
                    kind: "hunk",
                    old: None,
                    new: None,
                    text: line.to_owned(),
                });
            }
            continue;
        }
        if line.starts_with("diff ")
            || line.starts_with("index ")
            || line.starts_with("--- ")
            || line.starts_with("+++ ")
        {
            continue;
        }
        if line.starts_with("\\ No newline") {
            parsed.push(ParsedDiffLine {
                kind: "note",
                old: None,
                new: None,
                text: line.to_owned(),
            });
            continue;
        }
        let (kind, old, new, text) = match line.as_bytes().first().copied() {
            Some(b'+') => {
                let new = new_line;
                new_line = new_line.map(|line| line + 1);
                ("add", None, new, line[1..].to_owned())
            }
            Some(b'-') => {
                let old = old_line;
                old_line = old_line.map(|line| line + 1);
                ("del", old, None, line[1..].to_owned())
            }
            Some(b' ') | None => {
                let old = old_line;
                let new = new_line;
                old_line = old_line.map(|line| line + 1);
                new_line = new_line.map(|line| line + 1);
                (
                    "context",
                    old,
                    new,
                    line.get(1..).unwrap_or_default().to_owned(),
                )
            }
            _ => continue,
        };
        parsed.push(ParsedDiffLine {
            kind,
            old,
            new,
            text,
        });
    }
    parsed
}

pub(crate) fn render_diff(patch: &str) -> Element {
    let binary = patch.contains("Binary files") || patch.contains("GIT binary patch");
    let lines = parse_unified_diff(patch);
    if binary {
        return rsx! {
            div { class: "binary-preview",
                Icon { name: "file".to_owned() }
                strong { "二进制文件" }
                p { "Git 返回二进制变更，无法生成文本 diff。" }
            }
        };
    }
    if lines.is_empty() {
        return rsx! {
            div { class: "change-preview-empty",
                strong { if patch.trim().is_empty() { "没有文本 diff" } else { "无法解析此 patch" } }
                p { "该文件可能为空、未追踪，或节点返回了非 unified 格式。" }
            }
        };
    }
    rsx! {
        div { class: "diff-lines",
            for line in lines {
                if line.kind == "hunk" {
                    div { class: "diff-hunk", "{line.text}" }
                } else {
                    {
                        let old_number = line.old.map_or_else(String::new, |number| number.to_string());
                        let new_number = line.new.map_or_else(String::new, |number| number.to_string());
                        let marker = if line.kind == "add" { "+" } else if line.kind == "del" { "−" } else { " " };
                        rsx! {
                            div { class: "diff-line {line.kind}",
                                span { class: "line-number", "{old_number}" }
                                span { class: "line-number", "{new_number}" }
                                span { class: "diff-marker", "{marker}" }
                                span { "{line.text}" }
                            }
                        }
                    }
                }
            }
        }
    }
}
