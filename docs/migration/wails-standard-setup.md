# Standard Wails setup correction

> Historical migration evidence. Migration and packaged macOS acceptance are complete;
> the old runtime was removed on 2026-10-30. Use the root README for current commands.

2026-10-28. This replaces the custom Node coordinator, root npm wrapper, manual
bundle creation, and startup smoke hooks introduced during the first shell spike.
The earlier Phase 1 journal remains historical evidence, not startup instructions.

## Reported failure and correction

The supplied macOS crash report shows `SIGKILL (Code Signature Invalid)` for the
old manually copied `InterleavingGo.app`. It does not establish an engine panic.
The old build coordinator did not use Wails' generated fresh-bundle and signing
workflow. A previous successful smoke launch did not establish reliable later
launches; that earlier claim was too broad.

The correction uses **Wails v3.0.0's own generated project tasks**, not another
project-specific launcher or a manual codesign workaround:

1. Installed the pinned official CLI with `go install`.
2. Generated a reference project with `wails3 init -t react -n interleaving-go`
   and the product identifier `dev.interleaving.go`, in a temporary directory.
3. Adopted its `Taskfile.yml` and `build/` files. Kept the generated macOS build,
   development bundle, packaging, prepare-output and codesign/verification tasks.
4. Adjusted only shared frontend paths/scripts/bindings and project metadata.
   The existing React tree remains in `apps/desktop`; there is no frontend copy.
5. Moved the desktop composition to standard `go/main.go`, using `//go:embed
   all:frontend/dist` and `application.AssetFileServerFS`, matching the scaffold.
6. Added the official Wails Vite plugin for the Wails target. Development uses
   the CLI-provided port (default 9245), live frontend server and HMR.
7. Added the `wails` npm alias in `apps/desktop/package.json`, invoking the
   official CLI in `go/`. The alias adds the normal Go tool directory to the child
   PATH, without modifying the user's global shell configuration.
8. Removed the custom Node coordinator, root `package.json`, custom asset package,
   old composition directory, startup probe hooks and stale custom app output.
   The logger port, Charm adapter, domain/application boundaries, tests and normal
   bounded diagnostic probe remain intact.

## Commands from repository root

```sh
(set -a && source ./example.env && npm --prefix apps/desktop run wails -- dev)
npm --prefix apps/desktop run wails -- build
npm --prefix apps/desktop run wails -- package
```

Wails owns process supervision, live reload and signed native bundles. No helper
creates/copies/signs an app independently. The generated tasks create fresh bundles
and perform ad-hoc signing and strict verification. Distribution notarization and
Developer ID signing remain separate release work.

## Verification

- `npm --prefix apps/desktop run wails -- package` completed with the generated
  macOS prepare-output, fresh bundle, ad-hoc signing and strict verification tasks.
  A separate `codesign --verify --deep --strict` check also passed.
- The exact environment-loaded development command above built and launched
  `interleaving-go.dev.app`. Its signature passed strict verification, the native
  process stayed running, and Vite on port 9245 returned HTTP 200.
- Eleven targeted frontend adapter/theme tests, `go test ./...`, `go vet ./...`,
  the frontend architecture check, and the original Tauri-target frontend build passed.
- UI acceptance testing is left to the user, as requested. No screenshot-based
  verification is claimed. Development logs also contain React Flow prop warnings;
  these are separate from the corrected native signing/startup path.

The old native smoke is removed and is not used as evidence for this correction.
Interactive dialogs and full Go engine parity remain outside the completed shell
functionality.
