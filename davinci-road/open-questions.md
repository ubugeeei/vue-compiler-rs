# Davinci — Open Questions

> [!NOTE]
> Active design discussions. Each entry gets a decision record (moved to the
> [charter](./README.md#decided-positions)) or is dropped with a note. Decided
> entries become stubs pointing at their charter row — never silently deleted.

## Decided (stubs)

- **Naming** → charter #11. Stage aliases are the primary implementation names
  (`vize_s1`, `vize_s2`, `vize_s1_to_s2`, future `vize_s3`). S1 has already
  been mechanically renamed from Sinopia; remaining art names stay courtesy
  aliases and, where not yet renamed, historical package ids until their own
  rename PRs land.
- **S3 scope** → charter #9. DOM + Vapor through S3; SSR thin S2→S4 path
  reading partition facts. Phase 3 measurements keep veto power.
- **Incrementality** → charter #10. salsa in the resident tier only; fused
  non-salsa pipeline for one-shot CLI; block content keys as firewall queries.
- **Fact query API** → charter #8. Static demand declarations + debug-build
  undeclared-access detector.
- **pug fidelity** → charter #12. First-class S1 dialect.
- **SFC style coordination** → charter #13. `v-bind()` bindings visible as S2
  ops.
- **Foreign expression type checking** → charter #14. Projection duty lives in
  the expression-dialect contract; boundary-typed integration is the fallback
  tier, not the default.
- **Contract linking** → charter #15. Two tiers: compiled-in traits + features
  for first-party, out-of-process serialized contract for external.
- **DevTool protocol** → [devtool.md §Transport](./devtool.md#transport)
  (P2-19 spike, [record](./plan/phase-2-records/p2-19.md)). Document over
  JSON-RPC: the P2-18 feed document is the unit everywhere; C-7's server
  speaks content-mapper-style JSON-RPC with `initialize` negotiating
  `schema_version` before any payload; served files stay the at-rest form,
  the wasm playground keeps the embedding; JSON-lines rejected.

## Fusion depth for the build path

How far can `vize build` fuse before diagnostics quality suffers? Emitting S2
during parse and skipping S1 materialization assumes spans + source text are
enough for error rendering (they should be — excerpts derive from `Span` +
source). The riskier line is fusing semantic-fact population into lowering:
synthesized attributes fuse cleanly, but anything needing lookahead (sibling
`v-else`, slot collection) must stay region-local. Needs a phase-2 prototype
measuring walk count and instruction locality against the phase-0 baselines.

Prototype note (2026-09-02): the S2 DOM emitter now exposes
`emit_dom_source_observed`, measuring the transform `BudgetObserver` plus the
single code-producing emit walk over the ladder in `emit_budget_observer`.
This does not answer the fusion policy yet: transform groups are still
serialized and the production build path is not switched.

Prototype note (2026-09-07): profiled source-map-free DOM compiles now record
the remaining pre-S2 template walk as `davinci.s2_dom.pre_s2.*` and reconcile
`davinci.s2_dom.build.walks` as that walk plus the S2 observer total. Ladder
evidence shows the emit walk is already at the one-walk target, while the
build path remains at 8 walks (1 pre-S2 + 7 S2 observer) until parse-to-S2
and S2 transform fusion land.

## Orphan analyses: productize or cut

`RaceConditionTracker` and `ProvideInjectTracker` have zero consumers;
`EffectGraph` has one (Doctor). The semantic-engine plan gives each a product
(async-race rules, cross-file provide/inject pairing, Vapor effect grouping) —
but each needs corpus evidence that the analysis is sound at scale before it
ships as a rule. Any product that doesn't earn corpus trust gets its fact group
demand-gated to zero cost rather than deleted, per charter #5.

## Rule-corpus fairness measurement

Charter #7 needs a metric: of Patina's 345 rule files, which are neutral-core
(should run on SFC + JSX + external dialects), which are Vue-dialect-bound
(`v-model` modifiers), and which are container-bound (SFC block structure)?
The phase-0 rule-parity matrix defines the classification; phase 4's exit gate
consumes it. Open: whether the classification is declared per rule (a
`dialect_scope` field) or derived from the fact groups the rule demands — with
static demand declarations (charter #8) the derived form is nearly free, so
lean derived unless rule authors need overrides.

## Complexity metric definition

Which metric family for template CFGs — cyclomatic, cognitive, or both — and
how cross-file attribution works (does a complex child component tax its
parents, or only its own score?), plus thresholds and rule presentation.
Decide with corpus distributions in hand (phase 4), not a priori; the existing
`vize_curator` complexity module and cross-file-complexity guide are the
starting data.

## `no_std` boundary reality check

Davinci-owned crates are `no_std + alloc` by charter #18, but the practical
boundary depends on dependencies: oxc crates and lightningcss assume `std` in
places, salsa is resident-tier-only anyway, and rayon is a `std` feature.
Needs an early audit: which existing crates could honestly become `no_std`,
what the `wasm32-wasip2` CI target covers (core compile lane vs full CLI), and
whether the WASI component model doubles as the out-of-process contract
transport (charter #15) — evaluate against the prior-art findings.

## AI optimization loop guardrails

**Decided** → charter #32: optimization PRs auto-merge on full gate passage;
contract/semantics changes stay maintainer-reviewed. Remaining detail for the
implementation roadmap: the sandboxing of optimization experiments (worktree
isolation, corpus-run quotas) and the audit trail format for auto-merged PRs.

## JS plugin API shape

Charter #29 commits to user-land JS plugins/custom rules; the API shape is
open: serialized visit batches vs proxy objects over napi; sync napi calls vs
a worker pool; how a JS rule's declared fact demands are expressed in the JS
SDK; how rule output caching keys include the plugin's own version/content;
and whether the same SDK surface doubles for the WASM tier (one authoring
model, two runtimes). ESLint-compatibility (running existing eslint-plugin-vue
rules unchanged) is explicitly _not_ the goal — the SDK targets the
neutral-core view. Needs a phase-4/5 spike with a real custom-rule case.

## App-level fact provider contract

Route trees, `definePageMeta`, i18n catalogs: in-tree providers cover Vue
Router and Nuxt (Vue-family scope), but the provider interface should be the
same one external ecosystems would use. Open: is a convention provider a third
kind of first-party plug-in (like input dialects), or a consumer of the
cross-file fact API with write access? Decide when phase 4 generalizes
Maestro's ecosystem services.
