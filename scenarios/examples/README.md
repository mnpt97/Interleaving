# Example scenarios

All valid examples use schema v2 with one once workload/VU/iteration. JSON/YAML pairs describe the same executable workflow; `.interl.yml` is accepted. Validation is offline and requires neither a server nor credentials. V1 is unsupported.

VS Code associates scenario JSON and YAML with the generated schema in
`schemas/scenario.schema.json`. YAML examples also include an explicit schema
comment to avoid unrelated schema detection. If old property errors remain after
updating, run **Developer: Reload Window** from the Command Palette.

| Files                                                              | Workflow                                                                                                                                                         |
| ------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `feature-showcase.interl.json`, `feature-showcase.interl.yaml`     | Living showcase of all supported scenario features; start here for a complete example                                                                            |
| `hello-api.json`, `hello-api.interl.json`, `hello-api.interl.yaml` | Anonymous GET `/health`                                                                                                                                          |
| `http-workflow.interl.json`, `http-workflow.interl.yaml`           | Anonymous GET/POST HTTP workflow                                                                                                                                 |
| `auth-login.interl.json`, `auth-login.interl.yaml`                 | POST login → extract token → set Authorization → GET `/auth/me`                                                                                                  |
| `auth-cookie.interl.json`, `auth-cookie.interl.yaml`               | POST login → GET `/auth/cookie-me` using the same user's cookie jar; no extraction/session-header step                                                           |
| `reusable-auth.interl.json`, `reusable-auth.interl.yaml`           | V2 initialize login → token export/input → set session → reusable account-inspection call; CLI/desktop execution; current canvas displays the iteration workflow |
| `invalid-json.interl.json`                                         | **Intentionally invalid:** trailing comma; validation must fail with exit 2                                                                                      |

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

See [schema v2](../../docs/scenario-v2.md), [M3 contracts and limits](../../docs/milestone-3.md), and [acceptance checks](../../docs/m3-acceptance.md). Do not include the intentional invalid fixture in a valid-example corpus.

See [M3.5 progress](../../docs/milestone-3.5.md) for reusable-workflow commands, report v3, and the current v2 desktop-view boundary.

The `exclusive-routing.interl.json` and `.yaml` pair demonstrates a structured exclusive split by `/health` response status: a 2xx response takes `/echo`, otherwise the default arm requests `/health`, then the selected route rejoins. These are conditional routes rather than response assertions.

`body-routing.interl.json` / `.yaml` use the local test server’s `/routing-body` endpoint and branch on `response.body.value.status = true`.

## Living feature showcase

Open [Feature showcase](feature-showcase.interl.json) in the desktop, or use its
[YAML equivalent](feature-showcase.interl.yaml). Start the local test server and
set the two fake credential variables shown above, then run:

```bash
cargo run --locked -p interl-cli --bin interl -- validate scenarios/examples/feature-showcase.interl.json
cargo run --locked -p interl-cli --bin interl -- run scenarios/examples/feature-showcase.interl.yaml --output /tmp/interl-showcase.json
```

The journey initializes bearer authentication, verifies it, replaces it with
cookie authentication, and verifies that session. Iteration calls a reusable
health workflow, routes on its typed results and the response body, sends a
templated request, removes a temporary session header, and sends literal text.
Cleanup removes headers and checks health. The normal run takes the ready arm;
change the iteration's `minimum` binding from `1` to `4` to take the default arm.
Both routes finish with twelve HTTP 200 responses against the local server.
Three POST `/long-poll` nodes pause before the health call, inside that
subworkflow, and after the branch rejoins. Change scenario variable `waitMs`
(default `2000`) to control their request-body `timeoutMs`. The default adds
about six seconds so active execution markers are easy to follow. The endpoint
accepts 0–60000 ms; keep the profile's HTTP timeout above the requested wait.

Alternatively, load the fake credentials from the repository root with
`set -a; source ./example.env; set +a`. Restart Tauri from that same terminal using
`npm --prefix apps/desktop run tauri -- dev` so it inherits the variables.

Feature checklist:

- One workload/profile/assigned identity; initialize → iteration → bounded cleanup.
- Native bearer and cookie Authentication with environment references and session replacement.
- Profile URL/header/timeout inheritance, per-node URL/timeout overrides, request headers and query strings.
- GET/POST requests; literal JSON, literal text, and nested object/array JSON templates.
- Configurable long-poll waits in the main journey and a nested workflow, driven by the shared `waitMs` variable.
- Scenario variables, identity fields, literals, workflow inputs/defaults, lifecycle exports, call outputs, and response-status sources.
- Reusable workflow invocation with typed input/output mappings; one Extract node publishes string, integer, and boolean outputs atomically.
- Session header set/remove operations, shared across workflow calls and lifecycle phases.
- Exclusive split/default/merge, inferred pairing, and bounded FEEL using response status/body and explicit typed inputs.
- Optional node names, workflow-local IDs, canvas positions, and edge attachment metadata.

In the desktop, use this scenario to exercise workflow navigation/call previews,
View/Run versus Edit, inspector sections, node changes, manual connections,
10 px snapping, rename/reference updates, undo/redo, validation, Save/Discard,
resizable panels, tooltips, themes, cancellation, and safe report export. UI state
is not persisted as invented scenario fields.

Run the showcase and use the sidebar to inspect initialize, iteration, cleanup,
and the nested health call. The captured canvas retains node names, positions,
and top/bottom gateway connections. Only the selected branch executes; the other
arm is marked skipped. Expand a node's single log row for its ordered events.
Use **View scenario** / **View run** in the Run sheet to change canvases; edits
must not change the captured result. Reduced motion keeps static progress.

On a fresh launch, verify the lifecycle canvas remains visible after the scenario
loads. Check all three phases and the Run canvas; repeated local node IDs across
phases must stay distinct and their connections must remain anchored.
Initialize, Iteration, and Cleanup stack top to bottom; each workflow runs left
to right inside its box. Check this layout in both the editor and captured Run view.
Reload with the showcase selected: the initial zoom should fit all three boxes,
Start, and End. Manual zoom should remain unchanged when selecting or moving nodes.
In Edit, resize each phase with its bottom-right handle; dimensions remain fixed
while moving nodes and are remembered locally after reload. Add an HTTP Request
with an empty path, drag it within its phase, then drop it into Initialize or
Cleanup (or use **Move to phase**). Moving does not create connections. Move
dependent nodes together with **Move to phase**; blocked moves leave the graph intact.
Drag a phase header to move the entire box and its contents; its position is also
remembered locally. Drag a request near the entry/exit row: its center should snap
onto the circle centers, allowing straight left-to-right connections.
The left sidebar groups steps into Initialize, Iteration, and Cleanup accordions,
with Iteration expanded initially. Expand phases independently and select steps
inside them; check keyboard navigation and the same grouping in the Run sidebar.
Default phase boxes fit their own content: the showcase's shorter Initialize and
Cleanup phases should be narrower than Iteration in both scenario and Run views.
Manual sizes still survive reload; untouched boxes use the compact defaults.
Default boxes share a vertical centerline, including Start and End. Check that
narrower phases are centered beneath Iteration; manually moved boxes keep their positions.

The scenario selector and top-right theme selector use styled shadcn Select
controls. Check arrow-key selection, Escape/focus return, and System/Light/Dark
appearance. Scenario switching stays disabled while a run is active.

Check that the workspace and inspectors show labels and current values without
repeated implementation explanations. Credential inputs name their environment
variables; Run shows a short payload-hidden reason when applicable. Validation
errors, effective URLs, and unsupported-feature states remain visible where needed.

Open **Read typed health output** to inspect the single **Extract readiness
fields** node and its three named outputs. Edit adds/removes output rows with a
property expression and type for each. Call workflow nodes use the workflow-diagram
icon to distinguish them from HTTP requests.

**Maintenance rule:** extend both showcase formats and this checklist whenever a
supported feature is added. Keep one runnable journey and extend its runtime
regression in `crates/interl-engine/tests/reusable.rs`. The regression checks
JSON/YAML equivalence, actual execution, route selection, cleanup, and safe reports;
the frontend contract suite also validates the JSON through the generated schema.
See [the contributor instructions](AGENTS.md).
