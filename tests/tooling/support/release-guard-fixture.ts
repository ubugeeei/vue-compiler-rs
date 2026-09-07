import fs from "node:fs";
import os from "node:os";
import path from "node:path";

import { runMoonScript } from "../_helpers/moonbit.ts";
import { writeFakeCommand } from "./fake-command.ts";

export interface RepositoryGuardOptions {
  branch: string;
  dirty?: boolean;
  ancestor?: boolean;
  headSha?: string;
  remoteSha?: string;
  parentLine?: string;
  localTagExists?: boolean;
  remoteTagExists?: boolean;
  pushFails?: boolean;
  stagedFiles?: boolean;
  manifestPrecheckFails?: boolean;
  manifestTestFails?: boolean;
  guardFails?: boolean;
}

export function runRepositoryGuardFixture(options: RepositoryGuardOptions) {
  const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), "vize-release-guard-"));
  const binDir = path.join(tempDir, "bin");
  const gitLogPath = path.join(tempDir, "git.log");
  const nodeLogPath = path.join(tempDir, "node.log");
  const manifestTestCountPath = path.join(tempDir, "manifest-test-count.txt");
  const guardShimPath = path.join(tempDir, "release-local-guard-shim.mjs");
  const cargoTomlPath = path.join(tempDir, "Cargo.toml");
  const cargoToml = '[workspace.package]\nversion = "0.290.0"\n';
  fs.mkdirSync(binDir, { recursive: true });
  fs.mkdirSync(path.join(tempDir, "npm"));
  fs.mkdirSync(path.join(tempDir, "tests/tooling"), { recursive: true });
  fs.writeFileSync(gitLogPath, "");
  fs.writeFileSync(nodeLogPath, "");
  fs.writeFileSync(manifestTestCountPath, "0");
  fs.writeFileSync(cargoTomlPath, cargoToml);
  fs.writeFileSync(path.join(tempDir, "pnpm-workspace.yaml"), "");
  fs.writeFileSync(path.join(tempDir, "pnpm-lock.yaml"), "");
  fs.writeFileSync(
    path.join(tempDir, "tests/tooling/package-manifests.test.ts"),
    options.manifestTestFails ? 'throw new Error("manifest drift");\n' : "",
  );
  fs.writeFileSync(
    guardShimPath,
    "process.exit(process.env.TEST_GUARD_FAILS === 'true' ? 1 : 0);\n",
  );
  writeFakeCommand(binDir, "cargo", "process.exit(0);");
  writeFakeCommand(
    binDir,
    "git",
    [
      "const fs = require('node:fs');",
      "const args = process.argv.slice(2);",
      "fs.appendFileSync(process.env.GIT_LOG, args.join(' ') + '\\n');",
      "if (args[0] === 'branch') { console.log(process.env.TEST_BRANCH); process.exit(0); }",
      "if (args[0] === 'status') { if (process.env.TEST_DIRTY === 'true') console.log(' M Cargo.toml'); process.exit(0); }",
      "if (args[0] === 'fetch') process.exit(0);",
      "if (args[0] === 'merge-base') process.exit(process.env.TEST_ANCESTOR === 'false' ? 1 : 0);",
      "if (args[0] === 'rev-list') { console.log(process.env.TEST_PARENT_LINE); process.exit(0); }",
      "if (args[0] === 'rev-parse' && args.includes('--verify')) process.exit(process.env.LOCAL_TAG_EXISTS === 'true' ? 0 : 1);",
      "if (args[0] === 'rev-parse') { console.log(args.at(-1) === 'HEAD' ? process.env.TEST_HEAD_SHA : process.env.TEST_REMOTE_SHA); process.exit(0); }",
      "if (args[0] === 'ls-remote' && process.env.REMOTE_TAG_EXISTS === 'true') { console.log('bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\\t' + args.at(-1)); process.exit(0); }",
      "if (args[0] === 'ls-remote') process.exit(2);",
      "if (args[0] === 'diff' && args.includes('--cached')) { if (process.env.TEST_STAGED_FILES !== 'false') console.log('Cargo.toml'); process.exit(0); }",
      "if (args[0] === 'push') process.exit(process.env.TEST_PUSH_FAIL === 'true' ? 1 : 0);",
      "if (['add', 'commit', 'tag'].includes(args[0])) process.exit(0);",
      "process.exit(1);",
    ].join("\n"),
  );
  writeFakeCommand(
    binDir,
    "fake-node",
    [
      "const fs = require('node:fs');",
      "const args = process.argv.slice(2);",
      "fs.appendFileSync(process.env.NODE_LOG, args.join(' ') + '\\n');",
      "if (args[0] !== '--test' || args[1] !== 'tests/tooling/package-manifests.test.ts') process.exit(1);",
      "const countPath = process.env.MANIFEST_TEST_COUNT;",
      "const count = Number(fs.readFileSync(countPath, 'utf8')) + 1;",
      "fs.writeFileSync(countPath, String(count));",
      "if (process.env.TEST_MANIFEST_PRECHECK_FAILS === 'true' && count === 1) process.exit(1);",
      "if (process.env.TEST_MANIFEST_TEST_FAILS === 'true' && count > 1) process.exit(1);",
      "process.exit(0);",
    ].join("\n"),
  );

  const result = runMoonScript("release", ["patch", "-y"], {
    cwd: tempDir,
    env: {
      PATH: `${binDir}${path.delimiter}${process.env.PATH ?? ""}`,
      GIT_LOG: gitLogPath,
      TEST_BRANCH: options.branch,
      TEST_DIRTY: String(options.dirty ?? false),
      TEST_ANCESTOR: String(options.ancestor ?? true),
      TEST_HEAD_SHA: options.headSha ?? "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
      TEST_REMOTE_SHA: options.remoteSha ?? "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
      TEST_PARENT_LINE:
        options.parentLine ??
        `${options.headSha ?? "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"} bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb`,
      LOCAL_TAG_EXISTS: String(options.localTagExists ?? false),
      REMOTE_TAG_EXISTS: String(options.remoteTagExists),
      TEST_PUSH_FAIL: String(options.pushFails ?? false),
      TEST_STAGED_FILES: String(options.stagedFiles ?? true),
      TEST_MANIFEST_PRECHECK_FAILS: String(options.manifestPrecheckFails ?? false),
      TEST_MANIFEST_TEST_FAILS: String(options.manifestTestFails ?? false),
      TEST_GUARD_FAILS: String(options.guardFails ?? false),
      VIZE_RELEASE_GUARD_SCRIPT: guardShimPath,
      VIZE_RELEASE_GUARD_RUNNER: process.execPath,
      VIZE_RELEASE_NODE: path.join(binDir, "fake-node"),
      NODE_LOG: nodeLogPath,
      MANIFEST_TEST_COUNT: manifestTestCountPath,
    },
  });
  const gitLog = fs.readFileSync(gitLogPath, "utf8");
  const nodeLog = fs.readFileSync(nodeLogPath, "utf8");
  return { cargoToml, cargoTomlPath, gitLog, nodeLog, result, tempDir };
}
