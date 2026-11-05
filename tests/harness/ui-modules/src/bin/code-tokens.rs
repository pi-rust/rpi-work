//! Dependency-free harness: includes the production display lexer and its tests.
//! Run with `rustc --edition=2021 --test ... -o target/code-tokens-tests.exe`.

#[path = "../../../../../apps/work-ui/src/features/conversation/code_tokens.rs"]
mod code_tokens;

fn main() {
    // Keep the regular auto-bin target useful without any UI/runtime dependency.
    let source = "fn main() { let ready = true; }";
    let tokens = code_tokens::tokenize(source, "rust");
    assert_eq!(
        tokens.iter().map(|token| token.text).collect::<String>(),
        source
    );
    assert!(tokens
        .iter()
        .any(|token| token.class == "code-token-keyword"));
}
