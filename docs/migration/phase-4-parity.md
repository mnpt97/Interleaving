# Phase 4 — Engine and CLI parity

> Historical migration evidence. Migration and packaged macOS acceptance are complete;
> the old runtime was removed on 2026-10-30. Use the root README for current commands.

Completed 2026-10-29. Go now passes the documented M3.5 engine/CLI compatibility
matrix. Rust/Tauri remains available; native Wails scenario operations are Phase 5.

## Changes

- Fresh authentication discards inherited Authorization, Proxy-Authorization and
  Cookie defaults, session headers and cookies, without mutating the profile.
  Cookie authentication accepts usable cookies scoped to a different API path.
- HTTP matches reference Accept defaults, deterministic header precedence,
  repeated-header previews, UTF-8/HTML JSON bytes, text-body omission, and safe
  timeout/TLS/connection/incomplete-body classifications. No redirects or hidden
  retry replay were added.
- Snapshots apply omitted-default rules at known schema locations. Literal
  payload objects remain opaque. Explicit empty fields and default bearer settings
  now project like Rust, without redacting unrelated labels.
- Extraction/parser failures have stable safe codes and messages, including
  duplicate decoded keys, missing/null properties, oversized strings and truncated
  responses. Named extraction remains atomic. JSON scalar ports preserve numeric
  types: decimal JSON `1.0` does not become an integer output.
- FEEL property access and operators still execute in pbinitiative/feel. Scalar
  context adaptation preserves JSON numbers and uses its attribute interface to
  classify intermediate nulls; there is no second property-path evaluator.
  The existing nine predicate differences in `feel-library-outcomes.json` remain
  explicit. This phase does not claim identical predicate semantics for those cases.
- Aggregate scalar budgets count values, including suspended frames and lifecycle
  bindings, rather than storage-map keys. Retained responses share the run budget.
- Healthy consumers receive terminal events even with a one-entry channel.
  Delivery applies cancellation-aware backpressure instead of Phase 3's arbitrary
  500 ms consumer timeout. Consumers drain events or explicitly close the owner
  (`Close`) or receiver (`CloseReceiver`); cancellation/closure unblocks delivery.
  Reports retain terminal information when live delivery is interrupted.
- Offline HTTP admission precedes credential lookup. Invalid configuration cannot
  trigger credential reads or HTTP, and reports are not written after preparation
  failure. Runtime diagnostics continue through the Charm Log adapter.
- Both showcase formats now include the literal `<ready>&雪`; HTTP encoding and
  sensitive omission are covered by Go comparison and the Rust showcase regression.
  Only their fixture hashes changed; frozen validation expectations and reference
  contract fingerprints remain intact.

## Compatibility matrix

| Area | Evidence |
| --- | --- |
| Every runnable example | All 18 valid JSON/YAML example files in the frozen catalog execute through both CLIs |
| Auth/session | Native bearer/cookie, mapped login, static token, custom device fields, replacement identity, API-scoped/expired cookies, rejected credentials, profile suppression and header precedence |
| Values/calls | Typed showcase outputs, lifecycle mappings, repeated calls with local IDs, nested/opaque templates, environment interpolation, strict scalar types and aggregate storage |
| Routing | Both showcase arms, default selection, no match, ambiguity, missing source data and operand type errors |
| HTTP | Error responses, redirect policy, request/response bounds, long polling, timeout, TLS, incomplete bodies, encoding and repeated headers |
| Failure/cancel | Preparation rejection, auth/extraction failures, Ctrl-C with/without cleanup, failure precedence, cleanup deadline, user/owner stop during cleanup and detached receiver |
| Security/reports | Full safe snapshots/graphs, scoped ordered events, immutable public reports, secret absence in reports/streams/Charm logs, bounded retention and private atomic report writes |
| Headless boundary | Import/dependency checks and CGO-disabled CLI build; no Wails or Rust production dependency |

The runtime harness has **90 cases**: 18 example files, 63 targeted journeys and
the nine Phase 3 smoke cases. It compares complete normalized reports, wire
requests, CLI stdout and structured preparation diagnostic locations. Parser,
budget, concurrency, owner/receiver and file-alias tests supplement cases that
need injectable options or explicit handle lifetime control.

Normalization remains limited to run IDs, elapsed nanoseconds and the fixture
origin. Request JSON key order/header casing and transport-managed wire headers
are handled as documented in `tests/compatibility/README.md`; errors, branches,
graphs, snapshots, scopes, application headers and payloads are not dropped.

## Verification

From `go/`:

```sh
go test ./... -count=1 -timeout=120s
go vet ./...
go test -race ./internal/application ./internal/adapters/inbound/cli ./internal/adapters/outbound/feel ./internal/adapters/outbound/httpclient ./internal/adapters/outbound/reportfiles ./internal/adapters/outbound/scenariocodec -count=1 -timeout=120s
CGO_ENABLED=0 go build -o bin/interl-go-headless ./cmd/interl-go
go build -o bin/interl-go ./cmd/interl-go
```

From the repository root:

```sh
node scripts/check-go-run-parity.mjs
node scripts/check-go-validation-parity.mjs
node scripts/capture-migration-baseline.mjs --check
node scripts/check-migration-boundaries.mjs
cargo test --offline --locked -p interl-core -p interl-http -p interl-engine -p interl-cli
```

The retained Rust suites passed 236 checks, including the updated JSON/YAML
showcase. Offline parity retains 23 frozen validation cases, 119 extended cases
and 12 reference fingerprints. The shared 84-case expression corpus still records
the nine selected external-library predicate outcomes separately from Rust.

## Next

Phase 5 connects Wails scenario files, validation, real execution/events, report
export and native lifetime handling to this Go application. Interactive/native
acceptance remains with the user. Existing Tauri commands are unchanged.
