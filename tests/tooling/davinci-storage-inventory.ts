import {
  emptyFileStorage,
  storageKinds,
  type FileStorage,
  type StorageKind,
  type StorageMeasurement,
} from "./davinci-storage-scan.ts";

export type StorageScope = "infra" | "s1" | "s2" | "s1_to_s2";
export type VecCategory = "contract" | "analysis" | "lower" | "pass" | "emit";
export type InventoryRow = {
  scope: StorageScope;
  category?: VecCategory;
  file: string;
  storage: FileStorage;
};
export type StorageSummary = StorageMeasurement & { files: number };
export type ScopeSummary = Record<StorageKind, StorageSummary>;

const scopes: StorageScope[] = ["infra", "s1", "s2", "s1_to_s2"];
const categories: VecCategory[] = ["contract", "analysis", "lower", "pass", "emit"];
const header = [
  "scope",
  "category",
  "file",
  "alloc_vec_direct",
  "alloc_vec_bound",
  "s0_string_direct",
  "s0_string_bound",
  "arena_vec_direct",
  "arena_vec_bound",
  "small_vec_direct",
  "small_vec_bound",
].join("\t");

export const categoryReasons: Record<VecCategory, string> = {
  contract: "variable-length owned Folio/S2 contract data",
  analysis: "unbounded diagnostics, lookup storage, and traversal results",
  lower: "source-sized lowering worklists and owned results",
  pass: "source-sized pass facts, provenance, and traversal worklists",
  emit: "ordered emitter buffers whose size follows the document",
};

export const expectedProductionAllocVec: StorageSummary = {
  files: 77,
  directPaths: 89,
  boundUses: 305,
};

function count(value: string, line: number): number {
  if (!/^\d+$/u.test(value)) throw new Error(`line ${line}: invalid count ${value}`);
  return Number(value);
}

export function parseStorageInventory(source: string): InventoryRow[] {
  const lines = source.trimEnd().split("\n");
  if (lines.shift() !== header) throw new Error("storage inventory header drifted");
  const files = new Set<string>();
  return lines.map((line, index) => {
    const fields = line.split("\t");
    if (fields.length !== 11) throw new Error(`line ${index + 2}: expected 11 fields`);
    const [scope, category, file, ...values] = fields;
    if (!scopes.includes(scope as StorageScope)) throw new Error(`line ${index + 2}: bad scope`);
    if (category !== "-" && !categories.includes(category as VecCategory)) {
      throw new Error(`line ${index + 2}: bad category`);
    }
    if (files.has(file)) throw new Error(`line ${index + 2}: duplicate ${file}`);
    files.add(file);
    const storage = emptyFileStorage();
    const counts = values.map((value) => count(value, index + 2));
    [storage.allocVec.directPaths, storage.allocVec.boundUses] = counts.slice(0, 2);
    [storage.s0String.directPaths, storage.s0String.boundUses] = counts.slice(2, 4);
    [storage.arenaVec.directPaths, storage.arenaVec.boundUses] = counts.slice(4, 6);
    [storage.smallVec.directPaths, storage.smallVec.boundUses] = counts.slice(6, 8);
    const hasAllocVec = storage.allocVec.directPaths > 0 || storage.allocVec.boundUses > 0;
    if (hasAllocVec !== (category !== "-")) {
      throw new Error(`line ${index + 2}: alloc Vec category mismatch`);
    }
    return {
      scope: scope as StorageScope,
      category: category === "-" ? undefined : (category as VecCategory),
      file,
      storage,
    };
  });
}

function emptySummary(): StorageSummary {
  return { files: 0, directPaths: 0, boundUses: 0 };
}

function emptyScopeSummary(): ScopeSummary {
  return Object.fromEntries(storageKinds.map((kind) => [kind, emptySummary()])) as ScopeSummary;
}

export function summarizeScopes(rows: readonly InventoryRow[]): Record<StorageScope, ScopeSummary> {
  const result = Object.fromEntries(scopes.map((scope) => [scope, emptyScopeSummary()])) as Record<
    StorageScope,
    ScopeSummary
  >;
  for (const row of rows) {
    for (const kind of storageKinds) {
      const measurement = row.storage[kind];
      if (measurement.directPaths === 0 && measurement.boundUses === 0) continue;
      const summary = result[row.scope][kind];
      summary.files += 1;
      summary.directPaths += measurement.directPaths;
      summary.boundUses += measurement.boundUses;
    }
  }
  return result;
}

export function summarizeAllocVecCategories(
  rows: readonly InventoryRow[],
): Record<VecCategory, StorageSummary> {
  const result = Object.fromEntries(
    categories.map((category) => [category, emptySummary()]),
  ) as Record<VecCategory, StorageSummary>;
  for (const row of rows) {
    if (!row.category) continue;
    const summary = result[row.category];
    summary.files += 1;
    summary.directPaths += row.storage.allocVec.directPaths;
    summary.boundUses += row.storage.allocVec.boundUses;
  }
  return result;
}

export function summarizeKind(rows: readonly InventoryRow[], kind: StorageKind): StorageSummary {
  const summary = emptySummary();
  for (const row of rows) {
    const measurement = row.storage[kind];
    if (measurement.directPaths === 0 && measurement.boundUses === 0) continue;
    summary.files += 1;
    summary.directPaths += measurement.directPaths;
    summary.boundUses += measurement.boundUses;
  }
  return summary;
}
