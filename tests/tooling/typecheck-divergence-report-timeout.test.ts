import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { test } from "node:test";

import {
  cleanup,
  readJson,
  run,
  setup,
  updateJson,
  unusableFailure,
  writeVueTsc,
} from "./_helpers/typecheck-divergence-report-fixture.ts";

const outerHarnessTimeoutMs = 20_000;

function artifactPath(fixture: ReturnType<typeof setup>) {
  return path.join(fixture.reportDir, "fixture-typecheck-divergence.json");
}

function killPidFileIfPresent(pidPath: string) {
  if (!fs.existsSync(pidPath)) return;

  const rawPid = fs.readFileSync(pidPath, "utf8").trim();
  const pid = Number.parseInt(rawPid, 10);
  if (!Number.isSafeInteger(pid) || pid <= 0 || String(pid) !== rawPid) return;

  try {
    process.kill(pid, "SIGKILL");
  } catch (error) {
    if ((error as { code?: unknown }).code !== "ESRCH") throw error;
  }
}

test("typecheck divergence report bounds a vue-tsc baseline that ignores SIGTERM", () => {
  if (process.platform === "win32") return;

  const fixture = setup();
  try {
    updateJson(
      fixture.registryPath,
      (registry) => (registry.projects[0].typecheckPerformance.hangTimeoutMs = 80),
    );
    writeVueTsc(
      fixture.vueTsc,
      `import { spawn } from "node:child_process";
process.on("SIGTERM", () => {});
spawn(process.execPath, ["-e", ${JSON.stringify(
        "process.on('SIGTERM', () => {}); setInterval(() => {}, 1000);",
      )}], { stdio: "inherit" });
setInterval(() => {}, 1000);`,
      fixture.invocationPath,
    );

    const startedAt = Date.now();
    const result = run(fixture, {}, [], { timeoutMs: outerHarnessTimeoutMs });
    const reason = "vue-tsc baseline failed to run: spawn timed out after 80ms";
    assert.equal(result.status, 1);
    assert.ok(
      Date.now() - startedAt < outerHarnessTimeoutMs,
      "timeout handling must not wait for a vue-tsc process tree that ignores SIGTERM",
    );
    assert.equal(result.stderr, `${unusableFailure(reason)}\n`);
    const artifact = readJson(artifactPath(fixture));
    assert.equal(artifact.baseline.exitCode, null);
    assert.equal(artifact.baseline.runError, reason);
    assert.equal(
      artifact.baseline.coverageRunError,
      `vue-tsc coverage baseline skipped because ${reason}`,
    );
    assert.equal(artifact.baseline.configuration.unusableReason, reason);
    assert.equal(artifact.baseline.coverage.unusableReason, reason);
    assert.equal(artifact.budget.verdict, "unusable");
    assert.equal(artifact.budget.passed, false);
  } finally {
    cleanup(fixture);
  }
});

test("typecheck divergence report closes inherited stdio children after checker exit", () => {
  if (process.platform === "win32") return;

  const fixture = setup();
  const childPidPath = path.join(fixture.fakeDir, "stdio-child.pid");
  try {
    fs.writeFileSync(
      fixture.vueTsc,
      `#!/usr/bin/env node
import { spawn } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

const child = spawn(process.execPath, ["-e", "setInterval(() => {}, 1000);"], { stdio: "inherit" });
if (typeof child.pid === "number") fs.writeFileSync(${JSON.stringify(childPidPath)}, String(child.pid));

if (process.argv.includes("--version")) {
  console.log("3.3.4");
  process.exit(0);
}

if (process.argv.includes("--listFilesOnly")) {
  console.log(path.join(process.cwd(), "src/App.vue"));
  process.exit(0);
}

process.stdout.write("src/App.vue(1,1): error TS2322: shared\\n");
process.exit(2);
`,
    );
    fs.chmodSync(fixture.vueTsc, 0o755);

    const startedAt = Date.now();
    const result = run(fixture, {}, ["--budget-mode", "record-only"], { timeoutMs: 8_000 });
    assert.equal(result.status, 0, result.stderr);
    assert.ok(
      Date.now() - startedAt < 8_000,
      "checker output capture must not wait for inherited stdio children after exit",
    );
    assert.equal(
      fs.existsSync(path.join(fixture.reportDir, "fixture-typecheck-divergence.json")),
      true,
    );
  } finally {
    killPidFileIfPresent(childPidPath);
    cleanup(fixture);
  }
});

test("typecheck divergence report records a vue-tsc coverage timeout as an unusable baseline", () => {
  if (process.platform === "win32") return;

  const fixture = setup();
  try {
    updateJson(
      fixture.registryPath,
      (registry) => (registry.projects[0].typecheckPerformance.hangTimeoutMs = 500),
    );
    writeVueTsc(
      fixture.vueTsc,
      `if (process.argv.includes("--listFilesOnly")) {
  setInterval(() => {}, 1000);
} else {
  process.stdout.write("src/App.vue(1,1): error TS2322: shared\\n");
  process.exit(2);
}`,
      fixture.invocationPath,
    );

    const startedAt = Date.now();
    const result = run(fixture, {}, [], { timeoutMs: outerHarnessTimeoutMs });
    const reason = "vue-tsc coverage baseline failed to run: spawn timed out after 500ms";
    assert.equal(result.status, 1);
    assert.ok(
      Date.now() - startedAt < outerHarnessTimeoutMs,
      "coverage timeout handling must not wait for the outer test timeout",
    );
    assert.equal(result.stderr, `${unusableFailure(reason)}\n`);
    const artifact = readJson(artifactPath(fixture));
    assert.equal(artifact.baseline.exitCode, 2);
    assert.equal(artifact.baseline.runError, null);
    assert.equal(artifact.baseline.coverageExitCode, null);
    assert.equal(artifact.baseline.coverageRunError, reason);
    assert.equal(artifact.baseline.configuration.verdict, "usable");
    assert.equal(artifact.baseline.coverage.unusableReason, reason);
    assert.equal(artifact.mutationOracle.unusableReason, reason);
    assert.equal(artifact.budget.verdict, "unusable");
  } finally {
    cleanup(fixture);
  }
});

test("typecheck divergence report drains verbose vue-tsc coverage output", () => {
  if (process.platform === "win32") return;

  const fixture = setup();
  try {
    updateJson(
      fixture.registryPath,
      (registry) => (registry.projects[0].typecheckPerformance.hangTimeoutMs = 5_000),
    );
    writeVueTsc(
      fixture.vueTsc,
      `if (process.argv.includes("--listFilesOnly")) {
  fs.writeSync(1, process.cwd() + "/src/App.vue\\n");
  const filler = process.cwd() + "/src/generated-support-file.ts\\n";
  for (let index = 0; index < 50_000; index++) fs.writeSync(1, filler);
  process.exit(0);
}

let output = "src/App.vue(1,1): error TS2322: shared\\n";
const source = fs.readFileSync("src/App.vue", "utf8");
const marker = "__vize_typecheck_mutation_probe: string = 1";
if (source.includes(marker)) {
  const line = source.slice(0, source.indexOf(marker)).split(/\\r?\\n/).length;
  output += \`src/App.vue(\${line},1): error TS2322: Type 'number' is not assignable to type 'string'.\\n\`;
}
process.stdout.write(output);
process.exit(2);`,
      fixture.invocationPath,
    );

    const result = run(fixture, {}, [], { timeoutMs: outerHarnessTimeoutMs });
    assert.equal(result.status, 0, result.stderr);
    const artifact = readJson(artifactPath(fixture));
    assert.equal(artifact.baseline.coverageRunError, null);
    assert.equal(artifact.baseline.coverage.baselineVueFileCount, 1);
    assert.equal(artifact.baseline.coverage.verdict, "usable");
    assert.equal(artifact.budget.verdict, "passed");
  } finally {
    cleanup(fixture);
  }
});

test("record-only mode still warns and uploads baseline timeout evidence", () => {
  if (process.platform === "win32") return;

  const fixture = setup();
  try {
    updateJson(
      fixture.registryPath,
      (registry) => (registry.projects[0].typecheckPerformance.hangTimeoutMs = 80),
    );
    writeVueTsc(
      fixture.vueTsc,
      `process.on("SIGTERM", () => {});
setInterval(() => {}, 1000);`,
      fixture.invocationPath,
    );

    const result = run(fixture, {}, ["--budget-mode", "record-only"], {
      timeoutMs: outerHarnessTimeoutMs,
    });
    const reason = "vue-tsc baseline failed to run: spawn timed out after 80ms";
    assert.equal(result.status, 0, result.stderr);
    assert.ok(
      result.stdout.includes(
        `::warning title=Typecheck divergence budget not enforced::${unusableFailure(reason)}`,
      ),
    );
    assert.equal(readJson(artifactPath(fixture)).budget.passed, false);
  } finally {
    cleanup(fixture);
  }
});
