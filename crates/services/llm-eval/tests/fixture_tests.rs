use std::fs;

use llm_eval::{TemplateFallbackSupport, load_fixtures_from_dir};

#[test]
fn loads_fixture_files_from_directory() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::write(
        temp.path().join("a.yaml"),
        r#"
- id: a
  role: Scaffolding
  prompt: hello
  template_fallback: required
  assertions:
    - type: MinChars
      value: 1
"#,
    )
    .expect("write fixture");

    let fixtures = load_fixtures_from_dir(temp.path()).expect("load fixtures");
    assert_eq!(fixtures.len(), 1);
    assert_eq!(fixtures[0].id, "a");
    assert_eq!(
        fixtures[0].template_fallback,
        TemplateFallbackSupport::Required
    );
}
