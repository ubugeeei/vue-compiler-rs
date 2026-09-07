import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { tmpdir } from "node:os";
import { test } from "node:test";

import {
  parseReleasePreflightMode,
  readPackageManifests,
} from "../../legacy-tools/github/release-preflight.mjs";
import { workspaceVersionFromCargoToml } from "../../legacy-tools/github/release-preflight-core.mjs";
import { repoRoot } from "./_helpers/moonbit.ts";
import { mutateDivergence } from "./_helpers/release-preflight-matrix-evidence-fixture.ts";
import { writeFakeCommand } from "./support/fake-command.ts";
import { createReleasePreflightVerifyOnlyFixture } from "./support/release-preflight-runner-fixture.ts";

const sha = "a".repeat(40);

test("release preflight CLI fails closed on unknown or ambiguous modes", () => {
  assert.equal(parseReleasePreflightMode([]), "bootstrap");
  assert.equal(parseReleasePreflightMode(["--verify-only"]), "verify-only");
  assert.equal(parseReleasePreflightMode(["--target-only"]), "target-only");
  assert.throws(() => parseReleasePreflightMode(["--verify-onyl"]), /Usage:/);
  assert.throws(() => parseReleasePreflightMode(["--verify-only", "--target-only"]), /Usage:/);
});

test("target-only mode verifies the hydrated main ref, HEAD, and the peeled remote tag", () => {
  const tempDir = fs.mkdtempSync(path.join(tmpdir(), "vize-release-target-"));
  const binDir = path.join(tempDir, "bin");
  const version = workspaceVersionFromCargoToml(
    fs.readFileSync(path.join(repoRoot, "Cargo.toml"), "utf8"),
  );
  const trackedManifests = spawnSync("git", ["ls-files", "-z", "--", "editors", "npm"], {
    cwd: repoRoot,
    encoding: "utf8",
  });
  assert.equal(trackedManifests.status, 0, trackedManifests.stderr);
  fs.mkdirSync(binDir, { recursive: true });
  writeFakeCommand(
    binDir,
    "git",
    [
      "const args = process.argv.slice(2);",
      "const command = args.join(' ');",
      "if (command === 'rev-parse HEAD') console.log(process.env.TEST_HEAD_SHA);",
      "else if (command === 'rev-parse refs/remotes/origin/main') console.log(process.env.TEST_MAIN_SHA);",
      "else if (args[0] === 'ls-files') process.stdout.write(JSON.parse(process.env.TEST_PACKAGE_MANIFESTS).join('\\0') + '\\0');",
      "else if (args[0] === 'ls-remote') {",
      "  console.log(`${process.env.TEST_TAG_OBJECT}\\trefs/tags/${process.env.TEST_TAG}`);",
      "  console.log(`${process.env.TEST_TAG_SHA}\\trefs/tags/${process.env.TEST_TAG}^{}`);",
      "} else if (command === 'rev-list --first-parent refs/remotes/origin/main') console.log(process.env.TEST_MAIN_FIRST_PARENT_HISTORY);",
      "else if (command === 'show refs/remotes/origin/main:Cargo.toml') process.stdout.write(process.env.TEST_MAIN_CARGO_TOML);",
      "else if (args[0] === 'rev-list') console.log(`${process.env.TEST_RELEASE_SHA} ${process.env.TEST_BASE_SHA}`);",
      "else if (args[0] === 'merge-base') process.exit(0);",
      "else process.exit(2);",
    ].join("\n"),
  );
  const run = (overrides: Record<string, string> = {}) =>
    spawnSync("rust-script", ["tools/commands/ci/github/release-preflight.rs", "--target-only"], {
      cwd: repoRoot,
      encoding: "utf8",
      env: {
        ...process.env,
        PATH: `${binDir}${path.delimiter}${process.env.PATH ?? ""}`,
        GITHUB_REF_TYPE: "tag",
        GITHUB_REF_NAME: `v${version}`,
        GITHUB_SHA: sha,
        TEST_HEAD_SHA: sha,
        TEST_RELEASE_SHA: sha,
        TEST_MAIN_SHA: sha,
        TEST_MAIN_FIRST_PARENT_HISTORY: [sha, "b".repeat(40)].join("\n"),
        TEST_MAIN_CARGO_TOML: `[workspace.package]\nversion = "${version}"\n`,
        TEST_TAG: `v${version}`,
        TEST_TAG_OBJECT: "c".repeat(40),
        TEST_TAG_SHA: sha,
        TEST_BASE_SHA: "b".repeat(40),
        TEST_PACKAGE_MANIFESTS: JSON.stringify(trackedManifests.stdout.split("\0").filter(Boolean)),
        ...overrides,
      },
    });

  const outcome = (result: ReturnType<typeof run>) => [result.status, result.stderr];

  try {
    const success = run();
    assert.deepEqual(outcome(success), [0, ""], `${success.stderr}\n${success.stdout}`.trim());

    // The repository's merge automation lands PRs throughout the 30-40 minute
    // gate wait. Ordinary drift keeps the workspace version, so the release
    // still owns its version line and the gates measured at the tag still say
    // what they said.
    assert.deepEqual(outcome(run({ TEST_MAIN_SHA: "d".repeat(40) })), [0, ""]);

    // A second release commit is the one kind of drift that does invalidate
    // this one: finishing now publishes a lower version after a higher one.
    assert.deepEqual(
      outcome(
        run({
          TEST_MAIN_SHA: "d".repeat(40),
          TEST_MAIN_CARGO_TOML: '[workspace.package]\nversion = "99.99.99"\n',
        }),
      ),
      [
        1,
        `Release v${version} (${sha}) is superseded: origin/main ${"d".repeat(40)} is at workspace version 99.99.99, not ${version}. Publishing it now would ship an older version after a newer one; cut the next release instead.\n`,
      ],
    );

    assert.deepEqual(outcome(run({ TEST_HEAD_SHA: "f".repeat(40) })), [
      1,
      `Checked out HEAD ${"f".repeat(40)} does not match release event SHA ${sha}\n`,
    ]);

    assert.deepEqual(
      outcome(
        run({
          TEST_MAIN_SHA: "d".repeat(40),
          TEST_MAIN_FIRST_PARENT_HISTORY: ["d".repeat(40), "b".repeat(40)].join("\n"),
        }),
      ),
      [
        1,
        `Release commit ${sha} is not on the first-parent history of current origin/main ${"d".repeat(40)}\n`,
      ],
    );

    assert.deepEqual(outcome(run({ TEST_TAG_SHA: "e".repeat(40) })), [
      1,
      `Remote tag v${version} points to ${"e".repeat(40)}, expected ${sha}\n`,
    ]);
  } finally {
    fs.rmSync(tempDir, { recursive: true, force: true });
  }
});

test("release metadata inventory discovers every non-private npm and editor package", () => {
  assert.deepEqual(
    readPackageManifests().map((manifest) => manifest.path),
    [
      "editors/vscode-art/package.json",
      "editors/vscode/package.json",
      "npm/builder/rspack/package.json",
      "npm/builder/unplugin/package.json",
      "npm/builder/vite-musea/package.json",
      "npm/builder/vite/package.json",
      "npm/cli/package.json",
      "npm/compose/core/package.json",
      "npm/framework/musea-nuxt/package.json",
      "npm/framework/nuxt-lint-config/package.json",
      "npm/framework/nuxt/package.json",
      "npm/fresco-native/package.json",
      "npm/fresco/package.json",
      "npm/marquette/package.json",
      "npm/mcp-musea/package.json",
      "npm/native/package.json",
      "npm/oxlint/package.json",
      "npm/ui/package.json",
      "npm/wasm/package.json",
    ],
  );
});

test("verify-only mode accepts flat job evidence returned by pagination", () => {
  const tempDir = fs.mkdtempSync(path.join(tmpdir(), "vize-release-jobs-"));
  try {
    const fixture = createReleasePreflightVerifyOnlyFixture(tempDir);
    const result = spawnSync(
      "rust-script",
      ["tools/commands/ci/github/release-preflight.rs", "--verify-only"],
      {
        cwd: repoRoot,
        encoding: "utf8",
        env: {
          ...process.env,
          PATH: `${fixture.binDir}${path.delimiter}${process.env.PATH ?? ""}`,
          ...fixture.env,
        },
      },
    );
    assert.ifError(result.error);
    assert.equal(result.status, 0, `${result.stderr}\n${result.stdout}`.trim());
    assert.match(result.stdout, new RegExp(`Release preflight passed for ${fixture.tag}`));
  } finally {
    fs.rmSync(tempDir, { recursive: true, force: true });
  }
});

test("verify-only mode reports failed matrix shard jobs for red release evidence", () => {
  const tempDir = fs.mkdtempSync(path.join(tmpdir(), "vize-release-red-matrix-"));
  try {
    const fixture = createReleasePreflightVerifyOnlyFixture(tempDir, {
      requireJobTimeoutArgs: true,
      mutateRuns(runs) {
        const matrix = runs.find((run) => run.name === "Real Project Matrix");
        assert.ok(matrix);
        matrix.conclusion = "cancelled";
      },
      mutateJobs(jobs) {
        const matrixJobs = jobs[106];
        assert.ok(matrixJobs);
        matrixJobs[0] = { ...matrixJobs[0], conclusion: "cancelled" };
        matrixJobs[12] = { ...matrixJobs[12], conclusion: "failure" };
      },
    });
    const result = spawnSync(
      "rust-script",
      ["tools/commands/ci/github/release-preflight.rs", "--verify-only"],
      {
        cwd: repoRoot,
        encoding: "utf8",
        env: {
          ...process.env,
          PATH: `${fixture.binDir}${path.delimiter}${process.env.PATH ?? ""}`,
          ...fixture.env,
        },
      },
    );

    assert.ifError(result.error);
    assert.equal(result.status, 1, `${result.stderr}\n${result.stdout}`.trim());
    assert.match(result.stderr, /Real Project Matrix: completed\/cancelled/);
    assert.match(
      result.stderr,
      /required jobs: real projects \(0\/22\)=completed\/cancelled, real projects \(12\/22\)=completed\/failure/,
    );
  } finally {
    fs.rmSync(tempDir, { recursive: true, force: true });
  }
});

test("bootstrap mode stops waiting when an in-progress gate has failed required jobs", () => {
  const tempDir = fs.mkdtempSync(path.join(tmpdir(), "vize-release-red-pending-matrix-"));
  try {
    const jobsRequestLog = path.join(tempDir, "jobs-request.log");
    const runsRequestLog = path.join(tempDir, "runs-request.log");
    const fixture = createReleasePreflightVerifyOnlyFixture(tempDir, {
      requireJobTimeoutArgs: true,
      mutateRuns(runs) {
        const matrix = runs.find((run) => run.name === "Real Project Matrix");
        assert.ok(matrix);
        matrix.status = "in_progress";
        matrix.conclusion = null;
      },
      mutateJobs(jobs) {
        const matrixJobs = jobs[106];
        assert.ok(matrixJobs);
        const failedJobIndex = matrixJobs.findIndex((job) => job.name === "real projects (0/22)");
        const pendingJobIndex = matrixJobs.findIndex((job) => job.name === "real projects (12/22)");
        assert.notEqual(failedJobIndex, -1);
        assert.notEqual(pendingJobIndex, -1);
        matrixJobs[failedJobIndex] = { ...matrixJobs[failedJobIndex], conclusion: "failure" };
        matrixJobs[pendingJobIndex] = {
          ...matrixJobs[pendingJobIndex],
          status: "in_progress",
          conclusion: null,
        };
      },
    });
    const result = spawnSync("rust-script", ["tools/commands/ci/github/release-preflight.rs"], {
      cwd: repoRoot,
      encoding: "utf8",
      env: {
        ...process.env,
        PATH: `${fixture.binDir}${path.delimiter}${process.env.PATH ?? ""}`,
        ...fixture.env,
        TEST_JOBS_REQUEST_LOG: jobsRequestLog,
        TEST_REJECT_RUNS_AFTER_JOB_REQUEST: "1",
        TEST_RUNS_REQUEST_LOG: runsRequestLog,
      },
      timeout: 10_000,
    });

    assert.ifError(result.error);
    assert.equal(result.status, 1, `${result.stderr}\n${result.stdout}`.trim());
    assert.doesNotMatch(
      result.stderr,
      /release preflight polled workflow runs after job failure evidence/,
    );
    const runsRequests = fs.readFileSync(runsRequestLog, "utf8").trim().split("\n");
    assert.equal(runsRequests.length, 2);
    for (const runRequest of runsRequests.map((url) => new URL(url))) {
      assert.equal(runRequest.pathname, "/repos/owner/repository/actions/runs");
      assert.equal(runRequest.searchParams.get("head_sha"), sha);
    }
    assert.deepEqual(fs.readFileSync(jobsRequestLog, "utf8").trim().split("\n").filter(Boolean), [
      "106",
      "106",
    ]);
    assert.match(result.stderr, /Real Project Matrix: in_progress\/no conclusion/);
    assert.match(result.stderr, /real projects \(0\/22\)=completed\/failure/);
    assert.match(result.stderr, /real projects \(12\/22\)=in_progress\/no conclusion/);
  } finally {
    fs.rmSync(tempDir, { recursive: true, force: true });
  }
});

test("verify-only mode rejects mutation states with observed typecheck drift", () => {
  const tempDir = fs.mkdtempSync(path.join(tmpdir(), "vize-release-mutation-drift-"));
  try {
    const fixture = createReleasePreflightVerifyOnlyFixture(tempDir, {
      mutateShardEntries(shard, entries) {
        if (shard !== 0) return;
        mutateDivergence(
          entries,
          (artifact) => (artifact.mutationOracle.states[1].observed.falseNegativeCount = 1),
        );
      },
    });
    const result = spawnSync(
      "rust-script",
      ["tools/commands/ci/github/release-preflight.rs", "--verify-only"],
      {
        cwd: repoRoot,
        encoding: "utf8",
        env: {
          ...process.env,
          PATH: `${fixture.binDir}${path.delimiter}${process.env.PATH ?? ""}`,
          ...fixture.env,
        },
      },
    );
    assert.ifError(result.error);
    assert.equal(result.status, 1, `${result.stderr}\n${result.stdout}`.trim());
    assert.match(result.stderr, /seeded mutation oracle/);
  } finally {
    fs.rmSync(tempDir, { recursive: true, force: true });
  }
});

test("release metadata inventory ignores untracked package manifests", () => {
  const untrackedDirectory = path.join(repoRoot, "npm", `.preflight-untracked-${process.pid}`);
  fs.mkdirSync(untrackedDirectory, { recursive: true });
  fs.writeFileSync(
    path.join(untrackedDirectory, "package.json"),
    '{"name":"untracked-release-output","version":"9.9.9"}',
  );
  try {
    assert.equal(
      readPackageManifests().some((manifest) => manifest.path.includes(".preflight-untracked-")),
      false,
    );
  } finally {
    fs.rmSync(untrackedDirectory, { recursive: true, force: true });
  }
});
