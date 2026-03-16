use std::collections::HashMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use audit_agent_core::workspace::{
    CargoWorkspace, CrateKind, CrateMeta, Dependency, DependencyGraph, FeatureFlag,
};
use serde::Deserialize;

pub struct WorkspaceAnalyzer;

#[derive(Debug, Clone)]
pub struct ExclusionSuggestion {
    pub crate_name: String,
    pub reason: String,
}

#[derive(Debug, Deserialize)]
struct WorkspaceManifest {
    workspace: Option<WorkspaceSection>,
}

#[derive(Debug, Deserialize)]
struct WorkspaceSection {
    members: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct CrateManifest {
    package: Option<PackageSection>,
    lib: Option<toml::Value>,
    bin: Option<Vec<BinSection>>,
    bench: Option<Vec<BenchSection>>,
    dependencies: Option<toml::Value>,
    features: Option<HashMap<String, toml::Value>>,
}

#[derive(Debug, Deserialize)]
struct PackageSection {
    name: String,
}

#[derive(Debug, Deserialize)]
struct BinSection {
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct BenchSection {
    name: Option<String>,
}

impl WorkspaceAnalyzer {
    pub fn analyze(root: &Path) -> Result<CargoWorkspace> {
        let root = root
            .canonicalize()
            .with_context(|| format!("workspace root not found: {}", root.display()))?;
        let manifest_path = root.join("Cargo.toml");
        let manifest_content = fs::read_to_string(&manifest_path)
            .with_context(|| format!("failed reading {}", manifest_path.display()))?;

        let workspace_manifest: WorkspaceManifest =
            toml::from_str(&manifest_content).context("invalid workspace Cargo.toml")?;

        let member_entries = workspace_manifest
            .workspace
            .and_then(|w| w.members)
            .unwrap_or_else(|| vec![".".to_string()]);

        let mut members = Vec::new();
        for member in member_entries {
            let member_path = root.join(&member);
            let crate_manifest_path = member_path.join("Cargo.toml");
            if !crate_manifest_path.exists() {
                continue;
            }
            let crate_manifest_str =
                fs::read_to_string(&crate_manifest_path).with_context(|| {
                    format!(
                        "failed reading crate manifest {}",
                        crate_manifest_path.display()
                    )
                })?;
            let parsed: CrateManifest = toml::from_str(&crate_manifest_str).with_context(|| {
                format!("invalid crate manifest {}", crate_manifest_path.display())
            })?;

            let package_name = parsed
                .package
                .as_ref()
                .map(|p| p.name.clone())
                .unwrap_or_else(|| member.replace('/', "-"));

            let kind = detect_crate_kind(&package_name, &member_path, &parsed);
            let dependencies = parse_dependencies(parsed.dependencies);
            let feature_flags = parse_features(parsed.features);

            members.push(CrateMeta {
                name: package_name,
                path: member_path,
                kind,
                dependencies,
            });

            // attach parsed features after member creation
            if !feature_flags.is_empty() {
                // This temporary map is stitched below.
            }
        }

        let member_names: Vec<String> = members.iter().map(|m| m.name.clone()).collect();
        let mut edges: HashMap<String, Vec<String>> = HashMap::new();
        let mut feature_flags: HashMap<String, Vec<FeatureFlag>> = HashMap::new();

        for member in &members {
            let manifest_path = member.path.join("Cargo.toml");
            let manifest = fs::read_to_string(&manifest_path)?;
            let parsed: CrateManifest = toml::from_str(&manifest)?;

            let deps = parse_dependencies(parsed.dependencies);
            let internal: Vec<String> = deps
                .iter()
                .filter(|dep| member_names.iter().any(|n| n == &dep.name))
                .map(|dep| dep.name.clone())
                .collect();
            edges.insert(member.name.clone(), internal);

            feature_flags.insert(member.name.clone(), parse_features(parsed.features));
        }

        Ok(CargoWorkspace {
            root,
            members,
            dependency_graph: DependencyGraph { edges },
            feature_flags,
        })
    }

    pub fn suggest_exclusions(workspace: &CargoWorkspace) -> Vec<ExclusionSuggestion> {
        workspace
            .members
            .iter()
            .filter(|c| {
                matches!(c.kind, CrateKind::Bench | CrateKind::FuzzTarget)
                    || c.name.contains("-bench")
                    || c.name.contains("-fuzz")
                    || c.name.contains("-example")
            })
            .map(|c| ExclusionSuggestion {
                crate_name: c.name.clone(),
                reason: format!("{:?} crate — typically not in audit scope", c.kind),
            })
            .collect()
    }
}

fn detect_crate_kind(name: &str, path: &Path, parsed: &CrateManifest) -> CrateKind {
    if name.contains("fuzz")
        || path.to_string_lossy().contains("fuzz")
        || parsed.bin.as_ref().is_some_and(|bins| {
            bins.iter()
                .any(|b| b.name.as_deref().unwrap_or("").contains("fuzz"))
        })
    {
        return CrateKind::FuzzTarget;
    }

    if name.contains("bench") || parsed.bench.as_ref().is_some_and(|b| !b.is_empty()) {
        return CrateKind::Bench;
    }

    if path.join("src/lib.rs").exists() || parsed.lib.is_some() {
        return CrateKind::Lib;
    }

    if path.join("src/main.rs").exists() {
        return CrateKind::Bin;
    }

    CrateKind::Lib
}

fn parse_dependencies(value: Option<toml::Value>) -> Vec<Dependency> {
    let Some(toml::Value::Table(table)) = value else {
        return vec![];
    };

    table
        .into_iter()
        .map(|(name, spec)| Dependency {
            name,
            req: match spec {
                toml::Value::String(v) => Some(v),
                toml::Value::Table(table) => table
                    .get("version")
                    .and_then(|v| v.as_str().map(ToString::to_string)),
                _ => None,
            },
        })
        .collect()
}

fn parse_features(value: Option<HashMap<String, toml::Value>>) -> Vec<FeatureFlag> {
    let Some(features) = value else {
        return vec![];
    };

    features
        .into_iter()
        .map(|(name, enabled)| {
            let enables = enabled
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(ToString::to_string))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            FeatureFlag { name, enables }
        })
        .collect()
}
