const REDACTION_TOKEN: &str = "[redacted]";

pub fn redact_ai_prompt(input: &str) -> String {
    let cleaned = input
        .chars()
        .filter(|ch| *ch == '\n' || *ch == '\t' || !ch.is_control())
        .collect::<String>();

    let mut output = String::new();
    for line in cleaned.lines() {
        let lower = line.to_ascii_lowercase();
        if contains_sensitive_marker(&lower) {
            output.push_str(REDACTION_TOKEN);
            output.push('\n');
            continue;
        }

        let sanitized = line
            .replace("```", "'''")
            .replace("<|", "< ")
            .replace("|>", " >")
            .replace("<<", "< ")
            .replace(">>", " >")
            .replace("SYSTEM:", "[role-redacted]:")
            .replace("System:", "[role-redacted]:")
            .replace("system:", "[role-redacted]:")
            .replace("ASSISTANT:", "[role-redacted]:")
            .replace("Assistant:", "[role-redacted]:")
            .replace("assistant:", "[role-redacted]:")
            .replace("USER:", "[role-redacted]:")
            .replace("User:", "[role-redacted]:")
            .replace("user:", "[role-redacted]:");
        output.push_str(&sanitized);
        output.push('\n');
    }

    if output.ends_with('\n') {
        output.pop();
    }

    output
}

fn contains_sensitive_marker(lower_line: &str) -> bool {
    lower_line.contains("api_key")
        || lower_line.contains("apikey")
        || lower_line.contains("client_secret")
        || lower_line.contains("secret_key")
        || lower_line.contains("secret=")
        || lower_line.contains("secret =")
        || lower_line.contains("secret:")
        || lower_line.contains("secret :")
        || lower_line.contains("password")
        || lower_line.contains("authorization:")
        || lower_line.contains("bearer ")
        || lower_line.contains("access_token")
        || lower_line.contains("refresh_token")
        || lower_line.contains("id_token")
        || lower_line.contains("token=")
        || lower_line.contains("token =")
        || lower_line.contains("token:")
        || lower_line.contains("token :")
        || contains_space_delimited_token_disclosure(lower_line)
        || contains_space_delimited_secret_disclosure(lower_line)
}

fn contains_space_delimited_token_disclosure(lower_line: &str) -> bool {
    let Some(value) = find_space_delimited_marker_value(lower_line, "token") else {
        return false;
    };

    value.len() >= 6
        && value
            .chars()
            .any(|ch| ch.is_ascii_digit() || matches!(ch, '-' | '_' | '.' | '/' | '+' | '='))
}

fn contains_space_delimited_secret_disclosure(lower_line: &str) -> bool {
    let Some(value) = find_space_delimited_marker_value(lower_line, "secret") else {
        return false;
    };

    value.len() >= 8
}

fn find_space_delimited_marker_value<'a>(line: &'a str, marker: &str) -> Option<&'a str> {
    let tokens = line.split_whitespace().collect::<Vec<_>>();
    for window in tokens.windows(2) {
        let key = normalize_token(window[0]);
        if key != marker {
            continue;
        }
        let value = normalize_token(window[1]);
        if !value.is_empty() {
            return Some(value);
        }
    }
    None
}

fn normalize_token(token: &str) -> &str {
    token.trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && !matches!(ch, '_' | '-' | '.'))
}
