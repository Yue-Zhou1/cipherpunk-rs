const ALLOWED_PREFIXES: &[&str] = &[
    "https://rustsec.org/",
    "https://crates.io/api/",
    "https://docs.rs/",
    "https://eips.ethereum.org/",
    "https://raw.githubusercontent.com/RustSec/advisory-db/",
    "https://api.github.com/",
    "https://github.com/advisories/",
    "https://nvd.nist.gov/vuln/detail/",
    "https://services.nvd.nist.gov/rest/json/",
];

pub fn is_allowed_url(url: &str) -> bool {
    ALLOWED_PREFIXES
        .iter()
        .any(|prefix| url.starts_with(prefix))
        && !has_path_traversal(url)
}

pub fn validate_url(url: &str) -> anyhow::Result<()> {
    if !is_allowed_url(url) {
        anyhow::bail!(
            "URL '{}' is not on the research allowlist. Allowed prefixes: {:?}",
            url,
            ALLOWED_PREFIXES
        );
    }
    Ok(())
}

fn has_path_traversal(url: &str) -> bool {
    if !url.contains("..") {
        return false;
    }

    let path = match url.find("://") {
        Some(protocol_idx) => {
            let tail = &url[(protocol_idx + 3)..];
            match tail.find('/') {
                Some(path_idx) => &tail[path_idx..],
                None => "",
            }
        }
        None => url,
    };

    path.contains("/../") || path.ends_with("/..") || path.starts_with("../")
}
