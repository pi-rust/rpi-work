use std::io::Read;

pub(crate) const LIMIT: usize = 4096;

// No Debug/Display implementation: credential bytes must never enter diagnostics.
pub(crate) struct Password(String);
impl Password {
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
    pub(crate) fn from_env(value: String) -> Result<Self, String> {
        validate(value.into_bytes())
    }
}

fn validate(mut bytes: Vec<u8>) -> Result<Password, String> {
    if bytes.len() > LIMIT {
        return Err("password exceeds 4096 bytes (input omitted)".into());
    }
    if bytes.last() == Some(&b'\n') {
        bytes.pop();
        if bytes.last() == Some(&b'\r') {
            bytes.pop();
        }
    }
    if bytes.is_empty() || bytes.iter().any(|b| matches!(b, b'\n' | b'\r' | 0)) {
        return Err("password must be one nonempty line (input omitted)".into());
    }
    String::from_utf8(bytes)
        .map(Password)
        .map_err(|_| "password must be UTF-8 (input omitted)".into())
}

pub(crate) fn read_password(reader: impl Read) -> Result<Password, String> {
    let mut bytes = Vec::new();
    reader
        .take((LIMIT + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| "could not read password from stdin (input omitted)".to_string())?;
    validate(bytes)
}

pub(crate) async fn stdin_password() -> Result<Password, String> {
    // Tokio's blocking stdin can keep runtime shutdown waiting forever after a
    // deadline. A detached OS reader thread instead dies with the CLI process.
    let (sender, receiver) = tokio::sync::oneshot::channel();
    std::thread::Builder::new()
        .name("password-stdin".into())
        .spawn(move || {
            let _ = sender.send(read_password(std::io::stdin().lock()));
        })
        .map_err(|_| "could not read password from stdin".to_string())?;
    tokio::time::timeout(super::DEADLINE, receiver)
        .await
        .map_err(|_| "password stdin deadline exceeded (15s)".to_string())?
        .map_err(|_| "password stdin reader failed".to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    fn error(bytes: &[u8]) -> String {
        match read_password(bytes) {
            Ok(_) => panic!("expected rejection"),
            Err(e) => e,
        }
    }
    #[test]
    fn bounded_single_line() {
        assert_eq!(
            read_password(&b"secret\r\n"[..])
                .unwrap_or_else(|_| panic!("valid line"))
                .as_str(),
            "secret"
        );
        assert!(read_password(&vec![b'x'; LIMIT][..]).is_ok());
        assert!(read_password(&vec![b'x'; LIMIT + 1][..]).is_err());
        for bytes in [
            &b"secret\nsecond\n"[..],
            &b"secret\r"[..],
            &b"secret\0"[..],
            &b""[..],
            &[255][..],
        ] {
            assert!(!error(bytes).contains("secret"));
        }
    }
    #[test]
    fn consumes_at_most_limit_plus_one() {
        let bytes = vec![b'x'; LIMIT * 3];
        let mut cursor = std::io::Cursor::new(bytes);
        assert!(read_password(&mut cursor).is_err());
        assert_eq!(cursor.position(), (LIMIT + 1) as u64);
    }
}
