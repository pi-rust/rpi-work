use dioxus::prelude::*;
pub(crate) fn project_file_kind(path: &str) -> &'static str {
    let extension = std::path::Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default();
    if extension.eq_ignore_ascii_case("md") || extension.eq_ignore_ascii_case("markdown") {
        "markdown"
    } else if extension.eq_ignore_ascii_case("html") || extension.eq_ignore_ascii_case("htm") {
        "html"
    } else if ["png", "jpg", "jpeg", "gif", "webp"]
        .iter()
        .any(|image| extension.eq_ignore_ascii_case(image))
    {
        "image"
    } else {
        "code"
    }
}

pub(crate) fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

pub(crate) fn markdown_static_html(text: &str) -> String {
    let mut html = String::new();
    let mut in_code = false;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            if in_code {
                html.push_str("</code></pre>");
            } else {
                html.push_str("<pre><code>");
            }
            in_code = !in_code;
        } else if in_code {
            html.push_str(&escape_html(line));
            html.push('\n');
        } else if let Some(heading) = line.strip_prefix("# ") {
            html.push_str("<h1>");
            html.push_str(&escape_html(heading));
            html.push_str("</h1>");
        } else if let Some(heading) = line.strip_prefix("## ") {
            html.push_str("<h2>");
            html.push_str(&escape_html(heading));
            html.push_str("</h2>");
        } else if let Some(item) = line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")) {
            html.push_str("<p>• ");
            html.push_str(&escape_html(item));
            html.push_str("</p>");
        } else if !line.trim().is_empty() {
            html.push_str("<p>");
            html.push_str(&escape_html(line));
            html.push_str("</p>");
        }
    }
    if in_code {
        html.push_str("</code></pre>");
    }
    html
}

pub(crate) fn static_html_preview(source: &str) -> String {
    let mut rendered = String::new();
    let mut inside_title = false;
    for fragment in source.split('<') {
        let Some((tag, text)) = fragment.split_once('>') else {
            rendered.push_str(&escape_html(fragment));
            continue;
        };
        let tag = tag.trim().to_ascii_lowercase();
        let closing = tag.starts_with('/');
        let name = tag
            .trim_start_matches('/')
            .split_whitespace()
            .next()
            .unwrap_or_default();
        if name == "title" {
            inside_title = !closing;
            continue;
        }
        if inside_title {
            continue;
        }
        match (name, closing) {
            ("h1" | "h2" | "h3", false) => rendered.push_str("<h1>"),
            ("h1" | "h2" | "h3", true) => rendered.push_str("</h1>"),
            ("p", false) => rendered.push_str("<p>"),
            ("p", true) => rendered.push_str("</p>"),
            ("a", false) => rendered.push_str("<span>"),
            ("a", true) => rendered.push_str("</span>"),
            _ => {}
        }
        rendered.push_str(&escape_html(text));
    }
    rendered
}

pub(crate) fn render_project_file(
    content: &work_client::FileContent,
    mode: &str,
    wrap: bool,
) -> Element {
    let kind = project_file_kind(&content.path);
    if mode == "preview" && kind == "markdown" {
        return rsx! { article { class: "project-markdown", dangerous_inner_html: markdown_static_html(&content.content) } };
    }
    if mode == "preview" && kind == "html" {
        let srcdoc = format!(
            "<!doctype html><meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; script-src 'none'; style-src 'unsafe-inline'; img-src 'none'; connect-src 'none'; form-action 'none'; base-uri 'none'\"><style>body{{font:13px/1.6 system-ui;padding:16px;color:#20252b}}h1{{font-size:22px}}a{{color:#176b87}}</style><body>{}</body>",
            static_html_preview(&content.content)
        );
        let iframe = format!(
            "<iframe class=\"project-html-preview\" sandbox=\"allow-same-origin\" referrerpolicy=\"no-referrer\" title=\"HTML 文件静态预览\" srcdoc=\"{}\"></iframe>",
            escape_html(&srcdoc)
        );
        return rsx! {
            div { class: "project-html-note", "静态 HTML · 禁止脚本、外部资源与外站导航" }
            div { class: "project-html-frame", dangerous_inner_html: iframe }
        };
    }
    let lines = content
        .content
        .lines()
        .enumerate()
        .map(|(index, line)| (index + 1, line.to_owned()))
        .collect::<Vec<_>>();
    rsx! {
        pre { class: if wrap { "project-code wrap-lines" } else { "project-code" },
            for (number, line) in lines {
                span { class: "project-code-line", i { "{number}" } code { "{line}" } }
            }
        }
    }
}
