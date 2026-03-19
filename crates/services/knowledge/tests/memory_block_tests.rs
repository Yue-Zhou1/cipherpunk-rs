#![cfg(feature = "memory-block")]

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

use anyhow::Result;
use knowledge::KnowledgeBase;
use knowledge::memory_block::MemoryBlock;
use knowledge::memory_block::config::{DistanceMetric, ResolvedEmbeddingConfig};
use knowledge::memory_block::embedder::EmbeddingProvider;
use knowledge::memory_block::types::{
    ArtifactMetadata, Evidence, ExtractionMetadata, Invariants, Remediation, SignatureSource,
    VulnerabilityDetails, VulnerabilitySignature,
};
use tempfile::tempdir;

const HEADER_LEN: usize = 0x100;

#[test]
fn memory_block_loads_and_returns_top_k_matches() {
    let temp = tempdir().expect("tempdir");
    let artifact_path = temp.path().join("knowledge.bin");

    let config = test_embedding_config("test-mini-lm");
    let signatures = vec![
        signature(
            "SIG-1",
            "Nonce reuse in AEAD",
            "Counter resets cause nonce collisions",
            &["aead", "nonce"],
        ),
        signature(
            "SIG-2",
            "Missing signature check",
            "Path accepts unsigned packet",
            &["signature", "auth"],
        ),
        signature(
            "SIG-3",
            "State rollback on restart",
            "Restart resets cryptographic sequence state",
            &["nonce", "state"],
        ),
    ];
    let vectors = vec![
        l2_normalize(vec![1.0, 0.0, 0.0]),
        l2_normalize(vec![0.0, 1.0, 0.0]),
        l2_normalize(vec![0.8, 0.2, 0.0]),
    ];
    write_test_memory_block(&artifact_path, &config, vectors, signatures).expect("write artifact");

    let mut lookup = BTreeMap::new();
    lookup.insert("nonce query".to_string(), l2_normalize(vec![1.0, 0.0, 0.0]));
    let embedder = FixedEmbedder::new(config.clone(), lookup);

    let block =
        MemoryBlock::load(&artifact_path, &config, Box::new(embedder)).expect("load memory block");
    let results = block.search("nonce query", 2).expect("search memory block");

    assert_eq!(results.len(), 2);
    assert_eq!(results[0].signature.id, "SIG-1");
    assert_eq!(results[1].signature.id, "SIG-3");
    assert!(results[0].score >= results[1].score);
}

#[test]
fn memory_block_load_rejects_embedding_config_mismatch() {
    let temp = tempdir().expect("tempdir");
    let artifact_path = temp.path().join("knowledge.bin");

    let artifact_config = test_embedding_config("artifact-model");
    let signatures = vec![signature(
        "SIG-1",
        "Nonce reuse in AEAD",
        "Counter resets cause nonce collisions",
        &["aead", "nonce"],
    )];
    write_test_memory_block(
        &artifact_path,
        &artifact_config,
        vec![l2_normalize(vec![1.0, 0.0, 0.0])],
        signatures,
    )
    .expect("write artifact");

    let runtime_config = test_embedding_config("runtime-model");
    let mut lookup = BTreeMap::new();
    lookup.insert("nonce query".to_string(), l2_normalize(vec![1.0, 0.0, 0.0]));
    let embedder = FixedEmbedder::new(runtime_config.clone(), lookup);

    let err = MemoryBlock::load(&artifact_path, &runtime_config, Box::new(embedder))
        .expect_err("config mismatch should fail");
    assert!(
        err.to_string().contains("embedding configuration mismatch"),
        "unexpected error: {err:#}"
    );
}

#[test]
fn knowledge_base_similar_cases_are_augmented_by_semantic_memory() {
    let temp = tempdir().expect("tempdir");
    let artifact_path = temp.path().join("knowledge.bin");

    let config = test_embedding_config("test-mini-lm");
    let signatures = vec![signature(
        "MEM-1",
        "Nonce uniqueness invariant",
        "Distinct encryption calls should never reuse nonces",
        &["nonce", "aead"],
    )];
    write_test_memory_block(
        &artifact_path,
        &config,
        vec![l2_normalize(vec![1.0, 0.0, 0.0])],
        signatures,
    )
    .expect("write artifact");

    let mut lookup = BTreeMap::new();
    lookup.insert(
        "nonce uniqueness".to_string(),
        l2_normalize(vec![1.0, 0.0, 0.0]),
    );
    let embedder = FixedEmbedder::new(config.clone(), lookup);
    let memory_block =
        MemoryBlock::load(&artifact_path, &config, Box::new(embedder)).expect("load memory block");

    let mut kb = KnowledgeBase::load_from_repo_root().expect("load knowledge base");
    kb.attach_memory_block(memory_block);

    let similar = kb.similar_cases(&["nonce".to_string(), "uniqueness".to_string()], 3);
    assert!(
        similar.iter().any(|case| case.id == "MEM-1"),
        "semantic memory should augment similar-case results"
    );
}

#[test]
fn memory_block_load_rejects_wrong_magic() {
    let temp = tempdir().expect("tempdir");
    let artifact_path = temp.path().join("knowledge.bin");
    let config = test_embedding_config("test-mini-lm");
    write_test_memory_block(
        &artifact_path,
        &config,
        vec![l2_normalize(vec![1.0, 0.0, 0.0])],
        vec![signature(
            "SIG-1",
            "Nonce reuse in AEAD",
            "Counter resets cause nonce collisions",
            &["aead", "nonce"],
        )],
    )
    .expect("write artifact");

    overwrite_bytes(&artifact_path, 0, b"BAD!").expect("corrupt magic");

    let mut lookup = BTreeMap::new();
    lookup.insert("nonce query".to_string(), l2_normalize(vec![1.0, 0.0, 0.0]));
    let embedder = FixedEmbedder::new(config.clone(), lookup);
    let err = MemoryBlock::load(&artifact_path, &config, Box::new(embedder))
        .expect_err("wrong magic should fail");
    assert!(
        err.to_string().contains("invalid memory-block magic"),
        "unexpected error: {err:#}"
    );
}

#[test]
fn memory_block_load_rejects_metadata_offset_before_header() {
    let temp = tempdir().expect("tempdir");
    let artifact_path = temp.path().join("knowledge.bin");
    let config = test_embedding_config("test-mini-lm");
    write_test_memory_block(
        &artifact_path,
        &config,
        vec![l2_normalize(vec![1.0, 0.0, 0.0])],
        vec![signature(
            "SIG-1",
            "Nonce reuse in AEAD",
            "Counter resets cause nonce collisions",
            &["aead", "nonce"],
        )],
    )
    .expect("write artifact");

    overwrite_u64_le(&artifact_path, 0x80, 32).expect("set metadata offset before header");

    let mut lookup = BTreeMap::new();
    lookup.insert("nonce query".to_string(), l2_normalize(vec![1.0, 0.0, 0.0]));
    let embedder = FixedEmbedder::new(config.clone(), lookup);
    let err = MemoryBlock::load(&artifact_path, &config, Box::new(embedder))
        .expect_err("metadata offset before header should fail");
    assert!(
        err.to_string().contains("invalid metadata offset"),
        "unexpected error: {err:#}"
    );
}

#[test]
fn memory_block_load_rejects_overlapping_vector_and_metadata_sections() {
    let temp = tempdir().expect("tempdir");
    let artifact_path = temp.path().join("knowledge.bin");
    let config = test_embedding_config("test-mini-lm");
    write_test_memory_block(
        &artifact_path,
        &config,
        vec![
            l2_normalize(vec![1.0, 0.0, 0.0]),
            l2_normalize(vec![0.0, 1.0, 0.0]),
        ],
        vec![
            signature(
                "SIG-1",
                "Nonce reuse in AEAD",
                "Counter resets cause nonce collisions",
                &["aead", "nonce"],
            ),
            signature(
                "SIG-2",
                "Missing signature check",
                "Path accepts unsigned packet",
                &["signature", "auth"],
            ),
        ],
    )
    .expect("write artifact");

    // Force metadata to start inside the vector range.
    overwrite_u64_le(&artifact_path, 0x80, HEADER_LEN as u64 + 4)
        .expect("set overlapping metadata offset");

    let mut lookup = BTreeMap::new();
    lookup.insert("nonce query".to_string(), l2_normalize(vec![1.0, 0.0, 0.0]));
    let embedder = FixedEmbedder::new(config.clone(), lookup);
    let err = MemoryBlock::load(&artifact_path, &config, Box::new(embedder))
        .expect_err("overlapping sections should fail");
    assert!(
        err.to_string().contains("overlap"),
        "unexpected error: {err:#}"
    );
}

#[derive(Debug, Clone)]
struct FixedEmbedder {
    config: ResolvedEmbeddingConfig,
    vectors: BTreeMap<String, Vec<f32>>,
}

impl FixedEmbedder {
    fn new(config: ResolvedEmbeddingConfig, vectors: BTreeMap<String, Vec<f32>>) -> Self {
        Self { config, vectors }
    }
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

fn test_embedding_config(model: &str) -> ResolvedEmbeddingConfig {
    ResolvedEmbeddingConfig {
        provider: "onnx".to_string(),
        model: model.to_string(),
        dimensions: 3,
        distance_metric: DistanceMetric::Cosine,
        l2_normalized: true,
        embedding_text_version: "v1".to_string(),
    }
}

fn signature(id: &str, title: &str, description: &str, tags: &[&str]) -> VulnerabilitySignature {
    VulnerabilitySignature {
        id: id.to_string(),
        source: SignatureSource {
            report: "Synthetic report".to_string(),
            pdf_path: "reports/synthetic.pdf".to_string(),
            page_range: [1, 2],
            kind: "audit_report".to_string(),
        },
        vulnerability: VulnerabilityDetails {
            title: title.to_string(),
            severity: "high".to_string(),
            category: "crypto".to_string(),
            description: description.to_string(),
            vulnerable_pattern: "counter += 1".to_string(),
            root_cause: "state reset".to_string(),
        },
        remediation: Remediation {
            description: "Persist nonce state".to_string(),
            code_pattern: "derive_nonce(session, counter)".to_string(),
        },
        invariants: Invariants {
            natural_language: "Distinct calls must use distinct nonces".to_string(),
            kani_hint: Some("kani::assume(a != b);".to_string()),
        },
        evidence: Evidence {
            excerpt: "Counter resets on startup".to_string(),
            section_title: Some("Finding 1".to_string()),
        },
        extraction: ExtractionMetadata {
            confidence: "high".to_string(),
            review_status: "reviewed".to_string(),
            embedding_text_version: "v1".to_string(),
        },
        tags: tags.iter().map(|value| value.to_string()).collect(),
        embedding_text: format!("{title}. {description}"),
    }
}

fn l2_normalize(values: Vec<f32>) -> Vec<f32> {
    let norm = values.iter().map(|value| value * value).sum::<f32>().sqrt();
    if norm <= f32::EPSILON {
        return values;
    }
    values.into_iter().map(|value| value / norm).collect()
}

fn write_test_memory_block(
    path: &Path,
    config: &ResolvedEmbeddingConfig,
    vectors: Vec<Vec<f32>>,
    signatures: Vec<VulnerabilitySignature>,
) -> Result<()> {
    assert_eq!(vectors.len(), signatures.len());
    for vector in &vectors {
        assert_eq!(vector.len(), config.dimensions as usize);
    }

    let metadata = ArtifactMetadata {
        schema_version: 1,
        generated_at: "2026-03-19T00:00:00Z".to_string(),
        embedding: config.clone(),
        signatures,
    };
    let metadata_blob = rmp_serde::to_vec_named(&metadata)?;
    let vector_blob = vectors_to_le_bytes(&vectors);

    let vector_offset = HEADER_LEN as u64;
    let vector_size = vector_blob.len() as u64;
    let metadata_offset = vector_offset + vector_size;
    let metadata_size = metadata_blob.len() as u64;

    let mut header = vec![0u8; HEADER_LEN];
    header[0..4].copy_from_slice(b"CPKN");
    header[4..8].copy_from_slice(&1u32.to_le_bytes());
    write_padded_ascii(&mut header[0x08..0x28], &config.provider);
    write_padded_ascii(&mut header[0x28..0x68], &config.model);
    header[0x68..0x6C].copy_from_slice(&config.dimensions.to_le_bytes());
    header[0x6C..0x70].copy_from_slice(&(vectors.len() as u32).to_le_bytes());
    header[0x70..0x78].copy_from_slice(&vector_offset.to_le_bytes());
    header[0x78..0x80].copy_from_slice(&vector_size.to_le_bytes());
    header[0x80..0x88].copy_from_slice(&metadata_offset.to_le_bytes());
    header[0x88..0x90].copy_from_slice(&metadata_size.to_le_bytes());

    let mut file = File::create(path)?;
    file.write_all(&header)?;
    file.write_all(&vector_blob)?;
    file.write_all(&metadata_blob)?;
    Ok(())
}

fn vectors_to_le_bytes(vectors: &[Vec<f32>]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(vectors.iter().map(Vec::len).sum::<usize>() * 4);
    for vector in vectors {
        for value in vector {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
    }
    bytes
}

fn write_padded_ascii(dst: &mut [u8], input: &str) {
    let bytes = input.as_bytes();
    let len = bytes.len().min(dst.len());
    dst[..len].copy_from_slice(&bytes[..len]);
}

fn overwrite_u64_le(path: &Path, offset: usize, value: u64) -> Result<()> {
    overwrite_bytes(path, offset, &value.to_le_bytes())
}

fn overwrite_bytes(path: &Path, offset: usize, bytes: &[u8]) -> Result<()> {
    let mut raw = fs::read(path)?;
    let end = offset + bytes.len();
    raw[offset..end].copy_from_slice(bytes);
    fs::write(path, raw)?;
    Ok(())
}
