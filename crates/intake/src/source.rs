use std::ffi::OsStr;
use std::fs;
use std::io::{Read, Seek};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use audit_agent_core::audit_config::{ResolvedSource, SourceOrigin};
use git2::{Cred, FetchOptions, Oid, RemoteCallbacks, Repository};
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

pub struct SourceResolver;

#[derive(Debug, Clone)]
pub struct ResolvedWithWarnings {
    pub source: ResolvedSource,
    pub warnings: Vec<SourceWarning>,
}

pub enum SourceInput {
    GitUrl {
        url: String,
        commit: String,
        auth: Option<GitAuth>,
        allow_branch_resolution: bool,
    },
    LocalPath {
        path: PathBuf,
        commit: Option<String>,
    },
    Archive {
        path: PathBuf,
    },
}

pub enum GitAuth {
    Token(String),
    SshKey {
        path: PathBuf,
        passphrase: Option<String>,
    },
    Netrc,
}

#[derive(Debug, Clone)]
pub enum SourceWarning {
    BranchResolved {
        branch: String,
        resolved_sha: String,
    },
    DirtyWorkingTree {
        uncommitted_files: Vec<PathBuf>,
    },
}

impl SourceResolver {
    pub async fn resolve(input: &SourceInput, work_dir: &Path) -> Result<ResolvedWithWarnings> {
        match input {
            SourceInput::GitUrl {
                url,
                commit,
                auth,
                allow_branch_resolution,
            } => {
                Self::clone_git(
                    url,
                    commit,
                    auth.as_ref(),
                    work_dir,
                    *allow_branch_resolution,
                )
                .await
            }
            SourceInput::LocalPath { path, commit } => {
                Self::resolve_local(path, commit.as_deref()).await
            }
            SourceInput::Archive { path } => Self::unpack_archive(path, work_dir).await,
        }
    }

    async fn clone_git(
        url: &str,
        commit: &str,
        auth: Option<&GitAuth>,
        work_dir: &Path,
        allow_branch_resolution: bool,
    ) -> Result<ResolvedWithWarnings> {
        fs::create_dir_all(work_dir)
            .with_context(|| format!("create work dir: {}", work_dir.display()))?;
        let dest = unique_subdir(work_dir, "git-src");
        let mut callbacks = RemoteCallbacks::new();
        callbacks.credentials(move |_url, username, _allowed_types| match auth {
            Some(GitAuth::Token(token)) => Cred::userpass_plaintext("oauth2", token),
            Some(GitAuth::SshKey { path, passphrase }) => {
                Cred::ssh_key(username.unwrap_or("git"), None, path, passphrase.as_deref())
            }
            Some(GitAuth::Netrc) => Cred::default(),
            None => {
                if let Some(user) = username {
                    Cred::ssh_key_from_agent(user)
                } else {
                    Cred::default()
                }
            }
        });

        let mut fetch = FetchOptions::new();
        fetch.remote_callbacks(callbacks);

        let mut builder = git2::build::RepoBuilder::new();
        builder.fetch_options(fetch);
        let repo = builder
            .clone(url, &dest)
            .with_context(|| format!("clone failed from {url}"))?;

        let mut warnings = vec![];
        let oid = if is_sha(commit) {
            Oid::from_str(commit).with_context(|| format!("invalid OID {commit}"))?
        } else if allow_branch_resolution {
            let branch_oid = resolve_branch_oid(&repo, commit)?;
            warnings.push(SourceWarning::BranchResolved {
                branch: commit.to_string(),
                resolved_sha: branch_oid.to_string(),
            });
            branch_oid
        } else {
            return Err(anyhow::anyhow!(
                "BranchNameNotAllowed: `{commit}`. Provide a full 40-character SHA"
            ));
        };

        let obj = repo.find_object(oid, None)?;
        repo.checkout_tree(&obj, None)?;
        repo.set_head_detached(oid)?;

        let head = repo.head()?.target().context("HEAD has no target")?;
        if head != oid {
            return Err(anyhow::anyhow!(
                "TOCTOU guard failed: HEAD {} does not match requested {}",
                head,
                oid
            ));
        }

        let content_hash = compute_content_hash(&dest)?;

        Ok(ResolvedWithWarnings {
            source: ResolvedSource {
                local_path: dest,
                origin: SourceOrigin::Git {
                    url: url.to_string(),
                    original_ref: (!is_sha(commit)).then_some(commit.to_string()),
                },
                commit_hash: oid.to_string(),
                content_hash,
            },
            warnings,
        })
    }

    async fn resolve_local(path: &Path, commit: Option<&str>) -> Result<ResolvedWithWarnings> {
        let canonical = path
            .canonicalize()
            .with_context(|| format!("local source path not found: {}", path.display()))?;
        let repo = Repository::open(&canonical).context("local path is not a git repository")?;

        let commit_hash = match commit {
            Some(commit) => {
                if is_sha(commit) {
                    commit.to_string()
                } else {
                    return Err(anyhow::anyhow!(
                        "BranchNameNotAllowed: `{commit}`. Provide a full 40-character SHA"
                    ));
                }
            }
            None => repo
                .head()?
                .target()
                .context("HEAD missing target")?
                .to_string(),
        };

        let mut status_opts = git2::StatusOptions::new();
        status_opts
            .include_untracked(true)
            .renames_head_to_index(true);
        let statuses = repo.statuses(Some(&mut status_opts))?;

        let mut warnings = vec![];
        let dirty: Vec<PathBuf> = statuses
            .iter()
            .filter_map(|entry| entry.path().map(PathBuf::from))
            .collect();
        if !dirty.is_empty() {
            warnings.push(SourceWarning::DirtyWorkingTree {
                uncommitted_files: dirty,
            });
        }

        let content_hash = compute_content_hash(&canonical)?;
        Ok(ResolvedWithWarnings {
            source: ResolvedSource {
                local_path: canonical.clone(),
                origin: SourceOrigin::Local {
                    original_path: canonical,
                },
                commit_hash,
                content_hash,
            },
            warnings,
        })
    }

    async fn unpack_archive(path: &Path, work_dir: &Path) -> Result<ResolvedWithWarnings> {
        fs::create_dir_all(work_dir)
            .with_context(|| format!("create work dir: {}", work_dir.display()))?;
        let bytes = fs::read(path).with_context(|| format!("read archive {}", path.display()))?;
        let archive_hash = format!("archive:{}", hex::encode(Sha256::digest(&bytes)));

        let dest = unique_subdir(work_dir, "archive-src");
        fs::create_dir_all(&dest)?;

        if looks_like_gzip_tar(&bytes) {
            let file = fs::File::open(path)?;
            let decoder = flate2::read::GzDecoder::new(file);
            let mut archive = tar::Archive::new(decoder);
            archive.unpack(&dest)?;
        } else if looks_like_zip(&bytes) {
            let file = fs::File::open(path)?;
            unpack_zip(file, &dest)?;
        } else {
            return Err(anyhow::anyhow!(
                "unsupported archive format; expected .tar.gz or .zip magic bytes"
            ));
        }

        let workspace_root = find_workspace_root(&dest)?.unwrap_or(dest.clone());
        let content_hash = compute_content_hash(&workspace_root)?;

        Ok(ResolvedWithWarnings {
            source: ResolvedSource {
                local_path: workspace_root,
                origin: SourceOrigin::Archive {
                    original_filename: path
                        .file_name()
                        .and_then(OsStr::to_str)
                        .unwrap_or_default()
                        .to_string(),
                },
                commit_hash: archive_hash,
                content_hash,
            },
            warnings: vec![],
        })
    }
}

fn resolve_branch_oid(repo: &Repository, branch: &str) -> Result<Oid> {
    for candidate in [
        format!("refs/remotes/origin/{branch}"),
        format!("refs/heads/{branch}"),
    ] {
        if let Ok(reference) = repo.find_reference(&candidate) {
            if let Some(oid) = reference.target() {
                return Ok(oid);
            }
        }
    }

    // fallback via revparse for tags/other refs
    if let Ok(obj) = repo.revparse_single(branch) {
        return Ok(obj.id());
    }

    Err(anyhow::anyhow!(
        "unable to resolve branch/reference `{branch}`"
    ))
}

fn unpack_zip<R: Read + Seek>(reader: R, dest: &Path) -> Result<()> {
    let mut zip = zip::ZipArchive::new(reader)?;
    for i in 0..zip.len() {
        let mut file = zip.by_index(i)?;
        let outpath = dest.join(file.mangled_name());
        if file.name().ends_with('/') {
            fs::create_dir_all(&outpath)?;
        } else {
            if let Some(parent) = outpath.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut outfile = fs::File::create(&outpath)?;
            std::io::copy(&mut file, &mut outfile)?;
        }
    }
    Ok(())
}

fn looks_like_gzip_tar(bytes: &[u8]) -> bool {
    bytes.len() >= 2 && bytes[0] == 0x1f && bytes[1] == 0x8b
}

fn looks_like_zip(bytes: &[u8]) -> bool {
    bytes.len() >= 4 && bytes[..4] == [0x50, 0x4b, 0x03, 0x04]
}

fn find_workspace_root(root: &Path) -> Result<Option<PathBuf>> {
    for entry in WalkDir::new(root).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.file_name() == Some(OsStr::new("Cargo.toml")) {
            let content = fs::read_to_string(path)?;
            if content.contains("[workspace]") {
                return Ok(path.parent().map(Path::to_path_buf));
            }
        }
    }
    Ok(None)
}

fn compute_content_hash(root: &Path) -> Result<String> {
    let mut files: Vec<PathBuf> = WalkDir::new(root)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.path().to_path_buf())
        .filter(|path| !path.components().any(|c| c.as_os_str() == ".git"))
        .collect();

    files.sort();

    let mut hasher = Sha256::new();
    for file in files {
        let rel = file.strip_prefix(root).unwrap_or(&file);
        hasher.update(rel.to_string_lossy().as_bytes());
        hasher.update([0u8]);
        let data = fs::read(&file)?;
        hasher.update(&data);
        hasher.update([0u8]);
    }

    Ok(hex::encode(hasher.finalize()))
}

fn unique_subdir(root: &Path, prefix: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    root.join(format!("{prefix}-{stamp}"))
}

fn is_sha(value: &str) -> bool {
    value.len() == 40 && value.chars().all(|c| c.is_ascii_hexdigit())
}
