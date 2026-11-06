//! Bounded syntax coloring for display, not a full parser or syntax validator.
//!
//! Only explicit ASCII language aliases enable coloring. All output borrows the
//! original source; render `text` with normal Dioxus text interpolation (escaping
//! remains the renderer's responsibility), never as HTML. This lexer deliberately
//! omits contextual syntax such as JS regex/JSX, template interpolation and shell
//! heredocs. Incomplete streamed strings/comments may extend through EOF.

const MAX_SOURCE_BYTES: usize = 64 * 1024;
const MAX_TOKENS: usize = 4096;
const KEYWORD: &str = "code-token-keyword";
const STRING: &str = "code-token-string";
const COMMENT: &str = "code-token-comment";
const NUMBER: &str = "code-token-number";
const LITERAL: &str = "code-token-literal";

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Token<'a> {
    pub class: &'static str,
    pub text: &'a str,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Language {
    Rust,
    JavaScript,
    Python,
    Json,
    Shell,
}

impl Language {
    fn from_alias(alias: &str) -> Option<Self> {
        // Bound metadata work too; do not parse fence attributes or HTML labels.
        if alias.len() > 32 || !alias.is_ascii() {
            return None;
        }
        let alias = alias.trim_matches(|c: char| c.is_ascii_whitespace());
        [
            (Self::Rust, &["rust", "rs"][..]),
            (
                Self::JavaScript,
                &["js", "javascript", "jsx", "ts", "typescript", "tsx"][..],
            ),
            (Self::Python, &["python", "py"][..]),
            (Self::Json, &["json"][..]),
            (Self::Shell, &["shell", "bash", "sh", "zsh"][..]),
        ]
        .into_iter()
        .find_map(|(language, aliases)| {
            aliases
                .iter()
                .any(|known| alias.eq_ignore_ascii_case(known))
                .then_some(language)
        })
    }

    fn word_class(self, word: &str) -> &'static str {
        let literal = match self {
            Self::Rust | Self::Shell => matches!(word, "true" | "false"),
            Self::JavaScript => matches!(word, "true" | "false" | "null" | "undefined"),
            Self::Python => matches!(word, "True" | "False" | "None"),
            Self::Json => matches!(word, "true" | "false" | "null"),
        };
        if literal {
            return LITERAL;
        }
        let keyword = match self {
            Self::Rust => matches!(
                word,
                "as" | "async"
                    | "await"
                    | "break"
                    | "const"
                    | "continue"
                    | "crate"
                    | "dyn"
                    | "else"
                    | "enum"
                    | "extern"
                    | "fn"
                    | "for"
                    | "if"
                    | "impl"
                    | "in"
                    | "let"
                    | "loop"
                    | "match"
                    | "mod"
                    | "move"
                    | "mut"
                    | "pub"
                    | "ref"
                    | "return"
                    | "self"
                    | "Self"
                    | "static"
                    | "struct"
                    | "super"
                    | "trait"
                    | "type"
                    | "unsafe"
                    | "use"
                    | "where"
                    | "while"
                    | "yield"
            ),
            Self::JavaScript => matches!(
                word,
                "abstract"
                    | "as"
                    | "async"
                    | "await"
                    | "break"
                    | "case"
                    | "catch"
                    | "class"
                    | "const"
                    | "continue"
                    | "debugger"
                    | "declare"
                    | "default"
                    | "delete"
                    | "do"
                    | "else"
                    | "enum"
                    | "export"
                    | "extends"
                    | "finally"
                    | "for"
                    | "from"
                    | "function"
                    | "get"
                    | "if"
                    | "implements"
                    | "import"
                    | "in"
                    | "instanceof"
                    | "interface"
                    | "keyof"
                    | "let"
                    | "namespace"
                    | "new"
                    | "of"
                    | "private"
                    | "protected"
                    | "public"
                    | "readonly"
                    | "return"
                    | "set"
                    | "static"
                    | "super"
                    | "switch"
                    | "this"
                    | "throw"
                    | "try"
                    | "type"
                    | "typeof"
                    | "var"
                    | "void"
                    | "while"
                    | "with"
                    | "yield"
            ),
            Self::Python => matches!(
                word,
                "and"
                    | "as"
                    | "assert"
                    | "async"
                    | "await"
                    | "break"
                    | "class"
                    | "continue"
                    | "def"
                    | "del"
                    | "elif"
                    | "else"
                    | "except"
                    | "finally"
                    | "for"
                    | "from"
                    | "global"
                    | "if"
                    | "import"
                    | "in"
                    | "is"
                    | "lambda"
                    | "nonlocal"
                    | "not"
                    | "or"
                    | "pass"
                    | "raise"
                    | "return"
                    | "try"
                    | "while"
                    | "with"
                    | "yield"
            ),
            Self::Shell => matches!(
                word,
                "if" | "then"
                    | "else"
                    | "elif"
                    | "fi"
                    | "for"
                    | "while"
                    | "until"
                    | "do"
                    | "done"
                    | "case"
                    | "esac"
                    | "in"
                    | "function"
                    | "select"
                    | "time"
                    | "export"
                    | "local"
                    | "readonly"
                    | "return"
                    | "break"
                    | "continue"
            ),
            Self::Json => false,
        };
        if keyword { KEYWORD } else { "" }
    }
}

pub(super) fn tokenize<'a>(source: &'a str, language: &str) -> Vec<Token<'a>> {
    if source.is_empty() {
        return Vec::new();
    }
    if source.len() > MAX_SOURCE_BYTES {
        return plain(source);
    }
    let Some(language) = Language::from_alias(language) else {
        return plain(source);
    };
    let bytes = source.as_bytes();
    let mut tokens: Vec<Token<'a>> = Vec::new();
    let mut offset = 0;
    // Delay plain runs rather than allocating a token for every punctuation mark.
    let mut plain_start = 0;
    while offset < bytes.len() {
        let start = offset;
        let (end, class) = scan(source, start, language);
        offset = end;
        if class.is_empty() {
            continue;
        }
        if plain_start < start && !push(&mut tokens, "", &source[plain_start..start]) {
            return plain(source);
        }
        if !push(&mut tokens, class, &source[start..end]) {
            return plain(source);
        }
        plain_start = end;
    }
    if plain_start < source.len() && !push(&mut tokens, "", &source[plain_start..]) {
        return plain(source);
    }
    tokens
}

fn plain(source: &str) -> Vec<Token<'_>> {
    vec![Token {
        class: "",
        text: source,
    }]
}

fn push<'a>(tokens: &mut Vec<Token<'a>>, class: &'static str, text: &'a str) -> bool {
    if tokens.len() == MAX_TOKENS {
        return false;
    }
    tokens.push(Token { class, text });
    true
}

fn scan(source: &str, start: usize, language: Language) -> (usize, &'static str) {
    let b = source.as_bytes();
    let byte = b[start];
    let tail = &b[start..];
    let c_family = matches!(language, Language::Rust | Language::JavaScript);
    if (c_family && tail.starts_with(b"//"))
        || (language == Language::Python && byte == b'#')
        || (language == Language::Shell && byte == b'#' && shell_word_start(b, start))
    {
        let mut end = start;
        while end < b.len() && !matches!(b[end], b'\n' | b'\r') {
            end += 1;
        }
        return (end, COMMENT);
    }
    if c_family && tail.starts_with(b"/*") {
        return (block_comment(b, start, language == Language::Rust), COMMENT);
    }
    if language == Language::Rust && byte == b'r' {
        if let Some(end) = rust_raw_string(b, start) {
            return (end, STRING);
        }
    }
    if byte == b'"'
        || (byte == b'\'' && !matches!(language, Language::Json | Language::Rust))
        || (byte == b'`' && matches!(language, Language::JavaScript | Language::Shell))
    {
        let triple =
            language == Language::Python && b.get(start..start + 3) == Some(&[byte, byte, byte]);
        let escapes = !(language == Language::Shell && byte == b'\'');
        return (quoted(b, start, byte, triple, escapes), STRING);
    }
    if language == Language::Rust && byte == b'\'' {
        // Only a complete, single-character/escape literal is a string. An
        // apostrophe before a lifetime or label must not swallow the rest.
        if let Some(end) = rust_character(source, start) {
            return (end, STRING);
        }
    }
    if language == Language::Shell && byte == b'\\' {
        // An escaped '#' or quote cannot begin a comment/string.
        let end = start + 1;
        return (next_char(source, end), "");
    }
    if byte.is_ascii_digit() {
        return (number(b, start), NUMBER);
    }
    if ident_start(byte) {
        let mut end = start + 1;
        while end < b.len() && ident_continue(b[end]) {
            end += 1;
        }
        return (end, language.word_class(&source[start..end]));
    }
    (next_char(source, start), "")
}

fn next_char(source: &str, start: usize) -> usize {
    start + source[start..].chars().next().map_or(0, char::len_utf8)
}

fn ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_' || byte == b'$' || byte >= 128
}

fn ident_continue(byte: u8) -> bool {
    ident_start(byte) || byte.is_ascii_digit()
}

fn shell_word_start(bytes: &[u8], start: usize) -> bool {
    if start == 0 {
        return true;
    }
    let previous = bytes[start - 1];
    if !previous.is_ascii_whitespace()
        && !matches!(previous, b';' | b'|' | b'&' | b'(' | b')' | b'<' | b'>')
    {
        return false;
    }
    // An escaped separator is part of the current word, not a comment boundary.
    // Each candidate examines its own preceding run, keeping total work linear.
    let mut before = start - 1;
    while before > 0 && bytes[before - 1] == b'\\' {
        before -= 1;
    }
    (start - 1 - before) % 2 == 0
}

fn block_comment(bytes: &[u8], start: usize, nested: bool) -> usize {
    let mut end = start + 2;
    let mut depth = 1;
    while end < bytes.len() {
        if nested && bytes[end..].starts_with(b"/*") {
            depth += 1;
            end += 2;
        } else if bytes[end..].starts_with(b"*/") {
            end += 2;
            depth -= 1;
            if depth == 0 {
                return end;
            }
        } else {
            end += 1;
        }
    }
    end
}

fn quoted(bytes: &[u8], start: usize, quote: u8, triple: bool, escapes: bool) -> usize {
    let width = if triple { 3 } else { 1 };
    let mut end = start + width;
    while end < bytes.len() {
        if escapes && bytes[end] == b'\\' {
            end += 1;
            if end < bytes.len() {
                // Skip bytes, not slices; the final boundary is quote/EOF.
                end += 1;
            }
        } else if bytes[end] == quote
            && (!triple || bytes.get(end..end + 3) == Some(&[quote, quote, quote]))
        {
            return end + width;
        } else {
            end += 1;
        }
    }
    end
}

fn rust_raw_string(bytes: &[u8], start: usize) -> Option<usize> {
    let mut end = start + 1;
    while end < bytes.len() && bytes[end] == b'#' {
        end += 1;
    }
    let hashes = end - start - 1;
    if bytes.get(end) != Some(&b'"') {
        return None;
    }
    end += 1;
    while end < bytes.len() {
        if bytes[end] != b'"' {
            end += 1;
            continue;
        }
        end += 1;
        let hash_start = end;
        while end < bytes.len() && end - hash_start < hashes && bytes[end] == b'#' {
            end += 1;
        }
        if end - hash_start == hashes {
            return Some(end);
        }
        // Consume the examined hash run even on a mismatch: no rescanning.
    }
    Some(end)
}

fn rust_character(source: &str, start: usize) -> Option<usize> {
    let b = source.as_bytes();
    let mut end = start + 1;
    match *b.get(end)? {
        b'\\' => {
            end += 1;
            match *b.get(end)? {
                b'x' => {
                    end += 1;
                    for _ in 0..2 {
                        if !b.get(end)?.is_ascii_hexdigit() {
                            return None;
                        }
                        end += 1;
                    }
                }
                b'u' => {
                    end += 1;
                    if b.get(end) != Some(&b'{') {
                        return None;
                    }
                    end += 1;
                    // Rust Unicode escapes have at most six hex digits.
                    let digits = end;
                    while end - digits < 6 && b.get(end).is_some_and(u8::is_ascii_hexdigit) {
                        end += 1;
                    }
                    if end == digits || b.get(end) != Some(&b'}') {
                        return None;
                    }
                    end += 1;
                }
                b'\'' | b'"' | b'\\' | b'n' | b'r' | b't' | b'0' => end += 1,
                _ => return None,
            }
        }
        b'\'' | b'\n' | b'\r' => return None,
        _ => end = next_char(source, end),
    }
    (b.get(end) == Some(&b'\'')).then_some(end + 1)
}

fn number(bytes: &[u8], start: usize) -> usize {
    let mut end = start;
    if bytes.get(start) == Some(&b'0') {
        let radix = match bytes.get(start + 1) {
            Some(b'x' | b'X') => 16,
            Some(b'o' | b'O') => 8,
            Some(b'b' | b'B') => 2,
            _ => 0,
        };
        if radix != 0 {
            end += 2;
            while end < bytes.len() && (bytes[end] == b'_' || (bytes[end] as char).is_digit(radix))
            {
                end += 1;
            }
            return end;
        }
    }
    while end < bytes.len() && (bytes[end].is_ascii_digit() || bytes[end] == b'_') {
        end += 1;
    }
    if bytes.get(end) == Some(&b'.') && bytes.get(end + 1).is_some_and(u8::is_ascii_digit) {
        end += 1;
        while end < bytes.len() && (bytes[end].is_ascii_digit() || bytes[end] == b'_') {
            end += 1;
        }
    }
    if matches!(bytes.get(end), Some(b'e' | b'E')) {
        let mut exponent = end + 1;
        if matches!(bytes.get(exponent), Some(b'+' | b'-')) {
            exponent += 1;
        }
        if bytes.get(exponent).is_some_and(u8::is_ascii_digit) {
            end = exponent + 1;
            while end < bytes.len() && (bytes[end].is_ascii_digit() || bytes[end] == b'_') {
                end += 1;
            }
        }
    }
    end
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checked<'a>(source: &'a str, language: &str) -> Vec<Token<'a>> {
        let tokens = tokenize(source, language);
        assert_eq!(tokens.iter().map(|t| t.text).collect::<String>(), source);
        assert!(tokens.len() <= MAX_TOKENS);
        let mut offset = 0;
        for token in &tokens {
            assert_ne!(token.text, "");
            assert_eq!(token.text.as_ptr(), source[offset..].as_ptr());
            assert!(matches!(
                token.class,
                "" | KEYWORD | STRING | COMMENT | NUMBER | LITERAL
            ));
            offset += token.text.len();
        }
        tokens
    }

    fn has(tokens: &[Token<'_>], class: &str, text: &str) -> bool {
        tokens.iter().any(|t| t.class == class && t.text == text)
    }

    #[test]
    fn rust_common_tokens() {
        let tokens = checked("pub fn main() { let x = 42; true } // hi\n", "rust");
        for word in ["pub", "fn", "let"] {
            assert!(has(&tokens, KEYWORD, word));
        }
        assert!(has(&tokens, NUMBER, "42"));
        assert!(has(&tokens, LITERAL, "true"));
        assert!(has(&tokens, COMMENT, "// hi"));
    }

    #[test]
    fn javascript_and_typescript_aliases() {
        for alias in ["js", "javascript", "jsx", "ts", "typescript", "tsx", " TS "] {
            let tokens = checked("const x = `hi ${name}`; /* c */ return null;", alias);
            assert!(has(&tokens, KEYWORD, "const"));
            assert!(has(&tokens, STRING, "`hi ${name}`"));
            assert!(has(&tokens, COMMENT, "/* c */"));
            assert!(has(&tokens, LITERAL, "null"));
        }
    }

    #[test]
    fn python_tokens_and_triple_quotes() {
        let tokens = checked(
            "def f():\n  return True, None, '''hé\nllo''', 2.5 # c",
            "py",
        );
        assert!(has(&tokens, KEYWORD, "def"));
        assert!(has(&tokens, LITERAL, "True"));
        assert!(has(&tokens, LITERAL, "None"));
        assert!(has(&tokens, STRING, "'''hé\nllo'''"));
        assert!(has(&tokens, NUMBER, "2.5"));
        assert!(has(&tokens, COMMENT, "# c"));
    }

    #[test]
    fn json_is_not_javascript() {
        let tokens = checked(
            "{\"x\": true, \"n\": -2.3e+4, \"v\": null, const: 'a'} // x",
            "json",
        );
        assert!(has(&tokens, STRING, "\"x\""));
        assert!(has(&tokens, LITERAL, "true"));
        assert!(has(&tokens, LITERAL, "null"));
        assert!(has(&tokens, NUMBER, "2.3e+4"));
        assert!(!tokens.iter().any(|t| matches!(t.class, KEYWORD | COMMENT)));
        assert!(!has(&tokens, STRING, "'a'"));
    }

    #[test]
    fn shell_aliases_and_comments() {
        for alias in ["shell", "bash", "sh", "zsh"] {
            let tokens = checked(
                "if true; then echo '#no' \"hi\" foo#bar ${#name} \\#safe a\\ #word a\\;#word; # yes\nfi",
                alias,
            );
            assert!(has(&tokens, KEYWORD, "if"));
            assert!(has(&tokens, STRING, "'#no'"));
            assert!(has(&tokens, STRING, "\"hi\""));
            assert_eq!(tokens.iter().filter(|t| t.class == COMMENT).count(), 1);
            assert!(has(&tokens, COMMENT, "# yes"));
        }
    }

    #[test]
    fn rust_lifetimes_and_labels_are_not_strings() {
        let tokens = checked(
            "fn f<'a>(x: &'a str) { 'outer: loop { break 'outer; } }",
            "rs",
        );
        assert!(!tokens.iter().any(|t| t.class == STRING));
        let tokens = checked("'a' '界' '\\n' '\\x41' '\\u{1F600}' 'unfinished", "rust");
        assert_eq!(tokens.iter().filter(|t| t.class == STRING).count(), 5);
    }

    #[test]
    fn rust_raw_strings_and_nested_comments() {
        let tokens = checked("r##\"<b>\"# still\"## /* a /* b */ c */ true", "rust");
        assert!(has(&tokens, STRING, "r##\"<b>\"# still\"##"));
        assert!(has(&tokens, COMMENT, "/* a /* b */ c */"));
        assert!(has(&tokens, LITERAL, "true"));
    }

    #[test]
    fn html_stays_original_text() {
        let source = "const x = \"<img src=x onerror='bad()'>&amp;\"; // </script>";
        let tokens = checked(source, "js");
        assert!(has(&tokens, STRING, "\"<img src=x onerror='bad()'>&amp;\""));
        assert!(has(&tokens, COMMENT, "// </script>"));
    }

    #[test]
    fn weird_metadata_has_no_autodetection() {
        let source = "fn test() { true } <script>";
        for alias in [
            "",
            "text",
            "html",
            "css",
            "rust extra",
            "{.rust}",
            "rust\" onclick=x",
            "<script>",
            "ｒｕｓｔ",
            "ru\0st",
            "rust\u{a0}",
        ] {
            assert_eq!(checked(source, alias), plain(source));
        }
        assert_eq!(checked(source, &"rust".repeat(1000)), plain(source));
    }

    #[test]
    fn unicode_identifiers_and_escapes() {
        for language in ["rust", "js", "python", "json", "sh"] {
            let tokens = checked("let界 café 😀 true界 \"\\😀界\" #界\n", language);
            assert!(!has(&tokens, KEYWORD, "let"));
            assert!(!has(&tokens, LITERAL, "true"));
            assert!(has(&tokens, STRING, "\"\\😀界\""));
        }
    }

    #[test]
    fn incomplete_streams_are_lossless() {
        for (source, language, class) in [
            ("\"界\\", "rust", STRING),
            ("'未完", "python", STRING),
            ("'''未\n完", "python", STRING),
            ("`未完 ${", "js", STRING),
            ("/* 未完", "rust", COMMENT),
            ("/* /* 未完 */", "rust", COMMENT),
            ("// 未完", "ts", COMMENT),
            ("# 未完", "bash", COMMENT),
            ("r###\"未完\"##", "rust", STRING),
        ] {
            assert!(has(&checked(source, language), class, source));
        }
    }

    #[test]
    fn empty_source_has_no_tokens() {
        for language in ["rust", "js", "py", "json", "sh", ""] {
            assert_eq!(checked("", language), [] as [Token<'_>; 0]);
        }
    }

    #[test]
    fn source_size_budget_is_in_bytes() {
        let at_limit = format!("true{}", " ".repeat(MAX_SOURCE_BYTES - 4));
        assert!(has(&checked(&at_limit, "rust"), LITERAL, "true"));
        let over_limit = format!("{at_limit}界");
        assert_eq!(checked(&over_limit, "rust"), plain(&over_limit));
        let unicode = "界".repeat(MAX_SOURCE_BYTES / 3 + 1);
        assert_eq!(checked(&unicode, "js"), plain(&unicode));
    }

    #[test]
    fn token_budget_falls_back_for_the_whole_source() {
        let at_limit = "true ".repeat(MAX_TOKENS / 2);
        assert_eq!(checked(&at_limit, "rust").len(), MAX_TOKENS);
        let over_limit = format!("{at_limit}true");
        assert_eq!(checked(&over_limit, "rust"), plain(&over_limit));
    }

    #[test]
    fn numbers_and_ranges() {
        let tokens = checked("0xff 0b101 0o77 12_345 1.25e-3 1..2 1e+", "rust");
        for number in ["0xff", "0b101", "0o77", "12_345", "1.25e-3", "1", "2"] {
            assert!(has(&tokens, NUMBER, number));
        }
        assert!(!has(&tokens, NUMBER, "1..2"));
        assert!(!has(&tokens, NUMBER, "1e+"));
    }

    #[test]
    fn comments_and_strings_do_not_cross_classify() {
        let tokens = checked("\"// not comment\" /* \"not string\" */ '\\''", "js");
        assert!(has(&tokens, STRING, "\"// not comment\""));
        assert!(has(&tokens, COMMENT, "/* \"not string\" */"));
        assert!(has(&tokens, STRING, "'\\''"));
        let tokens = checked("// c\r\nlet x", "js");
        assert!(has(&tokens, COMMENT, "// c"));
        assert!(has(&tokens, KEYWORD, "let"));
    }

    #[test]
    fn all_stream_prefixes_and_deterministic_noise_reconstruct() {
        let sample = "fn café<'a>() { r##\"界\"##; '🦀'; /* c */ }\n'\\u{1234}' # x";
        for language in ["rust", "js", "python", "json", "shell", "unknown"] {
            for end in (0..=sample.len()).filter(|end| sample.is_char_boundary(*end)) {
                checked(&sample[..end], language);
            }
            let mut state = 7_u32;
            let alphabet = [
                'a', '界', '😀', '\'', '"', '`', '#', '/', '*', '\\', '0', '\n', ' ', '$',
            ];
            let mut source = String::new();
            for _ in 0..2000 {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                source.push(alphabet[state as usize % alphabet.len()]);
            }
            checked(&source, language);
        }
    }

    #[test]
    fn adversarial_raw_delimiters_stay_bounded() {
        let source = format!("r{}\"{}", "#".repeat(1000), "\"###".repeat(10000));
        assert!(has(&checked(&source, "rust"), STRING, &source));
        let source = "r######## ".repeat(5000);
        checked(&source, "rust");
    }
}
