# interl-core

The shared scenario library for the CLI, desktop app, and execution engine. It
parses configuration and decides whether it is valid to run. It does not send HTTP
requests, read credentials, or execute workflows.

Import the public API directly from `interl_core`; there is no versioned module:

```rust
use interl_core::{compile_scenario, load_scenario, ScenarioDocument};
```

The module cleanup does not change scenario JSON/YAML. The existing
`schemaVersion` field remains a file-format check; it does not select a separate
Rust model or implementation.

## Where to start reading

| File/module | Responsibility |
| --- | --- |
| `src/lib.rs` | Public API, processing sequence, Rust conventions, executable example. |
| `src/model.rs` and `src/model/` | Canonical data types, grouped into common transport data, scenario/workflow ownership, value sources, and graph nodes. |
| `src/parse/` | Bounded, duplicate-aware JSON and restricted YAML readers, followed by strict typed deserialization. |
| `src/load.rs` | Extension-based format selection and bounded UTF-8 file reads. |
| `src/document.rs` | Editor inspection, original source/provenance, and incomplete-draft saving. |
| `src/compile.rs` | Semantic compilation pipeline and template admission. |
| `src/compile/references.rs` | Input/output scopes, producer order, types, and invocation bindings. |
| `src/compile/calls.rs` | Recursion checks, named call-cost summaries, and lifecycle reachability. |
| `src/compile/secrets.rs` | Credential-reference dependencies; values are resolved later by the engine. |
| `src/graph_validation.rs` | Shared declarations, linear connectivity, graph/layout constraints, and connected execution order. |
| `src/auth_validation.rs` | Extraction/session validation, capture sensitivity, and value lifetimes. |
| `src/program.rs` | Immutable compiled workflow/auth plans sharing one document snapshot. |
| `src/scalar.rs` | Borrowed scalar descriptors that avoid temporary owned copies during validation. |
| `src/expressions.rs` | The bounded FEEL property-access compiler. |
| `src/diagnostic.rs` | Structured safe diagnostics and standard Rust error conversions. |

## Rustdoc comments

`///` documents the next type, field, function, or implementation. `//!` documents
the containing module or crate. IDEs display these comments on hover, and Rustdoc
turns them into linked API pages. Code examples in Rust fences become doctests.

Comments explain the behavior, errors, assumptions, and Rust patterns used at
that location: `Result`/`?`, `Option`, borrowing and lifetimes, enum matching,
Serde derives and visitors, associated types, `Deref`, type aliases, and visibility.
Public documentation and intra-doc links are checked by crate-level lints.

Build browsable API documentation:

```bash
cargo doc -p interl-core --no-deps --open
```

Include private helpers when reading through compiler/parser implementation details:

```bash
cargo doc -p interl-core --no-deps --document-private-items --open
```

Run the tests and documentation examples:

```bash
cargo test -p interl-core --locked --offline
```

## Ownership and error-handling conventions

Compilation borrows the source while validating it. Only successful compilation
creates an owned document snapshot. `Program` and its workflow plans share that
immutable snapshot through `Arc`, so a cloned workflow remains valid after its
parent program is dropped and editor changes cannot affect an accepted run.

Validation visits borrowed scalar descriptors directly. It allocates strings only
when an owned result is needed, such as a retained value key or dependency. Public
helpers returning owned scalar descriptors keep their existing behavior.

Missing workflow/identity references return safe diagnostics. Clippy rejects
production `unwrap`, `expect`, and explicit `panic!` calls, and also checks for
redundant cloning and needless collection. Test assertions may unwrap setup data.
Expression compilation errors implement the standard Rust `Error` trait, so they
work with `?` and ordinary error chains.

```bash
cargo clippy -p interl-core --all-targets --all-features --locked --offline -- -D warnings
```
