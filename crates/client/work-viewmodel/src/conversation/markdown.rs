//! Bounded, renderer-independent Markdown presentation. Never emit HTML strings,
//! remote image sources, arbitrary attributes or platform/file actions. This is
//! public answer formatting, not message identity or execution state.

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use serde::Serialize;

use super::code_tokens;

const MAX_BYTES: usize = 1024 * 1024;
const MAX_EVENTS: usize = 32_768;
const MAX_DEPTH: usize = 48;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarkdownView {
    pub nodes: Vec<Node>,
    pub copy_text: String,
    /// Complexity fallback preserves the entire original as escaped plain text.
    pub plain: bool,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Node {
    Text {
        text: String,
    },
    Element {
        tag: Element,
        children: Vec<Self>,
        #[serde(skip_serializing_if = "Option::is_none")]
        start: Option<u64>,
    },
    Link {
        href: Option<String>,
        children: Vec<Self>,
    },
    Image {
        alt: String,
    },
    CodeBlock {
        code: String,
        language: String,
        tokens: Vec<CodeToken>,
    },
    Task {
        checked: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Element {
    Div,
    P,
    H1,
    H2,
    H3,
    H4,
    H5,
    H6,
    Blockquote,
    Ul,
    Ol,
    Li,
    Em,
    Strong,
    Del,
    Code,
    Span,
    Br,
    Hr,
    Table,
    Thead,
    Tbody,
    Tr,
    Th,
    Td,
}

#[derive(Debug, Serialize)]
pub struct CodeToken {
    pub text: String,
    pub class: &'static str,
}

impl Node {
    fn element(tag: Element) -> Self {
        Self::Element {
            tag,
            children: Vec::new(),
            start: None,
        }
    }

    fn children_mut(&mut self) -> Option<&mut Vec<Self>> {
        match self {
            Self::Element { children, .. } | Self::Link { children, .. } => Some(children),
            _ => None,
        }
    }
}

/// Only explicit HTTP(S) links may navigate, on a user gesture. Relative paths,
/// credentials, controls, backslashes and external application schemes cannot
/// gain file/platform capabilities by appearing inside an answer.
#[must_use]
pub fn safe_href(value: &str) -> Option<String> {
    if value.contains('\\') || value.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return None;
    }
    let url = url::Url::parse(value).ok()?;
    (matches!(url.scheme(), "http" | "https")
        && value.split_once("://").is_some_and(|(scheme, authority)| {
            scheme.eq_ignore_ascii_case(url.scheme()) && !authority.starts_with('/')
        })
        && url.host_str().is_some()
        && url.username().is_empty()
        && url.password().is_none())
    .then(|| url.to_string())
}

fn heading(level: HeadingLevel) -> Element {
    match level {
        HeadingLevel::H1 => Element::H1,
        HeadingLevel::H2 => Element::H2,
        HeadingLevel::H3 => Element::H3,
        HeadingLevel::H4 => Element::H4,
        HeadingLevel::H5 => Element::H5,
        HeadingLevel::H6 => Element::H6,
    }
}

fn start_node(tag: Tag<'_>, stack: &[Node]) -> Node {
    let element = match tag {
        Tag::Paragraph | Tag::HtmlBlock => Element::P,
        Tag::Heading { level, .. } => heading(level),
        Tag::BlockQuote(_) => Element::Blockquote,
        Tag::List(start) => {
            return Node::Element {
                tag: if start.is_some() {
                    Element::Ol
                } else {
                    Element::Ul
                },
                children: Vec::new(),
                start,
            };
        }
        Tag::Item => Element::Li,
        Tag::Emphasis => Element::Em,
        Tag::Strong => Element::Strong,
        Tag::Strikethrough => Element::Del,
        Tag::Link { dest_url, .. } => {
            return Node::Link {
                href: safe_href(&dest_url),
                children: Vec::new(),
            };
        }
        Tag::CodeBlock(kind) => {
            let language = match kind {
                CodeBlockKind::Fenced(info) => info
                    .split_ascii_whitespace()
                    .next()
                    .filter(|s| s.len() <= 32 && s.is_ascii())
                    .unwrap_or("text")
                    .to_owned(),
                CodeBlockKind::Indented => "text".into(),
            };
            return Node::CodeBlock {
                code: String::new(),
                language,
                tokens: Vec::new(),
            };
        }
        Tag::Table(_) => Element::Table,
        Tag::TableHead => Element::Thead,
        Tag::TableRow => Element::Tr,
        Tag::TableCell => {
            if stack.iter().any(|node| {
                matches!(
                    node,
                    Node::Element {
                        tag: Element::Thead,
                        ..
                    }
                )
            }) {
                Element::Th
            } else {
                Element::Td
            }
        }
        // Images use a temporary container, converted to inert alt text at End.
        // Other extension tags are not enabled; future text stays escaped.
        _ => Element::Span,
    };
    Node::element(element)
}

fn finish_node(mut node: Node, end: TagEnd) -> Node {
    if end == TagEnd::Image {
        return Node::Image {
            alt: copy_node(&node),
        };
    }
    if let Node::CodeBlock {
        code,
        language,
        tokens,
    } = &mut node
    {
        *tokens = code_tokens::tokenize(code, language)
            .into_iter()
            .map(|token| CodeToken {
                text: token.text.to_owned(),
                class: token.class,
            })
            .collect();
    }
    if let Node::Element { tag, children, .. } = &mut node {
        if *tag == Element::Thead {
            *children = vec![Node::Element {
                tag: Element::Tr,
                children: std::mem::take(children),
                start: None,
            }];
        } else if *tag == Element::Table {
            let mut head = Vec::new();
            let mut body = Vec::new();
            for child in std::mem::take(children) {
                if matches!(
                    &child,
                    Node::Element {
                        tag: Element::Thead,
                        ..
                    }
                ) {
                    head.push(child);
                } else {
                    body.push(child);
                }
            }
            head.push(Node::Element {
                tag: Element::Tbody,
                children: body,
                start: None,
            });
            *children = head;
        }
    }
    node
}

fn push(stack: &mut [Node], node: Node) {
    if let Some(children) = stack.last_mut().and_then(Node::children_mut) {
        children.push(node);
    }
}

fn fallback(source: &str) -> MarkdownView {
    MarkdownView {
        nodes: vec![Node::Text {
            text: source.to_owned(),
        }],
        copy_text: source.to_owned(),
        plain: true,
    }
}

/// `CommonMark` plus tables, strikethrough and read-only task markers. No HTML
/// renderer, heading attributes, math, diagrams, embeds or executable extensions.
/// Excess complexity falls back to the full source, not truncated output.
pub fn present(source: &str) -> MarkdownView {
    if source.len() > MAX_BYTES {
        return fallback(source);
    }
    let options =
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let mut stack = vec![Node::element(Element::Div)];
    for (count, event) in Parser::new_ext(source, options).enumerate() {
        if count >= MAX_EVENTS {
            return fallback(source);
        }
        match event {
            Event::Start(tag) => {
                if stack.len() >= MAX_DEPTH {
                    return fallback(source);
                }
                let node = start_node(tag, &stack);
                stack.push(node);
            }
            Event::End(end) => {
                let Some(node) = stack.pop() else {
                    return fallback(source);
                };
                push(&mut stack, finish_node(node, end));
            }
            Event::Text(text) => {
                if let Some(Node::CodeBlock { code, .. }) = stack.last_mut() {
                    code.push_str(&text);
                } else {
                    push(
                        &mut stack,
                        Node::Text {
                            text: text.into_string(),
                        },
                    );
                }
            }
            Event::Html(text)
            | Event::InlineHtml(text)
            | Event::InlineMath(text)
            | Event::DisplayMath(text) => {
                push(
                    &mut stack,
                    Node::Text {
                        text: text.into_string(),
                    },
                );
            }
            Event::Code(text) => push(
                &mut stack,
                Node::Element {
                    tag: Element::Code,
                    children: vec![Node::Text {
                        text: text.into_string(),
                    }],
                    start: None,
                },
            ),
            Event::SoftBreak => push(&mut stack, Node::Text { text: "\n".into() }),
            Event::HardBreak => push(&mut stack, Node::element(Element::Br)),
            Event::Rule => push(&mut stack, Node::element(Element::Hr)),
            Event::TaskListMarker(checked) => push(&mut stack, Node::Task { checked }),
            Event::FootnoteReference(text) => push(
                &mut stack,
                Node::Text {
                    text: format!("[^{text}]"),
                },
            ),
        }
    }
    let Some(Node::Element {
        children: nodes, ..
    }) = stack.pop()
    else {
        return fallback(source);
    };
    let copy_text = copy_children(&nodes);
    MarkdownView {
        nodes,
        copy_text,
        plain: false,
    }
}

fn block(node: &Node) -> bool {
    matches!(
        node,
        Node::CodeBlock { .. }
            | Node::Element {
                tag: Element::P
                    | Element::H1
                    | Element::H2
                    | Element::H3
                    | Element::H4
                    | Element::H5
                    | Element::H6
                    | Element::Blockquote
                    | Element::Ul
                    | Element::Ol
                    | Element::Table
                    | Element::Hr,
                ..
            }
    )
}

fn copy_children(nodes: &[Node]) -> String {
    let mut output = String::new();
    for (index, node) in nodes.iter().enumerate() {
        if index > 0 && (block(node) || block(&nodes[index - 1])) {
            output.push_str("\n\n");
        }
        output.push_str(&copy_node(node));
    }
    output
}

/// Readable answer text only: no header, author, copy controls or tool output.
/// Paragraphs use blank lines, lists/rows newlines, table cells tabs. Code-copy
/// retains the parsed code exactly; answer-copy omits its final block newline.
fn copy_node(node: &Node) -> String {
    match node {
        Node::Text { text } => text.clone(),
        Node::CodeBlock { code, .. } => code.trim_end_matches('\n').to_owned(),
        Node::Image { alt } => format!("图片（未加载）：{alt}"),
        Node::Task { checked } => if *checked { "[x] " } else { "[ ] " }.into(),
        Node::Link { children, .. } => copy_children(children),
        Node::Element { tag, children, .. } => match tag {
            Element::Br => "\n".into(),
            Element::Hr => "---".into(),
            Element::Ul | Element::Ol | Element::Table | Element::Thead | Element::Tbody => {
                children
                    .iter()
                    .map(copy_node)
                    .collect::<Vec<_>>()
                    .join("\n")
            }
            Element::Tr => children
                .iter()
                .map(copy_node)
                .collect::<Vec<_>>()
                .join("\t"),
            _ => copy_children(children),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn view(text: &str) -> Value {
        serde_json::to_value(present(text)).unwrap()
    }

    #[test]
    fn structured_blocks_inline_and_copy_are_renderer_independent() {
        let output = view(
            "## Heading\n\nHello **strong**, *em*, ~~gone~~ and `code`.\n\n- a\n- b\n\n> quote\n\n3. third\n4. fourth",
        );
        assert_eq!(
            output["copyText"],
            "Heading\n\nHello strong, em, gone and code.\n\na\nb\n\nquote\n\nthird\nfourth"
        );
        assert_eq!(output["nodes"][0]["tag"], "h2");
        assert_eq!(output["nodes"][4]["tag"], "ol");
        assert_eq!(output["nodes"][4]["start"], 3);
    }

    #[test]
    fn table_has_valid_head_body_rows_and_read_only_tasks() {
        let output = view("| A | B |\n| - | - |\n| one | two |\n\n- [x] done\n- [ ] todo");
        let table = &output["nodes"][0];
        assert_eq!(table["tag"], "table");
        assert_eq!(table["children"][0]["tag"], "thead");
        assert_eq!(table["children"][0]["children"][0]["tag"], "tr");
        assert_eq!(
            table["children"][0]["children"][0]["children"][0]["tag"],
            "th"
        );
        assert_eq!(table["children"][1]["tag"], "tbody");
        assert_eq!(output["copyText"], "A\tB\none\ttwo\n\n[x] done\n[ ] todo");
    }

    #[test]
    fn code_reuses_bounded_rust_tokens_and_keeps_code_payload() {
        let output = view("```rust extra-info\nfn main() { println!(\"<script>\"); }\n```\n");
        let code = &output["nodes"][0];
        assert_eq!(code["language"], "rust");
        assert_eq!(code["code"], "fn main() { println!(\"<script>\"); }\n");
        assert_eq!(
            code["tokens"][0],
            json!({"class":"code-token-keyword","text":"fn"})
        );
        let joined: String = code["tokens"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["text"].as_str().unwrap())
            .collect();
        assert_eq!(joined, code["code"].as_str().unwrap());
    }

    #[test]
    fn unclosed_streamed_fence_and_unknown_language_keep_text() {
        for text in [
            "```unrecognized\nstreaming <img>\n",
            "```rust\nlet value = \"incomplete",
        ] {
            let output = view(text);
            assert_eq!(output["nodes"][0]["kind"], "codeBlock");
            assert!(output["copyText"].as_str().unwrap().contains(
                if text.contains("unrecognized") {
                    "streaming <img>"
                } else {
                    "let value"
                }
            ));
        }
    }

    #[test]
    fn only_explicit_credential_free_http_links_navigate() {
        for href in [
            "https://example.org/path?q=1#part",
            "http://127.0.0.1:3030/",
            "HTTPS://example.org/",
        ] {
            assert!(safe_href(href).is_some(), "{href}");
        }
        for href in [
            "javascript:alert(1)",
            "data:text/html,x",
            "file:///tmp/a",
            "rpi://execute",
            "mailto:a@example.org",
            "//example.org",
            "src/main.rs",
            "#part",
            "https://",
            "https:example.org",
            "https:///example.org",
            "https://user:secret@example.org",
            "https://x\\@evil.org",
            "https://x\n.test",
            " https://example.org",
        ] {
            assert!(safe_href(href).is_none(), "{href}");
        }
    }

    #[test]
    fn entity_encoded_scheme_cannot_bypass_safe_links() {
        let output = view("[bad](jav&#97;script:alert%281%29) [ok](https://example.org)");
        let children = output["nodes"][0]["children"].as_array().unwrap();
        assert!(children[0]["href"].is_null());
        assert_eq!(children[2]["href"], "https://example.org/");
        assert_eq!(output["copyText"], "bad ok");
    }

    #[test]
    fn raw_html_is_only_text_and_images_never_expose_sources() {
        let output = view(
            "<script>alert(1)</script>\n\n<img src=\"https://example.org/track\" onerror=\"bad()\">\n\n![alt **bold**](https://example.org/secret-tracker)",
        );
        let serialized = output.to_string();
        assert!(!serialized.contains("secret-tracker"));
        assert_eq!(
            output["nodes"][2]["children"][0],
            json!({"kind":"image","alt":"alt bold"})
        );
        assert!(
            output["copyText"]
                .as_str()
                .unwrap()
                .contains("<script>alert(1)</script>")
        );
    }

    #[test]
    fn byte_depth_and_event_limits_preserve_full_plain_source() {
        for source in [
            "界".repeat(MAX_BYTES / 3 + 1),
            format!("{}text", "> ".repeat(MAX_DEPTH + 1)),
            "a\n\n".repeat(MAX_EVENTS),
        ] {
            let output = present(&source);
            assert!(output.plain);
            assert_eq!(output.copy_text, source);
            assert!(matches!(&output.nodes[0], Node::Text { text } if text == &source));
        }
    }

    #[test]
    fn literal_html_entities_and_unicode_are_text_not_double_escaped() {
        assert_eq!(
            present("Unicode 中文 &amp; &lt;b&gt; \\*literal\\*").copy_text,
            "Unicode 中文 & <b> *literal*"
        );
        assert_eq!(present("").copy_text, "");
        assert!(!present("").plain);
    }
}
