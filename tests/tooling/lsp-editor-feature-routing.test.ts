import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { test } from "node:test";
import { pathToFileURL } from "node:url";
import { testOutputRoot } from "./support/lsp/paths.ts";
import type { LspInitializationOptions } from "./support/lsp/protocol.ts";
import { LspSession } from "./support/lsp/session.ts";

const ALL_EDITOR_FEATURES_OFF: LspInitializationOptions = {
  codeActions: false,
  codeLens: false,
  completion: false,
  definition: false,
  documentLinks: false,
  documentSymbols: false,
  editor: true,
  fileRename: false,
  autoInsert: false,
  foldingRanges: false,
  formatting: false,
  hover: false,
  inlayHints: false,
  lint: false,
  references: false,
  rename: false,
  semanticTokens: false,
  signatureHelp: false,
  typecheck: false,
  workspaceSymbols: false,
};

type RequestCase = {
  method: string;
  params: (uri: string, workspaceDir: string) => unknown;
  expected?: unknown;
};

const position = { line: 1, character: 7 };
const range = { start: position, end: position };

const DISABLED_REQUESTS: RequestCase[] = [
  {
    method: "textDocument/hover",
    params: (uri) => ({ textDocument: { uri }, position }),
  },
  {
    method: "textDocument/declaration",
    params: (uri) => ({ textDocument: { uri }, position }),
  },
  {
    method: "textDocument/definition",
    params: (uri) => ({ textDocument: { uri }, position }),
  },
  {
    method: "textDocument/typeDefinition",
    params: (uri) => ({ textDocument: { uri }, position }),
  },
  {
    method: "textDocument/implementation",
    params: (uri) => ({ textDocument: { uri }, position }),
  },
  {
    method: "textDocument/references",
    params: (uri) => ({ textDocument: { uri }, position, context: { includeDeclaration: true } }),
  },
  {
    method: "textDocument/documentHighlight",
    params: (uri) => ({ textDocument: { uri }, position }),
  },
  {
    method: "textDocument/completion",
    params: (uri) => ({ textDocument: { uri }, position }),
  },
  {
    method: "textDocument/signatureHelp",
    params: (uri) => ({
      textDocument: { uri },
      position,
      context: { triggerKind: 1, isRetrigger: false },
    }),
  },
  {
    method: "textDocument/documentSymbol",
    params: (uri) => ({ textDocument: { uri } }),
  },
  {
    method: "textDocument/documentLink",
    params: (uri) => ({ textDocument: { uri } }),
  },
  {
    method: "textDocument/documentColor",
    params: (uri) => ({ textDocument: { uri } }),
    expected: [],
  },
  {
    method: "textDocument/colorPresentation",
    params: (uri) => ({
      textDocument: { uri },
      color: { red: 1, green: 0, blue: 0, alpha: 1 },
      range,
    }),
    expected: [],
  },
  {
    method: "textDocument/semanticTokens/full",
    params: (uri) => ({ textDocument: { uri } }),
  },
  {
    method: "textDocument/semanticTokens/range",
    params: (uri) => ({ textDocument: { uri }, range }),
  },
  {
    method: "textDocument/codeLens",
    params: (uri) => ({ textDocument: { uri } }),
  },
  {
    method: "textDocument/inlayHint",
    params: (uri) => ({ textDocument: { uri }, range }),
  },
  {
    method: "textDocument/foldingRange",
    params: (uri) => ({ textDocument: { uri } }),
  },
  {
    method: "textDocument/selectionRange",
    params: (uri) => ({ textDocument: { uri }, positions: [position] }),
  },
  {
    method: "textDocument/codeAction",
    params: (uri) => ({ textDocument: { uri }, range, context: { diagnostics: [] } }),
  },
  {
    method: "textDocument/prepareRename",
    params: (uri) => ({ textDocument: { uri }, position }),
  },
  {
    method: "textDocument/rename",
    params: (uri) => ({ textDocument: { uri }, position, newName: "renamedMessage" }),
  },
  {
    method: "textDocument/linkedEditingRange",
    params: (uri) => ({ textDocument: { uri }, position }),
  },
  {
    method: "textDocument/formatting",
    params: (uri) => ({ textDocument: { uri }, options: { tabSize: 2, insertSpaces: true } }),
  },
  {
    method: "textDocument/rangeFormatting",
    params: (uri) => ({
      textDocument: { uri },
      range,
      options: { tabSize: 2, insertSpaces: true },
    }),
  },
  {
    method: "textDocument/onTypeFormatting",
    params: (uri) => ({
      textDocument: { uri },
      position,
      ch: "}",
      options: { tabSize: 2, insertSpaces: true },
    }),
  },
  {
    method: "textDocument/prepareCallHierarchy",
    params: (uri) => ({ textDocument: { uri }, position }),
  },
  {
    method: "volar/client/autoInsert",
    params: (uri) => ({
      textDocument: { uri },
      selection: position,
      change: { rangeOffset: 0, rangeLength: 0, text: ">" },
    }),
  },
  {
    method: "workspace/symbol",
    params: () => ({ query: "message" }),
  },
  {
    method: "workspace/willRenameFiles",
    params: (_uri, workspaceDir) => ({
      files: [
        {
          oldUri: pathToFileURL(path.join(workspaceDir, "Widget.vue")).href,
          newUri: pathToFileURL(path.join(workspaceDir, "RenamedWidget.vue")).href,
        },
      ],
    }),
  },
];

test("vize lsp honors explicit editor feature disables for live requests", async () => {
  const testRootDir = path.join(testOutputRoot, "lsp-editor-feature-routing");
  fs.mkdirSync(testRootDir, { recursive: true });
  const workspaceDir = fs.mkdtempSync(path.join(testRootDir, "workspace-"));
  const session = new LspSession();

  try {
    await session.initialize(workspaceDir, ALL_EDITOR_FEATURES_OFF);

    const source = `<script setup lang="ts">
const message = "hello"
</script>

<template>
  <span>{{ message }}</span>
</template>
`;
    const filePath = path.join(workspaceDir, "Widget.vue");
    const uri = pathToFileURL(filePath).href;
    fs.writeFileSync(filePath, source, "utf8");
    session.notify("textDocument/didOpen", {
      textDocument: {
        uri,
        languageId: "vue",
        version: 1,
        text: source,
      },
    });

    for (const request of DISABLED_REQUESTS) {
      const response = await session.request(request.method, request.params(uri, workspaceDir));
      assert.deepEqual(response, request.expected ?? null, `${request.method} should be inert`);
    }
  } finally {
    await session.shutdown();
    fs.rmSync(workspaceDir, { recursive: true, force: true });
    fs.rmSync(testRootDir, { recursive: true, force: true });
  }
});
