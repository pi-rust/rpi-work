pub(crate) fn node_origin(url: &str) -> String {
    if super::local_node::is_local(url) {
        // The local workspace identity is independent of transport and runtime paths.
        return "rpi-work://local".to_owned();
    }
    let trimmed = url.trim();
    let Some((scheme, remainder)) = trimmed.split_once("://") else {
        return trimmed.to_ascii_lowercase();
    };
    let scheme = scheme.to_ascii_lowercase();
    let authority = remainder
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default()
        .rsplit('@')
        .next()
        .unwrap_or_default();
    let (host, port) = if let Some(end) = authority.find(']') {
        (&authority[..=end], authority[end + 1..].strip_prefix(':'))
    } else {
        authority
            .rsplit_once(':')
            .map_or((authority, None), |(host, port)| (host, Some(port)))
    };
    let port = port.and_then(|port| port.parse::<u16>().ok());
    let port = port
        .filter(|port| !((scheme == "http" && *port == 80) || (scheme == "https" && *port == 443)));
    let host = host.to_ascii_lowercase();
    match port {
        Some(port) => format!("{scheme}://{host}:{port}"),
        None => format!("{scheme}://{host}"),
    }
}

pub(crate) fn project_scope_key(origin: &str, project_id: &str) -> String {
    format!("{}::project::{project_id}", node_origin(origin))
}

pub(crate) fn session_scope_key(origin: &str, session_id: &str) -> String {
    format!("{}::session::{session_id}", node_origin(origin))
}

#[cfg(test)]
mod scope_identity_tests {
    use super::{node_origin, project_scope_key, session_scope_key};

    #[test]
    fn origins_ignore_paths_credentials_case_and_default_ports() {
        assert_eq!(
            node_origin(" HTTP://Example.COM:080/base?query#fragment "),
            "http://example.com"
        );
        assert_eq!(
            node_origin("https://user:secret@Example.COM:443?query"),
            "https://example.com"
        );
        assert_eq!(node_origin("http://[::1]:80/base"), "http://[::1]");
        assert_eq!(node_origin("http://[::1]:3030/"), "http://[::1]:3030");
    }

    #[test]
    fn project_and_session_identity_preserve_distinct_nodes() {
        let first = "http://localhost:3030/";
        let second = "http://localhost:4040/";
        let mut purposes = std::collections::HashMap::new();
        purposes.insert(project_scope_key(first, "same-id"), "元Agent");
        purposes.insert(project_scope_key(second, "same-id"), "普通项目");
        assert_eq!(purposes.len(), 2);
        assert_eq!(
            purposes[&project_scope_key("HTTP://LOCALHOST:3030/base", "same-id")],
            "元Agent"
        );
        assert_eq!(purposes[&project_scope_key(second, "same-id")], "普通项目");
        assert_ne!(
            session_scope_key(first, "same-id"),
            session_scope_key(second, "same-id")
        );
        assert_eq!(
            session_scope_key(first, "same-id"),
            session_scope_key("HTTP://LOCALHOST:3030/base", "same-id")
        );
    }
}
