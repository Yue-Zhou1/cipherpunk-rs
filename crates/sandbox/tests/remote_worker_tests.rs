use std::collections::HashMap;

use sandbox::{ExecutionRequest, NetworkPolicy, ResourceBudget, ToolImage};

fn sample_remote_request() -> ExecutionRequest {
    ExecutionRequest {
        image: ToolImage::Kani,
        command: vec!["kani".to_string(), "--version".to_string()],
        mounts: vec![],
        env: HashMap::new(),
        budget: ResourceBudget {
            cpu_cores: 1.0,
            memory_mb: 1024,
            disk_gb: 2,
            timeout_secs: 60,
        },
        network: NetworkPolicy::Allowlist(vec!["github.com".to_string()]),
    }
}

#[tokio::test]
async fn remote_worker_executes_job_and_returns_signed_artifact_manifest() {
    let runner = sandbox::remote::RemoteExecutor::for_tests();
    let result = runner.execute(sample_remote_request()).await.unwrap();
    assert!(!result.container_digest.is_empty());
    assert!(!result.artifacts.is_empty());
    assert!(!result.signed_manifest.signature.is_empty());
}
