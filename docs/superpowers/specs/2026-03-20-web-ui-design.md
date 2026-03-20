# Web-Based UI Design Spec

**Date:** 2026-03-20
**Status:** Approved

## 1. Overview

Add a browser-based frontend to the audit agent that provides an interactive code intelligence viewer — a lighter version of the desktop (Tauri) app focused on visualizing file connections, function call graphs, and data flow. No Docker sandbox, no LLM features, no heavy computation.

### Goals

- **Zero-install**: Users access a URL in any browser — nothing to install
- **Interactive code intelligence**: VSCode-like layout with resizable panels, bidirectional navigation between graph nodes and source code
- **Dual deployment**: Self-hosted first (`cargo build` or Docker), SaaS-ready later
- **Shared frontend codebase**: Same React app serves both Tauri desktop and web, differing only in transport layer
- **No authentication**: Anyone can use it

### Non-Goals (Web Version)

- Docker-based sandbox execution (Kani, Z3, MadSim, Miri, Fuzz)
- LLM-powered features (AI overview, prose polish, assume hints)
- Review queue, checklist panel, toolbench panel
- Report generation (PDF/Markdown)

## 2. Features In Scope

### MVP (v1.0)

| # | Feature | Description |
|---|---------|-------------|
| 1 | File tree explorer | Browse source files with expand/collapse |
| 2 | Monaco code editor | Read source with syntax highlighting, click-to-navigate |
| 3 | File connection graph | Interactive graph showing file dependencies |
| 4 | Function call graph | Function-level call graphs with qualified names and signatures |
| 5 | Dataflow graph | Cross-file data flow tracing — click a variable, see where it flows |
| 6 | Security overview | Assets, trust boundaries, hotspots |
| 7 | Wizard flow | Source selection (git/local/archive) → config → workspace confirm |
| 8 | Session management | Create, list, reopen audit sessions |
| 9 | Split-view | Graph canvas + code editor side by side, bidirectional navigation |
| 10 | Pre-computed loading | Load sessions previously created by desktop or CLI |

### MVP Graph Interactions

| Interaction | Behavior |
|-------------|----------|
| Click function node in graph | Opens file in Monaco at that function's line |
| Click function name in Monaco | Highlights corresponding node in graph, pans to it |
| Hover node in graph | Tooltip with function signature, file path, line number |
| Click edge in graph | Shows relationship detail (calls, parameter flow, etc.) |
| Expand/collapse module node | Shows/hides child function nodes (compound nodes) |
| Minimap | Always visible in graph canvas corner, click to navigate |

### Post-MVP (v1.1)

| # | Feature | Description |
|---|---------|-------------|
| 11 | On-demand analysis | Run lightweight tree-sitter analysis on a new repo (no Docker/LLM) |
| 12 | Graph search | Filter nodes by name, highlight matching subgraph |
| 13 | Path tracing | Select two nodes, highlight shortest dependency path |

## 3. Architecture

```
Browser
┌────────────────────────────────────────────────────────┐
│  React + Vite + shadcn/ui + Tailwind                   │
│  allotment (resizable panels)                          │
│  React Flow + ELK.js (graph canvas)                    │
│  Monaco Editor (code viewer)                           │
│                                                        │
│  Transport: HttpTransport (fetch + WebSocket)          │
└──────────────────┬─────────────────────────────────────┘
                   │ HTTP REST + WebSocket
┌──────────────────┴─────────────────────────────────────┐
│  crates/apps/web-server (new Axum crate)               │
│  ├── Axum REST endpoints (mirrors IPC commands)        │
│  ├── WebSocket endpoint (session events stream)        │
│  ├── Static file serving (tower-http::ServeDir)        │
│  └── CORS middleware (tower-http::CorsLayer)           │
├────────────────────────────────────────────────────────┤
│  Reused crates:                                        │
│  ├── project-ir (enhanced: qualified names, sigs,      │
│  │               variable flow, function call graph)   │
│  ├── session-store (as-is)                             │
│  ├── intake (source resolution, workspace detection)   │
│  └── core (shared types)                               │
│                                                        │
│  New shared crate:                                     │
│  └── crates/services/session-manager                   │
│      (extracted from tauri-ui/ipc.rs)                  │
└────────────────────────────────────────────────────────┘

Desktop (Tauri) — unchanged behavior, same React app:
  TauriTransport → ipc.rs → session-manager → same crates
```

### Key Architectural Decisions

1. **One React app, two transports**: Build-time env var `VITE_TRANSPORT=tauri|http` selects transport implementation. Tauri uses `window.__TAURI__.core.invoke`; web uses `fetch()` + `WebSocket`.

2. **New `crates/apps/web-server` crate**: Thin Axum HTTP layer. Binary name: `audit-agent-web`. Does NOT duplicate business logic — delegates to `session-manager`.

3. **New `crates/services/session-manager` crate**: Extracted from `tauri-ui/ipc.rs`. Contains `SessionManager` with all wizard flow state, session CRUD, project IR building/caching, and security overview logic. Both `tauri-ui` and `web-server` depend on this crate.

4. **Enhanced `project-ir`**: Extract more data from tree-sitter AST (qualified function names, signatures, parameter/return flow, variable declarations) to support function-level call graphs and data flow tracing.

5. **Single deployable binary**: Axum serves both API and static frontend via `tower-http::ServeDir` (dev: filesystem, prod: either filesystem or `rust-embed`).

6. **Graph library strategy**: Web UI uses React Flow + ELK.js. Desktop keeps Cytoscape.js for now. The `GraphLens` component is the only divergence point — conditionally imported based on `VITE_TRANSPORT`. Desktop UI redesign (migrating to React Flow) is a separate future initiative.

## 4. Backend — `crates/apps/web-server`

### API Endpoints

| Method | Path | Maps to | Description |
|--------|------|---------|-------------|
| POST | `/api/source/resolve` | `resolve_source` | Resolve git/local/archive source |
| POST | `/api/config/parse` | `parse_config` | Validate audit YAML config |
| POST | `/api/workspace/detect` | `detect_workspace` | Analyze workspace crates |
| POST | `/api/workspace/confirm` | `confirm_workspace` | Finalize workspace scope |
| POST | `/api/sessions` | `create_audit_session` | Create new audit session |
| GET | `/api/sessions` | `list_audit_sessions` | List all sessions |
| GET | `/api/sessions/:id` | `open_audit_session` | Load existing session |
| GET | `/api/sessions/:id/tree` | `get_project_tree` | File tree for session |
| GET | `/api/sessions/:id/files/*path` | `read_source_file` | Read single source file |
| GET | `/api/sessions/:id/graphs/:lens` | `load_*_graph` | File/feature/dataflow graph |
| GET | `/api/sessions/:id/security` | `load_security_overview` | Security overview data |
| GET | `/api/sessions/:id/manifest` | `get_audit_manifest` | Audit manifest |
| WS | `/api/sessions/:id/events` | console + execution events | Real-time event stream |

### Error Response Format

All error responses use a consistent JSON envelope:

```json
{
  "error": {
    "code": "SESSION_NOT_FOUND",
    "message": "No session with id 'sess-abc123'",
    "status": 404
  }
}
```

HTTP status codes: 400 (bad request), 404 (not found), 422 (validation), 500 (internal).

### CORS Configuration

Development: Allow `http://localhost:5173` (Vite dev server).
Production: Configurable via `--cors-origin` CLI flag, defaults to same-origin.

### Shared Business Logic — `crates/services/session-manager`

Extract from `tauri-ui/ipc.rs` (~1100 lines of `UiSessionState`) into a new `SessionManager`:

```rust
/// Shared session management logic used by both Tauri IPC and Axum web server.
pub struct SessionManager {
    work_dir: PathBuf,
    session_store: Arc<SessionStore>,
    project_ir_cache: Arc<RwLock<HashMap<String, ProjectIr>>>,

    // Wizard flow state (per-connection in web, singleton in Tauri)
    wizard_state: RwLock<WizardState>,
}

struct WizardState {
    resolved_source: Option<ResolvedSourceView>,
    validated_config: Option<ValidatedConfig>,
    confirmation_summary: Option<ConfirmationSummary>,
    audit_config: Option<AuditConfig>,
}
```

**Concurrency model**: `SessionManager` methods take `&self` (not `&mut self`). Interior mutability via `RwLock` for wizard state and IR cache. This works for both:
- **Tauri**: Single `SessionManager` behind Tauri's managed state
- **Axum**: `Arc<SessionManager>` in `AppState`, safe for concurrent requests

**Web-specific concern**: Wizard flow state is per-connection. In the web server, each browser tab needs its own `WizardState`. This is handled by keying wizard state on a client-generated `wizard_id` (passed as a header or query param). `SessionManager` holds `RwLock<HashMap<String, WizardState>>` with TTL-based cleanup.

### State Management

```rust
pub struct AppState {
    pub manager: Arc<SessionManager>,
}
```

Axum handlers delegate to `manager` methods. Example:

```rust
async fn resolve_source(
    State(state): State<AppState>,
    Json(input): Json<SourceInputIpc>,
) -> Result<Json<ResolveSourceResponse>, AppError> {
    let result = state.manager.resolve_source(input).await?;
    Ok(Json(result))
}
```

## 5. Frontend — UI Stack

| Library | Purpose | Version |
|---------|---------|---------|
| React 18 | UI framework | ^18.3 |
| Vite | Build tool | ^5.4 |
| TypeScript | Type safety | ^5.7 |
| shadcn/ui | UI components (buttons, dialogs, tabs, etc.) | latest |
| Tailwind CSS | Utility-first styling | ^3.4 |
| allotment | VSCode-style resizable split panes | ^1.0 |
| React Flow | Interactive graph canvas (web build only) | ^11 |
| ELK.js | Hierarchical graph layout engine | ^0.9 |
| Monaco Editor | Code editor with syntax highlighting | ^0.52 |
| Lucide React | Icons (already used) | ^0.469 |
| React Router | URL-based routing (web build only) | ^6 |

### Frontend Routing (Web Only)

Web build uses React Router for bookmarkable URLs:

| Path | Component |
|------|-----------|
| `/` | Session list / wizard entry |
| `/wizard` | Wizard flow (source → config → confirm) |
| `/sessions/:id` | Workstation shell for a session |

Tauri build continues using `useState<AppMode>` (no URL routing needed for desktop).

### Layout Structure

```
┌─────────────────────────────────────────────────────────────┐
│  Title Bar: "Audit Agent — session-id"                      │
├────┬────────────┬───────────────────────────┬───────────────┤
│    │            │ Tab: file.rs              │               │
│ A  │  File Tree │─────────────────────────  │  Security     │
│ c  │  Explorer  │                           │  Overview     │
│ t  │            │  Monaco Editor            │               │
│ i  │  (resize)  │  (syntax highlight,       │───────────────│
│ v  │            │   click-to-navigate)      │               │
│ i  │            │                           │  Session      │
│ t  │            ├───────────────────────────│  Details      │
│ y  │            │                           │               │
│    │            │  React Flow Graph Canvas  │               │
│ B  │            │  (ELK.js layout,          │               │
│ a  │            │   compound nodes,         │               │
│ r  │            │   minimap, path trace)    │               │
│    │            │                           │               │
├────┴────────────┴───────────────────────────┴───────────────┤
│  Activity Console (collapsible)                             │
└─────────────────────────────────────────────────────────────┘
```

All panel boundaries are resizable via allotment. The editor/graph split is vertical (top/bottom) within the center column.

### Graph Component Strategy

The `GraphLens` component is the only frontend divergence point:
- **Web build**: `GraphLensReactFlow.tsx` — React Flow + ELK.js with compound nodes, minimap, bidirectional code navigation
- **Tauri build**: `GraphLensCytoscape.tsx` — existing Cytoscape.js implementation (unchanged)

Selected via conditional import based on `VITE_TRANSPORT`:
```typescript
const GraphLens = import.meta.env.VITE_TRANSPORT === 'http'
  ? lazy(() => import('./GraphLensReactFlow'))
  : lazy(() => import('./GraphLensCytoscape'));
```

## 6. Transport Abstraction

Refactor `ui/src/ipc/commands.ts` to use a `Transport` interface:

```typescript
// ui/src/ipc/transport.ts
export interface Transport {
  invoke<T>(command: string, args: Record<string, unknown>): Promise<T>;
  subscribe<T>(
    event: string,
    sessionId: string,
    handler: (payload: T) => void,
  ): () => void;
}

export class TauriTransport implements Transport {
  invoke<T>(command: string, args: Record<string, unknown>): Promise<T> {
    return window.__TAURI__!.core!.invoke!<T>(command, args);
  }

  subscribe<T>(
    event: string,
    _sessionId: string,
    handler: (payload: T) => void,
  ): () => void {
    let disposed = false;
    let unlisten: (() => void) | null = null;
    window.__TAURI__!.event!.listen!<T>(event, (e) => handler(e.payload))
      .then((stop) => { if (disposed) stop(); else unlisten = stop; });
    return () => { disposed = true; unlisten?.(); };
  }
}

// Command name → HTTP route mapping
const COMMAND_ROUTES: Record<string, { method: string; path: (args: Record<string, unknown>) => string }> = {
  resolve_source:        { method: 'POST', path: () => '/api/source/resolve' },
  parse_config:          { method: 'POST', path: () => '/api/config/parse' },
  detect_workspace:      { method: 'POST', path: () => '/api/workspace/detect' },
  confirm_workspace:     { method: 'POST', path: () => '/api/workspace/confirm' },
  create_audit_session:  { method: 'POST', path: () => '/api/sessions' },
  list_audit_sessions:   { method: 'GET',  path: () => '/api/sessions' },
  open_audit_session:    { method: 'GET',  path: (a) => `/api/sessions/${a.session_id}` },
  get_project_tree:      { method: 'GET',  path: (a) => `/api/sessions/${a.session_id}/tree` },
  read_source_file:      { method: 'GET',  path: (a) => `/api/sessions/${a.session_id}/files/${a.path}` },
  load_file_graph:       { method: 'GET',  path: (a) => `/api/sessions/${a.session_id}/graphs/file` },
  load_feature_graph:    { method: 'GET',  path: (a) => `/api/sessions/${a.session_id}/graphs/feature` },
  load_dataflow_graph:   { method: 'GET',  path: (a) => `/api/sessions/${a.session_id}/graphs/dataflow` },
  load_security_overview:{ method: 'GET',  path: (a) => `/api/sessions/${a.session_id}/security` },
  get_audit_manifest:    { method: 'GET',  path: (a) => `/api/sessions/${a.session_id}/manifest` },
};

export class HttpTransport implements Transport {
  private baseUrl: string;
  private wsBaseUrl: string;

  constructor(baseUrl: string) {
    this.baseUrl = baseUrl;
    this.wsBaseUrl = baseUrl.replace(/^http/, 'ws');
  }

  async invoke<T>(command: string, args: Record<string, unknown>): Promise<T> {
    const route = COMMAND_ROUTES[command];
    if (!route) throw new Error(`Unknown command: ${command}`);

    const url = `${this.baseUrl}${route.path(args)}`;
    const response = await fetch(url, {
      method: route.method,
      headers: { 'Content-Type': 'application/json' },
      body: route.method === 'POST' ? JSON.stringify(args) : undefined,
    });

    if (!response.ok) {
      const err = await response.json();
      throw new Error(err.error?.message ?? `HTTP ${response.status}`);
    }
    return response.json();
  }

  subscribe<T>(
    _event: string,
    sessionId: string,
    handler: (payload: T) => void,
  ): () => void {
    const ws = new WebSocket(
      `${this.wsBaseUrl}/api/sessions/${sessionId}/events`
    );
    ws.onmessage = (e) => handler(JSON.parse(e.data));

    // Auto-reconnect on close (non-clean)
    let closed = false;
    ws.onclose = (e) => {
      if (!closed && !e.wasClean) {
        setTimeout(() => {
          if (!closed) this.subscribe(_event, sessionId, handler);
        }, 2000);
      }
    };

    return () => { closed = true; ws.close(); };
  }
}

// Selected at build time
export const transport: Transport =
  import.meta.env.VITE_TRANSPORT === 'http'
    ? new HttpTransport(import.meta.env.VITE_API_URL ?? '')
    : new TauriTransport();
```

Each command function calls `transport.invoke()` instead of `tauriInvoke()`. Mock/fallback data is removed from production code (kept in test fixtures only).

## 7. Enhanced Project IR

### Current State

The Project IR in `crates/data/project-ir` currently provides:
- **File graph**: File-level nodes, minimal edges
- **Symbol graph**: Function/trait/macro nodes with name-based call edges (no `line` field)
- **Feature graph**: `#[cfg(feature)]` markers
- **Dataflow graph**: Function-level stubs with placeholder value previews

Tree-sitter extracts function names and call sites but NOT signatures, qualified names, parameters, or variable declarations.

### Enhancements Required

| Enhancement | Tree-sitter source | New IR data |
|-------------|-------------------|-------------|
| Line numbers | `function_item` start position | `SymbolNode.line: u32` (NEW) |
| Qualified names | `scoped_identifier`, `use_declaration` | `SymbolNode.qualified_name: Option<String>` (NEW) |
| Function signatures | `function_item` → `parameters`, `return_type` | `SymbolNode.signature: Option<FunctionSignature>` (NEW) |
| Variable declarations | `let_declaration`, `const_item`, `static_item` | New `VariableNode` in dataflow graph |
| Parameter flow | Function params → call argument matching | New `DataflowEdge` variant `parameter_flow` |
| Return flow | Return expressions → caller assignment | New `DataflowEdge` variant `return_flow` |

### New Types

```rust
pub struct FunctionSignature {
    pub parameters: Vec<ParameterInfo>,
    pub return_type: Option<String>,
}

pub struct ParameterInfo {
    pub name: String,
    pub type_annotation: Option<String>,
    pub position: usize,
}

// Extended SymbolNode (new fields marked)
pub struct SymbolNode {
    pub id: String,
    pub name: String,
    pub qualified_name: Option<String>,        // NEW
    pub file: PathBuf,
    pub kind: SymbolKind,
    pub line: u32,                              // NEW
    pub signature: Option<FunctionSignature>,   // NEW
}
```

### Backward Compatibility

- Existing graphs (file, feature) continue working unchanged
- Symbol graph gains new optional fields (non-breaking for serde: `#[serde(default)]`)
- Dataflow graph gains new edge types (additive)
- Desktop (Tauri) app benefits from enhanced IR automatically

## 8. Deployment

### Self-Hosted (Binary)

```bash
# Build frontend
cd ui && VITE_TRANSPORT=http npm run build

# Build server
cargo build -p audit-agent-web --release

# Run (serves API + frontend static files)
./audit-agent-web --port 3000 --work-dir ./audit-data --static-dir ./ui/dist
```

### Docker

```dockerfile
FROM node:22-bookworm AS ui-builder
WORKDIR /app/ui
COPY ui/ .
RUN npm ci && VITE_TRANSPORT=http npm run build

FROM rust:1.88-bookworm AS rust-builder
WORKDIR /app
COPY . .
RUN cargo build -p audit-agent-web --release

FROM debian:bookworm-slim
COPY --from=rust-builder /app/target/release/audit-agent-web /usr/local/bin/
COPY --from=ui-builder /app/ui/dist /usr/share/audit-agent/web
EXPOSE 3000
CMD ["audit-agent-web", "--port", "3000", "--static-dir", "/usr/share/audit-agent/web"]
```

### Docker Compose

```yaml
version: "3.8"
services:
  audit-agent:
    build: .
    ports:
      - "3000:3000"
    volumes:
      - ./audit-data:/data
    environment:
      - WORK_DIR=/data
```

## 9. Future Considerations

- **Desktop UI redesign**: Apply same UI stack (allotment, React Flow, ELK.js, shadcn/ui) to desktop Tauri app — separate initiative after web UI ships
- **SaaS mode**: Add reverse proxy, rate limiting, optional auth layer
- **Deeper analysis**: Integrate rust-analyzer LSP for type-aware call graphs (beyond tree-sitter)
- **Collaboration**: WebSocket-based shared sessions for team review
