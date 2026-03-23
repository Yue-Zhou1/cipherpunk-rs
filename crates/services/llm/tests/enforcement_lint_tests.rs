use std::path::PathBuf;

use syn::visit::Visit;

struct ParseJsonContractVisitor {
    found_call: bool,
}

impl<'ast> Visit<'ast> for ParseJsonContractVisitor {
    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        if is_parse_json_contract_call(&node.func) {
            self.found_call = true;
        }
        syn::visit::visit_expr_call(self, node);
    }
}

fn is_parse_json_contract_call(func: &syn::Expr) -> bool {
    match func {
        syn::Expr::Path(path_expr) => path_expr
            .path
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "parse_json_contract"),
        _ => false,
    }
}

#[test]
fn forbid_parse_json_contract_calls_outside_enforcement_layer() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("services/")
        .parent()
        .expect("crates/")
        .parent()
        .expect("repo root")
        .to_path_buf();

    let mut violations = Vec::<String>::new();
    for entry in walkdir::WalkDir::new(root.join("crates"))
        .into_iter()
        .filter_map(|value| value.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        if entry.path().extension().and_then(|value| value.to_str()) != Some("rs") {
            continue;
        }

        let path = entry.path();
        let normalized = path.to_string_lossy().replace('\\', "/");
        let is_allowed_path = normalized.ends_with("/crates/services/llm/src/sanitize.rs")
            || normalized.ends_with("/crates/services/llm/src/enforcement.rs")
            || normalized.contains("/crates/services/llm/tests/");

        let text = std::fs::read_to_string(path).expect("read source");
        let syntax = syn::parse_file(&text)
            .unwrap_or_else(|err| panic!("failed to parse {}: {err}", path.display()));
        let mut visitor = ParseJsonContractVisitor { found_call: false };
        visitor.visit_file(&syntax);

        if visitor.found_call && !is_allowed_path {
            violations.push(path.display().to_string());
        }
    }

    assert!(
        violations.is_empty(),
        "parse_json_contract call(s) found outside enforcement layer:\n{}",
        violations.join("\n")
    );
}
