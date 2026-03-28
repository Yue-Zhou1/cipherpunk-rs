# Explorer API Integration Design

> Replace fixture data in the CodebaseExplorer Graph tab with real backend data via a dedicated API endpoint, using on-demand hierarchical loading.

**Spec:** `docs/superpowers/specs/2026-03-28-explorer-api-integration-design.md`
**Related:** `docs/superpowers/specs/2026-03-26-codebase-explorer-design.md`

---

## 1. API Contract

### Endpoint

```
GET /api/sessions/:session_id/explorer-graph
```

### Query Parameters

| Param | Values | Default | Purpose |
|-------|--------|---------|---------|
| `depth` | `overview` \| `full` | `overview` | Controls detail level |
| `cluster` | `<node_id>` | — | Fetches children of a specific cluster on expansion |

### Request Patterns

1. **Initial load** — `?depth=overview` returns crates + modules + file stubs with `childCount`. No symbols, no cross-file edges.
2. **Cluster expansion** — `?cluster=crt_a3f8c2d1e405` returns children (files, symbols, signatures, edges) of that cluster.
3. **Full load** — `?depth=full` returns everything. Used only for small codebases where overview is unnecessary.

### Response Shape

Maps directly to frontend `ExplorerGraph` type — no conversion layer needed.

```json
{
  "sessionId": "sess-123",
  "nodes": [
    {
      "id": "crt_a3f8c2d1e405",
      "label": "engine-crypto",
      "kind": "crate",
      "childCount": 24
    },
    {
      "id": "sym_b7e2f401c930",
      "label": "verify_signature",
      "kind": "function",
      "filePath": "engine-crypto/src/verify.rs",
      "line": 42,
      "signature": {
        "parameters": [
          { "name": "msg", "typeAnnotation": "&[u8]", "position": 0 },
          { "name": "sig", "typeAnnotation": "&Signature", "position": 1 }
        ],
        "returnType": "Result<bool>"
      }
    }
  ],
  "edges": [
    {
      "from": "crt_a3f8c2d1e405",
      "to": "mod_c4d91a02b718",
      "relation": "contains"
    },
    {
      "from": "sym_b7e2f401c930",
      "to": "sym_d2c8e503a1f7",
      "relation": "parameter_flow",
      "parameterName": "msg",
      "parameterPosition": 0
    }
  ]
}
```

### Node ID Format

Deterministic hash-based IDs: 3-char type prefix + `_` + 12-char hex hash of qualified path.

| Prefix | Node kind |
|--------|-----------|
| `crt_` | crate |
| `mod_` | module |
| `fil_` | file |
| `sym_` | symbol (function, trait_impl_method, macro_call) |

Same source always produces the same ID. Fixed-length regardless of codebase size.

Uses a stable hash (FNV-1a) to ensure determinism across Rust versions. 12 hex chars (48 bits) to keep collision probability negligible up to ~16M nodes.

```rust
fn hash_id(prefix: &str, qualified_path: &str) -> String {
    // FNV-1a — stable across Rust versions, unlike DefaultHasher
    let mut h: u64 = 0xcbf29ce484222325;
    for byte in qualified_path.bytes() {
        h ^= byte as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{}_{:012x}", prefix, h & 0xFFFF_FFFF_FFFF)
}
// hash_id("sym", "engine-crypto/src/verify.rs::verify_signature")
// → "sym_a3f8c2d1e405"
```

### Parameter Precedence

`depth` and `cluster` are mutually exclusive. If `cluster` is provided, `depth` is ignored. The handler validates this and returns `400 Bad Request` if `depth=full` and `cluster` are both provided.

### Error Responses

Follows the existing `ApiErrorEnvelope` pattern:

| Code | When |
|------|------|
| `SESSION_NOT_FOUND` | Invalid session ID |
| `PROJECT_IR_NOT_BUILT` | Analysis not yet complete for this session |
| `UNKNOWN_CLUSTER` | `cluster` param references a non-existent node ID |

---

## 2. Backend — Explorer Graph Builder

### Location

New module: `crates/services/session-manager/src/explorer_graph.rs`

### Core Struct

```rust
pub struct ExplorerGraphBuilder<'a> {
    ir: &'a ProjectIr,
    root: &'a Path,
}
```

### Build Pipeline

```
ProjectIr
  ├── file_graph
  ├── symbol_graph
  ├── dataflow_graph
  └── feature_graph
           │
           ▼
  ExplorerGraphBuilder::build()
    1. Scan Cargo workspace     → detect crate boundaries
    2. Infer modules            → group files by directory path
    3. Build hierarchy          → crate→module→file→symbol contains edges
    4. Resolve signatures       → attach FunctionSignature from SymbolNode
    5. Merge dataflow edges     → map to parameter_flow / return_flow
    6. Generate hash IDs        → deterministic per node
    7. Attach child counts      → count children per cluster node
           │
           ▼
  ExplorerGraphResponse
```

### Depth Filtering

- `depth=overview` — steps 1-3 and 6-7 only. Returns crate/module/file nodes with `childCount` and `contains` edges. No symbols, no cross-file edges.
- `depth=full` — full pipeline, all nodes and edges.
- `cluster=<id>` — full pipeline, filtered to children of the requested cluster and their interconnecting edges.

### Response Types

Dedicated types, separate from the existing `ProjectGraphResponse`:

```rust
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplorerGraphResponse {
    pub session_id: String,
    pub nodes: Vec<ExplorerNodeResponse>,
    pub edges: Vec<ExplorerEdgeResponse>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplorerNodeResponse {
    pub id: String,
    pub label: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<FunctionSignatureResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub child_count: Option<u32>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionSignatureResponse {
    pub parameters: Vec<ParameterInfoResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_type: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParameterInfoResponse {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub type_annotation: Option<String>,
    pub position: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplorerEdgeResponse {
    pub from: String,
    pub to: String,
    pub relation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameter_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameter_position: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_preview: Option<String>,
}
```

### Design Constraints

- `ExplorerGraphBuilder` is stateless per request — no internal caching
- Takes `&ProjectIr` by reference, never clones the full IR
- Caching handled at session manager level if needed later

---

## 3. Backend — Axum Route & Session Manager

### Route Registration

In `crates/apps/web-server/src/lib.rs`, alongside existing `/graphs/:lens` routes:

```
GET /api/sessions/:session_id/explorer-graph
```

No changes to existing endpoints.

### Handler

```rust
#[derive(Deserialize)]
pub struct ExplorerGraphQuery {
    #[serde(default = "default_depth")]
    depth: String,
    cluster: Option<String>,
}

async fn load_explorer_graph(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Query(query): Query<ExplorerGraphQuery>,
) -> Result<Json<ExplorerGraphResponse>, AppError>
```

### ExplorerDepth Enum

```rust
pub enum ExplorerDepth {
    Overview,
    Full,
}
```

### Session Manager Method

```rust
impl SessionManager {
    pub async fn load_explorer_graph(
        &self,
        session_id: &str,
        depth: ExplorerDepth,
        cluster: Option<&str>,
    ) -> Result<ExplorerGraphResponse>
}
```

Retrieves cached `ProjectIr` for the session, passes to `ExplorerGraphBuilder::build()`, returns response.

### Data Freshness Event

When the session manager finishes re-analysis (updates `ProjectIr`), it emits `explorer_graph_stale` on the existing event bus.

---

## 4. Frontend — Transport Layer

### New Transport Route

In `ui/src/ipc/transport.ts`, add to `COMMAND_ROUTES`:

```typescript
load_explorer_graph: {
  method: "GET",
  path: (args) => {
    const sid = encodeURIComponent(String(args.session_id ?? ""));
    const params = new URLSearchParams();
    if (args.depth) params.set("depth", String(args.depth));
    if (args.cluster) params.set("cluster", String(args.cluster));
    const qs = params.toString();
    return `/api/sessions/${sid}/explorer-graph${qs ? `?${qs}` : ""}`;
  },
},
```

### Tiered Timeouts

Built into the command function, not a shared constant:

| Request | Timeout |
|---------|---------|
| `?depth=overview` | 3s |
| `?cluster=<id>` | 5s |
| `?depth=full` | 15s |

### New Command Function

In `ui/src/ipc/commands.ts`:

```typescript
export async function loadExplorerGraph(
  sessionId: string,
  depth?: "overview" | "full",
  cluster?: string,
): Promise<ExplorerGraphResponse> {
  return getTransport().invoke("load_explorer_graph", {
    session_id: sessionId,
    depth,
    cluster,
  });
}
```

No fixture fallback. Backend unavailable = error state.

### Response Types

In `ui/src/ipc/commands.ts`:

```typescript
export type ExplorerGraphResponse = {
  sessionId: string;
  nodes: ExplorerNodeResponse[];
  edges: ExplorerEdgeResponse[];
};

export type ExplorerNodeResponse = {
  id: string;
  label: string;
  kind: string;
  filePath?: string;
  line?: number;
  signature?: {
    parameters: { name: string; typeAnnotation?: string; position: number }[];
    returnType?: string;
  };
  childCount?: number;
};

export type ExplorerEdgeResponse = {
  from: string;
  to: string;
  relation: string;
  parameterName?: string;
  parameterPosition?: number;
  valuePreview?: string;
};
```

---

## 5. Frontend — Type Updates

### `ExplorerNode` in `types.ts`

Add `childCount` to support on-demand loading (overview nodes carry child counts before their children are loaded):

```typescript
export type ExplorerNode = {
  id: string;
  label: string;
  kind: ExplorerNodeKind;
  filePath?: string;
  line?: number;
  signature?: FunctionSignature;
  childCount?: number;  // NEW — from backend, used by ClusterNode
};
```

Update `AdaptiveLayout.tsx` to prefer `node.childCount` over counting `contains` edges. The `contains`-edge counting remains as a fallback for expanded clusters (where children are already loaded).

### `ExplorerNodeKind` and `ExplorerEdgeRelation`

The backend sends `kind` and `relation` as plain strings. The frontend union types (`"crate" | "module" | ...`) serve as documentation and type safety. The backend must only emit values within these unions. If the backend adds new kinds (e.g., `"struct"`, `"enum"`), the frontend union must be updated in lockstep. This is enforced by backend tests that assert the set of emitted kind/relation values.

### `AdaptiveLayout.tsx` — ID parsing removal

The current `parentModuleId()` and `parentCrateId()` functions parse the node ID string to infer hierarchy (e.g., extracting path segments from `"module:crates/intake"`). With hash-based IDs, this breaks. Replace with `contains` edge traversal: build a `parentMap: Map<string, string>` from `contains` edges on load, then look up `parentMap.get(nodeId)` instead of parsing the ID.

### `sessionId` handling

`sessionId` is stored in the `useUnifiedGraph` hook state (passed as a parameter), not inside `ExplorerGraph`. The `ExplorerProvider` must accept `sessionId` as a prop and pass it to `useUnifiedGraph(sessionId)`. The `mergeClusterData` function operates on `ExplorerGraph` (nodes + edges only) — `sessionId` is not part of the graph model.

---

## 6. Frontend — useUnifiedGraph Hook Rewrite

### Signature

```typescript
export function useUnifiedGraph(sessionId: string): {
  graph: ExplorerGraph;
  nodeMap: Map<string, ExplorerNode>;
  isLoading: boolean;
  loadingClusters: Set<string>;
  error: string | null;
  isStale: boolean;
  expandCluster: (clusterId: string) => void;
  reload: () => void;
}
```

### Behavior

1. **On mount** — calls `loadExplorerGraph(sessionId, "overview")`. Sets `isLoading = true`. On success, indexes nodes into `nodeMap`. On failure, sets `error`.
2. **On `expandCluster(id)`** — calls `loadExplorerGraph(sessionId, undefined, id)`. Adds `id` to `loadingClusters` set. Merges returned children into existing graph with deduplication. Removes `id` from `loadingClusters` on completion. Tracks `id` in `loadedClusters` set to skip re-fetching on collapse/re-expand.
3. **On `reload()`** — cancels all in-flight expansion requests via cancellation flag, clears `isStale`, `loadingClusters`, `loadedClusters`, resets graph, re-fetches overview.
4. **Cleanup** — cancellation flag pattern to prevent state updates after unmount.

### Race Condition Handling

- **Concurrent cluster expansions:** `loadingClusters` is a `Set<string>`, not a single slot. Multiple clusters can load in parallel without overwriting each other's state.
- **Stale event during initial load:** If `explorer_graph_stale` fires while `isLoading` is true, suppress it — the data being loaded is already the latest.
- **Reload during active expansion:** `reload()` increments a cancellation generation counter. In-flight expansion callbacks check the counter before merging — if it changed, they discard their results silently.
- **Re-expansion of loaded clusters:** `loadedClusters: Set<string>` tracks clusters whose children are already in the graph. `expandCluster` skips the API call and just toggles UI expansion for these.

### O(1) Node Index

```typescript
const nodeMap = useMemo(() => {
  const map = new Map<string, ExplorerNode>();
  for (const node of graph.nodes) {
    map.set(node.id, node);
  }
  return map;
}, [graph.nodes]);
```

Exposed through `ExplorerContextValue`. All components use `nodeMap.get(id)` for lookups.

### Merge on Cluster Expansion

```typescript
function mergeClusterData(
  current: ExplorerGraph,
  expansion: ExplorerGraphResponse,
): ExplorerGraph {
  const existingIds = new Set(current.nodes.map(n => n.id));
  const newNodes = expansion.nodes.filter(n => !existingIds.has(n.id));
  const existingEdgeKeys = new Set(
    current.edges.map(e => `${e.from}→${e.to}→${e.relation}`)
  );
  const newEdges = expansion.edges.filter(
    e => !existingEdgeKeys.has(`${e.from}→${e.to}→${e.relation}`)
  );
  return {
    nodes: [...current.nodes, ...newNodes],
    edges: [...current.edges, ...newEdges],
  };
}
```

### Stale Data Subscription

```typescript
useEffect(() => {
  const unsubscribe = getTransport().subscribe<{ event: string }>(
    "explorer_graph_stale",
    sessionId,
    (payload) => {
      if (payload.event === "explorer_graph_stale") {
        setIsStale(true);
      }
    },
  );
  return unsubscribe;
}, [sessionId]);
```

Note: The current `HttpTransport.subscribe()` does not filter by event type — it delivers all WebSocket messages for the session. The hook must filter by `payload.event` itself. A transport-level event filter is a separate improvement outside this spec's scope.

---

## 7. Frontend — ExplorerContext Updates

### New Fields on ExplorerContextValue

```typescript
// Added to existing ExplorerContextValue:
nodeMap: Map<string, ExplorerNode>;
isLoading: boolean;
loadingClusters: Set<string>;
error: string | null;
isStale: boolean;
expandCluster: (clusterId: string) => void;
reload: () => void;
```

### Component Impacts

- `useFocusContext` and `useTrace` — replace internal linear scans with `nodeMap.get(id)` calls
- `ClusterNode.tsx` — when `loadingClusters.has(node.id)`, show spinner instead of expand indicator
- `ExplorerCanvas.tsx` — on cluster node click, call `expandCluster(id)` instead of just `toggleCluster(id)`
- `ExplorerProvider` — accept `sessionId` prop, pass to `useUnifiedGraph(sessionId)`, expose all new fields on context

---

## 8. Loading & Error UI States

### Three States

| State | Display | When |
|-------|---------|------|
| Initial loading | Centered spinner + "Loading project graph..." in canvas area. Toolbar visible but disabled. | First mount, before overview arrives |
| Error | Error banner with message + "Retry" button, replacing canvas area. Toolbar visible but disabled. | Timeout, network failure, server error |
| Loaded | Normal graph canvas + toolbar | Overview received successfully |

### Per-Cluster Loading

Only the cluster being expanded shows a loading indicator. All other clusters remain interactive.

### Per-Cluster Error

If a cluster expansion fails, show error inline on that cluster node ("Failed to load. Click to retry."). Don't break the entire view.

### Stale Data Banner

Appears above the toolbar when `isStale = true`:

```
┌─────────────────────────────────────────────┐
│ ⟳ Graph data has been updated.  [Reload]    │
└─────────────────────────────────────────────┘
```

Does not interrupt focus/trace state. User reloads when ready. After reload, focus/trace state is cleared.

---

## 9. Cleanup — Removals

### Files to Delete

| File | Reason |
|------|--------|
| `CodebaseExplorer/fixtures/mockGraph.ts` | Replaced by backend API |

### Remove from `ui/src/ipc/commands.ts`

- `loadFileGraph()`
- `loadFeatureGraph()`
- `loadDataflowGraph()`
- `loadSymbolGraph()`
- `invokeGraphCommand()` helper
- `isGraphTimeoutError()` helper
- `GraphLensKind` type
- All `*Fallback` graph functions and related `loadCommandFixtures()` imports

### Remove from `ui/src/ipc/commands.fixtures.ts`

- `loadFileGraphFallback()`
- `loadFeatureGraphFallback()`
- `loadDataflowGraphFallback()`
- `loadSymbolGraphFallback()`
- Any `ProjectGraphResponse` import that becomes unused after removal

### Remove from `ui/src/ipc/transport.ts`

- `GRAPH_TIMEOUT_MS` constant
- `GRAPH_COMMANDS` set
- Special-case timeout check in HTTP invoke logic
- `load_file_graph` route from `COMMAND_ROUTES`
- `load_feature_graph` route
- `load_dataflow_graph` route
- `load_symbol_graph` route

### Backend — No Deletions

Existing `/graphs/:lens` Axum routes stay. They may serve other consumers. Only the frontend side is cleaned up.

---

## 10. Testing Strategy

Fixtures are removed from production. Tests use inline minimal data per test.

### Test Helper

```typescript
function makeTestGraph(overrides?: Partial<ExplorerGraph>): ExplorerGraph {
  return {
    nodes: overrides?.nodes ?? [
      { id: "sym_aaa", label: "foo", kind: "function" },
      { id: "sym_bbb", label: "bar", kind: "function" },
    ],
    edges: overrides?.edges ?? [
      { from: "sym_aaa", to: "sym_bbb", relation: "calls" },
    ],
  };
}
```

### Hook Unit Tests (`hooks.test.ts`)

| Hook | Tests |
|------|-------|
| `useUnifiedGraph` | Calls API on mount with "overview". Sets isLoading during fetch. Sets error on failure. expandCluster merges without duplicates. reload clears and re-fetches. nodeMap provides O(1) access. |
| `useFocusContext` | BFS neighbor computation uses nodeMap. Upstream/downstream correct at depth 1, 2, 3. |
| `useTrace` | BFS parent-map path reconstruction. Parameter name filtering. Dead-end returns null. |
| `useDepthControl` | Unchanged |
| `useAdaptiveThresholds` | Unchanged |

### Integration Tests (`CodebaseExplorer.test.tsx`)

| Test | Verifies |
|------|----------|
| Initial loading state | Spinner shown, toolbar disabled |
| Overview render | After mock API resolves, cluster nodes appear |
| Error state | Mock API rejects, error banner with retry |
| Cluster expansion | Click cluster, mock API returns children, new nodes appear |
| Stale notification | Emit event, banner appears, click reload, re-fetches |
| FOCUS transition | Click node, context panel appears |
| Esc returns to OVERVIEW | Press Esc clears focus |

### Backend Tests (`explorer_graph.rs`)

| Test | Verifies |
|------|----------|
| ID determinism | Same ProjectIr produces identical hash IDs |
| Hierarchy construction | Cargo workspace parsed into crate→module→file tree |
| Signature preservation | FunctionSignature from SymbolNode appears in response |
| Depth filtering | overview returns no symbols, full returns everything |
| Cluster scoping | cluster param returns only subtree children and edges |
| Edge merging | Dataflow edges mapped to parameter_flow/return_flow |
