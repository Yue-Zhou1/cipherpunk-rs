use std::fs;
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result, bail};

pub mod config;
pub mod embedder;
pub mod format;
pub mod schema;
pub mod search;
pub mod types;

use config::{DistanceMetric, ResolvedEmbeddingConfig};
use embedder::EmbeddingProvider;
use format::BlockHeader;
use search::top_k_cosine;
use types::{ArtifactMetadata, VulnerabilitySignature};

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub signature: VulnerabilitySignature,
    pub score: f32,
}

pub struct MemoryBlock {
    pub(crate) header: BlockHeader,
    vector_data: Vec<f32>,
    metadata: ArtifactMetadata,
    embedder: Arc<dyn EmbeddingProvider>,
}

impl std::fmt::Debug for MemoryBlock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MemoryBlock")
            .field("header", &self.header)
            .field("vector_count", &self.header.signature_count)
            .field("metadata", &self.metadata)
            .finish()
    }
}

impl MemoryBlock {
    pub fn load(
        path: &Path,
        runtime_config: &ResolvedEmbeddingConfig,
        embedder: Box<dyn EmbeddingProvider>,
    ) -> Result<Self> {
        let bytes =
            fs::read(path).with_context(|| format!("read memory block {}", path.display()))?;
        if bytes.len() < format::HEADER_LEN {
            bail!(
                "memory block file too small: expected at least {} bytes, got {}",
                format::HEADER_LEN,
                bytes.len()
            );
        }

        let header = BlockHeader::parse(&bytes[..format::HEADER_LEN])?;
        header.validate_layout(bytes.len())?;

        let metadata_start = usize::try_from(header.metadata_offset)
            .context("metadata offset does not fit in usize")?;
        let metadata_end = metadata_start
            .checked_add(
                usize::try_from(header.metadata_size)
                    .context("metadata size does not fit in usize")?,
            )
            .context("metadata range overflows usize")?;
        let metadata =
            rmp_serde::from_slice::<ArtifactMetadata>(&bytes[metadata_start..metadata_end])
                .context("decode metadata blob from memory block")?;

        validate_embedding_config(&header, &metadata, runtime_config, embedder.config())?;
        validate_vector_dimensions(&header, &metadata)?;

        let vector_start =
            usize::try_from(header.vector_offset).context("vector offset does not fit in usize")?;
        let vector_end = vector_start
            .checked_add(
                usize::try_from(header.vector_size).context("vector size does not fit in usize")?,
            )
            .context("vector range overflows usize")?;
        let vector_data = parse_vector_blob(&bytes[vector_start..vector_end])?;

        let expected = header.signature_count as usize * header.dimensions as usize;
        if vector_data.len() != expected {
            bail!(
                "vector payload length mismatch: expected {expected} floats, got {}",
                vector_data.len()
            );
        }

        Ok(Self {
            header,
            vector_data,
            metadata,
            embedder: Arc::from(embedder),
        })
    }

    pub fn search(&self, query_text: &str, k: usize) -> Result<Vec<SearchResult>> {
        if k == 0 || query_text.trim().is_empty() {
            return Ok(vec![]);
        }

        if self.metadata.embedding.distance_metric != DistanceMetric::Cosine {
            bail!(
                "unsupported distance metric {:?} for v1 search",
                self.metadata.embedding.distance_metric
            );
        }

        let mut query = self.embedder.embed(query_text)?;
        if query.len() != self.header.dimensions as usize {
            bail!(
                "query embedding dimension mismatch: expected {}, got {}",
                self.header.dimensions,
                query.len()
            );
        }

        if self.metadata.embedding.l2_normalized {
            l2_normalize_in_place(&mut query);
        }

        let ranked = top_k_cosine(
            &self.vector_data,
            self.header.dimensions as usize,
            &query,
            k,
        );
        Ok(ranked
            .into_iter()
            .filter_map(|(index, score)| {
                self.metadata
                    .signatures
                    .get(index)
                    .cloned()
                    .map(|signature| SearchResult { signature, score })
            })
            .collect())
    }

    pub fn metadata(&self) -> &ArtifactMetadata {
        &self.metadata
    }
}

fn validate_embedding_config(
    header: &BlockHeader,
    metadata: &ArtifactMetadata,
    runtime_config: &ResolvedEmbeddingConfig,
    embedder_config: &ResolvedEmbeddingConfig,
) -> Result<()> {
    let artifact_config = &metadata.embedding;
    if artifact_config != runtime_config
        || runtime_config != embedder_config
        || header.provider != runtime_config.provider
        || header.model != runtime_config.model
        || header.dimensions != runtime_config.dimensions
    {
        bail!(
            "embedding configuration mismatch: artifact={:?}, runtime={:?}, embedder={:?}, header=(provider={}, model={}, dims={})",
            artifact_config,
            runtime_config,
            embedder_config,
            header.provider,
            header.model,
            header.dimensions
        );
    }

    Ok(())
}

fn validate_vector_dimensions(header: &BlockHeader, metadata: &ArtifactMetadata) -> Result<()> {
    if header.signature_count as usize != metadata.signatures.len() {
        bail!(
            "signature count mismatch: header={}, metadata={}",
            header.signature_count,
            metadata.signatures.len()
        );
    }
    if header.signature_count == 0 {
        bail!("memory block corpus is empty");
    }
    if header.dimensions == 0 {
        bail!("embedding dimensions must be greater than zero");
    }

    Ok(())
}

fn parse_vector_blob(bytes: &[u8]) -> Result<Vec<f32>> {
    if bytes.len() % 4 != 0 {
        bail!(
            "vector blob byte length must be a multiple of 4, got {}",
            bytes.len()
        );
    }

    Ok(bytes
        .chunks_exact(4)
        .map(|raw| f32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]))
        .collect())
}

fn l2_normalize_in_place(values: &mut [f32]) {
    let norm = values.iter().map(|value| value * value).sum::<f32>().sqrt();
    if norm <= f32::EPSILON {
        return;
    }
    for value in values {
        *value /= norm;
    }
}
