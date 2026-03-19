#![cfg(feature = "memory-block")]

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::Result;
use knowledge::memory_block::MemoryBlock;
use knowledge::memory_block::config::{DistanceMetric, ResolvedEmbeddingConfig};
use knowledge::memory_block::embedder::EmbeddingProvider;

#[test]
fn python_written_bundle_is_readable_by_rust_loader() {
    let artifact_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/python_writer_knowledge.bin");
    assert!(
        artifact_path.exists(),
        "missing test fixture at {}",
        artifact_path.display()
    );

    let config = ResolvedEmbeddingConfig {
        provider: "onnx".to_string(),
        model: "all-MiniLM-L6-v2".to_string(),
        dimensions: 3,
        distance_metric: DistanceMetric::Cosine,
        l2_normalized: true,
        embedding_text_version: "v1".to_string(),
    };

    let mut vectors = BTreeMap::new();
    vectors.insert("nonce query".to_string(), vec![1.0, 0.0, 0.0]);
    let embedder = FixedEmbedder {
        config: config.clone(),
        vectors,
    };

    let block =
        MemoryBlock::load(&artifact_path, &config, Box::new(embedder)).expect("load fixture");
    let results = block.search("nonce query", 2).expect("search fixture");
    assert!(!results.is_empty(), "expected at least one search result");
    assert_eq!(results[0].signature.id, "PY-SIG-1");
}

#[derive(Debug, Clone)]
struct FixedEmbedder {
    config: ResolvedEmbeddingConfig,
    vectors: BTreeMap<String, Vec<f32>>,
}

impl EmbeddingProvider for FixedEmbedder {
    fn embed(&self, text: &str) -> Result<Vec<f32>> {
        self.vectors
            .get(text)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("missing vector for query `{text}`"))
    }

    fn config(&self) -> &ResolvedEmbeddingConfig {
        &self.config
    }
}
