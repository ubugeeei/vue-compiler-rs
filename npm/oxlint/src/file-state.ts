import fs from "node:fs";

import type { Context } from "@oxlint/plugins";

import { lintPatina } from "./binding.js";
import type { PatinaDiagnostic, PatinaRuleOptions, SfcBlock, SingleScriptMap } from "./model.js";
import { extractSfcBlocks } from "./sfc-blocks.js";
import { createSingleScriptMap } from "./script-map.js";
import {
  getCacheKey,
  getVizeSettings,
  isIncrementalPreset,
  isTypeAwareRuleName,
} from "./settings.js";
import { isScriptLikeFile } from "./file-kinds.js";
import { resolveWorkaroundSource } from "./workaround.js";

export interface FileState {
  readonly revision: SourceRevisionIdentity;
  filename: string;
  source: string;
  extractedScript: string;
  usesOriginalLocations: boolean;
  reportedDiagnostics: Set<string>;
  sfcBlocks: readonly SfcBlock[] | undefined;
  scriptMap: SingleScriptMap | null | undefined;
  allDiagnosticsByRule: Map<string, PatinaDiagnostic[]> | null;
  allDiagnosticsIncludesTypeAware: boolean;
  partialDiagnosticsByRule: Map<string, readonly PatinaDiagnostic[]>;
  requestedRules: Set<string>;
  reportedTypeAwareRuntimeDiagnostic: boolean;
}

interface SourceRevisionIdentity {
  // Exact text avoids hash collisions and per-rule digest allocations. For
  // normal files physicalSource and state.source share the same string reference.
  readonly physicalSource: string;
}

interface CachedFileState {
  state: FileState;
  lastUsed: number;
}

// A watch process only needs the active working set, not every file ever seen.
const FILE_STATE_CACHE_CAPACITY = 128;
const fileStateCache = new Map<string, CachedFileState>();
const EMPTY_DIAGNOSTICS: readonly PatinaDiagnostic[] = [];
const TYPE_AWARE_RUNTIME_RULE = "type/corsa-runtime";
let fileStateCacheClock = 0;

export function getFileState(context: Context): FileState {
  const settings = getVizeSettings(context);
  const physicalSource = fs.readFileSync(context.physicalFilename, "utf8");
  const resolvedSource = resolveWorkaroundSource(physicalSource, context.physicalFilename);
  const cacheKey = getCacheKey(resolvedSource.filename, settings);
  const cached = fileStateCache.get(cacheKey);
  if (cached && cached.state.revision.physicalSource === physicalSource) {
    if (cached.state.extractedScript !== context.sourceCode.text) {
      // A dual-script SFC is visited with multiple extracted programs for the
      // same physical revision. Diagnostics/reporting remain shared to avoid
      // duplicates; only the extracted-program location map becomes stale.
      cached.state.extractedScript = context.sourceCode.text;
      cached.state.scriptMap = undefined;
    }
    cached.lastUsed = nextFileStateCacheClock();
    return cached.state;
  }

  const state: FileState = {
    revision: {
      physicalSource,
    },
    filename: resolvedSource.filename,
    source: resolvedSource.source,
    extractedScript: context.sourceCode.text,
    usesOriginalLocations:
      resolvedSource.usesOriginalLocations || isScriptLikeFile(resolvedSource.filename),
    reportedDiagnostics: new Set(),
    sfcBlocks: undefined,
    scriptMap: undefined,
    allDiagnosticsByRule: null,
    allDiagnosticsIncludesTypeAware: false,
    partialDiagnosticsByRule: new Map(),
    requestedRules: new Set(),
    reportedTypeAwareRuntimeDiagnostic: false,
  };
  fileStateCache.set(cacheKey, {
    state,
    lastUsed: nextFileStateCacheClock(),
  });
  evictLeastRecentlyUsedFileState();
  return state;
}

export function clearFileStateCache(): void {
  fileStateCache.clear();
  fileStateCacheClock = 0;
}

export function getFileStateCacheStats(): { capacity: number; entries: number } {
  return {
    capacity: FILE_STATE_CACHE_CAPACITY,
    entries: fileStateCache.size,
  };
}

export function getDiagnosticsForRule(
  context: Context,
  state: FileState,
  ruleName: string,
  ruleOptions?: PatinaRuleOptions,
): readonly PatinaDiagnostic[] {
  if (ruleOptions != null) {
    return getConfiguredRuleDiagnostics(context, state, ruleName, ruleOptions);
  }

  if (state.allDiagnosticsByRule) {
    if (isTypeAwareRuleName(ruleName) && !state.allDiagnosticsIncludesTypeAware) {
      const settings = getVizeSettings(context);
      state.allDiagnosticsByRule = indexDiagnosticsByRule(
        lintPatina(state.source, state.filename, { ...settings, typeAware: true }).diagnostics,
      );
      state.allDiagnosticsIncludesTypeAware = true;
    }

    return diagnosticsForRule(state, state.allDiagnosticsByRule, ruleName);
  }

  const cached = state.partialDiagnosticsByRule.get(ruleName);
  if (cached) {
    return cached;
  }

  const settings = getVizeSettings(context);
  const ruleSettings = isTypeAwareRuleName(ruleName) ? { ...settings, typeAware: true } : settings;
  if (isIncrementalPreset(settings)) {
    const diagnostics = lintPatina(state.source, state.filename, ruleSettings, [
      ruleName,
    ]).diagnostics;
    const indexedDiagnostics = indexDiagnosticsByRule(diagnostics);
    const ruleDiagnostics = diagnosticsForRule(state, indexedDiagnostics, ruleName);
    state.partialDiagnosticsByRule.set(ruleName, ruleDiagnostics);
    return ruleDiagnostics;
  }

  if (state.requestedRules.size === 0) {
    state.requestedRules.add(ruleName);
    const diagnostics = lintPatina(state.source, state.filename, ruleSettings, [
      ruleName,
    ]).diagnostics;
    const indexedDiagnostics = indexDiagnosticsByRule(diagnostics);
    const ruleDiagnostics = diagnosticsForRule(state, indexedDiagnostics, ruleName);
    state.partialDiagnosticsByRule.set(ruleName, ruleDiagnostics);
    return ruleDiagnostics;
  }

  state.requestedRules.add(ruleName);
  const fullPassTypeAware =
    settings.typeAware === true || Array.from(state.requestedRules).some(isTypeAwareRuleName);
  const allDiagnostics = lintPatina(state.source, state.filename, {
    ...settings,
    typeAware: fullPassTypeAware,
  }).diagnostics;
  state.allDiagnosticsByRule = indexDiagnosticsByRule(allDiagnostics);
  state.allDiagnosticsIncludesTypeAware = fullPassTypeAware;
  state.partialDiagnosticsByRule.clear();
  return diagnosticsForRule(state, state.allDiagnosticsByRule, ruleName);
}

function getConfiguredRuleDiagnostics(
  context: Context,
  state: FileState,
  ruleName: string,
  ruleOptions: PatinaRuleOptions,
): readonly PatinaDiagnostic[] {
  const cacheKey = configuredRuleCacheKey(ruleName, ruleOptions);
  const cached = state.partialDiagnosticsByRule.get(cacheKey);
  if (cached) {
    return cached;
  }

  const settings = getVizeSettings(context);
  const ruleSettings = isTypeAwareRuleName(ruleName) ? { ...settings, typeAware: true } : settings;
  const diagnostics = lintPatina(
    state.source,
    state.filename,
    ruleSettings,
    [ruleName],
    ruleOptions,
  ).diagnostics;
  const indexedDiagnostics = indexDiagnosticsByRule(diagnostics);
  const ruleDiagnostics = diagnosticsForRule(state, indexedDiagnostics, ruleName);
  state.partialDiagnosticsByRule.set(cacheKey, ruleDiagnostics);
  return ruleDiagnostics;
}

function configuredRuleCacheKey(ruleName: string, ruleOptions: PatinaRuleOptions): string {
  return [
    ruleName,
    ruleOptions.componentNameInTemplateCasing ?? "",
    ruleOptions.customEventNameCasing ?? "",
    ruleOptions.noMutatingProps == null ? "" : noMutatingPropsCacheKey(ruleOptions.noMutatingProps),
    ruleOptions.sfcElementOrder == null ? "" : sfcElementOrderCacheKey(ruleOptions.sfcElementOrder),
    ruleOptions.htmlSelfClosing == null ? "" : htmlSelfClosingCacheKey(ruleOptions.htmlSelfClosing),
    ruleOptions.vOnEventHyphenation ?? "",
    ruleOptions.attributeHyphenation ?? "",
  ].join("\0");
}

function noMutatingPropsCacheKey(
  options: NonNullable<PatinaRuleOptions["noMutatingProps"]>,
): string {
  return options.shallowOnly === true ? "1" : "0";
}

function sfcElementOrderCacheKey(
  options: NonNullable<PatinaRuleOptions["sfcElementOrder"]>,
): string {
  return (options.order ?? [])
    .map((group) => (Array.isArray(group) ? group.join("\u001f") : group))
    .join("\u001e");
}

function htmlSelfClosingCacheKey(
  options: NonNullable<PatinaRuleOptions["htmlSelfClosing"]>,
): string {
  return [
    options.html?.void ?? "",
    options.html?.normal ?? "",
    options.html?.component ?? "",
    options.svg ?? "",
    options.math ?? "",
  ].join("/");
}

export function getScriptMap(state: FileState): SingleScriptMap | null {
  if (state.scriptMap !== undefined) {
    return state.scriptMap;
  }

  state.scriptMap = createSingleScriptMap(state.source, state.extractedScript, getSfcBlocks(state));
  return state.scriptMap;
}

export function getSfcBlocks(state: FileState): readonly SfcBlock[] {
  if (state.sfcBlocks !== undefined) {
    return state.sfcBlocks;
  }

  state.sfcBlocks = extractSfcBlocks(state.source);
  return state.sfcBlocks;
}

export function markDiagnosticAsReported(state: FileState, diagnostic: PatinaDiagnostic): boolean {
  const key = diagnosticIdentity(diagnostic);
  if (state.reportedDiagnostics.has(key)) {
    return false;
  }

  state.reportedDiagnostics.add(key);
  return true;
}

function diagnosticIdentity(diagnostic: PatinaDiagnostic): string {
  const { start, end } = diagnostic.location;
  return [
    diagnostic.rule,
    diagnostic.severity,
    diagnostic.message,
    start.line,
    start.column,
    start.offset,
    end.line,
    end.column,
    end.offset,
  ].join("\0");
}

function nextFileStateCacheClock(): number {
  fileStateCacheClock += 1;
  return fileStateCacheClock;
}

function evictLeastRecentlyUsedFileState(): void {
  if (fileStateCache.size <= FILE_STATE_CACHE_CAPACITY) {
    return;
  }

  let oldestKey: string | undefined;
  let oldestUse = Number.POSITIVE_INFINITY;
  for (const [key, cached] of fileStateCache) {
    if (cached.lastUsed < oldestUse) {
      oldestKey = key;
      oldestUse = cached.lastUsed;
    }
  }

  if (oldestKey !== undefined) {
    fileStateCache.delete(oldestKey);
  }
}

function indexDiagnosticsByRule(
  diagnostics: readonly PatinaDiagnostic[],
): Map<string, PatinaDiagnostic[]> {
  const grouped = new Map<string, PatinaDiagnostic[]>();

  for (const diagnostic of diagnostics) {
    const existing = grouped.get(diagnostic.rule);
    if (existing) {
      existing.push(diagnostic);
      continue;
    }

    grouped.set(diagnostic.rule, [diagnostic]);
  }

  return grouped;
}

function diagnosticsForRule(
  state: FileState,
  diagnosticsByRule: Map<string, PatinaDiagnostic[]>,
  ruleName: string,
): readonly PatinaDiagnostic[] {
  const directDiagnostics = diagnosticsByRule.get(ruleName);
  if (directDiagnostics && directDiagnostics.length > 0) {
    return directDiagnostics;
  }

  if (!isTypeAwareRuleName(ruleName) || state.reportedTypeAwareRuntimeDiagnostic) {
    return EMPTY_DIAGNOSTICS;
  }

  const runtimeDiagnostics = diagnosticsByRule.get(TYPE_AWARE_RUNTIME_RULE);
  if (!runtimeDiagnostics || runtimeDiagnostics.length === 0) {
    return EMPTY_DIAGNOSTICS;
  }

  state.reportedTypeAwareRuntimeDiagnostic = true;
  return runtimeDiagnostics;
}
