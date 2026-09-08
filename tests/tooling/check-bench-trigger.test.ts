import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const CHECK_BENCH_TRIGGER_PATHS = [
  "tools/benchmarks/scripts/benchmark-binary.mjs",
  "tools/benchmarks/scripts/check-gate.mjs",
  "tools/benchmarks/scripts/check-gate-env.mjs",
  "tools/benchmarks/scripts/check-gate-plants.mjs",
  "tools/benchmarks/scripts/check-gate-report.mjs",
  "tools/benchmarks/scripts/check-gate-report.test.mjs",
  "tools/benchmarks/scripts/generate.mjs",
  ".github/workflows/check-bench.yml",
];

function readCheckBenchPullRequestPaths(): string[] {
  const workflow = fs
    .readFileSync(path.join(root, ".github", "workflows", "check-bench.yml"), "utf8")
    .replace(/\r\n?/g, "\n")
    .split("\n");
  const pullRequestIndex = workflow.findIndex((line) => line === "  pull_request:");
  assert.notEqual(pullRequestIndex, -1, "check-bench.yml must declare a pull_request trigger");
  const pathsIndex = workflow.findIndex(
    (line, index) => index > pullRequestIndex && line === "    paths:",
  );
  assert.notEqual(pathsIndex, -1, "check-bench.yml pull_request trigger must declare paths");

  const paths = [];
  for (const line of workflow.slice(pathsIndex + 1)) {
    if (!line.startsWith("      - ")) break;
    paths.push(line.slice("      - ".length).replace(/^["']|["']$/g, ""));
  }
  return paths;
}

test("check-bench pull request trigger covers every fail-closed measurement helper", () => {
  assert.deepEqual(readCheckBenchPullRequestPaths().sort(), CHECK_BENCH_TRIGGER_PATHS.toSorted());
});
