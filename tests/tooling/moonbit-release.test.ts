import assert from "node:assert/strict";
import { execSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";

import { repoRoot, runMoonScript } from "./_helpers/moonbit.ts";
import { runRepositoryGuardFixture } from "./support/release-guard-fixture.ts";

function writeTempFile(contents: string): string {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "vize-release-test-"));
  const file = path.join(dir, "input.yaml");
  fs.writeFileSync(file, contents);
  return file;
}

const cargoToml = fs.readFileSync(path.join(repoRoot, "Cargo.toml"), "utf8");
const currentVersion = cargoToml.match(/^version = "(.+)"$/m)?.[1];

assert.ok(currentVersion, "Failed to read current version from Cargo.toml");

test("release script fails clearly when stdin is not interactive", () => {
  const result = runMoonScript("release", ["minor"]);

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /Error: Confirmation requires an interactive terminal\. Re-run with -y to skip the prompt\.\n$/,
  );
  assert.match(
    result.stdout,
    new RegExp(
      `^Current version: ${currentVersion.replaceAll(".", "\\.")}\\nNew version: .+ \\(tag: v.+\\)\\n\\n$`,
    ),
  );
});

test("release script clears prerelease suffixes for stable bumps", () => {
  const cases = [
    ["1.2.3-alpha.1", "patch", "1.2.4"],
    ["1.2.3-beta", "minor", "1.3.0"],
    ["1.2.3-rc.1", "major", "2.0.0"],
    ["1.2.3-alpha.1", "release", "1.2.3"],
    ["1.2.3-alpha.1", "alpha", "1.2.3-alpha.2"],
  ] as const;

  for (const [current, bump, expected] of cases) {
    const result = runMoonScript("release", ["--print-bump", current, bump]);

    assert.equal(result.status, 0, `${result.stderr}\n${result.stdout}`.trim());
    assert.equal(result.stdout.trim(), expected);
  }
});

test("release script rewrites native workspace pins and minimum release age excludes", () => {
  const workspaceYaml = [
    "minimumReleaseAgeExclude:",
    '  - "@vizejs/native-darwin-arm64@0.100.0 || 0.106.0"',
    '  - "@vizejs/native-darwin-x64@0.100.0 || 0.106.0"',
    '  - "@vizejs/native-linux-x64-gnu@0.100.0 || 0.106.0 || 0.107.0"',
    '  - "@scope/not-native@0.106.0"',
    "",
    "catalogs:",
    "  repo-tooling:",
    '    "@iarna/toml": "2.2.5"',
    "  some-other:",
    '    "@vizejs/native-darwin-arm64": "0.106.0"',
    "  # Published native binary packages.",
    "  native-binaries:",
    '    "@vizejs/native-darwin-arm64": "0.106.0"',
    '    "@vizejs/native-darwin-x64": "0.106.0"',
    '    "@vizejs/native-linux-arm64-gnu": "0.106.0"',
    "",
    "peerDependencyRules:",
    "  allowAny:",
    '    - "*"',
    "",
  ].join("\n");
  const inputPath = writeTempFile(workspaceYaml);

  const result = runMoonScript("release", [
    "--print-workspace-catalog-update",
    inputPath,
    "0.106.0",
    "0.107.0",
  ]);

  assert.equal(result.status, 0, `${result.stderr}\n${result.stdout}`);
  const lines = result.stdout.split("\n");

  assert.ok(
    result.stdout.includes('  - "@vizejs/native-darwin-arm64@0.100.0 || 0.106.0 || 0.107.0"'),
    "minimumReleaseAgeExclude keeps darwin arm64 aligned",
  );
  assert.ok(
    result.stdout.includes('  - "@vizejs/native-darwin-x64@0.100.0 || 0.106.0 || 0.107.0"'),
    "minimumReleaseAgeExclude keeps darwin x64 aligned",
  );
  assert.ok(
    result.stdout.includes('  - "@vizejs/native-linux-x64-gnu@0.100.0 || 0.106.0 || 0.107.0"'),
    "minimumReleaseAgeExclude does not duplicate an existing release version",
  );
  assert.ok(
    result.stdout.includes('  - "@scope/not-native@0.106.0"'),
    "non-native minimumReleaseAgeExclude entries are preserved",
  );

  const otherCatalogLine = lines.find((line) =>
    line.startsWith('    "@vizejs/native-darwin-arm64": '),
  );
  assert.ok(otherCatalogLine, "first native-darwin-arm64 line preserved");
  assert.equal(
    otherCatalogLine,
    '    "@vizejs/native-darwin-arm64": "0.106.0"',
    "non-native-binaries catalog must not be rewritten",
  );

  const nativeBlockStart = lines.indexOf("  native-binaries:");
  assert.notEqual(nativeBlockStart, -1, "native-binaries header preserved");
  assert.equal(lines[nativeBlockStart + 1], '    "@vizejs/native-darwin-arm64": "0.107.0"');
  assert.equal(lines[nativeBlockStart + 2], '    "@vizejs/native-darwin-x64": "0.107.0"');
  assert.equal(lines[nativeBlockStart + 3], '    "@vizejs/native-linux-arm64-gnu": "0.107.0"');

  assert.ok(result.stdout.includes("peerDependencyRules:"), "later sections preserved");
});

test("release script rewrites only the native-binaries catalog block in pnpm-lock.yaml", () => {
  const lockfile = [
    "catalogs:",
    "  linting:",
    "    oxlint:",
    "      specifier: 1.64.0",
    "      version: 1.64.0",
    "  native-binaries:",
    "    '@vizejs/native-darwin-arm64':",
    "      specifier: 0.106.0",
    "      version: 0.106.0",
    "    '@vizejs/native-darwin-x64':",
    "      specifier: 0.106.0",
    "      version: 0.106.0",
    "  oxc:",
    "    oxc-transform:",
    "      specifier: 0.130.0",
    "      version: 0.130.0",
    "importers:",
    "  npm/native:",
    "    optionalDependencies:",
    "      '@vizejs/native-darwin-arm64':",
    "        specifier: catalog:native-binaries",
    "        version: 0.106.0",
    "packages:",
    "  '@vizejs/native-darwin-arm64@0.106.0':",
    "    resolution: {integrity: sha512-AAA==}",
    "",
  ].join("\n");
  const inputPath = writeTempFile(lockfile);

  const result = runMoonScript("release", [
    "--print-lockfile-catalog-update",
    inputPath,
    "0.106.0",
    "0.107.0",
  ]);

  assert.equal(result.status, 0, `${result.stderr}\n${result.stdout}`);
  const out = result.stdout;

  assert.match(
    out,
    /native-binaries:\n {4}'@vizejs\/native-darwin-arm64':\n {6}specifier: 0\.107\.0\n {6}version: 0\.107\.0\n/,
  );
  assert.match(
    out,
    / {4}'@vizejs\/native-darwin-x64':\n {6}specifier: 0\.107\.0\n {6}version: 0\.107\.0\n/,
  );

  assert.match(out, /linting:\n {4}oxlint:\n {6}specifier: 1\.64\.0\n {6}version: 1\.64\.0\n/);

  assert.ok(
    out.includes("        version: 0.106.0"),
    "project importer version (six-space indent) preserved",
  );
  assert.ok(
    out.includes("'@vizejs/native-darwin-arm64@0.106.0':"),
    "packages section key preserved",
  );
  assert.ok(out.includes("resolution: {integrity: sha512-AAA==}"), "integrity hash preserved");
});

test("release version sweep covers every manifest the preflight verifies", () => {
  const result = runMoonScript("release", ["--print-extra-package-json-paths"]);

  assert.equal(result.status, 0, `${result.stderr}\n${result.stdout}`);
  const extraPaths = new Set(result.stdout.split("\n").filter(Boolean));

  // The release preflight fails the release when any tracked, non-private
  // manifest under these roots disagrees with the release version, so the
  // bump tool must reach exactly that set: flat npm/<package> manifests via
  // its directory scan, everything else via the extra list. A package added
  // to a nested group (npm/compose, npm/ui, ...) that misses this sweep
  // aborts the release at the preflight — this pins the alignment.
  const tracked = execSync("git ls-files -z -- editors npm", { cwd: repoRoot })
    .toString()
    .split("\0")
    .filter((relativePath) => relativePath.endsWith("/package.json"));
  for (const manifestPath of tracked) {
    const manifest = JSON.parse(fs.readFileSync(path.join(repoRoot, manifestPath), "utf8")) as {
      private?: boolean;
    };
    if (manifest.private === true) {
      assert.ok(
        !extraPaths.has(manifestPath),
        `${manifestPath} is private and must stay out of the release version sweep`,
      );
      continue;
    }
    const coveredByFlatScan = /^npm\/[^/]+\/package\.json$/.test(manifestPath);
    if (!coveredByFlatScan) {
      assert.ok(
        extraPaths.has(manifestPath),
        `${manifestPath} must be bumped with release commits (preflight verifies it)`,
      );
    }
  }

  for (const manifestPath of extraPaths) {
    assert.ok(
      tracked.includes(manifestPath),
      `${manifestPath} is in the release sweep but not tracked under editors/ or npm/`,
    );
  }
});

test("release script creates immutable tags and pushes main and tag atomically", () => {
  const fixture = runRepositoryGuardFixture({ branch: "main" });

  try {
    assert.equal(fixture.result.status, 0, fixture.result.stderr);
    assert.match(fixture.gitLog, /^commit --no-verify -m chore: release v0\.290\.1$/m);
    assert.match(fixture.gitLog, /^tag -a v0\.290\.1 -m Release 0\.290\.1$/m);
    assert.match(fixture.gitLog, /^push --atomic origin main refs\/tags\/v0\.290\.1$/m);
    assert.doesNotMatch(fixture.gitLog, /--force-tag|(?:^|\s)--force(?:\s|$)|--allow-empty/);
  } finally {
    fs.rmSync(fixture.tempDir, { recursive: true, force: true });
  }
});

test("release script accepts an exact merge commit at the main tip", () => {
  const fixture = runRepositoryGuardFixture({
    branch: "main",
    parentLine:
      "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb cccccccccccccccccccccccccccccccccccccccc",
  });

  try {
    assert.equal(fixture.result.status, 0, fixture.result.stderr);
    assert.doesNotMatch(fixture.gitLog, /^rev-list\b/m);
    assert.match(fixture.result.stdout, /Release 0\.290\.1 complete!/);
  } finally {
    fs.rmSync(fixture.tempDir, { recursive: true, force: true });
  }
});

test("release script explains local cleanup after an atomic push failure", () => {
  const fixture = runRepositoryGuardFixture({ branch: "main", pushFails: true });

  try {
    assert.equal(fixture.result.status, 1);
    assert.match(fixture.result.stderr, /Failed to atomically push main and the release tag/);
    assert.match(fixture.result.stderr, /git tag -d v0\.290\.1/);
    assert.match(fixture.result.stderr, /git reset --hard origin\/main/);
  } finally {
    fs.rmSync(fixture.tempDir, { recursive: true, force: true });
  }
});

test("release script rejects the removed force-tag escape hatch", () => {
  const result = runMoonScript("release", ["patch", "-y", "--force-tag"]);

  assert.equal(result.status, 1);
  assert.match(result.stderr, /--force-tag is not supported; published release tags are immutable/);
});

test("release script refuses to create an empty release commit", () => {
  const fixture = runRepositoryGuardFixture({ branch: "main", stagedFiles: false });

  try {
    assert.equal(fixture.result.status, 1);
    assert.match(fixture.result.stderr, /No release changes were staged/);
    assert.doesNotMatch(fixture.gitLog, /^(?:commit|tag|push)\b/m);
  } finally {
    fs.rmSync(fixture.tempDir, { recursive: true, force: true });
  }
});

test("release script explains cleanup after manifest verification fails", () => {
  const fixture = runRepositoryGuardFixture({ branch: "main", manifestTestFails: true });

  try {
    assert.equal(fixture.result.status, 1);
    assert.match(fixture.result.stderr, /package manifest alignment tests failed/);
    assert.match(fixture.result.stderr, /git reset --hard origin\/main/);
    assert.match(fixture.gitLog, /^commit --no-verify -m chore: release v0\.290\.1$/m);
    assert.doesNotMatch(fixture.gitLog, /^(?:tag|push)\b/m);
  } finally {
    fs.rmSync(fixture.tempDir, { recursive: true, force: true });
  }
});

test("release script checks manifest alignment before repository mutation", () => {
  const fixture = runRepositoryGuardFixture({ branch: "main", manifestPrecheckFails: true });

  try {
    assert.equal(fixture.result.status, 1);
    assert.match(
      fixture.result.stderr,
      /package manifest alignment tests failed before repository mutation/,
    );
    assert.match(fixture.nodeLog, /^--test tests\/tooling\/package-manifests\.test\.ts$/m);
    assert.doesNotMatch(fixture.gitLog, /^(?:add|commit|tag|push)\b/m);
    assert.equal(fs.readFileSync(fixture.cargoTomlPath, "utf8"), fixture.cargoToml);
  } finally {
    fs.rmSync(fixture.tempDir, { recursive: true, force: true });
  }
});

test("release script stops before mutation when the local guard fails", () => {
  const fixture = runRepositoryGuardFixture({ branch: "main", guardFails: true });

  try {
    assert.equal(fixture.result.status, 1);
    assert.match(fixture.result.stderr, /Release preflight failed before repository mutation/);
    assert.doesNotMatch(fixture.gitLog, /^(?:add|commit|tag|push)\b/m);
    assert.equal(fs.readFileSync(fixture.cargoTomlPath, "utf8"), fixture.cargoToml);
  } finally {
    fs.rmSync(fixture.tempDir, { recursive: true, force: true });
  }
});
