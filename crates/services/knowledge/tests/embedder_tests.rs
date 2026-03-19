#![cfg(feature = "memory-block")]

use knowledge::memory_block::config::{DistanceMetric, ResolvedEmbeddingConfig};
use knowledge::memory_block::embedder::{
    EmbeddingProvider, HttpEmbedder, provider_from_resolved_config,
};
use mockito::{Matcher, Server};

#[test]
fn http_embedder_calls_openai_compatible_endpoint_and_parses_vector() {
    let mut server = Server::new();
    let _mock = server
        .mock("POST", "/v1/embeddings")
        .match_header(
            "content-type",
            Matcher::Regex("application/json".to_string()),
        )
        .match_body(Matcher::Regex(
            r#""model"\s*:\s*"text-embedding-test""#.to_string(),
        ))
        .match_body(Matcher::Regex(
            r#""input"\s*:\s*"nonce uniqueness""#.to_string(),
        ))
        .match_body(Matcher::Regex(
            r#""encoding_format"\s*:\s*"float""#.to_string(),
        ))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            r#"{
                "object": "list",
                "data": [
                    {"object": "embedding", "index": 0, "embedding": [1.0, 0.0, 0.0]}
                ],
                "model": "text-embedding-test"
            }"#,
        )
        .create();

    let config = test_config("http", "text-embedding-test", 3);
    let embedder = HttpEmbedder::new(config, server.url(), None).expect("build http embedder");
    let vector = embedder
        .embed("nonce uniqueness")
        .expect("embed query text");
    assert_eq!(vector, vec![1.0, 0.0, 0.0]);
}

#[test]
fn http_embedder_rejects_dimension_mismatch() {
    let mut server = Server::new();
    let _mock = server
        .mock("POST", "/v1/embeddings")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            r#"{
                "data": [
                    {"embedding": [0.5, 0.5]}
                ]
            }"#,
        )
        .create();

    let config = test_config("http", "text-embedding-test", 3);
    let embedder = HttpEmbedder::new(config, server.url(), None).expect("build http embedder");
    let err = embedder
        .embed("nonce uniqueness")
        .expect_err("dimension mismatch should fail");
    assert!(
        err.to_string().contains("embedding dimension mismatch"),
        "unexpected error: {err:#}"
    );
}

#[test]
fn provider_resolution_rejects_unwired_onnx() {
    let err = match provider_from_resolved_config(test_config("onnx", "all-MiniLM-L6-v2", 384)) {
        Ok(_) => panic!("onnx should be rejected until wired"),
        Err(err) => err,
    };
    assert!(
        err.to_string()
            .contains("ONNX embedding provider is not wired yet"),
        "unexpected error: {err:#}"
    );
}

fn test_config(provider: &str, model: &str, dimensions: u32) -> ResolvedEmbeddingConfig {
    ResolvedEmbeddingConfig {
        provider: provider.to_string(),
        model: model.to_string(),
        dimensions,
        distance_metric: DistanceMetric::Cosine,
        l2_normalized: true,
        embedding_text_version: "v1".to_string(),
    }
}
