# M2 — Single-user HTTP execution

This records the historical M2 contract, including report v1. Current behavior uses [scenario v2 and report/live protocol v3](milestone-3.5.md); legacy scenario formats are unsupported.

M1 was accepted by the user before M2 began. M2 preserves schema v1 and the monorepo structure. The CLI and Tauri use the same `PreparedRun` from `interl-engine`; the frontend never schedules requests or validates executable semantics.

## Execution contract

`PreparedRun::new` validates and compiles the core document, rejects unsupported M2 runtime configurations, preflights requests, and captures an immutable copy. Construction performs no network requests. `start` creates a UUID run ID, a bounded event receiver, cancellation token, and completion task. There is exactly one group with `count: 1`; start → HTTP… → end follows compiled graph connectivity, independently of node array order or canvas positions. At most 256 nodes are executable, with scenario/group/node IDs capped at 128 bytes.

Every run owns one `HttpSession` and its own reqwest client/cookie jar. Session headers merge with request headers case-insensitively; request headers override profile headers. The profile timeout applies unless overridden per request; both must be 1–300000 ms. Timeouts cover the request and response-body reads. HTTPS uses normal rustls certificate verification. Redirect following and retries are disabled; an HTTP redirect remains an observable response.

All schema methods (GET/POST/PUT/PATCH/DELETE), JSON bodies, and text bodies are supported. JSON bodies default to `application/json`; an explicit profile/request Content-Type is retained. Paths are root-relative and preserve the base URL's origin; `/health` replaces a base URL path prefix using standard URL resolution. Fragments and line breaks are rejected for execution. Query strings are part of `path`.

`${name}` accepts ASCII `[A-Za-z_][A-Za-z0-9_]*` and resolves environment strings, booleans, or numbers only. Missing variables, compound/null values, malformed placeholders, and dot-segment substitutions are configuration errors before requests begin. Substituted values are percent encoded as individual components. Headers and bodies are literal; there is no FEEL evaluation, secret provider, or implicit mutation. Later scopes may extend precedence to step → virtual user → environment.

HTTP 4xx/5xx are received responses, increment `httpErrorResponses`, and do not fail the transport workflow. Assertions are not implemented, so `passed` means the workflow completed transport operations. DNS, TLS, connection, timeout, body, and other transport errors stop subsequent nodes and produce `error`; cancellation produces `cancelled`. No automatic retry occurs. The first implementation reports one anonymous user only and performs no authentication workflow or concurrent scheduling.

## Bounds and delivery policy

Default `RunOptions` (shared by CLI/desktop):

| Option | Default | Hard limits |
|---|---:|---|
| `maxResponseBytes` | 65536 | 1–1048576 |
| `maxRequestBytes` | 65536 | 1–1048576 |
| `maxPreviewBytes` | 16384 | 1–65536 |
| `maxReportBytes` | 8388608 | 65536–16777216 |
| `eventCapacity` | 64 | 1–1024 |
| `sensitiveFields` | empty | at most 64 names, 128 bytes each |

Request bytes bound each serialized body and expanded path. Response bytes bound stored body data; reading stops at the limit and marks `truncated` when additional bytes are observed. This is an observable bounded response, not a transport failure. Only complete JSON bodies within the preview limit are displayed. Text, binary, invalid JSON, and truncated JSON bodies are omitted from trace previews.

The engine applies channel backpressure during execution. Cancellation interrupts both HTTP I/O and blocked event sends. If cancellation or a closed receiver prevents live delivery, the report retains the events and records `deliveryDroppedEvents`. A closed receiver cancels execution. The frontend replaces its live list with the completed report's event list, recovering terminal events that could not be delivered live. The report reserves space for failure/cancellation/completion events; crossing the ordinary event budget stops execution with `resourceLimit` and sets `eventsTruncated`.

Reports are held in memory until completion and written only on explicit CLI output or desktop export. Tauri allows one active run, accepts cancellation only for its active ID, cancels on window destruction, and retains only the most recently completed report. Starting another run releases that previous report. The frontend's run store is separate from editor state; layout edits and opening another document do not mutate captured input.

## Redaction and report v1

Redaction happens before snapshot/event construction, so live IPC and persisted reports use identical safe payloads. Authorization, cookies, API keys, token/password/secret/private-key/credential/session fields, and configured field names are masked. Known sensitive values and their percent-encoded forms are also scrubbed from echoed JSON strings. All environment values and URL query values are treated conservatively as sensitive. Response headers and JSON sensitive fields are learned before emitting their preview. Header previews are bounded to 64 entries and 8192 bytes. Redaction matching is bounded to 4096 patterns / 1 MiB; exhausting that budget omits previews rather than exposing unknown values. Arbitrary text bodies are never included in snapshots or previews.

The snapshot excludes layout, masks environment values, omits text bodies and query strings, and redacts structured payloads and headers. It is an audit snapshot, not an executable export or a source of credentials. The original scenario files are not rewritten by execution. M3 secret references and lifecycle management remain deferred.

Report envelope:

```text
reportVersion: 1
runId: UUID string
status: passed | failed (reserved) | cancelled | error
snapshot: redacted configuration without layout
summary: { requests, responses, httpErrorResponses, elapsedNs }
events: RunEvent[]
error: null | { kind, message }
eventsTruncated: boolean
deliveryDroppedEvents: integer
```

Each event has `runId`, a monotonic `sequence` starting at 1, `elapsedNs` from a monotonic clock, `userId`, `nodeId` (nullable), `type`, and a typed `payload`. Events: `run.started`, `user.spawned`, `step.started`, `request.sent`, `response.received`, `step.finished`, `step.failed`, `user.finished`, `run.cancelled`, and `run.finished`. Request/response events carry bounded redacted headers and body previews; responses include their HTTP status. Error messages are static and exclude raw reqwest errors/URLs.

CLI exit codes: 0 completed, 1 reserved assertion failure, 2 invalid input/configuration, 3 execution/infrastructure/output failure, 130 interrupted. `--output` writes compact versioned JSON separately from human stdout and diagnostic stderr. Existing scenario inputs cannot be used as the same output destination. Ctrl-C retains a partial report when output is supplied.

## Verification

See the root [README](../README.md) for commands and desktop smoke steps. `cargo run -p interl-http --example test_server` exposes `/health`, `/echo`, `/not-found`, and `/slow` on loopback port 8080. The automated fixture binds ephemeral loopback ports and also exercises cookies, redirects, large bodies, and secret echoes. Tests contact no public API.

Automated parity compares event semantics across equivalent YAML/JSON and Tauri preparation/headless engine paths. Run IDs, elapsed times, and server-generated headers naturally vary across runs. Native IPC/dialog rendering and visual behavior still require desktop smoke verification; compile/unit tests do not demonstrate those interactions.

## Acceptance checklist

- [x] Isolated HTTP session with headers, bounded bodies, timeouts, and structured errors.
- [x] Sequential execution of the same immutable schema-v1 workflow in the shared engine.
- [x] Cancellation during HTTP I/O, event backpressure, and receiver closure.
- [x] Ordered typed events and a bounded, redacted report with terminal outcomes.
- [x] `interl run --output`, documented exit codes, and Ctrl-C partial reports.
- [x] Tauri start/cancel/export commands and frontend Run/Cancel, timeline, node status, and request/response inspection.
- [x] Local-server integration coverage, YAML/JSON parity, frontend tests, formatting, Clippy, and macOS debug bundle.
- [ ] Native M2 smoke checks for Run/Cancel, dialogs, light/dark rendering, and report export; Computer Use access was not approved.
