use std::path::PathBuf;

use worker_protocol::{
    SignedArtifactManifest, WorkerArtifactRef, WorkerExecutionRequest, WorkerExecutionResult,
    WorkerResourceUsage,
};

#[derive(Debug, Default, Clone)]
pub struct WorkerRunner;

impl WorkerRunner {
    pub async fn execute(request: WorkerExecutionRequest) -> WorkerExecutionResult {
        let primary_artifact = PathBuf::from("/artifacts/worker-report.json");
        WorkerExecutionResult {
            exit_code: 0,
            stdout: format!("remote worker executed request {}", request.request_id),
            stderr: String::new(),
            artifacts: vec![primary_artifact.clone()],
            container_digest: "sha256:remote-worker".to_string(),
            duration_ms: 1,
            resource_usage: WorkerResourceUsage {
                memory_bytes: Some(16 * 1024 * 1024),
                cpu_nanos: Some(500_000),
            },
            signed_manifest: SignedArtifactManifest {
                manifest_id: format!("manifest-{}", request.request_id),
                signature: "test-signature".to_string(),
                artifacts: vec![WorkerArtifactRef {
                    path: primary_artifact,
                    digest: "sha256:artifact".to_string(),
                }],
            },
        }
    }
}
