use std::time::Duration;

use chrono::Utc;
use mockito::Matcher;
use research::allowlist::{is_allowed_url, validate_url};
use research::cache::ResearchCache;
use research::sources::github::GithubAdvisorySource;
use research::sources::nvd::NvdSource;
use research::sources::rustsec::RustSecSource;
use research::{ResearchFinding, ResearchQuery, ResearchResult, ResearchService};

#[test]
fn allowlist_accepts_known_security_sources() {
    for url in [
        "https://rustsec.org/advisories/RUSTSEC-2020-0001.html",
        "https://crates.io/api/v1/crates/openssl",
        "https://docs.rs/openssl/latest/openssl/",
        "https://eips.ethereum.org/EIPS/eip-155",
        "https://raw.githubusercontent.com/RustSec/advisory-db/main/crates/openssl/RUSTSEC-2020-0001.md",
        "https://github.com/advisories/GHSA-xxxx-yyyy-zzzz",
        "https://api.github.com/advisories?ecosystem=cargo&affects=openssl",
        "https://nvd.nist.gov/vuln/detail/CVE-2024-0001",
        "https://services.nvd.nist.gov/rest/json/cves/2.0?keywordSearch=openssl",
    ] {
        assert!(is_allowed_url(url), "{url} should be allowlisted");
        validate_url(url).expect("allowlisted URL should validate");
    }
}

#[test]
fn allowlist_blocks_non_allowlisted_and_tricky_urls() {
    for url in [
        "https://example.com/security",
        "https://rustsec.org.evil.com/advisories/foo",
        "https://docs.rs/../../etc/passwd",
    ] {
        assert!(!is_allowed_url(url), "{url} should not be allowlisted");
        assert!(
            validate_url(url).is_err(),
            "validate_url should reject {url}"
        );
    }
}

#[tokio::test]
async fn cache_returns_hit_then_expires_after_ttl() {
    let mut cache = ResearchCache::with_ttl(Duration::from_millis(25));
    let result = sample_result("cache-key");
    cache.insert("cache-key".to_string(), result);

    let hit = cache.get("cache-key");
    assert!(hit.is_some(), "cache hit expected before TTL");

    tokio::time::sleep(Duration::from_millis(35)).await;
    assert!(
        cache.get("cache-key").is_none(),
        "cache entry should expire"
    );
}

#[tokio::test]
async fn service_enforces_rate_limit_after_ten_uncached_queries() {
    let mut server = mockito::Server::new_async().await;
    let _rustsec_mock = server
        .mock("GET", Matcher::Regex(r"^/api/v1/crates/.*$".to_string()))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"vulnerabilities":[]}"#)
        .expect(10)
        .create();

    let service = ResearchService::with_base_urls_for_tests(
        format!("{}/api/v1/crates", server.url()),
        format!("{}/advisories", server.url()),
        format!("{}/rest/json/cves/2.0", server.url()),
        10,
        Duration::from_secs(60),
    )
    .expect("create service");

    for idx in 0..10 {
        let query = ResearchQuery::RustSecAdvisory {
            crate_name: format!("crate-{idx}"),
        };
        service
            .query(&query)
            .await
            .expect("query within rate limit");
    }

    let err = service
        .query(&ResearchQuery::RustSecAdvisory {
            crate_name: "crate-over-limit".to_string(),
        })
        .await
        .expect_err("11th call should fail");
    assert!(
        err.to_string().contains("rate limit"),
        "error should mention rate limit, got: {err}"
    );
}

#[tokio::test]
async fn rustsec_source_parses_findings_from_mock_response() {
    let mut server = mockito::Server::new_async().await;
    let _mock = server
        .mock("GET", "/api/v1/crates/openssl")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            r#"{
                "vulnerabilities": [
                    {
                        "id": "RUSTSEC-2020-0001",
                        "advisory": {
                            "title": "OpenSSL vuln",
                            "description": "A parsing issue",
                            "cvss": "8.8"
                        },
                        "versions": {
                            "patched": ">=0.10.30"
                        }
                    }
                ]
            }"#,
        )
        .create();

    let source = RustSecSource::with_base_url(format!("{}/api/v1/crates", server.url()))
        .expect("create rustsec source");
    let result = source.query("openssl").await.expect("query rustsec source");

    assert_eq!(result.findings.len(), 1);
    let finding = &result.findings[0];
    assert_eq!(finding.source, "RustSec");
    assert_eq!(finding.id, "RUSTSEC-2020-0001");
    assert_eq!(finding.title, "OpenSSL vuln");
    assert_eq!(finding.severity.as_deref(), Some("8.8"));
    assert!(
        finding
            .affected_versions
            .as_deref()
            .unwrap_or_default()
            .contains(">=0.10.30")
    );
}

#[tokio::test]
async fn github_source_parses_findings_from_mock_response() {
    let mut server = mockito::Server::new_async().await;
    let _mock = server
        .mock("GET", "/advisories?ecosystem=cargo&affects=openssl")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            r#"[
                {
                    "ghsa_id": "GHSA-xxxx-yyyy-zzzz",
                    "summary": "OpenSSL issue",
                    "description": "Potential vulnerability",
                    "severity": "high",
                    "html_url": "https://github.com/advisories/GHSA-xxxx-yyyy-zzzz"
                }
            ]"#,
        )
        .create();

    let source = GithubAdvisorySource::with_base_url(format!("{}/advisories", server.url()))
        .expect("create github source");
    let result = source.query("openssl").await.expect("query github source");

    assert_eq!(result.findings.len(), 1);
    let finding = &result.findings[0];
    assert_eq!(finding.source, "GitHub Advisory");
    assert_eq!(finding.id, "GHSA-xxxx-yyyy-zzzz");
    assert_eq!(finding.title, "OpenSSL issue");
    assert_eq!(finding.severity.as_deref(), Some("high"));
}

#[tokio::test]
async fn nvd_source_parses_findings_from_mock_response() {
    let mut server = mockito::Server::new_async().await;
    let _mock = server
        .mock("GET", "/rest/json/cves/2.0?keywordSearch=openssl")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            r#"{
                "vulnerabilities": [
                    {
                        "cve": {
                            "id": "CVE-2026-1234",
                            "descriptions": [
                                { "lang": "en", "value": "OpenSSL issue from NVD" }
                            ],
                            "metrics": {
                                "cvssMetricV31": [
                                    { "cvssData": { "baseScore": 9.1 } }
                                ]
                            },
                            "references": [
                                { "url": "https://nvd.nist.gov/vuln/detail/CVE-2026-1234" }
                            ]
                        }
                    }
                ]
            }"#,
        )
        .create();

    let source = NvdSource::with_base_url(format!("{}/rest/json/cves/2.0", server.url()))
        .expect("create nvd source");
    let result = source
        .query("openssl", None)
        .await
        .expect("query nvd source");

    assert_eq!(result.findings.len(), 1);
    let finding = &result.findings[0];
    assert_eq!(finding.source, "NVD");
    assert_eq!(finding.id, "CVE-2026-1234");
    assert_eq!(finding.title, "CVE-2026-1234");
    assert_eq!(finding.severity.as_deref(), Some("9.1"));
    assert!(finding.description.contains("OpenSSL issue from NVD"));
}

#[tokio::test]
#[ignore]
async fn integration_openssl_query_returns_non_empty_findings() {
    let service = ResearchService::new().expect("create research service");
    let result = service
        .query(&ResearchQuery::RustSecAdvisory {
            crate_name: "openssl".to_string(),
        })
        .await
        .expect("run openssl query");
    assert!(
        !result.findings.is_empty(),
        "expected known advisories for openssl"
    );
}

fn sample_result(query: &str) -> ResearchResult {
    ResearchResult {
        query: query.to_string(),
        findings: vec![ResearchFinding {
            source: "test".to_string(),
            id: "id-1".to_string(),
            title: "title".to_string(),
            description: "description".to_string(),
            severity: Some("low".to_string()),
            affected_versions: None,
            url: "https://rustsec.org/".to_string(),
            fetched_at: Utc::now(),
        }],
        source_url: "https://rustsec.org/".to_string(),
        cached: false,
        fetched_at: Utc::now(),
    }
}
