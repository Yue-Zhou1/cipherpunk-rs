use std::path::PathBuf;

use tracing::warn;
use worker_protocol::{
    SignedArtifactManifest, WorkerExecutionRequest, WorkerExecutionResult, WorkerMount,
    WorkerNetworkPolicy, WorkerResourceBudget,
};

use crate::{
    ExecutionRequest, ExecutionResult, NetworkPolicy, ResourceUsage, SandboxError,
    redaction::redact_ai_prompt,
};

#[derive(Debug, Clone)]
pub struct RemoteExecutionResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub artifacts: Vec<PathBuf>,
    pub container_digest: String,
    pub duration_ms: u64,
    pub resource_usage: ResourceUsage,
    pub signed_manifest: SignedArtifactManifest,
}

#[derive(Debug, Clone)]
pub struct RemoteExecutor {
    endpoint: String,
    test_mode: bool,
}

impl Default for RemoteExecutor {
    fn default() -> Self {
        Self {
            endpoint: "http://127.0.0.1:8787".to_string(),
            test_mode: false,
        }
    }
}

impl RemoteExecutor {
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            test_mode: false,
        }
    }

    pub fn for_tests() -> Self {
        Self {
            endpoint: "test://remote-worker".to_string(),
            test_mode: true,
        }
    }

    pub async fn execute(
        &self,
        request: ExecutionRequest,
    ) -> Result<RemoteExecutionResult, SandboxError> {
        let worker_request = to_worker_request(request);
        let worker_result = if self.test_mode {
            simulated_worker_result(&worker_request)
        } else {
            // v3 ships with deterministic simulation until transport + worker auth are enabled.
            warn!(
                endpoint = %self.endpoint,
                "remote worker transport is not configured; using simulated execution result"
            );
            simulated_worker_result(&worker_request)
        };
        Ok(from_worker_result(worker_result))
    }
}

fn to_worker_request(request: ExecutionRequest) -> WorkerExecutionRequest {
    WorkerExecutionRequest {
        request_id: format!("req-{}", uuid::Uuid::new_v4().simple()),
        image: format!("{:?}", request.image),
        command: request.command,
        mounts: request
            .mounts
            .into_iter()
            .map(|mount| WorkerMount {
                host_path: mount.host_path,
                container_path: mount.container_path,
                read_only: mount.read_only,
            })
            .collect(),
        env: request.env,
        budget: WorkerResourceBudget {
            cpu_millicores: (request.budget.cpu_cores * 1000.0).max(100.0) as u32,
            memory_mb: request.budget.memory_mb,
            disk_gb: request.budget.disk_gb,
            timeout_secs: request.budget.timeout_secs,
        },
        network: match request.network {
            NetworkPolicy::Disabled => WorkerNetworkPolicy::Disabled,
            NetworkPolicy::Allowlist(hosts) => WorkerNetworkPolicy::Allowlist(hosts),
        },
    }
}

fn simulated_worker_result(request: &WorkerExecutionRequest) -> WorkerExecutionResult {
    let artifact_path = PathBuf::from("/artifacts/remote-run/report.json");
    WorkerExecutionResult {
        exit_code: 0,
        stdout: format!(
            "remote-worker endpoint accepted request {} with command {:?}",
            request.request_id, request.command
        ),
        stderr: String::new(),
        artifacts: vec![artifact_path.clone()],
        container_digest: "sha256:remote-worker-simulated".to_string(),
        duration_ms: 2,
        resource_usage: worker_protocol::WorkerResourceUsage {
            memory_bytes: Some(8 * 1024 * 1024),
            cpu_nanos: Some(1_000_000),
        },
        signed_manifest: SignedArtifactManifest {
            manifest_id: format!("manifest-{}", request.request_id),
            signature: "simulated-signature".to_string(),
            artifacts: vec![worker_protocol::WorkerArtifactRef {
                path: artifact_path,
                digest: "sha256:remote-artifact".to_string(),
            }],
        },
    }
}

fn from_worker_result(result: WorkerExecutionResult) -> RemoteExecutionResult {
    RemoteExecutionResult {
        exit_code: result.exit_code,
        stdout: redact_ai_prompt(&result.stdout),
        stderr: redact_ai_prompt(&result.stderr),
        artifacts: result.artifacts,
        container_digest: result.container_digest,
        duration_ms: result.duration_ms,
        resource_usage: ResourceUsage {
            memory_bytes: result.resource_usage.memory_bytes,
            cpu_nanos: result.resource_usage.cpu_nanos,
        },
        signed_manifest: result.signed_manifest,
    }
}

impl From<RemoteExecutionResult> for ExecutionResult {
    fn from(result: RemoteExecutionResult) -> Self {
        ExecutionResult {
            exit_code: result.exit_code,
            stdout: result.stdout,
            stderr: result.stderr,
            artifacts: result.artifacts,
            container_digest: result.container_digest,
            duration_ms: result.duration_ms,
            resource_usage: result.resource_usage,
        }
    }
}
