# interl-engine

Runs a compiled scenario for one virtual user. CLI and desktop adapters use the
same preparation, interpreter, redaction, events, and final report.

`PreparedRun::new` compiles the document, validates resource limits and request
targets, resolves required credentials, and captures a redacted snapshot. It
returns diagnostics before execution if any admission check fails.

`PreparedRun::start_control` consumes the prepared run and spawns a Tokio task.
Drain `RunHandle.events` concurrently with `RunHandle.task`: the bounded channel
deliberately pauses execution when its consumer is slow. The final report retains
events even when cancellation prevents live delivery. Dropping a Tokio task handle
does not cancel the task; adapters should signal `StopCause::OwnerClosed` when closing.

## Source map

| Module | Responsibility |
| --- | --- |
| `report.rs` | Public options, events, reports, failures, and task handles |
| `program.rs` | Preparation, graph projection, and interpreter state |
| `program/interpreter.rs` | Lifecycle phases, workflow calls, authentication, and HTTP execution |
| `program/values.rs` | Scalar resolution, templates, and invocation storage budgets |
| `trace.rs` | Retained event accounting and cancellation-aware live delivery |
| `budget.rs` | JSON byte measurement without temporary payload buffers |
| `control.rs` | Shared cancellation signals and stop-cause priority |
| `extract.rs` and `extract/json.rs` | Bounded extraction with read-only JSON lookup |
| `secrets.rs` | Credential resolution and non-serializable secret storage |
| `redaction.rs` and `security.rs` | Sanitized snapshots, request/response previews, and observations |

## Rust ownership and cancellation

The interpreter owns all mutable session and invocation state. `Arc<Program>`
shares immutable compiled definitions; cloning an `Arc` increments its reference
count rather than copying node configurations. A local handle lets a method borrow
a node while mutating other runtime state across an `await`.

Invocation frames own their resolved values. Source lookup borrows values until a
request, call input, or output needs an owned copy. Compiled last-use information
releases values and retained responses as execution advances.

Cloned cancellation tokens signal the same cancellation state. User cancellation
may still permit bounded cleanup with a separate token. Owner shutdown, receiver
loss, and safety limits prevent or interrupt cleanup. The strongest recorded stop
cause wins even when multiple tasks request cancellation concurrently.

`tokio::select!` drops futures for branches that lose the race. Trace delivery first
reserves channel space so a cancelled delivery attempt does not consume the event
needed by the retained report. HTTP execution similarly races shutdown and cleanup
deadlines, without blocking a Tokio worker thread.

## Documentation and checks

`///` comments describe items and appear in rust-analyzer hover information.
`//!` comments describe modules; ordinary `//` comments explain implementation
decisions. Public documentation and intra-doc links are compiler-enforced.

```sh
cargo test -p interl-engine --all-features --locked --offline
cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
cargo doc -p interl-engine --no-deps --document-private-items --locked --offline
```

Integration tests start localhost HTTP fixtures. Production code denies Clippy's
`unwrap_used`, `expect_used`, and `panic` lints; test assertions may unwrap fixtures.
