# Migration complete — legacy runtime retirement

2026-10-30. The user confirmed the packaged macOS application works and explicitly
authorized removal of Rust/Tauri. Wails + Go is the sole implementation. Performance
comparisons were skipped by request; no comparative performance claim is made.

## Removed and retained

- Removed the Rust workspace/manifests, crates, Tauri project, native SDK/CLI npm
  dependencies, adapter and compatibility facade, Rust-only test fixtures, editor
  recommendations, project-local Rust skill and local Cargo build artifacts.
- React now imports the host-neutral desktop module directly. Production/default
  builds target Wails; browser preview is explicit. Existing Wails preference keys
  are unchanged, preserving saved theme and layout.
- Kept Wails' native file/dialog/stream tests. Theme and auth-loading tests now
  mock the host port. The former engine-owned editor fixtures live under
  `tests/fixtures/editor`; authored examples and execution semantics are unchanged.
- Made `schemas/scenario.schema.json` authoritative. The existing Node generator
  produces TypeScript types and the Go codec's embedded copy, with no Cargo call.
  `schema:check` and Go contract regressions enforce agreement.
- Preserved frozen regression expectations as `contract-baseline.json` and explicit
  FEEL library outcomes. Validation checks compare Go with those expectations;
  runtime checks execute only Go. Removed obsolete reference-capture tooling.
- Updated the README, design, Go guide, example guidance, VS Code configuration
  and maintenance instructions. Earlier migration journals remain historical
  evidence, explicitly marked as retired instructions.

The Go domain/application still depends on ports, not Wails or concrete adapters.
Desktop and CLI share the same application; Charm Log remains the logging adapter.
Global toolchains and the user's exported reports were not removed or modified.

## Verification

- Full Go suite passed with `go -C go test ./... -count=1 -timeout=180s`.
- `go -C go vet ./...` passed.
- Frontend: 248 tests passed in the initial complete run; one auth-loading suite
  needed its port mock completed. Its four tests then passed, covering all 252
  retained frontend tests across 41 files. Tauri-specific tests were removed;
  equivalent Wails protocol/dialog coverage remains.
- TypeScript, canonical-schema drift and frontend host-boundary checks passed.
- Go-only CLI: 119 extended validation cases and 90 runtime cases passed,
  including preparation isolation, wire counts, ordered events, cancellation,
  cleanup and credential-sentinel absence.
- The official `npm --prefix apps/desktop run wails -- package` task passed after
  removal, including production frontend/bindings, native build, ad-hoc signing
  and strict/deep signature verification. The packaged executable has the
  `production` build tag and contains the production asset names. The ordinary
  development watcher may regenerate `go/frontend/dist` independently; that
  directory is not evidence of the already-packaged binary's contents.
- No native UI was launched during retirement checks. User packaged acceptance
  preceded retirement; this step changes host/dependency wiring, not the UI UX.

The shell-selected Node 22 installation aborted when running npm. The already
installed Homebrew Node 24 ran the normal npm/Wails commands successfully with a
command-local PATH. No global installation or shell configuration was changed.
The existing macOS deployment-target linker warning remains documented in the
packaging journal; older OS support and distribution notarization are separate work.

## Next

Continue product work from M3.5/M4 plans. There is no remaining migration gate or
legacy runtime to maintain. Developer ID signing/notarization and native validation
of additional platforms remain distribution tasks, not blockers for this migration.
