use std::io::Write;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};
use audit_agent_core::finding::FindingId;
use serde::{Deserialize, Serialize};
use tokio::fs;
use zip::CompressionMethod;
use zip::write::FileOptions;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvironmentManifest {
    pub rust_toolchain: String,
    pub cargo_lock_hash: String,
    pub workspace_root: PathBuf,
    pub audit_id: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceManifest {
    pub finding_id: String,
    pub title: String,
    pub agent_version: String,
    pub source_commit: String,
    pub source_content_hash: Option<String>,
    pub tool: String,
    pub tool_version: String,
    pub container_image: String,
    pub container_digest: String,
    pub reproduction_command: String,
    pub expected_output_description: String,
    pub files: Vec<String>,
    #[serde(default)]
    pub environment_manifest: Option<EnvironmentManifest>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceFile {
    pub relative_path: PathBuf,
    pub contents: Vec<u8>,
}

impl EvidenceFile {
    pub fn text(path: impl AsRef<Path>, content: impl Into<String>) -> Self {
        Self {
            relative_path: path.as_ref().to_path_buf(),
            contents: content.into().into_bytes(),
        }
    }

    pub fn binary(path: impl AsRef<Path>, content: Vec<u8>) -> Self {
        Self {
            relative_path: path.as_ref().to_path_buf(),
            contents: content,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidencePack {
    pub manifest: EvidenceManifest,
    pub files: Vec<EvidenceFile>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceStore {
    base_dir: PathBuf,
}

impl EvidenceStore {
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
        }
    }

    pub async fn save_pack(&self, finding_id: &FindingId, pack: &EvidencePack) -> Result<()> {
        let finding_id_text = finding_id.to_string();
        if pack.manifest.finding_id != finding_id_text {
            bail!(
                "finding ID mismatch: key is {finding_id_text}, manifest has {}",
                pack.manifest.finding_id
            );
        }

        let finding_dir = self.finding_dir(finding_id);
        if finding_dir.exists() {
            fs::remove_dir_all(&finding_dir).await.with_context(|| {
                format!("remove old evidence directory {}", finding_dir.display())
            })?;
        }
        fs::create_dir_all(&finding_dir)
            .await
            .with_context(|| format!("create evidence directory {}", finding_dir.display()))?;

        let mut manifest = pack.manifest.clone();
        let mut files = Vec::new();
        for item in &pack.files {
            ensure_safe_relative_path(&item.relative_path)?;
            let abs_path = finding_dir.join(&item.relative_path);
            if let Some(parent) = abs_path.parent() {
                fs::create_dir_all(parent).await.with_context(|| {
                    format!("create evidence subdirectory {}", parent.display())
                })?;
            }
            fs::write(&abs_path, &item.contents)
                .await
                .with_context(|| format!("write evidence file {}", abs_path.display()))?;
            files.push(path_to_manifest_entry(&item.relative_path));
        }

        files.push("manifest.json".to_string());
        files.push("reproduce.sh".to_string());
        files.sort();
        files.dedup();
        manifest.files = files;

        let reproduce_path = finding_dir.join("reproduce.sh");
        let reproduce_script = render_reproduce_script(&manifest, &finding_dir);
        fs::write(&reproduce_path, reproduce_script.as_bytes())
            .await
            .with_context(|| format!("write reproduce script {}", reproduce_path.display()))?;
        make_executable(&reproduce_path)?;

        let manifest_path = finding_dir.join("manifest.json");
        let manifest_bytes = serde_json::to_vec_pretty(&manifest)?;
        fs::write(&manifest_path, manifest_bytes)
            .await
            .with_context(|| format!("write manifest {}", manifest_path.display()))?;

        Ok(())
    }

    pub async fn generate_reproduce_script(&self, finding_id: &FindingId) -> Result<String> {
        let manifest = self.load_manifest(finding_id).await?;
        let finding_dir = self.finding_dir(finding_id);
        Ok(render_reproduce_script(&manifest, &finding_dir))
    }

    pub async fn export_zip(&self, finding_ids: &[FindingId], dest: &Path) -> Result<()> {
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create zip output directory {}", parent.display()))?;
        }

        let dest_file = std::fs::File::create(dest)
            .with_context(|| format!("create zip archive {}", dest.display()))?;
        let mut zip = zip::ZipWriter::new(dest_file);
        let default_options = FileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .unix_permissions(0o644);

        for finding_id in finding_ids {
            let finding_dir = self.finding_dir(finding_id);
            let manifest = self.load_manifest(finding_id).await?;
            let prefix = format!("{finding_id}/");

            for relative in &manifest.files {
                let relative_path = Path::new(relative);
                ensure_safe_relative_path(relative_path)?;
                let abs_path = finding_dir.join(relative_path);
                let bytes = std::fs::read(&abs_path).with_context(|| {
                    format!("read evidence file for zip {}", abs_path.display())
                })?;
                let zip_entry = format!("{prefix}{relative}").replace('\\', "/");
                let options = if relative.ends_with(".sh") {
                    default_options.unix_permissions(0o755)
                } else {
                    default_options
                };
                zip.start_file(zip_entry, options)
                    .with_context(|| format!("start zip entry for {}", abs_path.display()))?;
                zip.write_all(&bytes)
                    .with_context(|| format!("write zip entry for {}", abs_path.display()))?;
            }
        }

        zip.finish().context("finalize zip archive")?;
        Ok(())
    }

    pub async fn load_manifest(&self, finding_id: &FindingId) -> Result<EvidenceManifest> {
        let manifest_path = self.finding_dir(finding_id).join("manifest.json");
        let bytes = fs::read(&manifest_path)
            .await
            .with_context(|| format!("read manifest {}", manifest_path.display()))?;
        let manifest: EvidenceManifest = serde_json::from_slice(&bytes)
            .with_context(|| format!("parse manifest {}", manifest_path.display()))?;
        Ok(manifest)
    }

    fn finding_dir(&self, finding_id: &FindingId) -> PathBuf {
        self.base_dir.join(finding_id.to_string())
    }
}

fn ensure_safe_relative_path(path: &Path) -> Result<()> {
    if path.is_absolute() {
        bail!("evidence path must be relative: {}", path.display());
    }

    for component in path.components() {
        match component {
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                bail!("invalid path component in {}", path.display())
            }
            Component::CurDir | Component::Normal(_) => {}
        }
    }

    Ok(())
}

fn path_to_manifest_entry(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn render_reproduce_script(manifest: &EvidenceManifest, finding_dir: &Path) -> String {
    let content_hash = manifest.source_content_hash.as_deref().unwrap_or("n/a");
    let evidence_dir = escape_shell_double_quotes(&finding_dir.to_string_lossy());
    let image_ref = image_reference(&manifest.container_image, &manifest.container_digest);

    format!(
        "#!/usr/bin/env bash\n\
# Auto-generated by audit-agent v{agent_version}\n\
# Finding: {finding_id} - {title}\n\
# Source commit: {source_commit}   Content hash: {content_hash}\n\
# Reproduced with: {tool} {tool_version}  Container: {container_digest}\n\
set -euo pipefail\n\
docker run --rm \\\n\
  --volume \"{evidence_dir}:/evidence:ro\" \\\n\
  --network none \\\n\
  {image_ref} \\\n\
  {reproduction_command}\n\
# Expected: {expected_output}\n",
        agent_version = manifest.agent_version,
        finding_id = manifest.finding_id,
        title = manifest.title,
        source_commit = manifest.source_commit,
        content_hash = content_hash,
        tool = manifest.tool,
        tool_version = manifest.tool_version,
        container_digest = manifest.container_digest,
        evidence_dir = evidence_dir,
        image_ref = image_ref,
        reproduction_command = manifest.reproduction_command,
        expected_output = manifest.expected_output_description,
    )
}

fn image_reference(image: &str, digest: &str) -> String {
    if image.contains('@') {
        image.to_string()
    } else {
        format!("{image}@{digest}")
    }
}

fn escape_shell_double_quotes(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn make_executable(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut perms = std::fs::metadata(path)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(path, perms)?;
    }
    Ok(())
}
