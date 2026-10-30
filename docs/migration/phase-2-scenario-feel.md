# Phase 2 — Scenario and FEEL compatibility

> Historical migration evidence. Migration and packaged macOS acceptance are complete;
> the old runtime was removed on 2026-10-30. Use the root README for current commands.

2026-10-28. Implementation and compatibility checks for the parallel Go runtime.
The Rust engine remains the execution reference. This phase adds offline Go
validation and compilation; it does not add a Go HTTP engine or enable desktop
scenario actions. Interactive Wails acceptance remains with the user.

## Implemented boundaries

- Domain: scenario/workflow/node/source/port types, safe diagnostics, structured
  DAG and inferred gateway-pair checks, all-path dataflow, call/lifecycle mappings,
  typed defaults/outputs, bounded call summaries, and credential descriptors.
- Application: injected `ScenarioCodec`, `ScenarioFiles`, `Expressions`, and
  `TargetURLs` ports. `Validate`, `ValidateFile`, and `Compile` share one pipeline.
  Successful compilation captures an independent document snapshot plus ordering,
  dominators, predicates, extraction paths, value/response lifetimes, conservative
  capture flags, reachable workflows, normalized targets, and secret references.
- Driven adapters: duplicate-aware JSON/restricted YAML, embedded shared JSON
  Schema, bounded local files, pbinitiative FEEL parser/evaluator, and WHATWG URL admission.
  No library types escape into domain/application. Existing logger port/Charm
  adapter remain unchanged.
- CLI: `validate <scenario>` returns 0/success on stdout or 2/diagnostics on
  stderr, including category/code/JSON Pointer and available location information.
  `status` reports validation true and execution false. `run` remains unavailable.
- Desktop: its existing availability gate stays disabled until the desktop
  integration phase. No UI acceptance is inferred from these tests.

JSON uses token-level duplicate/depth/count checks and `json.Number`, preserving
the distinction between integer and floating/exponent representations. Explicit
null defaults remain distinct from omitted defaults. Isolated UTF-16 escapes are
rejected before Go can replace them. Diagnostics retain no raw parser errors or
payload values. Source and expression locations have separate fields.

Both readers enforce 1 MiB source, depth 64 (hard cap 128), 10,000 entries per
collection and 100,000 values. The YAML adapter uses `go.yaml.in/yaml/v3 v3.0.4`
syntax nodes, without YAML unmarshaling/coercion or alias expansion. The library
constructs its syntax tree within the source-byte bound before the adapter's
depth/count walk; this is not an event-stream memory-budget guarantee. Anchors,
aliases, explicit tags, merges, nonstring keys, multiple documents and ambiguous
plain scalars are rejected. Further memory tuning should use measurement rather
than claiming that syntax-node allocation is charged incrementally.

The embedded codec schema is an exact copy of `schemas/scenario.schema.json`;
a regression checks byte equality. To refresh it after an intentional contract
change, copy that file to
`go/internal/adapters/outbound/scenariocodec/scenario.schema.json`. The adapter
uses `github.com/santhosh-tekuri/jsonschema/v6 v6.0.3`; all references in this
contract are local. Validation never resolves process credentials or sends HTTP.

## FEEL adapter decision — external evaluation (2026-10-29)

`github.com/pbinitiative/feel v1.0.6` now owns scanning, parsing **and runtime
operator evaluation** behind `go/internal/adapters/outbound/feel`. The internal
`expr`/`operand` tree and recursive evaluator have been removed. Evaluation calls
`library.Node.Eval` once with a fresh interpreter and private scope. The library
also owns property lookup, comparison and short-circuit decisions. No Rust
production fallback, local comparison code, or local property-path evaluator remains.

The adapter retains only project boundary work:

- Admission limits and an AST allowlist; static type constraints still validate
  declared workflow inputs. Unsupported syntax is not enabled by changing libraries.
- Dependency metadata and external AST handles kept private behind domain interfaces.
- Bounded JSON-to-library context/number conversion and safe error classification.
  No caller-supplied host objects/functions enter the interpreter.
- Predicate results must be boolean. Extraction/token results must fit the existing
  string/boolean/safe-integer port types. Integer output conversion checks exactness
  using the library's numeric comparison, without decimal formatting/rounding.
- Cancellation checks during context admission and before/after `Node.Eval`.
  The library has no context hook inside evaluation. AST/payload bounds limit that
  work; no detached worker is started to simulate interruption.

`Expressions.CompileExtractionValue` supplies reusable external evaluation handles
for named extractions and bearer-token paths. Compiled flows retain these handles
as `ExtractionValues` and `AuthTokens`. The older `Extractions` path slices remain
metadata only. Phase 3 must use the handles for evaluation. Library types never
cross the application/domain port.

Context admission permits plain JSON data only and bounds it to depth 64,
100,000 values, 10,000 entries per collection and 1 MiB of aggregate key/string/
numeric data. Only referenced inputs are included, and body data is excluded from
status-only predicates. JSON numbers are converted to the library's numeric type
without first rounding through float64. Typed workflow integer ports still retain
the existing safe integer bounds.

Sources: [library](https://github.com/pbinitiative/feel),
[upstream](https://github.com/superisaac/FEEL.go).

### Runtime differences from Rust

The Go implementation now deliberately uses the pinned library's runtime behavior.
It does **not** claim identical results to the old strict Rust evaluator in every
case. `tests/compatibility/feel-library-outcomes.json` records nine exact cases,
with the original Rust expectation, the library outcome, expression and reason.
Go tests verify that each entry still matches its Rust reference before applying
that specific library expectation. The frozen Phase 0 corpus is unchanged.

| Behavior | Pinned library outcome | Rust reference |
| --- | --- | --- |
| Null response field compared with `true` | false | unavailable error |
| Boolean ordering | ordered (`false < true`) | type error |
| Response number `1.0` compared with `1` | equal | type error |
| Large response integer compared with `1` | ordinary numeric comparison | type error |
| Mixed-type `!=` | false | type error |
| Numeric/string operands to `and` | library truthiness | type error |
| Null operand to `and true` | true in this pinned library | unavailable error |

These are observations of v1.0.6, not a claim of full DMN FEEL conformance. The
showcase uses explicit boolean comparisons and continues to validate in both
languages. Static input typing remains in the scenario compiler; typed extraction
outputs still reject fractions, null, structured/oversized values and unsafe integers.

### Parsing/literal adaptation

Rust's parser uses a synthetic scope to disambiguate names. The selected Go parser
has no scope parameter and can parse `inputs.minimum and inputs.label` as a single
qualified name. The adapter groups admitted ASCII property tokens before parsing
and joins an admitted whitespace-separated minus/integer pair. Library string
nodes are JSON-decoded once and bound as literal variables because its string
helper decodes only some JSON escapes. Resolving those variables and evaluating
the expression remain the external interpreter's responsibility.

The shared corpus now contains 84 cases (68 predicates, 16 extraction paths).
Both implementations execute the same inputs: Rust checks reference expectations,
and Go checks library outcomes with the nine explicit differences above. Additional
Go tests compare results directly with the external interpreter and verify delegate
invocation, concurrent scope isolation, cancellation, safe errors and extraction.

## URL compatibility decision

Differential cases exposed mismatches with Go `net/url`: Rust normalizes empty
userinfo, backslashes, encoded hosts, shorthand HTTP URLs and surrounding spaces;
Go's standard parser rejected those, while accepting an out-of-range port.
Use `github.com/nlnwa/whatwg-url v0.6.2` behind `domain.TargetURLs`, matching Rust's
WHATWG URL semantics. The adapter rejects credentials and query/fragment presence
(including empty delimiters), and returns a canonical URL without raw parser errors.
Sources: [project](https://github.com/nlnwa/whatwg-url),
[API](https://pkg.go.dev/github.com/nlnwa/whatwg-url/url).

## Reproduce checks

From `go/`:

```sh
go test ./...
go vet ./...
go build -o bin/interl-go ./cmd/interl-go
./bin/interl-go validate ../scenarios/examples/feature-showcase.interl.json
./bin/interl-go validate ../scenarios/examples/feature-showcase.interl.yaml
```

From the repository root:

```sh
cargo build --offline --locked -p interl-cli --bin interl
cargo test --offline --locked -p interl-core --test migration_contract
node scripts/capture-migration-baseline.mjs --check
node scripts/check-go-validation-parity.mjs
```

The differential script launches the two validators independently against temporary
local fixtures. This is test infrastructure, never a production Rust fallback.
`--capture-rust` explicitly captures expectations only for the separate extended
scenario corpus; it does not rewrite the frozen Phase 0 snapshot. Normal checks
compare against the committed expectation and both implementations. No credential
environment or network server is required.

## Verification status

- Go test suite and `go vet ./...` passed, including architecture/import checks.
- All 23 frozen CLI cases and 12 reference fingerprints remain unchanged and pass.
- All 119 extended scenario cases match the current Rust CLI and committed
  category/code/path/exit/output expectations; Go also checks them independently.
- The initial 79-case FEEL corpus matched the former internal evaluator. After
  external evaluation replacement, 84 shared cases run with explicit library
  outcome differences; this is not an unqualified Rust runtime parity claim.
- The feature showcase validates in JSON and YAML through the real Go CLI.
  Fake-port tests check replacement, cancellation, independent plan snapshots,
  inferred gateway pairing, typed extraction and secret-descriptor reachability.
- Short two-worker fuzz runs passed: 11,447 codec inputs and 44,555 expression
  inputs. These are bounded smoke fuzz runs, not exhaustive parser proofs.
- After external evaluation replacement, the complete Go suite/vet and the adapter
  race checks passed. A further two-worker runtime fuzz run passed 90,633 inputs.
  Tests exercise the library delegate directly and run prepared showcase extraction
  and bearer-token handles; no internal evaluator is used in those checks.
- `CGO_ENABLED=0 go build` produced a working headless CLI; status reports
  validation true/execution false. The CLI imports no Wails/native dependencies.
- Rust migration tests and formatting checks passed; existing Rust production
  semantics and the showcase JSON/YAML were not changed. Exercise guidance now
  includes Go validation commands. No new persisted scenario feature was invented.

The external-evaluation replacement is part of Phase 2. Its documented runtime
differences supersede the earlier exact-evaluation-parity claim. Next is Phase 3's
HTTP execution vertical slice. No scenario execution or desktop
parity is claimed by this phase, and the user's interactive Wails acceptance is
still tracked separately.
