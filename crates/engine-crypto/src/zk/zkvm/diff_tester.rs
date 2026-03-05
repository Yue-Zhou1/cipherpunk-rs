use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use regex::Regex;
use sandbox::SandboxExecutor;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ZkvmBackend {
    Sp1,
    Risc0,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffTestRequest {
    pub backend: ZkvmBackend,
    pub boundary_input: String,
    pub native_output: String,
    pub zkvm_output: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffTestResult {
    pub backend: ZkvmBackend,
    pub boundary_input: String,
    pub native_output: String,
    pub zkvm_output: String,
    pub divergence_detected: bool,
    pub summary: String,
}

#[derive(Debug, Default, Clone)]
pub struct ZkvmDiffTester;

impl ZkvmDiffTester {
    pub fn new() -> Self {
        Self
    }

    pub fn with_sandbox(_sandbox: Arc<SandboxExecutor>) -> Self {
        // Hook for future sandbox-backed execution wiring.
        Self
    }

    pub fn without_sandbox_for_tests() -> Self {
        Self
    }

    pub async fn run(&self, req: DiffTestRequest) -> Result<DiffTestResult> {
        let native = req.native_output.trim().to_string();
        let zkvm = req.zkvm_output.trim().to_string();
        let divergence_detected = native != zkvm;

        let backend = match req.backend {
            ZkvmBackend::Sp1 => "SP1",
            ZkvmBackend::Risc0 => "RISC0",
        };

        let summary = if divergence_detected {
            format!(
                "{backend} divergence on boundary input {}: native output differs from zkVM output",
                req.boundary_input
            )
        } else {
            format!(
                "{backend} remained consistent on boundary input {}",
                req.boundary_input
            )
        };

        Ok(DiffTestResult {
            backend: req.backend,
            boundary_input: req.boundary_input,
            native_output: native,
            zkvm_output: zkvm,
            divergence_detected,
            summary,
        })
    }

    pub async fn verify_image_hash_binding(&self, guest_path: &Path) -> Result<bool> {
        let guest_bytes = std::fs::read(guest_path)
            .with_context(|| format!("failed to read guest image {}", guest_path.display()))?;
        let mut hasher = Sha256::new();
        hasher.update(&guest_bytes);
        let computed = hex::encode(hasher.finalize());

        let expected = read_expected_hash(guest_path)?;
        Ok(expected
            .map(|expected| expected.eq_ignore_ascii_case(&computed))
            .unwrap_or(false))
    }
}

fn read_expected_hash(guest_path: &Path) -> Result<Option<String>> {
    let sidecar = guest_path.with_extension("image_hash");
    if sidecar.exists() {
        let value = std::fs::read_to_string(&sidecar)
            .with_context(|| format!("failed to read {}", sidecar.display()))?;
        return Ok(Some(value.trim().to_string()));
    }

    let source = std::fs::read_to_string(guest_path).ok();
    let Some(source) = source else {
        return Ok(None);
    };

    let pattern = Regex::new(r"(?m)expected_image_hash\s*[:=]\s*([A-Fa-f0-9]{64})")
        .context("compile expected hash regex")?;
    Ok(pattern
        .captures(&source)
        .and_then(|caps| caps.get(1).map(|m| m.as_str().to_string())))
}
