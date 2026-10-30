# Interleaving — Engineering Design & Codex Implementation Brief

**Status:** Proposed implementation contract · **Version:** 0.1 · **Primary platform:** macOS desktop, portable Go CLI

**Migration complete (2026-10-30):** Wails + Go is the sole implementation. The
user verified the packaged macOS app and authorized Go/Wails removal. Preserve
hexagonal ports/adapters and scenario semantics. The canonical contract is
`schemas/scenario.schema.json`; generate its TypeScript and Go-embedded consumers
with `npm --prefix apps/desktop run schema:generate`.

## 1. Product definition

**Interleaving** is a local-first, visual API concurrency and correctness testing tool. Its headless executable is **`interl-go`**. Users model independent authenticated HTTP clients, run them as isolated virtual users, coordinate their requests with barriers, and inspect event traces to uncover races such as double booking, lost updates, and broken idempotency.

This is **not** a general-purpose load testing product or a Postman clone. The differentiator is **stateful multi-client orchestration with correctness assertions**. Desktop and CLI must execute the same versioned scenario through the same Go application.

### Goals

1. Define portable, reviewable JSON scenarios independently of the GUI.
2. Configure client profiles, identities, virtual-user groups, workflows, and assertions.
3. Give every virtual user its own mutable HTTP session and event stream.
4. Coordinate concurrent operations with explicit, timeout-bounded barriers.
5. Explain results through correlated, redacted structured events and reports.
6. Run the same scenario locally and in CI using `interl-go`.

### Non-goals for v0.1

Distributed workers, high-scale throughput benchmarking, arbitrary scripting, user-written plugins, a database/server, non-HTTP protocols, full HAR compatibility, deterministic replay of external services, and arbitrary cyclic visual graphs.

## 2. Technology choices

| Layer                   | Technology                  | Responsibility                                               |
| ----------------------- | --------------------------- | ------------------------------------------------------------ |
| Desktop shell           | Wails 3                     | Window, filesystem dialogs, invoke commands, event transport |
| UI                      | React + TypeScript + Vite   | Editor, configuration, event/timeline inspection             |
| Styling                 | Tailwind CSS v4 + shadcn/ui | Accessible reusable UI                                       |
| Graph editor            | `@xyflow/react`             | Visual scenario editing only                                 |
| UI state                | Zustand                     | Editor/run state                                             |
| UI forms/schema         | React Hook Form + Zod       | Form validation and helpful diagnostics                      |
| Scenario models         | Go models + shared JSON Schema                | Authoritative versioned document contract                    |
| Execution               | Go contexts and goroutines                       | Virtual-user tasks, scheduling, cancellation, barriers       |
| HTTP                    | Go net/http + crypto/tls            | Transport, sessions, TLS, cookies                            |
| CLI                     | Go CLI adapter                        | `interl-go validate`, `interl-go run`             |
| Integration test server | Go net/http                        | Controlled auth, race, timeout, echo endpoints               |

Use current mutually compatible stable dependency versions; do not blindly pin versions from this document. Prefer simple, tested solutions to abstraction-heavy frameworks.

## 3. Repository structure

```text
apps/desktop/src/                    React editor, inspectors, run visualization
apps/desktop/src/lib/desktop/        DesktopHost port and Wails/browser adapters
go/main.go                          Wails composition root
go/cmd/interl-go/                    Headless CLI composition root
go/cmd/interl-test-server/           Local HTTP fixture
go/internal/domain/                 Scenario models, values and compilation
go/internal/application/            Use cases, execution, ports and safe events
go/internal/adapters/               Inbound/outbound implementations
go/internal/bootstrap/              Application composition
schemas/scenario.schema.json         Canonical portable file contract
tests/fixtures/editor/               Shared editor regression fixtures
scenarios/examples/                  Living showcase and JSON/YAML examples
```

**Dependency direction:** driving adapters call application ports; domain and
application packages depend on no concrete adapters. Composition wires transport,
credentials, FEEL, codecs, files, clocks, IDs and Charm logging. The headless CLI
never imports Wails. Persisted scenario data remains separate from React Flow.

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

### Execution containers (M4)

M4 adds graphical **execution containers** around selected nodes or workflow calls.
Two variants separate workload scheduling from repetition inside one user's journey:

| Container | Execution ownership | Configuration example |
| --- | --- | --- |
| **Load** | A workload group schedules independent VUs; each VU repeats the enclosed journey sequentially | Ramp 0 → 50 VUs over 2 minutes, hold 50 for 5 minutes, then ramp down |
| **Repeat** | The current VU repeats the enclosed steps using its existing identity and session | Repeat 10 times, for 2 minutes, or until a bounded condition becomes true |

A Load container is the graphical representation of an existing workload group
and its executor, not an independently scheduled node inside a reusable workflow.
Wrapping a valid selection creates or binds the group's iteration workflow with
explicit inputs/outputs. Container membership affects execution and must be
persisted independently of canvas coordinates; moving a node across a drawn
border alone must not silently change membership. Reusable definitions remain
independent of their callers' load schedules.

A Repeat container is a new structured loop within one VU. It does not create
additional VUs or replace the workload executor. Count mode runs the configured
number of repetitions; duration mode starts repetitions while time remains;
until mode evaluates a read-only bounded condition after each successful
repetition and stops when it becomes true. Until mode requires a maximum count
or duration, and reaching that limit without satisfying the condition reports a
distinct limit outcome. Conditions use explicitly scoped current-repetition
outputs; unavailable values and evaluation errors must not silently continue.
Pin loop input/output, iteration-local lifetime, pacing, and nesting limits before
implementation. Reject arbitrary graph back-edges and nested Load schedulers;
bound supported Repeat nesting and total execution/resource use.

Per-VU lifecycle remains **initialize once → repeat journey → cleanup once**.
Retain session state across repetitions by default. Authentication inside the
repeated body deliberately models repeated logins; initialization outside it
authenticates each newly started VU once. A Repeat container may be part of a Load
journey without changing these ownership rules.

Editor behavior:

- Select a connected region or workflow calls, then **Wrap in Load** or **Wrap in Repeat**. Validate a single entry/exit and preserve references through explicit boundary mappings; reject invalid selections with actionable diagnostics.
- Show a compact container summary such as `0 → 50 VUs · ramp 2m` or `Repeat · 10 times`. Configure load stages, counts, duration, and stop conditions in the inspector.
- Support editing, unwrapping, undo/redo, save/reopen, and Go validation without changing the selected nodes' meaning unexpectedly. Define schedule ownership when wrapping an existing workload selection; never silently multiply its VUs.
- Run displays scheduled versus achieved VUs, completed iterations, container/repetition identity, and stop outcomes. Use aggregate load progress and a selected-VU view rather than one DOM token per VU.

Duration expiry and ramp-down stop admission of new repetitions, allow active
work to drain within a configured grace period, then cancel remaining work and
perform bounded per-VU cleanup. User cancellation propagates to HTTP operations
and waits. Specify completion, failure, deadline, and cancellation outcomes in the
shared Go contract and keep CLI/Wails observations equivalent. Arrival-rate
executors, parallel execution, and cross-user coordination are not enabled by
these containers; arrival-rate scheduling remains later work and synchronization
remains M5.

M4 acceptance includes fixed/staged loads, count/duration/until repetition,
conditional limits and missing outputs, nested-loop/resource bounds, independent
VU sessions, graceful stopping, safe iteration-correlated reports, and graphical
wrap/unwrap round trips. Extend the living feature showcase and its regression as
each supported container capability lands; do not add non-executable placeholders
to today's examples.

### Reusable workflows and authentication ownership

Reuse authentication as a workflow/subprocess executed inside the calling VU, rather than as an independently scheduled child scenario. A parent scenario owns scheduling; a reusable workflow owns behavior and typed input/output contracts. An invocation shares only the caller's explicitly owned session/identity, receives explicit input mappings, and exposes explicit output mappings. Its temporary values remain scoped to that invocation. Authentication outputs stay Go-only and sensitive; exported outputs inherit sensitivity and existing retention/resource limits. Bound invocation depth/count and reject direct/indirect recursive calls.

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
ScenarioDocument (Go models)
          |
          v
Schema and semantic validation
          |
          v
Compiled Program
          |
          v
application
```

The loader, parser, schema types, and validation logic belong in `domain/scenariocodec`.

Neither `application`, `interl-http`, nor the Wails adapter may implement separate YAML/JSON parsing or scenario validation.

### Go API

Provide a shared API conceptually equivalent to:

The application exposes `ValidateFile`, `ValidateSource`, `InspectSource` and
`Start` through Go interfaces. Contexts carry cancellation; typed diagnostics carry
safe codes, field paths and optional node references. Filesystem and codec ports
perform bounded reads and parsing before domain compilation.

`load_scenario` detects the format using the file extension. Unsupported extensions must produce an explicit error rather than silently guessing.

`parse_scenario` accepts an explicit format so the desktop application can validate unsaved editor content.

### Schema requirements

- Use a single set of Go models for both formats.
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

FEEL is read-only: it may later provide extraction values, assertions, and conditions. It never schedules work or implicitly mutates state. Explicit operations own mutation; the Go engine owns scheduling. M1 preserves strings without FEEL syntax validation or evaluation.

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

1. Equivalent YAML and JSON scenarios deserialize to equal Go models.
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
- Add YAML parsing using a maintained Go YAML library.
- Add format-equivalence tests.
- Support both file formats in `interl validate`.
- Open and validate both formats in Wails using the same core loader and validator; preserve source-format metadata separately from executable semantics.

**Milestone 2 — HTTP execution**

- Ensure `interl run` accepts both formats through the shared loader.
- Reuse the M1 Wails loader and validator for execution inputs.
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
- Use typed Go node variants for node variants (`start`, `end`, `http`, later `extract`, `setSession`, `barrier`, `assert`, `fork`, `join`, `loop`). Keep v0.1 types deliberately small.
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
domain/scenariocodec: parse -> validate -> compile
         |
         v
Immutable Program + resolved run config
         |
         v
application: RunCoordinator
         |---- VirtualUser 1 -> isolated HttpSession -> HTTP transport
         |---- VirtualUser 2 -> isolated HttpSession -> HTTP transport
         |---- VirtualUser N -> isolated HttpSession -> HTTP transport
         |
         +---- bounded event channel -> Wails UI / CLI reporter
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

The Go engine owns execution order, concurrency, coordination, and result status. React Flow owns only node placement and editing. A transport abstraction should be small and enable later TLS identity swaps; do not build a plugin framework.

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
- mTLS identity is selected at TLS handshake. To rotate a certificate, create a **new HTTP transport Client** with the new identity, atomically replace that user's active transport for future requests, and allow in-flight requests to finish using the previous client. Record a non-secret certificate fingerprint. Never imply swapping a header rotates mTLS identity.

## 7. HTTP behavior

- Initially support GET, POST, PUT, PATCH, DELETE; path + query + headers + JSON/text body; explicit timeout.
- Use Go net/http with crypto/tls; keep the client/transport long-lived per user for connection reuse and cookies.
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

Use a **bounded Go channel** for event delivery and a monotonic sequence allocator. Define an explicit overflow/backpressure policy; correctness-critical failure events must not be silently discarded. Persist a bounded run report with summary and events. For a first implementation, avoid a database; JSON output is sufficient.

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

The Go CLI is built from `go/cmd/interl-go`; its binary name is **`interl-go`**. CLI and desktop use the same application ports for validation and execution.

Proposed exit codes: `0` success, `1` scenario executed but assertions failed, `2` invalid input/configuration, `3` execution/infrastructure error, `130` user interruption. Document these and test them. Keep human-readable stdout/stderr separate from `--output` JSON. For CI, report schema must be versioned.

## 10. Wails desktop API

Wails is a thin driving adapter around the same Go application used by the CLI.
Scenario open/read/save/validate, start/cancel, and report export use typed service
bindings. The official Wails JSONStream carries bounded, safe run events and the
terminal report. Native dialogs, close protection and appearance stay in the Wails
adapter. React imports only the DesktopHost port; browser preview has its own
non-native adapter. The application depends on ports for transport, credentials,
files, expressions, timers, IDs and logging. Composition injects Charm Log.

### UI layout

- Left: Clients / Identities / Scenarios / Runs explorer.
- Center: React Flow canvas with typed nodes and connection validation.
- Right: contextual node/client/identity inspector.
- Bottom: run controls, chronological event timeline, HTTP response details, assertion outcomes.
- Top: scenario name, save/load, validation status, Run/Cancel.

Editor changes are separate from the immutable snapshot captured at run start. Display unsaved changes and validation errors; don't mutate an active run when editing the canvas.

### Scenario authoring and execution modes (M3.5)

**Implemented Phase 6b:** author the main scenario on one canvas with Initialize,
Iteration, and Cleanup containers between scenario Start/End boundaries. Nodes
and reusable workflow calls belong explicitly to a phase; canvas geometry does
not determine membership. Initially project/compile containers through the
existing lifecycle bindings and phase-owned workflows. Cleanup remains a bounded
lifecycle-controller action after eligible failures/cancellation, not an ordinary
success-edge dependency; preserve the primary failure and report cleanup errors
separately. See the [Phase 6b design](plans/milestone-3.5.md#phase-6b--unified-lifecycle-canvas)
for ownership, import, editor, Run, dataflow, and acceptance rules. This authoring
correction belongs in M3.5; VU scheduling and repetition remain M4.

Add a Camunda-style modeling workflow using the existing React Flow setup: a palette of typed nodes, contextual add/connect/delete actions, arrowheads showing connection direction, and configuration through the collapsible inspector. This is an interaction reference, not BPMN file-format compatibility or adoption of Camunda engine semantics.

**Edit mode** creates/modifies scenarios, reusable workflows, lifecycle bindings, connections, client profiles, identity references, workload groups, and node inputs/outputs. The explorer distinguishes definitions from runtime users; workflow selectors and expandable subprocesses expose reusable authentication. M3.5 enables only one executing group/VU with one iteration; unsupported load executors are not presented as runnable. HTTP configuration includes method/path/headers/body/timeout and explicit response extraction/output references. Validation connects Go field/node diagnostics to the inspector and canvas. Provide dirty-state handling, undo/redo, and save/load without serializing React Flow runtime objects or resolved secrets.

**Run mode** displays the immutable captured scenario, workload/workflow bindings, and layout, with execution markers (visual “tokens”) driven by engine events. Show active/completed/failed/cancelled nodes and observed edge traversal; the canvas is read-only. Tokens represent workflow progress, never authentication values. Correlate progress with group/VU/iteration/workflow/invocation/node identity so repeated subprocesses do not collide. M3.5 shows one VU; M4 adds aggregate group counts and a selected-VU view rather than rendering every VU as a DOM token. Preserve invocation paths in traces and safe input/output inspection. Preserve safe trace inspection, Cancel/export, reduced-motion support, and separation from edits made after run start. Edit/Run workspace modes coexist with bottom Validation/Run detail tabs.

Branching is an explicit requested capability. The user-accepted allocation is exclusive conditional split/merge in M3.5, with parallel fork/join, barriers, and cross-user coordination in M5. Structured exclusive routing is implemented under ADR 0002; conditions stay read-only and bounded, with explicit gateway operations owning routing. Branch-local dataflow, condition limits, and safe route observations are validated in the shared compiler/runtime. Parallel scheduling remains unavailable.

M3.5 uses scenario schema v2 and report/live protocol v3 as specified in [ADR 0002](docs/adr/0002-editor-and-routing.md). The accepted pre-Phase-2 amendment removes V1 loading/migration entirely. Phases 1–6 provide typed scalar ports, bounded calls/cleanup, named extraction outputs, exclusive routing, and invocation-aware captured Run visualization. Run uses safe graph keys/labels and preserves its results independently of the editable document; see [progress](docs/milestone-3.5.md).

See the [M3.5 implementation plan](plans/milestone-3.5.md) for dependency order and acceptance criteria. Phases 1–6 are implemented; Phase 6b's unified lifecycle canvas is implemented; remaining M3.5 end-to-end acceptance is tracked separately.

## 11. Milestones and acceptance criteria

| Milestone                 | Deliverable                                                                      | Acceptance                                                             |
| ------------------------- | -------------------------------------------------------------------------------- | ---------------------------------------------------------------------- |
| **M1 — Foundation**       | Workspace, desktop scaffold, scenario models/validator, CLI `validate`, canvas   | CLI and Wails validate the same `hello-api.json`; unit tests pass      |
| **M2 — Single-user HTTP** | HTTP adapter, sequential engine, event channel, `interl run`, response inspector | GET/POST execute in desktop and CLI; errors/timeouts/cancel observable |
| **M3 — Stateful auth**    | Identity refs, login/extract/session updates, one user-owned session              | Single-user bearer/cookie auth; redacted CLI/desktop reports               |
| **M3.5 — Scenario editor** | Unified Initialize/Iteration/Cleanup canvas, reusable calls, typed inspectors/validation, directed canvas, exclusive branches, Edit/Run tokens; one VU/iteration | Create/edit/save all lifecycle phases on one canvas, run the immutable snapshot, and visualize actual progress; exclusive routing and failure cleanup match the approved Go contract |
| **M4 — Virtual users**    | Multiple workload groups, fixed/ramping VUs, graphical Load/Repeat execution containers, bounded count/duration/until repetition, identity assignment, global hooks, bounded concurrency | Independent groups ramp correctly; containers wrap/unwrap and save/reopen; per-VU auth/session isolation, conditional limits, graceful stop, bounded resources; two-user auth isolation proof |
| **M5 — Synchronization**  | Barriers, parallel fork/join, cross-user assertions; exclusive branching delivered in M3.5                            | Controlled double-booking test catches broken invariant                |
| **M6 — Network/TLS**      | mTLS identity lifecycle, explicit retry policies, fault handling                 | Rotation uses new transport; in-flight requests remain safe            |
| **M7 — Observability**    | Timeline filters, correlations, reports, repeat runs                             | Reproducible scenario inputs and redacted comparable traces            |
| **M8 — CI/distribution**  | CLI exit codes, packaged app, docs, security hardening                           | macOS app and headless CI run from same scenario                       |

M3 scope follows the [accepted authentication ADR](docs/adr/0001-m3-auth-foundation.md): production retains one group/count 1; new two-user isolation tests and the two-user acceptance proof move to M4. Existing session-isolation coverage remains. Implementation and automated evidence are recorded in [M3 progress](docs/milestone-3.md); native acceptance remains separately tracked.

### Implemented foundation

The Go domain, application, CLI, Wails adapter, HTTP sessions and shared frontend
implement M1–M3 and the delivered M3.5 editor/runtime scope. New work extends these
ports and adapters rather than scaffolding another runtime. Regression requirements
include strict JSON/YAML admission, graph/dataflow checks, auth redaction,
cancellation/cleanup, bounded capture, draft save/reopen and native dialogs.
Use `go test ./...`, `go vet ./...`, schema drift checks and the frontend suites.

## 12. Security and reliability guardrails

- Never persist credentials, cookies, tokens, or private keys in scenario JSON or unredacted reports.
- Use secret references and controlled resolution at runtime; do not put secrets in URL query strings by default.
- Set safe request timeouts, body limits, user count limits, barrier timeouts, and bounded channels.
- Respect TLS certificate validation by default; no automatic insecure fallback.
- Treat scenario files as untrusted input; validate before execution and avoid arbitrary shell execution.
- Support user cancellation and ensure tasks terminate; don't leave dangling background runs.
- Document that client-side barriers cannot guarantee exact simultaneous server execution.
- Don't call a run deterministic unless the server/environment is controlled; use **reproducible scenario and recorded trace**.

## 13. Implementation and verification

The Wails desktop and Go CLI are implemented in `go/`, with shared React source
in `apps/desktop/`. Core domain/application packages remain independent of Wails,
HTTP/filesystem implementations and the external FEEL library. Keep adapters
replaceable; runtime logging uses the logging port and Charm adapter.

Use the [README](README.md) for startup, packaging and verification. The migration
and user acceptance are recorded in [the migration plan](plans/go-wails-migration.md).
Continue M3.5/M4 product work from its milestone plan; do not reintroduce the retired
runtime or expand load scheduling as part of maintenance.

## 14. Decisions to revisit later

- M3 uses environment references only; revisit OS keychain support in a later milestone.
- M3 introduced report v2; M3.5 Phase 1 emits report/live protocol v3. Revisit artifact retention policy in a later milestone.
- Barrier participation across multiple groups, failure propagation, and partial-arrival semantics.
- Precise assertion DSL; prefer structured typed assertions over arbitrary expressions.
- Transport factory interface for mTLS certificate rotation.
- File persistence UX, scenario migration/versioning strategy, and release signing.

**Guiding principle:** Make it easy to define _who_ is acting, _what state each actor owns_, _when their requests overlap_, and _which outcome proves correctness_.
