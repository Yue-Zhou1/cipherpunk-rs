import type {
  ExplorerEdge,
  ExplorerGraph,
  ExplorerNode,
  FunctionSignature,
  ParameterInfo,
} from "../types";

type ModuleSeed = {
  crateName: string;
  moduleName: string;
  files: string[];
};

function crateId(crateName: string): string {
  return `module:crates/${crateName}`;
}

function moduleId(crateName: string, moduleName: string): string {
  return `module:crates/${crateName}/src/${moduleName}`;
}

function filePath(crateName: string, moduleName: string, fileName: string): string {
  return `crates/${crateName}/src/${moduleName}/${fileName}`;
}

function fileId(path: string): string {
  return `file:${path}`;
}

function symbolId(path: string, functionName: string): string {
  return `symbol:${path}::${functionName}`;
}

function makeSignature(
  params: Array<[string, string]>,
  returnType?: string
): FunctionSignature {
  const parameters: ParameterInfo[] = params.map(([name, typeAnnotation], position) => ({
    name,
    typeAnnotation,
    position,
  }));

  return {
    parameters,
    returnType,
  };
}

function signatureFor(functionName: string, index: number): FunctionSignature {
  switch (functionName) {
    case "entry_point":
      return makeSignature(
        [
          ["payload", "&[u8]"],
          ["sig", "&Signature"],
          ["pubkey", "&PublicKey"],
        ],
        "Result<AuditRequest>"
      );
    case "validate_submission":
      return makeSignature([["request", "&AuditRequest"]], "Result<ValidatedRequest>");
    case "verify_signature":
      return makeSignature(
        [
          ["msg", "&[u8]"],
          ["sig", "&Signature"],
          ["pubkey", "&PublicKey"],
        ],
        "Result<bool>"
      );
    case "keccak256":
      return makeSignature([["input", "&[u8]"]], "[u8; 32]");
    case "parse_submission":
    case "parse_packet":
    case "parse_template":
    case "parse_model_response":
      return makeSignature([["raw", "&str"]], "Result<ParsedValue>");
    case "hash_blake3":
      return makeSignature([["input", "&[u8]"]], "Hash");
    case "detect_reentrancy":
    case "detect_overflow":
    case "detect_auth_bypass":
      return makeSignature([["ir", "&ProjectIr"]], "Vec<Finding>");
    case "build_prompt":
      return makeSignature(
        [
          ["finding", "&FindingSummary"],
          ["template", "&PromptTemplate"],
        ],
        "PromptRequest"
      );
    default: {
      if (functionName.startsWith("verify_")) {
        return makeSignature([["candidate", "&VerificationInput"]], "Result<bool>");
      }
      if (functionName.startsWith("hash_")) {
        return makeSignature([["bytes", "&[u8]"]], "Hash");
      }
      if (functionName.startsWith("parse_")) {
        return makeSignature([["source", "&str"]], "Result<ParsedValue>");
      }
      if (functionName.startsWith("render_")) {
        return makeSignature([["report", "&AuditReport"]], "String");
      }
      if (functionName.startsWith("helper_")) {
        return makeSignature([["ctx", "&ExplorerContext"]], "Result<()> ");
      }
      if (functionName.startsWith("retry_")) {
        return makeSignature([["attempt", "u32"], ["ctx", "&RetryContext"]], "Result<()> ");
      }
      if (functionName.startsWith("enqueue_")) {
        return makeSignature([["job", "&ValidatedRequest"]], "Result<JobId>");
      }
      return makeSignature(
        [
          ["ctx", "&ExecutionContext"],
          ["input", "&[u8]"],
        ],
        index % 2 === 0 ? "Result<()> " : "Result<usize>"
      );
    }
  }
}

function defaultPrimaryFunctionName(fileName: string): string {
  const stem = fileName.replace(/\.rs$/, "").replace(/[^a-zA-Z0-9]+/g, "_").toLowerCase();
  return `process_${stem}`;
}

function defaultSecondaryFunctionName(fileName: string, index: number): string {
  const stem = fileName.replace(/\.rs$/, "").replace(/[^a-zA-Z0-9]+/g, "_").toLowerCase();
  return `helper_${stem}_${(index % 3) + 1}`;
}

function createHierarchy(
  seeds: ModuleSeed[]
): {
  nodes: ExplorerNode[];
  edges: ExplorerEdge[];
  filePaths: string[];
  fileIdByPath: Map<string, string>;
} {
  const nodes: ExplorerNode[] = [];
  const edges: ExplorerEdge[] = [];
  const filePaths: string[] = [];
  const fileIdByPath = new Map<string, string>();
  const crateNames = [...new Set(seeds.map((seed) => seed.crateName))];

  for (const crateName of crateNames) {
    nodes.push({
      id: crateId(crateName),
      label: crateName,
      kind: "crate",
      filePath: `crates/${crateName}`,
      line: 1,
    });
  }

  for (const seed of seeds) {
    const mId = moduleId(seed.crateName, seed.moduleName);
    nodes.push({
      id: mId,
      label: seed.moduleName,
      kind: "module",
      filePath: `crates/${seed.crateName}/src/${seed.moduleName}`,
      line: 1,
    });
    edges.push({
      from: crateId(seed.crateName),
      to: mId,
      relation: "contains",
    });

    for (const name of seed.files) {
      const path = filePath(seed.crateName, seed.moduleName, name);
      const fId = fileId(path);
      filePaths.push(path);
      fileIdByPath.set(path, fId);

      nodes.push({
        id: fId,
        label: name,
        kind: "file",
        filePath: path,
        line: 1,
      });

      edges.push({
        from: mId,
        to: fId,
        relation: "contains",
      });
    }
  }

  return { nodes, edges, filePaths, fileIdByPath };
}

function appendSymbol(
  nodes: ExplorerNode[],
  edges: ExplorerEdge[],
  symbolIndex: Map<string, string>,
  fileIdByPath: Map<string, string>,
  path: string,
  functionName: string,
  signatureIndex: number,
  linkFromFile: boolean
): void {
  const id = symbolId(path, functionName);
  nodes.push({
    id,
    label: functionName,
    kind: "function",
    filePath: path,
    line: 10 + signatureIndex * 3,
    signature: signatureFor(functionName, signatureIndex),
  });
  symbolIndex.set(`${path}::${functionName}`, id);

  if (linkFromFile) {
    const fId = fileIdByPath.get(path);
    if (fId) {
      edges.push({
        from: fId,
        to: id,
        relation: "contains",
      });
    }
  }
}

function appendSymbolEdge(
  edges: ExplorerEdge[],
  symbolIndex: Map<string, string>,
  fromPath: string,
  fromFn: string,
  toPath: string,
  toFn: string,
  relation: ExplorerEdge["relation"],
  extra: Partial<ExplorerEdge> = {}
): void {
  const from = symbolIndex.get(`${fromPath}::${fromFn}`);
  const to = symbolIndex.get(`${toPath}::${toFn}`);
  if (!from || !to) {
    return;
  }
  edges.push({
    from,
    to,
    relation,
    ...extra,
  });
}

function createMediumFixture(): ExplorerGraph {
  const seeds: ModuleSeed[] = [
    {
      crateName: "intake",
      moduleName: "api",
      files: ["routes.rs", "controller.rs", "auth.rs", "telemetry.rs", "dto.rs"],
    },
    {
      crateName: "intake",
      moduleName: "parsers",
      files: ["validator.rs", "decode.rs", "normalize.rs", "schema.rs"],
    },
    {
      crateName: "intake",
      moduleName: "ingest",
      files: ["queue.rs", "store.rs", "ingest.rs", "retry.rs"],
    },
    {
      crateName: "engine-crypto",
      moduleName: "signature",
      files: ["verify.rs", "sign.rs", "recover.rs", "batch.rs", "keyring.rs"],
    },
    {
      crateName: "engine-crypto",
      moduleName: "hash",
      files: ["keccak.rs", "blake3.rs", "merkle.rs", "domain.rs"],
    },
    {
      crateName: "engine-crypto",
      moduleName: "random",
      files: ["nonce.rs", "rng.rs", "entropy.rs", "drbg.rs"],
    },
    {
      crateName: "engine-distributed",
      moduleName: "consensus",
      files: ["leader.rs", "vote.rs", "commit.rs", "round.rs"],
    },
    {
      crateName: "engine-distributed",
      moduleName: "network",
      files: ["transport.rs", "gossip.rs", "peer_set.rs", "codec.rs", "retry.rs"],
    },
    {
      crateName: "findings",
      moduleName: "detectors",
      files: ["reentrancy.rs", "overflow.rs", "auth.rs", "config.rs"],
    },
    {
      crateName: "findings",
      moduleName: "reporting",
      files: ["format.rs", "markdown.rs", "json.rs", "summary.rs"],
    },
    {
      crateName: "llm",
      moduleName: "prompts",
      files: ["builder.rs", "sanitizer.rs", "templates.rs"],
    },
    {
      crateName: "llm",
      moduleName: "orchestrator",
      files: ["planner.rs", "dispatcher.rs", "context.rs", "response.rs"],
    },
  ];

  const { nodes, edges, filePaths, fileIdByPath } = createHierarchy(seeds);
  const symbolIndex = new Map<string, string>();

  const primaryByPath: Record<string, string> = {
    "crates/intake/src/api/routes.rs": "entry_point",
    "crates/intake/src/api/controller.rs": "handle_submission",
    "crates/intake/src/api/auth.rs": "verify_request_auth",
    "crates/intake/src/api/telemetry.rs": "record_ingest_metrics",
    "crates/intake/src/api/dto.rs": "parse_request_dto",
    "crates/intake/src/parsers/validator.rs": "validate_submission",
    "crates/intake/src/parsers/decode.rs": "parse_submission",
    "crates/intake/src/parsers/normalize.rs": "normalize_request",
    "crates/intake/src/parsers/schema.rs": "validate_schema",
    "crates/intake/src/ingest/queue.rs": "enqueue_submission",
    "crates/intake/src/ingest/store.rs": "persist_submission",
    "crates/intake/src/ingest/ingest.rs": "run_ingest_pipeline",
    "crates/intake/src/ingest/retry.rs": "retry_ingest",
    "crates/engine-crypto/src/signature/verify.rs": "verify_signature",
    "crates/engine-crypto/src/signature/sign.rs": "sign_message",
    "crates/engine-crypto/src/signature/recover.rs": "recover_signer",
    "crates/engine-crypto/src/signature/batch.rs": "batch_verify_signatures",
    "crates/engine-crypto/src/signature/keyring.rs": "load_public_key",
    "crates/engine-crypto/src/hash/keccak.rs": "keccak256",
    "crates/engine-crypto/src/hash/blake3.rs": "hash_blake3",
    "crates/engine-crypto/src/hash/merkle.rs": "build_merkle_root",
    "crates/engine-crypto/src/hash/domain.rs": "domain_separator",
    "crates/engine-crypto/src/random/nonce.rs": "next_nonce",
    "crates/engine-crypto/src/random/rng.rs": "fill_random_bytes",
    "crates/engine-crypto/src/random/entropy.rs": "collect_entropy",
    "crates/engine-crypto/src/random/drbg.rs": "reseed_drbg",
    "crates/engine-distributed/src/consensus/leader.rs": "elect_leader",
    "crates/engine-distributed/src/consensus/vote.rs": "verify_vote",
    "crates/engine-distributed/src/consensus/commit.rs": "commit_round",
    "crates/engine-distributed/src/consensus/round.rs": "advance_round",
    "crates/engine-distributed/src/network/transport.rs": "send_message",
    "crates/engine-distributed/src/network/gossip.rs": "broadcast_vote",
    "crates/engine-distributed/src/network/peer_set.rs": "select_peers",
    "crates/engine-distributed/src/network/codec.rs": "parse_packet",
    "crates/engine-distributed/src/network/retry.rs": "retry_send",
    "crates/findings/src/detectors/reentrancy.rs": "detect_reentrancy",
    "crates/findings/src/detectors/overflow.rs": "detect_overflow",
    "crates/findings/src/detectors/auth.rs": "detect_auth_bypass",
    "crates/findings/src/detectors/config.rs": "load_detection_config",
    "crates/findings/src/reporting/format.rs": "format_finding",
    "crates/findings/src/reporting/markdown.rs": "render_markdown_report",
    "crates/findings/src/reporting/json.rs": "render_json_report",
    "crates/findings/src/reporting/summary.rs": "report_summary",
    "crates/llm/src/prompts/builder.rs": "build_prompt",
    "crates/llm/src/prompts/sanitizer.rs": "sanitize_prompt",
    "crates/llm/src/prompts/templates.rs": "parse_template",
    "crates/llm/src/orchestrator/planner.rs": "plan_analysis",
    "crates/llm/src/orchestrator/dispatcher.rs": "dispatch_plan",
    "crates/llm/src/orchestrator/context.rs": "build_context_window",
    "crates/llm/src/orchestrator/response.rs": "parse_model_response",
  };

  for (const [index, path] of filePaths.entries()) {
    const fileName = path.slice(path.lastIndexOf("/") + 1);
    const functionName = primaryByPath[path] ?? defaultPrimaryFunctionName(fileName);
    appendSymbol(nodes, edges, symbolIndex, fileIdByPath, path, functionName, index, true);
  }

  for (let index = 0; index < 30; index += 1) {
    const path = filePaths[index];
    const fileName = path.slice(path.lastIndexOf("/") + 1);
    const functionName = defaultSecondaryFunctionName(fileName, index);
    appendSymbol(nodes, edges, symbolIndex, fileIdByPath, path, functionName, 100 + index, false);
  }

  const p = {
    entry: "crates/intake/src/api/routes.rs",
    validator: "crates/intake/src/parsers/validator.rs",
    decode: "crates/intake/src/parsers/decode.rs",
    normalize: "crates/intake/src/parsers/normalize.rs",
    queue: "crates/intake/src/ingest/queue.rs",
    verify: "crates/engine-crypto/src/signature/verify.rs",
    keyring: "crates/engine-crypto/src/signature/keyring.rs",
    keccak: "crates/engine-crypto/src/hash/keccak.rs",
    vote: "crates/engine-distributed/src/consensus/vote.rs",
    commit: "crates/engine-distributed/src/consensus/commit.rs",
    gossip: "crates/engine-distributed/src/network/gossip.rs",
    summary: "crates/findings/src/reporting/summary.rs",
    format: "crates/findings/src/reporting/format.rs",
    markdown: "crates/findings/src/reporting/markdown.rs",
    reentrancy: "crates/findings/src/detectors/reentrancy.rs",
    overflow: "crates/findings/src/detectors/overflow.rs",
    promptBuilder: "crates/llm/src/prompts/builder.rs",
    promptSanitizer: "crates/llm/src/prompts/sanitizer.rs",
    promptTemplate: "crates/llm/src/prompts/templates.rs",
    dispatch: "crates/llm/src/orchestrator/dispatcher.rs",
    response: "crates/llm/src/orchestrator/response.rs",
  };

  const callEdges: Array<[string, string, string, string]> = [
    [p.entry, "entry_point", p.decode, "parse_submission"],
    [p.entry, "entry_point", p.validator, "validate_submission"],
    [p.validator, "validate_submission", p.normalize, "normalize_request"],
    [p.validator, "validate_submission", p.verify, "verify_signature"],
    [p.verify, "verify_signature", p.keccak, "keccak256"],
    [p.verify, "verify_signature", p.keyring, "load_public_key"],
    [p.normalize, "normalize_request", p.queue, "enqueue_submission"],
    [p.queue, "enqueue_submission", p.gossip, "broadcast_vote"],
    [p.gossip, "broadcast_vote", p.vote, "verify_vote"],
    [p.vote, "verify_vote", p.commit, "commit_round"],
    [p.commit, "commit_round", p.summary, "report_summary"],
    [p.reentrancy, "detect_reentrancy", p.format, "format_finding"],
    [p.overflow, "detect_overflow", p.format, "format_finding"],
    [p.summary, "report_summary", p.promptBuilder, "build_prompt"],
    [p.promptSanitizer, "sanitize_prompt", p.promptBuilder, "build_prompt"],
    [p.promptBuilder, "build_prompt", p.dispatch, "dispatch_plan"],
    [p.dispatch, "dispatch_plan", p.markdown, "render_markdown_report"],
    [p.dispatch, "dispatch_plan", p.response, "parse_model_response"],
    [p.promptTemplate, "parse_template", p.promptBuilder, "build_prompt"],
  ];

  for (const [fromPath, fromFn, toPath, toFn] of callEdges) {
    appendSymbolEdge(edges, symbolIndex, fromPath, fromFn, toPath, toFn, "calls");
  }

  const paramFlowEdges: Array<
    [string, string, string, string, string, number, string | undefined]
  > = [
    [p.entry, "entry_point", p.validator, "validate_submission", "request", 0, "AuditRequest"],
    [p.decode, "parse_submission", p.validator, "validate_submission", "request", 0, "DecodedPayload"],
    [p.entry, "entry_point", p.verify, "verify_signature", "msg", 0, "payload bytes"],
    [p.entry, "entry_point", p.verify, "verify_signature", "sig", 1, "sig bytes"],
    [p.entry, "entry_point", p.verify, "verify_signature", "pubkey", 2, "caller pubkey"],
    [p.validator, "validate_submission", p.verify, "verify_signature", "msg", 0, "normalized msg"],
    [p.verify, "verify_signature", p.keccak, "keccak256", "input", 0, "message digest input"],
    [p.promptTemplate, "parse_template", p.promptBuilder, "build_prompt", "template", 1, "sanitized template"],
    [p.summary, "report_summary", p.promptBuilder, "build_prompt", "finding", 0, "summary finding"],
    [p.promptSanitizer, "sanitize_prompt", p.promptBuilder, "build_prompt", "template", 1, "safe template"],
    [p.queue, "enqueue_submission", p.gossip, "broadcast_vote", "ctx", 0, undefined],
    [p.gossip, "broadcast_vote", p.vote, "verify_vote", "candidate", 0, undefined],
  ];

  for (const [fromPath, fromFn, toPath, toFn, parameterName, parameterPosition, valuePreview] of paramFlowEdges) {
    appendSymbolEdge(edges, symbolIndex, fromPath, fromFn, toPath, toFn, "parameter_flow", {
      parameterName,
      parameterPosition,
      valuePreview,
    });
  }

  const returnFlowEdges: Array<[string, string, string, string, string | undefined]> = [
    [p.keccak, "keccak256", p.verify, "verify_signature", "digest"],
    [p.verify, "verify_signature", p.validator, "validate_submission", "is_valid"],
    [p.validator, "validate_submission", p.entry, "entry_point", "validated request"],
    [p.reentrancy, "detect_reentrancy", p.format, "format_finding", "finding list"],
    [p.overflow, "detect_overflow", p.format, "format_finding", "finding list"],
    [p.summary, "report_summary", p.promptBuilder, "build_prompt", "finding summary"],
    [p.promptBuilder, "build_prompt", p.dispatch, "dispatch_plan", "prompt request"],
    [p.dispatch, "dispatch_plan", p.markdown, "render_markdown_report", "render plan"],
    [p.response, "parse_model_response", p.summary, "report_summary", "llm response"],
  ];

  for (const [fromPath, fromFn, toPath, toFn, valuePreview] of returnFlowEdges) {
    appendSymbolEdge(edges, symbolIndex, fromPath, fromFn, toPath, toFn, "return_flow", {
      valuePreview,
    });
  }

  appendSymbolEdge(edges, symbolIndex, p.vote, "verify_vote", p.commit, "commit_round", "cfg");
  appendSymbolEdge(edges, symbolIndex, p.dispatch, "dispatch_plan", p.markdown, "render_markdown_report", "cfg");
  appendSymbolEdge(edges, symbolIndex, p.dispatch, "dispatch_plan", p.response, "parse_model_response", "cfg");

  return { nodes, edges };
}

function createSmallFixture(): ExplorerGraph {
  const seeds: ModuleSeed[] = [
    {
      crateName: "sandbox",
      moduleName: "parser",
      files: ["lexer.rs", "ast.rs", "decoder.rs", "schema.rs", "query.rs"],
    },
    {
      crateName: "sandbox",
      moduleName: "crypto",
      files: ["verify.rs", "hash.rs", "merkle.rs", "signature.rs", "nonce.rs"],
    },
    {
      crateName: "sandbox",
      moduleName: "io",
      files: ["input.rs", "output.rs", "transport.rs", "cache.rs", "state.rs"],
    },
  ];

  const { nodes, edges, filePaths, fileIdByPath } = createHierarchy(seeds);
  const symbolIndex = new Map<string, string>();
  const primaryNames = [
    "parse_tokens",
    "build_ast",
    "decode_payload",
    "validate_shape",
    "parse_query",
    "verify_signature",
    "hash_message",
    "build_tree",
    "sign_payload",
    "next_nonce",
    "read_input",
    "write_output",
    "send_packet",
    "cache_lookup",
    "hydrate_state",
  ];

  for (const [index, path] of filePaths.entries()) {
    appendSymbol(
      nodes,
      edges,
      symbolIndex,
      fileIdByPath,
      path,
      primaryNames[index] ?? defaultPrimaryFunctionName(path),
      index,
      true
    );
  }

  const parser = "crates/sandbox/src/parser/decoder.rs";
  const verify = "crates/sandbox/src/crypto/verify.rs";
  const hash = "crates/sandbox/src/crypto/hash.rs";
  const state = "crates/sandbox/src/io/state.rs";

  appendSymbolEdge(edges, symbolIndex, parser, "decode_payload", verify, "verify_signature", "calls");
  appendSymbolEdge(edges, symbolIndex, verify, "verify_signature", hash, "hash_message", "calls");
  appendSymbolEdge(edges, symbolIndex, hash, "hash_message", state, "hydrate_state", "calls");
  appendSymbolEdge(
    edges,
    symbolIndex,
    parser,
    "decode_payload",
    verify,
    "verify_signature",
    "parameter_flow",
    {
      parameterName: "msg",
      parameterPosition: 0,
      valuePreview: "decoded bytes",
    }
  );

  appendSymbolEdge(edges, symbolIndex, verify, "verify_signature", state, "hydrate_state", "return_flow", {
    valuePreview: "verification result",
  });

  return { nodes, edges };
}

function createLargeFixture(): ExplorerGraph {
  const crateNames = [
    "alpha",
    "beta",
    "gamma",
    "delta",
    "epsilon",
    "zeta",
    "eta",
    "theta",
  ];
  const moduleNames = ["api", "core", "storage", "crypto", "network"];

  const seeds: ModuleSeed[] = [];
  for (const crateName of crateNames) {
    for (const moduleName of moduleNames) {
      const files = Array.from({ length: 5 }, (_, index) => `unit_${index + 1}.rs`);
      seeds.push({ crateName, moduleName, files });
    }
  }

  const { nodes, edges, filePaths, fileIdByPath } = createHierarchy(seeds);
  const symbolIndex = new Map<string, string>();
  const moduleFirstSymbols: string[] = [];

  for (const seed of seeds) {
    const path = filePath(seed.crateName, seed.moduleName, "unit_1.rs");
    const functionName = `analyze_${seed.moduleName}_${seed.crateName}`;
    appendSymbol(nodes, edges, symbolIndex, fileIdByPath, path, functionName, moduleFirstSymbols.length, true);
    moduleFirstSymbols.push(`${path}::${functionName}`);
  }

  for (let index = 1; index < moduleFirstSymbols.length; index += 1) {
    const previous = moduleFirstSymbols[index - 1];
    const current = moduleFirstSymbols[index];
    const [fromPath, fromFn] = previous.split("::");
    const [toPath, toFn] = current.split("::");
    appendSymbolEdge(edges, symbolIndex, fromPath, fromFn, toPath, toFn, "calls");
  }

  for (let index = 0; index < 12; index += 1) {
    const from = moduleFirstSymbols[index];
    const to = moduleFirstSymbols[index + 1];
    if (!from || !to) {
      continue;
    }
    const [fromPath, fromFn] = from.split("::");
    const [toPath, toFn] = to.split("::");
    appendSymbolEdge(edges, symbolIndex, fromPath, fromFn, toPath, toFn, "parameter_flow", {
      parameterName: "ctx",
      parameterPosition: 0,
    });
  }

  const rootFile = filePaths[0];
  const terminalFile = filePaths[filePaths.length - 1];
  appendSymbol(nodes, edges, symbolIndex, fileIdByPath, rootFile, "verify_global_state", 500, true);
  appendSymbol(nodes, edges, symbolIndex, fileIdByPath, terminalFile, "hash_terminal_state", 501, true);
  appendSymbolEdge(
    edges,
    symbolIndex,
    rootFile,
    "verify_global_state",
    terminalFile,
    "hash_terminal_state",
    "return_flow",
    { valuePreview: "global digest" }
  );

  return { nodes, edges };
}

export const smallFixture: ExplorerGraph = createSmallFixture();
export const mediumFixture: ExplorerGraph = createMediumFixture();
export const largeFixture: ExplorerGraph = createLargeFixture();
