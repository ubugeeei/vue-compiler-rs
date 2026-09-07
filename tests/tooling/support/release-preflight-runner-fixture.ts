import assert from "node:assert/strict";
import { Buffer } from "node:buffer";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { crc32 } from "node:zlib";

import { workspaceVersionFromCargoToml } from "../../../legacy-tools/github/release-preflight-core.mjs";
import {
  realProjectArtifacts,
  shardEntries,
} from "../_helpers/release-preflight-matrix-evidence-fixture.ts";
import { repoRoot } from "../_helpers/moonbit.ts";
import { writeFakeCommand } from "./fake-command.ts";

const releaseSha = "a".repeat(40);
const baseSha = "b".repeat(40);

export function createReleasePreflightVerifyOnlyFixture(
  tempDir: string,
  options: {
    mutateJobs?: (jobs: ReturnType<typeof releaseWorkflowJobs>) => void;
    mutateRuns?: (runs: ReturnType<typeof releaseWorkflowRuns>) => void;
    mutateShardEntries?: (shard: number, entries: Record<string, string>) => void;
    requireJobTimeoutArgs?: boolean;
  } = {},
) {
  const binDir = path.join(tempDir, "bin");
  const dataDir = path.join(tempDir, "github-api");
  const artifactDir = path.join(tempDir, "artifacts");
  const version = workspaceVersionFromCargoToml(
    fs.readFileSync(path.join(repoRoot, "Cargo.toml"), "utf8"),
  );
  const tag = `v${version}`;
  const trackedManifests = trackedReleaseManifests();
  const runs = releaseWorkflowRuns(tag);
  const matrixRun = runs.find((candidate) => candidate.name === "Real Project Matrix");
  assert.ok(matrixRun);
  const artifacts = realProjectArtifacts(matrixRun);
  const jobs = releaseWorkflowJobs();
  options.mutateRuns?.(runs);
  options.mutateJobs?.(jobs);

  fs.mkdirSync(binDir, { recursive: true });
  fs.mkdirSync(dataDir, { recursive: true });
  fs.mkdirSync(artifactDir, { recursive: true });
  for (const shard of artifacts.keys()) {
    const entries = shardEntries(shard, {
      typecheckProject: typecheckProjectIds()[shard] ?? null,
    });
    options.mutateShardEntries?.(shard, entries);
    fs.writeFileSync(path.join(artifactDir, `${shard}.zip`), storedZip(Object.entries(entries)));
  }
  writeJson(path.join(dataDir, "runs.json"), runs);
  writeJson(path.join(dataDir, "artifacts.json"), artifacts);
  writeJson(path.join(dataDir, "jobs.json"), jobs);

  writeFlatJobEvidenceGitCommand(binDir);
  writeFlatJobEvidenceCurlCommand(binDir);

  return {
    binDir,
    tag,
    env: {
      GITHUB_API_URL: "https://api.github.test",
      GITHUB_REPOSITORY: "owner/repository",
      GITHUB_REF_NAME: tag,
      GITHUB_REF_TYPE: "tag",
      GITHUB_SHA: releaseSha,
      GITHUB_TOKEN: "secret",
      TEST_ARTIFACT_DIR: artifactDir,
      TEST_ARTIFACTS_FILE: path.join(dataDir, "artifacts.json"),
      TEST_BASE_SHA: baseSha,
      TEST_JOBS_FILE: path.join(dataDir, "jobs.json"),
      TEST_MAIN_CARGO_TOML: `[workspace.package]\nversion = "${version}"\n`,
      TEST_PACKAGE_MANIFESTS: JSON.stringify(trackedManifests),
      TEST_RELEASE_SHA: releaseSha,
      TEST_REQUIRE_JOB_TIMEOUT_ARGS: options.requireJobTimeoutArgs === true ? "1" : "0",
      TEST_RUNS_FILE: path.join(dataDir, "runs.json"),
      TEST_TAG: tag,
      TEST_TAG_SHA: releaseSha,
    },
  };
}

function releaseWorkflowRuns(tag: string) {
  const run = (
    id: number,
    name: string,
    workflowPath: string,
    event: string,
    displayTitle = `${name} release evidence`,
  ) => ({
    id,
    name,
    display_title: displayTitle,
    path: workflowPath,
    event,
    head_branch: event === "push" ? "main" : tag,
    head_sha: releaseSha,
    status: "completed",
    conclusion: "success",
    html_url: `https://example.test/runs/${id}`,
    created_at: `2026-07-12T00:${id}:00Z`,
    run_started_at: `2026-07-12T00:${id}:00Z`,
    updated_at: `2026-07-12T00:${id}:00Z`,
  });
  return [
    run(101, "Check", ".github/workflows/check.yml", "push"),
    run(
      102,
      "Benchmark",
      ".github/workflows/benchmark.yml",
      "workflow_dispatch",
      `Benchmark ${baseSha}...${releaseSha}`,
    ),
    run(
      103,
      "Fuzz",
      ".github/workflows/fuzz.yml",
      "workflow_dispatch",
      `Fuzz replay @ ${releaseSha}`,
    ),
    run(104, "Miri", ".github/workflows/miri.yml", "push"),
    run(105, "Docs build", ".github/workflows/build-docs.yml", "push"),
    run(
      106,
      "Real Project Matrix",
      ".github/workflows/real-project-matrix.yml",
      "workflow_dispatch",
      `Real Project Matrix @ ${releaseSha}`,
    ),
  ];
}

function releaseWorkflowJobs() {
  return Object.fromEntries(
    [
      [101, ["test-scripts"]],
      [102, ["pr-benchmark-budget"]],
      [
        103,
        [
          "Fuzz sfc_parse",
          "Fuzz template_lexer",
          "Fuzz js_ts_expression",
          "Fuzz css_parse",
          "Fuzz template_compile",
        ],
      ],
      [106, Array.from({ length: 22 }, (_, shard) => `real projects (${shard}/22)`)],
    ].map(([id, names]) => [
      id,
      (names as string[]).map((name) => ({ name, status: "completed", conclusion: "success" })),
    ]),
  );
}

function writeFlatJobEvidenceGitCommand(binDir: string): void {
  writeFakeCommand(
    binDir,
    "git",
    [
      "const args = process.argv.slice(2);",
      "const command = args.join(' ');",
      "if (command === 'rev-parse HEAD') console.log(process.env.TEST_RELEASE_SHA);",
      "else if (command === 'rev-parse refs/remotes/origin/main') console.log(process.env.TEST_RELEASE_SHA);",
      "else if (command === 'rev-list --first-parent refs/remotes/origin/main') console.log(process.env.TEST_RELEASE_SHA);",
      "else if (command === 'show refs/remotes/origin/main:Cargo.toml') process.stdout.write(process.env.TEST_MAIN_CARGO_TOML);",
      "else if (args[0] === 'ls-files') process.stdout.write(JSON.parse(process.env.TEST_PACKAGE_MANIFESTS).join('\\0') + '\\0');",
      "else if (args[0] === 'ls-remote') console.log(`${process.env.TEST_TAG_SHA}\\trefs/tags/${process.env.TEST_TAG}`);",
      "else if (args[0] === 'rev-list') console.log(`${process.env.TEST_RELEASE_SHA} ${process.env.TEST_BASE_SHA}`);",
      "else if (args[0] === 'merge-base') process.exit(0);",
      "else if (args[0] === 'diff') console.log('crates/vize/src/lib.rs');",
      "else process.exit(2);",
    ].join("\n"),
  );
}

function writeFlatJobEvidenceCurlCommand(binDir: string): void {
  writeFakeCommand(
    binDir,
    "curl",
    [
      "const fs = require('node:fs');",
      "const path = require('node:path');",
      "const url = process.argv.at(-1);",
      "const args = process.argv.slice(2);",
      "const read = (file) => JSON.parse(fs.readFileSync(file, 'utf8'));",
      "const send = (value) => process.stdout.write(JSON.stringify(value));",
      "const appendLog = (file, value) => { if (file) fs.appendFileSync(file, `${value}\\n`); };",
      "const isJobsRequest = url.includes('/actions/runs/') && url.includes('/jobs');",
      "if (process.env.TEST_REQUIRE_JOB_TIMEOUT_ARGS === '1' && isJobsRequest) {",
      "  if (!args.includes('--connect-timeout') || !args.includes('--max-time')) {",
      "    console.error('jobs request is missing bounded curl timeout flags');",
      "    process.exit(22);",
      "  }",
      "}",
      "if (url.includes('/actions/runs?')) {",
      "  if (process.env.TEST_REJECT_RUNS_AFTER_JOB_REQUEST === '1' && process.env.TEST_JOBS_REQUEST_LOG && fs.existsSync(process.env.TEST_JOBS_REQUEST_LOG)) {",
      "    console.error('release preflight polled workflow runs after job failure evidence');",
      "    process.exit(22);",
      "  }",
      "  appendLog(process.env.TEST_RUNS_REQUEST_LOG, url);",
      "  send({ workflow_runs: read(process.env.TEST_RUNS_FILE) });",
      "}",
      "else if (url.includes('/actions/runs/') && url.includes('/artifacts')) send({ artifacts: read(process.env.TEST_ARTIFACTS_FILE) });",
      "else if (url.startsWith('https://example.test/artifacts/')) {",
      "  const output = args[args.indexOf('--output') + 1];",
      "  const shard = url.match(/\\/artifacts\\/(\\d+)\\.zip$/)?.[1];",
      "  if (output == null || shard == null) process.exit(22);",
      "  fs.copyFileSync(path.join(process.env.TEST_ARTIFACT_DIR, `${shard}.zip`), output);",
      "} else if (isJobsRequest) {",
      "  const id = url.match(/\\/actions\\/runs\\/(\\d+)\\/jobs/)?.[1];",
      "  appendLog(process.env.TEST_JOBS_REQUEST_LOG, id ?? 'unknown');",
      "  send({ jobs: read(process.env.TEST_JOBS_FILE)[id] ?? [] });",
      "} else if (url.includes('/issues?')) send([]);",
      "else { console.error(`unexpected curl url: ${url}`); process.exit(22); }",
    ].join("\n"),
  );
}

function trackedReleaseManifests(): string[] {
  const trackedManifests = spawnSync("git", ["ls-files", "-z", "--", "editors", "npm"], {
    cwd: repoRoot,
    encoding: "utf8",
  });
  assert.equal(trackedManifests.status, 0, trackedManifests.stderr);
  return trackedManifests.stdout.split("\0").filter(Boolean);
}

function typecheckProjectIds() {
  const registry = JSON.parse(
    fs.readFileSync(path.join(repoRoot, "tests/_fixtures/vue-ecosystem-fixtures.json"), "utf8"),
  );
  return registry.projects
    .filter(
      (project: { typecheckPerformance?: { enabled?: boolean } }) =>
        project.typecheckPerformance?.enabled === true,
    )
    .map((project: { id: string }) => project.id);
}

function storedZip(files: [string, string][]) {
  const locals: Buffer[] = [];
  const central: Buffer[] = [];
  let offset = 0;
  for (const [name, text] of files) {
    const nameBytes = Buffer.from(name, "utf8");
    const data = Buffer.from(text, "utf8");
    const checksum = crc32(data);
    const local = Buffer.alloc(30 + nameBytes.byteLength);
    local.writeUInt32LE(0x04034b50, 0);
    local.writeUInt16LE(20, 4);
    local.writeUInt32LE(checksum, 14);
    local.writeUInt32LE(data.byteLength, 18);
    local.writeUInt32LE(data.byteLength, 22);
    local.writeUInt16LE(nameBytes.byteLength, 26);
    nameBytes.copy(local, 30);
    locals.push(local, data);

    const header = Buffer.alloc(46 + nameBytes.byteLength);
    header.writeUInt32LE(0x02014b50, 0);
    header.writeUInt16LE(20, 4);
    header.writeUInt16LE(20, 6);
    header.writeUInt32LE(checksum, 16);
    header.writeUInt32LE(data.byteLength, 20);
    header.writeUInt32LE(data.byteLength, 24);
    header.writeUInt16LE(nameBytes.byteLength, 28);
    header.writeUInt32LE(offset, 42);
    nameBytes.copy(header, 46);
    central.push(header);
    offset += local.byteLength + data.byteLength;
  }
  const directory = Buffer.concat(central);
  const end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50, 0);
  end.writeUInt16LE(files.length, 8);
  end.writeUInt16LE(files.length, 10);
  end.writeUInt32LE(directory.byteLength, 12);
  end.writeUInt32LE(offset, 16);
  return Buffer.concat([...locals, directory, end]);
}

function writeJson(filePath: string, value: unknown): void {
  fs.writeFileSync(filePath, JSON.stringify(value));
}
