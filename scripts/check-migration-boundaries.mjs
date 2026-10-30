import { readdirSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { resolve, relative } from "node:path";
const root = fileURLToPath(new URL("../", import.meta.url));
const source = resolve(root, "apps/desktop/src");
const files = (dir) =>
  readdirSync(dir, { withFileTypes: true }).flatMap((entry) =>
    entry.isDirectory()
      ? files(resolve(dir, entry.name))
      : [resolve(dir, entry.name)],
  );
const errors = [];
for (const file of files(source)) {
  const name = relative(source, file).replaceAll("\\", "/");
  if (
    !/\.[tj]sx?$/.test(file) ||
    /\.test\.[tj]sx?$/.test(file) ||
    name.startsWith("lib/desktop/")
  )
    continue;
  const text = readFileSync(file, "utf8");
  if (
    /(?:from\s*|import\s*\()\s*['"](?:@wailsio\/|[^'"]*desktop\/generated\/)/.test(
      text,
    )
  )
    errors.push(`${name}: native dependency outside its adapter`);
}
if (errors.length) throw new Error(errors.join("\n"));
console.log(
  "Frontend host boundaries pass. Go import/dependency rules run in internal/architecture tests.",
);
