use std::collections::HashMap;

use sandbox::{
    ExecutionRequest, NetworkPolicy, ResourceBudget, SandboxError, SandboxExecutor, ToolImage,
};

fn base_budget(timeout_secs: u64, memory_mb: u64) -> ResourceBudget {
    ResourceBudget {
        cpu_cores: 1.0,
        memory_mb,
        disk_gb: 1,
        timeout_secs,
    }
}

fn request(image: &str, cmd: &[&str], timeout_secs: u64, memory_mb: u64) -> ExecutionRequest {
    ExecutionRequest {
        image: ToolImage::Custom(image.to_string()),
        command: cmd.iter().map(|s| s.to_string()).collect(),
        mounts: vec![],
        env: HashMap::new(),
        budget: base_budget(timeout_secs, memory_mb),
        network: NetworkPolicy::Disabled,
    }
}

fn docker_inspect_id(image: &str) -> String {
    let output = std::process::Command::new("docker")
        .args(["inspect", "--format", "{{.Id}}", image])
        .output()
        .expect("run docker inspect");
    assert!(output.status.success(), "docker inspect should succeed");
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

#[tokio::test]
async fn captures_stdout_for_echo_hello() {
    let executor = SandboxExecutor::new().expect("docker client");
    let result = executor
        .execute(request(
            "busybox:1.36",
            &["sh", "-lc", "echo hello"],
            15,
            128,
        ))
        .await
        .expect("execution should succeed");

    assert_eq!(result.exit_code, 0);
    assert_eq!(result.stdout.trim(), "hello");
}

#[tokio::test]
async fn timeout_kills_container() {
    let executor = SandboxExecutor::new().expect("docker client");
    let err = executor
        .execute(request("busybox:1.36", &["sh", "-lc", "sleep 5"], 1, 128))
        .await
        .expect_err("timeout expected");

    assert!(matches!(err, SandboxError::Timeout));
}

#[tokio::test]
async fn oom_returns_oom_killed() {
    let executor = SandboxExecutor::new().expect("docker client");
    let err = executor
        .execute(request(
            "python:3.12-alpine",
            &[
                "python",
                "-c",
                "x=[]\nwhile True:\n x.append('x'*1024*1024)\n",
            ],
            30,
            32,
        ))
        .await
        .expect_err("oom expected");

    assert!(matches!(err, SandboxError::OomKilled));
}

#[tokio::test]
async fn captures_image_digest_matching_docker_inspect() {
    let image = "busybox:1.36";
    let executor = SandboxExecutor::new().expect("docker client");
    let result = executor
        .execute(request(image, &["sh", "-lc", "echo ok"], 15, 128))
        .await
        .expect("execution should succeed");

    let expected = docker_inspect_id(image);
    assert_eq!(result.container_digest, expected);
}

#[tokio::test]
async fn network_disabled_blocks_external_curl() {
    let executor = SandboxExecutor::new().expect("docker client");
    let result = executor
        .execute(request(
            "curlimages/curl:8.10.1",
            &["sh", "-lc", "curl -sS --max-time 5 https://example.com"],
            20,
            128,
        ))
        .await
        .expect("container should run");

    assert_ne!(result.exit_code, 0, "network-disabled curl should fail");
}
