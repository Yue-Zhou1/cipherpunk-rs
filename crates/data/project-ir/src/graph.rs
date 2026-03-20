use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Graph<Node, Edge> {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

impl<Node, Edge> Default for Graph<Node, Edge> {
    fn default() -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileNode {
    pub id: String,
    pub path: PathBuf,
    pub language: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolNode {
    pub id: String,
    pub name: String,
    pub qualified_name: Option<String>,
    pub file: PathBuf,
    pub kind: String,
    pub line: u32,
    pub signature: Option<FunctionSignature>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionSignature {
    pub parameters: Vec<ParameterInfo>,
    pub return_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParameterInfo {
    pub name: String,
    pub type_annotation: Option<String>,
    pub position: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeatureNode {
    pub id: String,
    pub name: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataflowNode {
    pub id: String,
    pub label: String,
    pub file: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BasicEdge {
    pub from: String,
    pub to: String,
    pub relation: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataflowEdge {
    pub from: String,
    pub to: String,
    pub relation: String,
    pub value_preview: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameworkView {
    pub framework: String,
    pub node_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextSnippet {
    pub node_id: String,
    pub file_path: PathBuf,
    pub snippet: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProjectIr {
    pub file_graph: Graph<FileNode, BasicEdge>,
    pub symbol_graph: Graph<SymbolNode, BasicEdge>,
    pub feature_graph: Graph<FeatureNode, BasicEdge>,
    pub dataflow_graph: Graph<DataflowNode, DataflowEdge>,
    pub framework_views: Vec<FrameworkView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProjectIrFragment {
    pub file_graph: Graph<FileNode, BasicEdge>,
    pub symbol_graph: Graph<SymbolNode, BasicEdge>,
    pub feature_graph: Graph<FeatureNode, BasicEdge>,
    pub dataflow_graph: Graph<DataflowNode, DataflowEdge>,
    pub framework_views: Vec<FrameworkView>,
}

impl ProjectIr {
    pub fn absorb(&mut self, fragment: ProjectIrFragment) {
        self.file_graph.nodes.extend(fragment.file_graph.nodes);
        self.file_graph.edges.extend(fragment.file_graph.edges);
        self.symbol_graph.nodes.extend(fragment.symbol_graph.nodes);
        self.symbol_graph.edges.extend(fragment.symbol_graph.edges);
        self.feature_graph
            .nodes
            .extend(fragment.feature_graph.nodes);
        self.feature_graph
            .edges
            .extend(fragment.feature_graph.edges);
        self.dataflow_graph
            .nodes
            .extend(fragment.dataflow_graph.nodes);
        self.dataflow_graph
            .edges
            .extend(fragment.dataflow_graph.edges);
        self.framework_views.extend(fragment.framework_views);
    }
}
