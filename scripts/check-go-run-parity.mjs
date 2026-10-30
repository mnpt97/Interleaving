// Run the Go CLI against a stateless loopback fixture. Never use real credentials.
import { createServer } from "node:http";
import { spawn } from "node:child_process";
import { mkdtemp, readFile, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { extendedCases } from "./lib/run-parity-cases.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const temp = await mkdtemp(resolve(tmpdir(), "interl-run-parity-"));
const sentinels = [
  "parity-user",
  "parity-password",
  "parity-token",
  "parity-cookie",
  "parity-second-user",
  "parity-second-password",
  "parity-second-cookie",
];
let mode = "success";
let activeProcess;
let observed = [];
let current = {};
const server = createServer(async (request, response) => {
  const chunks = [];
  for await (const chunk of request) chunks.push(chunk);
  const body = Buffer.concat(chunks).toString();
  const url = new URL(request.url, "http://fixture");
  const path = url.pathname.replace(/^\/api/, "");
  observed.push({
    method: request.method,
    path: request.url,
    headers: request.headers,
    body,
  });
  if (mode === "broken" && path.startsWith("/broken")) {
    request.socket.destroy();
    return;
  }
  if (mode === "incomplete") {
    response.writeHead(200, { "Content-Length": "100" });
    response.flushHeaders();
    response.write("{");
    setTimeout(() => response.destroy(), 5);
    return;
  }
  if (mode === "wait") return;
  if (mode === "cleanup-timeout" && url.pathname === "/cleanup") return;
  if (mode === "cancel" && url.pathname === "/health") {
    activeProcess.kill("SIGINT");
    return;
  }
  if (mode === "transport" && url.pathname === "/health") {
    request.socket.destroy();
    return;
  }
  response.setHeader("Content-Type", "application/json");
  // Fixed fixture headers make wire previews deterministic across separate runs.
  response.setHeader("Date", "Thu, 29 Oct 2026 12:00:00 GMT");
  response.setHeader("Connection", "close");
  if (mode === "repeated-headers")
    response.setHeader("X-Test", ["first", "second"]);
  let value = { ok: true };
  if (path === "/auth/login") {
    const credentials = JSON.parse(body);
    const user = credentials.username ?? credentials.deviceId;
    const password = credentials.password ?? credentials.deviceSecret;
    const second = user === sentinels[4];
    if (
      user !== sentinels[second ? 4 : 0] ||
      password !== sentinels[second ? 5 : 1]
    )
      throw Error("credential mapping");

    response.setHeader(
      "Set-Cookie",
      `sid=${sentinels[second ? 6 : 3]}; Path=${mode === "cookie-path" ? "/api" : "/"}; HttpOnly${mode === "expired-cookie" ? "; Max-Age=0" : ""}`,
    );
    value = { accessToken: sentinels[2] };
    if (mode === "login-body") {
      response.end(current.loginBody);
      return;
    }
    if (mode === "login-rejected") {
      response.statusCode = 401;
      value = { error: "Unauthorized" };
    }
  } else if (path === "/auth/me") {
    if (request.headers.authorization !== `Bearer ${sentinels[2]}`) {
      response.statusCode = 401;
      value = { error: "Unauthorized" };
    }
  } else if (path === "/auth/cookie-me") {
    if (
      ![sentinels[3], sentinels[6]].some(
        (cookie) => request.headers.cookie === `sid=${cookie}`,
      )
    ) {
      response.statusCode = 401;
      value = { error: "Unauthorized" };
    }
  } else if (url.pathname === "/routing-body") {
    value = { value: { status: true, attempts: 3, label: "ready" } };
  } else if (url.pathname === "/long-poll") {
    await new Promise((resolve) =>
      setTimeout(resolve, JSON.parse(body).timeoutMs),
    );
  }
  if (path === "/not-found") response.statusCode = 404;
  if (mode === "redirect") {
    response.statusCode = 302;
    response.setHeader("Location", "/never");
  }
  if (mode === "large") value = { padding: "x".repeat(70000) };
  if (mode === "http-error") response.statusCode = 503;
  response.end(JSON.stringify(value));
});
await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
const origin = `http://127.0.0.1:${server.address().port}`;
const execute = (binary, args, missing) =>
  new Promise((resolve, reject) => {
    const env = {
      ...process.env,
      INTERL_ALICE_USERNAME: sentinels[0],
      INTERL_ALICE_PASSWORD: sentinels[1],
      INTERL_USERNAME: sentinels[0],
      INTERL_PASSWORD: sentinels[1],
      STATIC_TOKEN: sentinels[2],
      BOB_USERNAME: sentinels[4],
      BOB_PASSWORD: sentinels[5],
    };
    if (missing) delete env.INTERL_ALICE_PASSWORD;
    const child = spawn(binary, args, { cwd: root, env });
    activeProcess = child;
    const timer = setTimeout(() => {
      child.kill("SIGKILL");
      reject(Error("CLI timed out"));
    }, 15000);
    let stdout = "",
      stderr = "";
    child.stdout.on("data", (data) => (stdout += data));
    child.stderr.on("data", (data) => (stderr += data));
    child.on("error", (error) => {
      clearTimeout(timer);
      reject(error);
    });
    child.on("close", (code) => {
      clearTimeout(timer);
      resolve({ code, stdout, stderr });
    });
  });
try {
  let count = 0;
  const failures = [];
  const baseline = JSON.parse(
    await readFile(
      resolve(root, "tests/compatibility/contract-baseline.json"),
      "utf8",
    ),
  );
  const examples = baseline.validations
    .filter((v) => v.exitCode === 0 && v.file.startsWith("scenarios/examples/"))
    .map((v) => ({ source: v.file, id: v.file, mode: "success", exit: 0 }));
  for (const test of [
    ...examples,
    ...extendedCases(),
    { file: "hello-api", mode: "success", exit: 0 },
    { file: "hello-api", mode: "http-error", exit: 0 },
    { file: "hello-api", mode: "transport", exit: 3 },
    { file: "hello-api", mode: "cancel", exit: 130 },
    { file: "hello-api", mode: "cancel", cleanup: true, exit: 130 },
    { file: "hello-api", mode: "cleanup-timeout", cleanup: true, exit: 3 },
    { file: "feature-showcase", mode: "success", minimum: 1, exit: 0 },
    { file: "feature-showcase", mode: "success", minimum: 4, exit: 0 },
    { file: "feature-showcase", mode: "success", missing: true, exit: 2 },
  ]) {
    try {
      current = test;
      const sourcePath =
        test.source ?? `scenarios/examples/${test.file}.interl.json`;
      const original = await readFile(resolve(root, sourcePath), "utf8");
      let source = original
        .replaceAll("http://127.0.0.1:8080", origin)
        .replace(/waitMs: 2000/g, "waitMs: 1");
      const format = sourcePath.endsWith(".yaml") ? "yaml" : "json";
      let doc;
      if (format === "json") {
        doc = JSON.parse(source);
        if (test.cleanup) {
          const cleanup = structuredClone(doc.workflows[0]);
          cleanup.id = "cleanup";
          cleanup.nodes[1].config.path = "/cleanup";
          doc.workflows.push(cleanup);
          doc.groups[0].lifecycle.cleanup = { workflowId: "cleanup" };
          doc.groups[0].lifecycle.cleanupTimeoutMs = 20;
        }
        if (doc.environment?.variables?.waitMs !== undefined)
          doc.environment.variables.waitMs = 1;
        if (test.minimum !== undefined)
          doc.groups[0].lifecycle.iteration.inputs.minimum.value = test.minimum;
        test.patch?.(doc);
        if (test.mode === "cookie-path")
          doc.workflows
            .find((w) => w.id === "initialize")
            .nodes.find((n) => n.id === "verify-cookie").config.path =
            "/api/auth/cookie-me";
        source = JSON.stringify(doc);
      }
      const scenario = resolve(temp, `scenario.interl.${format}`);
      await writeFile(scenario, source);
      {
        const name = "go",
          binary = resolve(root, "go/bin/interl-go");
        mode = test.mode;
        observed = [];
        const output = resolve(temp, `${name}.json`);
        await rm(output, { force: true });
        const result = await execute(
          binary,
          ["run", scenario, "--output", output],
          test.missing,
        );
        if (result.code !== test.exit)
          throw Error(
            `${name} ${test.id ?? test.file ?? test.source} exit ${result.code}: ${result.stderr}`,
          );
        const raw = await readFile(output, "utf8").catch((error) => {
          if (error.code === "ENOENT") return null;
          throw error;
        });
        if (test.missing && raw !== null)
          throw Error(`${name} wrote a report after preparation failed`);
        const report = raw === null ? null : JSON.parse(raw);
        const diagnostics = [
          ...result.stderr.matchAll(/\[([^/\]]+)\/([^\]]+)\] ([^:]*):/g),
        ].map(([, category, code, fieldPath]) => ({
          category,
          code,
          fieldPath,
        }));
        if (test.exit === 2 && (observed.length !== 0 || raw !== null))
          throw Error(`${name} preparation performed HTTP or wrote a report`);
        if (
          test.diagnostic &&
          !diagnostics.some((d) => d.code === test.diagnostic)
        )
          throw Error(`${name} preparation diagnostic mismatch`);
        if (test.error && report?.error?.kind !== test.error)
          throw Error(
            `${name} expected ${test.error}, got ${JSON.stringify(report?.error)}`,
          );
        for (const secret of sentinels)
          if (
            JSON.stringify(report).includes(secret) ||
            result.stdout.includes(secret) ||
            result.stderr.includes(secret)
          )
            throw Error(`${name} secret escaped`);
        if (report) {
          if (report.summary.requests !== observed.length)
            throw Error("request count differs from wire observations");
          const events = report.events ?? [];
          if (
            events.some(
              (event, index) =>
                index > 0 && event.sequence <= events[index - 1].sequence,
            )
          )
            throw Error("events are not ordered");
          if (test.exit === 0 && report.status !== "passed")
            throw Error("successful CLI did not produce a passed report");
          if (test.exit === 130 && report.status !== "cancelled")
            throw Error("cancelled CLI did not produce a cancelled report");
        }
      }
      count++;
    } catch (error) {
      failures.push(
        `${test.id ?? test.id ?? test.file ?? test.source}: ${error.message}`,
      );
    }
  }
  if (failures.length)
    throw Error(
      `${failures.length} runtime mismatches:\n${failures.join("\n")}`,
    );
  console.log(
    `Verified ${count} Go runtime contract cases: reports, ordered events, wire requests, cancellation, and secret absence.`,
  );
} finally {
  server.closeAllConnections();
  await new Promise((resolve) => server.close(resolve));
  await rm(temp, { recursive: true, force: true });
}
