//! Read-only file presentation. Diff colors describe raw prefixes, not parsed
//! hunks, changed-line identities, review targets or executable git actions.
use crate::conversation::code_tokens;
use serde::Serialize;

#[derive(Serialize)]
pub struct FileLine<'a> {
    pub number: Option<usize>,
    pub kind: &'static str,
    pub tokens: Vec<code_tokens::Token<'a>>,
}

/// Reuse the bounded full-file lexer and its CRLF-aware row splitting. Keep the
/// caller's original string separately for exact copying; rows are display only.
pub fn present<'a>(source: &'a str, path: &str, diff: bool) -> Vec<FileLine<'a>> {
    // Bound rendered row/token work too, not only the lexer. Preserve the full
    // source in one literal text node rather than allocating a million rows.
    if source.len() > 1024 * 1024
        || source
            .bytes()
            .filter(|byte| *byte == b'\n')
            .take(10_000)
            .count()
            >= 10_000
    {
        return vec![FileLine {
            number: None,
            kind: "plain",
            tokens: vec![code_tokens::Token {
                class: "",
                text: source,
            }],
        }];
    }
    code_tokens::tokenize_file(source, if diff { "" } else { path })
        .into_iter()
        .enumerate()
        .map(|(index, tokens)| {
            let first = tokens.first().map_or("", |token| token.text);
            let kind = if !diff {
                "source"
            } else if first.starts_with("+++")
                || first.starts_with("---")
                || first.starts_with("@@")
                || first.starts_with("diff ")
            {
                "meta"
            } else if first.starts_with('+') {
                "add"
            } else if first.starts_with('-') {
                "remove"
            } else {
                "context"
            };
            FileLine {
                number: (!diff).then_some(index + 1),
                kind,
                tokens,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_uses_shared_tokens_and_real_rows_not_trailing_phantom_lines() {
        let lines = present("pub fn main() {\r\n\r\n}\r\n", "main.rs", false);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].number, Some(1));
        assert_eq!(lines[0].tokens[0].text, "pub");
        assert_eq!(lines[0].tokens[0].class, "code-token-keyword");
        assert!(lines[1].tokens.is_empty());
        assert_eq!(lines[2].number, Some(3));
    }
    #[test]
    fn raw_diff_headers_are_not_added_deleted_lines_or_file_line_numbers() {
        let text = "--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n+new\n context\n\\ No newline at end of file\n";
        let rows = present(text, "file.rs", true);
        assert_eq!(
            rows.iter().map(|row| row.kind).collect::<Vec<_>>(),
            [
                "meta", "meta", "meta", "remove", "add", "context", "context"
            ]
        );
        assert!(rows.iter().all(|row| row.number.is_none()));
        let reconstructed: String = rows
            .iter()
            .map(|row| {
                row.tokens
                    .iter()
                    .map(|token| token.text)
                    .collect::<String>()
                    + "\n"
            })
            .collect();
        assert_eq!(reconstructed, text);
    }
    #[test]
    fn large_or_many_line_files_use_one_full_literal_node_not_unbounded_rows() {
        for source in ["\n".repeat(10_000) + "尾部", "界".repeat(350_000)] {
            for diff in [false, true] {
                let rows = present(&source, "main.rs", diff);
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0].kind, "plain");
                assert_eq!(rows[0].number, None);
                assert_eq!(rows[0].tokens[0].text, source);
                assert!(rows[0].tokens[0].class.is_empty());
            }
        }
    }

    #[test]
    fn empty_and_unknown_sources_are_literal_without_evaluation_or_truncation() {
        assert!(present("", "empty.txt", false).is_empty());
        let text = "<img src=never>\n\u{4e16}\u{754c}";
        let rows = present(text, "unknown.bin", false);
        assert_eq!(rows[0].tokens[0].text, "<img src=never>");
        assert_eq!(rows[1].tokens[0].text, "世界");
        assert!(
            rows.iter()
                .flat_map(|row| &row.tokens)
                .all(|token| token.class.is_empty())
        );
    }
}
