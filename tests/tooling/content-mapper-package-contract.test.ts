import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import {
  assertInstalledMapperContract,
  runInstalledContentMapperChecks,
} from "../../legacy-tools/npm/smoke-release-runtime.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");

function withInstalledMapper(run: (installDir: string, packageRoot: string) => void): void {
  const installDir = fs.mkdtempSync(path.join(os.tmpdir(), "vize-mapper-contract-"));
  const packageRoot = path.join(installDir, "node_modules", "vize");
  const mapperPath = path.join(packageRoot, "bin", "vize");
  try {
    fs.mkdirSync(path.dirname(mapperPath), { recursive: true });
    fs.writeFileSync(
      path.join(packageRoot, "package.json"),
      JSON.stringify({
        name: "vize",
        typescript: {
          contentMapper: {
            exec: ["node", "./bin/vize", "content-mapper"],
            compilerOptions: ["noUnusedLocals"],
          },
        },
      }),
    );
    fs.writeFileSync(mapperPath, "#!/usr/bin/env node\n");
    fs.chmodSync(mapperPath, 0o755);
    run(installDir, packageRoot);
  } finally {
    fs.rmSync(installDir, { force: true, recursive: true });
  }
}

test("installed Content Mapper contract accepts the production package shape", () => {
  withInstalledMapper((installDir) => assertInstalledMapperContract(installDir));
});

test("installed Content Mapper contract rejects a corrupted exec entry", () => {
  withInstalledMapper((installDir, packageRoot) => {
    const manifestPath = path.join(packageRoot, "package.json");
    const manifest = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
    manifest.typescript.contentMapper.exec = ["node", "./bin/missing", "content-mapper"];
    fs.writeFileSync(manifestPath, JSON.stringify(manifest));

    assert.throws(
      () => assertInstalledMapperContract(installDir),
      /must expose the production typescript\.contentMapper contract/,
    );
  });
});

// Upstream microsoft/typescript-go@e72cbeaaa moved the manifest block from the
// top-level "tsContentMapper" key to "typescript.contentMapper". A package still
// shipping the retired key resolves to no mapper at all (TS100035), so the
// contract must fail closed rather than treat the old shape as equivalent.
test("installed Content Mapper contract rejects the retired tsContentMapper key", () => {
  withInstalledMapper((installDir, packageRoot) => {
    const manifestPath = path.join(packageRoot, "package.json");
    const manifest = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
    manifest.tsContentMapper = manifest.typescript.contentMapper;
    delete manifest.typescript;
    fs.writeFileSync(manifestPath, JSON.stringify(manifest));

    assert.throws(
      () => assertInstalledMapperContract(installDir),
      /must expose the production typescript\.contentMapper contract/,
    );
  });
});

// "compilerOptions" is the only channel that carries noUnusedLocals into the
// transform; dropping it silently changes which TS6133 diagnostics Vize emits.
test("installed Content Mapper contract rejects a dropped compilerOptions declaration", () => {
  withInstalledMapper((installDir, packageRoot) => {
    const manifestPath = path.join(packageRoot, "package.json");
    const manifest = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
    delete manifest.typescript.contentMapper.compilerOptions;
    fs.writeFileSync(manifestPath, JSON.stringify(manifest));

    assert.throws(
      () => assertInstalledMapperContract(installDir),
      /must expose the production typescript\.contentMapper contract/,
    );
  });
});

// Upstream microsoft/typescript-go@08475bbcc removed manifest-declared extensions
// in favour of a per-transform "extension" field, and the manifest parser no
// longer reads the key. Shipping it would advertise a contract upstream ignores.
test("installed Content Mapper contract rejects a retired extensions declaration", () => {
  withInstalledMapper((installDir, packageRoot) => {
    const manifestPath = path.join(packageRoot, "package.json");
    const manifest = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
    manifest.typescript.contentMapper.extensions = { ".vue": ".tsx" };
    fs.writeFileSync(manifestPath, JSON.stringify(manifest));

    assert.throws(
      () => assertInstalledMapperContract(installDir),
      /must expose the production typescript\.contentMapper contract/,
    );
  });
});

test("installed Content Mapper contract rejects a missing executable", () => {
  withInstalledMapper((installDir, packageRoot) => {
    fs.rmSync(path.join(packageRoot, "bin", "vize"));
    assert.throws(() => assertInstalledMapperContract(installDir), /bin[/\\]vize must exist/);
  });
});

test("installed Content Mapper release smoke compares declarations with TypeScript 7 tsc", () => {
  withInstalledMapper((installDir) => {
    const tsgo = path.join(installDir, "tsgo-stub.mjs");
    fs.writeFileSync(
      tsgo,
      [
        "#!/usr/bin/env node",
        "if (process.argv.includes('-p') && process.argv.includes('tsconfig.error.json')) {",
        "  process.stdout.write('errors/Broken.vue TS2322\\n');",
        "  process.stdout.write('errors/JavaScriptConsumer.js TS2322\\n');",
        "  process.stdout.write('errors/JsxConsumer.jsx TS2322\\n');",
        "  process.stdout.write('src/Unused.vue TS6133\\n');",
        "  process.exit(1);",
        "}",
        "process.exit(0);",
        "",
      ].join("\n"),
    );
    fs.chmodSync(tsgo, 0o755);

    const tscBin = path.join(installDir, "node_modules", "typescript", "bin", "tsc");
    fs.mkdirSync(path.dirname(tscBin), { recursive: true });
    fs.writeFileSync(tscBin, "#!/usr/bin/env node\n");
    fs.chmodSync(tscBin, 0o755);

    const calls: { args: string[]; command: string; cwd?: string }[] = [];
    const before = process.env.VIZE_TEST_CONTENT_MAPPER_TSGO;
    process.env.VIZE_TEST_CONTENT_MAPPER_TSGO = tsgo;
    try {
      runInstalledContentMapperChecks(installDir, root, (command, args, options) => {
        calls.push({ args, command, cwd: options.cwd });
        if (args.includes("tsconfig.emit.json")) {
          const dist = path.join(options.cwd, "dist");
          fs.mkdirSync(dist, { recursive: true });
          fs.writeFileSync(path.join(dist, "App.vue.d.ts"), "export {}\n");
          fs.writeFileSync(path.join(dist, "main.d.ts"), "export {}\n");
        }
      });
    } finally {
      if (before == null) {
        delete process.env.VIZE_TEST_CONTENT_MAPPER_TSGO;
      } else {
        process.env.VIZE_TEST_CONTENT_MAPPER_TSGO = before;
      }
    }

    assert.ok(
      calls.some(
        (call) =>
          call.command === process.execPath &&
          call.args[0] === tscBin &&
          call.args.includes("dist/verify.ts"),
      ),
      "declaration consumer smoke must compare against the installed TypeScript 7 bin/tsc launcher",
    );
  });
});
