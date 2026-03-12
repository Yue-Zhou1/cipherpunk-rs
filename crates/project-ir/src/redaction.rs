use crate::graph::DataflowEdge;

pub fn redact_dataflow(edges: &mut [DataflowEdge], allow_value_previews: bool) {
    if allow_value_previews {
        return;
    }
    for edge in edges {
        edge.value_preview = None;
    }
}
