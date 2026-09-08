# Vue Parity Matrix

This matrix defines which Vue compatibility surfaces are release-blocking for the v1 alpha line.
Official Vue tooling is the baseline whenever Vize output disagrees with it, unless a Vize guide
explicitly documents a narrower behavior.

## Baseline Versions

| Tool              | Baseline                                                                         | Gate                                            |
| ----------------- | -------------------------------------------------------------------------------- | ----------------------------------------------- |
| Vue runtime       | `vue@3.5.34` for published runtime smoke, `vue@3.6.0-beta.10` for fixture parity | `fresh-install-smoke`, `vue-parity`             |
| Vue compiler      | `@vue/compiler-sfc@3.6.0-beta.10`                                                | `vp run --filter './tests' test:check:fixtures` |
| Vue type checking | `vue-tsc@3.3.11` with `typescript@6.0.3`                                         | `vp run --filter './tests' test:check:fixtures` |
| Vite              | `vite@npm:@voidzero-dev/vite-plus-core@0.1.21`                                   | `fresh-install-smoke`, app e2e                  |
| Node.js           | `22` and `24`                                                                    | `node-engine-compat`, `fresh-install-smoke`     |

## Compatibility Surfaces

| Surface                                                 | Status                | Release-blocking evidence                                                                                                                           | Notes                                                                                                                                                                                                                                                                                                                                                                                              |
| ------------------------------------------------------- | --------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| SFC parse and `<script setup>` compile                  | Alpha-supported       | `@vue/compiler-sfc` accepts parity fixtures in `test:check:fixtures`; compiler fixture coverage is `625/625`                                        | Vize must not reject fixtures accepted by the official compiler baseline.                                                                                                                                                                                                                                                                                                                          |
| Template compilation, directives, slots, and asset URLs | Alpha-supported       | compiler fixture coverage, Vite plugin runtime smoke, app e2e                                                                                       | Official Vue output remains the tie-breaker for unlisted edge cases.                                                                                                                                                                                                                                                                                                                               |
| `vize check` diagnostic file surface                    | Alpha-supported       | `vue-tsc` parity in `tests/snapshots/check/toolchain-parity.ts`                                                                                     | For files with errors, Vize and `vue-tsc` must agree on the Vue files that own diagnostics and share TypeScript diagnostic codes.                                                                                                                                                                                                                                                                  |
| Options API (VDOM)                                      | Alpha-supported       | `vue-tsc` parity in `tests/snapshots/check/options-api.ts` (`options-api` fixture)                                                                  | Default-on (matches `vue-tsc`); opt out via `typeChecker.optionsApi: false`. `props`/`data`/`computed`/`methods`/`mixins`/`extends` resolve as `this`-typed template bindings; `vize check` matches `vue-tsc`'s diagnostic surface (clean valid usage, intentional error at the same spot).                                                                                                        |
| Class components                                        | Incubating            | `vize check` diagnostics in `tests/snapshots/check/class-component.ts` (`class-component` fixture)                                                  | `vue-class-component` / `vue-property-decorator` (`@Options`/`@Prop`, data/getter/method). Vize resolves the class instance type and template bindings and catches the intentional error; full `vue-tsc` surface parity is infeasible because `vue-tsc`'s class-component virtual code emits spurious TS2528/TS2339, so the gate asserts Vize's own diagnostics plus the shared intentional error. |
| `@vizejs/native` NAPI runtime                           | Alpha-supported       | `fresh-install-smoke` requires `compileSfc` from installed tarballs on macOS, Linux, and Windows                                                    | Root package and compatible platform package must be installed from tarballs together.                                                                                                                                                                                                                                                                                                             |
| `@vizejs/vite-plugin` production build                  | Alpha-supported       | `fresh-install-smoke` runs `vite build` from installed tarballs on Node 22 and 24                                                                   | Covers a real Vue app shell instead of only verifying package entrypoints.                                                                                                                                                                                                                                                                                                                         |
| Vapor compiler mode                                     | Experimental          | VDOM/Vapor fixture coverage                                                                                                                         | Experimental: API, output, and workflow shape may change with Vue Vapor upstream changes before v1 stable.                                                                                                                                                                                                                                                                                         |
| Vapor mode Options API                                  | Experimental          | Rust unit tests and Vapor fixture coverage                                                                                                          | Experimental opt-in: Options-API-authored components compiled for Vapor mode.                                                                                                                                                                                                                                                                                                                      |
| Standalone CDN build                                    | Experimental          | release package smoke                                                                                                                               | Experimental opt-in: single-file standalone bundle usable directly from a CDN with no bundler step.                                                                                                                                                                                                                                                                                                |
| Zero-JavaScript prerenderer (Island)                    | Planned               | n/a — not yet implemented                                                                                                                           | Roadmap surface targeting production-ready once shipped: Island-architecture prerendering that emits zero client JavaScript for static islands.                                                                                                                                                                                                                                                    |
| SSR-specific compiler and lint behavior                 | Preview               | Rust unit tests and fixture coverage                                                                                                                | Production SSR apps must validate their own hydration and renderer constraints.                                                                                                                                                                                                                                                                                                                    |
| Nuxt, Rspack, unplugin, and Musea integrations          | Compatibility preview | package build/check jobs and release smoke installs                                                                                                 | Host-framework compatibility must be validated before a production rollout.                                                                                                                                                                                                                                                                                                                        |
| Editor language features                                | Alpha-supported       | [Maestro Vue Language Tools Scorecard](./maestro-vue-language-tools-scorecard.md), extension packaging, real-server editor scenarios, and LSP tests | Vize editor support is gated against the Vue Language Server protocol surface with machine-checked feature rows, must-include/must-exclude oracles, editor breadth evidence, and Misskey / Vue Vben Admin latency budgets.                                                                                                                                                                         |
| WASM package runtime                                    | Experimental          | release package smoke                                                                                                                               | API and runtime shape may change during alpha.                                                                                                                                                                                                                                                                                                                                                     |
| Legacy Vue 2 dialect (`vue.version`)                    | Incubating            | Rust unit tests in `vize_atelier_core` (`legacy.rs`, `legacy_filters.rs`) and `vize_armature::legacy`                                               | Off by default: requires the `legacy` cargo feature and a `vue.version` opt-in (`"2"` / `"2.7"`). Resolves a Vue-2 template dialect and desugars Vue 2 sugar to Vue 3 equivalents before the main transform. See [Legacy Vue support](#legacy-vue-support) for the exact supported surface and gaps.                                                                                               |
| petite-vue standalone HTML                              | Incubating            | Rust unit tests in `vize_carton` (detection), `vize_armature` (document parse), `vize_croquis`, `vize_patina`, and `vize_maestro`                   | Detected structurally from standalone HTML (`<script src>` / ES import / `PetiteVue.createApp`); not a `.vue` SFC surface. Parser, lint, and LSP model `v-scope` / `v-effect`. See [petite-vue support](#petite-vue-support) for the exact supported surface and gaps.                                                                                                                             |

## Legacy Vue support

Legacy (pre-Vue-3) support is **off by default**. It activates only when the downstream crates are
built with the `legacy` cargo feature **and** the project opts in via `vue.version` (`vize_carton`'s
`VueVersion`; default `"3"` is modern Vue 3 and never touches any legacy path). The dialect is
resolved once per file (`vize_relief::options` carries `dialect: VueVersion`) and mapped to a
capability set (`vize_armature::legacy::LegacyDialectCapabilities`).

Verified-present Vue 2 template desugaring (`crates/vize_atelier_core/src/steps/legacy.rs`,
`desugar_legacy_template`), gated by the `legacy` feature and the Vue-2 dialect capability set:

- **`.sync` modifier** — `:foo.sync="bar"` expands to a plain `:foo="bar"` bind plus an
  `@update:foo="bar = $event"` listener, matching Vue 2 semantics.
- **`slot-scope` / `scope` attributes** — the pre-2.6 scoped-slot spelling
  (`<template slot="name" slot-scope="props">`) desugars to `v-slot:name="props"`.
- **`.native` modifier strip** — `@click.native` has `.native` removed (Vue 3 lets component
  listeners fall through by default), via `desugar_v2_v_on_modifiers`.
- **Numeric `keyCode` modifiers** — built-in numeric key codes are rewritten to their Vue 3 key
  names, mirroring the removed `@vue/compiler-dom` `keyCodes` table.

Verified-present Vue 2 pipe filters
(`crates/vize_atelier_core/src/steps/legacy_filters.rs`): `{{ value \| capitalize }}` style
filter chains are parsed (string and bracket literals are not split on `\|`) and the referenced
filter names are surfaced on `RootNode::filters`
(`crates/vize_relief/src/ast/nodes.rs`) for `_resolveFilter` asset emission. The whole module is
`#[cfg(feature = "legacy")]`, so the field and code path do not exist in a default build.

Verified-present script-side recognition: `Vue.extend({ ... })` (and a named `extend` import) is
recognized as a component-options call alongside `defineComponent` / `export default {}`
(`crates/vize_croquis/src/script_parser/process/options_api.rs`, `is_component_options_callee`),
so Options-API template bindings resolve from a Vue 2 component object.

Honest gaps:

- Legacy support is **off-by-default** behind the `legacy` cargo feature **and** the `vue.version`
  opt-in; the default Vue 3 build neither parses filters nor desugars Vue 2 sugar.
- The desugaring covers the bounded sugar listed above; it is not a complete Vue 2 compiler.
- `VueVersion` also names the Vue 1 / 0.11 / 0.10 lines, but only the Vue 2 template dialect drives
  the desugaring above today.

## petite-vue support

[petite-vue](https://github.com/vuejs/petite-vue) is supported as a **standalone HTML document**
surface, not as a `.vue` SFC. A document is classified as petite-vue **structurally**
(`crates/vize_carton/src/dialect.rs`, `detect_petite_vue_document`): a `<script src>` resolving to
the petite-vue package, an ES import of it, or a `PetiteVue.createApp` global call. A comment merely
mentioning "petite-vue" never flips the dialect, and lookalikes such as `petite-vuex` do not match.

Verified-present surfaces:

- **Document parse mode** — `crates/vize_armature/src/parser.rs` (`parse_document` /
  `Parser::new_document`) parses a whole HTML document and tolerates the leading
  `<!DOCTYPE html>` so directives (`v-scope`, `v-effect`, `@click`) on a real page are not rejected.
- **`v-scope` binding modeling** — `crates/vize_croquis/src/drawer/helpers/v_scope.rs`
  (`extract_v_scope_bindings`) extracts the top-level keys of a `v-scope="{ ... }"` object literal
  and models them in the scope chain (as `v-slot`-kind scopes).
- **Lint rules** — `crates/vize_patina/src/rules/petite_vue/`:
  `no_unsupported_directive` flags Vue-3-only directives petite-vue does not support (its built-in
  set includes `v-scope`, `v-effect`, `v-if`/`v-else`/`v-else-if`, `v-for`, `v-show`, `v-html`,
  `v-text`, `v-model`, `v-bind`, `v-on`); `valid_v_scope` requires `v-scope` to bind a
  (possibly empty) object literal. Both are gated on the petite-vue dialect and are no-ops for
  normal Vue SFC linting.
- **LSP** — `v-scope` bindings get completion
  (`crates/vize_maestro/src/ide/completion/template/bindings.rs`), hover
  (`crates/vize_maestro/src/ide/hover/template.rs`,
  `hover_petite_vue_scope_binding`), and go-to-definition
  (`crates/vize_maestro/src/ide/definition/template.rs`).

Honest gaps:

- petite-vue `v-scope` bindings have **no full virtual-document / virtual-TS model**: LSP resolves
  them against the parsed `v-scope` scope chain, not against a generated TypeScript document, so
  type-level features available to `.vue` SFCs are not provided for these bindings.
- The doctype is **tolerated** by document-mode parsing but is **not a first-class AST node**; it is
  consumed so downstream lint/scope analysis can run, rather than represented in the template AST.
- There is **no dedicated `valid-v-effect` lint rule**; `v-effect` is recognized as a supported
  petite-vue directive by `no-unsupported-directive` but its expression is not separately validated.

## Required Release Gates

The release commit must pass these gates before any alpha-supported Vue surface can be described as
production-trial ready:

- `vp run --workspace-root coverage`
- `vp run --workspace-root coverage:source`
- `vp run --workspace-root coverage:source:branch` for nightly branch coverage on core Rust
  compiler crates
- `vp run --filter './tests' test:check:fixtures`
- `rust-script tools/commands/release/npm/smoke-release-install.rs --prepare-manifests --runtime-checks ...`
- `node --test tests/tooling/release-readiness.test.ts tests/tooling/github-workflows.test.ts`

Preview and experimental surfaces may ship only with release notes that call out their status and
known unsupported behavior.
