use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("services/")
        .parent()
        .expect("crates/")
        .parent()
        .expect("repo root")
        .to_path_buf()
}

fn read_file(root: &Path, relative: &str) -> String {
    std::fs::read_to_string(root.join(relative)).expect("read source file")
}

#[test]
fn role_bearing_callsites_use_role_aware_helper() {
    let root = repo_root();
    let migrated = [
        "crates/services/llm/src/evidence_gate.rs",
        "crates/engines/crypto/src/kani/scaffolder.rs",
        "crates/engines/distributed/src/harness/builder.rs",
        "crates/engines/distributed/src/economic/mod.rs",
        "crates/engines/lean/src/scaffold.rs",
        "crates/services/report/src/generator.rs",
    ];

    let mut violations = Vec::<String>::new();
    for relative in migrated {
        let text = read_file(&root, relative);
        if !text.contains("role_aware_llm_call(") {
            violations.push(format!("{relative}: missing role_aware_llm_call"));
        }
        if text.contains("llm_call_traced(") {
            violations.push(format!("{relative}: contains llm_call_traced"));
        }
    }

    assert!(
        violations.is_empty(),
        "role-aware migration violations:\n{}",
        violations.join("\n")
    );
}

#[test]
fn copilot_does_not_call_llm_call_traced_directly() {
    let root = repo_root();
    let path = "crates/services/llm/src/copilot.rs";
    let text = read_file(&root, path);
    assert!(
        !text.contains("llm_call_traced("),
        "{path} should route through the contract enforcer instead of direct llm_call_traced"
    );
}
