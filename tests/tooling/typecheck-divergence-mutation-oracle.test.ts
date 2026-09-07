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
  updateVizeOutput,
} from "./_helpers/typecheck-divergence-report-fixture.ts";

const outerHarnessTimeoutMs = 20_000;

function artifactPath(fixture: ReturnType<typeof setup>, extension: string) {
  return path.join(fixture.reportDir, `fixture-typecheck-divergence.${extension}`);
}

test("seeded mutation oracle accepts a shared probe with shifted compiler coordinates", () => {
  const fixture = setup({
    baselineOutput: "",
    baselineFiles: ["src/App.vue"],
    baselineMutation: "shifted",
    vizeDiagnostics: [],
    vizeMutation: "shifted",
  });
  try {
    const result = run(fixture);
    assert.equal(result.status, 0, result.stderr);
    const oracle = readJson(artifactPath(fixture, "json")).mutationOracle;
    assert.equal(oracle.passed, true);
    assert.equal(oracle.expectedDiagnosticMatched, true);
    assert.equal(oracle.file, "src/App.vue");
    assert.deepEqual(oracle.span, { line: 3, column: 1 });
    assert.equal(oracle.states.length, 3);
    const [cleanState, brokenState, repairedState] = oracle.states;
    assert.equal(cleanState.sharedCount, 0);
    assert.equal(cleanState.messageMismatchCount, 0);
    assert.equal(brokenState.sharedCount, 1);
    assert.equal(brokenState.messageMismatchCount, 0);
    assert.equal(repairedState.sharedCount, 0);
    assert.equal(repairedState.messageMismatchCount, 0);
    assert.notEqual(brokenState.sourceSha256, cleanState.sourceSha256);
    assert.equal(repairedState.sourceSha256, cleanState.sourceSha256);
  } finally {
    cleanup(fixture);
  }
});

test("seeded mutation oracle narrows checker reruns to the selected probe file", () => {
  const fixture = setup({
    baselineOutput: "",
    baselineFiles: ["src/App.vue", "src/Other.vue"],
    vizeDiagnostics: [],
  });
  try {
    fs.writeFileSync(path.join(fixture.fixtureRoot, "src", "Other.vue"), "<template />\n");
    updateVizeOutput(fixture, (parsed) => {
      parsed.fileCount = 2;
      parsed.files = [
        { file: "src/App.vue", diagnostics: [] },
        { file: "src/Other.vue", diagnostics: [] },
      ];
    });

    const result = run(fixture);
    assert.equal(result.status, 0, result.stderr);
    const artifact = readJson(artifactPath(fixture, "json"));
    const oracle = artifact.mutationOracle;
    assert.equal(oracle.passed, true);

    const fullBaselineConfig = readJson(
      path.join(fixture.reportDir, "fixture-vue-tsc.tsconfig.json"),
    );
    assert.deepEqual(fullBaselineConfig.files, ["../src/App.vue", "../src/Other.vue"]);

    const mutationConfig = readJson(
      path.join(fixture.fixtureRoot, ".vize-baseline", "fixture-mutation-vue-tsc.tsconfig.json"),
    );
    assert.deepEqual(mutationConfig.files, [`../${oracle.file}`]);

    for (const state of oracle.states) {
      assert.match(state.vize.command, new RegExp(` check ${oracle.file.replace("/", "\\/")} `));
      assert.doesNotMatch(state.vize.command, /src\/\*\*\/\*\.vue/u);
      assert.match(state.baseline.command, /fixture-mutation-vue-tsc\.tsconfig\.json/u);
    }
  } finally {
    cleanup(fixture);
  }
});

test("seeded mutation oracle bounds a Vize mutation process that ignores SIGTERM", () => {
  if (process.platform === "win32") return;

  const fixture = setup();
  const sourcePath = path.join(fixture.fixtureRoot, "src/App.vue");
  const originalSource = fs.readFileSync(sourcePath, "utf8");
  try {
    updateJson(
      fixture.registryPath,
      (registry) => (registry.projects[0].typecheckPerformance.hangTimeoutMs = 80),
    );
    fs.writeFileSync(
      fixture.vize,
      `#!/usr/bin/env node
import { spawn } from "node:child_process";
if (process.argv.includes("--version")) { console.log("vize 0.0.0-test"); process.exit(0); }
process.on("SIGTERM", () => {});
spawn(process.execPath, ["-e", ${JSON.stringify(
        "process.on('SIGTERM', () => {}); setInterval(() => {}, 1000);",
      )}], { stdio: "inherit" });
setInterval(() => {}, 1000);
`,
    );
    fs.chmodSync(fixture.vize, 0o755);

    const startedAt = Date.now();
    const result = run(fixture, {}, [], { timeoutMs: outerHarnessTimeoutMs });
    assert.equal(result.status, 1);
    assert.ok(
      Date.now() - startedAt < outerHarnessTimeoutMs,
      "timeout handling must not wait for a Vize mutation process tree that ignores SIGTERM",
    );
    const artifact = readJson(artifactPath(fixture, "json"));
    assert.match(
      artifact.mutationOracle.unusableReason,
      /Vize mutation run failed: spawn timed out after 80ms/,
    );
    assert.equal(fs.readFileSync(sourcePath, "utf8"), originalSource);
  } finally {
    cleanup(fixture);
  }
});
