# Go/Wails migration — Phase 0 baseline and work journal

> Historical migration evidence. Migration and packaged macOS acceptance are complete;
> the old runtime was removed on 2026-10-30. Use the root README for current commands.

Date: 2026-10-28. Starting revision: `e151982c7d889ae9e566b4666162e7aada8564d1`.
Scope: establish the existing reference, inventories, toolchain decision and test
fixtures. Go/Wails implementation and native launches have not started.

## Work journal

1. Read the accepted migration plan and inspected the current clean working tree,
   workspace manifests, bridge, schemas, CLI, runtime and regression suites.
2. Inventoried direct desktop dependencies, supported expressions, limits and
   lifecycle/cancellation semantics. Identified adapters that must move behind ports.
3. Checked installed tools and verified the Wails release and module manifest
   against upstream sources. Selected an exact toolchain for the next phase without
   changing the installed Go version or global defaults.
4. Ran the baseline checks. Initial runs exposed missing/stale test setup rather
   than Go migration regressions; repaired only those reference-test issues below.
5. Added a language-neutral expression corpus and a Rust reference test, small
   invalid scenario inputs, and an offline CLI snapshot capture/check command.
6. Recorded comparison rules and benchmark methodology before any Go results exist.
7. Re-ran the complete suites after the repairs, captured and checked the offline
   reference snapshot, and completed the Phase 0 inventory gate.
8. Added narrow `.gitignore` exceptions for this evidence directory and the migration
   plan: the repository previously ignored all newly created `docs/` and `plans/`
   files. No Git staging or commit was performed.

## Installed tools and selected migration pins

| Item | Observed / selected |
| --- | --- |
| Host | macOS 27.0.1, arm64 |
| Go installed | `go1.27.2 darwin/arm64`; `GOTOOLCHAIN=auto` |
| Go selected for migration | **Go 1.27.3**, not yet downloaded/installed for this project |
| Wails selected | **`github.com/wailsapp/wails/v3 v3.0.0`**; `wails3` not installed |
| Rust / Cargo | 1.99.0 |
| Node / npm | v22.23.3 / 10.9.9 |
| Native tools | `/Library/Developer/CommandLineTools`; `xcrun --find clang` succeeds |

The [Wails v3.0.0 release](https://github.com/wailsapp/wails/releases/tag/v3.0.0)
is published, not an assumed prerelease. Its [tagged module manifest](https://raw.githubusercontent.com/wailsapp/wails/v3.0.0/v3/go.mod)
declares Go 1.26.0 and recommends toolchain 1.26.10. The release recommends Go
1.26.10 or 1.27.3+ for a `crypto/x509` fix; the installed 1.27.2 meets the minimum
but is not the selected build toolchain. The [official Go download metadata](https://go.dev/dl/?mode=json)
lists Go 1.27.3 as stable. Phase 1 should select that toolchain explicitly and pin
Wails v3.0.0 in the Go module and checksums, without changing system Go defaults.

No `go.mod`, `go.sum`, generated Wails bindings, global CLI install, GUI launch or
native compatibility claim is produced by Phase 0. Xcode tool presence is not a
successful Wails native build; `wails3 doctor` and smoke/native checks remain Phase 1.
Windows and Linux prerequisites are documented upstream but not verified here.

## Baseline repairs

These failures were observed before implementing Go, with the reference code at the
starting revision. No production Rust/React behavior was changed to make tests pass.

| Failure | Repair |
| --- | --- |
| CLI integration tests cannot compile the shared long-poll fixture: missing `serde::Deserialize` import dependency | Add existing workspace `serde` as a CLI dev dependency and update its Cargo lockfile dependency list; no new package version |
| Inspector navigation test searches the old Workflow nodes sidebar while the app defaults to lifecycle accordions | Explicitly select definition/workflow view in that test, preserving its existing same/other-node inspector assertions |
| Core output-liveness collision test renames `end` to `$return` but leaves the newly added example layout referencing `end` | Rename the layout position in the test alongside the node/edge so it still exercises a valid document |

The sandbox also blocks local HTTP listeners. Workspace integration tests require
loopback permission; that environment failure is not a product failure. Initial
failures and final results must remain distinguishable in the verification record.

## Public compatibility surface

| Surface | Reference and rule |
| --- | --- |
| Scenario schema | `schemas/scenario.schema.json`, version **2**; Rust is the generation authority |
| Generated frontend contract | `apps/desktop/src/lib/generated/scenario-file.ts` and the scenario adapter |
| Reports / live messages | `apps/desktop/src/lib/run-types.ts` and `crates/interl-engine/src/report.rs`; report **3**, live protocol **3** |
| CLI | `validate <scenario>`, `run <scenario> --output <report>`; 0 completion, 1 failed-outcome reservation, 2 invalid/preparation, 3 runtime/infrastructure, 130 interruption |
| HTTP status | HTTP 4xx/5xx alone does not imply transport/run failure; authentication and routing still apply their own semantics |
| Scope | One group, one VU and one iteration execute; calls do not create VUs; no M4 ramps, repetition, pools or M5 parallel gateways/barriers |
| Runtime evidence | Existing Rust HTTP/engine/CLI/Tauri tests; the living showcase runs both formats and both branch choices with 12 successful responses and bounded waits |
| Snapshot | `tests/compatibility/rust-baseline.json`: source hashes, CLI fixture outcomes and diagnostic signatures |

Malformed/invalid JSON examples and semantic drafts remain distinct from executable
scenarios. JSON-compatible YAML, duplicate-key rejection, strict fields, omitted
defaults, JSON literals and byte-based locations must survive the port. Go's
default JSON number representation must not introduce float coercion or lose safe
integer checks; strict decoding and typed scalar admission need explicit tests.

## Expression inventory and feasibility

The existing implementation does **not** evaluate arbitrary FEEL. Core uses pinned
`dsntk-feel` / `dsntk-feel-parser` 0.3.0 for admitted ASTs; its own bounded runtime
interpreter evaluates the compiled instructions.

- Extraction admits `response.body.<ASCII object-property>...` paths. Named outputs
  allow one Extract node to produce multiple typed values; this is not general FEEL
  map/context construction. No list indexing, function calls, filters or arithmetic.
- Routes admit scalar values and comparisons `=`, `!=`, `<`, `<=`, `>`, `>=`,
  boolean `and`/`or`, parentheses, `response.status`, admitted response-body paths
  and explicitly declared `inputs.<name>`. No coercion between strings/booleans/integers.
- Short-circuiting, missing/null/structured-value errors and cancellation matter.
  Missing data is not a default branch result. General `not(...)`, decimal/exponent
  literals, leading-zero numbers and integers beyond the safe range are rejected.
- Source limit: 4096 UTF-8 bytes; property paths: at most 32 names, each at most 128
  ASCII bytes. Predicate admission also bounds tokens (512), nodes (256), comparisons
  (64) and depth (30), plus retained compiled storage limits.
- `tests/compatibility/expressions.json` freezes **34 cases** executable by the Rust
  reference test. Broader Rust regressions remain required.

**Go adapter decision is open.** A candidate for Phase 2 evaluation is
[superisaac/FEEL.go](https://github.com/superisaac/FEEL.go), whose repository describes
a general FEEL interpreter. No candidate was installed, benchmarked, selected or
declared compatible in Phase 0. Review parser access, error offsets, scalar types,
resource bounds, maintenance/license and compiled representation before choosing.
An unbounded evaluator behind an interface is not an acceptable adapter.

The alternative is a small bounded compatible parser/evaluator with the same corpus
and acceptance/rejection rules. It must be an explicit recorded decision in Phase 2,
not a silent expression-language change. Neither route permits a Rust fallback in
the Go production engine.

## Resource and lifecycle invariants

| Area | Current reference values / behavior |
| --- | --- |
| Scenario parser defaults | 1 MiB input, depth 64 (hard cap 128), 10,000 entries/collection, 100,000 aggregate values |
| Run defaults | 65,536 request bytes, 65,536 response bytes, 16,384 preview bytes, 8,388,608 retained report bytes, 64 live-channel slots |
| Admission examples | Event capacity 1–1024; preview 1–65,536 bytes; report 131,072–16,777,216 bytes; additional sensitive-field count/name limits |
| HTTP | Timeout max 300,000 ms; session header budget 8192 bytes; explicit override/encoding/redirect/retry/cookie semantics remain tested |
| Credentials | 8192 bytes/value, 4096 declared references, 1,048,576 aggregate bytes; resolve only reachable required references |
| Extraction budgets | Complete response admission up to 1,048,576 bytes, depth 64, 100,000 JSON values; 128 outputs / 65,536 aggregate output bytes / 8192 bytes per value |
| Cleanup | 2000 ms default, 1–5000 ms configured; deadline covers the whole nested cleanup call tree |
| Cancellation | User cancel can attempt eligible cleanup; receiver/owner closure interrupts/skips cleanup as the current controller specifies |
| Result precedence | Primary initialize/iteration failure remains primary; cleanup has a separate outcome and can turn an otherwise successful run into an error |
| Public-data boundary | Safe snapshot, graph, request/response observations, errors and reports are sanitized before CLI/Wails delivery; no raw secrets in driving adapters |
| Backpressure | Execution respects bounded event/report budgets; consumer closure and terminal delivery behavior must be preserved |

Default transport response limits and extraction admission maxima are separate
limits, not interchangeable buffer settings. Keep UTF-8/serialized-byte accounting
and shared nested-frame budgets; do not replace them with Go string/rune counts or
per-frame independent limits.

## Desktop adapter inventory

| Current location | Required host port behavior |
| --- | --- |
| `src/lib/tauri.ts` | Availability, validate/read/open/save, run start/cancel/export, public runtime schemas, protocol mismatch handling, early-event cancellation |
| `src/lib/theme.tsx` | System/preferred theme and native window theme; browser fallback remains explicit |
| `src/App.tsx` | Native close subscription, prevent-close/dirty-state decision, disposal and destroy; keep frontend unsaved-change policy |
| `src-tauri/src/scenario.rs` | Shared parsing/validation, file provenance, draft save, accepted semantic-draft behavior and error mapping |
| `src-tauri/src/execution.rs` | Run registry, safe channel/report forwarding, cancellation and export; owner teardown |
| Frontend local layout | Preserve Tauri keys; Wails receives an isolated namespace without changing scenario persistence |

Replace these through `DesktopHost`; keep business rules and secret handling out of
both host adapters. The Rust engine currently has direct HTTP dependencies; the new
Go core follows the hexagonal dependency rules rather than copying those imports.
The dependency checks in Phase 1 must inspect Go imports and frontend host imports.

## Native acceptance and known gaps

M3.5 native packaging/dialog/theme/keyboard/end-to-end acceptance remains pending in
the existing progress documents. No new native GUI or browser permission is assumed.
Automated Rust Tauri-adapter tests are evidence of adapter parity, not native UI
acceptance. M4 multi-user isolation/load generation remains future scope.

Some README/design historical text still describes earlier linear/M2 capabilities.
Use current source/schema, the M3.5 plan and executable tests for the frozen matrix;
do not implement obsolete root-level formats or old pairing fields based on a stale
paragraph. Resolve actual source/spec disagreements with an explicit decision.

## Comparison and benchmark procedure fixed before Go measurements

See [comparison rules](../../tests/compatibility/README.md). Generated IDs require
bijective mapping; timing values and explicitly volatile headers are the only normal
runtime nondeterminism exceptions. Never normalize away scopes, failures, redaction,
chosen routes, meaningful ordering, body types, or counts.

For Phase 6, use release builds on the same host and a controlled server on an
independent, non-saturated process/machine. Fix fixture seed/reset, HTTP/TLS and
connection reuse settings, response sizes, capture/report options and environment
references. Run engines sequentially, randomize comparison order, use at least five
warm-ups and twenty measured repetitions, and report distributions plus host/toolchain
metadata. Measure CPU, wall time, peak RSS, output size, cancellation/cleanup latency
and failures; capture Go allocation/GC profiles and Rust profiles separately from
unprofiled timing runs.

Current production supports one VU/iteration: compare supported scenarios, large
documents, repeated runs and limit cases. A Go microbenchmark with many goroutines
cannot establish parity or M4 dispatch accuracy. Define scheduling-delay/load tests
when M4 exists. Desktop startup/rendering/event responsiveness needs a separate test
using identical React builds and event catalogues. Agree quantitative tolerances
before the Go comparison; no fabricated performance result is recorded here.

## Verification results

| Command | Result |
| --- | --- |
| `cargo test --workspace --offline --locked` (loopback allowed) | **257 passed**, 0 failed/ignored; includes the 2 corpus reference tests |
| `npm --prefix apps/desktop test -- --maxWorkers=2` | **244 passed**, 42 files |
| `npm --prefix apps/desktop run build` | TypeScript and production build pass; existing chunk-size and mixed Tauri-import advisories remain |
| `cargo clippy --workspace --all-targets --offline --locked -- -D warnings` | Pass |
| `cargo fmt --all -- --check` | Pass |
| `npm --prefix apps/desktop run schema:check` | Pass; schema artifacts unchanged |
| `cargo build --offline --locked -p interl-cli --bin interl` | Pass |
| `node scripts/capture-migration-baseline.mjs --write` then `--check` | **23 validation cases, 12 reference fingerprints**, capture and verification pass |

The full frontend rerun supersedes the initial 243/244 result. The full Rust rerun
supersedes the missing-dependency, sandbox listener and stale-layout failures.
Temporary command logs were written under `/tmp/interl-migration-*.log`; they are
local troubleshooting aids, not committed credentials or portable evidence.

**Phase 0 gate: complete.** The runtime baseline, public contracts, adapter seams,
native gaps, expression feasibility risks and comparison procedure are recorded.
The exact chosen versions are in `tests/compatibility/toolchains.json`; dependency
installation/checksums and native Wails verification remain Phase 1 work. No Go
runtime feature, Go module, Wails window or migration-performance result is claimed.

Next: Phase 1's hexagonal skeleton, frontend host port and Wails shell smoke test.
