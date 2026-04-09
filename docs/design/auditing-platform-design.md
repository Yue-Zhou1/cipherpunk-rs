# Interactive Auditing Platform — Product Design Document

**Version:** 0.5 (Internal Draft)  
**Status:** Design Phase  
**Audience:** Core engineering team, internal security researchers  
**Changelog:**  
- v0.5 — Committed minimal v1 archetype set; clarified "Explain this contract" as v1 orientation agent; added agent evaluation infrastructure; specified IR abstraction boundary for multi-language extension; tightened path enumeration UX, search filters, playbook degradation, and feasibility estimates  
- v0.4 — Narrowed v1 to a Solidity-first solo workflow; added operational modes, platform trust model, context pipeline details, report assembly/export, and clearer verification semantics  
- v0.1 — Initial design philosophy, capabilities, UX, collaboration model  
- v0.2 — Added AI Agent Architecture (Section 9) and Verification System (Section 10)  
- v0.3 — Aligned Sections 1, 3, 5.2, and 8 with four-mode agent architecture; corrected inaccurate framing of AI as purely auditor-invoked

---

## 1. Background & Motivation

### The Problem with Current AI Auditors

The dominant AI auditing tools today — autonomous pipeline systems like Plamen and CertiK AI Auditor — share a fundamental design assumption: the AI drives the investigation, the human reviews the output. The auditor is a consumer of findings, not a participant in discovery.

This model has two structural weaknesses:

**It is a black box.** The reasoning path from codebase to finding is opaque. The auditor receives a severity-ranked list but has no visibility into what was examined, what was ruled out, or why a particular path was flagged. Verifying the output requires re-doing the work.

**It ignores auditor cognition.** Expert security engineers do not audit linearly or exhaustively — they form suspicions, pull threads, test hypotheses, and build mental models iteratively. An autonomous pipeline forces a batch, non-interactive workflow onto a process that is fundamentally exploratory and human-driven.

### The Current Collaboration Reality

A typical 3-person audit team operates like this: three people read the same codebase in parallel, in their own editors, with their own personal note-taking systems. Every 2–3 days they sync on a call, verbally describe what they found, try to figure out if two people independently found the same thing, and slowly merge into a shared document.

There is no shared representation of the investigation in progress. There is only a shared representation of the output — the final report. Everything that happened before that — the suspicions, the dead ends, the partial paths, the "I looked at this and it seemed fine" — is invisible to teammates and lost after the engagement ends.

Three things happen on every engagement as a result:

- **Duplicate work:** Two engineers independently trace the same call path, neither knowing the other did it.
- **Lost weak signals:** Engineer A notices something odd but doesn't write it up. Engineer B has context that would make it critical. They never connect these.
- **Knowledge stratification:** The senior's mental model of the system — the most valuable artifact produced during the engagement — never gets transferred to juniors in a structured way.

### The Opportunity

This platform is built around a different design assumption: **the human drives the investigation, and the AI participates at multiple levels of initiative — from explicit oracle on demand, to ambient observer surfacing connections, to proactive hint when confidence is high enough.** The investigation itself becomes a shared object — not just the output.

---

## 2. Target Users

**Primary:** Security engineers at audit firms working in teams of 2–4 on smart contract and L2 infrastructure engagements. Expected experience range: junior (1–2 YOE) to senior (5+ YOE).

**Secondary:** Independent security researchers doing solo audits or competitive audit contests (Code4rena, Sherlock, Cantina).

**Initial deployment:** Internal use by the founding team. External release gated on internal validation.

### Initial Product Boundary (v1)

The long-term design target is the full four-mode, multi-language platform described in this document. The initial product, however, must validate the core workflow with a deliberately narrow slice.

**v1 target:**

- Solidity / EVM codebases only
- Solo auditor workflow, local-first by default
- DAG canvas with contract/function/state views, structural filters, and suspicion markers
- Mode 1 Reactive Oracle with three starting agent types: State Invariant Checker, Caller Trust Analyzer, Side Effect Tracer — plus a lightweight "Explain this contract" orientation agent (not a full typed agent; a single system prompt variant for orientation context)
- Minimal archetype set: one protocol archetype (ERC-4626 vault) plus two structural overlays (proxy/upgrade pattern, ERC-20 token interface) — sufficient to validate delta highlighting, checklist overlays, and layered risk annotation without pretending every v1 match defines a full-system coverage baseline
- Session persistence, evidence tagging, Foundry subprocess verification, and verification status tracking
- Structured finding export plus Markdown report draft generation

**Explicitly deferred beyond v1:**

- Rust/Anchor, Move, and ZK-specific reasoning modes
- Full Mode 2 accumulator as a productized default experience
- Mode 3 LLM hinting, personalization, and Mode 4 synthesis
- Real-time multi-user collaboration, playbook sharing, and firm-specific report templating

### What These Users Already Have

The platform must slot into or improve on these existing workflows — not demand they be abandoned:

- **Code navigation:** VS Code, vim, or similar editors with Solidity/Rust language servers
- **Static analysis:** Slither, Aderyn, Mythril (run ad hoc, not continuously)
- **Call graph generation:** Surya, sol2uml (static output, non-interactive)
- **Notes and findings tracking:** Notion, Obsidian, personal markdown files, or raw text
- **Team coordination:** Slack or Discord for async, video calls every 2–3 days for sync
- **PoC development:** Foundry, Hardhat (entirely separate from the audit analysis workflow)

The platform's real competition is not Plamen or CertiK — it is this disconnected combination of tools.

---

## 3. Design Philosophy

### 3.1 Amplification Over Automation

The platform amplifies the auditor's existing reasoning process rather than attempting to replace it. The auditor remains the investigative agent — forming hypotheses, directing exploration, making judgments. The AI participates at four distinct levels of initiative, not a single fixed role.

At the most explicit level, the auditor invokes a typed agent on a selected scope and receives a focused, structured response — a fast, knowledgeable colleague answering a precise question. At a quieter level, the AI continuously accumulates context from the auditor's navigation, enriching subsequent responses without interrupting the current task. At a proactive level, the AI surfaces spatially-anchored hints when its pattern recognition confidence crosses a threshold the auditor sets — asking questions rather than asserting conclusions, respecting the auditor's expertise. At the highest level, the auditor invokes a synthesis agent to reason across the entire accumulated investigation — cross-suspicion composition, coverage gap analysis, adversarial stress-testing of findings.

This is a deliberate inversion of the autonomous auditor model in terms of control — but not in terms of AI passivity. The AI is not waiting silently to be asked. It is observing, accumulating, and surfacing — within boundaries the auditor controls. The distinction is that the AI never takes over the investigation. Every observation it surfaces is an input to the auditor's judgment, not a substitute for it.

### 3.2 The Investigation as a First-Class Object

Current tools produce reports. This platform produces an **investigation** — a structured, persistent, visual artifact that captures not just findings but the full reasoning path: which hypotheses were formed, which were tested, which were confirmed or cleared, and which remain open. The report is generated from the investigation, not constructed after the fact from memory.

This makes the auditor's reasoning transparent, reviewable by teammates, and valuable beyond the current engagement.

### 3.3 Spatial Reasoning Over Linear Reading

Code is typically consumed linearly — file by file, function by function. But vulnerabilities are relational: they emerge from the interaction of components across call boundaries, storage layouts, trust domains, and execution contexts. The platform exposes these relationships as a navigable visual space, making structural patterns that are invisible in a linear editor immediately apparent.

The DAG canvas is not a documentation tool. It is a cognitive environment for reasoning about system topology.

### 3.4 Low Commitment, High Reward

The platform does not require workflow adoption. An engineer can open a single contract, trace one path, invoke one agent, and extract value — without configuring a project, creating an account, or committing to a new process. Every interaction should reward the engineer with information they did not have before they started.

This means:

- The solo experience is complete and valuable without any collaboration features active
- The first meaningful visual appears within 60 seconds of loading a codebase
- The core solo loop is useful without collaboration, synthesis, or proactive hinting

Collaboration is the natural emergent property of multiple engineers independently finding the platform useful — not a prerequisite for it.

Higher-order features may depend on persisted investigation state or optional infrastructure, but the product must degrade gracefully: when a dependency is absent, the auditor still gets a complete, trustworthy local workflow rather than a half-functional experience.

### 3.5 Transparency as a Safety Property

In security tooling, a false negative presented with confidence is more dangerous than no tool at all. The platform is designed so the auditor can always see why an agent produced an answer — which lines were in scope, what assumptions were made, what was not examined. Agents are explicit about their uncertainty. The auditor's judgment always supersedes the agent's output, and the platform's UX reflects this hierarchy.

This transparency requirement applies across all four agent modes. For explicit invocations (Modes 1 and 4), the auditor can inspect the full context injected into any agent call — what code was in scope, what session history was included, what the agent was told it could not see. For the passive accumulator (Mode 2), the auditor can query the current session context model at any time — what has been observed, what connections have been noted — so it is never a hidden influence. For proactive hints (Mode 3), each hint is spatially anchored to the code that triggered it and displays the pattern that was recognized, so the auditor can evaluate the basis for the hint rather than acting on it blindly.

No agent output is presented as authoritative. All agent outputs are labeled with their confidence level, evidence class, and scope boundary.

Agent-originated clearances are always provisional. If an agent says something appears safe, the response must surface the unchecked assumptions prominently, and the auditor must explicitly accept or reject those assumptions before marking the item cleared.

### 3.6 A Tool Worth Playing With

The best tools in a security engineer's life have a quality of intellectual toyability — Ghidra for reverse engineers, Burp Suite for web security, a good debugger. You open them and want to explore, not because you have a specific task, but because the tool makes exploration rewarding in itself.

An engineer should be able to load a protocol they have never seen, spend 20 minutes with the graph, and come away with a genuine understanding of the system's topology that would have taken two hours of manual reading. That experience is intrinsically motivating and is how organic adoption happens.

---

## 4. The Auditor's Cognitive Modes

The platform is designed to serve five distinct cognitive modes that auditors cycle through during an engagement. Most existing tools serve only one or two.

| Mode | Description | Platform Support |
|---|---|---|
| **Orientation** | Building a mental model of a new system — trust domains, value flows, architectural patterns | Protocol fingerprint, archetype detection, heatmapped DAG |
| **Suspicion-forming** | Pattern recognition, intuition-driven anomaly detection | Suspicion markers, visual heatmaps, historical pattern matching via RAG |
| **Hypothesis testing** | "If X is malicious, can they reach Y?" — attack path reasoning | Interactive path tracing, what-if mode, threat model presets |
| **Evidence building** | Proving exploitability — PoC construction and validation | PoC scaffold generation, agent-assisted logical verification |
| **Reporting** | Constructing the audit report from captured reasoning | Finding graph export, annotation-driven report generation |

---

## 5. Core Capabilities

### 5.1 Codebase Orientation Engine

The first two hours on a new protocol are disproportionately valuable — the mental model built here shapes the entire engagement.

**Protocol fingerprint on load.** When a codebase is loaded, the platform generates a structural digest: number of trust levels, value entry and exit points, external dependencies, upgrade mechanisms, and graph centrality metrics (which contracts have highest betweenness centrality in the call graph). This is the orientation briefing — a system map, not a vulnerability list.

**Protocol archetype detection.** The platform pattern-matches the codebase against a library of protocol archetypes and structural overlays. v1 ships with one protocol archetype (ERC-4626 vault) and two overlays (proxy/upgrade pattern, ERC-20 token interface). Protocol archetypes define an expected audit surface and can power delta highlighting, checklist overlays, and coverage-gap baselines. Structural overlays enrich the graph with additional risk lenses and checklist items, but do not by themselves claim a complete expected system shape. Additional archetypes (AMM variants, lending protocols, governance systems, bridges, ZK/L2 aggregator architectures) are added as the library matures. When no protocol archetype matches, coverage-gap baselines are unavailable and the UI shows this explicitly rather than silently degrading; overlay-only matches still provide useful annotations without over-claiming completeness.

**Delta highlighting against archetype template.** Once an archetype is matched, the DAG overlays a delta view. Nodes and edges corresponding to standard, battle-tested archetype components are visually de-emphasized but never hidden. Nodes representing deviations — additions, modifications, or novel components — are prominently highlighted. Integration seams, configuration edges, and trust-boundary crossings remain legible even inside "standard" regions, because many critical vulnerabilities live at the seams rather than in obviously novel code.

**"Explain this contract" agent.** Single-click invocation on any contract node produces a role summary: what this contract does in the system, what invariants it assumes, and what breaks if it misbehaves. Orientation context, not a vulnerability scan. This is not a full typed agent — it is a lightweight system prompt variant that ships in v1 alongside the three core agent types. It uses the same context pipeline but does not require the auditor to articulate a suspicion type, making it the natural first-touch interaction during orientation.

**Complexity and coupling heatmap.** Before reading a single line, the auditor sees which parts of the system are structurally complex — high fan-in/fan-out, deep inheritance chains, heavy delegatecall usage, dense external dependencies. Switchable lenses: complexity, privilege level, external exposure, mutation density, coverage status.

---

### 5.2 Suspicion-Triggered Agent Invocation

Agent participation in the investigation takes two forms: explicit invocation by the auditor on a selected scope, and proactive surfacing by the platform when a high-confidence pattern is recognized during navigation. Both forms are covered here. The full architecture of these modes is detailed in Section 9.

**Explicit invocation — line-pinned and selection-scoped agent calls.** Right-click any line, highlight any range, or select any DAG node to invoke an agent. The agent's context is bounded to exactly the selection plus its dependency graph. The agent does not have access to the rest of the codebase unless the auditor explicitly expands scope. This is the primary mechanism for preventing hallucinated cross-contract reasoning.

**Typed agent invocations.** The invocation menu presents semantically distinct agent specializations the auditor chooses between. Choosing the agent type is itself a cognitive act — it requires the auditor to articulate what kind of suspicion they have.

Available agent types:

- **State invariant checker** — Does this function maintain the invariants I would expect?
- **Caller trust analyzer** — Who can call this, under what conditions, and should they be able to?
- **Side effect tracer** — What state does this touch, including external contracts?
- **Spec compliance checker** — Does this implementation match the NatSpec or documentation?
- **Historical pattern matcher** — Does this pattern appear in known exploits? (RAG-grounded)
- **Archetype deviation checker** — Does this deviate from the standard implementation of this pattern, and how?

The full vocabulary grows over time. v1 ships only the first three agent types; the others depend on more mature retrieval, archetype, and documentation pipelines.

**Suspicion markers, not finding flags.** When something feels off, the auditor places a suspicion marker — an open question, not a finding. Suspicions are tracked visually on the DAG and move through a lifecycle: `open → investigating → confirmed finding / cleared / deferred`. This maps accurately to audit psychology and makes the trail of cleared suspicions itself a valuable audit artifact.

**Suspicion composability.** Multiple suspicions can be linked into a compound hypothesis: "If suspicion A and suspicion B are both real, does attack path C become possible?" The platform tracks these relationships in the finding graph.

---

### 5.3 Attack Path Reasoning

**Source-to-sink path enumeration.** The auditor specifies a threat actor capability (e.g., "controls this address," "has flash loan access") and a target state (e.g., "contract balance decreases without authorization," "this invariant is violated"). The platform enumerates bounded candidate paths through the call and state-transition graph. Paths are ranked by directness (fewest hops, fewest unresolved preconditions) and presented progressively: the five most direct paths render immediately as colored overlays; additional paths are available on demand via an "expand N more paths" control. Every path is labeled by analysis class (`static reachability`, `state-conditioned`, `hypothetical/what-if`), depth limits, unresolved dynamic dispatch, and any external edges the system could not expand.

**Reachability precondition annotation.** Each enumerated path is annotated with its preconditions: which paths require privileged access, which require specific state, which are unconditionally reachable. An agent evaluates preconditions per path.

**What-if mode.** The auditor can temporarily remove any guard, modifier, or access control check from the DAG — without modifying actual source code — and re-run path analysis. The blast radius of a single access control failure becomes immediately visible. This operates on a clearly marked hypothetical analysis graph, not the source-of-truth call graph, and the UI must make that distinction impossible to miss.

**Cross-contract execution trace.** The bottom timeline drawer linearizes a multi-hop attack path into a step-by-step execution sequence, showing which contract is active at each step, what state is modified, and what value is transferred. Animated particle flow on the DAG provides directional intuition; the timeline provides analytical precision.

**Flash loan threat model preset.** One-click activation: the platform grants the threat actor an arbitrary balance of any in-scope token and re-evaluates which attack paths become newly reachable. This is a standard precondition for a large class of DeFi exploits and should not require manual setup.

**ZK/L2 constraint mode.** For ZK circuit and proof aggregator audits, a dedicated reasoning mode replaces execution-path tracing with constraint satisfiability reasoning: "Is there a satisfying witness assignment that violates this invariant?" Agent invocations in this mode reason about constraint completeness, soundness holes in custom gates, prover/verifier mismatch, and recursion depth correctness rather than EVM execution paths.

---

### 5.4 Evidence Construction Workspace

**PoC scaffold generation.** When an attack path is traced and the auditor is confident it is real, the platform generates a PoC test skeleton: contracts set up, attack steps commented in natural language, placeholders for the specific exploit logic. The auditor fills in the exploit, not the boilerplate.

**Agent-assisted logical pre-flight.** Before running a PoC, the auditor asks an agent to review the exploit logic for reachability: are the preconditions achievable? Are there access controls or state checks that would block the path? This is a fast logical check, not execution — it catches obvious issues before the engineer invests time in Foundry.

**Execution result integration.** When connected to a local Foundry environment, test results are displayed inline. A passing PoC turns the relevant DAG path green and promotes the suspicion node to a confirmed finding. A failing PoC annotates the path edge at the point of revert with the revert reason.

**Evidence tagging.** Each finding is tagged with its evidence class: executed PoC, logical code trace, RAG-matched historical pattern, or agent-only analysis. Evidence class is visible in the finding graph and propagates to the report. The auditor always knows — and can communicate to the client — how a finding was substantiated.

---

### 5.5 Collaborative Investigation

Collaboration is a natural property of the shared canvas, not a separate feature set. This is intentionally post-v1: the first release validates the solo workflow and structured investigation model before live multi-user state becomes a product commitment. When a second engineer opens the same session, the full investigation state is immediately visible — no briefing required.

**Named investigation threads.** Each engineer owns named threads: directed investigation paths through the DAG. Threads are visible as colored overlays on the shared canvas. Engineers can see where teammates are working, avoid duplicate effort, and identify where independent threads converge on the same contract — convergence that is often structurally significant.

**Coverage map.** As engineers mark nodes and edges as reviewed, the canvas builds a coverage heatmap showing human attention, not just risk level. The goal by engagement end is full coverage with no high-risk unreviewed nodes.

**Async annotation threads.** Any node or edge can host an annotation thread — a question, concern, or note directed at a teammate. The teammate sees it in their next session, invokes a relevant agent, and replies with evidence. The annotation thread becomes part of the finding's evidence trail.

**Skill-level transparent collaboration.** The shared canvas makes the senior engineer's reasoning visible to the team in real time — which nodes they are expanding, which paths they are tracing, what annotations they are leaving. This is structured knowledge transfer embedded in the work itself. Junior engineers observe and learn from the senior's investigation pattern without a separate mentorship process.

**Finding graph ownership.** Each confirmed finding is added to a shared finding graph with named ownership, tracking which engineer identified the root suspicion, who contributed supporting evidence, and what the final severity determination was. The audit report is assembled from the finding graph.

---

### 5.6 Session Continuity and Investigation Memory

**Full state persistence.** Every agent invocation, suspicion marker, annotation, path trace, and coverage update is saved. Reopening an investigation on day 3 reconstructs the exact spatial and analytical state from day 2.

**Agent conversation history per node.** Each DAG node carries a conversation history — every agent invoked on it, every question asked, every answer received. This is scrollable from the node detail panel and constitutes the audit log for that component.

**Suspicion lifecycle audit trail.** The full trail of how each suspicion evolved — which agents were invoked, what evidence emerged, when it was promoted or cleared — is always accessible. At report time, this trail becomes methodology documentation: not just what was found, but how.

**Reusable investigation playbooks.** Engineers can save a named playbook: an intent-driven bundle of scoped agent templates, DAG queries, and path-tracing heuristics representing their methodology for a vulnerability class or protocol archetype. Playbooks encode what to investigate and how to parameterize the investigation, not a brittle fixed click sequence tied to one agent version or one protocol layout. Senior engineers' expertise becomes reusable and teachable. Junior engineers can run an expert's vault-audit playbook against a new vault contract and understand the reasoning behind each step — not just the conclusion.

**Playbook graceful degradation.** When a playbook references an agent type, archetype, or capability not available in the current platform version or workspace configuration, the unavailable step is shown as skipped with an explanation ("Step 3: Archetype Deviation Checker — unavailable, requires archetype library extension"), not silently dropped or errored. The remaining steps execute normally. This ensures playbooks authored against the full platform remain usable in earlier or constrained environments.

---

## 6. UI/UX Architecture

### 6.1 Three-Panel Layout with Bottom Drawer

**Left panel (~250px, collapsible):** File and contract tree showing codebase hierarchy. Icons indicate file type, language, and risk annotation. Coverage status visible per file.

**Center panel (fluid):** The DAG canvas — the primary interactive surface and the heart of the product.

**Right panel (~350px, collapsible):** Inspector/Detail panel. Shows context for the selected node or edge: source code, call signature, storage slot access, permission modifiers, agent conversation history, annotation threads, suspicion lifecycle status.

**Bottom drawer (~200px, toggleable):** Trace timeline and data-flow ribbon. When tracing an attack path, this shows the linearized execution sequence with step-by-step state transformations. During multi-path analysis, paths appear as parallel swim lanes.

### 6.2 DAG Canvas Design

**Semantic zoom.** At macro zoom, the canvas shows contract clusters with internal detail collapsed. Zooming in progressively reveals functions, then state variable access patterns. Prevents cognitive overload from 500+ simultaneous nodes.

**Node differentiation by semantic role:**
- Contracts: rounded rectangles with header bar, colored by risk level
- Functions: pill shapes linked from parent contract
- State variables: diamond shapes (they are targets)
- External calls: dashed border with external indicator
- Entry points (public/external): thicker border signaling attacker surface

**Edge semantics:**
- Regular calls: solid lines with directional arrows
- Cross-contract calls: thicker colored lines (trust boundaries)
- Storage reads/writes: dotted lines to state variable diamonds
- Delegatecall / low-level calls: red-dashed treatment (high-risk)
- Value transfers: animated pulse edge

**Trust boundary visualization.** Automatically detected and drawn as dashed domain outlines: admin-only functions, user-facing surface, oracle dependencies, external protocol integrations. Helps auditors reason about privilege escalation and cross-domain attacks.

### 6.3 Core Interaction Patterns

**Right-click context menu on any node or edge:** Trace all paths from here / to here, highlight callers/callees, invoke agent (typed menu), mark as reviewed, place suspicion marker, add annotation, add to current investigation thread.

**Search and filter bar:** Search begins with fast string and structural filters: function name, variable, modifier, visibility, mutability, and patterns (`delegatecall`, `selfdestruct`, `transfer`, `assembly`), plus saved filter presets. Non-matching nodes fade to 20% opacity. Matching nodes and their immediate neighborhood remain at full opacity. v1 structural filters include modifier-aware filtering ("all `external` functions lacking `onlyOwner`") and state-write filtering ("all functions that write to `balances`"). These are backed by the same thin abstraction layer used by the context pipeline; the Solidity frontend simply populates that abstraction from AST facts, rather than creating a separate semantics path for search. Richer semantic filters ("external functions that modify user balances through any code path") are layered on top once the underlying IR is reliable; they are not a prerequisite for the initial product.

**Double-click to expand/collapse:** Contract nodes expand in-place to reveal their internal function graph without losing spatial context.

**Minimap:** Fixed bottom-right, shows heatmap coloring of the full graph. The auditor can spot hot zones even when zoomed into a single contract.

### 6.4 Animation Principles

Animations carry information — they are not decorative.

**Layout transitions:** Node clusters expand/collapse with spring-physics animation (~300ms, ease-out). Preserves the auditor's spatial memory of the graph.

**Path tracing particle flow:** Small particles travel along each active path edge in the direction of call execution. The primary tool for making multi-hop attack chains cognitively tractable.

**Suspicion-to-finding promotion:** The node pulses with a brief color transition (yellow → red) when a suspicion is confirmed. The relevant path edges transition to full opacity.

**Agent response arrival:** A subtle pulse on the associated node when new agent information arrives. Non-intrusive — the auditor is not forced to read it immediately.

**Search/filter transitions:** Matching nodes scale up (0.95 → 1.0) with a brief flash; non-matching nodes fade to 20% opacity in ~200ms. Reversal on query clear is immediate.

### 6.5 Audit-Specific UI Components

**Annotation layer.** Color-coded sticky notes on any node or edge: red (vulnerability), yellow (concern/suspicion), green (reviewed/safe), blue (note/question). Persist across sessions, exportable as part of the audit report.

**Diff mode.** For upgrade audits: changed nodes pulse with a blue outline, added nodes have a green glow, removed nodes appear as ghost outlines. Edges that changed are re-colored. Critical for proxy contract upgrade reviews.

**Archetype checklist overlay.** A toggleable panel listing known vulnerability patterns for the detected archetype. Clicking a checklist item runs a pre-built DAG query highlighting relevant nodes and edges. Provides structured starting points for less experienced engineers.

---

## 7. Entry Points and Onboarding

**Zero-configuration solo start.** Drop in a single file, paste a contract address, or point at a directory. A useful graph appears in under 60 seconds. No project configuration, no account creation, no dependency installation required.

**Single-file mode is a full mode.** An engineer investigating one suspicious contract gets the complete feature set. Single-file is not a degraded experience.

**Local-first, cloud-optional.** Solo use operates entirely locally without cloud dependency. State is persisted locally. Cloud sync activates when a teammate is invited to a session. Engineers should feel comfortable loading unreleased client code before trusting the platform with it.

**Collaboration entry.** Sharing an investigation generates a link. A teammate opening the link sees the full current investigation state immediately — the canvas is the briefing.

### 7.1 Operational Modes and Graceful Degradation

The platform has three operational states:

- **Offline / local-only.** Parsing, graph generation, static detectors, suspicion tracking, annotations, session persistence, local Foundry execution, and report assembly from existing artifacts work without network access.
- **Connected inference.** Remote agent invocations, historical pattern matching, and optional synced investigation state become available when connectivity and policy permit.
- **Degraded remote state.** If an external model or API is unavailable, the UI disables agent actions explicitly, shows why, and preserves the rest of the investigation workflow. The platform never silently drops into stale or partial agent behavior.

### 7.2 Security and Trust Model

Because the platform handles pre-disclosure vulnerability information, its own trust model is product-critical.

**Default posture.** Local-only by default. Code, notes, session graph, and execution artifacts remain on disk unless the auditor explicitly enables sync or remote inference.

**Invocation transparency.** Before any remote agent call, the auditor can inspect exactly what leaves the machine: selected scope, retrieved context, provider, and configured retention policy. Redaction controls apply at invocation time.

**Cloud collaboration requirements.** Synced investigation state must be encrypted in transit and at rest, access-controlled per workspace, auditable, revocable, and time-bounded for shared links. The server side must treat investigation data as sensitive customer material, not product telemetry.

**Deployment profiles.** The architecture must support at least three profiles: local-only, vendor-API-assisted, and self-hosted / firm-managed inference. Enterprise adoption depends on all three being representable, even if they do not all ship on day one.

### 7.3 Joining an Existing Investigation

When a second auditor joins mid-engagement, the platform must assume they do not yet understand the canvas vocabulary or the current investigation state.

A new collaborator landing in a shared session first sees a short investigation summary: protocol fingerprint, open suspicions, confirmed findings, unreviewed hot zones, and active threads. The summary is paired with a legend for node and edge semantics and a one-click "take me to the important regions" tour. The canvas is still the briefing, but the platform provides just enough narrative scaffolding that the canvas is legible on first contact.

---

## 8. Competitive Positioning

This platform does not compete with Plamen or CertiK AI Auditor for the same use case. It occupies a different and complementary position.

| | Autonomous AI Auditors | This Platform |
|---|---|---|
| **Who drives the investigation** | AI pipeline | Human auditor, with AI participating at four levels of initiative |
| **Interaction model** | Trigger and wait | Continuous, exploratory |
| **Transparency** | Black box output | Full reasoning trail |
| **Target user** | Any developer | Experienced security engineers |
| **Team collaboration** | None (report merge) | Shared live investigation canvas |
| **Value unit** | Finding list | Investigation artifact |
| **Replaces** | Junior-level pattern scanning | Disconnected toolchain |
| **Best fit** | Large, standardized codebases | Novel/complex architectures, L2/ZK |

The platforms are complementary: an autonomous scanner runs in the background producing a baseline finding list while the expert auditor conducts the real investigation here. The autonomous scanner's output can be imported as pre-populated suspicion markers — a starting point the expert verifies and extends.

---

## 9. AI Agent Architecture

The agent layer is not a single AI assistant. It is four qualitatively distinct participation modes, each with a different initiation model, a different cognitive role, and different engineering requirements. The auditor controls which modes are active and at what sensitivity.

The central design tension resolved here: a fully passive agent (only responds when invoked) limits its value for less experienced auditors who don't know what to ask. A fully proactive agent (constantly volunteers observations) breaks the deep focus security work requires. The solution is **ambient awareness with threshold-based surfacing** — the agent is always observing context but surfaces information only when its confidence that it is relevant crosses a threshold the auditor controls.

Unless otherwise noted, this section describes the end-state architecture. v1 ships only Mode 1 on Solidity / EVM; Modes 2–4 are designed now to avoid painting the system into a corner later, but they are not prerequisites for initial product validation.

---

### 9.1 Mode 1 — Reactive Oracle (Auditor-Initiated, Scoped)

The primary interaction mode. The auditor selects a scope — any range of code, any DAG node, any call path — invokes a typed agent, and receives a focused, structured response. The agent's context is bounded to exactly the selected scope plus its explicit dependency graph. No codebase-wide diffusion.

**Agent type vocabulary.** The auditor chooses the kind of analysis, not just "analyze this." Choosing an agent type forces the auditor to articulate what kind of suspicion they have — a cognitive forcing function independent of the agent's response.

| Agent Type | Question Answered |
|---|---|
| State Invariant Checker | Does this function maintain expected invariants? |
| Caller Trust Analyzer | Who can call this, and should they be able to? |
| Side Effect Tracer | What state does this touch, including external contracts? |
| Spec Compliance Checker | Does this match the NatSpec / documentation? |
| Historical Pattern Matcher | Does this pattern appear in known exploit corpora? (RAG-grounded) |
| Constraint Analyzer *(ZK mode)* | Is this constraint system complete for the stated transition function? |

**Structured uncertainty as a first-class output.** Agent responses distinguish between:
- `CONFIRMED` — here is the exact execution path demonstrating the issue
- `POSSIBLE` — here is the condition under which it would be exploitable, which I cannot verify without additional context
- `WORTH RULING OUT` — this pattern superficially resembles X but I believe it is safe because Y — verify Y

**Explicit blind spots.** After each analysis the agent reports what it could not see: external dependencies not in scope, assumptions about caller behavior it could not verify, state that was read but not traceable to its origin. The agent's blind spots direct the next investigation step as usefully as its findings.

No item can move to `cleared` solely because an agent said so. The auditor must explicitly accept the blind spots or continue the investigation.

**Suggested follow-up.** At the end of each response, one proposed next question grounded in the specific analysis just performed — not a generic checklist item. The auditor chooses whether to follow it. This is the primary mechanism by which the agent expands the auditor's investigative thread without taking control of it.

**Feasibility:** High. Well-scoped LLM prompt engineering with structured output contracts. For v1, the scope is Solidity / EVM only with three typed agent types plus the lightweight "Explain this contract" orientation agent. Additional languages and richer agent vocabularies are follow-on work once the context pipeline and evaluation harness are stable.

---

### 9.2 Mode 2 — Passive Context Accumulator (Always On, Never Interrupts)

Runs silently in the background while the auditor navigates the DAG. Builds a **session context model** — a structured, bounded representation of the investigation state: which contracts have been examined (with depth and confidence scores), which suspicions are open and their current status, which agent invocations have occurred with their conclusions summarized to 1–2 sentences, which storage variables and state transitions have been observed.

This model is never surfaced directly to the auditor unprompted. But it is automatically injected as context into every other mode's agent invocations. The result: agent responses become progressively more intelligent as a session continues, because the agent knows what the auditor has already examined even when the auditor does not explicitly reference it.

**The connection-surfacing capability.** When a new agent invocation touches a code region that shares state variables, call paths, or storage slots with something examined earlier in the session, the agent can surface that connection: "The storage variable written here was also read in the withdrawal function you examined 30 minutes ago." This cross-temporal awareness is the most valuable output of the accumulator and would otherwise require the auditor to hold the entire session in working memory.

**On-demand session summary.** At any point, the auditor can ask "what have I examined, and what connections have you noticed?" — receiving a structured summary of the investigation from the agent's perspective. Useful before sync calls, before writing the report, or when returning to a complex engagement after a break.

**Implementation approach.** The session context model is maintained as a structured graph data structure, not as a raw conversation transcript. This keeps it bounded in size regardless of session length and makes it efficiently serializable into context windows. For long sessions, a vector-indexed retrieval layer retrieves the most relevant historical chunks per invocation query rather than injecting the full model.

**Session graph schema.** The accumulator maintains a typed property graph, not free-form chat memory. Core node types: `code_entity`, `suspicion`, `finding`, `evidence_artifact`, `invariant`, `trust_boundary`, and `session_event`. Core edge types: `calls`, `reads`, `writes`, `depends_on`, `supports`, `contradicts`, `observed_during`, and `mentions`. Raw code references and evidence artifacts are retained losslessly; navigation traces and prior agent outputs are summarized into derived nodes with back-pointers to the original artifacts. As the session grows, low-signal navigation events are compressed first; high-signal suspicions, findings, and evidence remain directly addressable.

The accumulator itself does not continuously stream tokens to a model. It observes locally, updates the session graph locally, and only retrieves relevant fragments at invocation time. This keeps cost tied to deliberate agent use rather than cursor movement.

**Feasibility:** Medium-High. Structured summary graph approach is 1–2 months additional on top of Mode 1. Full semantic retrieval adds another 1–2 months and vector store infrastructure. The main design challenge is determining what to compress vs. retain as sessions grow.

---

### 9.3 Mode 3 — Threshold-Based Hint Surface (Proactive, Controlled)

Watches the auditor's navigation — which nodes are being expanded, which paths are traced, which annotations placed — and pattern-matches against a recognition library. When it identifies a high-confidence signal, it surfaces a hint. The threshold is auditor-controlled: senior engineers run quiet, juniors benefit from a more active setting.

**Hints are spatially anchored, not interruptive.** A hint appears as a subtle indicator on the relevant DAG node — a small icon, a color shift in the node border — not a popup or notification. The auditor sees it in peripheral vision and chooses whether to engage it. It does not demand attention or block navigation.

**Hints are questions, not conclusions.** "Have you checked whether this callback can be triggered before the balance update?" rather than "This function has a reentrancy vulnerability." This respects the auditor's expertise, invites investigation, and avoids the false authority problem where an agent assertion short-circuits the auditor's own reasoning.

**Hints expire.** If the auditor navigates away from the relevant region without engaging the hint, it fades after a configurable interval. No accumulating backlog.

**Two-tier architecture.** Static pattern recognition (Slither/Aderyn detectors surfaced as hint UX) handles fast, deterministic patterns and runs on every node expansion. LLM-powered contextual hints handle non-obvious observations and run on a batched schedule with a cheaper model (Haiku) as pre-screener. Only high-confidence LLM hints escalate to the auditor's view.

**Personalization via preference database.** When an auditor engages a hint (investigates the suggested region), that pattern is reinforced for their profile. When they dismiss a hint without engaging, the pattern is downweighted. Over time, the hint system calibrates to each auditor's pattern recognition style. This is implemented as a retrieval filter over a preference database — not online model training.

**Feasibility:** Static pattern hints with threshold UX — Low, 4–6 weeks after the solo core is stable. LLM-powered contextual hints with batching — Medium, 2–3 months. Preference-database personalization — Medium, 1–2 months additional.

---

### 9.4 Mode 4 — Synthesis Agent (Auditor-Initiated, Cross-Investigation)

Operates at the level of the full investigation rather than individual code regions. The auditor explicitly invokes this mode when they want the agent to reason across everything accumulated so far. Four capabilities:

**Cross-suspicion composition.** Given all open suspicions, are there combinations that form a more severe attack path than any individual suspicion implies? A finding that is Medium in isolation may be Critical when composed with a separate unguarded state transition. The agent identifies compositionally-adjacent suspicions using call graph structure as a pre-filter (graph-based adjacency analysis reduces the combinatorial space before any LLM reasoning), then reasons about their interactions.

**Coverage gap analysis.** Based on what has been examined and what the protocol archetype implies should be verified, what has received no auditor attention? This is a diff between the archetype's expected audit surface and the session's actual coverage map — not a generic checklist but a targeted gap report specific to this investigation.

**Narrative synthesis for reporting.** Given the confirmed findings and their evidence trails, draft a coherent attack narrative explaining the relationship between findings, the severity reasoning, and the impact chain. The agent constructs the first draft; the auditor edits. Report writing moves from blank-page reconstruction to structured editing. Most of the raw material (reasoning trails, evidence tags, annotation history) already exists in the investigation graph.

**Devil's advocate on confirmed findings.** Before a finding enters the report, the auditor can invoke the inversion mandate: "I believe this is a Critical finding — argue against it." The agent takes an adversarial role, attempting to construct the strongest possible counter-argument. Any argument the agent can construct is an argument the client's team will also make. This stress-tests findings before they leave the platform.

**Feasibility:** Medium. 2–3 months once Mode 2's structured summary graph exists and the core solo workflow is already validated. Cross-suspicion composition with graph pre-filtering is the most technically involved piece. This is roadmap work beyond the initial release.

---

### 9.5 Cross-Cutting Agent Infrastructure

Three platform-level requirements affect all four modes:

**Context injection pipeline.** Every agent invocation assembles a composite context: selected code scope + AST subgraph of dependencies + relevant session context from the accumulator + archetype context + investigation history summary. This pipeline must be fast, correct, and consistent across all invocation types. It is unglamorous but load-bearing — a bug here degrades every agent response. Estimated 4–6 weeks of dedicated engineering.

**Composite context schema.** Each invocation produces a structured object with: `hard_scope`, `dependency_subgraph`, `advisory_context`, `retrieved_history`, `archetype_context`, `blind_spots`, and `execution_constraints`. `hard_scope` is the only code the agent may treat as directly inspected. Advisory context can inform suggestions and connection-surfacing, but facts about unseen code must be labeled as advisory unless the auditor explicitly expands scope.

**Token budget and overflow policy.** Context assembly is budgeted, not best-effort. For Mode 1, most tokens belong to selected code and immediate dependencies; session and archetype context are subordinate. For Mode 4, the investigation graph and evidence trail dominate, with raw code excerpts pulled in only where needed. If the payload exceeds the window, the platform drops advisory context in a defined order and warns the auditor before truncating `hard_scope`.

| Invocation Type | Priority 1 | Priority 2 | Priority 3 |
|---|---|---|---|
| Mode 1 | `hard_scope` | `dependency_subgraph`, `blind_spots` | `retrieved_history`, `archetype_context` |
| Mode 4 | findings, evidence trail | targeted code excerpts | archetype and session summaries |

**Inference latency management.** Mode 1 and Mode 4 invocations (explicit, awaited) can tolerate 5–15 second response times if the response streams visibly and the auditor can continue navigating while it generates. Mode 3 hints must feel ambient — handled by running static detectors synchronously and LLM hints asynchronously with a batching delay. The platform never blocks the auditor's navigation for any agent operation.

**Model routing strategy.** Mode 1 and Mode 4 use a frontier reasoning model by default because security reasoning quality matters more than raw throughput. Historical pattern matching can split recall and explanation: retrieval plus a cheaper model for candidate generation, escalated to the frontier model for final reasoning when needed. Mode 3 uses deterministic detectors and cheap screeners first, escalating only high-confidence cases. Auditor or firm policy can pin provider and model tier per workspace.

**Agent response inspectability.** For any invocation, the auditor can expand a "context used" view showing exactly what code was in scope, what session context was injected, and what the agent was explicitly told it could not see. This is the transparency property that makes the agent layer trustable rather than a new black box. It also serves as a debugging surface during development.

---

### 9.6 Agent Evaluation Infrastructure

Agent quality is the most differentiated part of the product and the hardest to measure without dedicated infrastructure. Without an evaluation harness, prompt changes, model upgrades, and context pipeline adjustments can silently degrade agent responses. This is engineering infrastructure, not a product feature, but it is a prerequisite for shipping agents with confidence.

**Benchmark corpus.** A curated set of known-vulnerable contracts (public real-world exploits, CTF challenges, synthetic edge cases, and sanitized internal reproductions) where the expected agent behavior is defined per agent type. For each contract-agent pair: what should the agent flag, what should it mark as blind spots, and what should it not hallucinate. Only public, synthetic, or explicitly sanitized fixtures are version-controlled alongside the agent prompts. Engagement-derived cases that contain client-sensitive material must be redacted, detached from client identity, or stored in a separate restricted evaluation store rather than normal repository history.

**Regression framework.** Every change to agent prompts, context pipeline logic, or model version is run against the benchmark corpus before deployment. Regressions — cases where a previously correct response degrades — are flagged automatically. This prevents the common failure mode where improving one agent type's prompt silently breaks another.

**In-product feedback signal.** Auditors can mark any agent response as helpful, unhelpful, or incorrect with an optional free-text note. This signal is logged per agent type and invocation context, and feeds back into prompt iteration and benchmark corpus expansion. Feedback retained outside the local workspace must respect the same redaction and sensitivity rules as any other investigation artifact. The feedback loop is lightweight — a single click, not a form — to maximize capture rate.

**Evaluation is not scoring.** The goal is not a leaderboard or a single accuracy number. It is a structured way to detect regressions, identify weak spots per agent type, and prioritize prompt engineering effort. The benchmark corpus and regression framework should be operational before v1 ships agents externally.

---

## 10. Verification System

Verification is where the investigation loop closes — or where it currently breaks. In existing workflows, verification means leaving the analysis environment entirely, setting up a Foundry test in a terminal, running it, and returning with results. This context switch is expensive and interrupts flow. The platform keeps the auditor in one surface from suspicion to confirmed, evidence-tagged finding.

Verification addresses three distinct questions that must not be conflated:

- **Existence** — does the vulnerability exist in the code as written?
- **Exploitability** — can an attacker realistically trigger it?
- **Impact** — what is the concrete outcome when triggered?

A real finding requires evidence for all three. The verification system is designed to produce that evidence without leaving the platform.

---

### 10.1 Embedded Execution Environment

The platform runs a sandboxed Foundry subprocess locally. When a PoC test is ready to execute, the platform spawns the process, captures stdout/stderr, and renders results inline on the DAG. No terminal required. For future Solana/Anchor engagements, the equivalent is a local validator subprocess running `anchor test`. For future Move targets, `aptos move test` or `sui move test`.

**External fallback.** For cases requiring mainnet fork state, specific block conditions, or deployed contract interaction, the platform generates the complete test file and the auditor executes it in their terminal. Results (pass/fail + revert trace) are importable back into the platform as a structured evidence artifact and attached to the relevant finding node.

---

### 10.2 PoC Scaffold Structure

When the agent or the auditor initiates PoC generation, the platform produces a **layered scaffold** — not a complete test, not a blank file.

**Layer 1 — Setup** *(fully generated)*: contract deployment, initial state configuration, actor address assignment, token balances. Generated from the call graph and archetype context. No auditor input required.

**Layer 2 — Attack steps** *(partially generated)*: the sequence of function calls along the confirmed attack path, with concrete parameter values where deterministic and explicit placeholders where auditor judgment is required. Each step is commented with the reasoning from the path trace — "this call triggers the callback before the balance update."

**Layer 3 — Assertion** *(auditor-completed)*: what condition proves the vulnerability was exploited? The agent proposes the assertion based on the claimed impact ("attacker balance increased by X," "invariant Y is violated") and the auditor confirms or corrects it. This is the most important layer and the one where agent assumptions are most likely to require adjustment.

**Assertion review.** Before execution, the platform requires the agent to explain in plain language why the proposed assertion proves existence, exploitability, and impact rather than merely observing a side effect. The auditor approves or edits this reasoning before the test is treated as evidence.

**Layer 4 — Edge cases** *(agent-suggested)*: alternative parameter values to test, boundary conditions, variant attack paths identified during analysis. Optional — the auditor decides whether to pursue them.

This structure means the auditor is doing real intellectual work (confirming the assertion, filling exploit logic) while not writing boilerplate. Time from "I believe this is exploitable" to "I have a running test" should be under 10 minutes for a well-understood finding.

---

### 10.3 Inline Result Rendering

Test results render directly onto the investigation surface:

**Pass.** The attack path in the DAG transitions to green. The finding node receives a `[POC-PASS]` badge. Test output — gas used, state diff, emitted events — is attached as structured evidence to the finding node and visible in the inspector panel.

**Fail with revert.** The DAG annotates the edge where execution reverted with the revert reason and the call stack depth at failure. This is actionable — a revert at a specific call graph edge tells the auditor exactly which guard blocked them, directing the next refinement step.

**Fail with incorrect assertion.** The test executed but the expected impact did not materialize. The agent analyzes the state diff and proposes why — "the balance increased but by less than expected due to fee accounting on line 247." Drives next iteration.

**Compilation error.** Shown inline with the relevant scaffold line highlighted. Agent suggests the fix. The most common failure mode for scaffolded PoCs and should resolve without the auditor touching a terminal.

---

### 10.4 Iterative Refinement Loop

Verification in practice is a loop — the first PoC fails, something is learned, the path is refined, the test is rewritten. The platform tracks this loop explicitly.

Each test run is a **versioned artifact** attached to the finding node: not just the latest result but the full attempt history, what changed between attempts, and what each failure revealed. The investigation is auditable — clients can see that findings were rigorously stress-tested, not just asserted.

After each failed run, the agent analyzes the revert trace in the context of the original attack path and proposes the most likely cause of failure and the most promising next refinement. Reasoning about execution traces against known vulnerability patterns is a task where frontier LLMs perform well and the suggestion quality is high.

---

### 10.5 Verification for ZK and L2 Targets

This section is explicitly post-v1. For ZK circuit audits, execution-based PoC does not directly apply. The verification problem is structurally different.

**Witness construction testing.** Can a valid witness be constructed that satisfies the prover but violates the intended semantic property? The platform supports the auditor in constructing and testing witness assignments against the constraint system. For Circom, Halo2, and Noir targets, witness generation code is instrumented and run locally within the embedded environment.

**Constraint coverage analysis.** Are there execution paths through the arithmetic circuit not covered by constraints? This is analogous to code coverage for constraint systems — a gap is a potential soundness hole. The platform integrates available constraint analysis tooling (circom-witnesscalc, Noir's constraint checker) and surfaces gaps as finding candidates on the DAG.

**Differential testing against reference implementation.** For a given input set, does the ZK circuit produce the same output as a plain-language reference implementation? The platform scaffolds differential test structure — the auditor provides or confirms the reference, the platform generates the comparison harness. Semantic mismatches between circuit and reference are often the fastest path to a soundness finding.

**Multi-prover aggregator verification.** For Risc0/SP1/TEE aggregation contexts, a finding may require demonstrating that two provers can be induced to disagree on a specific input and that the aggregation logic accepts the result incorrectly. The platform scaffolds a local multi-prover test harness with adversarially crafted inputs. This is heavier infrastructure than Foundry integration but follows the same subprocess model.

---

### 10.6 Verification Status as a First-Class Finding Property

Every finding node carries a verification status tag. This is displayed prominently on the node and in the inspector panel — not buried in the description.

Every finding also stores three separate judgments that must not be collapsed into one field:

- **Severity** — the client impact if the claim is real
- **Confidence** — how likely the claim is correct
- **Verification status** — what evidence has been produced so far

| Status | Meaning |
|---|---|
| `UNVERIFIED` | Finding exists based on code analysis only. No execution evidence. |
| `POC-ATTEMPTED` | Test written and executed. Result inconclusive or failing. |
| `CODE-TRACE` | Manually traced execution path with concrete values. No execution. |
| `POC-PASS` | Test executed. Vulnerability confirmed with passing assertion. |
| `FUZZ-PASS` | Fuzzer (Foundry invariant / Medusa) found a counterexample. |
| `FORMALLY-VERIFIED` | Constraint analysis confirmed. *(ZK mode)* |

Verification status informs confidence and report language, but it does not mechanically determine severity. A Critical finding with `UNVERIFIED` status is treated differently from one with `POC-PASS` in terms of confidence, wording, and review pressure. Auditors are not blocked from shipping `CODE-TRACE` findings with appropriate caveats, but the status is visible and deliberate.

**Soft gate before report inclusion.** When the auditor marks a finding ready for the report, the platform prompts: "This finding is currently `CODE-TRACE`. Confirm you want to include it at this verification level, or upgrade to `POC-PASS` first." Not a hard block — a deliberate moment that forces a conscious decision about evidence quality.

---

### 10.7 Report Assembly and Export

The investigation artifact only saves time if it maps cleanly into the outputs audit firms already ship.

**Canonical finding schema.** Every finding stores: title, affected components, vulnerability class, severity, confidence, verification status, exploit preconditions, impact narrative, remediation sketch, and linked evidence artifacts.

**Report derivation.** Reports are generated from the finding graph and evidence trail, not rewritten from scratch. Client-facing claims should trace back to code traces, execution artifacts, annotations, or reviewer-authored reasoning. Agent responses can be linked as internal workflow context and investigative provenance, but they are not the sole evidentiary basis for a client-facing claim.

**Export targets.** v1 requires structured JSON export and a Markdown draft that a human can finish inside the firm's preferred template. Firm-specific formatting, executive-summary tuning, and PDF/Word template engines are deferred until the core finding schema is stable.

**Review workflow.** Before export, the platform highlights findings with low confidence, missing remediation text, or insufficient verification so the team can decide whether to strengthen or caveat them.

---

### 10.8 Feasibility Assessment

| Component | Feasibility | Estimated Effort |
|---|---|---|
| Foundry subprocess integration + result rendering | High | 4–6 weeks |
| Layered PoC scaffold generation | High | 3–4 weeks |
| Iterative refinement loop with versioned artifacts | Medium | 3–4 weeks |
| Revert trace agent analysis | Medium | 2–3 weeks |
| Structured finding export + Markdown report draft | High | 3–4 weeks |
| Firm-specific report template engine | Medium-Low | 4–6 weeks |
| ZK witness construction testing | Medium | 6–8 weeks |
| Constraint coverage analysis | Medium-Low | 8–12 weeks |
| Multi-prover aggregator harness | Low-Medium | 8–12 weeks |

Recommended v1 scope: Foundry integration, layered scaffold generation, inline result rendering, verification status tracking, and structured finding export with a Markdown report draft. ZK-specific verification and firm-specific template engines are deferred unless they become explicit launch requirements.

---

## 11. Language Extension and IR Abstraction

The v1 context pipeline, session graph, and agent prompts are Solidity-only in implementation but must not be Solidity-only in interface. If internal data structures are tightly coupled to Solidity's compilation model (inheritance trees, storage layout slots, proxy-specific patterns), adding Rust/Anchor or Move later will require rearchitecting the pipeline rather than extending it.

**Design constraint for v1:** the context pipeline, session graph node types, search/filter semantics, and agent prompt templates operate on a thin abstraction layer — not raw Solidity AST nodes. The abstraction exposes: callable entities (functions/methods), state locations (storage slots / account fields), call edges (direct, cross-contract, delegated), visibility/access control modifiers, and trust boundary annotations. The Solidity frontend populates this abstraction; future language frontends implement the same interface.

The cost of this constraint in v1 is low — it is a design discipline on internal interfaces, not additional features. The cost of not doing it is potentially high if the second language requires rewriting the context pipeline, session graph schema, and every agent prompt template.

**What the abstraction does not cover in v1:** language-specific concepts with no cross-language analogue (Solidity storage layout collisions, Move resource ownership, ZK constraint systems) remain as language-specific extensions to the base abstraction. The base layer handles the common case; language extensions handle the specialized reasoning.

---

## 12. Open Questions (Remaining)

**Agent response format per type.** State invariant checkers need different structure than historical pattern matchers. What is the correct output schema per agent type to maximize auditor utility without overwhelming them?

**Session boundary and branching.** When does a new investigation thread or hypothesis deserve a branched context rather than shared accumulation? Stale context from a different line of inquiry can still mislead the agent even if it is accurate.

**Remote inference deployment profile.** Which hosting options are required for the first external users: vendor API only, self-hosted, or firm-managed gateways?

**Collaboration permissions.** What are the exact semantics of share links, revocation, and role-based access for reviewers vs. contributors?

**Search roadmap.** After v1 string and structural filters, what is the smallest useful semantic search slice worth adding without bloating the IR?

**Archetype and overlay expansion criteria.** The v1 set is intentionally split between one protocol archetype (ERC-4626) and two overlays (proxy/upgrade, ERC-20). What is the process for validating and adding new protocol archetypes versus new overlays — engineering spike per pattern, community contribution, or engagement-driven extraction?

---

## 13. Non-Goals

- This platform is not an autonomous auditor. It does not run unsupervised and does not produce findings without human investigation.
- This platform is not an all-in-one product. It does not replace Foundry, Slither, or the engineer's code editor.
- This platform does not aim to be useful to non-security engineers. General developer accessibility is not a v1 design constraint.
- This platform does not enforce a workflow. Engineers use any subset of features in any order. There is no required process.
- This platform does not aim for CI/CD pipeline integration. That is a different product category with a different design philosophy.
- This platform does not aim for immediate multi-language parity in v1. Solidity / EVM is the starting slice.
- This platform does not aim for full firm-specific report templating in v1. Structured exports and Markdown drafts are sufficient for the first release.

---

*Document maintained by the core team. Update with each design iteration.*  
*v0.1 — Initial design philosophy, capabilities, UX, collaboration model.*  
*v0.2 — Added AI Agent Architecture (Section 9) and Verification System (Section 10).*  
*v0.3 — Aligned Sections 1, 3, 5.2, and 8 with four-mode agent architecture. Corrected inaccurate framing of AI participation as purely auditor-invoked.*  
*v0.4 — Narrowed v1 scope, added trust and degraded-mode requirements, expanded context pipeline details, and clarified reporting and verification semantics.*  
*v0.5 — Committed minimal v1 archetype set, clarified orientation agent, added agent evaluation infrastructure and IR abstraction boundary, tightened path enumeration, search filters, playbook degradation, and feasibility estimates.*
