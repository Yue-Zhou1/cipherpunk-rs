# Plan 4 — T2: Visibility & Control

> **For Agent:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Give users and operators clear visibility into what happened during an audit — every LLM call, tool action, engine outcome, and review decision — and persist a structured audit plan artifact that documents what the system decided to analyze and why.

**Architecture:** Item A extends the session event model with typed categories and adds query/aggregation capabilities. Item B derives a persistent, displayable audit plan artifact from deterministic workstation sources that already exist (`ProjectIr`-derived overview, checklist planning, tool recommendations, and engine config).

**Tech Stack:** Rust workspace crates, SQLite, serde, Axum, React + TypeScript, Tailwind CSS.

**Depends on:** Plan 1A (engine outcomes — surfaced in activity summary), Plan 1B (LLM provenance — recorded in session events).

---

## Planning Conventions

- Recommended execution context: a dedicated git worktree.
- Additive schema changes only — use `#[serde(default)]` for new fields.
- New session store tables use `CREATE TABLE IF NOT EXISTS` for safe migration.
- Observability data is informational — it does not affect finding generation or verification status.
- Deterministic engines and ProjectIR remain the source of truth for audit analysis; AI layers may assist planning, explanation, and recovery, but do not redefine deterministic results.
- Every task must land with tests.

## Release Gates

This plan is complete only when all of the following are true:

- `GET /api/sessions/:id/activity` returns a structured `ActivitySummary` with LLM, tool, review, and engine breakdowns.
- The activity console UI shows categorized, filterable events.
- An `AuditPlan` artifact is persisted after deterministic overview/checklist/tool planning completes.
- `GET /api/sessions/:id/plan` returns the structured plan.
- The workstation has an "Audit Plan" panel/tab showing the plan.
- The executive report includes a "Methodology" section derived from the plan.
- `cargo test -p session-store`, `cargo test -p session-manager`, `cargo test -p web-server` pass.
- `cd ui && npm run build` passes.

## Explicit Non-Goals

- Real-time metrics dashboards (Grafana/Prometheus integration).
- Cost estimation or billing — token tracking is for visibility, not invoicing.
- Editable audit plans — the plan is generated and persisted, not interactively modified.

---

## Item A: Structured Observability for Prompts, Tool Actions, and Review Decisions

### Context

`llm_call()` currently emits only `tracing::debug`. Tool action results are stored in `session_events` as opaque JSON with `event_type = "tool.action"`. Review decisions are logged as `event_type = "review.action"`. There is no aggregation, no typed querying, and no summary view. The activity console in the UI shows raw log lines.

### Tasks

#### Task 1: Extend AuditEvent with observability variants

**File:** `crates/apps/orchestrator/src/events.rs`

Expand the `AuditEvent` enum (Plan 1A already added `EngineCompleted` and `EngineFailed`):

```rust
pub enum AuditEvent {
    // From Plan 1A
    EngineCompleted { engine: String, findings_count: usize, duration_ms: u64 },
    EngineFailed { engine: String, reason: String },
    AuditCompleted { audit_id: String, output_dir: PathBuf, finding_count: usize },

    // New observability events
    LlmInteraction {
        role: String,
        provider: String,
        model: Option<String>,
        prompt_chars: usize,
        response_chars: usize,
        duration_ms: u64,
        succeeded: bool,
    },
    ToolActionCompleted {
        action_id: String,
        tool_family: String,
        target: String,
        status: String,
        duration_ms: u64,
    },
    ReviewDecisionApplied {
        record_id: String,
        action: String,
        analyst_note: Option<String>,
    },
}
```

Add `impl AuditEvent`:

```rust
impl AuditEvent {
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::EngineCompleted { .. } => "engine.completed",
            Self::EngineFailed { .. } => "engine.failed",
            Self::AuditCompleted { .. } => "audit.completed",
            Self::LlmInteraction { .. } => "llm.interaction",
            Self::ToolActionCompleted { .. } => "tool.action.completed",
            Self::ReviewDecisionApplied { .. } => "review.decision",
        }
    }

    pub fn to_session_event(&self, session_id: &str) -> session_store::SessionEvent {
        let now = chrono::Utc::now();
        session_store::SessionEvent {
            event_id: format!("{}:{}", self.event_type(), now.timestamp_micros()),
            event_type: self.event_type().to_string(),
            payload: serde_json::to_string(self).unwrap_or_default(),
            created_at: now,
        }
    }
}
```

Derive `Serialize, Deserialize` on `AuditEvent` for payload serialization.

---

#### Task 2: Add typed event queries to SessionStore

**File:** `crates/data/session-store/src/sqlite.rs`

Add methods:

```rust
/// List events filtered by type.
pub fn list_events_by_type(
    &self,
    session_id: &str,
    event_type: &str,
) -> Result<Vec<SessionEvent>> {
    let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("{e}"))?;
    let mut stmt = conn.prepare(
        "SELECT event_id, event_type, payload, created_at \
         FROM session_events \
         WHERE session_id = ?1 AND event_type = ?2 \
         ORDER BY created_at ASC"
    )?;
    // ... execute and collect
}

/// Count events by type for a session.
pub fn count_events_by_type(
    &self,
    session_id: &str,
) -> Result<HashMap<String, usize>> {
    let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("{e}"))?;
    let mut stmt = conn.prepare(
        "SELECT event_type, COUNT(*) \
         FROM session_events \
         WHERE session_id = ?1 \
         GROUP BY event_type"
    )?;
    // ... execute and collect into HashMap
}
```

**Tests:** Insert events of various types. Verify `list_events_by_type` filters correctly. Verify `count_events_by_type` returns correct counts.

---

#### Task 3: Build ActivitySummary aggregation

**File:** `crates/services/session-manager/src/workflow.rs`

Add IPC types:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivitySummary {
    pub session_id: String,
    pub llm_calls: Vec<LlmCallSummary>,
    pub tool_actions: Vec<ToolActionSummary>,
    pub review_decisions: Vec<ReviewDecisionSummary>,
    pub engine_outcomes: Vec<EngineOutcomeView>,
    pub total_events: usize,
    pub total_duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmCallSummary {
    pub role: String,
    pub count: usize,
    pub avg_duration_ms: u64,
    pub total_prompt_chars: usize,
    pub total_response_chars: usize,
    pub providers_used: Vec<String>,
    pub succeeded: usize,
    pub failed: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolActionSummary {
    pub tool_family: String,
    pub count: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub avg_duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewDecisionSummary {
    pub action: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineOutcomeView {
    pub engine: String,
    pub status: String,
    pub findings_count: usize,
    pub duration_ms: u64,
}
```

---

#### Task 4: Implement activity summary loading

**File:** `crates/services/session-manager/src/state.rs`

Add method:

```rust
pub fn load_activity_summary(
    &self,
    session_id: &str,
) -> Result<ActivitySummary> {
    let store = self.session_store.as_ref()
        .ok_or_else(|| anyhow::anyhow!("no session store"))?;

    let events = store.list_events(session_id)?;

    // Parse events by type and aggregate:
    let mut llm_by_role: HashMap<String, Vec<LlmInteractionData>> = HashMap::new();
    let mut tool_by_family: HashMap<String, Vec<ToolActionData>> = HashMap::new();
    let mut review_by_action: HashMap<String, usize> = HashMap::new();
    let mut engine_outcomes: Vec<EngineOutcomeView> = Vec::new();

    for event in &events {
        match event.event_type.as_str() {
            "llm.interaction" => {
                if let Ok(data) = serde_json::from_str::<LlmInteractionEvent>(&event.payload) {
                    llm_by_role.entry(data.role.clone()).or_default().push(data);
                }
            }
            "tool.action.completed" => {
                if let Ok(data) = serde_json::from_str::<ToolActionEvent>(&event.payload) {
                    tool_by_family.entry(data.tool_family.clone()).or_default().push(data);
                }
            }
            "review.decision" => {
                if let Ok(data) = serde_json::from_str::<ReviewDecisionEvent>(&event.payload) {
                    *review_by_action.entry(data.action.clone()).or_default() += 1;
                }
            }
            "engine.completed" | "engine.failed" => {
                // Parse and add to engine_outcomes
            }
            _ => {}
        }
    }

    // Build summary structs from aggregated data
    // ... (compute averages, counts, provider lists)

    Ok(ActivitySummary {
        session_id: session_id.to_string(),
        llm_calls: /* ... */,
        tool_actions: /* ... */,
        review_decisions: /* ... */,
        engine_outcomes,
        total_events: events.len(),
        total_duration_ms: /* sum of all event durations */,
    })
}
```

**File:** `crates/services/session-manager/src/lib.rs`

Add delegation:

```rust
pub async fn load_activity_summary(
    &self,
    session_id: &str,
) -> SessionResult<ActivitySummary> {
    let state = self.inner.lock().await;
    state.load_activity_summary(session_id).map_err(map_state_error)
}
```

**Tests:** Create session, insert mock events of each type, call `load_activity_summary`. Verify counts, averages, and provider lists are correct.

---

#### Task 5: Add activity summary API endpoint

**File:** `crates/apps/web-server/src/lib.rs`

Add route:

```rust
.route("/api/sessions/:session_id/activity", get(load_activity_summary))
```

Handler:

```rust
async fn load_activity_summary(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<ActivitySummary>, AppError> {
    let summary = state.manager.load_activity_summary(&session_id).await?;
    Ok(Json(summary))
}
```

---

#### Task 6: Emit LLM interaction events from enforcement layer

**File:** `crates/services/llm/src/enforcement.rs`

After each `llm_call_traced()` (both success and failure), if an event sink is available, emit `AuditEvent::LlmInteraction`. This requires passing an optional event sink or session store reference through the enforcement layer.

Design decision: Rather than threading the event sink through every LLM call, use tracing structured logging as the primary mechanism (already done in Plan 1B's `llm_call_traced`), and have a tracing subscriber in the orchestrator/session-manager that captures structured log events and persists them as session events.

Alternative (simpler): Add an optional `session_event_recorder: Option<Arc<dyn Fn(AuditEvent)>>` callback to `ContractEnforcer`. The orchestrator sets this when creating the copilot service.

Choose the simpler approach for now — the tracing-subscriber approach can be added later.

```rust
impl<T: DeserializeOwned> ContractEnforcer<T> {
    pub fn with_event_recorder(
        mut self,
        recorder: Arc<dyn Fn(AuditEvent) + Send + Sync>,
    ) -> Self {
        self.event_recorder = Some(recorder);
        self
    }
}
```

After each LLM call in the enforcement loop, call the recorder if present.

---

#### Task 7: Enhance activity console UI

**File:** `ui/src/features/workstation/ActivityConsole.tsx`

Current state: shows raw log lines from `tail_session_console`.

Add a parallel data source: fetch `ActivitySummary` from `/api/sessions/:id/activity`.

Add filter tabs above the console:

```tsx
type ActivityFilter = "all" | "llm" | "tools" | "reviews" | "engines";

const [filter, setFilter] = useState<ActivityFilter>("all");

<div className="flex gap-1 px-3 py-1 border-b border-gray-200 bg-gray-50">
  {(["all", "llm", "tools", "reviews", "engines"] as const).map(f => (
    <button
      key={f}
      onClick={() => setFilter(f)}
      className={`text-xs px-2 py-0.5 rounded ${filter === f ? "bg-blue-100 text-blue-700" : "text-gray-600"}`}
    >
      {f.charAt(0).toUpperCase() + f.slice(1)}
      {summary && f !== "all" && (
        <span className="ml-1 text-gray-400">
          ({getCountForFilter(summary, f)})
        </span>
      )}
    </button>
  ))}
</div>
```

When filter is active, show only matching entries. For the summary header:

```tsx
{summary && (
  <div className="px-3 py-2 text-xs text-gray-500 border-b">
    {summary.llm_calls.reduce((s, c) => s + c.count, 0)} LLM calls ·
    {summary.tool_actions.reduce((s, c) => s + c.count, 0)} tool actions ·
    {summary.review_decisions.reduce((s, c) => s + c.count, 0)} reviews ·
    {summary.engine_outcomes.length} engines
  </div>
)}
```

Add provider/model badges for LLM entries:

```tsx
{entry.source === "llm.interaction" && (
  <span className="text-xs bg-purple-100 text-purple-700 rounded px-1 ml-1">
    {entry.provider}/{entry.model}
  </span>
)}
```

Add status badges for tool action entries:

```tsx
{entry.source === "tool.action.completed" && (
  <span className={`text-xs rounded px-1 ml-1 ${
    entry.status === "Completed" ? "bg-green-100 text-green-700" : "bg-red-100 text-red-700"
  }`}>
    {entry.status}
  </span>
)}
```

**Test:** Vitest: render activity console with mock summary data. Verify filter tabs show counts. Verify clicking "llm" filter shows only LLM entries.

---

## Item B: Session-Scoped Audit Plan Artifact

### Context

The workstation already exposes deterministic planning inputs derived from `ProjectIr` and session state: security overview data, checklist planning data, tool recommendations, and engine configuration. Those inputs are visible piecemeal, but they are not persisted as a single reviewable artifact that answers "what did the system decide to analyze and why?" The initial `AuditPlan` should be built from those existing deterministic sources rather than assuming AI job outputs are already the source of truth.

### Tasks

#### Task 8: Define AuditPlan type

**File:** `crates/core/src/session.rs`

Add:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AuditPlan {
    pub plan_id: String,
    pub session_id: String,
    pub overview: AuditPlanOverview,
    pub domains: Vec<AuditPlanDomain>,
    pub recommended_tools: Vec<AuditPlanTool>,
    pub engines: AuditPlanEngines,
    pub rationale: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AuditPlanOverview {
    pub assets: Vec<String>,
    pub trust_boundaries: Vec<String>,
    pub hotspots: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AuditPlanDomain {
    pub id: String,
    pub rationale: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AuditPlanTool {
    pub tool: String,
    pub rationale: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AuditPlanEngines {
    pub crypto_zk: bool,
    pub distributed: bool,
}
```

`AuditPlan` is a first-class artifact type. Do **not** add it to `AuditRecordKind`; review records and planning artifacts have different lifecycle semantics.

---

#### Task 9: Generate and persist audit plan from deterministic workstation sources

**File:** `crates/services/session-manager/src/state.rs`

Construct and persist the plan from deterministic sources already available in the workstation flow:
- `ProjectIr.security_overview()` or equivalent overview source
- `ProjectIr.checklist_plan()` or the checklist-plan response already exposed to the UI
- `KnowledgeBase` tool recommendations
- session engine configuration

```rust
fn generate_audit_plan(
    session: &AuditSession,
    overview: &SecurityOverview,
    checklist_plan: &ChecklistPlan,
    tool_recommendations: &[ToolRecommendation],
    config: &AuditConfig,
) -> AuditPlan {
    let plan = AuditPlan {
        plan_id: format!("plan-{}", chrono::Utc::now().timestamp_micros()),
        session_id: session.session_id.clone(),
        overview: AuditPlanOverview {
            assets: overview.assets.clone(),
            trust_boundaries: overview.trust_boundaries.clone(),
            hotspots: overview.hotspots.clone(),
        },
        domains: checklist_plan.domains.iter().map(|d| AuditPlanDomain {
            id: d.id.clone(),
            rationale: d.rationale.clone(),
        }).collect(),
        recommended_tools: tool_recommendations.iter().map(|t| AuditPlanTool {
            tool: t.tool.clone(),
            rationale: t.rationale.clone(),
        }).collect(),
        engines: AuditPlanEngines {
            crypto_zk: config.engines.crypto_zk,
            distributed: config.engines.distributed,
        },
        rationale: format!(
            "Generated from workspace analysis of {} target crates with {} detected frameworks.",
            config.scope.target_crates.len(),
            config.scope.detected_frameworks.len(),
        ),
        created_at: chrono::Utc::now(),
    };
    plan
}
```

Persist the full `AuditPlan` as a first-class artifact. The simplest initial implementation is a typed session event:
```rust
let plan_event = SessionEvent {
    event_id: format!("audit-plan:{}", plan.plan_id),
    event_type: "audit.plan.generated".to_string(),
    payload: serde_json::to_string(&plan)?,
    created_at: chrono::Utc::now(),
};
store.append_event(session_id, &plan_event)?;
```

If a dedicated artifact table is added later, it should store the same `AuditPlan` payload without routing through `AuditRecord`.

---

#### Task 10: Add plan loading API

**File:** `crates/services/session-manager/src/state.rs`

```rust
pub fn load_audit_plan(
    &self,
    session_id: &str,
) -> Result<AuditPlanResponse> {
    let store = self.session_store.as_ref()
        .ok_or_else(|| anyhow::anyhow!("no session store"))?;

    // Find the audit plan event
    let events = store.list_events_by_type(session_id, "audit.plan.generated")?;
    let plan_event = events.last()
        .ok_or_else(|| anyhow::anyhow!("no audit plan generated for this session"))?;

    let plan: AuditPlan = serde_json::from_str(&plan_event.payload)?;

    Ok(AuditPlanResponse {
        session_id: session_id.to_string(),
        plan_id: plan.plan_id,
        overview: AuditPlanOverviewView {
            assets: plan.overview.assets,
            trust_boundaries: plan.overview.trust_boundaries,
            hotspots: plan.overview.hotspots,
        },
        domains: plan.domains.iter().map(|d| ChecklistDomainPlanResponse {
            id: d.id.clone(),
            rationale: d.rationale.clone(),
        }).collect(),
        recommended_tools: plan.recommended_tools.iter().map(|t| ToolRecommendationView {
            tool: t.tool.clone(),
            rationale: t.rationale.clone(),
        }).collect(),
        engines: EngineSelectionView {
            crypto_zk: plan.engines.crypto_zk,
            distributed: plan.engines.distributed,
        },
        rationale: plan.rationale,
        created_at: plan.created_at.to_rfc3339(),
    })
}
```

**File:** `crates/services/session-manager/src/workflow.rs`

Add IPC response types:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditPlanResponse {
    pub session_id: String,
    pub plan_id: String,
    pub overview: AuditPlanOverviewView,
    pub domains: Vec<ChecklistDomainPlanResponse>,
    pub recommended_tools: Vec<ToolRecommendationView>,
    pub engines: EngineSelectionView,
    pub rationale: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditPlanOverviewView {
    pub assets: Vec<String>,
    pub trust_boundaries: Vec<String>,
    pub hotspots: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolRecommendationView {
    pub tool: String,
    pub rationale: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineSelectionView {
    pub crypto_zk: bool,
    pub distributed: bool,
}
```

---

#### Task 11: Add plan API endpoint

**File:** `crates/apps/web-server/src/lib.rs`

Add route:

```rust
.route("/api/sessions/:session_id/plan", get(load_audit_plan))
```

Handler:

```rust
async fn load_audit_plan(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<AuditPlanResponse>, AppError> {
    let plan = state.manager.load_audit_plan(&session_id).await?;
    Ok(Json(plan))
}
```

Return 404 if no plan exists yet (plan is generated after the deterministic planning inputs have been materialized for the session).

---

#### Task 12: Build AuditPlanPanel UI component

**File:** `ui/src/features/workstation/AuditPlanPanel.tsx` (new)

```tsx
import { useEffect, useState } from "react";
import type { AuditPlanResponse } from "../../ipc/commands";

type AuditPlanPanelProps = {
  sessionId: string;
};

export function AuditPlanPanel({ sessionId }: AuditPlanPanelProps) {
  const [plan, setPlan] = useState<AuditPlanResponse | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    loadAuditPlan(sessionId)
      .then(setPlan)
      .catch((err) => setError(err.message))
      .finally(() => setLoading(false));
  }, [sessionId]);

  if (loading) return <div className="p-4 text-gray-500">Loading plan...</div>;
  if (error) return <div className="p-4 text-gray-500">No audit plan generated yet.</div>;
  if (!plan) return null;

  return (
    <div className="p-4 space-y-4 overflow-y-auto">
      <h2 className="text-lg font-semibold">Audit Plan</h2>
      <p className="text-sm text-gray-600">{plan.rationale}</p>

      {/* Architecture Overview */}
      <Section title="Architecture Overview">
        <SubSection title="Assets" items={plan.overview.assets} />
        <SubSection title="Trust Boundaries" items={plan.overview.trustBoundaries} />
        <SubSection title="Hotspots" items={plan.overview.hotspots} />
      </Section>

      {/* Selected Domains */}
      <Section title="Analysis Domains">
        {plan.domains.map((d) => (
          <div key={d.id} className="ml-2 mb-2">
            <span className="font-medium text-sm">{d.id}</span>
            <p className="text-xs text-gray-500 ml-2">{d.rationale}</p>
          </div>
        ))}
      </Section>

      {/* Recommended Tools */}
      <Section title="Recommended Tools">
        {plan.recommendedTools.map((t) => (
          <div key={t.tool} className="ml-2 mb-2">
            <span className="font-medium text-sm">{t.tool}</span>
            <p className="text-xs text-gray-500 ml-2">{t.rationale}</p>
          </div>
        ))}
      </Section>

      {/* Engine Selection */}
      <Section title="Engines">
        <div className="ml-2 text-sm space-y-1">
          <div>Crypto/ZK: <Badge enabled={plan.engines.cryptoZk} /></div>
          <div>Distributed: <Badge enabled={plan.engines.distributed} /></div>
        </div>
      </Section>
    </div>
  );
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  const [open, setOpen] = useState(true);
  return (
    <div className="border border-gray-200 rounded">
      <button onClick={() => setOpen(!open)}
        className="w-full text-left px-3 py-2 text-sm font-medium bg-gray-50 hover:bg-gray-100">
        {open ? "▾" : "▸"} {title}
      </button>
      {open && <div className="px-3 py-2">{children}</div>}
    </div>
  );
}

function SubSection({ title, items }: { title: string; items: string[] }) {
  if (items.length === 0) return null;
  return (
    <div className="mb-2">
      <span className="text-xs font-medium text-gray-600">{title}</span>
      <ul className="ml-2">
        {items.map((item, i) => (
          <li key={i} className="text-xs text-gray-700">• {item}</li>
        ))}
      </ul>
    </div>
  );
}

function Badge({ enabled }: { enabled: boolean }) {
  return (
    <span className={`text-xs px-1 rounded ${enabled ? "bg-green-100 text-green-700" : "bg-gray-100 text-gray-500"}`}>
      {enabled ? "enabled" : "disabled"}
    </span>
  );
}
```

---

#### Task 13: Add AuditPlanPanel to workstation

**File:** `ui/src/features/workstation/WorkstationShell.tsx`

Add "Audit Plan" as a tab or panel option in the workstation layout. Position it alongside SecurityOverviewPanel and ChecklistPanel — it's a reference document that helps the analyst understand the audit scope.

If the workstation uses a tab system, add it as a tab. If it uses a panel grid, add it as a collapsible panel in the sidebar area.

---

#### Task 14: Include audit plan in executive report

**File:** `crates/services/report/src/generator.rs`

When generating the executive report, if an audit plan artifact exists:

1. Add a "Methodology" section after the executive summary and before findings.
2. Content:
   ```markdown
   ## Methodology

   The following analysis plan was generated based on workspace analysis.

   **Architecture Overview:**
   - Assets: {asset_count} identified
   - Trust Boundaries: {boundary_count} identified
   - Hotspots: {hotspot_count} identified

   **Analysis Domains:**
   {for each domain: "- **{id}**: {rationale}"}

   **Tools Used:**
   {for each tool: "- **{tool}**: {rationale}"}

   **Engines:**
   - Crypto/ZK: {enabled/disabled}
   - Distributed: {enabled/disabled}
   ```

Load the plan from the session store artifact/event path (if available) or from `AuditOutputs` if a future stage explicitly exports it.

**Test:** Generate report with an audit plan. Verify "Methodology" section appears in markdown output with correct content.

---

## Dependency Map

```
Task 1  (event types)        ← no deps
Task 2  (store queries)      ← no deps
Task 3  (summary types)      ← no deps
Task 4  (summary loading)    ← Task 1, Task 2, Task 3
Task 5  (API endpoint)       ← Task 4
Task 6  (event emission)     ← Task 1, Plan 1B enforcement
Task 7  (UI console)         ← Task 5

Task 8  (plan types)         ← no deps
Task 9  (plan generation)    ← Task 8
Task 10 (plan loading)       ← Task 9
Task 11 (plan API)           ← Task 10
Task 12 (plan UI)            ← Task 11
Task 13 (workstation)        ← Task 12
Task 14 (report)             ← Task 9
```

Items A and B are independent and can be developed in parallel.
