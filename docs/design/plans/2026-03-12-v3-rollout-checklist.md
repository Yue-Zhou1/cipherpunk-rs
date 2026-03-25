# v3 Rollout Checklist

## Representative Repositories

- Rust crypto library with signature + serialization paths
- Circom/Cairo proving repository with witness/constraint hotspots
- Distributed consensus node with network partition simulation scenarios

## Verification Steps

1. Intake and session persistence
- Create a session from wizard mode.
- Restart the app and confirm session + records are still available.

2. Workstation review queue
- Confirm, reject, suppress, and annotate at least one candidate each.
- Confirm actions are reflected in session console events.

3. Evidence reproducibility
- For verified findings, confirm `container_digest`, `reproduction_command`, and expected output are present.
- Run at least one generated `reproduce.sh` in an isolated environment.

4. Report output
- Validate technical report includes: tool inventory, checklist coverage, verified findings, unverified candidate appendix, recommended fixes, and regression section.
- Validate executive and technical `.typ` template outputs are emitted.

5. Remote worker fallback and observability
- Run sandbox execution with `ExecutionBackend::LocalDocker` and `ExecutionBackend::RemoteWorker`.
- Confirm structured logs include backend, status, duration, and failure details.
- Confirm retries/timeouts produce explicit error entries.

6. Security hardening
- Validate redaction on AI prompt inputs (secrets, tokens, role labels).
- Validate network allowlist rejects commands referencing non-allowlisted hosts.

## Exit Gate

- All Rust tests pass (`cargo test`).
- All frontend tests and build pass (`cd ui && npm test && npm run build`).
- No verified finding is emitted without reproducibility metadata.
- Remote-worker path is either enabled and verified or explicitly deferred with risk acceptance.
