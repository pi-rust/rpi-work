//! Browser node selection is same-origin page navigation, not a credential proxy.
//! A return hint never selects a node or grants authorization; only a user action navigates.
use dioxus::prelude::*;

#[derive(Clone, Copy)]
pub(crate) struct NodeNavigation {
    pub home_address: Signal<String>,
    pub navigate: EventHandler<String>,
}

#[cfg(any(target_arch = "wasm32", test))]
fn home_address(current: &str, hint: Option<&str>) -> String {
    hint.and_then(|raw| super::connections::normalize_address(raw, false).ok())
        .unwrap_or_else(|| current.trim_end_matches('/').to_owned())
}

#[cfg(any(target_arch = "wasm32", test))]
fn destination(address: &str, home: &str) -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({
        "address": super::connections::normalize_address(address, false)?,
        "home": super::connections::normalize_address(home, false)?,
    }))
}

#[cfg(target_arch = "wasm32")]
pub(super) fn use_navigation(current: String, mut warning: Signal<String>) {
    let initial = current.trim_end_matches('/').to_owned();
    let mut home = use_signal(|| initial);
    use_future(move || {
        let current = current.clone();
        async move {
            // sessionStorage contains a public origin only, never passwords or tickets.
            // The fragment is a manual return hint, NOT an automatic redirect target.
            let mut reader = document::eval(
                r#"
                const marker = new URLSearchParams(location.hash.slice(1));
                let hint = marker.get('rpi-entry');
                if (hint === null) {
                    try { hint = sessionStorage.getItem('rpi-work-web-home-v1'); } catch (_) {}
                }
                if (marker.has('rpi-entry')) {
                    marker.delete('rpi-entry');
                    const hash = marker.toString();
                    history.replaceState(history.state, '', location.pathname + location.search + (hash ? '#' + hash : ''));
                }
                dioxus.send(hint);
            "#,
            );
            let hint = reader.recv::<Option<String>>().await.ok().flatten();
            let address = home_address(&current, hint.as_deref());
            home.set(address.clone());
            let encoded = serde_json::to_string(&address).expect("origin serializes");
            document::eval(&format!(
                "try {{ sessionStorage.setItem('rpi-work-web-home-v1', {encoded}); }} catch (_) {{}}"
            ));
        }
    });
    use_context_provider(move || NodeNavigation {
        home_address: home,
        navigate: EventHandler::new(move |address: String| {
            let target = match destination(&address, &home.peek()) {
                Ok(target) => target,
                Err(error) => {
                    warning.set(error);
                    return;
                }
            };
            // No cross-origin fetch, API discovery, password forwarding, or owner proxy.
            // Suppress Referer so the current session URL is not disclosed to the target.
            document::eval(&format!(
                r#"
                const target = {target};
                const url = new URL(target.address + '/');
                url.hash = new URLSearchParams({{'rpi-entry': target.home}}).toString();
                if (url.origin === location.origin) {{
                    // A hash-only link would not remount HostedEntry after expiry.
                    // Explicit same-node recovery must revalidate discovery and auth.
                    history.replaceState(history.state, '', url.href);
                    location.reload();
                }} else {{
                    const link = document.createElement('a');
                    link.href = url.href;
                    link.referrerPolicy = 'no-referrer';
                    link.rel = 'noreferrer';
                    link.click();
                }}
            "#
            ));
        }),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn return_hint_is_only_a_validated_origin() {
        assert_eq!(
            home_address("https://current.test/", None),
            "https://current.test"
        );
        assert_eq!(
            home_address("https://current.test", Some("https://entry.test:14180/")),
            "https://entry.test:14180"
        );
        for raw in [
            "javascript:alert(1)",
            "//other.test",
            "https://u:p@other.test",
            "https://other.test/path",
            "https://other.test/?password=x",
            "https://other.test/#ticket=x",
            "https://other.test\\evil",
            " https://other.test\nignored",
            "",
        ] {
            assert_eq!(
                home_address("https://current.test", Some(raw)),
                "https://current.test",
                "{raw}"
            );
            assert!(destination(raw, "https://current.test").is_err(), "{raw}");
            assert!(destination("https://other.test", raw).is_err(), "{raw}");
        }
    }

    #[test]
    fn navigation_contains_only_the_target_and_public_return_origin() {
        assert_eq!(
            destination("HTTP://127.0.0.1:14180/", "https://entry.test/").unwrap(),
            serde_json::json!({"address":"http://127.0.0.1:14180", "home":"https://entry.test"})
        );
        assert_eq!(
            destination("https://entry.test", "https://entry.test").unwrap()["home"],
            "https://entry.test"
        );
        assert!(
            destination(
                &format!("https://{}.test", "a".repeat(2048)),
                "https://entry.test"
            )
            .is_err()
        );
    }
}
