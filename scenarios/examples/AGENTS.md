# Living feature showcase

Whenever a supported scenario, editor, or execution feature is added or changed,
extend `feature-showcase.interl.json` and its equivalent YAML file in the same
change. Keep one coherent runnable journey; do not add unsupported placeholders.
Update the feature checklist in `README.md` and the showcase runtime regression
in `go/internal/adapters/inbound/testserver/handler_test.go` when expected behavior changes.

The showcase runs against the existing local test server using fake credentials
through environment references. Keep JSON and YAML semantically identical,
preserve readable canvas layouts, and validate both formats. Extend the local
fixture when a new supported feature needs an endpoint. Runtime values and real
credentials must never be committed.

Keep the YAML schema modeline pointing to `../../schemas/scenario.schema.json`
at the top of each example so editors use the current contract.

For UI-only features, record how to exercise them with this scenario in the
README; do not invent persisted schema fields for transient UI state.
