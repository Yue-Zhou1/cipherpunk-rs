use std::collections::HashMap;
use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CargoWorkspace {
    pub root: PathBuf,
    pub members: Vec<CrateMeta>,
    pub dependency_graph: DependencyGraph,
    pub feature_flags: FeatureFlagMap,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CrateMeta {
    pub name: String,
    pub path: PathBuf,
    pub kind: CrateKind,
    pub dependencies: Vec<Dependency>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
pub enum CrateKind {
    #[default]
    Lib,
    Bin,
    Test,
    Bench,
    FuzzTarget,
    Example,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Dependency {
    pub name: String,
    pub req: Option<String>,
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DependencyGraph {
    pub edges: HashMap<String, Vec<String>>,
}

pub type FeatureFlagMap = HashMap<String, Vec<FeatureFlag>>;

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct FeatureFlag {
    pub name: String,
    pub enables: Vec<String>,
}
