import type {
  CrateRecord,
  FindingRecord,
  OutputType,
  SourceMode,
  StepDefinition,
} from "../types";

export const STEPS: StepDefinition[] = [
  { id: 1, label: "Source", title: "Source Selection" },
  { id: 2, label: "Config", title: "Audit Configuration" },
  { id: 3, label: "Inputs", title: "Optional Inputs" },
  { id: 4, label: "Workspace", title: "Workspace Confirmation" },
  { id: 5, label: "Execute", title: "Live Execution" },
  { id: 6, label: "Results", title: "Audit Results" },
];

export const SOURCE_TAB_LABELS: Record<SourceMode, string> = {
  git: "Git URL",
  local: "Local Path",
  archive: "Upload Archive",
};

export const LLM_DEGRADE = [
  "kani::assume() search hints",
  "prose polish in reports",
  "economic attack descriptions",
];

export const WORKSPACE_CRATES: CrateRecord[] = [
  { name: "circuit-core", status: "in_scope" },
  { name: "test-utils", status: "excluded", reason: "dev-only" },
  { name: "bridge-adapter", status: "ambiguous" },
];

export const OUTPUT_BUTTONS: Array<{ type: OutputType; label: string }> = [
  { type: "executive_pdf", label: "Download Executive Report PDF" },
  { type: "technical_pdf", label: "Download Technical Report PDF" },
  { type: "evidence_pack_zip", label: "Download Evidence Pack ZIP" },
  { type: "findings_sarif", label: "Download findings.sarif" },
  { type: "findings_json", label: "Download findings.json" },
  { type: "regression_tests_zip", label: "Download Regression Tests ZIP" },
];

export const FINDINGS: FindingRecord[] = [
  {
    id: "F-ZK-0042",
    severity: "High",
    category: "Crypto Misuse",
    title: "canonicality check missing",
    framework: "Halo2",
    ruleId: "CRYPTO-003",
    verificationStatus: "Verified",
    llmGenerated: false,
    description:
      "Field deserialization path does not validate canonical form before constructing the in-memory value.",
    affected: "src/field.rs:142-158",
    recommendation:
      "Add canonical form validation before constructing the field element and reject non-canonical encodings.",
    codeSnippet: "142 | fn from_bytes(bytes: [u8; 32]) -> Self",
    reproduceScript: "#!/bin/sh\ncd evidence/F-ZK-0042\ncargo run -p harness",
    evidenceFiles: [
      { name: "harness.rs", content: "fn main() { /* proof harness */ }" },
      { name: "query.smt2", content: "; SMT query proving invalid canonical branch" },
      { name: "output.txt", content: "counterexample: bytes[7] = 0xff" },
    ],
    cdg: {
      nodes: [
        { id: "RangeCheck" },
        { id: "MainCircuit", risk: true },
        { id: "HashChip" },
      ],
      edges: [
        { from: "RangeCheck", to: "MainCircuit" },
        { from: "RangeCheck", to: "HashChip" },
      ],
    },
  },
  {
    id: "F-CR-009",
    severity: "Medium",
    category: "Crypto Misuse",
    title: "nonce reuse risk",
    framework: "Rust Crypto",
    ruleId: "CRYPTO-007",
    verificationStatus: "Verified",
    llmGenerated: false,
    description:
      "Nonce reuse in signing function can allow key leakage when signatures are observed across repeated messages.",
    affected: "src/signing.rs:88-109",
    recommendation: "Generate nonce per message using RFC6979 or a cryptographically secure random source.",
    codeSnippet: "88 | signer.sign_with_nonce(private_key, reused_nonce, message)",
    reproduceScript: "#!/bin/sh\ncd evidence/F-CR-009\ncargo test -p signer nonce_reuse",
    evidenceFiles: [
      { name: "reproduce.log", content: "recovered private key after nonce collision" },
      { name: "signing.rs", content: "pub fn sign_with_nonce(...) { ... }" },
    ],
  },
  {
    id: "ECON-001",
    severity: "Observation",
    category: "Economic",
    title: "tx ordering sensitivity",
    framework: "Economic",
    ruleId: "bridge.yaml",
    verificationStatus: "Unverified",
    llmGenerated: true,
    description:
      "Transaction ordering assumptions can break expected invariants in bridge settlement windows.",
    affected: "src/settlement.rs:220-244",
    recommendation:
      "Add explicit ordering constraints and replay-protection checks in settlement sequencing logic.",
    codeSnippet: "220 | if settle_window.is_open() { apply_ordered_path(); }",
    reproduceScript: "#!/bin/sh\ncd evidence/ECON-001\ncat notes.md",
    evidenceFiles: [
      { name: "notes.md", content: "Manual analyst note: ordering edge cases remain unproven." },
    ],
  },
  {
    id: "F-DS-002",
    severity: "Medium",
    category: "Distributed",
    title: "safety violation under partition",
    framework: "MadSim",
    ruleId: "DIST-002",
    verificationStatus: "Verified",
    llmGenerated: false,
    description:
      "Network partition scenario allows conflicting commits at the same height before recovery.",
    affected: "distributed/consensus.rs:301-377",
    recommendation:
      "Strengthen quorum intersection checks and delay commit on uncertain partition boundaries.",
    codeSnippet: "332 | if partitioned && quorum_partial { commit(block) }",
    reproduceScript: "#!/bin/sh\ncd evidence/F-DS-002\n./replay.sh --seed 0xDEADBEEF",
    evidenceFiles: [
      { name: "trace.json", content: "{\"seed\":\"0xDEADBEEF\",\"events\":[...] }" },
      { name: "replay.sh", content: "#!/bin/sh\n./target/debug/madsim --seed 0xDEADBEEF" },
    ],
    trace: {
      seed: "0xDEADBEEF",
      durationTicks: 10000,
      events: [
        { tick: 0, node: "node-0", event: "MessageSent { proposal: block-1 }" },
        { tick: 12, node: "node-1", event: "MessageReceived { from: node-0 }" },
        { tick: 100, node: "node-2", event: "DoubleVote { height: 50 }" },
        {
          tick: 101,
          node: "ALL",
          event: "SafetyInvariant VIOLATED",
          violation: true,
        },
      ],
      violationSummary: "node-0 committed A at height 50 while node-1 committed B at height 50.",
    },
  },
];
