import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { test } from "node:test";
import { pathToFileURL } from "node:url";
import { firstLocation, isDiagnosticsForUri, offsetToPosition } from "./support/lsp/assertions.ts";
import { root, testOutputRoot } from "./support/lsp/paths.ts";
import type { ServerCapabilities } from "./support/lsp/protocol.ts";
import { LspSession } from "./support/lsp/session.ts";
import {
  requireTypecheckDependency,
  resolveTypecheckRuntime,
} from "./support/typecheck-dependency.ts";

const source = `<script setup lang="ts">
const message = "hello"
</script>

<template>
  <span>{{ message }}</span>
</template>
`;

const tsxSource = `const message = "hello"

export const view = <span>{message}</span>
`;

type DeclarationCapabilities = ServerCapabilities & {
  declarationProvider?: unknown;
};

test("vize lsp maps textDocument/declaration from authored SFC template bindings", async () => {
  const testRootDir = path.join(testOutputRoot, "lsp-declaration-capability");
  fs.mkdirSync(testRootDir, { recursive: true });
  const workspaceDir = fs.mkdtempSync(path.join(testRootDir, "workspace-"));
  fs.writeFileSync(
    path.join(workspaceDir, "tsconfig.json"),
    JSON.stringify(
      {
        compilerOptions: {
          strict: true,
          target: "ES2022",
          module: "ESNext",
          moduleResolution: "bundler",
          noEmit: true,
        },
        include: ["**/*"],
      },
      null,
      2,
    ),
  );
  const filePath = path.join(workspaceDir, "Widget.vue");
  const uri = pathToFileURL(filePath).href;
  const session = new LspSession();

  try {
    const init = (await session.initialize(workspaceDir, {
      editor: true,
      lint: false,
      typecheck: true,
    })) as { capabilities?: DeclarationCapabilities };
    assert.equal(init.capabilities?.declarationProvider, true);

    fs.writeFileSync(filePath, source, "utf8");
    session.notify("textDocument/didOpen", {
      textDocument: { uri, languageId: "vue", version: 1, text: source },
    });
    await session.waitForNotification(
      "textDocument/publishDiagnostics",
      (params) => isDiagnosticsForUri(params, uri),
      60_000,
    );

    const position = offsetToPosition(source, source.lastIndexOf("message }}</span>") + 3);
    const declaration = (await session.request("textDocument/declaration", {
      textDocument: { uri },
      position,
    })) as Array<{ uri: string; range: { start: { line: number; character: number } } }>;
    const location = firstLocation(declaration);
    assert.equal(location.uri, uri);
    assert.deepEqual(location.range.start, offsetToPosition(source, source.indexOf("message =")));
    assert.ok(!location.uri.endsWith(".vue.ts"), JSON.stringify(declaration));
  } finally {
    await session.shutdown();
    fs.rmSync(workspaceDir, { recursive: true, force: true });
    fs.rmSync(testRootDir, { recursive: true, force: true });
  }
});

test("vize lsp maps textDocument/declaration from authored TSX expressions", async (t) => {
  const corsaPath = requireTypecheckDependency(
    t,
    resolveTypecheckRuntime(root),
    "TypeScript 7/Corsa runtime for TSX declaration navigation",
    "TypeScript 7/Corsa runtime not found; skipping TSX declaration navigation test",
  );
  if (corsaPath == null) return;

  const testRootDir = path.join(testOutputRoot, "lsp-tsx-declaration-capability");
  fs.mkdirSync(testRootDir, { recursive: true });
  const workspaceDir = fs.mkdtempSync(path.join(testRootDir, "workspace-"));
  const sourceDir = path.join(workspaceDir, "src");
  fs.mkdirSync(sourceDir, { recursive: true });
  writeVueShim(sourceDir);
  fs.writeFileSync(
    path.join(workspaceDir, "vize.config.json"),
    JSON.stringify({
      lsp: { lint: false, typecheck: true },
      typeChecker: { corsaPath, jsxTypecheck: true },
    }),
    "utf8",
  );
  fs.writeFileSync(
    path.join(workspaceDir, "tsconfig.json"),
    JSON.stringify(
      {
        compilerOptions: {
          jsx: "preserve",
          jsxImportSource: "vue",
          module: "ESNext",
          moduleResolution: "bundler",
          noEmit: true,
          strict: true,
          target: "ES2022",
        },
        include: ["src/**/*"],
      },
      null,
      2,
    ),
  );
  const filePath = path.join(sourceDir, "Widget.tsx");
  const uri = pathToFileURL(filePath).href;
  const session = new LspSession();

  try {
    const init = (await session.initialize(workspaceDir, {
      editor: true,
      lint: false,
      typecheck: true,
    })) as { capabilities?: DeclarationCapabilities };
    assert.equal(init.capabilities?.declarationProvider, true);

    fs.writeFileSync(filePath, tsxSource, "utf8");
    session.notify("textDocument/didOpen", {
      textDocument: { uri, languageId: "typescriptreact", version: 1, text: tsxSource },
    });
    await session.waitForNotification(
      "textDocument/publishDiagnostics",
      (params) => isDiagnosticsForUri(params, uri),
      60_000,
    );

    const position = offsetToPosition(tsxSource, tsxSource.lastIndexOf("message}</span>") + 3);
    const declaration = (await session.request("textDocument/declaration", {
      textDocument: { uri },
      position,
    })) as Array<{ uri: string; range: { start: { line: number; character: number } } }>;
    const location = firstLocation(declaration);
    assert.equal(location.uri, uri);
    assert.deepEqual(
      location.range.start,
      offsetToPosition(tsxSource, tsxSource.indexOf("message =")),
    );
    assert.ok(!location.uri.endsWith(".jsx.ts"), JSON.stringify(declaration));
  } finally {
    await session.shutdown();
    fs.rmSync(workspaceDir, { recursive: true, force: true });
    fs.rmSync(testRootDir, { recursive: true, force: true });
  }
});

function writeVueShim(sourceDir: string): void {
  fs.writeFileSync(
    path.join(sourceDir, "vue-shim.d.ts"),
    `declare namespace JSX {
  interface IntrinsicElements {
    span: { children?: unknown }
  }
}

declare module "vue/jsx-runtime" {
  export const Fragment: unique symbol
  export function jsx(type: unknown, props: unknown): unknown
  export function jsxs(type: unknown, props: unknown): unknown
}
`,
    "utf8",
  );
}
