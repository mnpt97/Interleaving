# Example scenarios

All valid examples use schema v1 and one group/count 1. JSON and YAML pairs describe the same executable workflow. `.interl.yml` is also accepted. Validation is offline and requires neither a server nor credentials.

| Files | Workflow |
|---|---|
| `hello-api.json`, `hello-api.interl.json`, `hello-api.interl.yaml` | Anonymous GET `/health` |
| `http-workflow.interl.json`, `http-workflow.interl.yaml` | Anonymous GET/POST HTTP workflow |
| `auth-login.interl.json`, `auth-login.interl.yaml` | POST login → extract token → set Authorization → GET `/auth/me` |
| `auth-cookie.interl.json`, `auth-cookie.interl.yaml` | POST login → GET `/auth/cookie-me` using the same user's cookie jar; no extraction/session-header step |
| `invalid-json.interl.json` | **Intentionally invalid:** trailing comma; validation must fail with exit 2 |

```bash
cargo run --locked -p interl-cli --bin interl -- validate scenarios/examples/auth-login.interl.yaml
cargo run --locked -p interl-cli --bin interl -- validate scenarios/examples/auth-cookie.interl.json
```

For execution, start the [loopback test server](../../docs/test-server.md) in a separate terminal. In the client terminal, supply only its fake credentials:

```bash
export INTERL_ALICE_USERNAME=alice
export INTERL_ALICE_PASSWORD=alice-password
cargo run --locked -p interl-cli --bin interl -- run scenarios/examples/auth-login.interl.yaml --output /tmp/interl-bearer.json
cargo run --locked -p interl-cli --bin interl -- run scenarios/examples/auth-cookie.interl.json --output /tmp/interl-cookie.json
```

Both auth workflows should finish with two HTTP 200 responses. Reports omit sensitive bodies/header values, including the server's user/session JSON; status and timing remain visible. `passed` means workflow completion, not an assertion that every HTTP response succeeded. An invalid password yields HTTP 401; bearer extraction then fails, whereas cookie-only execution completes with unauthorized responses until assertions are implemented.

Scenarios contain references, never resolved credentials. To try another fake identity without changing the files, set the same two variables to `bob` and `bob-password`; identity ID `alice` is a stable scenario label, not a username assertion. Static bearer identity references are supported and tested, but the local server issues fresh tokens at login, so the reproducible examples use login rather than a hard-coded token.

See [schema v1](../../docs/scenario-v1.md), [M3 contracts and limits](../../docs/milestone-3.md), and [acceptance checks](../../docs/m3-acceptance.md). Do not include the intentional invalid fixture in a valid-example corpus.
