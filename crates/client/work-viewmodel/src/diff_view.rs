//! Validated unified-diff presentation and draft feedback from an observed patch.
use crate::file_view::{self, FileLine};
use serde::Serialize;
use sha2::{Digest, Sha256};

#[derive(Serialize)]
pub struct DiffLine<'a> {
    #[serde(flatten)]
    pub line: FileLine<'a>,
    pub old_number: Option<u32>,
    pub new_number: Option<u32>,
}

#[derive(Serialize)]
pub struct DiffView<'a> {
    pub rows: Vec<DiffLine<'a>>,
    pub structured: bool,
    pub fallback_reason: Option<&'static str>,
    pub gutter_digits: usize,
    pub feedback_revision: Option<String>,
}

#[derive(Clone, Copy)]
struct Range {
    next: u32,
    remaining: u32,
}
impl Range {
    fn parse(text: &str) -> Option<Self> {
        let (start, count) = text.split_once(',').unwrap_or((text, "1"));
        let number = |value: &str| {
            (!value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()))
                .then(|| value.parse::<u32>().ok())
                .flatten()
        };
        let next = number(start)?;
        let remaining = number(count)?;
        if remaining > 0 && (next == 0 || next.checked_add(remaining - 1).is_none()) {
            return None;
        }
        Some(Self { next, remaining })
    }
    fn take(&mut self) -> Option<u32> {
        self.remaining = self.remaining.checked_sub(1)?;
        let number = self.next;
        if self.remaining > 0 {
            self.next = self.next.checked_add(1)?;
        }
        Some(number)
    }
    fn interval(self) -> (u64, u64) {
        let start = u64::from(self.next) + u64::from(self.remaining == 0);
        (start, start + u64::from(self.remaining))
    }
}

fn hunk(line: &str) -> Option<(Range, Range)> {
    let (old, rest) = line.strip_prefix("@@ -")?.split_once(" +")?;
    let (new, suffix) = rest.split_once(" @@")?;
    if !suffix.is_empty() && !suffix.starts_with(' ') {
        return None;
    }
    let ranges = (Range::parse(old)?, Range::parse(new)?);
    (ranges.0.remaining > 0 || ranges.1.remaining > 0).then_some(ranges)
}

#[derive(Clone, Copy)]
struct Mark {
    kind: &'static str,
    old: Option<u32>,
    new: Option<u32>,
}
impl Mark {
    const fn plain(kind: &'static str) -> Self {
        Self {
            kind,
            old: None,
            new: None,
        }
    }
}

fn validate(rows: &[DiffLine<'_>]) -> Result<Vec<Mark>, &'static str> {
    let mut marks = Vec::with_capacity(rows.len());
    let mut active: Option<(Range, Range)> = None;
    let mut headers = 0;
    let mut hunks = 0;
    let mut previous_end = (0, 0);
    let mut last_body: Option<(bool, bool)> = None;
    let mut eof = (false, false);
    for row in rows {
        let line = row.line.tokens.first().map_or("", |token| token.text);
        if line == "\\ No newline at end of file" {
            let (old, new) = last_body.take().ok_or("invalid")?;
            eof.0 |= old;
            eof.1 |= new;
            marks.push(Mark::plain("note"));
            continue;
        }
        last_body = None;
        if let Some((old, new)) = active.as_mut() {
            if old.remaining > 0 || new.remaining > 0 {
                let mut mark = Mark::plain("context");
                match line.as_bytes().first() {
                    Some(b'-') if !eof.0 => {
                        mark.kind = "remove";
                        mark.old = Some(old.take().ok_or("invalid")?);
                    }
                    Some(b'+') if !eof.1 => {
                        mark.kind = "add";
                        mark.new = Some(new.take().ok_or("invalid")?);
                    }
                    Some(b' ') if !eof.0 && !eof.1 => {
                        mark.old = Some(old.take().ok_or("invalid")?);
                        mark.new = Some(new.take().ok_or("invalid")?);
                    }
                    _ => return Err("invalid"),
                }
                last_body = Some((mark.old.is_some(), mark.new.is_some()));
                marks.push(mark);
                continue;
            }
            active = None;
        }
        if line.starts_with("diff --git ") {
            if matches!(headers, 1 | 2) {
                return Err("invalid");
            }
            headers = 0;
            previous_end = (0, 0);
            eof = (false, false);
        } else if line.starts_with("diff ")
            || line.starts_with("@@@")
            || line == "GIT binary patch"
            || line.starts_with("Binary files ")
        {
            return Err("unsupported");
        } else if line.starts_with("--- ") {
            if matches!(headers, 1 | 2) {
                return Err("invalid");
            }
            headers = 1;
            previous_end = (0, 0);
            eof = (false, false);
        } else if line.starts_with("+++ ") {
            if headers != 1 {
                return Err("invalid");
            }
            headers = 2;
        } else if line.starts_with("@@") {
            if !matches!(headers, 2 | 3) {
                return Err("invalid");
            }
            let (old, new) = hunk(line).ok_or("invalid")?;
            let (old_start, old_end) = old.interval();
            let (new_start, new_end) = new.interval();
            if old_start < previous_end.0 || new_start < previous_end.1 {
                return Err("invalid");
            }
            previous_end = (old_end, new_end);
            active = Some((old, new));
            hunks += 1;
            headers = 3;
            marks.push(Mark::plain("hunk"));
            continue;
        } else if headers != 0
            || ![
                "index ",
                "old mode ",
                "new mode ",
                "new file mode ",
                "deleted file mode ",
                "similarity index ",
                "dissimilarity index ",
                "rename from ",
                "rename to ",
                "copy from ",
                "copy to ",
            ]
            .iter()
            .any(|prefix| line.starts_with(prefix))
        {
            return Err("unsupported");
        }
        marks.push(Mark::plain("meta"));
    }
    if matches!(headers, 1 | 2)
        || active.is_some_and(|(old, new)| old.remaining > 0 || new.remaining > 0)
    {
        return Err("invalid");
    }
    if hunks == 0 {
        return Err("no_hunks");
    }
    Ok(marks)
}

/// Validate the complete bounded patch before exposing any line numbers. A bad
/// later hunk invalidates the entire projection, not just its remaining suffix.
#[must_use]
pub fn present<'a>(source: &'a str, path: &str) -> DiffView<'a> {
    let mut view = DiffView {
        rows: file_view::present(source, path, true)
            .into_iter()
            .map(|line| DiffLine {
                line,
                old_number: None,
                new_number: None,
            })
            .collect(),
        structured: false,
        fallback_reason: None,
        gutter_digits: 1,
        feedback_revision: None,
    };
    if view.rows.is_empty() {
        return view;
    }
    if view.rows[0].line.kind == "plain" {
        view.fallback_reason = Some("limit");
        return view;
    }
    match validate(&view.rows) {
        Ok(marks) => {
            for (row, mark) in view.rows.iter_mut().zip(marks) {
                row.line.kind = mark.kind;
                row.old_number = mark.old;
                row.new_number = mark.new;
                for number in [mark.old, mark.new].into_iter().flatten() {
                    view.gutter_digits = view.gutter_digits.max(number.to_string().len());
                }
            }
            view.structured = true;
            if feedback_lines(&view, path) {
                view.feedback_revision = Some(format!("{:x}", Sha256::digest(source.as_bytes())));
            }
        }
        Err(reason) => view.fallback_reason = Some(reason),
    }
    view
}

/// Only a single ordinary patch whose file markers agree with the requested
/// project-relative path can supply a feedback location. Other patches can
/// still use the read-only projection above.
#[must_use]
fn feedback_lines(view: &DiffView<'_>, path: &str) -> bool {
    if !view.structured || path.is_empty() || path.chars().any(char::is_control) {
        return false;
    }
    let mut markers = view.rows.iter().filter_map(|row| {
        (row.line.kind == "meta")
            .then(|| row.line.tokens.first().map(|token| token.text))
            .flatten()
    });
    let mut files = 0;
    let mut old = None;
    let mut new = None;
    for line in &mut markers {
        if line.starts_with("diff --git ") {
            files += 1;
            if files > 1 {
                return false;
            }
        } else if let Some(value) = line.strip_prefix("--- ") {
            if old.replace(value).is_some() {
                return false;
            }
        } else if let Some(value) = line.strip_prefix("+++ ") {
            if new.replace(value).is_some() {
                return false;
            }
        } else if ["rename ", "copy "]
            .iter()
            .any(|prefix| line.starts_with(prefix))
        {
            return false;
        }
    }
    let old_matches = old.is_some_and(|value| value.strip_prefix("a/") == Some(path));
    let new_matches = new.is_some_and(|value| value.strip_prefix("b/") == Some(path));
    (old_matches && (new_matches || new == Some("/dev/null")))
        || (old == Some("/dev/null") && new_matches)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftFeedback<'a> {
    pub path: &'a str,
    pub staged: bool,
    pub index: usize,
    pub revision: String,
    pub location: String,
    pub comment: &'a str,
    pub message: String,
}

/// A text draft quotes the exact observed row and names its original diff
/// scope. The caller must first re-read the same file and scope.
#[must_use]
pub fn feedback_draft<'a>(
    expected: &str,
    fresh: &str,
    path: &'a str,
    staged: bool,
    index: usize,
    text: &'a str,
) -> Option<DraftFeedback<'a>> {
    let text = text.trim();
    if expected != fresh || text.is_empty() || text.chars().count() > 2000 {
        return None;
    }
    let view = present(fresh, path);
    let revision = view.feedback_revision?;
    let row = view.rows.get(index)?;
    let (side, line) = row
        .new_number
        .map(|line| ("新", line))
        .or_else(|| row.old_number.map(|line| ("旧", line)))?;
    let line_text: String = row.line.tokens.iter().map(|token| token.text).collect();
    let excerpt: String = line_text.chars().take(240).collect();
    let suffix = if excerpt.len() < line_text.len() {
        "…"
    } else {
        ""
    };
    let scope = if staged { "已暂存" } else { "工作区" };
    Some(DraftFeedback {
        path,
        staged,
        index,
        revision,
        comment: text,
        location: format!("{path} · {side}行 {line} · {scope}"),
        message: format!(
            "行内反馈 · {path} · {scope} Diff · {side}行 {line}\n  {excerpt}{suffix}\n{text}"
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn feedback_only_quotes_the_exact_single_file_patch_and_line_side() {
        let patch = "--- a/file.rs\r\n+++ b/file.rs\r\n@@ -2 +3 @@\r\n-before\r\n+after\r\n";
        assert!(present(patch, "file.rs").feedback_revision.is_some());
        assert_eq!(
            feedback_draft(patch, patch, "file.rs", true, 3, " check ")
                .map(|feedback| feedback.message)
                .as_deref(),
            Some("行内反馈 · file.rs · 已暂存 Diff · 旧行 2\n  -before\ncheck")
        );
        assert_eq!(
            feedback_draft(patch, patch, "file.rs", false, 4, "check")
                .map(|feedback| feedback.message)
                .as_deref(),
            Some("行内反馈 · file.rs · 工作区 Diff · 新行 3\n  +after\ncheck")
        );
        assert!(feedback_draft(patch, patch, "file.rs", true, 2, "check").is_none());
        assert!(feedback_draft(patch, patch, "other.rs", true, 4, "check").is_none());
        assert!(
            feedback_draft(
                patch,
                &patch.replace("+after", "+changed"),
                "file.rs",
                true,
                4,
                "check"
            )
            .is_none()
        );
        assert!(
            present(&format!("{patch}{patch}"), "file.rs")
                .feedback_revision
                .is_none()
        );
    }

    fn numbers(view: &DiffView<'_>) -> Vec<(Option<u32>, Option<u32>)> {
        view.rows
            .iter()
            .filter(|row| row.old_number.is_some() || row.new_number.is_some())
            .map(|row| (row.old_number, row.new_number))
            .collect()
    }
    #[test]
    fn header_like_body_crlf_and_no_newline_are_not_metadata() {
        let text = "--- a/x\r\n+++ b/x\r\n@@ -10,2 +20,2 @@ fn\r\n--- source\r\n+++ source\r\n context\r\n\\ No newline at end of file\r\n";
        let view = present(text, "x");
        assert!(view.structured);
        assert_eq!(
            numbers(&view),
            [(Some(10), None), (None, Some(20)), (Some(11), Some(21))]
        );
        assert_eq!(
            view.rows
                .iter()
                .map(|row| row.line.kind)
                .collect::<Vec<_>>(),
            ["meta", "meta", "hunk", "remove", "add", "context", "note"]
        );
        assert_eq!(view.gutter_digits, 2);
    }
    #[test]
    fn zero_ranges_omitted_counts_multiple_hunks_and_file_resets() {
        let text = "diff --git a/x b/x\n--- /dev/null\n+++ b/x\n@@ -0,0 +1 @@\n+new\n@@ -10 +11,0 @@\n-old\ndiff --git a/y b/y\n--- a/y\n+++ b/y\n@@ -1 +1 @@\n-a\n+b\n";
        let view = present(text, "x");
        assert!(view.structured);
        assert_eq!(
            numbers(&view),
            [
                (None, Some(1)),
                (Some(10), None),
                (Some(1), None),
                (None, Some(1))
            ]
        );
    }
    #[test]
    fn checked_ranges_accept_last_u32_line_but_reject_overflow_or_zero_data_line() {
        let text = "--- a/x\n+++ b/x\n@@ -4294967295 +4294967295 @@\n-a\n+b\n";
        let view = present(text, "x");
        assert!(view.structured);
        assert_eq!(view.gutter_digits, 10);
        for header in [
            "@@ -4294967296 +1 @@",
            "@@ -4294967295,2 +1 @@",
            "@@ -0 +1 @@",
            "@@ -1,+1 +1 @@",
            "@@ -1 +1 @@bad",
            "@@ -0,0 +0,0 @@",
        ] {
            assert!(
                !present(&format!("--- a/x\n+++ b/x\n{header}\n-a\n+b\n"), "x").structured,
                "{header}"
            );
        }
    }
    #[test]
    fn malformed_truncated_overlapping_and_unknown_content_never_get_partial_numbers() {
        for tail in [
            "@@ -3,2 +3,2 @@\n-x\n+y\n",
            "@@ -1 +1 @@\n-x\n+y\n",
            "@@ -3 +3 @@\n\n",
            "@@ -3 +3 @@\nunknown\n",
            "--- a/next\n",
            "--- a/next\n+++ b/next\n",
            "trailing junk\n",
        ] {
            let text = format!("--- a/x\n+++ b/x\n@@ -1 +1 @@\n-a\n+b\n{tail}");
            let view = present(&text, "x");
            assert!(!view.structured, "{tail}");
            assert!(numbers(&view).is_empty());
            assert_eq!(view.rows.len(), text.lines().count());
        }
    }
    #[test]
    fn binary_combined_metadata_and_note_misuse_fall_back_literally() {
        for text in [
            "diff --cc x\n@@@ -1 -1 +1 @@@\n",
            "GIT binary patch\nliteral 0\n",
            "diff --git a/x b/x\nBinary files a/x and b/x differ\n",
            "diff --git a/x b/x\nrename from a\nrename to b\n",
            "\\ No newline at end of file\n",
            "--- a/x\n+++ b/x\n@@ -1,2 +1,2 @@\n a\n\\ No newline at end of file\n b\n",
        ] {
            let view = present(text, "x");
            assert!(!view.structured);
            assert!(numbers(&view).is_empty());
            assert_eq!(
                view.rows
                    .iter()
                    .map(|row| row
                        .line
                        .tokens
                        .iter()
                        .map(|token| token.text)
                        .collect::<String>())
                    .collect::<Vec<_>>(),
                text.lines().collect::<Vec<_>>()
            );
        }
    }
    #[test]
    fn no_newline_notes_track_each_side_and_preserve_all_metadata_and_unicode() {
        let source = "diff --git a/界 b/界\nindex 111..222 100644\n--- a/界\n+++ b/界\n@@ -1 +1 @@\n-旧行\n\\ No newline at end of file\n+新行\n\\ No newline at end of file\n";
        let view = present(source, "界");
        assert!(view.structured);
        assert_eq!(numbers(&view), [(Some(1), None), (None, Some(1))]);
        assert_eq!(
            view.rows
                .iter()
                .map(|row| row
                    .line
                    .tokens
                    .iter()
                    .map(|token| token.text)
                    .collect::<String>())
                .collect::<Vec<_>>(),
            source.lines().collect::<Vec<_>>()
        );
        let duplicated = source.to_owned() + "\\ No newline at end of file\n";
        assert!(!present(&duplicated, "界").structured);
        assert!(numbers(&present(&duplicated, "界")).is_empty());
    }
    #[test]
    fn empty_and_large_patches_keep_the_existing_full_literal_budget() {
        assert!(present("", "x").rows.is_empty());
        for text in ["\n".repeat(10_000) + "尾部", "界".repeat(350_000)] {
            let view = present(&text, "x");
            assert!(!view.structured);
            assert_eq!(view.fallback_reason, Some("limit"));
            assert_eq!(view.rows.len(), 1);
            assert_eq!(view.rows[0].line.tokens[0].text, text);
        }
    }
}
