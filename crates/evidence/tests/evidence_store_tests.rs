use std::process::Command;

use audit_agent_core::finding::FindingId;
use evidence::{EvidenceFile, EvidenceManifest, EvidencePack, EvidenceStore};
use tempfile::tempdir;

fn repo_digest(image: &str) -> (String, String) {
    let pull = Command::new("docker")
        .args(["pull", image])
        .output()
        .expect("docker pull should run");
    assert!(
        pull.status.success(),
        "docker pull failed: {}",
        String::from_utf8_lossy(&pull.stderr)
    );

    let output = Command::new("docker")
        .args(["inspect", "--format", "{{index .RepoDigests 0}}", image])
        .output()
        .expect("docker inspect should run");
    assert!(
        output.status.success(),
        "docker inspect failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let (repo, digest) = value
        .split_once('@')
        .expect("repo digest should include '@'");
    (repo.to_string(), digest.to_string())
}

fn sample_pack(image: &str, digest: &str, reproduce_cmd: &str) -> EvidencePack {
    EvidencePack {
        manifest: EvidenceManifest {
            finding_id: "F-ZK-0042".to_string(),
            title: "Example finding".to_string(),
            agent_version: "0.1.0".to_string(),
            source_commit: "a1b2c3d4".to_string(),
            source_content_hash: Some("sha256:content".to_string()),
            tool: "kani".to_string(),
            tool_version: "0.57.0".to_string(),
            container_image: image.to_string(),
            container_digest: digest.to_string(),
            reproduction_command: reproduce_cmd.to_string(),
            expected_output_description: "prints reproduced".to_string(),
            files: vec![],
        },
        files: vec![
            EvidenceFile::text("harness/src/lib.rs", "pub fn harness() {}\n"),
            EvidenceFile::text("harness/Cargo.toml", "[package]\nname=\"harness\"\n"),
            EvidenceFile::text("smt2/query.smt2", "(check-sat)\n"),
            EvidenceFile::text("smt2/output.txt", "sat\n"),
            EvidenceFile::text("traces/trace.json", "{\"events\":[]}\n"),
            EvidenceFile::text("traces/seed.txt", "1337\n"),
            EvidenceFile::text("traces/replay.sh", "#!/usr/bin/env bash\necho replay\n"),
            EvidenceFile::binary("corpus/input-0001", vec![0, 1, 2, 3]),
        ],
    }
}

#[tokio::test]
async fn save_and_load_round_trip_for_all_evidence_file_types() {
    let dir = tempdir().expect("tempdir");
    let store = EvidenceStore::new(dir.path());
    let finding_id = FindingId::new("F-ZK-0042");
    let pack = sample_pack("busybox", "sha256:dummy", "sh -lc 'echo reproduced'");

    store
        .save_pack(&finding_id, &pack)
        .await
        .expect("save pack");
    let manifest = store
        .load_manifest(&finding_id)
        .await
        .expect("load manifest");

    assert_eq!(manifest.finding_id, "F-ZK-0042");
    assert!(manifest.files.iter().any(|f| f == "manifest.json"));
    assert!(manifest.files.iter().any(|f| f == "reproduce.sh"));
    assert!(manifest.files.iter().any(|f| f == "harness/src/lib.rs"));
    assert!(manifest.files.iter().any(|f| f == "smt2/query.smt2"));
    assert!(manifest.files.iter().any(|f| f == "traces/trace.json"));
    assert!(manifest.files.iter().any(|f| f == "corpus/input-0001"));
}

#[tokio::test]
async fn reproduce_script_runs_successfully_in_clean_docker_environment() {
    let (image, digest) = repo_digest("busybox:1.36");
    let dir = tempdir().expect("tempdir");
    let store = EvidenceStore::new(dir.path());
    let finding_id = FindingId::new("F-ZK-0042");
    let pack = sample_pack(
        &image,
        &digest,
        "sh -lc 'test -f /evidence/manifest.json && echo reproduced'",
    );

    store
        .save_pack(&finding_id, &pack)
        .await
        .expect("save pack");
    let script = store
        .generate_reproduce_script(&finding_id)
        .await
        .expect("generate script");
    assert!(script.contains("--network none"));
    assert!(script.contains("prints reproduced"));

    let script_path = dir.path().join("F-ZK-0042").join("reproduce.sh");
    let output = Command::new("env")
        .args([
            "-i",
            "PATH=/usr/local/bin:/usr/bin:/bin",
            "bash",
            script_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("run reproduce.sh");
    assert!(
        output.status.success(),
        "reproduce failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("reproduced"),
        "expected output in stdout: {stdout}"
    );
}

#[tokio::test]
async fn export_zip_file_count_matches_manifest_files_list() {
    let dir = tempdir().expect("tempdir");
    let store = EvidenceStore::new(dir.path());
    let finding_id = FindingId::new("F-ZK-0042");
    let pack = sample_pack("busybox", "sha256:dummy", "sh -lc 'echo reproduced'");
    store
        .save_pack(&finding_id, &pack)
        .await
        .expect("save pack");

    let zip_path = dir.path().join("evidence-pack.zip");
    store
        .export_zip(std::slice::from_ref(&finding_id), &zip_path)
        .await
        .expect("export zip");

    let manifest = store
        .load_manifest(&finding_id)
        .await
        .expect("load manifest");
    let zip_file = std::fs::File::open(&zip_path).expect("open zip");
    let mut archive = zip::ZipArchive::new(zip_file).expect("read zip");
    let prefix = format!("{finding_id}/");
    let mut zip_count = 0usize;
    for idx in 0..archive.len() {
        let entry = archive.by_index(idx).expect("zip entry");
        if entry.name().starts_with(&prefix) {
            zip_count += 1;
        }
    }
    assert_eq!(zip_count, manifest.files.len());
}

#[tokio::test]
async fn export_zip_with_multiple_findings_creates_single_archive_with_all_findings() {
    let dir = tempdir().expect("tempdir");
    let store = EvidenceStore::new(dir.path());
    let finding_a = FindingId::new("F-ZK-0042");
    let finding_b = FindingId::new("F-ZK-0099");

    let mut pack_a = sample_pack("busybox", "sha256:dummy-a", "sh -lc 'echo a'");
    pack_a.manifest.finding_id = "F-ZK-0042".to_string();
    let mut pack_b = sample_pack("busybox", "sha256:dummy-b", "sh -lc 'echo b'");
    pack_b.manifest.finding_id = "F-ZK-0099".to_string();

    store
        .save_pack(&finding_a, &pack_a)
        .await
        .expect("save pack a");
    store
        .save_pack(&finding_b, &pack_b)
        .await
        .expect("save pack b");

    let zip_path = dir.path().join("evidence-pack-multi.zip");
    store
        .export_zip(&[finding_a.clone(), finding_b.clone()], &zip_path)
        .await
        .expect("export zip");
    assert!(zip_path.exists(), "zip archive should exist");

    let zip_file = std::fs::File::open(&zip_path).expect("open zip");
    let mut archive = zip::ZipArchive::new(zip_file).expect("read zip");
    let mut has_a = false;
    let mut has_b = false;
    for idx in 0..archive.len() {
        let entry = archive.by_index(idx).expect("zip entry");
        if entry.name().starts_with("F-ZK-0042/") {
            has_a = true;
        }
        if entry.name().starts_with("F-ZK-0099/") {
            has_b = true;
        }
    }
    assert!(has_a, "missing entries for finding A");
    assert!(has_b, "missing entries for finding B");
}
