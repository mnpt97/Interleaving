// Differential test only: neither production binary invokes the other runtime.
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, resolve, join } from "node:path";
import { fileURLToPath } from "node:url";
import assert from "node:assert/strict";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const corpusPath = resolve(root, "tests/compatibility/phase-2-scenarios.json");
const corpus = JSON.parse(readFileSync(corpusPath, "utf8"));
if (process.argv.length > 2)
  throw new Error(
    "No flags are supported; contract expectations are reviewed explicitly.",
  );
const temporary = mkdtempSync(join(tmpdir(), "interl-validation-parity-"));
function source(test) {
  if (test.repeat !== undefined)
    return (
      (test.prefix ?? "") + test.repeat.repeat(test.count) + (test.suffix ?? "")
    );
  if (test.source !== undefined) return test.source;
  const document = JSON.parse(readFileSync(resolve(root, test.base), "utf8"));
  const rawNumbers = [];
  for (const patch of test.patches ?? []) {
    let parent = document;
    for (const key of patch.path.slice(0, -1)) {
      if (parent[key] === undefined) parent[key] = {};
      parent = parent[key];
    }
    if (patch.rawValue !== undefined) {
      assert(
        /^-?(?:0|[1-9]\d*)(?:\.\d+)?(?:[eE][+-]?\d+)?$/.test(patch.rawValue),
        "rawValue must be a JSON number",
      );
      const marker = `__INTERL_RAW_NUMBER_${rawNumbers.length}__`;
      parent[patch.path.at(-1)] = marker;
      rawNumbers.push([JSON.stringify(marker), patch.rawValue]);
    } else parent[patch.path.at(-1)] = patch.value;
  }
  let encoded = JSON.stringify(document);
  for (const [marker, number] of rawNumbers) {
    assert.equal(
      encoded.split(marker).length,
      2,
      "raw number marker collision",
    );
    encoded = encoded.replace(marker, number);
  }
  return encoded;
}
function run(binary, path) {
  const result = spawnSync(binary, ["validate", path], {
    cwd: root,
    encoding: "utf8",
    timeout: 15000,
  });
  if (result.error) throw result.error;
  if (![0, 2].includes(result.status))
    throw new Error(`${binary}: exit ${result.status}: ${result.stderr}`);
  const diagnostics = [
    ...result.stderr.matchAll(/\[([^/\]]+)\/([^\]]+)\] ([^:]*):/g),
  ].map(([, category, code, fieldPath]) => ({ category, code, fieldPath }));
  if (result.status === 2 && !diagnostics.length)
    throw new Error(`Unstructured failure: ${result.stderr}`);
  assert(
    !/SECRET_SENTINEL/.test(result.stdout + result.stderr),
    "sensitive sentinel leaked",
  );
  return {
    exitCode: result.status,
    diagnostics,
    stdoutPresent: !!result.stdout,
    stderrPresent: !!result.stderr,
  };
}
let failures = 0;
try {
  for (const test of corpus.cases) {
    const file = join(
      temporary,
      `scenario.${test.format === "yaml" ? "yaml" : "json"}`,
    );
    writeFileSync(file, source(test));
    const actual = run(resolve(root, "go/bin/interl-go"), file);
    try {
      assert.deepEqual(actual, test.expected);
    } catch {
      failures++;
      console.error(
        `${test.id}:\n  Expected ${JSON.stringify(test.expected)}\n  Actual   ${JSON.stringify(actual)}`,
      );
    }
  }
} finally {
  rmSync(temporary, { recursive: true, force: true });
}
if (failures) throw new Error(`${failures} validation parity mismatches`);
console.log(
  `Go validation contract passed for ${corpus.cases.length} extended cases.`,
);
