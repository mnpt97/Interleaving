# Contract regression fixtures

These frozen scenario, expression and diagnostic cases protect the current Go
implementation. Their origin was the migration baseline; no removed runtime is
needed to execute the checks. `contract-baseline.json` records example hashes and
expected offline validation results. FEEL library outcome differences are explicit
in `feel-library-outcomes.json`; they are not implemented with a custom evaluator.

From the repository root:

```sh
go -C go test ./internal/compatibility -count=1
go -C go build -o bin/interl-go ./cmd/interl-go
node scripts/check-go-validation-parity.mjs
node scripts/check-go-run-parity.mjs
npm --prefix apps/desktop run schema:check
```

The validation harness checks Go against frozen expectations. The runtime harness
executes only the Go CLI against a local fake HTTP fixture and checks exit codes,
preparation isolation, reports, ordered events, wire-request counts, cancellation,
cleanup and absence of credential sentinels. It requires loopback-port access.
`tests/fixtures/editor` holds JSON/YAML fixtures used by frontend regressions.

`schemas/scenario.schema.json` is the canonical schema. The generator updates the
TypeScript file contract and the schema embedded by the Go codec; `--check` and Go
compatibility tests reject drift. Changes to the persisted contract require updates
to this schema, Go models/validation, generated consumers and regressions together.
