use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use audit_agent_core::workspace::CargoWorkspace;
use intake::detection::DetectedEntryPoint;
use llm::{CompletionOpts, EvidenceGate, LlmProvider, LlmRole, llm_call_traced};

use crate::feasibility::AdapterPoint;
use crate::util::sanitize_ident;

const ENTRY_CALL_PLACEHOLDER: &str = "// ENTRY_POINT_CALL";
const DEFAULT_ENTRY_CALL_TODO: &str = "// TODO: fill entry point call";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetworkTopology {
    Mesh,
    Star,
    Ring,
    Custom(String),
}

impl Default for NetworkTopology {
    fn default() -> Self {
        Self::Mesh
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DistributedAuditConfig {
    pub node_count: usize,
    pub topology: NetworkTopology,
    pub simulation_duration_secs: u64,
}

impl Default for DistributedAuditConfig {
    fn default() -> Self {
        Self {
            node_count: 3,
            topology: NetworkTopology::Mesh,
            simulation_duration_secs: 1,
        }
    }
}

#[derive(Debug, Clone)]
pub struct MadSimHarness {
    pub project_dir: PathBuf,
    pub entry_point: String,
    pub node_count: usize,
    pub topology: NetworkTopology,
    main_file: PathBuf,
    source: String,
}

impl MadSimHarness {
    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn run_smoke_test(&self) -> Result<()> {
        let binary_path = self.project_dir.join("harness-smoke");
        let compile_output = Command::new("rustc")
            .arg("--edition=2024")
            .arg(&self.main_file)
            .arg("-o")
            .arg(&binary_path)
            .output()
            .context("failed to run rustc for harness smoke test")?;

        if !compile_output.status.success() {
            let stderr = String::from_utf8_lossy(&compile_output.stderr).to_string();
            anyhow::bail!("harness smoke compile failed: {stderr}");
        }

        let run_output = Command::new(&binary_path)
            .output()
            .context("failed to execute harness smoke binary")?;
        if !run_output.status.success() {
            let stderr = String::from_utf8_lossy(&run_output.stderr).to_string();
            anyhow::bail!("harness smoke execution failed: {stderr}");
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterScaffold {
    pub adapter_points: Vec<AdapterPoint>,
    pub description: String,
}

pub struct HarnessBuilder {
    llm: Option<Arc<dyn LlmProvider>>,
    #[allow(dead_code)]
    evidence_gate: Arc<EvidenceGate>,
    output_root: PathBuf,
}

impl HarnessBuilder {
    pub fn new(llm: Option<Arc<dyn LlmProvider>>, evidence_gate: Arc<EvidenceGate>) -> Self {
        Self {
            llm,
            evidence_gate,
            output_root: std::env::temp_dir(),
        }
    }

    pub fn without_llm_for_tests() -> Self {
        Self {
            llm: None,
            evidence_gate: Arc::new(EvidenceGate::without_sandbox_for_tests()),
            output_root: std::env::temp_dir(),
        }
    }

    pub fn new_with_llm_for_tests(llm: Arc<dyn LlmProvider>) -> Self {
        Self {
            llm: Some(llm),
            evidence_gate: Arc::new(EvidenceGate::without_sandbox_for_tests()),
            output_root: std::env::temp_dir(),
        }
    }

    pub async fn generate_level_a(
        &self,
        _workspace: &CargoWorkspace,
        entry_points: &[DetectedEntryPoint],
        config: &DistributedAuditConfig,
    ) -> Result<MadSimHarness> {
        let entry_point_name = entry_points
            .first()
            .map(|entry| entry.function.clone())
            .unwrap_or_else(|| "start_node".to_string());

        let skeleton = self.generate_skeleton(&entry_point_name, config);
        let entry_call = if let Some(llm) = &self.llm {
            llm_call_traced(
                llm.as_ref(),
                LlmRole::Scaffolding,
                &self.entry_call_prompt(entry_points),
                &CompletionOpts::default(),
            )
            .await
            .map(|(raw, provenance)| {
                tracing::debug!(
                    provider = %provenance.provider,
                    model = ?provenance.model,
                    role = %provenance.role,
                    duration_ms = provenance.duration_ms,
                    attempt = provenance.attempt,
                    "captured madsim-entry-call LLM provenance"
                );
                self.filter_entry_call(&raw)
            })
            .unwrap_or_else(|_| self.entry_call_todo_comment(entry_points))
        } else {
            self.entry_call_todo_comment(entry_points)
        };

        let source = skeleton.replace(ENTRY_CALL_PLACEHOLDER, &entry_call);
        let project_dir = self.prepare_project_dir("madsim-level-a")?;
        let main_file = project_dir.join("src/main.rs");
        self.write_harness_project(&project_dir, &main_file, &source)?;

        Ok(MadSimHarness {
            project_dir,
            entry_point: entry_point_name,
            node_count: config.node_count,
            topology: config.topology.clone(),
            main_file,
            source,
        })
    }

    pub async fn generate_level_b_scaffold(
        &self,
        workspace: &CargoWorkspace,
        adapter_points: &[AdapterPoint],
    ) -> Result<AdapterScaffold> {
        let mut lines = Vec::new();
        lines.push("MadSim LevelB adapter scaffold".to_string());
        lines.push(format!("workspace: {}", workspace.root.display()));
        lines.push(String::new());

        for point in adapter_points {
            let display_path = relative_display_path(&workspace.root, &point.file);
            lines.push(format!(
                "- {}:{} [{}] {}",
                display_path, point.line, point.crate_name, point.reason
            ));
        }

        Ok(AdapterScaffold {
            adapter_points: adapter_points.to_vec(),
            description: lines.join("\n"),
        })
    }

    pub fn filter_entry_call(&self, raw: &str) -> String {
        raw.lines()
            .find(|line| {
                let trimmed = line.trim_start();
                trimmed.starts_with("node_handle") || trimmed.contains("spawn(async")
            })
            .map(|line| line.trim().to_string())
            .unwrap_or_else(|| DEFAULT_ENTRY_CALL_TODO.to_string())
    }

    fn entry_call_prompt(&self, entry_points: &[DetectedEntryPoint]) -> String {
        let mut prompt = String::from(
            "Generate exactly one Rust line with a node spawn call. \
             Use this shape: node_handle.spawn(async move { ... });\n",
        );
        if entry_points.is_empty() {
            prompt.push_str("No entry points were detected. Use a TODO comment.\n");
        } else {
            prompt.push_str("Detected entry points:\n");
            for entry in entry_points {
                prompt.push_str(&format!(
                    "- {}::{} at {}:{}\n",
                    entry.crate_name,
                    entry.function,
                    entry.file.display(),
                    entry.line
                ));
            }
        }
        prompt
    }

    fn entry_call_todo_comment(&self, entry_points: &[DetectedEntryPoint]) -> String {
        if let Some(entry) = entry_points.first() {
            format!("// TODO: fill entry point call for {}", entry.function)
        } else {
            DEFAULT_ENTRY_CALL_TODO.to_string()
        }
    }

    fn generate_skeleton(&self, harness_name: &str, config: &DistributedAuditConfig) -> String {
        let safe_name = sanitize_ident(harness_name);
        format!(
            r#"#![allow(unused)]
use std::future::Future;
use std::pin::Pin;
use std::task::{{Context, Poll, RawWaker, RawWakerVTable, Waker}};
use std::time::{{Duration, Instant}};

mod madsim {{
    pub mod runtime {{
        #[derive(Clone)]
        pub struct Handle;

        pub struct NodeBuilder;
        pub struct NodeHandle;

        impl Handle {{
            pub fn current() -> Self {{
                Self
            }}

            pub fn create_node(&self) -> NodeBuilder {{
                NodeBuilder
            }}
        }}

        impl NodeBuilder {{
            pub fn name(self, _name: String) -> Self {{
                self
            }}

            pub fn ip(self, _ip: std::net::IpAddr) -> Self {{
                self
            }}

            pub fn build(self) -> NodeHandle {{
                NodeHandle
            }}
        }}

        impl NodeHandle {{
            pub fn spawn<F>(&self, _fut: F)
            where
                F: std::future::Future<Output = ()> + Send + 'static,
            {{
            }}
        }}
    }}

    pub mod time {{
        pub async fn sleep(_duration: std::time::Duration) {{}}
    }}
}}

#[derive(Clone, Default)]
struct NodeConfig {{
    node_id: usize,
}}

async fn start_node(_cfg: NodeConfig) {{}}

async fn audit_harness_{safe_name}() {{
    let handle = madsim::runtime::Handle::current();

    for i in 0..{node_count} {{
        let node_handle = handle
            .create_node()
            .name(format!("node-{{}}", i))
            .ip(format!("10.0.0.{{}}", i + 1).parse().unwrap())
            .build();
        let cfg = NodeConfig {{ node_id: i }};
        {entry_placeholder}
    }}

    madsim::time::sleep(Duration::from_secs({simulation_duration})).await;
    assert!(true, "invariant placeholder");
}}

fn noop_raw_waker() -> RawWaker {{
    fn clone(_: *const ()) -> RawWaker {{
        noop_raw_waker()
    }}
    fn wake(_: *const ()) {{}}
    fn wake_by_ref(_: *const ()) {{}}
    fn drop(_: *const ()) {{}}

    static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, wake, wake_by_ref, drop);
    RawWaker::new(std::ptr::null(), &VTABLE)
}}

fn block_on<F: Future>(mut fut: F) -> F::Output {{
    let waker = unsafe {{ Waker::from_raw(noop_raw_waker()) }};
    let mut cx = Context::from_waker(&waker);
    let mut fut = unsafe {{ Pin::new_unchecked(&mut fut) }};
    let deadline = Instant::now() + Duration::from_secs(2);

    loop {{
        if Instant::now() > deadline {{
            panic!("timed out while awaiting harness completion");
        }}
        match fut.as_mut().poll(&mut cx) {{
            Poll::Ready(output) => return output,
            Poll::Pending => std::thread::yield_now(),
        }}
    }}
}}

fn main() {{
    block_on(audit_harness_{safe_name}());
}}
"#,
            node_count = config.node_count,
            simulation_duration = config.simulation_duration_secs,
            safe_name = safe_name,
            entry_placeholder = ENTRY_CALL_PLACEHOLDER
        )
    }

    fn prepare_project_dir(&self, prefix: &str) -> Result<PathBuf> {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("system clock before unix epoch")?
            .as_nanos();
        let dir = self.output_root.join(format!("{prefix}-{nanos}"));
        fs::create_dir_all(dir.join("src")).with_context(|| {
            format!(
                "failed to create harness project directory {}",
                dir.display()
            )
        })?;
        Ok(dir)
    }

    fn write_harness_project(
        &self,
        project_dir: &Path,
        main_file: &Path,
        source: &str,
    ) -> Result<()> {
        let cargo_toml = project_dir.join("Cargo.toml");
        fs::write(
            &cargo_toml,
            r#"[package]
name = "madsim-generated-harness"
version = "0.1.0"
edition = "2024"
"#,
        )
        .with_context(|| format!("failed to write {}", cargo_toml.display()))?;

        fs::write(main_file, source)
            .with_context(|| format!("failed to write {}", main_file.display()))?;
        Ok(())
    }
}

fn relative_display_path(root: &Path, file: &Path) -> String {
    file.strip_prefix(root)
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| file.display().to_string())
}
