# Go / Wails implementation

Wails + Go is the sole implementation. The user verified the packaged macOS app
and the migration is complete (2026-10-30). Both desktop and CLI use the same
hexagonal application core. See the [retirement record](../docs/migration/phase-7-retirement.md).

## Start from the repository root

Prerequisites: Go 1.27.3, existing frontend npm dependencies, and the official Wails CLI:

```sh
go install github.com/wailsapp/wails/v3/cmd/wails3@v3.0.0
```

Start Wails with the local fake credentials:

```sh
(set -a && source ./example.env && npm --prefix apps/desktop run wails -- dev)
```

The npm alias invokes **`wails3 dev`** from the Go project directory. It adds Go's
usual installed-tool directory to the child process PATH; it does not install Go,
change global settings, or invoke a custom launcher. Wails runs the Vite server on
its standard port **9245**, waits for it, starts the signed development bundle,
and manages frontend hot reload and Go rebuild/restart. Stop it with Ctrl-C.

The shared UI stays in `apps/desktop`; generated frontend assets go to
`go/frontend/dist`. `go/main.go`, `Taskfile.yml`, and `build/` follow the official
`wails3 init -t react` scaffold. Generated tasks are adapted only for this shared
frontend location, its Wails build scripts, and its generated binding directory.

## Standard build and package commands

From the repository root:

```sh
npm --prefix apps/desktop run wails -- build
npm --prefix apps/desktop run wails -- package
npm --prefix apps/desktop run wails -- task common:generate:bindings
```

Or from `go/`, with Go's bin directory on your PATH: `wails3 dev`, `wails3 build`,
and `wails3 package`. On macOS, the generated Wails tasks prepare the build output,
create fresh development/production bundles, ad-hoc sign them, and verify signatures.
The packaged app is `go/bin/interleaving-go.app`, with identifier
`dev.interleaving.go`. Developer ID signing/notarization for distribution is separate.
For the release app with the local fake credentials, run from the repository root:

```sh
(set -a && source ./example.env && ./go/bin/interleaving-go.app/Contents/MacOS/interleaving-go)
```

Finder does not source `example.env`. Close the current app before changing its
startup environment. The package embeds the frontend and needs no Vite server.

There is no root `package.json`, custom Node build coordinator, hand-written bundle
creation, or startup probe. The old `InterleavingGo.app` custom artifact is retired.

## CLI and architecture

Start the Go test server from the repository root in its own terminal:

```sh
go -C go run ./cmd/interl-test-server
```

It listens on `http://127.0.0.1:8080` and provides all existing HTTP, fake auth and
long-poll endpoints. No environment loading is needed for the server. Load
`example.env` when starting Wails/the scenario CLI. See [test-server details](../docs/test-server.md).

From `go/`:

```sh
go test ./...
go vet ./...
go build -o bin/interl-go ./cmd/interl-go
./bin/interl-go status
./bin/interl-go probe
./bin/interl-go validate ../scenarios/examples/feature-showcase.interl.json
./bin/interl-go validate ../scenarios/examples/feature-showcase.interl.yaml
```

The CLI never imports Wails or the UI. `status` reports validation true/execution true;
`probe` emits bounded synthetic JSON on stdout and diagnostics on stderr.
`validate` accepts JSON/YAML, returns 0 for valid input or 2 for invalid/unreadable
input, and prints safe structured-location diagnostics to stderr. It needs no
test server or credentials. `run` executes through the Go application port with
isolated HTTP/cookie sessions, external FEEL evaluation and redacted report v3.
Exit codes are 0 (completed), 2 (preparation failure), 3 (execution/report failure),
and 130 (cancelled). HTTP error status alone does not fail a workflow.

Start the existing local test server in another terminal, then from the repository root:

```sh
(set -a && source ./example.env && ./go/bin/interl-go run scenarios/examples/feature-showcase.interl.yaml --output /tmp/interl-go-showcase.json)
```

Ctrl-C cancels iteration and still permits eligible cleanup within its separate
deadline. Runtime diagnostics use the `application.Logger` port with Charm Log
on stderr. Stdout carries the ordered event names and final summary; it is command
output, not a second logger. Report writes reject input aliases and use private
temporary files followed by atomic replacement.

The application injects strict codecs, bounded file reads, the pbinitiative FEEL
parser/evaluator adapter, and WHATWG URL parsing through ports. The core validates graphs,
typed dataflow, inferred gateway pairs, lifecycle bindings and call limits.
Compiled plans contain checked dependencies and independent document snapshots.
Predicate, extraction and bearer-token evaluation delegates to the external
library. Its documented runtime outcomes recorded during migration include null handling,
numeric/boolean comparisons and logical coercion; no internal evaluator remains.
See [Phase 2 decisions and checks](../docs/migration/phase-2-scenario-feel.md).
See [Phase 3 execution and verification](../docs/migration/phase-3-execution.md)
for the initial slice, and [Phase 4 parity](../docs/migration/phase-4-parity.md)
for the complete documented engine/CLI matrix and known FEEL differences.

Live event consumers must drain the bounded stream or call `CloseReceiver` when
detaching. `Cancel` permits eligible cleanup; `Close` disposes the run owner.
Delivery does not fail a healthy run merely because a consumer is briefly slow.

`application.Logger`, `Clock`, and `IDs` are driven ports. The Charm Log v2.0.1
adapter (`charm.land/log/v2`) is injected at composition; the core imports no logger
library and cannot call Fatal. Wails framework logging uses the same adapter via
slog. Fake adapters and import/dependency checks enforce the hexagonal boundaries.
Credentials/raw response data must be sanitized before crossing the logging port;
known-key omission in the adapter is only defense in depth.

The shared frontend consumes `DesktopHost`; generated bindings and SDK imports
stay inside adapters. Wails local preferences keep their existing namespace. Native
scenario operations use the same application as the CLI. Official Wails JSON
streams carry bounded execution messages; generated bindings handle files,
validation, cancellation and report export. See the
[Phase 5 journal and native checks](../docs/migration/phase-5-desktop.md).

See the [standard setup correction](../docs/migration/wails-standard-setup.md),
[Phase 1 journal](../docs/migration/phase-1-shell.md), and
[hexagonal migration plan](../plans/go-wails-migration.md).
