# Phase 6 — macOS packaging and Wails decision

> Historical migration evidence. Migration and packaged macOS acceptance are complete;
> the old runtime was removed on 2026-10-30. Use the root README for current commands.

2026-10-30. **Wails + Go is the chosen primary implementation**, as explicitly
decided by the user. CLI performance comparisons and desktop performance profiling
are skipped by request. No relative performance claim is made. Rust/Tauri stays
available, including the current scenario-schema generator; removal is not authorized.

Automated package inspection passed. **Packaged interactive acceptance is pending
with the user.** No desktop UI was launched during these checks.

## Build and observed evidence

From the repository root:

```sh
npm --prefix apps/desktop run wails -- package
codesign --verify --deep --strict --verbose=2 go/bin/interleaving-go.app
plutil -lint go/bin/interleaving-go.app/Contents/Info.plist
file go/bin/interleaving-go.app/Contents/MacOS/interleaving-go
otool -L go/bin/interleaving-go.app/Contents/MacOS/interleaving-go
go version -m go/bin/interleaving-go.app/Contents/MacOS/interleaving-go
node scripts/check-migration-boundaries.mjs
```

- Used the existing official Wails v3.0.0 package task; no alternative launcher,
  custom bundling script, or runtime startup changes were introduced.
- Production frontend type checking and Vite build passed. Binding generation
  processed one service with fourteen methods. Go built the executable with
  `-tags production -trimpath -buildvcs=false -ldflags="-w -s"`.
- Output: `go/bin/interleaving-go.app`, bundle identifier `dev.interleaving.go`,
  version `0.1.0`, native macOS arm64 executable built with Go 1.27.3.
- Independent strict/deep signature verification passed: valid on disk and
  satisfies its designated requirement. The signature is **ad-hoc**, not Developer
  ID signing or notarization; distribution/Gatekeeper acceptance is not established.
- The plist is valid and its executable exists with execute permission. Dynamic
  library inspection lists system frameworks/libraries only, including WebKit;
  no repository-local dylib or Rust subprocess is required by the executable.
- A byte-for-byte check found all twelve generated frontend files embedded in
  the packaged executable. All four asset references in `index.html` resolve to
  generated files; the index contains no Vite development client. This checks
  packaging completeness; it does not substitute for rendering in the native app.
- Frontend host architecture checks passed. No execution/UI source was changed
  for this step, so existing engine/component suites were not rerun.
- Executable size: 15,473,008 bytes. SHA-256 for this build:
  `8e2b78502a584b420ea444ae2556a1a7c4c579cf62c951f01df2874e11a332b4`.

The first sandboxed build could not write Go's external build cache; the same
official package command succeeded with that filesystem access allowed.
The linker warns that a Go object targets macOS 13 while the generated task links
for macOS 12. This inspection does not establish macOS 12 compatibility; older-OS
support needs its own target/toolchain decision and native verification.

## User packaged-app checklist

Keep the local fixture running in its own terminal:

```sh
go -C go run ./cmd/interl-test-server
```

1. Stop Wails development and its Vite server with Ctrl-C; quit other copies of
   Interleaving Go. In Finder, open `go/bin/interleaving-go.app`. Confirm the editor
   remains visible and the initial canvas fits without a frontend server.
2. Open the anonymous Hello API example using the native dialog, run it against
   the fixture, and confirm completion. Cancel an Open dialog and confirm the
   current scenario stays selected. Open both JSON and YAML showcase variants.
3. Finder does not source `example.env`. Without credentials in the launch
   environment, the authenticated showcase should report the safe missing-secret
   error. Close the app before the credential-loaded launch below.
4. Launch the packaged executable from the repository root with fake credentials:

   ```sh
   (set -a && source ./example.env && ./go/bin/interleaving-go.app/Contents/MacOS/interleaving-go)
   ```

   This runs the release executable directly; it starts no Vite or Wails dev
   process. Environment variables are read at process startup and not stored in
   the package. Reopening through Finder does not preserve this shell environment.
5. Run the showcase: expect twelve HTTP 200 responses and completed cleanup on
   the unchanged example. Inspect grouped logs, nested calls, graph progress and
   the server's request durations. Cancel during a long poll; confirm cleanup,
   a cancelled report and the ability to run again.
6. Edit a copy: add/move an unfinished HTTP node, save to a new JSON file in a
   temporary folder, reopen it and repair its path. Check undo/redo, Save/Discard
   and switching modes. Keep the original showcase files unchanged.
7. Export a report, cancel an export dialog, and try an unwritable destination.
   Expect actionable errors, retained results and no credential values in the
   exported report. Confirm close protection with unsaved changes and on exit
   during a running request.
8. Check light/dark/system theme, canvas navigation, panel resizing and persisted
   layout. Close and relaunch the package to confirm it still opens normally.

Record each failure with the launch method, scenario and visible error. These
checks remain unobserved until the user reports results; the earlier "it works"
confirmation concerned request logging, not this packaged checklist.

## Remaining release work

Native package acceptance above, Developer ID signing/notarization when needed for
distribution, and native verification on any additional advertised platform remain
separate work. The Wails/Go choice is settled and the main README now uses it for
development and CLI commands. Rust removal and moving schema-generation authority
need an explicit follow-up decision.
