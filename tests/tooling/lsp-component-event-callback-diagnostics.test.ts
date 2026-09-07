import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { test } from "node:test";
import { pathToFileURL } from "node:url";

import { isDiagnosticsForUri, offsetToPosition } from "./support/lsp/assertions.ts";
import { root, testOutputRoot } from "./support/lsp/paths.ts";
import type { PublishDiagnosticsParams } from "./support/lsp/protocol.ts";
import { LspSession } from "./support/lsp/session.ts";
import {
  requireTypecheckDependency,
  resolveTypecheckRuntime,
} from "./support/typecheck-dependency.ts";

test("vize lsp preserves global component event callback diagnostics", async (t) => {
  const corsaPath = requireTypecheckDependency(
    t,
    resolveTypecheckRuntime(root),
    "TypeScript 7/Corsa runtime for global component event LSP diagnostics",
    "TypeScript 7/Corsa runtime not found; skipping global component event LSP diagnostics",
  );
  if (corsaPath == null) return;
  const vuePackage = requireTypecheckDependency(
    t,
    resolveVuePackage(),
    "Vue package for global component event LSP diagnostics",
    "Vue package not found; skipping global component event LSP diagnostics",
  );
  if (vuePackage == null) return;

  const testRootDir = path.join(testOutputRoot, "lsp-component-event-callback-diagnostics");
  fs.mkdirSync(testRootDir, { recursive: true });
  const workspaceDir = fs.mkdtempSync(path.join(testRootDir, "workspace-"));
  const session = new LspSession();
  let initialized = false;

  try {
    const sourceDir = path.join(workspaceDir, "src");
    fs.mkdirSync(sourceDir, { recursive: true });
    linkVuePackage(workspaceDir, vuePackage);
    writeProjectConfig(workspaceDir, corsaPath);

    fs.writeFileSync(
      path.join(sourceDir, "global-components.d.ts"),
      `import type { DefineComponent } from "vue"
export {}

declare module "vue" {
  interface GlobalComponents {
    MkSuspense: DefineComponent<{
      onResolved?: (result: { file: string }) => void
    }>
  }
}
`,
      "utf8",
    );
    const source = `<template>
  <MkSuspense @resolved="(result) => result.file.length" />
  <MkSuspense @resolved="(result) => result.missing" />
</template>
`;
    const filePath = path.join(sourceDir, "App.vue");
    const uri = pathToFileURL(filePath).href;
    fs.writeFileSync(filePath, source, "utf8");

    await session.initialize(workspaceDir, {
      editor: true,
      lint: false,
      typecheck: true,
    });
    initialized = true;
    session.notify("textDocument/didOpen", {
      textDocument: { uri, languageId: "vue", version: 1, text: source },
    });

    const publish = (await session.waitForNotification(
      "textDocument/publishDiagnostics",
      (params) =>
        isDiagnosticsForUri(params, uri) &&
        params.diagnostics.some((diagnostic) => diagnostic.code === 2339),
      120_000,
    )) as PublishDiagnosticsParams;
    const messages = publish.diagnostics.map((diagnostic) => diagnostic.message ?? "");
    const codes = publish.diagnostics.map((diagnostic) => diagnostic.code);
    const missingStart = offsetToPosition(
      source,
      source.indexOf("result.missing") + "result.".length,
    );

    assert.deepEqual(codes, [2339], messages.join("\n"));
    assert.equal(
      publish.diagnostics[0].message?.startsWith(
        "Property 'missing' does not exist on type '{ file: string; }'.",
      ),
      true,
      publish.diagnostics[0].message,
    );
    assert.deepEqual(
      {
        code: publish.diagnostics[0].code,
        range: publish.diagnostics[0].range,
        severity: publish.diagnostics[0].severity,
        source: publish.diagnostics[0].source,
      },
      {
        code: 2339,
        range: {
          start: missingStart,
          end: { line: missingStart.line, character: missingStart.character + "missing".length },
        },
        severity: 1,
        source: "vize/types",
      },
    );
    assert.equal(
      messages.some((message) => message.includes("implicitly has an 'any' type")),
      false,
      messages.join("\n"),
    );
  } finally {
    if (initialized) {
      await session.shutdown();
    } else {
      await session.kill().catch(() => undefined);
    }
    fs.rmSync(workspaceDir, { recursive: true, force: true });
    fs.rmSync(testRootDir, { recursive: true, force: true });
  }
});

function resolveVuePackage(): string | undefined {
  return [
    path.join(root, "node_modules/vue"),
    path.join(root, "tests/node_modules/vue"),
    path.join(root, "playground/node_modules/vue"),
  ].find((candidate) => fs.existsSync(candidate));
}

function linkVuePackage(workspaceDir: string, vuePackage: string): void {
  const nodeModules = path.join(workspaceDir, "node_modules");
  fs.mkdirSync(nodeModules, { recursive: true });
  symlink(vuePackage, path.join(nodeModules, "vue"));

  const vueNamespace = path.join(path.dirname(vuePackage), "@vue");
  if (fs.existsSync(vueNamespace)) {
    symlink(vueNamespace, path.join(nodeModules, "@vue"));
  }
}

function symlink(source: string, target: string): void {
  fs.rmSync(target, { force: true, recursive: true });
  fs.symlinkSync(source, target, process.platform === "win32" ? "junction" : "dir");
}

function writeProjectConfig(workspaceDir: string, corsaPath: string): void {
  fs.writeFileSync(
    path.join(workspaceDir, "vize.config.json"),
    JSON.stringify({
      lsp: { lint: false, typecheck: true },
      typeChecker: { corsaPath },
    }),
    "utf8",
  );
  fs.writeFileSync(
    path.join(workspaceDir, "tsconfig.json"),
    JSON.stringify({
      compilerOptions: {
        module: "ESNext",
        moduleResolution: "bundler",
        noEmit: true,
        strict: true,
        target: "ES2022",
      },
      include: ["src/**/*"],
    }),
    "utf8",
  );
}
