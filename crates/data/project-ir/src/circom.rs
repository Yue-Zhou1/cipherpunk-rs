use anyhow::Result;
use audit_agent_core::workspace::CargoWorkspace;
use walkdir::WalkDir;

use crate::LanguageMapper;
use crate::graph::{FileNode, FrameworkView, ProjectIrFragment};

#[derive(Debug, Default)]
pub struct CircomMapper;

impl LanguageMapper for CircomMapper {
    fn can_handle(&self, workspace: &CargoWorkspace) -> bool {
        workspace.members.iter().any(|member| {
            WalkDir::new(&member.path)
                .follow_links(true)
                .into_iter()
                .filter_map(|entry| entry.ok())
                .any(|entry| entry.path().extension().and_then(|v| v.to_str()) == Some("circom"))
        })
    }

    fn build(&self, workspace: &CargoWorkspace) -> Result<ProjectIrFragment> {
        let mut fragment = ProjectIrFragment::default();
        for member in &workspace.members {
            for entry in WalkDir::new(&member.path)
                .follow_links(true)
                .into_iter()
                .filter_map(|entry| entry.ok())
            {
                if !entry.file_type().is_file() {
                    continue;
                }
                if entry.path().extension().and_then(|v| v.to_str()) != Some("circom") {
                    continue;
                }
                fragment.file_graph.nodes.push(FileNode {
                    id: format!("file:{}", entry.path().display()),
                    path: entry.path().to_path_buf(),
                    language: "circom".to_string(),
                });
            }
        }
        if !fragment.file_graph.nodes.is_empty() {
            fragment.framework_views.push(FrameworkView {
                framework: "circom".to_string(),
                node_ids: fragment
                    .file_graph
                    .nodes
                    .iter()
                    .map(|node| node.id.clone())
                    .collect(),
            });
        }
        Ok(fragment)
    }
}
