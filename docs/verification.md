# Verification — M1 and M2

M1 loading, schema/graph validation, CLI validation, Tauri opening/export, layout editing, and light/dark appearance were implemented and accepted by the user. M2 adds one-user HTTP execution using the shared Rust engine. No authentication workflow, concurrent scheduling, synchronization barriers, mTLS lifecycle, FEEL evaluation, or advanced reporting/editor features were added.

## M2 automated results — 2026-10-14

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
