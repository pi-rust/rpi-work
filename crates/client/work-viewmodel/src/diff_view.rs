//! Validated, read-only unified-diff presentation. Numbers are display data,
//! never review identities, revisions, permissions or executable git actions.
use crate::file_view::{self, FileLine};
use serde::Serialize;

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
        }
        Err(reason) => view.fallback_reason = Some(reason),
    }
    view
}

#[cfg(test)]
mod tests {
    use super::*;
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
