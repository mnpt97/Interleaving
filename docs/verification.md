# Verification — M1, M2, and M3 progress

M1 loading, schema/graph validation, CLI validation, Tauri opening/export, layout editing, and light/dark appearance were implemented and accepted by the user. M2 adds one-user HTTP execution using the shared Rust engine. No authentication workflow, concurrent scheduling, synchronization barriers, mTLS lifecycle, FEEL evaluation, or advanced reporting/editor features were added.

## M2 automated results — 2026-10-14

These are the initial M2 delivery results. The later Phase 1 verification below includes the subsequent UI refinements and new schema tests.

- Workspace metadata resolves the five intended members and the existing dependency direction.
- `cargo fmt --all -- --check` passes. The shared test fixture uses imports that format consistently under the Rust 2021 library crates and Rust 2024 Tauri member.
- `cargo clippy --workspace --all-targets --offline --locked -- -D warnings` passes.
- `cargo test --workspace --offline --locked` passes: **44 Rust tests**, no ignored tests.
- `npm --prefix apps/desktop run test` passes: **25 frontend tests** across six suites.
- `npm --prefix apps/desktop run build` passes application/configuration TypeScript checks and the production Vite build.
- `npm --prefix apps/desktop run tauri -- build --debug --bundles app` passes and produces `target/debug/bundle/macos/Interleaving.app`.
- The UI source detector reports no findings for the changed shell, stylesheet, run panel, and canvas. This source check does not establish visual or native interaction correctness.

The workspace test suite verifies GET/POST bodies, headers, encoded interpolation, isolated cookie jars, no automatic redirects, response limits, 200/404 responses, distinct connection/DNS/TLS/body/timeout errors, cancellation in flight and under channel backpressure, receiver-close cancellation, ordered events, bounded reports, redaction of snapshots and live/report payloads, YAML/JSON multi-request execution, layout independence, CLI exits and Ctrl-C partial reports, and shared desktop/headless preparation. Parity checks exclude naturally varying run IDs, elapsed timing, and Date headers.

The HTTP suite was also rerun after a test-only Clippy fix that checks the number of bytes read from the mock peer. No production behavior changed after the full workspace pass.

## M3 Phase 1 automated results — 2026-10-14

- Core-only tests pass: **24 tests**, including ten new auth schema tests.
- Full workspace tests pass: **55 Rust tests**, no ignored tests, including CLI rejection of both staged auth fixtures and existing local HTTP/engine/adapter regressions.
- Workspace check and all-targets Clippy pass with `--offline --locked`; Clippy denies warnings. Formatting check passes.
- Frontend tests pass: **40 tests** across seven suites, including the pre-M3 inspector and Run-tab refinements. Frontend type checks and production build pass.
- Documentation links and code fences were checked. No new dependencies were installed for Phase 1; repository manifests and lockfiles were not changed by this phase.

This verifies typed schema parsing, strict shapes, format equivalence, literal preservation, resource/duplicate-key regressions, and temporary capability guards. Authentication execution is not enabled. The auth examples remain parse-only schema fixtures; CLI validation/run returns exit 2 with `unsupportedFeature`. Native desktop smoke checks were not rerun; their pending status below is unchanged. See [M3 progress](milestone-3.md).

## M3 Phase 2 automated results — 2026-10-14

- Core tests pass: **42 tests**, including 17 auth semantic/dataflow tests and one internal AST/budget test added in Phase 2.
- Full workspace tests pass: **75 Rust tests**, no ignored tests. The CLI validates both auth formats without credential environment values, while auth execution remains rejected with exit 2/`unsupportedExecutionFeature`. Engine tests guard each new capability independently, and Rust Tauri tests verify shared validation/expression diagnostics.
- Workspace check, formatting check, and workspace/all-targets Clippy pass with `--offline --locked` and warnings denied for Clippy.
- All **40 frontend tests** and the production build/type checks pass. Frontend auth wire models/rendering remain pending; this does not establish UI support for the new fixtures.
- Pinned `dsntk-feel` and `dsntk-feel-parser` to 0.3.0 in core; the root lockfile includes their dependencies. The general evaluator is absent. No frontend packages were added.
- Documentation links and code fences pass inspection. No credentials were resolved or authentication HTTP steps executed; native smoke checks were not rerun.

Phase 2 verifies identity/field/reference checks, graph ordering, header conflicts, FEEL syntax limits and separate expression locations, template composition bounds, immutable dependency/capture/liveness metadata, canonical serialization, and preservation of literal/M1/M2 semantics. See [M3 progress](milestone-3.md).

## M3 Phase 3 automated results — 2026-10-14

- Full workspace tests pass: **89 Rust tests**, none ignored. Engine unit tests now total **15**, covering cached/selected reference resolution, missing/empty/non-Unicode/provider failures, individual/aggregate/reference limits, safe Debug/errors, sensitive observation omission, raw/encoded/escaped echoes, reference-only snapshots/exports, trace-channel/report parity, and redaction exhaustion.
- An engine integration regression proves anonymous and capability-rejected runs do not call the injected provider. Existing CLI, core, HTTP, engine cancellation/report, and Rust Tauri parity regressions pass.
- Formatting, workspace check, and workspace/all-targets Clippy with warnings denied pass using the existing locked dependencies.
- All **40 frontend tests** and production build/type checks pass. No frontend code, packages, manifests, or lockfiles changed in Phase 3.
- Documentation links and code fences pass inspection. Native desktop smoke checks were not rerun.

Auth remains capability-gated. Resolution/capture tests use injected fake credentials and synthetic HTTP observations through the real trace channel; they do not establish real login, extraction, session mutation, or desktop auth behavior. End-to-end startup/CLI/Tauri credential-failure checks remain Phase 7/8 work. Phase 3 also fixes text-request byte metadata and makes matcher exhaustion explicitly omit body/header previews. See [M3 progress](milestone-3.md).

## M3 Phase 4 automated results — 2026-10-15

- All **eight targeted auth fixture tests** and **97 workspace Rust tests** pass, none ignored. Coverage includes accepted fake identities, fresh token/cookie/session values, independent bearer/cookie access, rejection and ambiguity cases, controlled token-response variants, response truncation, readiness-based cancellation, timeout, and bounded session storage.
- Formatting check, workspace check, and workspace/all-targets Clippy with warnings denied pass using `--offline --locked`. All-targets Clippy checks the manual server example as well as the shared fixture used by adapter tests. A Rust-edition-dependent import ordering conflict and a Clippy conditional warning in the shared module were corrected before final checks.
- All **40 frontend tests** and production build/type checks pass. No frontend source, dependencies, manifests, or lockfiles changed in Phase 4.
- Documentation file links and code fences pass inspection. No interactive manual-server smoke or native desktop acceptance check was performed.

The automated and manual servers share the same controlled auth handlers. See [startup, fake credentials, route contracts, and limits](test-server.md). Engine auth execution remains gated; these results verify endpoints/transport, not login workflow execution, extraction, or session mutation. New two-user isolation acceptance tests remain deferred to M4. Existing isolation tests continue to pass. Native M2 acceptance remains pending.

## M3 Phase 5 automated results — 2026-10-15

- All **seven targeted HTTP session-update tests** and **104 workspace Rust tests** pass, none ignored. Coverage includes case-insensitive precedence, replacement/removal of inherited headers, request overrides, immutable prepared header snapshots, failed-update atomicity, name/value byte boundaries, encoded body/path limits, materialized JSON literal preservation, cookie retention, and safe failure at transport header capacity.
- Formatting check, workspace check, and workspace/all-targets Clippy with warnings denied pass using `--offline --locked`. Rust lint/regression checks were rerun after replacing potentially panicking header insert/merge paths with checked operations. Capacity failures preserve prior session values and removal markers.
- All **40 frontend tests** and production build/type checks pass. No frontend source, adapters, dependencies, manifests, lockfiles, or report versions changed.
- Documentation file links and code fences pass inspection. Native desktop smoke checks were not rerun; M2 acceptance remains pending.

Phase 5 adds HTTP transport APIs only: atomic session header updates preserve the existing client/cookie jar, and resolved bodies pass through normal HTTP validation and byte limits. The engine still rejects auth workflows. Template materialization, FEEL extraction, workflow `setSession` execution, and adapter/UI support remain later phases. Existing cookie-isolation regressions pass; no new two-user isolation acceptance tests were added. See [M3 HTTP API behavior](milestone-3.md#http-session-updates-and-resolved-request-bodies).

## M3 Phase 6 automated results — 2026-10-15

- All **15 targeted extraction tests** and **119 workspace Rust tests** pass, none ignored. Engine unit tests now total **30**. New coverage includes YAML/JSON-equivalent compiled paths, maximum path/string boundaries, literal preservation, unsupported FEEL rejection, missing/null/wrong-type/empty/oversized outputs, duplicate decoded keys, malformed UTF-8/numbers, trailing content, depth/value budgets, unavailable/truncated bodies, response/output admission and reclamation, stale-output invalidation, and safe Debug/errors.
- A deterministic serde-protocol test proves cancellation between parsed values stops interpretation without a timing race. Operation-boundary cancellation checks and direct controlled login/missing-token/malformed/text/unfamiliar-field/truncated-response cases also pass. These tests do not invoke production auth workflows.
- Formatting check, workspace check, and workspace/all-targets Clippy with warnings denied pass using `--offline --locked`. All **40 frontend tests** and production build/type checks pass.
- Documentation file links and code fences pass inspection. No dependencies, manifests, lockfiles, adapters, frontend source, or report versions changed. Native desktop checks were not rerun; M2 acceptance remains pending.

The private extraction foundation executes only the bounded, core-compiled FEEL object-property subset. It has no credential provider, mutable session, scheduling, or event API. Production auth remains capability-gated. Phase 7 must connect the helpers to the user runner, register sensitive outputs before observations, invoke last-use cleanup, and remove the temporary non-test dead-code allowance. New two-user isolation acceptance remains M4 work. See [limits and staged extraction behavior](milestone-3.md#bounded-extraction-foundation).

## M3 Phase 7 automated results — 2026-10-15

- Full workspace tests pass: **137 Rust tests**, none ignored. Engine coverage now includes **33 unit tests**, **15 auth integration tests**, and **11 existing runner integration tests**. The 18 added tests verify bearer login in both formats/connectivity order, cookie-only login, static bearer references, captured credentials, typed nested templates/literal preservation, rendered-body/header bounds, atomic failed updates, credential/extraction failure stopping, unfamiliar token capture, inherited header removal, cookie retention, cancellation/receiver closure/incomplete-body reads, safe partial reports, and auth report budgets.
- Auth integration checks compare live events with retained report events when delivery completes. Sensitive source observations are omitted before delivery, reports omit fake token/cookie/password values, session events contain normalized header names only, and literal session values/custom prefixes are redacted from snapshots. Existing raw/encoded/escaped redaction and extraction/liveness regressions remain green.
- Formatting, workspace check, and workspace/all-targets Clippy with warnings denied pass with `--offline --locked`. New reports use **version 2** for anonymous and authenticated runs. Rust/CLI expectations and frontend report/session-event wire validation were updated together; historical v1 remains documented in M2.
- All **43 frontend tests** across eight suites and production build/type checks pass. Three new wire-contract tests cover v2 live/report acceptance, historical/unknown version rejection, and rejection of session payloads containing values or malformed name lists. Auth scenario loading/rendering remains Phase 9.
- The first full regression run found an existing parallel-test temporary-directory collision in CLI tests: wall-clock nanoseconds were assumed unique. An atomic per-process suffix fixes that isolation issue; the complete suite then passed. Clippy/formatting were rerun after the test-only fix.
- Documentation file links and code fences pass inspection. No dependencies, manifests, or lockfiles changed. Native desktop smoke and dedicated authenticated adapter acceptance were not performed; M2 native acceptance remains pending.

Phase 7 enables authentication in the shared engine and removes the staged capability gate/dead-code allowance. CLI/Tauri inherit the engine capability, but dedicated authenticated adapter parity, accepted-start lifecycle guards, and failure presentation remain Phase 8. No new two-user isolation acceptance tests or M4 scheduling were added. See [runtime and report v2](milestone-3.md#user-runtime-and-report-v2).

## M3 Phase 8 automated results — 2026-10-15

- Full workspace tests pass: **152 Rust tests**, none ignored. Four new authenticated CLI process tests and eleven new desktop-adapter tests were added. Desktop tests now total **17**; existing engine/core/HTTP regressions remain green.
- CLI tests supply credentials only through each child's environment, preserve original input bytes, compare YAML/JSON reports to the headless engine, and check safe stdout/stderr/report data. Missing/empty/oversized credentials return exit 2 without HTTP hits or an output report. Extraction/session failures return 3; cookie-only success and completed HTTP 401 workflows return 0; an owned Unix child receives SIGINT and returns 130 with a safe partial report.
- Rust desktop-adapter tests exercise the command helpers for reservation before preparation, duplicate-start rejection, provider IO outside the mutex, cancelled/failed/unwound preparation cleanup, previous-report retention, stale IDs, task failure, failed event delivery, window cleanup during auth IO, safe exports, and authenticated format/headless parity. Failed native event sends are included in the report's dropped-delivery count. Test providers never mutate the parent environment.
- All **49 frontend tests** across nine suites and production build/type checks pass. Six new tests cover structured single/array diagnostics, bounded diagnostic lists, safe fallbacks, missing-reference presentation, and retry after rejected preparation. Auth scenario loading/rendering is still Phase 9.
- Formatting, workspace check, and workspace/all-targets Clippy with warnings denied pass using `--offline --locked`. Documentation file links/code fences pass inspection. Report v2 is unchanged; no dependencies, manifests, or lockfiles changed.

These results establish actual CLI process behavior and Rust desktop-adapter/helper/serialization parity. Native webview/dialog/IPC interactions were not exercised, and M2 native smoke acceptance remains pending. Phase 9 has not started; no M4 scheduling or new two-user isolation acceptance tests were added. See [adapter lifecycle and verification boundary](milestone-3.md#cli-and-desktop-adapter-lifecycle).

## M3 Phase 9 automated results — 2026-10-15

- All **66 frontend tests across 12 suites** pass. Seventeen new tests cover M3 wire fields and recursive template preservation, rejected credential/source shapes, expression locations, new-node presentation/handles/selection/status, reference-only inspection, hidden literal values/prefixes, names-only session changes, sensitive payload omission, and safe extraction failures.
- Desktop bridge tests cover replacement of the workflow from JSON/YAML open responses, preserving source metadata and auth configuration during layout edits/JSON save, and an immutable reference-only active-run input. YAML parsing remains Rust-owned; frontend tests mock the validated canonical document returned by Rust.
- `npm --prefix apps/desktop run build` passes application/configuration TypeScript checks and the production Vite build. The existing Tailwind plugin timing advisory is informational. The mechanical UI detector returns no findings; updated documentation links/code fences pass inspection.
- No Rust source, dependencies, manifests, or lockfiles changed. Rust checks/tests were not rerun for this frontend-only phase; Phase 8's **152 passing Rust tests** remain the last full-workspace evidence.
- No native webview/dialog/IPC interaction or visual light/dark smoke was performed. Previously declined Computer Use access was not retried. M2 native acceptance and M3 native auth/timeout/cancel/export acceptance remain pending for Phase 10. No M4 implementation or new two-user isolation tests were added.

## M3 Phase 10 final automated results — 2026-10-15

- `cargo metadata --offline --locked --format-version 1 --no-deps` succeeds with five workspace members. Rust formatting, workspace check, and workspace/all-targets Clippy with warnings denied pass.
- `cargo test --workspace --offline --locked` passes in the final serial run: **153 Rust tests**, none ignored, plus all doctest checks. Added one cookie example format/compile/serialization test; the existing engine cookie test now runs both actual example files. CLI process and Rust desktop-adapter parity/security/lifecycle regressions remain green.
- All **66 frontend tests across 12 suites** pass. Prettier check, application/configuration TypeScript checks, and the production Vite build pass. Generated Tauri schemas are excluded from UI formatting; user formatting preferences are preserved.
- `npm --prefix apps/desktop run tauri -- build --debug --bundles app` succeeds, producing `target/debug/bundle/macos/Interleaving.app`. The documented executable path exists. The existing `.app` bundle-identifier advisory and Vite plugin timing advisory are informational; the application identifier was not changed.
- All **nine valid JSON/YAML example files** validate with the required credential variables absent; `invalid-json.interl.json` remains intentionally invalid and exits 2. Bearer/cookie engine tests execute both formats against the controlled server. Current manual-server CLI commands are documented; no separate interactive manual-server session was performed in this phase.
- Updated documentation file links and code fences pass. Reference-only cookie examples, current bearer YAML comments, supported-schema/ADR guidance, CLI/dev/packaged environment inheritance, and the [native acceptance checklist](m3-acceptance.md) are documented. The design table reflects the already-accepted M4 isolation deferral.
- An initial overlapping package/test run passed all unit/integration tests but rustdoc could not locate a Tauri artifact while packaging rebuilt shared dependencies. After builds finished, the complete serial workspace rerun passed without code changes. Avoid overlapping packaging and final doctest verification when collecting acceptance evidence.

Implementation and automated Phase 10 verification are complete. Native rendering, dialogs, Run/Cancel, theme, and report-export interaction remain **pending** in [M3 acceptance](m3-acceptance.md). Previously declined app access was not retried. No M4 implementation, new two-user isolation acceptance tests, dependency installation, lockfile changes, Git staging, or commits were performed in this phase.

## Issues found and corrected during verification

- Refactoring snapshot redaction briefly returned the original snapshot. Regression tests caught retained layout and sensitive header data; the corrected implementation removes layout and returns the redacted value.
- A TLS error wrapped in `io::Error` initially appeared as a generic connection failure. Classification now inspects the wrapped error directly; the local invalid-TLS-peer test passes.
- Incomplete response bodies use reqwest's decode-error path. They now receive the explicit body-error category.
- JSON requests retain an explicit Content-Type instead of appending a generated duplicate; POST integration coverage verifies this header behavior.
- Cancellation interrupts a blocked event send and an in-flight HTTP request when the receiver closes. Terminal events remain in the bounded report even when live delivery is unavailable.

## Native M2 acceptance remaining

Computer Use access to Interleaving was not approved. Native rendering, Run/Cancel clicks, file dialogs, and report export could not be exercised interactively in this session. The successful bundle and adapter/frontend tests do not replace that verification. M1's user-confirmed acceptance does not establish M2's native interaction behavior.

Follow the [README desktop smoke steps](../README.md#verification) with the included loopback-only axum server. Compare CLI and desktop reports by status, event types, node IDs, request methods, and response payloads; IDs/timing and generated Date headers naturally differ. Confirm light and dark appearance, cancellation, timeout diagnostics, and JSON export.

## Packaging advisory and ownership

Tauri retains its existing advisory that bundle identifier `dev.interleaving.app` ends in `.app`. It does not prevent the debug bundle from building; review naming before distribution. Release packaging/signing is outside M2.

Build outputs remain ignored. Cargo dependencies update the root lockfile; no frontend dependencies were added for M2. Git staging and commits are handled by the user; neither was performed here.

See [M2 execution/report contracts](milestone-2.md) and the [scenario v1 contract](scenario-v1.md) for supported behavior and resource limits.
