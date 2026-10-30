# Phase 3 — Go execution slice

> Historical migration evidence. Migration and packaged macOS acceptance are complete;
> the old runtime was removed on 2026-10-30. Use the root README for current commands.

Completed 2026-10-29. Rust/Tauri remains the reference. Go now executes a complete
single-group/VU/iteration journey through the shared application run port.

## Implementation

1. Added `Runs.Start/StartFile/Cancel`, bounded `RunHandle.Events`, repeatable
   `Wait`, explicit owner disposal, report destination checking and report export.
   Admission, compilation, credential resolution, HTTP configuration and initial
   projection budgets complete before the first HTTP request. Each run owns its
   compiled snapshot, session, credentials, values, trace and cancellation state.
2. Added driven ports for per-VU HTTP sessions, credential lookup, bounded response
   JSON, URL projection/interpolation, deadlines and report files. Their adapters
   use net/http with isolated cookie jars, environment variables, the existing
   duplicate-aware JSON reader, WHATWG URLs, system contexts and atomic file writes.
   Core code imports neither adapters nor ambient network/filesystem/logger APIs.
3. Added the bounded interpreter for start/end, HTTP, native bearer/cookie auth,
   atomic extraction/session updates, scoped reusable calls, typed mappings,
   exclusive routing and lifecycle exports. Runtime predicates, extraction and
   auth tokens use the external pbinitiative FEEL handles prepared in Phase 2.
4. Implemented fresh authentication sessions, header precedence, origin/path-prefix
   protection, percent-encoded environment substitution, no redirects or implicit
   retry replay, response caps and safe transport error classifications.
5. Added separate work/owner contexts. User cancellation stops iteration; eligible
   cleanup uses an independent bounded deadline. Owner disposal and safety stops
   skip cleanup. Cleanup failure cannot replace an earlier failure. Events and
   retained reports have resource budgets; stalled consumers stop the run instead
   of growing queues indefinitely. Terminal report information remains available.
6. Added report v3 projections, scoped events and redaction before public delivery.
   Authentication/retained-value journeys omit sensitive bodies and header values
   from the beginning. Snapshots hide credentials, literals, environment values,
   expressions and layout; the safe graph retains layout and positional keys.
   Public event/report payloads have independent ownership and preserve JSON numbers.
7. Enabled Go CLI `run <scenario> [--output <report>]` and execution capability.
   Exit codes: 0 passed, 2 preparation, 3 execution/report failure, 130 cancelled.
   Runtime diagnostics use `application.Logger`, wired to Charm Log. Stdout is
   the CLI's event-name/summary protocol. No fmt logging was introduced in the core.
8. Extended both showcase formats with an interpolated cleanup query, its exercise
   guidance and Rust regression. Updated only their two fixture hashes; frozen
   validation outcomes, reference fingerprints and FEEL expectations remain intact.

## Verification

From `go/`:

```sh
go test ./... -count=1 -timeout=90s
go vet ./...
go test -race ./internal/application ./internal/adapters/inbound/cli ./internal/adapters/outbound/httpclient ./internal/adapters/outbound/reportfiles -count=1 -timeout=90s
CGO_ENABLED=0 go build -o bin/interl-go-headless ./cmd/interl-go
go build -o bin/interl-go ./cmd/interl-go
```

From the repository root:

```sh
node scripts/capture-migration-baseline.mjs --check
node scripts/check-go-validation-parity.mjs
node scripts/check-go-run-parity.mjs
cargo test --offline --locked -p interl-engine --test reusable feature_showcase_runs_in_json_and_yaml_with_identical_safe_outcomes
```

Execution tests cover the showcase in JSON/YAML, both branch outcomes, fake
transport/fixed clock replacement and real HTTP. They verify typed call results,
fresh authentication, cookie/header isolation, report/event ownership, secret
absence in reports/streams/Charm logs, cleanup cancellation/deadlines, primary
error precedence, stalled consumers, response/report limits and file aliases.
CLI tests check passed/cancelled reports, exit codes, numeric precision and input
overwrite rejection. The nine-case differential harness compares full normalized
Rust/Go reports, CLI output and wire requests; normalization is documented in
`tests/compatibility/README.md`.

The Go YAML showcase also passed against the existing Rust local test-server
example using `example.env`: twelve requests/responses, all HTTP 200, completed
cleanup, and safe report v3. Native UI testing remains with the user.

## Run locally

Start the existing server in a separate terminal:

```sh
cargo run --locked -p interl-http --example test_server
```

Build from `go/`, then run from the repository root:

```sh
(set -a && source ./example.env && ./go/bin/interl-go run scenarios/examples/feature-showcase.interl.yaml --output /tmp/interl-go-showcase.json)
```

## Next

Phase 4 is now complete; see the [parity journal](phase-4-parity.md). It refines
the initial delivery behavior to use cancellation-aware backpressure: consumers
drain events or explicitly close the receiver/owner, rather than being stopped
by a short delivery timeout.

Phase 4 expands the compatibility matrix to every supported example and meaningful
engine/HTTP regression, including repeated invocation scopes, alternate identity
modes, malformed/truncated response data, cancellation races and resource/error
boundaries. The nine external-library FEEL differences remain explicitly recorded;
this phase does not claim complete Rust semantic parity. Phase 5 wires native
scenario operations and real execution to Wails; those UI actions remain disabled.
