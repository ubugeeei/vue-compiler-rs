import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const command = path.join(repoRoot, "tools/commands/davinci/consumer-migration-surfaces.rs");
const legacyFiles = [
  "legacy-tools/davinci/consumer-migration-surfaces.mjs",
  "legacy-tools/davinci/lib/consumer-migration-render.mjs",
  "legacy-tools/davinci/lib/consumer-migration-scan.mjs",
  "legacy-tools/davinci/lib/markdown.mjs",
  "legacy-tools/davinci/lib/paths.mjs",
  "legacy-tools/davinci/lib/rust-source.mjs",
];
const scannedCrates = [
  "vize_atelier_core",
  "vize_atelier_dom",
  "vize_atelier_sfc",
  "vize_atelier_ssr",
  "vize_atelier_vapor",
  "vize_atelier_jsx",
  "vize_patina",
  "vize_canon",
  "vize_glyph",
  "vize_maestro",
];

function writeFile(root, relPath, content) {
  const target = path.join(root, relPath);
  fs.mkdirSync(path.dirname(target), { recursive: true });
  fs.writeFileSync(target, content);
}

function copyFile(root, relPath) {
  const target = path.join(root, relPath);
  fs.mkdirSync(path.dirname(target), { recursive: true });
  fs.copyFileSync(path.join(repoRoot, relPath), target);
}

function createScratchRepo() {
  const scratch = fs.mkdtempSync(path.join(os.tmpdir(), "vize-consumer-surfaces-"));
  writeFile(scratch, "Cargo.toml", "[workspace]\n");
  writeFile(scratch, "pnpm-workspace.yaml", "packages: []\n");
  for (const relPath of legacyFiles) copyFile(scratch, relPath);
  for (const crate of scannedCrates) {
    writeFile(
      scratch,
      `crates/${crate}/Cargo.toml`,
      `[package]\nname = "${crate}"\nversion = "0.0.0"\nedition = "2024"\n`,
    );
  }
  writeFile(scratch, "crates/vize_atelier_core/src/lib.rs", "use vize_s0::Root;\n");
  return scratch;
}

function runSurfaceCommand(root, mode) {
  return spawnSync("rust-script", [command, mode], {
    cwd: root,
    encoding: "utf8",
    env: { ...process.env, VIZE_REPO_ROOT: root },
  });
}

void test("consumer migration surface check fails on an injected stale artifact", () => {
  const scratch = createScratchRepo();
  try {
    const write = runSurfaceCommand(scratch, "--write");
    assert.equal(write.status, 0, `${write.stdout}${write.stderr}`.trim());

    const clean = runSurfaceCommand(scratch, "--check");
    assert.equal(clean.status, 0, `${clean.stdout}${clean.stderr}`.trim());

    fs.appendFileSync(
      path.join(scratch, "davinci-road/plan/consumer-migration-surfaces.tsv"),
      "injected\tedit\n",
    );
    const stale = runSurfaceCommand(scratch, "--check");

    assert.equal(stale.status, 1, `--check accepted stale artifacts:\n${stale.stdout}`);
    assert.match(
      stale.stderr,
      /stale: davinci-road\/plan\/consumer-migration-surfaces\.tsv drifted/,
    );
    assert.match(
      stale.stderr,
      /Regenerate with: rust-script tools\/commands\/davinci\/consumer-migration-surfaces\.rs --write/,
    );
  } finally {
    fs.rmSync(scratch, { recursive: true, force: true });
  }
});
