# Project maintenance

When adding or changing a supported scenario, editor, or execution feature,
extend the living feature showcase in the same change:

- `scenarios/examples/feature-showcase.interl.json` and its equivalent YAML file.
- The feature checklist and UI exercise guidance in `scenarios/examples/README.md`.
- The showcase regression in `crates/interl-engine/tests/reusable.rs` when behavior changes.

Keep the example runnable against the local test server with fake credentials.
For UI-only features, update the exercise guidance without inventing persisted
fields. Follow `scenarios/examples/AGENTS.md` for showcase-specific details.

Keep interface copy focused on labels, actions, current values, and actionable
errors. Avoid persistent explanations of implementation details or instructions
that repeat what the controls already communicate.
