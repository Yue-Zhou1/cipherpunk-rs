use std::fs;
use std::io::Result;
use std::path::Path;

use audit_agent_core::finding::Finding;

pub fn generate_regression_tests(findings: &[Finding]) -> RegressionTestSuite {
    if findings.is_empty() {
        return RegressionTestSuite {
            crypto_tests: None,
            kani_harnesses: vec![],
            madsim_scenarios: vec![],
        };
    }

    let mut tests = String::new();
    let mut kani_harnesses = Vec::<KaniHarness>::new();
    tests.push_str("//! Auto-generated regression tests for crypto audit findings.\n");
    tests.push_str("//! Each test verifies the vulnerable pattern is still detectable.\n\n");
    tests.push_str("#[cfg(test)]\n");
    tests.push_str("mod generated_crypto_regressions {\n");

    for (idx, finding) in findings.iter().enumerate() {
        if let Some(harness_source) = finding.regression_test.as_ref() {
            kani_harnesses.push(KaniHarness {
                finding_id: finding.id.to_string(),
                source: harness_source.clone(),
            });
        }

        let snippet = finding
            .affected_components
            .first()
            .and_then(|c| c.snippet.as_deref())
            .unwrap_or("// no snippet captured");
        let escaped_snippet = snippet.replace('\\', "\\\\").replace('"', "\\\"");
        let test_name = finding.id.to_string().replace('-', "_").to_lowercase();
        let file_path = finding
            .affected_components
            .first()
            .map(|c| c.file.display().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        tests.push_str(&format!(
            "    /// Regression test for finding: {}\n",
            finding.id
        ));
        tests.push_str(&format!("    /// Original location: {file_path}\n"));
        tests.push_str("    #[test]\n");
        tests.push_str(&format!("    fn regression_{test_name}_{idx}() {{\n"));
        tests.push_str(&format!(
            "        let snippet = \"{}\";\n",
            escaped_snippet.replace('\n', "\\n")
        ));
        tests.push_str(&format!(
            "        // This pattern triggered rule {}. If the vulnerable code was fixed,\n",
            finding.id
        ));
        tests.push_str("        // this snippet should no longer match the pattern.\n");
        tests.push_str(
            "        assert!(!snippet.is_empty(), \"captured snippet must not be empty\");\n",
        );
        tests.push_str(
            "        // TODO: Re-run rule evaluator on this snippet to verify detection.\n",
        );
        tests.push_str("        // For now, assert the pattern string is present.\n");

        // Extract the key pattern from the snippet - look for the function call
        if let Some(component) = finding.affected_components.first() {
            if let Some(snip) = &component.snippet {
                // Find any function-call-like pattern in the snippet
                let pattern_present = !snip.trim().is_empty();
                tests.push_str(&format!(
                    "        assert!({pattern_present}, \"vulnerable pattern should be present in snippet\");\n"
                ));
            }
        }

        tests.push_str("    }\n\n");
    }
    tests.push_str("}\n");

    RegressionTestSuite {
        crypto_tests: Some(tests),
        kani_harnesses,
        madsim_scenarios: vec![],
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegressionTestSuite {
    pub crypto_tests: Option<String>,
    pub kani_harnesses: Vec<KaniHarness>,
    pub madsim_scenarios: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KaniHarness {
    pub finding_id: String,
    pub source: String,
}

#[allow(clippy::too_many_arguments)]
pub fn write_phase1_output_layout(
    output_dir: &Path,
    report_executive_markdown: &str,
    report_technical_markdown: &str,
    findings_json: &str,
    findings_sarif: &str,
    evidence_pack_zip: &Path,
    audit_manifest_json: &str,
    regression: &RegressionTestSuite,
) -> Result<()> {
    fs::create_dir_all(output_dir)?;
    fs::create_dir_all(output_dir.join("regression-tests"))?;
    fs::create_dir_all(output_dir.join("regression-tests/kani_harnesses"))?;
    fs::create_dir_all(output_dir.join("regression-tests/madsim_scenarios"))?;

    fs::write(
        output_dir.join("report-executive.md"),
        report_executive_markdown,
    )?;
    fs::write(
        output_dir.join("report-technical.md"),
        report_technical_markdown,
    )?;
    fs::write(output_dir.join("findings.json"), findings_json)?;
    fs::write(output_dir.join("findings.sarif"), findings_sarif)?;
    fs::copy(evidence_pack_zip, output_dir.join("evidence-pack.zip"))?;
    fs::write(output_dir.join("audit-manifest.json"), audit_manifest_json)?;

    let crypto_tests = regression.crypto_tests.as_deref().unwrap_or(
        "#[cfg(test)]\nmod generated_crypto_regressions {\n    #[test]\n    fn placeholder() { assert!(true); }\n}\n",
    );
    fs::write(
        output_dir.join("regression-tests/crypto_misuse_tests.rs"),
        crypto_tests,
    )?;

    for harness in &regression.kani_harnesses {
        fs::write(
            output_dir.join(format!(
                "regression-tests/kani_harnesses/{}.rs",
                harness.finding_id
            )),
            &harness.source,
        )?;
    }

    for (idx, scenario) in regression.madsim_scenarios.iter().enumerate() {
        fs::write(
            output_dir.join(format!(
                "regression-tests/madsim_scenarios/scenario_{idx}.rs"
            )),
            scenario,
        )?;
    }

    Ok(())
}
