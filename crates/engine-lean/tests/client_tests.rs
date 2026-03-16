use engine_lean::client::AxleClient;
use engine_lean::types::{
    AxleCheckRequest, AxleDisproveRequest, AxleSorry2LemmaRequest, DEFAULT_LEAN_ENV,
};
use mockito::Server;
use std::sync::{Mutex, OnceLock};

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn check_response_ok() -> &'static str {
    r#"{"okay":true,"content":"theorem foo : 1 = 1 := rfl","lean_messages":{"errors":[],"warnings":[],"infos":[]},"tool_messages":{"errors":[],"warnings":[],"infos":[]},"failed_declarations":[]}"#
}

fn check_response_fail() -> &'static str {
    r#"{"okay":false,"content":"bad","lean_messages":{"errors":["unknown identifier 'x'"],"warnings":[],"infos":[]},"tool_messages":{"errors":[],"warnings":[],"infos":[]},"failed_declarations":["foo"]}"#
}

const DISPROVE_RESPONSE: &str = r#"{"content":"...","lean_messages":{"errors":[],"warnings":[],"infos":[]},"tool_messages":{"errors":[],"warnings":[],"infos":[]},"disproved_theorems":["foo"]}"#;
const SORRY2LEMMA_RESPONSE: &str = r#"{"content":"...","lean_messages":{"errors":[],"warnings":[],"infos":[]},"lemma_names":["subgoal_0","subgoal_1"]}"#;

#[tokio::test]
async fn check_with_key_sends_authorization_header() {
    let mut server = Server::new_async().await;
    let mock = server
        .mock("POST", "/check")
        .match_header(
            "authorization",
            mockito::Matcher::Regex("Bearer test-key".into()),
        )
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(check_response_ok())
        .create_async()
        .await;

    let client = AxleClient::new(server.url(), Some("test-key".to_string()));
    let result = client
        .check(&AxleCheckRequest {
            content: "theorem foo : 1 = 1 := rfl".to_string(),
            environment: DEFAULT_LEAN_ENV.to_string(),
            timeout_seconds: None,
        })
        .await
        .unwrap();

    assert!(result.okay);
    assert!(result.lean_messages.errors.is_empty());
    mock.assert_async().await;
}

#[tokio::test]
async fn check_without_key_omits_authorization_header() {
    let mut server = Server::new_async().await;
    let mock = server
        .mock("POST", "/check")
        .match_header("authorization", mockito::Matcher::Missing)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(check_response_ok())
        .create_async()
        .await;

    let client = AxleClient::new(server.url(), None);
    let result = client
        .check(&AxleCheckRequest {
            content: "theorem foo : 1 = 1 := rfl".to_string(),
            environment: DEFAULT_LEAN_ENV.to_string(),
            timeout_seconds: None,
        })
        .await
        .unwrap();

    assert!(result.okay);
    mock.assert_async().await;
}

#[tokio::test]
async fn check_returns_errors_when_lean_is_invalid() {
    let mut server = Server::new_async().await;
    server
        .mock("POST", "/check")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(check_response_fail())
        .create_async()
        .await;

    let client = AxleClient::new(server.url(), None);
    let result = client
        .check(&AxleCheckRequest {
            content: "bad lean code".to_string(),
            environment: DEFAULT_LEAN_ENV.to_string(),
            timeout_seconds: None,
        })
        .await
        .unwrap();

    assert!(!result.okay);
    assert!(!result.lean_messages.errors.is_empty());
}

#[tokio::test]
async fn disprove_returns_disproved_theorems() {
    let mut server = Server::new_async().await;
    server
        .mock("POST", "/disprove")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(DISPROVE_RESPONSE)
        .create_async()
        .await;

    let client = AxleClient::new(server.url(), None);
    let result = client
        .disprove(&AxleDisproveRequest {
            content: "theorem foo : False := sorry".to_string(),
            environment: DEFAULT_LEAN_ENV.to_string(),
            names: Some(vec!["foo".to_string()]),
            timeout_seconds: None,
        })
        .await
        .unwrap();

    assert_eq!(result.disproved_theorems, vec!["foo"]);
}

#[tokio::test]
async fn sorry2lemma_returns_extracted_lemma_names() {
    let mut server = Server::new_async().await;
    server
        .mock("POST", "/sorry2lemma")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(SORRY2LEMMA_RESPONSE)
        .create_async()
        .await;

    let client = AxleClient::new(server.url(), None);
    let result = client
        .sorry2lemma(&AxleSorry2LemmaRequest {
            content: "theorem foo : 1 = 1 := sorry".to_string(),
            environment: DEFAULT_LEAN_ENV.to_string(),
            extract_sorries: Some(true),
            extract_errors: Some(true),
            timeout_seconds: None,
        })
        .await
        .unwrap();

    assert_eq!(result.lemma_names, vec!["subgoal_0", "subgoal_1"]);
}

#[tokio::test]
async fn from_env_uses_key_when_set() {
    let _guard = env_lock().lock().expect("env lock");
    unsafe { std::env::set_var("AXLE_API_KEY", "env-key") };
    let client = AxleClient::from_env("http://localhost".to_string());
    assert!(client.has_api_key());

    unsafe { std::env::remove_var("AXLE_API_KEY") };
}

#[tokio::test]
async fn from_env_proceeds_without_key() {
    let _guard = env_lock().lock().expect("env lock");
    unsafe { std::env::remove_var("AXLE_API_KEY") };
    let client = AxleClient::from_env("http://localhost".to_string());
    assert!(!client.has_api_key());
}
