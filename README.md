# Interleaving

A desktop and CLI tool for stateful API concurrency and correctness testing. **Single-user HTTP and stateful authentication are implemented, with cancellation, a redacted trace, and versioned JSON reports.** M1 loading, validation, and layout editing remain available.

## Repository

```text
apps/desktop/                 Tauri 2 + React + TypeScript + Vite
apps/desktop/src-tauri/       Thin desktop adapter
crates/interl-core/           Shared scenario schema, parsing, validation, compilation
crates/interl-http/           Isolated HTTP sessions and bounded responses
crates/interl-engine/         Sequential single-user execution and redacted events
crates/interl-cli/            CLI binary: interl
scenarios/examples/          Equivalent YAML and JSON examples
docs/                        Supported schema and verification
plans/                       Implementation plans and accepted decisions
```

See [DESIGN.md](DESIGN.md), [the M1 plan](plans/milestone-1.md), and [the supported scenario contract](docs/scenario-v1.md).

M3 implementation and automated verification are available; native acceptance remains pending. The shared Rust engine now runs the `auth-login.interl.json` and `.yaml` workflows using environment credential references. Reports use version 2; validation still needs no credentials or network access. Authenticated CLI and Rust desktop-adapter parity are tested; desktop auth-node loading/rendering and reference-only inspection are implemented in Phase 9; native interaction checks remain pending. See [M3 progress and schema additions](docs/milestone-3.md).

## Setup

Use a stable Rust toolchain with rustfmt and Clippy. The desktop requires Node compatible with Vite's declared engine (Node 22.12+ is supported by the current lockfile) and macOS Xcode Command Line Tools. Other platforms require their normal Tauri 2 prerequisites.

Install JavaScript dependencies from the committed npm lockfile:

```bash
npm --prefix apps/desktop ci
```

Cargo uses the root workspace lockfile. The headless CLI has no Tauri dependency. Both adapters use `interl-core` and the same `interl-engine`; only the desktop adapter depends on Tauri.

## Validate

```bash
cargo run --locked -p interl-cli --bin interl -- validate scenarios/examples/hello-api.json
cargo run --locked -p interl-cli --bin interl -- validate scenarios/examples/hello-api.interl.yaml
```

`.interl.json`, `.interl.yaml`, and `.interl.yml` are supported; plain `.json`, `.yaml`, and `.yml` are also accepted. Validation is offline and never contacts the example server. For validation, exit code `0` means valid and `2` means invalid input or an unreadable file. Diagnostics go to stderr; success goes to stdout.

## Run a local workflow

In one terminal, start the included loopback-only verification server:

```bash
cargo run --locked -p interl-http --example test_server
```

See [test-server routes and fake auth credentials](docs/test-server.md). M3 Phase 4 adds controlled auth endpoints; shared-engine auth execution is available; see [M3 runtime and environment setup](docs/milestone-3.md#user-runtime-and-report-v2).

In another terminal:

```bash
cargo run --locked -p interl-cli --bin interl -- run scenarios/examples/hello-api.interl.yaml --output /tmp/interl-results.json
cargo run --locked -p interl-cli --bin interl -- run scenarios/examples/http-workflow.interl.json --output /tmp/interl-workflow.json
```

Runs send real HTTP requests. Execution requires exactly one group with `count: 1` and a linear workflow; M3 adds the documented authentication steps to the original anonymous HTTP behavior. GET, POST, PUT, PATCH, and DELETE use the existing schema. Query strings go in `path`; `${variable}` resolves environment scalars with percent encoding. JSON and text bodies are supported; only bounded, complete JSON bodies receive a trace preview. Retries and automatic redirects are disabled.

Run exits: `0` completed transport workflow (including HTTP 4xx/5xx), `1` reserved for future assertion failures, `2` invalid input/configuration, `3` transport/infrastructure failure, `130` interrupted. Ctrl-C cancels in-flight work and still writes a partial report when `--output` is supplied. Human progress goes to stdout, diagnostics to stderr, and JSON only to the output file. M3 authentication uses environment references resolved in Rust; missing credentials reject startup with exit 2, and extraction/session failures stop later steps with exit 3. See [M2 execution and report contracts](docs/milestone-2.md).

## Authentication examples

Start the local server, then in the client terminal:

```bash
export INTERL_ALICE_USERNAME=alice
export INTERL_ALICE_PASSWORD=alice-password
cargo run --locked -p interl-cli --bin interl -- run scenarios/examples/auth-login.interl.yaml --output /tmp/interl-auth.json
cargo run --locked -p interl-cli --bin interl -- run scenarios/examples/auth-cookie.interl.json --output /tmp/interl-cookie.json
```

Launch Tauri from that same terminal to inherit the variables, then open either example format. No automatic `.env` loading or secret-entry UI exists. See [the examples](scenarios/examples/README.md), [runtime contracts](docs/milestone-3.md), and the [M3 acceptance checklist](docs/m3-acceptance.md), including launching a packaged app with its environment.

## Desktop

```bash
npm --prefix apps/desktop run tauri -- dev
```

The desktop validates the bundled example on opening. Use **Open scenario** for YAML/JSON, the **Validate** button in the **Validation** tab below the canvas to check the current document, and the inspector or canvas to change positions. Execution configuration is read-only. Switch to the **Run** tab to execute the validated snapshot, **Cancel** to stop it, select an event row to inspect redacted details, and use **Export report** to save the last completed report. Switching tabs, editing positions, or opening another scenario does not change an active run. **Save as JSON** uses a native save dialog and the core serializer; YAML is never overwritten by JSON export. The original source format/path remains associated with the open input.

To test validation feedback, open `scenarios/examples/invalid-json.interl.json` in the desktop app. Its trailing comma is intentional: validation fails, the Validation tab opens automatically, and a yellow warning icon appears before its label. While loading or validating a scenario, a rotating loader replaces the result icon. Valid files show a check icon in the label's color and preserve the selected tab.

For a browser-only visual preview:

```bash
npm --prefix apps/desktop run dev
```

The preview renders the bundled example and supports layout changes. Native file opening, saving, and authoritative validation, HTTP execution, and report export require Tauri; the preview labels this limitation and disables those actions.

## Verification

Run from the repository root:

```bash
cargo metadata --locked --format-version 1 --no-deps
cargo fmt --all -- --check
cargo check --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
npm --prefix apps/desktop run build
npm --prefix apps/desktop run test
npm --prefix apps/desktop run format:check
```

The frontend build checks both application TypeScript and Vite/Vitest configuration. Workspace tests include core parsing/graph regressions, a local axum server, isolated HTTP sessions, cancellation, redaction, bounded reports, CLI process/interrupt behavior, and adapter parity. Network tests bind loopback ports; restricted sandboxes must permit this. Frontend tests cover layout-only changes, stale asynchronous results, cancellation, diagnostics, and position form validation.

Desktop smoke checks:

1. Launch Tauri and confirm the example validates with three visible nodes.
2. Open both example formats and compare workflow and validation results.
3. Select and move a node; confirm inspector coordinates update.
4. Open an invalid file and inspect actionable diagnostics.
5. Save a JSON copy, validate it with the CLI, and verify the YAML source is unchanged.
6. Start the local test server, run both HTTP example formats, and inspect GET/POST response events.
7. Open `http-workflow.interl.json`, change a request path to `/not-found` in an external editor, reopen, and confirm HTTP 404 is a response with a completed run.
8. Change a request path to `/slow` and timeout to 1000 ms; confirm a timeout. Restore 10000 ms, reopen, Run, then Cancel; confirm a cancelled report.
9. Export a report and compare event types, node IDs, status, and response payloads with CLI output. Run IDs and elapsed timing naturally differ.

Builds and tests generate ignored artifacts. Root `Cargo.lock` and desktop `package-lock.json` belong in version control. All Rust profiles are defined in the root manifest.
