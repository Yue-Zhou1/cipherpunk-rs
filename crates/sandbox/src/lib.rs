use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use bollard::Docker;
use bollard::container::{
    Config as ContainerConfig, CreateContainerOptions, KillContainerOptions, LogsOptions,
    RemoveContainerOptions, StartContainerOptions, WaitContainerOptions,
};
use bollard::errors::Error as BollardError;
use bollard::image::CreateImageOptions;
use bollard::models::{HostConfig, Mount as DockerMount, MountTypeEnum};
use futures_util::StreamExt;
use futures_util::stream::TryStreamExt;
use thiserror::Error;
use uuid::Uuid;

pub struct SandboxExecutor {
    docker: Docker,
    image_registry: ImageRegistry,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolImage {
    Kani,
    Z3,
    Miri,
    MadSim,
    Fuzz,
    Custom(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mount {
    pub host_path: PathBuf,
    pub container_path: PathBuf,
    pub read_only: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExecutionRequest {
    pub image: ToolImage,
    pub command: Vec<String>,
    pub mounts: Vec<Mount>,
    pub env: HashMap<String, String>,
    pub budget: ResourceBudget,
    pub network: NetworkPolicy,
}

#[derive(Debug, Clone, PartialEq)]
pub enum NetworkPolicy {
    Disabled,
    Allowlist(Vec<String>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExecutionResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub artifacts: Vec<PathBuf>,
    pub container_digest: String,
    pub duration_ms: u64,
    pub resource_usage: ResourceUsage,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ResourceUsage {
    pub memory_bytes: Option<u64>,
    pub cpu_nanos: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResourceBudget {
    pub cpu_cores: f64,
    pub memory_mb: u64,
    pub disk_gb: u64,
    pub timeout_secs: u64,
}

#[derive(Debug, Error)]
pub enum SandboxError {
    #[error("docker error: {0}")]
    Docker(#[from] BollardError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("sandbox timeout")]
    Timeout,
    #[error("sandbox process OOM-killed")]
    OomKilled,
    #[error("network allowlist mode is not yet implemented")]
    AllowlistNotImplemented,
}

impl SandboxExecutor {
    pub fn new() -> Result<Self, SandboxError> {
        let docker = Docker::connect_with_local_defaults()?;
        Ok(Self {
            docker,
            image_registry: ImageRegistry::default(),
        })
    }

    pub fn with_registry(image_registry: ImageRegistry) -> Result<Self, SandboxError> {
        let docker = Docker::connect_with_local_defaults()?;
        Ok(Self {
            docker,
            image_registry,
        })
    }

    pub async fn execute(
        &self,
        request: ExecutionRequest,
    ) -> Result<ExecutionResult, SandboxError> {
        let image = self.image_registry.resolve(&request.image);
        self.ensure_image(&image).await?;

        if matches!(request.network, NetworkPolicy::Allowlist(_)) {
            return Err(SandboxError::AllowlistNotImplemented);
        }

        let container_name = format!("audit-sandbox-{}", Uuid::new_v4().simple());
        let host_config = HostConfig {
            mounts: Some(
                request
                    .mounts
                    .iter()
                    .map(|mount| DockerMount {
                        target: Some(mount.container_path.to_string_lossy().to_string()),
                        source: Some(mount.host_path.to_string_lossy().to_string()),
                        typ: Some(MountTypeEnum::BIND),
                        read_only: Some(mount.read_only),
                        ..Default::default()
                    })
                    .collect(),
            ),
            memory: Some((request.budget.memory_mb * 1024 * 1024) as i64),
            nano_cpus: Some((request.budget.cpu_cores * 1_000_000_000.0) as i64),
            network_mode: Some("none".to_string()),
            ..Default::default()
        };

        let env = if request.env.is_empty() {
            None
        } else {
            Some(
                request
                    .env
                    .iter()
                    .map(|(k, v)| format!("{k}={v}"))
                    .collect::<Vec<_>>(),
            )
        };

        self.docker
            .create_container(
                Some(CreateContainerOptions {
                    name: container_name.as_str(),
                    platform: None,
                }),
                ContainerConfig {
                    image: Some(image.clone()),
                    cmd: Some(request.command.clone()),
                    env,
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    tty: Some(false),
                    host_config: Some(host_config),
                    ..Default::default()
                },
            )
            .await?;

        self.docker
            .start_container(&container_name, None::<StartContainerOptions<String>>)
            .await?;

        let started = Instant::now();
        let mut wait_stream = self
            .docker
            .wait_container(&container_name, None::<WaitContainerOptions<String>>);
        let wait_next = wait_stream.try_next();

        let exit_code =
            match tokio::time::timeout(Duration::from_secs(request.budget.timeout_secs), wait_next)
                .await
            {
                Ok(Ok(Some(result))) => i32::try_from(result.status_code).unwrap_or_default(),
                Ok(Ok(None)) => 0,
                Ok(Err(BollardError::DockerContainerWaitError { code, .. })) => {
                    i32::try_from(code).unwrap_or_default()
                }
                Ok(Err(err)) => {
                    self.cleanup_container(&container_name).await;
                    return Err(err.into());
                }
                Err(_) => {
                    let _ = self
                        .docker
                        .kill_container(
                            &container_name,
                            Some(KillContainerOptions { signal: "KILL" }),
                        )
                        .await;
                    self.cleanup_container(&container_name).await;
                    return Err(SandboxError::Timeout);
                }
            };

        let mut stdout = String::new();
        let mut stderr = String::new();
        let mut logs_stream = self.docker.logs(
            &container_name,
            Some(LogsOptions::<String> {
                follow: false,
                stdout: true,
                stderr: true,
                timestamps: false,
                tail: "all".to_string(),
                ..Default::default()
            }),
        );

        while let Some(entry) = logs_stream.next().await {
            match entry? {
                bollard::container::LogOutput::StdOut { message } => {
                    stdout.push_str(&String::from_utf8_lossy(&message));
                }
                bollard::container::LogOutput::StdErr { message } => {
                    stderr.push_str(&String::from_utf8_lossy(&message));
                }
                bollard::container::LogOutput::Console { message } => {
                    stdout.push_str(&String::from_utf8_lossy(&message));
                }
                _ => {}
            }
        }

        let inspect = self.docker.inspect_container(&container_name, None).await?;
        let oom_killed = inspect
            .state
            .as_ref()
            .and_then(|s| s.oom_killed)
            .unwrap_or(false);
        let error_mentions_oom = inspect
            .state
            .as_ref()
            .and_then(|s| s.error.as_ref())
            .map(|error| error.to_ascii_lowercase().contains("oom"))
            .unwrap_or(false);
        if oom_killed || (exit_code == 137 && error_mentions_oom) {
            self.cleanup_container(&container_name).await;
            return Err(SandboxError::OomKilled);
        }

        let image_info = self.docker.inspect_image(&image).await?;
        let container_digest = image_info.id.unwrap_or_default();

        let duration_ms = started.elapsed().as_millis() as u64;
        let result = ExecutionResult {
            exit_code,
            stdout,
            stderr,
            artifacts: vec![],
            container_digest,
            duration_ms,
            resource_usage: ResourceUsage::default(),
        };

        self.cleanup_container(&container_name).await;
        Ok(result)
    }

    async fn ensure_image(&self, image: &str) -> Result<(), SandboxError> {
        if self.docker.inspect_image(image).await.is_ok() {
            return Ok(());
        }

        let mut stream = self.docker.create_image(
            Some(CreateImageOptions {
                from_image: image,
                ..Default::default()
            }),
            None,
            None,
        );
        while stream.try_next().await?.is_some() {}
        Ok(())
    }

    async fn cleanup_container(&self, container_name: &str) {
        let _ = self
            .docker
            .remove_container(
                container_name,
                Some(RemoveContainerOptions {
                    force: true,
                    ..Default::default()
                }),
            )
            .await;
    }
}

#[derive(Debug, Clone)]
pub struct ImageRegistry {
    kani: String,
    z3: String,
    miri: String,
    madsim: String,
    fuzz: String,
}

impl Default for ImageRegistry {
    fn default() -> Self {
        Self {
            kani: "audit-agent/kani:0.57.0".to_string(),
            z3: "audit-agent/z3:4.13.0".to_string(),
            miri: "audit-agent/miri:nightly-2024-11-01".to_string(),
            madsim: "audit-agent/madsim:0.2.30".to_string(),
            fuzz: "audit-agent/fuzz:0.12.0-0.21.0".to_string(),
        }
    }
}

impl ImageRegistry {
    fn resolve(&self, image: &ToolImage) -> String {
        match image {
            ToolImage::Kani => self.kani.clone(),
            ToolImage::Z3 => self.z3.clone(),
            ToolImage::Miri => self.miri.clone(),
            ToolImage::MadSim => self.madsim.clone(),
            ToolImage::Fuzz => self.fuzz.clone(),
            ToolImage::Custom(value) => value.clone(),
        }
    }
}
