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

See [DESIGN.md](DESIGN.md), [the M1 plan](plans/milestone-1.md), and [the supported scenario contract](docs/scenario-v2.md).

For a complete runnable example, start with the [living feature showcase](scenarios/examples/README.md#living-feature-showcase). Extend its JSON/YAML pair whenever a supported feature is added.

M3 implementation and automated verification are available; native acceptance remains pending. The shared Rust engine now runs the `auth-login.interl.json` and `.yaml` workflows using environment credential references. New reports use version 3 after M3.5 Phase 1; validation still needs no credentials or network access. Authenticated CLI and Rust desktop-adapter parity are tested; desktop auth-node loading/rendering and reference-only inspection are implemented in Phase 9; native interaction checks remain pending. See [M3 progress and schema additions](docs/milestone-3.md).

The active milestone is [M3.5 — Scenario editor and visual execution](plans/milestone-3.5.md). Phases 1–6 provide node authoring, reusable workflows/lifecycle, typed configuration, exclusive routing, and captured Run visualization with grouped execution logs. Scenario schema v2 and report/live protocol v3 are supported; V1 is unsupported. Phase 6b's unified lifecycle canvas and Phase 7 end-to-end/native acceptance remain pending. M4 adds load scheduling; M5 retains parallel branching and barriers. See [current capabilities and verification](docs/milestone-3.5.md).

A [Go + Wails migration plan](plans/go-wails-migration.md) defines a parallel implementation with hexagonal architecture, shared React UI and compatibility gates. It is planned work; the existing Rust/Tauri app and CLI remain available.

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

## Desktop — local development

To restart with the local example credentials, stop the running Tauri development
process with Ctrl-C and close the app. From the repository root:

```bash
(set -a && source ./example.env && npm --prefix apps/desktop run tauri -- dev)
```

The committed `example.env` contains only the local server's fake credentials,
including the references used by the showcase and new Authentication nodes.
Loading the variables after the app has started does not update its environment;
restart it from the configured terminal. Keep the local test server running in a
separate terminal:

```bash
cargo run --locked -p interl-http --example test_server
```

The desktop opens in **View/Run**, with a scenario dropdown and a read-only canvas. Use **Open scenario** for YAML/JSON. Switch to **Edit** to rename the scenario, move nodes, or use **Add node** (+) → **HTTP Request**. Configure the request method/path in its inspector. Save, Undo, and Redo are compact Edit-only icons; Save enables for unsaved persisted changes. Existing JSON files save in place; new/YAML scenarios choose a JSON destination on first save. YAML is never overwritten by JSON, and original-source metadata remains separate from the save destination.

**Add node → Authentication** adds a login node before existing requests; choose another connection in the picker when needed. Each node owns its username/password environment references (initially `INTERL_USERNAME` and `INTERL_PASSWORD`). Configure the login request and choose **Bearer token** or **Session cookie**, then apply changes. Following nodes automatically use this session until another Authentication replaces it. Failed login stops the run; credentials, tokens, and cookies remain in Rust. Set the **Scenario base URL** once (including a prefix such as `/api/v1`); HTTP Request and Authentication inherit it and can specify a per-node override. See the [schema contract](docs/scenario-v2.md#authentication-nodes-and-base-urls--2026-10-19).

Use **Validate** in the bottom **Validation** tab, then the **Run** tab to execute the validated snapshot. Dirty transitions offer Save/Discard/Cancel. Run preparation still validates the exact input in Rust. **Cancel**, safe event inspection, and **Export report** remain available. The active run uses an immutable capture while its originating draft can be edited independently; scenario switching stays disabled until the run finishes.

Run displays the captured graph with observed node/edge progress. Select lifecycle
phases and workflow invocations in the sidebar, or open a call from its node.
Each node execution has one expandable log row with status, duration, HTTP status,
and safe event details. Use **View scenario** / **View run** in the Run sheet to
switch between the current draft and retained results. The scenario and theme
dropdowns use styled shadcn Select controls with keyboard navigation.

To test validation feedback, open `scenarios/examples/invalid-json.interl.json` in the desktop app. Its trailing comma is intentional: opening reports diagnostics and preserves the current scenario so it is not replaced with an unusable document. While loading or validating a scenario, a rotating loader replaces the result icon. Valid files show a check icon in the label's color and preserve the selected tab.

For a browser-only visual preview:

```bash
npm --prefix apps/desktop run dev
```

The preview supports bundled scenario selection and Edit-only naming, layout, and HTTP insertion. Native file opening, saving, and authoritative validation, HTTP execution, and report export require Tauri; the preview labels this limitation and disables those actions.

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


The [scenario contract](docs/scenario-v2.md) is exported from Rust as
[JSON Schema](schemas/scenario.schema.json) and generated TypeScript file types.
Run `npm --prefix apps/desktop run schema:generate` after changing persisted Rust
models; `npm --prefix apps/desktop run schema:check` checks both artifacts for drift.
The `body-routing.interl.json` / `.yaml` examples use the local test server's
`/routing-body` endpoint to branch on `response.body.value.status = true`.
