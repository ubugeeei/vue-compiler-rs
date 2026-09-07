// Self-test for the Davinci banned-assertion lint ("lint the linter",
// plan/phase-0.md P0-12). The lint enforces davinci-road/assurance.md
// "Strict oracles — no partial matching" over Rust test code; this suite
// pins (a) the exact finding set on a deliberately bad fixture and (b) a
// green run over the real tree under the committed allowlist.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const lintPath = path.join(repoRoot, "tools", "commands", "davinci", "assertion-lint.rs");
const fixtureDir = path.join(repoRoot, "tests", "_fixtures", "davinci-assertion-lint");
const allowlistPath = path.join(repoRoot, "davinci-road", "plan", "assertion-allowlist.toml");

function runLint(args: string[]) {
  return spawnSync("rust-script", [lintPath, ...args], { cwd: repoRoot, encoding: "utf8" });
}

function badFixtureFindings(): string[] {
  const fixture = fs.readFileSync(path.join(fixtureDir, "bad_example.rs"), "utf8");
  return fixture.split("\n").flatMap((line, index) => {
    const marker = /\/\/ FLAG \[([a-z-]+)\]$/.exec(line.trim());
    if (marker == null) return [];
    return [`bad_example.rs:${index + 1} [${marker[1]}] ${line.trim()}`];
  });
}

function temporaryAllowlist(expires: string, entries: string[]): string {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "vize-davinci-assertion-lint-"));
  const allowlist = path.join(dir, "allowlist.toml");
  const paths = entries.map((entry) => `  "${entry}",`).join("\n");
  fs.writeFileSync(
    allowlist,
    `[[allow]]
justification = "fixture coverage"
expires = "${expires}"
paths = [
${paths}
]
`,
  );
  return allowlist;
}

function removeTemporaryAllowlist(allowlist: string): void {
  fs.rmSync(path.dirname(allowlist), { recursive: true, force: true });
}

// Paths listed by the committed allowlist: every `[[allow]]` group carries a
// justification, an expiry, and the `paths = [ … ]` array it covers.
function allowlistPaths(): string[] {
  const allowlist = fs.readFileSync(allowlistPath, "utf8");
  const groups = allowlist.split("\n").filter((line) => line.trim() === "[[allow]]").length;
  assert.ok(groups > 0, "committed allowlist must declare [[allow]] groups");
  // Both array styles have to parse: the TOML formatter collapses a group that
  // lists a single path onto one line and expands the rest.
  return [...allowlist.matchAll(/^paths = \[([^\]]*)\]/gm)].flatMap(([, body]) =>
    [...body.matchAll(/"([^"]+)"/g)].map(([, entry]) => entry),
  );
}

test("assertion lint flags every marked weak assertion in the bad fixture, and only those", () => {
  // The fixture is the ground truth: every line carrying a `// FLAG [category]`
  // marker must be reported as `bad_example.rs:<line> [<category>] <line text>`,
  // in file order, and nothing else may appear.
  const expectedFindings = badFixtureFindings();
  assert.equal(expectedFindings.length, 5, "fixture must carry all five banned categories");

  const result = runLint(["--root", fixtureDir]);
  assert.equal(result.status, 1, result.stderr);
  assert.equal(result.stderr, "");
  assert.equal(
    result.stdout,
    `${expectedFindings.join("\n")}\n` +
      "assertion-lint: 5 unlisted findings — fix the assertion (exact oracles only) " +
      "or triage via davinci-road/plan/assertion-allowlist.toml\n",
  );
});

test("explicit allowlist suppresses fixture findings only before expiry", () => {
  const expectedFindings = badFixtureFindings();
  const freshAllowlist = temporaryAllowlist("2999-12-31", ["bad_example.rs"]);
  try {
    const result = runLint(["--root", fixtureDir, "--allowlist", freshAllowlist]);
    assert.equal(result.stderr, "");
    assert.equal(result.status, 0, result.stdout);
    assert.equal(
      result.stdout,
      `assertion-lint: OK (${expectedFindings.length} findings in 1 files suppressed by allowlist)\n`,
    );
  } finally {
    removeTemporaryAllowlist(freshAllowlist);
  }

  const expiredAllowlist = temporaryAllowlist("1970-01-01", ["bad_example.rs"]);
  try {
    const result = runLint(["--root", fixtureDir, "--allowlist", expiredAllowlist]);
    assert.equal(result.stderr, "");
    assert.equal(result.status, 1, result.stdout);
    assert.equal(
      result.stdout,
      expectedFindings.map((finding) => `${finding} (allowlist entry expired)`).join("\n") +
        "\nassertion-lint: 5 unlisted findings — fix the assertion (exact oracles only) " +
        "or triage via davinci-road/plan/assertion-allowlist.toml\n",
    );
  } finally {
    removeTemporaryAllowlist(expiredAllowlist);
  }
});

test("assertion lint exits 0 on the real tree under the committed allowlist", () => {
  const entryCount = allowlistPaths().length;
  assert.ok(entryCount > 0, "committed allowlist must not be empty while the debt exists");

  const result = runLint([]);
  // stderr stays empty: the lint warns there about allowlist entries that no
  // longer match any finding, so a fully-cleaned file must also drop its
  // entry (the list only shrinks — assurance doctrine ratchet).
  assert.equal(result.stderr, "");
  assert.equal(result.status, 0, result.stdout);
  const summary =
    /^assertion-lint: OK \((\d+) findings in (\d+) files suppressed by allowlist\)\n$/.exec(
      result.stdout,
    );
  assert.ok(summary, `unexpected lint output: ${result.stdout}`);
  assert.equal(
    Number(summary[2]),
    entryCount,
    "every allowlist entry must suppress at least one finding",
  );
});

test("committed allowlist entries point at files that still exist", () => {
  const paths = allowlistPaths();
  assert.ok(paths.length > 0, "allowlist must declare [[allow]] paths");
  for (const entry of paths) {
    assert.ok(
      fs.existsSync(path.join(repoRoot, entry)),
      `allowlist entry ${entry} points at a missing file — remove or update it`,
    );
  }
});
