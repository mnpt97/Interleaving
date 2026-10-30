# Interleaving

A desktop and CLI tool for stateful API concurrency and correctness testing.
Wails + Go is the sole implementation, with a shared React editor and a hexagonal
Go application core. The macOS package was verified by the user; the migration is
complete. Current execution supports one virtual user with authentication,
exclusive routing, reusable workflows, lifecycle cleanup and redacted reports.
Load scheduling and parallel branching remain future milestones.

## Repository

```text
apps/desktop/                 React + TypeScript + Vite + shadcn UI
go/                           Go domain, application, ports and adapters
schemas/scenario.schema.json  Canonical scenario file contract
scenarios/examples/           Equivalent JSON/YAML examples and living showcase
tests/                        Contract and editor regression fixtures
scripts/                      Schema generation and contract/architecture checks
docs/                         Design, verification and migration evidence
```

See [DESIGN.md](DESIGN.md), the [Go architecture guide](go/README.md), and the
[living feature showcase](scenarios/examples/README.md#living-feature-showcase).

## Setup

Use Go 1.27.3, Node 22.12+ compatible with the frontend lockfile, and macOS Xcode
Command Line Tools. Install the pinned official Wails CLI and frontend dependencies:

```sh
go install github.com/wailsapp/wails/v3/cmd/wails3@v3.0.0
npm --prefix apps/desktop ci
```

No Rust toolchain is required. Native acceptance currently covers macOS; additional
platforms require their own native verification.

## Local development

Start the local test server from the repository root in one terminal:

```sh
go -C go run ./cmd/interl-test-server
```

Start Wails in another terminal with the local fake credentials:

```sh
(set -a && source ./example.env && npm --prefix apps/desktop run wails -- dev)
```

Wails owns the Vite server and native process. Stop with Ctrl-C. `example.env`
contains fake fixture credentials only. Restart the app after changing its startup
environment. The server listens on `http://127.0.0.1:8080` and logs per-request
`duration_ms`; see [server endpoints and timing boundaries](docs/test-server.md).

For a browser-only editor preview, use `npm --prefix apps/desktop run dev`.
Native file dialogs, authoritative validation and execution require Wails.

## CLI

Build the headless executable from the repository root:

```sh
go -C go build -o bin/interl-go ./cmd/interl-go
./go/bin/interl-go validate scenarios/examples/feature-showcase.interl.json
(set -a && source ./example.env && ./go/bin/interl-go run scenarios/examples/feature-showcase.interl.yaml --output /tmp/interl-results.json)
```

Validation is offline and requires no credentials. The same Go application handles
desktop and CLI execution. Exit codes: `0` completed, `2` preparation/validation
failure, `3` execution/report failure, `130` cancelled. HTTP 4xx/5xx responses alone
do not fail a workflow. Ctrl-C cancels execution and permits eligible bounded
cleanup. Reports and UI events redact credential values. Runtime logging uses the
logging port with Charm Log; the CLI does not import Wails.

## Desktop usage

Open JSON/YAML scenarios in View/Run. Edit enables node configuration, movement,
connections, rename, undo/redo and saving. JSON saves preserve draft state; YAML
sources are saved to a chosen JSON destination. Unsaved transitions offer Save or
Discard; closing the dialog leaves the draft in place.

Run uses an immutable captured graph. Inspect node executions and nested calls in
the Run sheet, cancel during execution, or export the safe report. The living
showcase and its [UI exercise guide](scenarios/examples/README.md) cover supported
features, including lifecycle boxes and long polling.

## Build and package

```sh
npm --prefix apps/desktop run build
npm --prefix apps/desktop run wails -- package
```

The official Wails task creates `go/bin/interleaving-go.app` with embedded frontend
assets and an ad-hoc signature. No Vite server is needed by the package. To launch
it with the local fake credentials:

```sh
(set -a && source ./example.env && ./go/bin/interleaving-go.app/Contents/MacOS/interleaving-go)
```

Finder does not source `example.env`. Developer ID signing/notarization for wider
distribution is separate from the verified local package. See the
[migration completion record](docs/migration/phase-7-retirement.md).

## Schema and verification

`schemas/scenario.schema.json` is authoritative. After a schema change, update Go
models/validation and regenerate the TypeScript and Go-embedded consumer copies:

```sh
npm --prefix apps/desktop run schema:generate
npm --prefix apps/desktop run schema:check
go -C go test ./... -count=1
go -C go vet ./...
node scripts/check-migration-boundaries.mjs
npm --prefix apps/desktop run typecheck
npm --prefix apps/desktop test -- --maxWorkers=2
npm --prefix apps/desktop run build
```

Go integration tests use ephemeral loopback ports. The frozen
[contract regressions](tests/compatibility/README.md) require no legacy executable.
Extend the JSON/YAML showcase and its regression when adding a supported feature.
