# Interleaving — Engineering Design & Codex Implementation Brief

**Status:** Proposed implementation contract · **Version:** 0.1 · **Primary platform:** macOS desktop, portable Rust CLI

## 1. Product definition

**Interleaving** is a local-first, visual API concurrency and correctness testing tool. Its executable is **`interl`**. Users model independent authenticated HTTP clients, run them as isolated virtual users, coordinate their requests with barriers, and inspect event traces to uncover races such as double booking, lost updates, and broken idempotency.

This is **not** a general-purpose load testing product or a Postman clone. The differentiator is **stateful multi-client orchestration with correctness assertions**. Desktop and CLI must execute the same versioned scenario through the same Rust engine.

### Goals

1. Define portable, reviewable JSON scenarios independently of the GUI.
2. Configure client profiles, identities, virtual-user groups, workflows, and assertions.
3. Give every virtual user its own mutable HTTP session and event stream.
4. Coordinate concurrent operations with explicit, timeout-bounded barriers.
5. Explain results through correlated, redacted structured events and reports.
6. Run the same scenario locally and in CI using `interl`.

### Non-goals for v0.1

Distributed workers, high-scale throughput benchmarking, arbitrary scripting, user-written plugins, a database/server, non-HTTP protocols, full HAR compatibility, deterministic replay of external services, and arbitrary cyclic visual graphs.

## 2. Technology choices

| Layer                   | Technology                  | Responsibility                                               |
| ----------------------- | --------------------------- | ------------------------------------------------------------ |
| Desktop shell           | Tauri 2                     | Window, filesystem dialogs, invoke commands, event transport |
| UI                      | React + TypeScript + Vite   | Editor, configuration, event/timeline inspection             |
| Styling                 | Tailwind CSS v4 + shadcn/ui | Accessible reusable UI                                       |
| Graph editor            | `@xyflow/react`             | Visual scenario editing only                                 |
| UI state                | Zustand                     | Editor/run state                                             |
| UI forms/schema         | React Hook Form + Zod       | Form validation and helpful diagnostics                      |
| Scenario models         | Rust + serde                | Authoritative versioned document contract                    |
| Execution               | Tokio                       | Virtual-user tasks, scheduling, cancellation, barriers       |
| HTTP                    | reqwest + rustls            | Transport, sessions, TLS, cookies                            |
| CLI                     | clap                        | `interl validate`, `interl run`, `interl report`             |
| Integration test server | axum                        | Controlled auth, race, timeout, echo endpoints               |

Use current mutually compatible stable dependency versions; do not blindly pin versions from this document. Prefer simple, tested solutions to abstraction-heavy frameworks.

## 3. Repository structure

```text
interleaving/
├── Cargo.toml                      # Rust workspace
├── rust-toolchain.toml             # optional stable channel pin
├── apps/desktop/
│   ├── package.json
│   ├── package-lock.json
│   ├── vite.config.ts
│   ├── tsconfig.json
│   ├── tsconfig.node.json
│   ├── src/                       # React/Vite
│   │   ├── features/scenarios/     # canvas, inspector, validation
│   │   ├── components/ui/
│   │   └── lib/                   # Tauri bridge, wire types
│   └── src-tauri/                  # thin desktop adapter
├── crates/
│   ├── interl-core/                # models, validation, compilation
│   ├── interl-http/                # HTTP transport/session/auth
│   ├── interl-engine/              # runtime, virtual users, events
│   └── interl-cli/                 # binary named `interl`
├── scenarios/examples/
├── plans/
└── docs/                           # schema and verification documentation
```

**Dependency direction:** `interl-core` has no UI/Tauri dependency; `interl-http` depends on core; `interl-engine` depends on core and HTTP; Tauri and CLI depend on engine/core. Neither reusable library may import Tauri. Keep UI schema adapters separate from React Flow's native node/edge types.

## 4. Domain model and ownership

| Concept               | Definition                                                             | Lifetime / mutability                       |
| --------------------- | ---------------------------------------------------------------------- | ------------------------------------------- |
| Scenario              | Versioned workflow, group declarations, assertions, references, layout | Immutable run snapshot                      |
| Environment           | Non-secret variables and references to secret sources                  | Immutable per run                           |
| Client profile        | Base URL, headers, timeouts, TLS policy, default auth strategy         | Immutable per run                           |
| Identity              | Secret references, username/password/token/certificate references      | Immutable config; resolved only at run time |
| Virtual-user group    | Client profile, identity assignment policy, user count, start policy   | Immutable per run                           |
| Virtual user          | Independent session, variable scope, cookies, active transport         | Mutable and isolated to one user            |
| Shared scenario state | Explicitly named values for coordinated tests                          | Mutable only via defined operations         |
| Step state            | Request/response, temporary extraction values                          | Ephemeral                                   |
| Run                   | Configuration snapshot, outcome, event sequence, report                | Created once; append-only observations      |

**Isolation invariant:** Two virtual users never share a mutable cookie jar, session header map, active HTTP client transport, or session token by accident. They may deliberately use the same credentials while maintaining independent sessions.

**Identity assignment modes:** `shared` (same identity, separate sessions), `pool` (assign from declared identities), `unique` (one distinct identity per user; validate enough identities exist). Identity count and user count are independent.

**Concurrency invariant:** `N` virtual users means `N` logical independent schedulers; it does not imply `N` OS threads, TCP connections, or unique identities.

## 5. Scenario File Formats — YAML and JSON

### Decision

Interleaving supports both YAML and JSON as scenario file formats.

- **YAML** is the preferred format for scenarios maintained manually in Git and executed in CI.
- **JSON** is the canonical serialization format for the desktop editor, internal APIs, and scenario snapshots.
- Both formats represent the **same versioned scenario schema**.
- Neither format may introduce features or execution semantics unavailable in the other.
- The execution engine must be completely independent of the source format.

Recommended file extensions:

- `*.interl.yaml` — preferred human-authored scenario
- `*.interl.yml` — accepted alias
- `*.interl.json` — canonical JSON scenario

### Architecture

All scenario files must pass through a shared loading and validation pipeline:

```text
YAML / JSON scenario file
          |
          v
Format detection
          |
          v
Format-specific parsing
          |
          v
ScenarioDocument (Rust / Serde)
          |
          v
Schema and semantic validation
          |
          v
Compiled ExecutionPlan
          |
          v
interl-engine
```

The loader, parser, schema types, and validation logic belong in `interl-core`.

Neither `interl-engine`, `interl-http`, nor the Tauri adapter may implement separate YAML/JSON parsing or scenario validation.

### Rust API

Provide a shared API conceptually equivalent to:

```rust
pub enum ScenarioFormat {
    Json,
    Yaml,
}

pub fn load_scenario(
    path: &std::path::Path,
) -> Result<ScenarioDocument, ScenarioLoadError>;

pub fn parse_scenario(
    source: &str,
    format: ScenarioFormat,
) -> Result<ScenarioDocument, ScenarioLoadError>;

pub fn validate_scenario(
    scenario: &ScenarioDocument,
) -> Result<(), Vec<ValidationError>>;
```

`load_scenario` detects the format using the file extension. Unsupported extensions must produce an explicit error rather than silently guessing.

`parse_scenario` accepts an explicit format so the desktop application can validate unsaved editor content.

### Schema requirements

- Use a single set of Rust `serde` models for both formats.
- Every scenario must declare `schemaVersion`.
- Unknown schema versions must be rejected with a useful diagnostic.
- Use explicit field names and stable enum representations.
- Reject unknown fields where appropriate, particularly in executable node configurations.
- Reject duplicate keys in both YAML and JSON.
- Reject YAML documents containing multiple top-level documents.
- Reject YAML custom tags, aliases, and anchors.
- Restrict YAML to the JSON-compatible data model: objects with string keys, arrays, strings, numbers, booleans, and null.
- Do not permit YAML-specific implicit typing to alter scenario semantics. For example, ambiguous values should be quoted or rejected.
- Ensure that valid YAML and JSON versions of the same scenario deserialize into equivalent `ScenarioDocument` values.

### FEEL expression handling

FEEL expressions are stored as strings in both formats.

For example, YAML:

```yaml
- id: assert-created
  type: assert
  config:
    expression: "response.status = 201 and response.body.id != null"
```

And JSON:

```json
{
  "id": "assert-created",
  "type": "assert",
  "config": {
    "expression": "response.status = 201 and response.body.id != null"
  }
}
```

The YAML/JSON parser must not interpret FEEL syntax. FEEL validation happens separately after deserialization, during scenario validation or compilation.

FEEL evaluation must use the same semantics regardless of the original file format.

FEEL is read-only: it may later provide extraction values, assertions, and conditions. It never schedules work or implicitly mutates state. Explicit operations own mutation; the Rust engine owns scheduling. M1 preserves strings without FEEL syntax validation or evaluation.

### Serialization and round-tripping

- JSON serialization must be deterministic, with stable field ordering where practical.
- YAML input must be supported without requiring YAML output initially.
- The desktop editor may load YAML scenarios, but it must not silently overwrite them with JSON.
- If YAML editing/export is not yet supported, offer an explicit **Save as JSON** operation.
- YAML comments and original formatting are not guaranteed to survive deserialization and reserialization.
- A scenario's executable semantics must remain unchanged when converting between YAML and JSON.
- React Flow node positions belong in `layout`, separate from executable workflow definitions.

### CLI behavior

The CLI must accept both formats transparently:

```bash
interl validate scenarios/checkout.interl.yaml
interl validate scenarios/checkout.interl.json

interl run scenarios/checkout.interl.yaml
interl run scenarios/checkout.interl.json
```

Validation errors must report:

- File path
- Error category
- Human-readable explanation
- Relevant node or field path when available
- Source line and column when the parser provides them

Both formats must produce equivalent validation results for equivalent scenarios.

### Security and resource limits

Scenario files are untrusted input.

The loader must enforce configurable limits on file size, nesting depth, and collection sizes.

YAML parsing must not permit arbitrary object construction, custom tags, alias expansion, or other mechanisms that could cause unexpected execution or resource exhaustion.

FEEL expressions must be validated and evaluated under separate resource limits.

### Tests

Add tests covering:

1. Equivalent YAML and JSON scenarios deserialize to equal Rust models.
2. Unsupported file extensions are rejected.
3. Invalid YAML and JSON produce useful diagnostics.
4. Duplicate object keys are rejected in both formats.
5. YAML aliases, anchors, custom tags, and multiple documents are rejected.
6. Unsupported `schemaVersion` values are rejected.
7. FEEL expressions are preserved exactly as strings.
8. JSON serialization and deserialization preserve scenario semantics.
9. Converting YAML to JSON preserves scenario semantics.
10. Both formats produce equivalent compiled execution plans.

### Implementation order

**Milestone 1 — Foundation**

- Implement the versioned `ScenarioDocument` schema.
- Implement JSON parsing, validation, and serialization.
- Define the `ScenarioFormat` and shared loader API.
- Add YAML parsing using a maintained Rust YAML library.
- Add format-equivalence tests.
- Support both file formats in `interl validate`.
- Open and validate both formats in Tauri using the same core loader and validator; preserve source-format metadata separately from executable semantics.

**Milestone 2 — HTTP execution**

- Ensure `interl run` accepts both formats through the shared loader.
- Reuse the M1 Tauri loader and validator for execution inputs.
- Add integration tests running equivalent YAML and JSON scenarios.
- Retain the source-format preservation established in M1.

**Later milestone — Editing and export**

- Add explicit YAML export.
- Define the desktop editor's policy for preserving YAML comments and formatting.
- Add format conversion commands if useful, such as `interl convert`.

Persist a **versioned JSON document**, not React Flow's runtime state. Use `schemaVersion: 1`. The example below is the **target contract to implement**; do not assume the repository already supports it.

```json
{
  "schemaVersion": 1,
  "id": "checkout-race",
  "name": "Checkout race",
  "environment": { "variables": { "itemId": "item-123" } },
  "clientProfiles": [
    {
      "id": "api",
      "baseUrl": "http://127.0.0.1:8080",
      "timeoutMs": 10000,
      "headers": { "accept": "application/json" }
    }
  ],
  "identities": [],
  "groups": [
    {
      "id": "buyers",
      "clientProfileId": "api",
      "count": 2,
      "identityAssignment": { "mode": "shared", "identityId": null }
    }
  ],
  "workflow": {
    "nodes": [
      { "id": "start", "type": "start" },
      {
        "id": "ready",
        "type": "barrier",
        "config": { "name": "ready", "participants": 2, "timeoutMs": 5000 }
      },
      {
        "id": "buy",
        "type": "http",
        "config": {
          "method": "POST",
          "path": "/items/${itemId}/purchase",
          "body": { "kind": "json", "value": {} }
        }
      },
      { "id": "end", "type": "end" }
    ],
    "edges": [
      { "from": "start", "to": "ready" },
      { "from": "ready", "to": "buy" },
      { "from": "buy", "to": "end" }
    ]
  },
  "assertions": [],
  "layout": {
    "positions": {
      "start": { "x": 100, "y": 180 },
      "ready": { "x": 320, "y": 180 },
      "buy": { "x": 540, "y": 180 },
      "end": { "x": 760, "y": 180 }
    }
  }
}
```

### Contract rules

- `schemaVersion` is mandatory; reject unsupported versions with an actionable error.
- IDs are stable and unique within their respective collections. All references must resolve.
- `workflow.nodes`/`edges` describe execution; `layout.positions` is optional and never changes runtime behavior.
- Use typed serde enums for node variants (`start`, `end`, `http`, later `extract`, `setSession`, `barrier`, `assert`, `fork`, `join`, `loop`). Keep v0.1 types deliberately small.
- Reject invalid graph structure: missing start/end, unreachable nodes, invalid edges, unsupported branching, unbounded cycles, duplicate IDs, invalid participant counts, missing profiles, negative/zero timeouts.
- **M1–M2 support exactly one start, one or more sequential HTTP nodes, and exactly one end.** Reject branching, joins, cycles, and disconnected nodes. The barrier example above is a future M5 target; maintain a separate `hello-api.json` compatible with early milestones.
- **Accepted M1 schema scope:** support a small typed contract with empty `identities` and `assertions`, and anonymous `shared` identity assignment (`identityId: null`). Reject unsupported future configurations explicitly; do not accept uninterpreted executable data. Future configurations require an explicit version/capability compatibility policy and must never silently change existing scenario meaning.
- Detect `.json`, `.yaml`, and `.yml` extensions, supporting the recommended `.interl.*` suffixes and the documented `hello-api.json`. Reject other extensions without content sniffing.
- Template resolution is explicitly scoped and must never evaluate arbitrary JavaScript. Define a narrow `${variable}` interpolation grammar and deterministic precedence: step → virtual user → scenario environment. Secret values must not be serialized into run snapshots or reports.
- Versioned migrations should be explicit when schema changes; do not silently reinterpret existing scenarios.

## 6. Compiler and runtime architecture

```text
YAML / JSON file / React editor
         |
         v
interl-core: parse -> validate -> compile
         |
         v
Immutable ExecutionPlan + resolved run config
         |
         v
interl-engine: RunCoordinator
         |---- VirtualUser 1 -> isolated HttpSession -> reqwest
         |---- VirtualUser 2 -> isolated HttpSession -> reqwest
         |---- VirtualUser N -> isolated HttpSession -> reqwest
         |
         +---- bounded event channel -> Tauri UI / CLI reporter
         +---- cancellation token
         +---- barrier registry / shared state (later milestone)
```

**Lifecycle:** load → parse → validate → resolve secrets → compile → snapshot config (redacted) → instantiate users → execute → evaluate assertions → finalize report.

The Rust engine owns execution order, concurrency, coordination, and result status. React Flow owns only node placement and editing. A transport abstraction should be small and enable later TLS identity swaps; do not build a plugin framework.

### Scheduling semantics

- Within one virtual user, normal workflow steps execute sequentially.
- Across users, steps execute concurrently under configurable global/per-host limits.
- A barrier releases only when its configured participants arrive; failure, cancellation, and timeout must unblock all waiters and yield a defined outcome.
- A barrier aligns **client-side start points**, not server-side request processing; never promise deterministic remote scheduling.
- Fork/join and loops must have explicit structured semantics; reject arbitrary graph cycles. Bound loops by count/time.
- Cancel should propagate to active users, waits, and HTTP operations; finalize the run with a partial event trace.
- Default to no implicit retries for mutation requests. Any retry policy must be explicit and observable.

### Session and authentication semantics

- Each user owns its own cookie store, session header overrides, token variables, and active transport handle.
- Auth workflow: HTTP login → extract token from response → update user session header → subsequent authenticated requests.
- Store secrets as references (environment variables / OS keychain in later phases), not literal plaintext in scenario JSON. Redact authorization, cookies, passwords, tokens, private keys, and configured sensitive fields in logs/events/exports.
- mTLS identity is selected at TLS handshake. To rotate a certificate, create a **new reqwest Client** with the new identity, atomically replace that user's active transport for future requests, and allow in-flight requests to finish using the previous client. Record a non-secret certificate fingerprint. Never imply swapping a header rotates mTLS identity.

## 7. HTTP behavior

- Initially support GET, POST, PUT, PATCH, DELETE; path + query + headers + JSON/text body; explicit timeout.
- Use reqwest with rustls; keep `Client` long-lived per user for connection reuse and cookies.
- Treat 4xx/5xx as **HTTP responses**, not transport errors; assertions decide whether they are failures.
- Differentiate DNS/TLS/connect/timeout/cancel errors from received HTTP statuses.
- Cap response body bytes and event payload sizes. Preserve metadata and mark truncated content.
- TLS verification must be enabled by default; insecure mode requires explicit opt-in and prominent UI warning.
- Never print secrets in CLI debug logs, traces, or JSON reports.

## 8. Events, reports, and correctness

Every event carries at least: `runId`, `sequence`, `elapsedNs` (monotonic relative to run start), `timestamp` (optional wall-clock UTC), `userId` (if applicable), `nodeId` (if applicable), `type`, and a typed redacted payload.

Initial event taxonomy:

```text
run.started
user.spawned
step.started
request.sent
response.received
session.updated
barrier.waiting
barrier.released
assertion.passed
assertion.failed
step.failed
user.finished
run.cancelled
run.finished
```

Use a **bounded Tokio channel** for event delivery and a monotonic sequence allocator. Define an explicit overflow/backpressure policy; correctness-critical failure events must not be silently discarded. Persist a bounded run report with summary and events. For a first implementation, avoid a database; JSON output is sufficient.

### Run result states

`passed`, `failed`, `cancelled`, `error`. A transport failure, schema error, assertion failure, and cancellation should remain distinguishable in machine-readable output.

### Correctness assertions (M5)

Support assertions that can express: expected status/JSON property, exactly one successful winner among competing users, uniqueness of returned resource IDs, idempotent duplicate submission, and final-state equality after concurrent writes. For cross-user assertions, evaluate against explicitly collected run data; avoid shared mutable globals without ownership rules.

## 9. CLI contract

```bash
interl --help
interl validate scenarios/examples/hello-api.json
interl run scenarios/examples/hello-api.json --env staging --output results.json
interl report results.json
```

`interl-cli` is the Cargo package; its binary name is **`interl`**. CLI and desktop must call the same `interl-core` validator and `interl-engine` runner.

Proposed exit codes: `0` success, `1` scenario executed but assertions failed, `2` invalid input/configuration, `3` execution/infrastructure error, `130` user interruption. Document these and test them. Keep human-readable stdout/stderr separate from `--output` JSON. For CI, report schema must be versioned.

## 10. Tauri desktop API

Tauri remains a thin adapter. Proposed commands:

```text
validate_scenario(source: string, format: ScenarioFormat) -> ValidationResult
open_scenario(path: string) -> OpenScenarioResult
start_scenario(json: string, options: RunOptions) -> { runId: string }
cancel_scenario(runId: string) -> CancellationResult
```

Stream `RunEvent` through a Tauri Channel (preferred for high-frequency streaming) or a suitably bounded event mechanism. Do not block command handlers for the duration of a run. Ensure only active run IDs can be cancelled, and clean up finished run handles.

### UI layout

- Left: Clients / Identities / Scenarios / Runs explorer.
- Center: React Flow canvas with typed nodes and connection validation.
- Right: contextual node/client/identity inspector.
- Bottom: run controls, chronological event timeline, HTTP response details, assertion outcomes.
- Top: scenario name, save/load, validation status, Run/Cancel.

Editor changes are separate from the immutable snapshot captured at run start. Display unsaved changes and validation errors; don't mutate an active run when editing the canvas.

## 11. Milestones and acceptance criteria

| Milestone                 | Deliverable                                                                      | Acceptance                                                             |
| ------------------------- | -------------------------------------------------------------------------------- | ---------------------------------------------------------------------- |
| **M1 — Foundation**       | Workspace, desktop scaffold, scenario models/validator, CLI `validate`, canvas   | CLI and Tauri validate the same `hello-api.json`; unit tests pass      |
| **M2 — Single-user HTTP** | HTTP adapter, sequential engine, event channel, `interl run`, response inspector | GET/POST execute in desktop and CLI; errors/timeouts/cancel observable |
| **M3 — Stateful auth**    | Identity refs, login/extract/session updates, cookie isolation                   | Two users authenticate independently; no session leakage               |
| **M4 — Virtual users**    | Groups, N users, identity assignment, concurrency limits                         | N independent users execute; bounded resources and cancellation        |
| **M5 — Synchronization**  | Barriers, structured branching, cross-user assertions                            | Controlled double-booking test catches broken invariant                |
| **M6 — Network/TLS**      | mTLS identity lifecycle, explicit retry policies, fault handling                 | Rotation uses new transport; in-flight requests remain safe            |
| **M7 — Observability**    | Timeline filters, correlations, reports, repeat runs                             | Reproducible scenario inputs and redacted comparable traces            |
| **M8 — CI/distribution**  | CLI exit codes, packaged app, docs, security hardening                           | macOS app and headless CI run from same scenario                       |

### M1 detailed task order

1. Verify existing Node, stable Rust, Xcode Command Line Tools, and Tauri prerequisites without changing global toolchain defaults.
2. **Scaffold initialization completed:** Tauri 2 React/TS already exists in `apps/desktop/`. Preserve it; add Tailwind v4, shadcn/ui, React Flow, Zustand, Zod, and React Hook Form.
3. **Workspace initialization completed:** the root workspace and four Rust crates already exist. Preserve membership and keep Tauri's generated manifest valid; define profiles only at the root.
4. Implement serde scenario structs in `interl-core` and a stable error format: error code, JSON path, message, optional node ID.
5. Implement `parse_scenario`, `validate_scenario`, and `compile_scenario` (compiler may initially only accept linear graphs).
6. Create `hello-api.json` with one client, one user, start → GET `/health` → end.
7. Implement `interl validate <path>` via clap with exit code `0` or `2`.
8. Add Tauri `validate_scenario` using the same core function.
9. Render scenario nodes/edges on React Flow; keep layout positions separate; show inspector and validation diagnostics.
10. Add tests: valid scenario, unsupported version, duplicate IDs, missing client profile, dangling edge, unreachable node, invalid cycle.
11. Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, frontend build, Tauri dev.

**M1 definition of done:** a developer can open a scenario in the desktop UI, see/edit its graph layout, validate it, and run the exact same validation with `interl validate`. No HTTP execution is required yet.

### M2 detailed task order

1. Implement an `HttpSession` in `interl-http` from an immutable client profile, with a per-user `reqwest::Client`, session headers, request timeout, and response size limit.
2. Support GET/POST, request headers, path interpolation, JSON body, and structured response/error types.
3. Implement a sequential `ExecutionPlan` runner for start → HTTP → end and one virtual user.
4. Add cancellation token and timeouts. Avoid panics for expected network errors.
5. Implement `RunEvent` types, monotonic sequence IDs, elapsed timestamps, and bounded event streaming.
6. Implement `interl run <path> [--output results.json]` using the shared engine; produce versioned JSON report.
7. Add Tauri start/cancel commands and stream events to the frontend.
8. Build run panel: Run/Cancel, step status, timeline, request/response inspector, clear failures.
9. Build an axum test server with `/health`, `/echo`, `/not-found`, `/slow`.
10. Integration tests: status 200, 404 response vs transport failure, POST body, headers, timeout, cancellation, event ordering, JSON report, CLI exit behavior.
11. Verify desktop and CLI produce semantically equivalent results from the same scenario.

**M2 definition of done:** a single virtual user executes a real HTTP workflow from either interface, with a redacted event trace and a machine-readable result.

## 12. Security and reliability guardrails

- Never persist credentials, cookies, tokens, or private keys in scenario JSON or unredacted reports.
- Use secret references and controlled resolution at runtime; do not put secrets in URL query strings by default.
- Set safe request timeouts, body limits, user count limits, barrier timeouts, and bounded channels.
- Respect TLS certificate validation by default; no automatic insecure fallback.
- Treat scenario files as untrusted input; validate before execution and avoid arbitrary shell execution.
- Support user cancellation and ensure tasks terminate; don't leave dangling background runs.
- Document that client-side barriers cannot guarantee exact simultaneous server execution.
- Don't call a run deterministic unless the server/environment is controlled; use **reproducible scenario and recorded trace**.

## 13. Codex working instructions

**When implementing this design:**

1. Inspect the existing repository first; preserve working generated Tauri code and project configuration.
2. Implement **one milestone at a time**, starting with M1. Do not implement M3–M8 ahead of acceptance.
3. Before changes, briefly state the files you will touch and the acceptance criteria.
4. Prefer small cohesive Rust modules and typed errors; avoid `unwrap` in runtime paths.
5. Add tests with each behavior. Use a local axum server rather than real public APIs for integration tests.
6. Keep the schema and runtime independent of the frontend. Do not serialize React Flow node objects as scenario files.
7. Keep code compiling at the end of each step; run formatting, tests, linting, and frontend build when tools are available.
8. If an API or dependency differs from assumptions, inspect installed versions and adjust rather than forcing outdated snippets.
9. Explain any intentional deviation from this design in a short ADR or change note.
10. Never claim an unimplemented CLI command or UI feature works.

### Suggested first Codex prompt

> Read `DESIGN.md` in full. Inspect the current repository and implement **Milestone 1 only**. Start by reporting the existing scaffold and a short file-by-file plan. Establish the Rust workspace and four crates, define the versioned scenario schema and validator in `interl-core`, implement `interl validate`, add a minimal example scenario and validation tests, and connect the same validator to Tauri. If time permits, render the scenario in React Flow without coupling the persisted schema to React Flow types. Preserve existing generated configuration. Run all available checks and report precisely what passed, failed, or could not be run. Do not start HTTP execution or later milestones.

## 14. Decisions to revisit later

- Whether identity/secret storage uses OS keychain in M3 or starts with environment references only.
- Exact JSON report schema and artifact retention policy.
- Barrier participation across multiple groups, failure propagation, and partial-arrival semantics.
- Precise assertion DSL; prefer structured typed assertions over arbitrary expressions.
- Transport factory interface for mTLS certificate rotation.
- File persistence UX, scenario migration/versioning strategy, and release signing.

**Guiding principle:** Make it easy to define _who_ is acting, _what state each actor owns_, _when their requests overlap_, and _which outcome proves correctness_.
