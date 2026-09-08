import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { test } from "vite-plus/test";

import {
  configureBundledCorsaRuntime,
  publicCorsaEnvironmentVariables,
  resolveBundledCorsaRuntime,
} from "../src/corsa-runtime.ts";

test("packed CLI prefers its compatible runtime over an older project runtime", () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "vize-cli-corsa-collision-"));
  try {
    const packageRoot = path.join(root, "node_modules", "vize");
    const platformPackage = `@typescript/typescript-${process.platform}-${process.arch}`;
    const oldRuntime = writeTypeScriptRuntime(root, platformPackage, "6.0.3", oldRuntimeSource);
    const bundledRuntime = writeTypeScriptRuntime(
      packageRoot,
      platformPackage,
      "7.0.2",
      compatibleRuntimeSource,
    );
    writeJson(path.join(packageRoot, "package.json"), {
      name: "vize",
      optionalDependencies: { [platformPackage]: "7.0.2" },
      type: "module",
    });
    writeNativeStub(packageRoot);

    const packageDir = path.dirname(fileURLToPath(import.meta.url));
    const builtCli = path.join(packageDir, "../dist/cli.mjs");
    const installedCli = path.join(packageRoot, "dist", "cli.mjs");
    fs.mkdirSync(path.dirname(installedCli), { recursive: true });
    fs.copyFileSync(builtCli, installedCli);

    const oldProbe = spawnSync(process.execPath, [oldRuntime, "--api", "--async"], {
      encoding: "utf8",
    });
    assert.equal(oldProbe.status, 2);
    assert.match(oldProbe.stderr, /flag provided but not defined: -async/u);

    const environment = { ...process.env, PROJECT_LOCAL_TSGO: oldRuntime };
    for (const name of publicCorsaEnvironmentVariables) delete environment[name];
    const run = spawnSync(process.execPath, [installedCli, "lsp"], {
      cwd: root,
      encoding: "utf8",
      env: environment,
    });

    assert.equal(run.status, 0, run.stderr);
    const result = JSON.parse(run.stdout) as {
      args: string[];
      probeStatus: number | null;
      selectedRuntime: string;
    };
    assert.deepEqual(result.args, ["lsp"]);
    assert.equal(result.selectedRuntime, fs.realpathSync(bundledRuntime));
    assert.equal(result.probeStatus, 0);
  } finally {
    fs.rmSync(root, { force: true, recursive: true });
  }
});

test("content mapper package wrapper reaches the native CLI bridge", () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "vize-cli-content-mapper-"));
  try {
    const packageRoot = path.join(root, "node_modules", "vize");
    writeJson(path.join(packageRoot, "package.json"), {
      name: "vize",
      type: "module",
    });
    writeNativeStub(
      packageRoot,
      `exports.runCli = (args) => {
  process.stdout.write(JSON.stringify({
    args,
    corsaPath: process.env.CORSA_PATH || null
  }));
};
`,
    );

    const packageDir = path.dirname(fileURLToPath(import.meta.url));
    const builtCli = path.join(packageDir, "../dist/cli.mjs");
    const installedCli = path.join(packageRoot, "dist", "cli.mjs");
    fs.mkdirSync(path.dirname(installedCli), { recursive: true });
    fs.copyFileSync(builtCli, installedCli);

    const environment = { ...process.env };
    for (const name of publicCorsaEnvironmentVariables) delete environment[name];
    const run = spawnSync(process.execPath, [installedCli, "content-mapper"], {
      cwd: root,
      encoding: "utf8",
      env: environment,
    });

    assert.equal(run.status, 0, run.stderr);
    assert.deepEqual(JSON.parse(run.stdout), {
      args: ["content-mapper"],
      corsaPath: null,
    });
  } finally {
    fs.rmSync(root, { force: true, recursive: true });
  }
});

test("all public runtime overrides retain precedence", () => {
  const fixture = createRuntimeFixture();
  try {
    for (const name of publicCorsaEnvironmentVariables) {
      const environment = { [name]: "/user/configured/tsgo" };
      const before = { ...environment };

      assert.equal(
        configureBundledCorsaRuntime(environment, { packageRoot: fixture.packageRoot }),
        null,
        name,
      );
      assert.deepEqual(environment, before, name);
    }
  } finally {
    fixture.cleanup();
  }
});

test("empty public runtime variables keep resolver fallback semantics", () => {
  const fixture = createRuntimeFixture();
  try {
    const environment = { TSGO_PATH: "" };

    const resolved = configureBundledCorsaRuntime(environment, {
      packageRoot: fixture.packageRoot,
    });

    assert.equal(resolved, fs.realpathSync(fixture.runtime));
    assert.equal(environment.CORSA_PATH, resolved);
    assert.equal(environment.TSGO_PATH, "");
  } finally {
    fixture.cleanup();
  }
});

test("missing bundled runtime leaves existing discovery untouched", () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "vize-cli-corsa-missing-"));
  try {
    const packageRoot = path.join(root, "node_modules", "vize");
    const platformPackage = `@typescript/typescript-${process.platform}-${process.arch}`;
    writeTypeScriptRuntime(root, platformPackage, "6.0.3", oldRuntimeSource);
    writeJson(path.join(packageRoot, "package.json"), {
      name: "vize",
      optionalDependencies: { [platformPackage]: "7.0.2" },
      type: "module",
    });
    const environment = {};

    assert.equal(resolveBundledCorsaRuntime({ packageRoot }), null);
    assert.equal(configureBundledCorsaRuntime(environment, { packageRoot }), null);
    assert.deepEqual(environment, {});
  } finally {
    fs.rmSync(root, { force: true, recursive: true });
  }
});

test("old bundled TypeScript platform runtime leaves existing discovery untouched", () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "vize-cli-corsa-old-runtime-"));
  try {
    const packageRoot = path.join(root, "node_modules", "vize");
    const platformPackage = `@typescript/typescript-${process.platform}-${process.arch}`;
    writeTypeScriptRuntime(packageRoot, platformPackage, "6.0.3", oldRuntimeSource);
    writeJson(path.join(packageRoot, "package.json"), {
      name: "vize",
      optionalDependencies: { [platformPackage]: "6.0.3" },
      type: "module",
    });
    const environment = {};

    assert.equal(resolveBundledCorsaRuntime({ packageRoot }), null);
    assert.equal(configureBundledCorsaRuntime(environment, { packageRoot }), null);
    assert.deepEqual(environment, {});
  } finally {
    fs.rmSync(root, { force: true, recursive: true });
  }
});

test("missing platform executable falls back without mutating the environment", () => {
  const fixture = createRuntimeFixture();
  try {
    fs.rmSync(fixture.runtime);
    const environment = {};

    assert.equal(
      configureBundledCorsaRuntime(environment, {
        packageRoot: fixture.packageRoot,
      }),
      null,
    );
    assert.deepEqual(environment, {});
  } finally {
    fixture.cleanup();
  }
});

test("unsupported platform falls back without mutating the environment", () => {
  const fixture = createRuntimeFixture();
  try {
    const environment = {};

    assert.equal(
      configureBundledCorsaRuntime(environment, {
        arch: "unsupported",
        packageRoot: fixture.packageRoot,
        platform: "aix",
      }),
      null,
    );
    assert.deepEqual(environment, {});
  } finally {
    fixture.cleanup();
  }
});

function createRuntimeFixture(): {
  cleanup(): void;
  packageRoot: string;
  runtime: string;
} {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "vize-cli-corsa-runtime-"));
  const packageRoot = path.join(root, "node_modules", "vize");
  const platformPackage = `@typescript/typescript-${process.platform}-${process.arch}`;
  const runtime = writeTypeScriptRuntime(
    packageRoot,
    platformPackage,
    "7.0.2",
    compatibleRuntimeSource,
  );
  writeJson(path.join(packageRoot, "package.json"), {
    name: "vize",
    optionalDependencies: { [platformPackage]: "7.0.2" },
    type: "module",
  });
  return {
    cleanup: () => fs.rmSync(root, { force: true, recursive: true }),
    packageRoot,
    runtime,
  };
}

function writeTypeScriptRuntime(
  packageRoot: string,
  platformPackage: string,
  version: string,
  runtimeSource: string,
): string {
  const nodeModules = path.join(packageRoot, "node_modules");
  const scope = path.join(nodeModules, "@typescript");
  const platformRoot = path.join(scope, platformPackage.replace("@typescript/", ""));
  writeJson(path.join(platformRoot, "package.json"), {
    name: platformPackage,
    version,
  });
  const runtime = path.join(platformRoot, "lib", process.platform === "win32" ? "tsc.exe" : "tsc");
  fs.mkdirSync(path.dirname(runtime), { recursive: true });
  fs.writeFileSync(runtime, runtimeSource);
  return runtime;
}

function writeNativeStub(packageRoot: string, source?: string): void {
  const nativeRoot = path.join(packageRoot, "node_modules", "@vizejs", "native");
  writeJson(path.join(nativeRoot, "package.json"), {
    main: "index.cjs",
    name: "@vizejs/native",
  });
  fs.writeFileSync(
    path.join(nativeRoot, "index.cjs"),
    source ??
      `const { spawnSync } = require("node:child_process");
exports.runCli = (args) => {
  const selectedRuntime = process.env.CORSA_PATH || process.env.PROJECT_LOCAL_TSGO;
  const probe = spawnSync(process.execPath, [selectedRuntime, "--api", "--async"], {
    encoding: "utf8",
  });
  process.stdout.write(JSON.stringify({ args, probeStatus: probe.status, selectedRuntime }));
};
`,
  );
}

function writeJson(filename: string, value: unknown): void {
  fs.mkdirSync(path.dirname(filename), { recursive: true });
  fs.writeFileSync(filename, `${JSON.stringify(value, null, 2)}\n`);
}

const oldRuntimeSource = `if (process.argv.includes("--async")) {
  process.stderr.write("flag provided but not defined: -async\\nUsage of api:\\n  -cwd string\\n");
  process.exit(2);
}
`;

const compatibleRuntimeSource = `if (!process.argv.includes("--async")) process.exit(2);
`;
