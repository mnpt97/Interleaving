# Interleaving desktop

Tauri 2 with React/TypeScript, Vite, Tailwind CSS v4, a minimal shadcn/ui foundation, React Flow, Zustand, Zod, and React Hook Form.

From the repository root:

```bash
npm --prefix apps/desktop ci
npm --prefix apps/desktop run tauri -- dev
npm --prefix apps/desktop run build
npm --prefix apps/desktop run test
```

`npm --prefix apps/desktop run dev` provides a browser preview with native actions disabled. Scenario semantics are validated exclusively in Rust. The graph adapter stores only positions in `layout`; it never persists React Flow runtime objects.

The header's **Color theme** control offers System, Light, and Dark. The saved choice applies on startup; System follows OS appearance changes. The supplied palette lives in `src/styles/min.css`, with all five color scales exposed through `tailwind.config.js` (explicitly loaded by Tailwind v4). Semantic UI and React Flow colors derive from those variables. Native window appearance follows the same preference.

See the [root README](../../README.md) for CLI usage and acceptance checks, and [the schema contract](../../docs/scenario-v1.md) for supported fields and formats.
