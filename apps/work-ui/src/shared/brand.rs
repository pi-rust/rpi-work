//! User-supplied rpi gear mark, embedded locally for Web, desktop and Android.
use base64::Engine as _;
use dioxus::prelude::*;
use std::sync::LazyLock;

static IMAGE_URL: LazyLock<String> = LazyLock::new(|| {
    let image = include_bytes!("../../../../assets/branding/rpi-gear.png");
    format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(image)
    )
});

static ICON_URL: LazyLock<String> = LazyLock::new(|| {
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1254 1254"><style>@media(prefers-color-scheme:dark){{image{{filter:invert(1)}}}}</style><image width="1254" height="1254" href="{}"/></svg>"#,
        *IMAGE_URL
    );
    format!(
        "data:image/svg+xml;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(svg)
    )
});

#[component]
pub(crate) fn BrandMark(class: String) -> Element {
    rsx! {
        span {
            class,
            "aria-hidden": "true",
            style: format!("--brand-image: url('{}')", *IMAGE_URL),
            span { class: "brand-image" }
        }
    }
}

pub(crate) fn icon_url() -> &'static str {
    ICON_URL.as_str()
}
