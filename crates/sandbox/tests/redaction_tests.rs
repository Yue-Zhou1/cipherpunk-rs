use sandbox::redaction::redact_ai_prompt;

#[test]
fn redacts_explicit_secret_markers() {
    let input = "Authorization: Bearer abc123\nsafe-line";
    let redacted = redact_ai_prompt(input);
    assert_eq!(redacted, "[redacted]\nsafe-line");
}

#[test]
fn does_not_redact_plain_token_substrings() {
    let input = "let parser_tokenizer = true;";
    let redacted = redact_ai_prompt(input);
    assert_eq!(redacted, input);
}

#[test]
fn does_not_redact_secret_sharing_context() {
    let input = "secret-sharing schemes can tolerate threshold failures";
    let redacted = redact_ai_prompt(input);
    assert_eq!(redacted, input);
}

#[test]
fn redacts_structured_secret_assignments() {
    let input = "client_secret = super-sensitive-value";
    let redacted = redact_ai_prompt(input);
    assert_eq!(redacted, "[redacted]");
}
