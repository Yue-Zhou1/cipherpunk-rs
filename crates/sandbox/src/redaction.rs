const REDACTION_TOKEN: &str = "[redacted]";

pub fn redact_ai_prompt(input: &str) -> String {
    let cleaned = input
        .chars()
        .filter(|ch| *ch == '\n' || *ch == '\t' || !ch.is_control())
        .collect::<String>();

    let mut output = String::new();
    for line in cleaned.lines() {
        let lower = line.to_ascii_lowercase();
        if lower.contains("api_key")
            || lower.contains("token")
            || lower.contains("secret")
            || lower.contains("password")
        {
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
