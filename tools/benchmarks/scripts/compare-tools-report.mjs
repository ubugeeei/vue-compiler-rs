/**
 * Reporting half of the tool comparison benchmark (tools/benchmarks/scripts/compare-tools.mjs).
 *
 * Engine classes are ranked separately (#3283). A surface whose declared
 * incumbent and Vize lane run on different underlying engines — `vue-tsc` on
 * the JavaScript TypeScript compiler versus `vize check` on native tsgo/Corsa
 * — must not publish a ratio between those two rows: it would measure
 * TypeScript's Go rewrite as much as the Vue layer.
 *
 * It may still publish a ratio, as long as both sides run the same engine.
 * `IN_CLASS_BASELINES_BY_SURFACE` names the incumbents that do (#4670 added
 * them as measured rows), and `createSurface` retargets the published ratio at
 * the first of them that was measured, leaving the declared incumbent as a
 * same-run reference timing ranked inside its own engine class. Only when no
 * such incumbent ran does a cross-engine surface fall back to publishing no
 * ratio at all (`speedupStatus: "cross-engine"`).
 */

import { ENGINE_CLASSES } from "./check-gate-report.mjs";

export const CROSS_ENGINE_CELL = "n/a (cross-engine)";

/**
 * Which surfaces span engine classes, and which class each of their variants
 * belongs to. Declared here rather than at the measurement site so a recorded
 * artifact can be re-rendered (tools/benchmarks/scripts/render-results.mjs) with the same
 * classification that produced it. `large-check` is `check` re-run over one
 * large SFC, so it carries the same variant ids.
 */
export const ENGINE_CLASSES_BY_SURFACE = {
  check: {
    "golar-default": "tsgo-native",
    "golar-typecheck": "tsgo-native",
    "verter-tsc": "tsgo-native",
    "vue-tsc": "typescript-js",
    "vize-check-1t": "tsgo-native",
    "vize-check-max": "tsgo-native",
  },
  "large-check": {
    "golar-default": "tsgo-native",
    "golar-typecheck": "tsgo-native",
    "verter-tsc": "tsgo-native",
    "vue-tsc": "typescript-js",
    "vize-check-1t": "tsgo-native",
    "vize-check-max": "tsgo-native",
  },
};

/**
 * Incumbent rows a cross-engine surface may publish a ratio against, in
 * preference order. `verter-tsc` is the direct analogue of the declared
 * incumbent — a Vue type checker over the same tsconfig — differing only in
 * that it drives the same native tsgo binary Vize does, so the ratio is the
 * Vue layer alone. Golar's typecheck command is the fallback when verter-tsc
 * did not resolve. Same keys as `ENGINE_CLASSES_BY_SURFACE`.
 */
export const IN_CLASS_BASELINES_BY_SURFACE = {
  check: ["verter-tsc", "golar-typecheck"],
  "large-check": ["verter-tsc", "golar-typecheck"],
};

function assertRequiredEngineVariants(surface) {
  const expected = ENGINE_CLASSES_BY_SURFACE[surface.id];
  if (expected == null && !surface.requireEngineVariants) {
    return;
  }
  if (expected == null) {
    throw new Error(
      `compare-tools: ${surface.id} requested engine-variant coverage but has no engine class map`,
    );
  }
  const actualIds = new Set(surface.variants.map((variant) => variant.id));
  const missing = Object.keys(expected).filter((id) => !actualIds.has(id));
  if (missing.length > 0) {
    throw new Error(
      `compare-tools: ${surface.id} is missing required engine-class variants: ${missing.join(", ")}`,
    );
  }
}

export function formatSpeedup(value) {
  if (!Number.isFinite(value)) {
    return "n/a";
  }
  return `${value.toFixed(1)}x`;
}

export function getVariant(surface, id) {
  if (!id) {
    return null;
  }
  return surface.variants.find((variant) => variant.id === id) ?? null;
}

function engineClassOf(surface, id) {
  return surface.engineClasses?.[id] ?? null;
}

/**
 * Pick the row the published ratio is measured against. Same-engine surfaces
 * keep their declared incumbent; a cross-engine surface takes the first
 * measured in-class incumbent and publishes nothing when there is none.
 */
function resolveSpeedupBaseline(surface) {
  const vizeMaxClass = engineClassOf(surface, surface.vizeMaxId);
  const declaredClass = engineClassOf(surface, surface.baselineId);
  const crossEngine =
    declaredClass != null && vizeMaxClass != null && declaredClass !== vizeMaxClass;
  if (!crossEngine) {
    return { crossEngine, inClass: false, baseline: getVariant(surface, surface.baselineId) };
  }
  for (const id of IN_CLASS_BASELINES_BY_SURFACE[surface.id] ?? []) {
    const variant = getVariant(surface, id);
    if (variant != null && variant.medianMs > 0 && engineClassOf(surface, id) === vizeMaxClass) {
      return { crossEngine, inClass: true, baseline: variant };
    }
  }
  return { crossEngine, inClass: false, baseline: null };
}

/**
 * Rank the variants of one engine class fastest-first. Only used for surfaces
 * that declare `engineClasses`; every other surface keeps a single ordering.
 */
export function rankWithinEngineClasses(surface) {
  const classes = surface.engineClasses;
  if (classes == null) {
    return null;
  }
  const grouped = new Map();
  for (const variant of surface.variants) {
    const engineClass = classes[variant.id];
    if (engineClass == null) {
      throw new Error(
        `compare-tools: variant ${variant.id} of surface ${surface.id} has no engine class`,
      );
    }
    if (!grouped.has(engineClass)) {
      grouped.set(engineClass, []);
    }
    grouped.get(engineClass).push(variant);
  }
  return [...grouped.entries()].map(([engineClass, variants]) => {
    const ordered = [...variants].sort((a, b) => a.medianMs - b.medianMs);
    const fastest = ordered[0];
    return {
      engineClass,
      label: ENGINE_CLASSES[engineClass] ?? engineClass,
      rows: ordered.map((variant) => ({
        id: variant.id,
        label: variant.label,
        medianMs: variant.medianMs,
        // Ratio against the fastest row of the SAME engine class, so it is the
        // Vue layer alone and is safe to publish.
        relativeToFastest:
          fastest.medianMs > 0 ? Number((variant.medianMs / fastest.medianMs).toFixed(3)) : null,
      })),
    };
  });
}

/**
 * Attach the primary speedup, refusing to compute one across engine classes.
 */
export function createSurface(surface) {
  assertRequiredEngineVariants(surface);
  const { requireEngineVariants: _requireEngineVariants, ...outputSurface } = surface;
  const vizeMax = getVariant(surface, surface.vizeMaxId);
  const comparable = vizeMax != null && vizeMax.medianMs > 0;
  const { crossEngine, inClass, baseline } = resolveSpeedupBaseline(surface);
  const ranked = comparable && baseline != null;
  let speedupStatus = "unavailable";
  if (ranked) {
    speedupStatus = inClass ? "in-class" : "ranked";
  } else if (crossEngine && comparable) {
    speedupStatus = "cross-engine";
  }

  return {
    ...outputSurface,
    // `null`, not NaN: NaN serialises to `null` in the JSON artifact anyway, so
    // the in-memory value must say the same thing the artifact says.
    primarySpeedup: ranked ? baseline.medianMs / vizeMax.medianMs : null,
    // The row the ratio is against, so a reader never has to infer whether it
    // came from the declared incumbent or from the in-class one.
    speedupBaselineId: ranked ? baseline.id : null,
    speedupStatus,
    engineClassRanking: rankWithinEngineClasses(surface),
  };
}

function surfaceSpeedupCell(surface) {
  return surface.speedupStatus === "cross-engine"
    ? CROSS_ENGINE_CELL
    : formatSpeedup(surface.primarySpeedup);
}

export function renderSurfaceTable(surface, formatMs) {
  // The ratio and the two medians beside it must be the same comparison, so
  // the row follows the published baseline rather than the declared one.
  const baseline = getVariant(surface, surface.speedupBaselineId ?? surface.baselineId);
  const vizeSingle = getVariant(surface, surface.vizeSingleId);
  const vizeMax = getVariant(surface, surface.vizeMaxId);
  return `| ${surface.label} | ${surface.files.toLocaleString()} | ${baseline?.label ?? "n/a"} | ${formatMs(baseline?.medianMs)} | ${vizeSingle ? formatMs(vizeSingle.medianMs) : "n/a"} | ${formatMs(vizeMax?.medianMs)} | ${surfaceSpeedupCell(surface)} |`;
}

/**
 * The sentence under a cross-engine surface's ranking: either why the ratio
 * above it is safe, or why there is no ratio at all.
 */
function engineClassNote(surface) {
  const declared = getVariant(surface, surface.baselineId);
  const published = getVariant(surface, surface.speedupBaselineId);
  if (published == null) {
    return `No cross-class ratio is published for ${surface.label}: the incumbent runs the JavaScript TypeScript compiler while Vize runs native tsgo, so a single number would credit TypeScript's Go rewrite to the Vue layer.`;
  }
  return `The ${surface.label} ratio compares Vize with ${published.label}, the incumbent that runs the same native tsgo engine, so it is the Vue layer alone. ${declared?.label ?? "The JavaScript-engine incumbent"} is listed above as a same-run reference timing and never as a ratio: it drives the JavaScript TypeScript compiler, so a single number against it would credit TypeScript's Go rewrite to the Vue layer.`;
}

export function renderEngineClassSections(surfaces, formatMs) {
  const lines = [];
  for (const surface of surfaces) {
    if (surface.engineClassRanking == null) {
      continue;
    }
    lines.push(`#### ${surface.label} — engine classes ranked separately`);
    lines.push("");
    lines.push("| Engine class | Row | Median | Relative to fastest in class |");
    lines.push("| --- | --- | ---: | ---: |");
    for (const group of surface.engineClassRanking) {
      for (const row of group.rows) {
        lines.push(
          `| ${group.label} | ${row.label} | ${formatMs(row.medianMs)} | ${row.relativeToFastest == null ? "n/a" : `${row.relativeToFastest.toFixed(2)}x`} |`,
        );
      }
    }
    lines.push("");
    lines.push(engineClassNote(surface));
    lines.push("");
  }
  return lines;
}
