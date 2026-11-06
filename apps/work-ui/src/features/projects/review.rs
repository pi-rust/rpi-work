use crate::shared::components::Icon;
use dioxus::prelude::*;

pub(crate) const CSS: &str = include_str!("review.css");
pub(crate) fn change_status_label(status: &str) -> &'static str {
    let status = status.trim().to_ascii_uppercase();
    match status.as_str() {
        "A" | "ADDED" | "ADD" | "UNTRACKED" => return "A",
        "D" | "DELETED" | "DELETE" => return "D",
        "R" | "RENAMED" | "RENAME" => return "R",
        "C" | "COPIED" | "COPY" => return "C",
        "T" | "TYPE" | "TYPE_CHANGED" => return "T",
        "U" | "UNMERGED" | "CONFLICT" | "CONFLICTED" => return "U",
        "M" | "MODIFIED" | "CHANGED" => return "M",
        _ => {}
    }

    // Porcelain status is a two-column value.  An untracked file is `??`,
    // while either column may contain a different kind of change (`AM`,
    // `MM`, ` D`, and so on).  Conflicts are more useful than the individual
    // staged/worktree changes, so recognize them before the other columns.
    if status.contains('U') || matches!(status.as_str(), "AA" | "DD") {
        return "U";
    }
    if status == "??" || status.contains('A') {
        return "A";
    }
    if status.contains('D') {
        return "D";
    }
    if status.contains('R') {
        return "R";
    }
    if status.contains('C') {
        return "C";
    }
    if status.contains('T') {
        return "T";
    }
    "M"
}

fn normalized_path(path: &str) -> String {
    path.replace('\\', "/")
}

fn is_in_directory(path: &str, directory: &str) -> bool {
    let directory = normalized_path(directory.trim())
        .trim_matches('/')
        .to_owned();
    if directory.is_empty() {
        return true;
    }
    let path = normalized_path(path);
    path == directory || path.starts_with(&(directory + "/"))
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
            let matches_directory = is_in_directory(&file.path, directory);
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
    let mut in_hunk = false;
    let mut parsed = Vec::new();
    for line in patch.lines() {
        // A new file starts a new pair of counters.  Without this reset, a
        // malformed or metadata-only first file can leak its counters into
        // the next file in a combined patch.
        if line.starts_with("diff ") {
            old_line = None;
            new_line = None;
            in_hunk = false;
            continue;
        }
        if line.starts_with("@@") {
            if let Some((old, new)) = parse_hunk_header(line) {
                old_line = Some(old);
                new_line = Some(new);
                in_hunk = true;
                parsed.push(ParsedDiffLine {
                    kind: "hunk",
                    old: None,
                    new: None,
                    text: line.to_owned(),
                });
            }
            continue;
        }
        // File headers are only metadata before a hunk.  Inside a hunk,
        // `--- content` and `+++ content` are ordinary deletion/addition
        // lines whose source text starts with `--`/`++`.
        if !in_hunk
            && (line.starts_with("index ") || line.starts_with("--- ") || line.starts_with("+++ "))
        {
            continue;
        }
        if line.starts_with("\\ No newline") {
            if in_hunk {
                parsed.push(ParsedDiffLine {
                    kind: "note",
                    old: None,
                    new: None,
                    text: line.to_owned(),
                });
            }
            continue;
        }
        if !in_hunk {
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

fn is_binary_metadata_line(line: &str) -> bool {
    line == "GIT binary patch" || (line.starts_with("Binary files ") && line.ends_with(" differ"))
}

fn is_binary_patch(patch: &str) -> bool {
    patch.lines().any(is_binary_metadata_line)
}

fn rename_metadata(patch: &str) -> Option<Vec<String>> {
    let metadata = patch
        .lines()
        .filter(|line| {
            line.starts_with("similarity index ")
                || line.starts_with("rename from ")
                || line.starts_with("rename to ")
        })
        .map(str::to_owned)
        .collect::<Vec<_>>();
    (!metadata.is_empty()).then_some(metadata)
}

pub(crate) fn render_diff(patch: &str) -> Element {
    let binary = is_binary_patch(patch);
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
        if let Some(metadata) = rename_metadata(patch) {
            return rsx! {
                div { class: "change-preview-metadata",
                    strong { "重命名文件" }
                    p { "Git 报告了重命名，但没有文本内容变化。" }
                    for line in metadata {
                        div { class: "diff-meta", "{line}" }
                    }
                }
            };
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn changed_file(
        path: &str,
        status: &str,
        original_path: Option<&str>,
    ) -> work_client::ChangedFile {
        work_client::ChangedFile {
            path: path.to_owned(),
            original_path: original_path.map(str::to_owned),
            status: status.to_owned(),
            staged: status.chars().next().is_some_and(|value| value != ' '),
            unstaged: status.chars().nth(1).is_some_and(|value| value != ' '),
        }
    }

    #[test]
    fn maps_git_porcelain_double_column_statuses() {
        for (status, expected) in [
            ("??", "A"),
            (" D", "D"),
            ("D ", "D"),
            (" R", "R"),
            ("R ", "R"),
            ("AM", "A"),
            ("MM", "M"),
            ("UU", "U"),
            ("AA", "U"),
            ("DD", "U"),
            (" T", "T"),
        ] {
            assert_eq!(change_status_label(status), expected, "status {status}");
        }
    }

    #[test]
    fn filters_directory_at_path_boundary_and_normalizes_separators() {
        let files = vec![
            changed_file("src/main.rs", " M", None),
            changed_file("src\\lib.rs", "AM", None),
            changed_file("src", "A", None),
            changed_file("src-other/main.rs", "M ", None),
            changed_file("src2/main.rs", "M ", None),
            changed_file("README.md", "R ", Some("src/README.md")),
        ];
        let reviewed = HashSet::new();
        let filtered = filter_changed_files(&files, "", "all", "src", false, &reviewed);

        assert_eq!(
            filtered
                .iter()
                .map(|file| file.path.as_str())
                .collect::<Vec<_>>(),
            vec!["src/main.rs", "src\\lib.rs", "src"]
        );
        assert_eq!(
            filter_changed_files(&files, "", "A", "src", false, &reviewed)
                .iter()
                .map(|file| file.path.as_str())
                .collect::<Vec<_>>(),
            vec!["src\\lib.rs", "src"]
        );
    }

    #[test]
    fn preserves_hunk_lines_starting_with_header_markers_and_resets_files() {
        let patch = concat!(
            "diff --git a/one.txt b/one.txt\n",
            "index 1111111..2222222 100644\n",
            "--- a/one.txt\n",
            "+++ b/one.txt\n",
            "@@ -1,2 +1,2 @@\n",
            "--- source content\n",
            "+++ source content\n",
            "diff --git a/two.txt b/two.txt\n",
            "index 3333333..4444444 100644\n",
            "--- a/two.txt\n",
            "+++ b/two.txt\n",
            "@@ -10 +20 @@\n",
            "-old\n",
            "+new\n",
        );
        let lines = parse_unified_diff(patch);

        assert_eq!(lines.len(), 6);
        assert_eq!(lines[0].kind, "hunk");
        assert_eq!(
            (
                lines[1].kind,
                lines[1].old,
                lines[1].new,
                lines[1].text.as_str()
            ),
            ("del", Some(1), None, "-- source content")
        );
        assert_eq!(
            (
                lines[2].kind,
                lines[2].old,
                lines[2].new,
                lines[2].text.as_str()
            ),
            ("add", None, Some(1), "++ source content")
        );
        assert_eq!(
            (lines[4].old, lines[4].new, lines[4].text.as_str()),
            (Some(10), None, "old")
        );
        assert_eq!(
            (lines[5].old, lines[5].new, lines[5].text.as_str()),
            (None, Some(20), "new")
        );
    }

    #[test]
    fn does_not_mistake_binary_wording_in_source_for_binary_patch() {
        let source_text = concat!(
            "diff --git a/text.txt b/text.txt\n",
            "--- a/text.txt\n",
            "+++ b/text.txt\n",
            "@@ -1,2 +1,2 @@\n",
            "-Binary files old differ\n",
            "+Binary files new differ\n",
            " Binary files are ordinary source text\n",
        );

        assert!(!is_binary_patch(source_text));
        assert!(is_binary_patch(
            "Binary files a/image.png and b/image.png differ\n"
        ));
        assert!(is_binary_patch("GIT binary patch\n"));
    }

    #[test]
    fn extracts_rename_metadata_without_treating_it_as_text() {
        let patch = concat!(
            "similarity index 100%\n",
            "rename from old-name.rs\n",
            "rename to new-name.rs\n",
        );

        assert_eq!(
            rename_metadata(patch),
            Some(vec![
                "similarity index 100%".to_owned(),
                "rename from old-name.rs".to_owned(),
                "rename to new-name.rs".to_owned(),
            ])
        );
        assert_eq!(rename_metadata("diff --git a/a b/b\n"), None);
    }
}
