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
| Scenario              | Versioned test definition: reusable workflows, workload groups, lifecycle bindings, assertions, references, layout | Immutable run snapshot |
| Environment           | Non-secret variables and references to secret sources                  | Immutable per run                           |
| Client profile        | Base URL, headers, timeouts, TLS policy, default auth strategy         | Immutable per run                           |
| Identity              | Secret references, username/password/token/certificate references      | Immutable config; resolved only at run time |
| Workflow | Reusable typed graph with explicit inputs/outputs; no independent load scheduler | Immutable definition / per-invocation state |
| Workload group | Client profile, identity assignment, lifecycle workflows, executor/load schedule | Immutable per run |
| Virtual user          | Independent session, variable scope, cookies, active transport         | Mutable and isolated to one user            |
| Shared scenario state | Explicitly named values for coordinated tests                          | Mutable only via defined operations         |
| Step state            | Request/response, temporary extraction values                          | Ephemeral                                   |
| Run                   | Configuration snapshot, outcome, event sequence, report                | Created once; append-only observations      |

**Isolation invariant:** Two virtual users never share a mutable cookie jar, session header map, active HTTP client transport, or session token by accident. They may deliberately use the same credentials while maintaining independent sessions.

**Identity assignment modes:** `shared` (same credentials, separate sessions), `pool` (assign from declared identities under an explicit reuse policy), `unique` (one distinct identity per active VU; validate enough identities for peak simultaneous demand). Identity count and VU count are independent. Assignment remains stable for the lifetime of a VU; pool leases and cross-group uniqueness scope must be defined in M4.

**Concurrency invariant:** `N` virtual users means `N` logical independent schedulers; it does not imply `N` OS threads, TCP connections, or unique identities.

### Accepted workload and actor model

A configured client and a running virtual user are separate concepts. A client profile describes connection/transport defaults; an identity describes the account or TLS principal through secret references. A workload group binds these definitions to behavior and scheduling. A VU is one instantiated actor, owning its mutable session and execution context. Do not add a second ambiguous “real user client” object that also owns cookies or a scheduler.

Fifty VUs using Alice's credentials mean fifty independently initialized client sessions acting as one account, not fifty distinct people. Sharing a certificate likewise shares a TLS principal, not an HTTP session. Server-side authentication policies can still invalidate concurrent sessions for the same account; client-side isolation does not change that behavior. Deliberate sharing of an issued authentication token/session is an advanced explicit policy, never the default.

A scenario can ultimately contain independent workload groups with different profiles, identity pools, workflows, and load schedules. A group may reuse a workflow used by another group without sharing runtime state. The accepted composition is:

```text
Scenario: checkout test
├── Reusable workflows: customer-login, browse-and-buy, admin-login, inspect-orders
├── Buyers workload
│   ├── Client profile: customer-api
│   ├── Identity assignment: customer account pool
│   ├── Initialize: customer-login; iteration: browse-and-buy
│   └── Executor: ramp 0 → 50 → 0 VUs
└── Administrators workload
    ├── Client profile: admin-api
    ├── Identity assignment: shared admin-service credentials
    ├── Initialize: admin-login; iteration: inspect-orders
    └── Executor: ramp 0 → 2 → 0 VUs
```

This illustrates the accepted target architecture, not current executable scheduling syntax. M3.5 Phase 1 supports only schema v2, reusable workflows/lifecycle bindings, and one once workload/VU/iteration. M4 adds multiple executing groups, fixed/ramping VUs, and identity-pool scheduling; M6 supplies mTLS transport. V1 is intentionally unsupported during pre-release development: reject it clearly without automatic migration. Inline login remains an explicit iteration sequence unless the author chooses an initialize workflow.

A load schedule belongs to a workload group, not to an identity or reusable workflow. M4 initially supports fixed VUs and staged ramping VUs, with explicit duration/iteration/stop policies, graceful draining, and hard resource limits. A VU completes workflow iterations sequentially; the scheduler controls repetitions without requiring a cycle in the graph. Ramping VUs controls concurrency, not requests per second. Arrival-rate executors are a separate future capability with bounded VU allocation. Report allocated/initializing/ready/draining VUs separately so authentication time and failures do not falsely appear as achieved business load.

### Reusable workflows and authentication ownership

Reuse authentication as a workflow/subprocess executed inside the calling VU, rather than as an independently scheduled child scenario. A parent scenario owns scheduling; a reusable workflow owns behavior and typed input/output contracts. An invocation shares only the caller's explicitly owned session/identity, receives explicit input mappings, and exposes explicit output mappings. Its temporary values remain scoped to that invocation. Authentication outputs stay Rust-only and sensitive; exported outputs inherit sensitivity and existing retention/resource limits. Bound invocation depth/count and reject direct/indirect recursive calls.

Per-VU initialization is the default place for login. A parent must not authenticate once and distribute mutable HTTP clients/cookie jars to its VUs. Supplying the same issued token to many VUs is a deliberately shared server-session test, even if the local jars differ; it requires an explicit future policy. Reusable auth workflows do not resolve secrets in React or turn scenario files into credential storage.

Independent buyer/admin workloads differ from a coordinated buyer/seller transaction. A coordinated journey needs distinct actor sessions, explicit data exchange, and synchronization. That orchestration belongs to M5; changing one VU's identity midway through a workflow is not a substitute for multiple actors.

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
Compiled Program
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

Persist a **versioned JSON document**, not React Flow's runtime state. Only `schemaVersion: 2` is supported. This minimal executable example uses one once workload and a workflow-local layout:

```json
{
  "schemaVersion": 2,
  "id": "hello-api",
  "name": "Hello API",
  "environment": {
    "variables": {}
  },
  "clientProfiles": [
    {
      "id": "api",
      "baseUrl": "http://127.0.0.1:8080",
      "timeoutMs": 10000,
      "headers": {
        "accept": "application/json"
      }
    }
  ],
  "identities": [],
  "groups": [
    {
      "id": "visitors",
      "clientProfileId": "api",
      "identityAssignment": {
        "mode": "shared",
        "identityId": null
      },
      "executor": {
        "kind": "once"
      },
      "lifecycle": {
        "iteration": {
          "workflowId": "main",
          "inputs": {}
        }
      }
    }
  ],
  "assertions": [],
  "workflows": [
    {
      "nodes": [
        {
          "id": "start",
          "type": "start"
        },
        {
          "id": "health",
          "type": "http",
          "config": {
            "method": "GET",
            "path": "/health"
          }
        },
        {
          "id": "end",
          "type": "end"
        }
      ],
      "edges": [
        {
          "from": "start",
          "to": "health",
          "id": "edge-0"
        },
        {
          "from": "health",
          "to": "end",
          "id": "edge-1"
        }
      ],
      "id": "main",
      "name": "Hello API",
      "inputs": {},
      "outputs": {},
      "layout": {
        "positions": {
          "start": {
            "x": 80,
            "y": 160
          },
          "health": {
            "x": 340,
            "y": 160
          },
          "end": {
            "x": 600,
            "y": 160
          }
        }
      }
    }
  ]
}
```

### Contract rules

- `schemaVersion` is mandatory; reject unsupported versions with an actionable error.
- IDs are stable and unique within their respective collections. All references must resolve.
- `workflows[].nodes`/`edges` describe execution; workflow-local `layout.positions` is optional and never changes runtime behavior.
- Use typed serde enums for node variants (`start`, `end`, `http`, later `extract`, `setSession`, `barrier`, `assert`, `fork`, `join`, `loop`). Keep v0.1 types deliberately small.
- Reject invalid graph structure: missing start/end, unreachable nodes, invalid edges, unsupported branching, unbounded cycles, duplicate IDs, invalid participant counts, missing profiles, negative/zero timeouts.
- **M1–M2 support exactly one start, one or more sequential HTTP nodes, and exactly one end.** Reject branching, joins, cycles, and disconnected nodes. Barriers remain a future M5 capability.
- **Historical M1 schema scope (superseded by the current V2 contract):** support a small typed contract with empty `identities` and `assertions`, and anonymous `shared` identity assignment (`identityId: null`). Reject unsupported future configurations explicitly; do not accept uninterpreted executable data. Future configurations require an explicit version/capability compatibility policy and must never silently change existing scenario meaning.
- Detect `.json`, `.yaml`, and `.yml` extensions, supporting the recommended `.interl.*` suffixes and the documented `hello-api.json`. Reject other extensions without content sniffing.
- Template resolution is explicitly scoped and must never evaluate arbitrary JavaScript. Define a narrow `${variable}` interpolation grammar and deterministic precedence: step → virtual user → scenario environment. Secret values must not be serialized into run snapshots or reports.
- Only schema v2 is accepted; reject v1 with an unsupported-version diagnostic. Future released-format migrations must be explicit.

## 6. Compiler and runtime architecture

```text
YAML / JSON file / React editor
         |
         v
interl-core: parse -> validate -> compile
         |
         v
Immutable Program + resolved run config
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

**Preparation:** load → parse → validate/compile → preflight → resolve only consumed secret references at accepted start → capture immutable run configuration (redacted for observation) → execute. Compilation/preflight do not read secret values or perform HTTP.

Secret references are resolved into immutable run inputs at accepted start; per-VU authentication is a separate network operation using the assigned captured credentials. Reusable login must not trigger frontend lookup or repeated environment reads.

**Target execution lifecycle:**

```text
Scenario setup — once per run, shared fixture preparation
    ↓
Workload scheduler creates a VU
    ↓
Assign identity and construct its isolated transport/session
    ↓
VU initialization — authenticate this VU
    ↓
Iteration workflow — once in M3.5; scheduler-controlled repetition in M4
    ↓
VU cleanup — optional logout/resource cleanup
    ↓
Scenario teardown — once per run, after all VUs stop
```

Scenario setup/teardown is distinct from per-VU initialization/cleanup. Global hooks exchange explicitly scoped fixture data/references, not mutable clients or implicit authenticated sessions. M3.5 implements single-VU initialization/iteration/cleanup and reusable calls; global setup/teardown execution and multi-group scheduling are M4 work. Cleanup must have bounded time and defined behavior after initialization failure, iteration failure, cancellation, and graceful stop; cleanup errors cannot erase the primary failure. Detailed hook/stop contracts are pinned before their implementation.

Retain a VU's authenticated session across iterations by default. An explicit per-iteration reset/initialization policy can model login traffic; M4 must define it separately from ordinary repetition. No hidden refresh, retry, or reauthentication is implied. A newly instantiated VU receives new runtime state; changing load never silently changes an existing VU's identity.

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

- Each VU owns its own cookie store, session header overrides, token variables, and active transport handle. Its assigned identity remains stable; no mutable session state is shared between different VUs or workload groups. A subprocess intentionally uses its own caller VU's session.
- Auth workflow: HTTP login → extract token from response → update user session header → subsequent authenticated requests. Reusable login runs in the caller's per-VU initialization context; cookie-only auth uses the same owned jar. Authentication happens for each newly started VU, including those created during a ramp.
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

M3.5 observations must additionally distinguish workload group, iteration, workflow, invocation, and lifecycle stage wherever applicable. The same local node ID in different workflows or repeated calls must not collide in traces, diagnostics, selection, or token projection. Define exact field/version changes in the M3.5 contract before implementation; this does not retroactively reinterpret the historical report v2 wire format. Phase 1 emits version 3 with explicit invocation context. Sensitive mapped outputs remain omitted/redacted.

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

### Scenario authoring and execution modes (M3.5)

Add a Camunda-style modeling workflow using the existing React Flow setup: a palette of typed nodes, contextual add/connect/delete actions, arrowheads showing connection direction, and configuration through the collapsible inspector. This is an interaction reference, not BPMN file-format compatibility or adoption of Camunda engine semantics.

**Edit mode** creates/modifies scenarios, reusable workflows, lifecycle bindings, connections, client profiles, identity references, workload groups, and node inputs/outputs. The explorer distinguishes definitions from runtime users; workflow selectors and expandable subprocesses expose reusable authentication. M3.5 enables only one executing group/VU with one iteration; unsupported load executors are not presented as runnable. HTTP configuration includes method/path/headers/body/timeout and explicit response extraction/output references. Validation connects Rust field/node diagnostics to the inspector and canvas. Provide dirty-state handling, undo/redo, and save/load without serializing React Flow runtime objects or resolved secrets.

**Run mode** displays the immutable captured scenario, workload/workflow bindings, and layout, with execution markers (visual “tokens”) driven by engine events. Show active/completed/failed/cancelled nodes and observed edge traversal; the canvas is read-only. Tokens represent workflow progress, never authentication values. Correlate progress with group/VU/iteration/workflow/invocation/node identity so repeated subprocesses do not collide. M3.5 shows one VU; M4 adds aggregate group counts and a selected-VU view rather than rendering every VU as a DOM token. Preserve invocation paths in traces and safe input/output inspection. Preserve safe trace inspection, Cancel/export, reduced-motion support, and separation from edits made after run start. Edit/Run workspace modes coexist with bottom Validation/Run detail tabs.

Branching is an explicit requested capability. The user-accepted allocation is exclusive conditional split/merge in M3.5, with parallel fork/join, barriers, and cross-user coordination in M5; detailed gateway/predicate contracts must be reviewed before implementation. Current Rust remains linear-only until an approved shared schema/compiler/runtime extension exists. Conditions stay read-only and bounded; explicit gateway operations own routing. Specify gateway semantics, branch-local dataflow, condition limits, and safe route observations before implementing controls. Do not treat drawing two arrows as implemented branching or enable unsupported configurations for Run.

M3.5 uses scenario schema v2 and report/live protocol v3 as specified in [ADR 0002](docs/adr/0002-editor-and-routing.md). The accepted pre-Phase-2 amendment removes V1 loading/migration entirely. Phase 1 provides scalar workflow ports, bounded calls/cleanup, and generated graph keys. The current desktop displays the iteration workflow from the complete V2 document. Exclusive predicates/routing remain gated until Phase 5 and full editor behavior remains later-phase work; see [progress](docs/milestone-3.5.md).

See the [M3.5 implementation plan](plans/milestone-3.5.md) for dependency order, acceptance criteria, and remaining decisions. M3.5 Phase 1 shared foundation is implemented; the visual editor and token projection are not implemented yet.

## 11. Milestones and acceptance criteria

| Milestone                 | Deliverable                                                                      | Acceptance                                                             |
| ------------------------- | -------------------------------------------------------------------------------- | ---------------------------------------------------------------------- |
| **M1 — Foundation**       | Workspace, desktop scaffold, scenario models/validator, CLI `validate`, canvas   | CLI and Tauri validate the same `hello-api.json`; unit tests pass      |
| **M2 — Single-user HTTP** | HTTP adapter, sequential engine, event channel, `interl run`, response inspector | GET/POST execute in desktop and CLI; errors/timeouts/cancel observable |
| **M3 — Stateful auth**    | Identity refs, login/extract/session updates, one user-owned session              | Single-user bearer/cookie auth; redacted CLI/desktop reports               |
| **M3.5 — Scenario editor** | Reusable workflow/lifecycle authoring, typed inspectors/validation, directed canvas, exclusive branches, Edit/Run tokens; one VU/iteration | Create/edit/save a scenario, run its immutable snapshot, and visualize actual progress; exclusive branch behavior matches the approved Rust contract |
| **M4 — Virtual users**    | Multiple workload groups, fixed/ramping VUs, iteration scheduling, identity assignment, global hooks, bounded concurrency                         | Independent groups ramp correctly; per-VU auth/session isolation, graceful stop, bounded resources; two-user auth isolation proof        |
| **M5 — Synchronization**  | Barriers, parallel fork/join, cross-user assertions; exclusive branching delivered in M3.5                            | Controlled double-booking test catches broken invariant                |
| **M6 — Network/TLS**      | mTLS identity lifecycle, explicit retry policies, fault handling                 | Rotation uses new transport; in-flight requests remain safe            |
| **M7 — Observability**    | Timeline filters, correlations, reports, repeat runs                             | Reproducible scenario inputs and redacted comparable traces            |
| **M8 — CI/distribution**  | CLI exit codes, packaged app, docs, security hardening                           | macOS app and headless CI run from same scenario                       |

M3 scope follows the [accepted authentication ADR](docs/adr/0001-m3-auth-foundation.md): production retains one group/count 1; new two-user isolation tests and the two-user acceptance proof move to M4. Existing session-isolation coverage remains. Implementation and automated evidence are recorded in [M3 progress](docs/milestone-3.md); native acceptance remains separately tracked.

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

- M3 uses environment references only; revisit OS keychain support in a later milestone.
- M3 introduced report v2; M3.5 Phase 1 emits report/live protocol v3. Revisit artifact retention policy in a later milestone.
- Barrier participation across multiple groups, failure propagation, and partial-arrival semantics.
- Precise assertion DSL; prefer structured typed assertions over arbitrary expressions.
- Transport factory interface for mTLS certificate rotation.
- File persistence UX, scenario migration/versioning strategy, and release signing.

**Guiding principle:** Make it easy to define _who_ is acting, _what state each actor owns_, _when their requests overlap_, and _which outcome proves correctness_.
