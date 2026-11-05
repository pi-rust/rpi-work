//! Official rpi website mark, embedded locally for Web, desktop and Android.
use dioxus::prelude::*;

pub(crate) const SVG: &str = include_str!("../../../../assets/branding/rpi.svg");

#[component]
pub(crate) fn BrandMark(class: String) -> Element {
    rsx! { span { class, "aria-hidden": "true", dangerous_inner_html: SVG } }
}

pub(crate) fn icon_url() -> String {
    let mut url = String::from("data:image/svg+xml,");
    for byte in SVG.bytes() {
        use std::fmt::Write;
        let _ = write!(url, "%{byte:02X}");
    }
    url
}
