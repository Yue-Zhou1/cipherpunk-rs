use std::fs;
use std::path::PathBuf;

use anyhow::Result;
use engine_crypto::zk::zkvm::diff_tester::{DiffTestRequest, ZkvmBackend, ZkvmDiffTester};
use sha2::{Digest, Sha256};

#[tokio::test]
async fn sp1_divergence_detected_when_native_and_zkvm_outputs_differ() -> Result<()> {
    let tester = ZkvmDiffTester::without_sandbox_for_tests();

    let result = tester
        .run(DiffTestRequest {
            backend: ZkvmBackend::Sp1,
            boundary_input: "u64::MAX".to_string(),
            native_output: "18446744073709551615".to_string(),
            zkvm_output: "0".to_string(),
        })
        .await?;

    assert!(result.divergence_detected);
    assert!(
        result
            .summary
            .contains("native output differs from zkVM output")
    );
    assert_eq!(result.boundary_input, "u64::MAX");
    Ok(())
}

#[tokio::test]
async fn verify_image_hash_binding_fails_on_mismatch() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let guest_path = dir.path().join("guest.bin");
    fs::write(&guest_path, b"guest-image-v1")?;

    let mut hasher = Sha256::new();
    hasher.update(b"different-image");
    let mismatch_hash = hex::encode(hasher.finalize());
    fs::write(
        PathBuf::from(&guest_path).with_extension("image_hash"),
        mismatch_hash,
    )?;

    let tester = ZkvmDiffTester::without_sandbox_for_tests();
    let ok = tester.verify_image_hash_binding(&guest_path).await?;
    assert!(!ok);

    Ok(())
}
