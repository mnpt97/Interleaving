# Scenario schema v1 — M1/M2 supported contract

Rust types and validation in `interl-core` are authoritative. JSON is canonical serialization; YAML is preferred for hand-authored scenarios. Both formats have identical semantics. `schemaVersion` is the integer `1`.

This page describes the operational M1/M2 execution contract. M3 Phase 2 adds typed auth shapes and static semantic validation described in [M3 schema additions](milestone-3.md); an engine capability guard prevents their execution until later phases. Existing scenario meanings remain unchanged.

## Document

Required fields: `schemaVersion`, `id`, `name`, `environment`, `clientProfiles`, `identities`, `groups`, `workflow`, and `assertions`. `layout` is optional and may be null. Unknown fields are rejected, except within arbitrary JSON payloads and environment values.

| Field | Shape and rules |
|---|---|
| `id`, `name` | Nonempty strings. IDs are strings, not generated UUIDs. |
| `environment` | Object with `variables`, a string-keyed JSON value map (defaults to `{}`). M2 path interpolation resolves these values; there is no expression evaluation. |
| `clientProfiles` | Nonempty array; each profile has unique nonempty `id`, absolute HTTP(S) `baseUrl` without embedded credentials, positive integer `timeoutMs`, and optional string-to-string `headers` (defaults to `{}`). |
| `identities`, `assertions` | Required empty arrays. Nonempty declarations remain unsupported through M2. |
| `groups` | Nonempty array; each group has unique nonempty `id`, resolving `clientProfileId`, positive `u32` `count`, and `identityAssignment`. Validation accepts declarations; M2 execution requires exactly one group with count 1. |
| `identityAssignment` | `{ "mode": "shared", "identityId": null }`. Omitted `identityId` also defaults to anonymous. Other modes and nonnull identities are unsupported. |
| `workflow` | Required `nodes` and `edges` arrays. See graph rules below. |
| `layout` | Optional `{ "positions": { "nodeId": { "x": number, "y": number } } }`. Positions default to `{}`; references must resolve and coordinates must be finite. |

Timeouts are unsigned 64-bit integers greater than zero. Negative, fractional, or out-of-range integer fields are schema errors. JSON arbitrary values support null, booleans, strings, arrays, objects, and numbers representable by the parser. NaN and infinity are unsupported.

## Nodes and edges

```json
{ "id": "start", "type": "start" }
{ "id": "request", "type": "http", "config": { "method": "GET", "path": "/health" } }
{ "id": "end", "type": "end" }
```

Only HTTP nodes accept `config`. All node IDs are unique. Edge shape is `{ "from": "start", "to": "request" }`; edges have no persisted editor ID.

HTTP configuration:

- Required `method`: `GET`, `POST`, `PUT`, `PATCH`, or `DELETE`.
- Required `path`: begins with one slash, not `//`, and contains no line breaks.
- Optional `headers`: string-to-string map, default `{}`. Names use HTTP token syntax; values contain no CR/LF.
- Optional `timeoutMs`: positive integer, absent/null means use the profile timeout.
- Optional `body`: absent/null, `{ "kind": "json", "value": <JSON value> }`, or `{ "kind": "text", "value": "text" }`.

M1 validation performs no execution, URL resolution, interpolation, FEEL checking, or secret resolution. M2 execution uses this same schema; see [the execution contract](milestone-2.md). There is no separate query field: query strings are part of `path`. Future fields must be introduced explicitly rather than ignored by older validators.

## Graph rules

Exactly one start, one or more sequential HTTP nodes, and exactly one end form one connected chain. Start has no incoming edge and one outgoing edge. End has one incoming edge and no outgoing edge. HTTP nodes have one of each.

Reject duplicate IDs/edges, dangling endpoints, self-loops, branching, joins, cycles (including disconnected cycles), and unreachable nodes. Compilation validates first and follows connectivity, independently of array order and layout. The immutable M1 execution plan contains only scenario/version identity and ordered workflow nodes; it is not a runnable session or resolved run configuration.

## Formats and limits

Recognized final extensions are `.json`, `.yaml`, and `.yml`, including recommended `.interl.*` names and legacy `hello-api.json`. Format detection does not sniff content. Files must be regular UTF-8 files.

Default limits: 1 MiB source bytes, depth 64, 10,000 entries per collection, and 100,000 aggregate values. `ScenarioLimits` configures parser/loader limits; effective maximum depth is capped at 128 for stack safety. Semantic diagnostics are capped at 100.

Duplicate keys are rejected recursively before map insertion, including request bodies and environment variables. JSON rejects trailing content. YAML accepts one document and rejects anchors, aliases, explicit tags (including standard tags), merge syntax, complex keys, and nonstring keys. This deliberately restricted YAML subset is stricter than general YAML.

Plain YAML `true`, `false`, and `null` retain their JSON types. Numbers use JSON-compatible syntax. Quote ambiguous scalar values such as `yes`, `on`, `01`, dates, or `.nan`; implicit empty/null values are rejected. Quoted values are strings. Mapping names such as the position key `y` are string keys; primitive numeric/boolean/null keys require quotes.

JSON output is deterministic, recursively orders object keys, and preserves array order and strings. This is not a promise of RFC 8785 canonicalization. YAML output and comment/format preservation are deferred. Desktop opening never overwrites the source; layout edits remain in memory until explicitly saved as a JSON copy through the native save dialog.

## Diagnostics and compatibility

Diagnostics contain `category`, stable `code`, `message`, `fieldPath`, optional `nodeId`, `filePath`, `line`, and `column`. Field paths are JSON Pointers; `$` denotes the root. Parser locations are one-based when available. Typed deserialization/semantic checks may provide a field path without a source location. Messages avoid including invalid values or complete source documents.

Load/parse errors stop deserialization. Semantic validation can return multiple deterministically ordered errors. CLI and desktop call the same core pipeline. `interl validate` exits `0` on success and `2` for invalid or unreadable input.

Future capabilities require an explicit compatibility/migration policy. Older implementations reject unsupported configurations; existing scenarios must not silently change meaning. FEEL will remain read-only, with explicit operations owning mutation and the Rust engine owning scheduling.


## M3 additive capabilities

The sections above describe the M1/M2 baseline. Current schema v1 also supports typed environment-reference identities, explicit `jsonTemplate` bodies, bounded read-only `extract` nodes, and explicit `setSession` header operations. See [the authoritative M3 additions](milestone-3.md#typed-schema-additions) for shapes, validation, limits, and compatibility. Assertions, pool/unique assignment, branching, and concurrency remain unsupported. Both formats share the same Rust schema; JSON/text literal meanings have not changed. Expression diagnostics add separate optional `expressionLine`/`expressionColumn`. Current reports use version 2, independently of scenario schemaVersion 1.
