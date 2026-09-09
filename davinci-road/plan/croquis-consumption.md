<!-- GENERATED FILE — do not edit by hand.
     Regenerate: rust-script tools/commands/davinci/croquis-consumers.rs --write
     Verify:     rust-script tools/commands/davinci/croquis-consumers.rs --check
     Generator:  tools/davinci/croquis-consumers.mjs -->

# Croquis consumption matrix

Which workspace crates consume the public analysis products of `crates/vize_croquis`. Mechanizes the 2026-08-13 hand audit in [semantic-engine.md](../semantic-engine.md#the-problem-measured) (Davinci P0-7).

## Resolution method (and its limits)

**Product enumeration** — parsed from source, not hardcoded:

- `pub` fields of the `Croquis` struct in `crates/vize_croquis/src/croquis.rs` (rows named `Croquis.<field>`), plus the tracker/product types those fields reference, resolved through croquis.rs's own `use crate::…` declarations.
- Types re-exported by croquis.rs (`pub use bindings::…`, `snapshot::…`, …) and the crate-root `pub use` groups in `crates/vize_croquis/src/lib.rs` whose source is a local module (this is what brings in the `effect_graph`, `scope`, `symbol`, `analyzer`, `drawer`, and `reactivity_overlay` families).
- Crate-root passthrough re-exports of foreign items are **excluded** from the product set: `vize_carton::is_builtin_directive`, `vize_carton::is_builtin_tag`, `vize_carton::is_html_tag`, `vize_carton::is_math_ml_tag`, `vize_carton::is_native_tag`, `vize_carton::is_reserved_prop`, `vize_carton::is_svg_tag`, `vize_carton::is_void_tag`, `vize_relief::BindingType`.

**Consumer resolution** — symbol-aware, per `crates/*/src/**/*.rs`:

- Rust `use` declarations are parsed (brace groups, `as` aliases, `pub use`) into per-file alias tables mapping local names to `vize_croquis` items; `pub use` re-export chains across crates are followed to a fixpoint (name-level, see limits). Comments and string literals are stripped before any counting, and the `use` declarations themselves are not counted as reference sites (a `pub use` re-export counts as one site).
- **type rows** — sites are references to a resolved local alias, a module-qualified member (`reactivity::ReactiveKind`), or a fully qualified `vize_croquis::…` path.
- **`Croquis.<field>` rows** — sites are field accesses (`summary.bindings`) counted only on receivers resolved to `Croquis` values: idents with a `Croquis` type annotation (params, struct fields, `let`, `&`/`&'a`/`&mut`/`Option<&…>`/`Box`/`Rc`/`Arc` wrappers), calls to same-file functions returning `Croquis`, and `let`-bindings whose right-hand side calls a workspace `pub fn` returning `Croquis` (`drawer.finish()`, `ctx.analysis()`), reads a workspace `pub` field typed `Croquis` (`result.croquis`), or calls an associated function on the `Croquis` type itself (`Croquis::default()`) — producer tables parsed from `crates/*/src`, matched by name. Inline chains through those producers (`entry.analysis.race_conditions`, `ctx.croquis().bindings`) are counted too.

**Known limits** (undercounts are possible; the grep lane below bounds them):

- Re-export chains resolve by item **name**, not full module path; same-named items reached through different facade modules would be conflated.
- No type inference: field accesses through closure params, iterator chains, destructuring patterns, or re-borrowed locals (`let b = &a;`) are not counted; the producer tables match croquis-returning method **names** without owner types, so a same-named method on an unrelated type can mark a false receiver (only matters if that value also has a product-named field).
- Macro-generated code is invisible to source parsing.
- `#[cfg(test)]` code inside `src/` is included; `tests/`, `benches/`, `examples/` directories are not scanned. `vize_croquis` itself is excluded (internal use is not consumption). Note that `vize_croquis_cf` is a separate crate and therefore counted as an external consumer, even though it is part of the same semantic layer.
- No glob imports (`use vize_croquis::…::*`) exist in the workspace today.

## Products with external consumers

| product                                | kind  | module              | consuming crate     | files | sites |
| -------------------------------------- | ----- | ------------------- | ------------------- | ----: | ----: |
| `Analyzer`                             | type  | `analyzer`          | `vize`              |     1 |     1 |
| `Analyzer`                             | type  | `analyzer`          | `vize_canon`        |    38 |   151 |
| `Analyzer`                             | type  | `analyzer`          | `vize_croquis_cf`   |    18 |    40 |
| `Analyzer`                             | type  | `analyzer`          | `vize_maestro`      |     2 |     2 |
| `Analyzer`                             | type  | `analyzer`          | `vize_vitrine`      |     1 |     1 |
| `AnalyzerOptions`                      | type  | `analyzer`          | `vize`              |     1 |     1 |
| `AnalyzerOptions`                      | type  | `analyzer`          | `vize_canon`        |    38 |   147 |
| `AnalyzerOptions`                      | type  | `analyzer`          | `vize_croquis_cf`   |    12 |    19 |
| `AnalyzerOptions`                      | type  | `analyzer`          | `vize_maestro`      |     2 |     2 |
| `AnalyzerOptions`                      | type  | `analyzer`          | `vize_vitrine`      |     1 |     1 |
| `BindingMetadata`                      | type  | `croquis`           | `vize_atelier_jsx`  |     1 |     1 |
| `BindingMetadata`                      | type  | `croquis`           | `vize_atelier_sfc`  |    13 |    31 |
| `BindingMetadata`                      | type  | `croquis`           | `vize_canon`        |     2 |     5 |
| `COMPILER_MACRO_NAMES`                 | type  | `croquis`           | `vize_patina`       |     1 |     1 |
| `ComponentShape`                       | type  | `croquis`           | `vize_maestro`      |     1 |     1 |
| `ComponentUsage`                       | type  | `croquis::template` | `vize_canon`        |    14 |    35 |
| `ComponentUsage`                       | type  | `croquis::template` | `vize_croquis_cf`   |    13 |    38 |
| `ComponentUsage`                       | type  | `croquis::template` | `vize_maestro`      |     2 |     5 |
| `Croquis`                              | type  | `croquis`           | `vize`              |     1 |     1 |
| `Croquis`                              | type  | `croquis`           | `vize_atelier_core` |     3 |    17 |
| `Croquis`                              | type  | `croquis`           | `vize_atelier_dom`  |     2 |     2 |
| `Croquis`                              | type  | `croquis`           | `vize_atelier_jsx`  |    13 |    24 |
| `Croquis`                              | type  | `croquis`           | `vize_atelier_sfc`  |     7 |    17 |
| `Croquis`                              | type  | `croquis`           | `vize_atelier_ssr`  |     1 |     1 |
| `Croquis`                              | type  | `croquis`           | `vize_canon`        |    69 |   169 |
| `Croquis`                              | type  | `croquis`           | `vize_croquis_cf`   |    40 |   101 |
| `Croquis`                              | type  | `croquis`           | `vize_maestro`      |     3 |     3 |
| `Croquis`                              | type  | `croquis`           | `vize_patina`       |     8 |    22 |
| `Croquis`                              | type  | `croquis`           | `vize_vitrine`      |     1 |     1 |
| `CroquisSemanticSnapshot`              | type  | `croquis`           | `vize_curator`      |     1 |     3 |
| `CroquisSemanticSummary`               | type  | `croquis`           | `vize_curator`      |     1 |     1 |
| `Drawer`                               | type  | `drawer`            | `vize_atelier_jsx`  |     1 |     1 |
| `Drawer`                               | type  | `drawer`            | `vize_atelier_sfc`  |     2 |     7 |
| `Drawer`                               | type  | `drawer`            | `vize_maestro`      |    19 |    23 |
| `Drawer`                               | type  | `drawer`            | `vize_patina`       |     1 |     1 |
| `DrawerOptions`                        | type  | `drawer`            | `vize_atelier_sfc`  |     1 |     5 |
| `DrawerOptions`                        | type  | `drawer`            | `vize_maestro`      |    19 |    23 |
| `EffectGraphScript`                    | type  | `effect_graph`      | `vize`              |     1 |     2 |
| `EffectGraphScript`                    | type  | `effect_graph`      | `vize_croquis_cf`   |     1 |     1 |
| `EffectGraphSummary`                   | type  | `effect_graph`      | `vize_croquis_cf`   |     9 |    20 |
| `ElementIdKind`                        | type  | `croquis::template` | `vize_croquis_cf`   |     1 |     3 |
| `ElementIdKind`                        | type  | `croquis::template` | `vize_patina`       |     1 |     2 |
| `EventHandlerScopeData`                | type  | `scope`             | `vize_canon`        |     6 |     8 |
| `EventHandlerScopeData`                | type  | `scope`             | `vize_croquis_cf`   |     1 |     1 |
| `EventListener`                        | type  | `croquis::template` | `vize_canon`        |     1 |     1 |
| `EventListener`                        | type  | `croquis::template` | `vize_croquis_cf`   |     6 |    10 |
| `EventListener`                        | type  | `croquis::template` | `vize_maestro`      |     1 |     2 |
| `InvalidExportKind`                    | type  | `croquis`           | `vize_vitrine`      |     1 |     6 |
| `MacroTracker`                         | type  | `macros`            | `vize_canon`        |     1 |     1 |
| `MacroTracker`                         | type  | `macros`            | `vize_maestro`      |     1 |     4 |
| `NonScriptSetupScopeData`              | type  | `scope`             | `vize_canon`        |     1 |     3 |
| `OptionGroup`                          | type  | `croquis`           | `vize_canon`        |     2 |     7 |
| `OptionMember`                         | type  | `croquis`           | `vize_patina`       |     2 |     3 |
| `PassedProp`                           | type  | `croquis::template` | `vize_canon`        |     9 |    20 |
| `PassedProp`                           | type  | `croquis::template` | `vize_croquis_cf`   |     8 |    17 |
| `PassedProp`                           | type  | `croquis::template` | `vize_maestro`      |     2 |     4 |
| `ReactivityTracker`                    | type  | `reactivity`        | `vize_vitrine`      |     1 |     1 |
| `Scope`                                | type  | `scope`             | `vize_canon`        |    11 |    17 |
| `Scope`                                | type  | `scope`             | `vize_patina`       |     2 |     2 |
| `ScopeBinding`                         | type  | `scope`             | `vize_atelier_core` |     1 |     1 |
| `ScopeChain`                           | type  | `scope`             | `vize_atelier_core` |     2 |     2 |
| `ScopeChain`                           | type  | `scope`             | `vize_canon`        |     2 |     7 |
| `ScopeData`                            | type  | `scope`             | `vize_canon`        |    11 |    17 |
| `ScopeData`                            | type  | `scope`             | `vize_croquis_cf`   |     8 |     8 |
| `ScopeData`                            | type  | `scope`             | `vize_patina`       |     2 |     2 |
| `ScopeId`                              | type  | `scope`             | `vize_canon`        |     5 |    14 |
| `ScopeId`                              | type  | `scope`             | `vize_croquis_cf`   |    13 |    23 |
| `ScopeId`                              | type  | `scope`             | `vize_maestro`      |     1 |     1 |
| `ScopeKind`                            | type  | `scope`             | `vize_atelier_core` |     1 |     1 |
| `ScopeKind`                            | type  | `scope`             | `vize_canon`        |    11 |    36 |
| `ScopeKind`                            | type  | `scope`             | `vize_croquis_cf`   |    10 |    21 |
| `ScopeKind`                            | type  | `scope`             | `vize_maestro`      |     7 |    43 |
| `ScopeKind`                            | type  | `scope`             | `vize_patina`       |     2 |     2 |
| `ScopeKind`                            | type  | `scope`             | `vize_vitrine`      |     1 |     4 |
| `SlotUsage`                            | type  | `croquis::template` | `vize_canon`        |     1 |     1 |
| `SlotUsage`                            | type  | `croquis::template` | `vize_croquis_cf`   |     2 |     3 |
| `SlotUsage`                            | type  | `croquis::template` | `vize_maestro`      |     2 |     3 |
| `SpreadProp`                           | type  | `croquis::template` | `vize_canon`        |     4 |     5 |
| `TemplateExpression`                   | type  | `croquis`           | `vize_canon`        |     8 |    21 |
| `TemplateExpression`                   | type  | `croquis`           | `vize_croquis_cf`   |     2 |     3 |
| `TemplateExpressionKind`               | type  | `croquis`           | `vize_canon`        |     4 |    11 |
| `TemplateExpressionKind`               | type  | `croquis`           | `vize_croquis_cf`   |     5 |     9 |
| `TypeExport`                           | type  | `croquis`           | `vize_canon`        |     2 |     9 |
| `TypeExportKind`                       | type  | `croquis`           | `vize_canon`        |     2 |     9 |
| `TypeExportKind`                       | type  | `croquis`           | `vize_vitrine`      |     1 |     2 |
| `UnusedVarContext`                     | type  | `croquis`           | `vize_patina`       |     1 |     4 |
| `VForScopeData`                        | type  | `scope`             | `vize_atelier_core` |     1 |     1 |
| `VForScopeData`                        | type  | `scope`             | `vize_canon`        |     1 |     4 |
| `VForScopeData`                        | type  | `scope`             | `vize_croquis_cf`   |     3 |     5 |
| `VSlotScopeData`                       | type  | `scope`             | `vize_atelier_core` |     1 |     1 |
| `VSlotScopeData`                       | type  | `scope`             | `vize_canon`        |     1 |     4 |
| `VSlotScopeData`                       | type  | `scope`             | `vize_croquis_cf`   |     1 |     1 |
| `build_effect_graph_from_script`       | type  | `effect_graph`      | `vize_croquis_cf`   |     1 |     2 |
| `build_effect_graph_from_script_setup` | type  | `effect_graph`      | `vize_croquis_cf`   |     1 |     1 |
| `build_effect_graph_from_sfc_scripts`  | type  | `effect_graph`      | `vize`              |     1 |     1 |
| `build_effect_graph_from_sfc_scripts`  | type  | `effect_graph`      | `vize_croquis_cf`   |     1 |     1 |
| `Croquis.binding_spans`                | field | `croquis`           | `vize_atelier_sfc`  |     1 |     1 |
| `Croquis.binding_spans`                | field | `croquis`           | `vize_canon`        |     5 |     8 |
| `Croquis.binding_spans`                | field | `croquis`           | `vize_maestro`      |     2 |     2 |
| `Croquis.binding_spans`                | field | `croquis`           | `vize_vitrine`      |     1 |     1 |
| `Croquis.bindings`                     | field | `croquis`           | `vize_atelier_core` |     1 |     2 |
| `Croquis.bindings`                     | field | `croquis`           | `vize_atelier_jsx`  |     1 |     1 |
| `Croquis.bindings`                     | field | `croquis`           | `vize_atelier_sfc`  |     6 |    16 |
| `Croquis.bindings`                     | field | `croquis`           | `vize_canon`        |    25 |    45 |
| `Croquis.bindings`                     | field | `croquis`           | `vize_croquis_cf`   |     2 |     2 |
| `Croquis.bindings`                     | field | `croquis`           | `vize_maestro`      |     5 |     9 |
| `Croquis.bindings`                     | field | `croquis`           | `vize_patina`       |     1 |     1 |
| `Croquis.bindings`                     | field | `croquis`           | `vize_vitrine`      |     1 |     2 |
| `Croquis.component_registrations`      | field | `croquis`           | `vize_patina`       |     2 |     2 |
| `Croquis.component_shape`              | field | `croquis`           | `vize_maestro`      |     1 |     1 |
| `Croquis.component_usages`             | field | `croquis`           | `vize`              |     1 |     1 |
| `Croquis.component_usages`             | field | `croquis`           | `vize_canon`        |    11 |    12 |
| `Croquis.component_usages`             | field | `croquis`           | `vize_croquis_cf`   |    18 |    33 |
| `Croquis.component_usages`             | field | `croquis`           | `vize_maestro`      |     6 |     7 |
| `Croquis.element_ids`                  | field | `croquis`           | `vize_croquis_cf`   |     1 |     1 |
| `Croquis.element_ids`                  | field | `croquis`           | `vize_patina`       |     1 |     2 |
| `Croquis.import_statements`            | field | `croquis`           | `vize_atelier_sfc`  |     1 |     2 |
| `Croquis.import_statements`            | field | `croquis`           | `vize_canon`        |     4 |     5 |
| `Croquis.import_statements`            | field | `croquis`           | `vize_patina`       |     1 |     1 |
| `Croquis.invalid_exports`              | field | `croquis`           | `vize_canon`        |     1 |     1 |
| `Croquis.invalid_exports`              | field | `croquis`           | `vize_croquis_cf`   |     1 |     2 |
| `Croquis.invalid_exports`              | field | `croquis`           | `vize_vitrine`      |     1 |     2 |
| `Croquis.macros`                       | field | `croquis`           | `vize_atelier_sfc`  |     3 |    11 |
| `Croquis.macros`                       | field | `croquis`           | `vize_canon`        |    23 |    70 |
| `Croquis.macros`                       | field | `croquis`           | `vize_croquis_cf`   |    14 |    25 |
| `Croquis.macros`                       | field | `croquis`           | `vize_maestro`      |     7 |    14 |
| `Croquis.macros`                       | field | `croquis`           | `vize_patina`       |     8 |    22 |
| `Croquis.macros`                       | field | `croquis`           | `vize_vitrine`      |     1 |     3 |
| `Croquis.options_descriptor`           | field | `croquis`           | `vize_canon`        |     2 |     2 |
| `Croquis.provide_inject`               | field | `croquis`           | `vize_croquis_cf`   |    10 |    26 |
| `Croquis.provide_inject`               | field | `croquis`           | `vize_vitrine`      |     1 |     2 |
| `Croquis.race_conditions`              | field | `croquis`           | `vize_croquis_cf`   |     1 |     1 |
| `Croquis.re_exports`                   | field | `croquis`           | `vize_canon`        |     1 |     1 |
| `Croquis.reactivity`                   | field | `croquis`           | `vize_canon`        |     2 |     2 |
| `Croquis.reactivity`                   | field | `croquis`           | `vize_croquis_cf`   |    11 |    22 |
| `Croquis.reactivity`                   | field | `croquis`           | `vize_maestro`      |     5 |     6 |
| `Croquis.reactivity`                   | field | `croquis`           | `vize_patina`       |     1 |     1 |
| `Croquis.reactivity`                   | field | `croquis`           | `vize_vitrine`      |     1 |     1 |
| `Croquis.scopes`                       | field | `croquis`           | `vize_canon`        |    18 |    41 |
| `Croquis.scopes`                       | field | `croquis`           | `vize_croquis_cf`   |    17 |    27 |
| `Croquis.scopes`                       | field | `croquis`           | `vize_maestro`      |     6 |     7 |
| `Croquis.scopes`                       | field | `croquis`           | `vize_patina`       |     2 |     4 |
| `Croquis.scopes`                       | field | `croquis`           | `vize_vitrine`      |     1 |     2 |
| `Croquis.setup_context`                | field | `croquis`           | `vize_canon`        |     1 |     1 |
| `Croquis.setup_context`                | field | `croquis`           | `vize_croquis_cf`   |     1 |     1 |
| `Croquis.template_expressions`         | field | `croquis`           | `vize_canon`        |     9 |    12 |
| `Croquis.template_expressions`         | field | `croquis`           | `vize_croquis_cf`   |     6 |    12 |
| `Croquis.template_info`                | field | `croquis`           | `vize_canon`        |     2 |     4 |
| `Croquis.template_info`                | field | `croquis`           | `vize_croquis_cf`   |     5 |    14 |
| `Croquis.type_exports`                 | field | `croquis`           | `vize_canon`        |    10 |    21 |
| `Croquis.type_exports`                 | field | `croquis`           | `vize_croquis_cf`   |     1 |     1 |
| `Croquis.type_exports`                 | field | `croquis`           | `vize_vitrine`      |     1 |     2 |
| `Croquis.types`                        | field | `croquis`           | `vize_canon`        |     6 |     9 |
| `Croquis.undefined_refs`               | field | `croquis`           | `vize_canon`        |     6 |     8 |
| `Croquis.undefined_refs`               | field | `croquis`           | `vize_patina`       |     1 |     1 |
| `Croquis.unused_bindings`              | field | `croquis`           | `vize_vitrine`      |     1 |     1 |
| `Croquis.used_components`              | field | `croquis`           | `vize_canon`        |     7 |     9 |
| `Croquis.used_components`              | field | `croquis`           | `vize_croquis_cf`   |    16 |    30 |
| `Croquis.used_components`              | field | `croquis`           | `vize_patina`       |     1 |     1 |

## Products with no external consumers

Computed (or exported) by `vize_croquis`, referenced by no other workspace crate under the resolution above.

- `croquis`: `AnalysisStats`, `CroquisStats`, `ImportStatementInfo`, `InvalidExport`, `OptionKey`, `OptionsDescriptor`, `ReExportInfo`, `SemanticBindingSnapshot`, `SemanticComponentUsageSnapshot`, `SemanticEventListenerSnapshot`, `SemanticInjectSnapshot`, `SemanticPassedPropSnapshot`, `SemanticProvideSnapshot`, `SemanticReactiveSourceSnapshot`, `SemanticReactivityLossSnapshot`, `SemanticScopeBindingSnapshot`, `SemanticScopeSnapshot`, `SemanticSlotUsageSnapshot`, `SemanticSourceRange`, `SemanticTemplateExpressionSnapshot`, `UndefinedRef`, `UnusedTemplateVar`, `Croquis.hoists`, `Croquis.symbols`, `Croquis.used_directives`
- `croquis::template`: `ComponentRegistration`, `ElementIdInfo`, `TemplateInfo`
- `effect_graph`: `EffectGraph`
- `hoist`: `HoistTracker`
- `provide`: `ProvideInjectTracker`
- `race`: `RaceConditionTracker`
- `reactivity_overlay`: `ReactivityEffectEdgeOverlay`, `ReactivityEffectGraphOverlay`, `ReactivityLossOverlay`, `ReactivityOverlay`, `ReactivityOverlaySummary`, `ReactivitySourceOverlay`
- `scope`: `BindingFlags`, `BlockKind`, `BlockScopeData`, `CallbackScopeData`, `ClientOnlyScopeData`, `ClosureScopeData`, `ExternalModuleScopeData`, `JsGlobalScopeData`, `JsRuntime`, `PARAM_INLINE_CAP`, `ParamNames`, `ParentScopes`, `ScriptSetupScopeData`, `Span`, `UniversalScopeData`, `VueGlobalScopeData`
- `setup_context`: `SetupContextTracker`
- `symbol`: `Symbol`, `SymbolFlags`, `SymbolId`, `SymbolTable`
- `types`: `TypeResolver`

## Non-product `vize_croquis` imports observed

Items consumers import from `vize_croquis` that are outside the product set above (module-path items never re-exported at the crate root nor referenced by `croquis.rs`). Kept visible so nothing resolved is silently dropped.

| item                                         | consuming crate      | files | sites |
| -------------------------------------------- | -------------------- | ----: | ----: |
| `BindingType`                                | `vize_atelier_core`  |     1 |    11 |
| `BindingType`                                | `vize_atelier_sfc`   |    17 |   131 |
| `BindingType`                                | `vize_canon`         |     9 |    22 |
| `BlockLocation`                              | `vize`               |     1 |     1 |
| `BlockLocation`                              | `vize_atelier_sfc`   |     3 |     3 |
| `BlockLocation`                              | `vize_glyph`         |     1 |     3 |
| `BlockLocation`                              | `vize_maestro`       |     2 |     2 |
| `BlockLocation`                              | `vize_patina`        |     1 |     1 |
| `DEFINE_EMITS`                               | `vize_atelier_sfc`   |     1 |     2 |
| `DEFINE_EMITS`                               | `vize_canon`         |     2 |     2 |
| `DEFINE_EXPOSE`                              | `vize_atelier_sfc`   |     1 |     2 |
| `DEFINE_EXPOSE`                              | `vize_canon`         |     1 |     1 |
| `DEFINE_MODEL`                               | `vize_atelier_sfc`   |     1 |     2 |
| `DEFINE_MODEL`                               | `vize_canon`         |     1 |     1 |
| `DEFINE_OPTIONS`                             | `vize_atelier_sfc`   |     1 |     2 |
| `DEFINE_PROPS`                               | `vize_atelier_sfc`   |     1 |     2 |
| `DEFINE_PROPS`                               | `vize_canon`         |     3 |     3 |
| `DEFINE_SLOTS`                               | `vize_atelier_sfc`   |     1 |     2 |
| `DEFINE_SLOTS`                               | `vize_canon`         |     1 |     1 |
| `EmitDefinition`                             | `vize_atelier_sfc`   |     1 |     1 |
| `EmitDefinition`                             | `vize_croquis_cf`    |     3 |     3 |
| `InjectEntry`                                | `vize_croquis_cf`    |     4 |     8 |
| `InjectPattern`                              | `vize_croquis_cf`    |     4 |    21 |
| `InjectPattern`                              | `vize_vitrine`       |     1 |     8 |
| `MacroCall`                                  | `vize_canon`         |     1 |     1 |
| `MacroKind`                                  | `vize_canon`         |     3 |     4 |
| `MacroKind`                                  | `vize_croquis_cf`    |     1 |     1 |
| `MacroKind`                                  | `vize_patina`        |     3 |     3 |
| `ModelDefinition`                            | `vize_atelier_sfc`   |     1 |     1 |
| `ModelDefinition`                            | `vize_canon`         |     4 |     7 |
| `PadOption`                                  | `vize_atelier_sfc`   |     1 |     1 |
| `PropDefinition`                             | `vize_atelier_sfc`   |     2 |     4 |
| `PropDefinition`                             | `vize_canon`         |     6 |    22 |
| `PropDefinition`                             | `vize_croquis_cf`    |     3 |     3 |
| `PropsDestructuredBindings`                  | `vize_canon`         |     1 |     1 |
| `ProvideEntry`                               | `vize_croquis_cf`    |     2 |     7 |
| `ProvideKey`                                 | `vize_croquis_cf`    |     7 |    34 |
| `ProvideKey`                                 | `vize_vitrine`       |     1 |     4 |
| `RaceConditionRisk`                          | `vize_croquis_cf`    |     2 |     4 |
| `RaceConditionRiskKind`                      | `vize_croquis_cf`    |     2 |     3 |
| `ReactiveKind`                               | `vize_atelier_core`  |     1 |     4 |
| `ReactiveKind`                               | `vize_croquis_cf`    |     7 |    37 |
| `ReactiveKind`                               | `vize_maestro`       |     5 |    41 |
| `ReactiveKind`                               | `vize_patina`        |     1 |     1 |
| `ReactiveSource`                             | `vize_croquis_cf`    |     1 |     1 |
| `ReactivityLoss`                             | `vize_patina`        |     1 |     2 |
| `ReactivityLossKind`                         | `vize_canon`         |     1 |    11 |
| `ReactivityLossKind`                         | `vize_croquis_cf`    |     2 |    18 |
| `ReactivityLossKind`                         | `vize_patina`        |     1 |    21 |
| `ScriptParseResult`                          | `vize_atelier_sfc`   |     1 |     1 |
| `ScriptParseResult`                          | `vize_patina`        |     2 |     2 |
| `ScriptParserOptions`                        | `vize_atelier_sfc`   |     1 |     1 |
| `ScriptParserOptions`                        | `vize_patina`        |     1 |     1 |
| `SetupContextViolation`                      | `vize_croquis_cf`    |     1 |     1 |
| `SetupContextViolationKind`                  | `vize_croquis_cf`    |     2 |    11 |
| `SfcCustomBlock`                             | `vize_atelier_sfc`   |     2 |     2 |
| `SfcCustomBlock`                             | `vize_patina`        |     1 |     2 |
| `SfcDescriptor`                              | `vize`               |     1 |     2 |
| `SfcDescriptor`                              | `vize_atelier_sfc`   |    13 |    33 |
| `SfcDescriptor`                              | `vize_canon`         |     9 |    15 |
| `SfcDescriptor`                              | `vize_maestro`       |     6 |    23 |
| `SfcDescriptor`                              | `vize_patina`        |    10 |    14 |
| `SfcDescriptor`                              | `vize_vitrine`       |     2 |     2 |
| `SfcError`                                   | `vize_atelier_sfc`   |    19 |    52 |
| `SfcError`                                   | `vize_canon`         |     2 |     2 |
| `SfcError`                                   | `vize_patina`        |     1 |     2 |
| `SfcParseOptions`                            | `vize`               |     9 |    12 |
| `SfcParseOptions`                            | `vize_atelier_sfc`   |    13 |   104 |
| `SfcParseOptions`                            | `vize_canon`         |     9 |    10 |
| `SfcParseOptions`                            | `vize_curator`       |     3 |     3 |
| `SfcParseOptions`                            | `vize_glyph`         |     1 |     1 |
| `SfcParseOptions`                            | `vize_maestro`       |     2 |     2 |
| `SfcParseOptions`                            | `vize_patina`        |     9 |    10 |
| `SfcParseOptions`                            | `vize_vitrine`       |     9 |    14 |
| `SfcScriptBlock`                             | `vize_atelier_sfc`   |     1 |     1 |
| `SfcScriptBlock`                             | `vize_curator`       |     1 |     1 |
| `SfcScriptBlock`                             | `vize_maestro`       |     3 |     6 |
| `SfcStyleBlock`                              | `vize_atelier_sfc`   |     5 |     7 |
| `SfcStyleBlock`                              | `vize_maestro`       |     2 |     8 |
| `SfcTemplateBlock`                           | `vize_atelier_sfc`   |     5 |     6 |
| `SfcTemplateBlock`                           | `vize_canon`         |     1 |     1 |
| `ViolationSeverity`                          | `vize_canon`         |     1 |     3 |
| `ViolationSeverity`                          | `vize_croquis_cf`    |     1 |     6 |
| `VirtualTsConfig`                            | `vize_patina`        |     1 |     1 |
| `VirtualTsOutput`                            | `vize_patina`        |     5 |    16 |
| `WITH_DEFAULTS`                              | `vize_atelier_sfc`   |     1 |     2 |
| `WITH_DEFAULTS`                              | `vize_canon`         |     2 |     2 |
| `analyze_script_setup_program`               | `vize_atelier_sfc`   |     1 |     1 |
| `artifact_macro_names`                       | `vize_atelier_sfc`   |     1 |     1 |
| `collect_options_descriptor`                 | `vize_patina`        |     4 |     4 |
| `collect_options_object`                     | `vize_patina`        |     1 |     1 |
| `extract_and_transform_v_bind`               | `vize_atelier_sfc`   |     1 |     1 |
| `extract_and_transform_v_bind_with_scope`    | `vize_atelier_sfc`   |     1 |     1 |
| `extract_identifier_refs_oxc`                | `vize_canon`         |     2 |     2 |
| `extract_identifiers_oxc`                    | `vize_canon`         |     5 |     5 |
| `extract_identifiers_oxc`                    | `vize_maestro`       |     1 |     1 |
| `extract_slot_props`                         | `vize_patina`        |     1 |     1 |
| `find_matching_paren`                        | `vize_atelier_sfc`   |     1 |     1 |
| `generate_declaration_ts`                    | `vize_vitrine`       |     1 |     2 |
| `generate_declaration_ts_with_split_scripts` | `vize_vitrine`       |     1 |     1 |
| `generate_virtual_ts_with_croquis`           | `vize_patina`        |     1 |     1 |
| `is_builtin_component`                       | `vize_atelier_sfc`   |     1 |     1 |
| `is_builtin_component`                       | `vize_patina`        |     3 |     7 |
| `is_builtin_macro`                           | `vize_atelier_sfc`   |     2 |     2 |
| `is_event_local`                             | `vize_canon`         |     1 |     1 |
| `is_global_allowed`                          | `vize_atelier_core`  |     5 |    14 |
| `is_global_allowed`                          | `vize_atelier_sfc`   |     1 |     1 |
| `is_global_allowed`                          | `vize_atelier_vapor` |     2 |     3 |
| `is_js_global`                               | `vize_canon`         |     2 |     2 |
| `is_kebab_case`                              | `vize_maestro`       |     1 |     1 |
| `is_kebab_case_loose`                        | `vize_patina`        |     1 |     1 |
| `is_keyword`                                 | `vize_canon`         |     1 |     1 |
| `is_pascal_case`                             | `vize_patina`        |     2 |     2 |
| `is_render_local`                            | `vize_canon`         |     1 |     1 |
| `is_runtime_erased_macro`                    | `vize_atelier_sfc`   |     1 |     1 |
| `is_vue_builtin`                             | `vize_canon`         |     1 |     1 |
| `macro_artifact_kind`                        | `vize_atelier_sfc`   |     1 |     3 |
| `names_match`                                | `vize_patina`        |     3 |     3 |
| `parse_script_setup`                         | `vize_atelier_sfc`   |     1 |     2 |
| `parse_script_setup`                         | `vize_maestro`       |     2 |     4 |
| `parse_script_setup`                         | `vize_musea`         |     2 |     2 |
| `parse_script_setup`                         | `vize_patina`        |     3 |     3 |
| `parse_script_setup_with_generic_and_jsx`    | `vize_patina`        |     1 |     1 |
| `parse_script_with_options`                  | `vize_patina`        |     1 |     1 |
| `parse_script_with_options_and_jsx`          | `vize_atelier_sfc`   |     1 |     1 |
| `parse_sfc`                                  | `vize_atelier_sfc`   |     1 |     1 |
| `parse_v_for_expression`                     | `vize_patina`        |     1 |     1 |
| `prod_scoped_v_bind_name`                    | `vize_atelier_sfc`   |     1 |     1 |
| `runtime_erased_macro_names`                 | `vize_atelier_sfc`   |     3 |     5 |
| `scoped_v_bind_name`                         | `vize_atelier_sfc`   |     1 |     1 |
| `strip_js_comments`                          | `vize_canon`         |     3 |     4 |
| `to_pascal_case`                             | `vize`               |     1 |     1 |
| `to_pascal_case`                             | `vize_canon`         |     2 |     2 |
| `to_pascal_case`                             | `vize_patina`        |     2 |     3 |

## Cross-check: symbol-resolved vs naive grep

The naive lane counts raw word-boundary text matches per product name (`\.field` matches for field rows) over the same files — comments, strings, doc text, and same-named unrelated symbols included, imports included. Disagreements are listed, **not** reconciled: `grep > resolved` usually means comments/unrelated same-named symbols (for field rows: field accesses on non-`Croquis` receivers); `grep < resolved` would indicate a resolver bug and must be investigated.

| product                                | resolved | grep | disagreeing crates (resolved/grep)                                                                                                                                                                                                                                                                                                                                                                                                                       |
| -------------------------------------- | -------: | ---: | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `Analyzer`                             |      195 |  287 | `vize` (1/2), `vize_canon` (151/230), `vize_croquis_cf` (40/49), `vize_doctor` (0/1), `vize_maestro` (2/3), `vize_vitrine` (1/2)                                                                                                                                                                                                                                                                                                                         |
| `AnalyzerOptions`                      |      170 |  288 | `vize` (1/2), `vize_canon` (147/226), `vize_croquis_cf` (19/56), `vize_maestro` (2/3)                                                                                                                                                                                                                                                                                                                                                                    |
| `BindingMetadata`                      |       37 |  132 | `vize_atelier_core` (0/24), `vize_atelier_dom` (0/20), `vize_atelier_jsx` (1/2), `vize_atelier_sfc` (31/49), `vize_atelier_ssr` (0/2), `vize_atelier_vapor` (0/14), `vize_canon` (5/8), `vize_relief` (0/6), `vize_s1_to_s2` (0/1), `vize_vitrine` (0/6)                                                                                                                                                                                                 |
| `COMPILER_MACRO_NAMES`                 |        1 |    3 | `vize_canon` (0/1), `vize_patina` (1/2)                                                                                                                                                                                                                                                                                                                                                                                                                  |
| `ComponentShape`                       |        1 |    2 | `vize_maestro` (1/2)                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| `ComponentUsage`                       |       78 |  142 | `vize_canon` (35/49), `vize_croquis_cf` (38/85), `vize_maestro` (5/8)                                                                                                                                                                                                                                                                                                                                                                                    |
| `Croquis`                              |      358 |  566 | `vize` (1/2), `vize_atelier_core` (17/23), `vize_atelier_dom` (2/6), `vize_atelier_jsx` (24/43), `vize_atelier_sfc` (17/43), `vize_atelier_ssr` (1/4), `vize_canon` (169/246), `vize_carton` (0/1), `vize_croquis_cf` (101/127), `vize_davinci` (0/2), `vize_maestro` (3/18), `vize_patina` (22/47), `vize_vitrine` (1/4)                                                                                                                                |
| `CroquisSemanticSnapshot`              |        3 |    4 | `vize_curator` (3/4)                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| `CroquisSemanticSummary`               |        1 |    2 | `vize_curator` (1/2)                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| `Drawer`                               |       32 |   54 | `vize_atelier_jsx` (1/2), `vize_atelier_sfc` (7/9), `vize_canon` (0/1), `vize_maestro` (23/40), `vize_patina` (1/2)                                                                                                                                                                                                                                                                                                                                      |
| `DrawerOptions`                        |       28 |   47 | `vize_atelier_sfc` (5/6), `vize_maestro` (23/41)                                                                                                                                                                                                                                                                                                                                                                                                         |
| `EffectGraphScript`                    |        3 |    5 | `vize` (2/3), `vize_croquis_cf` (1/2)                                                                                                                                                                                                                                                                                                                                                                                                                    |
| `EffectGraphSummary`                   |       20 |   32 | `vize_croquis_cf` (20/32)                                                                                                                                                                                                                                                                                                                                                                                                                                |
| `ElementIdKind`                        |        5 |    7 | `vize_croquis_cf` (3/4), `vize_patina` (2/3)                                                                                                                                                                                                                                                                                                                                                                                                             |
| `EventHandlerScopeData`                |        9 |   16 | `vize_canon` (8/14), `vize_croquis_cf` (1/2)                                                                                                                                                                                                                                                                                                                                                                                                             |
| `EventListener`                        |       13 |   22 | `vize_canon` (1/2), `vize_croquis_cf` (10/16), `vize_maestro` (2/4)                                                                                                                                                                                                                                                                                                                                                                                      |
| `MacroTracker`                         |        5 |    6 | `vize_croquis_cf` (0/1)                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| `NonScriptSetupScopeData`              |        3 |    4 | `vize_canon` (3/4)                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| `OptionGroup`                          |        7 |    9 | `vize_canon` (7/9)                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| `OptionMember`                         |        3 |    5 | `vize_patina` (3/5)                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| `PassedProp`                           |       41 |   62 | `vize_canon` (20/30), `vize_croquis_cf` (17/25), `vize_maestro` (4/7)                                                                                                                                                                                                                                                                                                                                                                                    |
| `ProvideInjectTracker`                 |        0 |    1 | `vize_croquis_cf` (0/1)                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| `ReactivityTracker`                    |        1 |    4 | `vize_atelier_core` (0/1), `vize_atelier_sfc` (0/1), `vize_vitrine` (1/2)                                                                                                                                                                                                                                                                                                                                                                                |
| `Scope`                                |       19 |   86 | `vize` (0/1), `vize_atelier_core` (0/3), `vize_atelier_dom` (0/1), `vize_atelier_jsx` (0/1), `vize_atelier_sfc` (0/5), `vize_atelier_ssr` (0/3), `vize_canon` (17/47), `vize_davinci` (0/1), `vize_maestro` (0/7), `vize_patina` (2/10), `vize_relief` (0/2), `vize_s1_to_s2` (0/2), `vize_s2` (0/1), `vize_vitrine` (0/2)                                                                                                                               |
| `ScopeBinding`                         |        1 |   26 | `vize_atelier_core` (1/2), `vize_atelier_jsx` (0/4), `vize_s1_to_s2` (0/11), `vize_s2` (0/9)                                                                                                                                                                                                                                                                                                                                                             |
| `ScopeChain`                           |        9 |   13 | `vize_atelier_core` (2/3), `vize_canon` (7/9), `vize_s1_to_s2` (0/1)                                                                                                                                                                                                                                                                                                                                                                                     |
| `ScopeData`                            |       27 |   40 | `vize_canon` (17/28), `vize_patina` (2/4)                                                                                                                                                                                                                                                                                                                                                                                                                |
| `ScopeId`                              |       38 |   57 | `vize_canon` (14/19), `vize_croquis_cf` (23/33), `vize_maestro` (1/2), `vize_patina` (0/1), `vize_vitrine` (0/2)                                                                                                                                                                                                                                                                                                                                         |
| `ScopeKind`                            |      107 |  131 | `vize_atelier_core` (1/2), `vize_canon` (36/47), `vize_croquis_cf` (21/25), `vize_maestro` (43/49), `vize_patina` (2/4)                                                                                                                                                                                                                                                                                                                                  |
| `SlotUsage`                            |        7 |   17 | `vize_canon` (1/3), `vize_croquis_cf` (3/8), `vize_maestro` (3/6)                                                                                                                                                                                                                                                                                                                                                                                        |
| `Span`                                 |        0 |  656 | `vize_atelier_core` (0/2), `vize_atelier_jsx` (0/73), `vize_atelier_sfc` (0/6), `vize_atelier_vapor` (0/1), `vize_canon` (0/56), `vize_carton` (0/89), `vize_davinci` (0/20), `vize_maestro` (0/4), `vize_patina` (0/153), `vize_relief` (0/8), `vize_s1_to_s2` (0/127), `vize_s2` (0/117)                                                                                                                                                               |
| `SpreadProp`                           |        5 |    9 | `vize_canon` (5/9)                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| `Symbol`                               |        0 |   68 | `vize_atelier_core` (0/5), `vize_atelier_sfc` (0/4), `vize_atelier_ssr` (0/2), `vize_canon` (0/3), `vize_croquis_cf` (0/27), `vize_fresco` (0/2), `vize_maestro` (0/1), `vize_patina` (0/15), `vize_relief` (0/5), `vize_s1_to_s2` (0/1), `vize_vitrine` (0/3)                                                                                                                                                                                           |
| `SymbolFlags`                          |        0 |    5 | `vize_atelier_sfc` (0/5)                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| `SymbolId`                             |        0 |    3 | `vize_patina` (0/3)                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| `TemplateExpression`                   |       24 |   47 | `vize_canon` (21/28), `vize_croquis_cf` (3/5), `vize_maestro` (0/14)                                                                                                                                                                                                                                                                                                                                                                                     |
| `TemplateExpressionKind`               |       20 |   29 | `vize_canon` (11/15), `vize_croquis_cf` (9/14)                                                                                                                                                                                                                                                                                                                                                                                                           |
| `TypeExport`                           |        9 |   12 | `vize_canon` (9/12)                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| `TypeExportKind`                       |       11 |   13 | `vize_canon` (9/11)                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| `TypeResolver`                         |        0 |    5 | `vize_canon` (0/5)                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| `UnusedVarContext`                     |        4 |    5 | `vize_patina` (4/5)                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| `VForScopeData`                        |       10 |   14 | `vize_atelier_core` (1/2), `vize_canon` (4/5), `vize_croquis_cf` (5/7)                                                                                                                                                                                                                                                                                                                                                                                   |
| `VSlotScopeData`                       |        6 |    9 | `vize_atelier_core` (1/2), `vize_canon` (4/5), `vize_croquis_cf` (1/2)                                                                                                                                                                                                                                                                                                                                                                                   |
| `build_effect_graph_from_script`       |        2 |    3 | `vize_croquis_cf` (2/3)                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| `build_effect_graph_from_script_setup` |        1 |    2 | `vize_croquis_cf` (1/2)                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| `build_effect_graph_from_sfc_scripts`  |        2 |    4 | `vize` (1/2), `vize_croquis_cf` (1/2)                                                                                                                                                                                                                                                                                                                                                                                                                    |
| `Croquis.binding_spans`                |       12 |   13 | `vize_maestro` (2/3)                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| `Croquis.bindings`                     |       78 |  687 | `vize` (0/2), `vize_atelier_core` (2/24), `vize_atelier_dom` (0/1), `vize_atelier_jsx` (1/36), `vize_atelier_sfc` (16/224), `vize_atelier_ssr` (0/3), `vize_atelier_vapor` (0/4), `vize_canon` (45/90), `vize_croquis_cf` (2/10), `vize_curator` (0/1), `vize_davinci` (0/10), `vize_fresco` (0/6), `vize_maestro` (9/13), `vize_musea` (0/1), `vize_patina` (1/6), `vize_relief` (0/1), `vize_s1_to_s2` (0/227), `vize_s2` (0/21), `vize_vitrine` (2/7) |
| `Croquis.component_usages`             |       53 |   58 | `vize_croquis_cf` (33/38)                                                                                                                                                                                                                                                                                                                                                                                                                                |
| `Croquis.hoists`                       |        0 |   17 | `vize_atelier_core` (0/8), `vize_atelier_sfc` (0/2), `vize_relief` (0/1), `vize_s1_to_s2` (0/6)                                                                                                                                                                                                                                                                                                                                                          |
| `Croquis.import_statements`            |        8 |   10 | `vize_maestro` (0/1), `vize_musea` (0/1)                                                                                                                                                                                                                                                                                                                                                                                                                 |
| `Croquis.macros`                       |      145 |  265 | `vize_atelier_sfc` (11/102), `vize_canon` (70/72), `vize_croquis_cf` (25/27), `vize_davinci` (0/4), `vize_maestro` (14/24), `vize_musea` (0/2), `vize_patina` (22/31)                                                                                                                                                                                                                                                                                    |
| `Croquis.provide_inject`               |       28 |   36 | `vize_croquis_cf` (26/34)                                                                                                                                                                                                                                                                                                                                                                                                                                |
| `Croquis.race_conditions`              |        1 |    4 | `vize_croquis_cf` (1/4)                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| `Croquis.reactivity`                   |       32 |   39 | `vize_atelier_core` (0/1), `vize_davinci` (0/3), `vize_patina` (1/4)                                                                                                                                                                                                                                                                                                                                                                                     |
| `Croquis.scopes`                       |       81 |  131 | `vize` (0/1), `vize_atelier_core` (0/4), `vize_atelier_jsx` (0/11), `vize_canon` (41/43), `vize_curator` (0/1), `vize_davinci` (0/9), `vize_patina` (4/17), `vize_s1_to_s2` (0/9)                                                                                                                                                                                                                                                                        |
| `Croquis.setup_context`                |        2 |    5 | `vize_croquis_cf` (1/4)                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| `Croquis.template_info`                |       18 |   20 | `vize_croquis_cf` (14/15), `vize_patina` (0/1)                                                                                                                                                                                                                                                                                                                                                                                                           |
| `Croquis.types`                        |        9 |   29 | `vize` (0/1), `vize_atelier_sfc` (0/13), `vize_canon` (9/11), `vize_davinci` (0/3), `vize_maestro` (0/1)                                                                                                                                                                                                                                                                                                                                                 |
| `Croquis.undefined_refs`               |        9 |   10 | `vize_canon` (8/9)                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| `Croquis.used_components`              |       40 |   44 | `vize_croquis_cf` (30/34)                                                                                                                                                                                                                                                                                                                                                                                                                                |
