# Example scenarios

`hello-api.json`, `hello-api.interl.json`, and `hello-api.interl.yaml` describe the same scenario: one anonymous user and start → GET `/health` → end. The files include optional layout positions.

Validation does not contact `127.0.0.1:8080`; no server is needed. Execution is outside Milestone 1.

```bash
cargo run --locked -p interl-cli --bin interl -- validate scenarios/examples/hello-api.json
cargo run --locked -p interl-cli --bin interl -- validate scenarios/examples/hello-api.interl.yaml
```

The `.interl.yml` suffix is also accepted. See [the supported schema](../../docs/scenario-v1.md) for fields, limits, and YAML restrictions.
