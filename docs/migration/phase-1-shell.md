# Phase 1 — Hexagonal skeleton and Wails shell

> Historical migration evidence. Migration and packaged macOS acceptance are complete;
> the old runtime was removed on 2026-10-30. Use the root README for current commands.

**Superseded startup/build instructions:** use the [standard Wails setup](wails-standard-setup.md).
The custom launcher and root npm wrappers below are historical and have been removed.

Date: 2026-10-28. Work follows the [parallel migration plan](../../plans/go-wails-migration.md).
Rust/Tauri and its scenarios/report protocols remain the reference. No Go engine
parity or completed migration is claimed by this shell spike.

## Work journal

1. Verified the user's Homebrew upgrade: installed Go is now **1.27.3**. Used normal
   `go mod`, `go get`, `go run`, `go build`, `go test`, and `go vet` commands in `go/`.
   No global toolchain defaults were changed by the agent. Network/cache access
   needed sandbox permission; that was distinct from installing another Go version.
2. Created a separate Go module and pinned Wails v3.0.0. Kept Wails initialization
   and native types in the desktop composition root/inbound adapter.
3. Added pure capability/error types, application-owned driving and dependency
   ports, a bounded synthetic bridge-probe use case, system clock/ID adapters,
   shared bootstrap, and a headless CLI. Unported `validate`/`run` return unavailable.
4. Applied the user's logging requirement: introduced `application.Logger`, owned
   level/field types, and a Charm Log adapter pinned to **v2.0.1**. The upstream
   repository now recommends module path `charm.land/log/v2`. Injection is explicit
   through the bootstrap into the core; no package-global logger is used. Wails'
   framework-only slog logger is also backed by the Charm adapter.
5. Introduced the frontend `DesktopHost` port and Tauri/Wails/browser adapters.
   Preserved the old `lib/tauri.ts` export facade while moving actual SDK imports
   behind adapters. App close handling and native theme calls now use the host port.
   Existing Tauri themes/layout keys stay intact; Wails keys are separately namespaced.
6. Generated typed Wails service bindings with the pinned CLI. Added native dialog
   methods, Cocoa public-API theme switching in the macOS adapter, close readiness/
   reference counting and disposal, and bounded synthetic probe methods. No Rust
   subprocess, credential resolver or HTTP execution is involved in the Go app.
7. Added a shared-frontend build target, separate port 1422, and a build/run helper
   that embeds assets and creates a distinct unsigned macOS bundle. Added an opt-in
   native smoke which opens the actual webview, checks the shared canvas/bindings,
   probes native theme/event integration and closes automatically.
8. Added architecture tests, CLI diagnostic/stdout tests, real/fake dependency
   replacement tests, logging-adapter tests and frontend Wails contract tests.
   Fixed a regression introduced during extraction: Tauri theme application must
   remain immediately observable; the adapter now owns its window API import.
9. Verified the native smoke returns `{"nativeSmoke":true}` and exits 0 on macOS
   27.0.1/arm64. Go tests/vet and both frontend targets are checked separately.

## Hexagonal boundaries implemented

- Domain imports only ordinary value/error facilities. Application imports domain
  and consumer-owned ports, not any concrete native/HTTP/logger adapter.
- The logging port has Debug/Info/Warn/Error levels, safe structured string fields,
  and context; it has no Fatal method, global logger, native object, or run-event
  function. Normal diagnostic output is stderr; synthetic JSON remains stdout.
- `charmlog` implements the port, owns output/format configuration, and provides
  the Wails slog bridge at the outer edge. Its known-key omission is defense in
  depth, not a replacement for the future engine's sensitivity/redaction policy.
- `bootstrap` chooses system adapters for CLI and desktop. Each composition root
  chooses the logger. Core tests substitute deterministic clocks/IDs and recording
  loggers; the same probe contract also runs through real system adapters.
- Go architecture tests parse imports/ambient time calls and inspect the CLI's
  transitive dependency graph. The CLI has no Wails, embedded UI or CGO dependency.
- Frontend import checks reject native SDK/generated-binding imports outside the
  desktop adapters. The temporary compatibility facade contains no native API calls.

## Verification and limits

- Wails module/build pin: v3.0.0, Go 1.27.3, current module checksums committed in
  `go/go.sum`; frontend runtime pinned to matching `@wailsio/runtime` 3.0.0.
- Native build: `node scripts/go-desktop.mjs build` succeeds and creates a separate
  `InterleavingGo.app` (`dev.interleaving.go`), embedding the shared Wails React build.
- Native smoke: actual separate bundle binary with `--smoke`, exit 0 and success
  JSON; shared canvas mounted, typed probe plus native stream received, dark/light/
  system theme calls completed. No screenshot/visual correctness claim.
- `go test ./...`, `go vet ./...`, CLI status/probe and import boundary checks pass.
- Frontend Wails adapter, Tauri theme and Tauri save tests pass together (13 tests).
- Full frontend regression: **249 tests in 43 files pass**. The initial theme-timing
  failure is superseded by the complete rerun and the focused bridge/theme tests.
- Go regression: **8 tests pass**, including the dependency graph and logger rules;
  `go vet ./...` passes. The final alias-aware architecture check also passes.
- Default Tauri frontend build and Wails frontend build pass. Both SDK integrations
  retain typed boundaries; no engine/report/schema generation changes were needed.
- The offline reference snapshot still verifies **23 validation outcomes and 12
  source fingerprints**; Rust's scenario/report contract was not recaptured.
- Native smoke was repeated against the final shared frontend and checks backend
  capability flags are false for validation/execution before probing. It again
  reports success and exits 0.
- Source formatting, frontend host import rules and documentation links pass.
- Original Tauri native command `npm --prefix apps/desktop run tauri -- build --debug --bundles app` passes and produces its original `target/debug/bundle/macos/Interleaving.app`, separately from the Go app. Existing bundle-identifier/chunk advisories are not migration failures.

**Interactive acceptance still pending:** native Open/Save dialog selection and
cancellation, dirty-draft close interaction/focus, side-by-side manual inspection,
and any non-macOS native build. Signing, notarization and distribution packaging
remain Phase 6. Automated probes do not establish these manual acceptance items.

**Phase 1 status:** code and automated native shell spike implemented; interactive
native acceptance items above remain open. Do not mark the complete Phase 1 native
acceptance gate passed solely from this automated smoke. Phase 2 is the scenario/
FEEL port; it has not started.

### Root command shortcuts

Added a scripts-only root `package.json`: `npm run wails` launches the shell;
`wails:build`, `wails:bindings` and `wails:smoke` expose the existing coordinator
actions. No dependency relocation or root installation is required. Verified
script discovery with `npm run`; native behavior remains the previously tested helper.
