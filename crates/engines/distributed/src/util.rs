pub fn sanitize_ident(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        "generated".to_string()
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::sanitize_ident;

    #[test]
    fn sanitize_ident_normalizes_symbols_and_case() {
        assert_eq!(sanitize_ident("My-Id v1"), "my_id_v1");
    }
}
