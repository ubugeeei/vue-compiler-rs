<!-- GENERATED FILE - do not edit by hand.
     Regenerate: rust-script tools/commands/davinci/consumer-migration-surfaces.rs --write
     Verify:     rust-script tools/commands/davinci/consumer-migration-surfaces.rs --check
     Generator:  tools/davinci/consumer-migration-surfaces.mjs -->

# Consumer migration surfaces

This inventory records where the user-facing consumers that must eventually
sit on Davinci/S0/S1/S2 still name stage crates, legacy AST/parser/Croquis
crates, or raw OXC crates directly on current `origin/main`. It is an
observational guard for planning only. It does not change rollout state.

## Resolution method

- Rust comments and string literals are stripped before matching; Cargo
  comments are stripped while dependency keys remain visible.
- Matches are lexical crate/surface names, not type-resolved imports. A row
  means "this file directly names this surface", not necessarily that every
  mention is a runtime dependency edge.
- Stage names are split into preferred physical names and compatibility
  code-name aliases so S0/S1/S2 migration work is measurable without changing
  rollout state.
- `source/manifest` includes production Rust files plus crate manifests.
  `test/dev` includes crate `tests`, `benches`, `tests.rs`,
  `*_tests.rs`, and Rust sites after the first `#[cfg(test)]` in a file.
- Content-mapper files under Canon are reported separately from the broader
  typechecker row so that protocol work can move in smaller PRs.
- Full file x surface x matched-name rows are generated in `davinci-road/plan/consumer-migration-surfaces.tsv`; this
  markdown keeps only top impact files to stay under the source-length gate.

## Surface legend

| surface          | group | matched name classes                                                                                                                               |
| ---------------- | ----- | -------------------------------------------------------------------------------------------------------------------------------------------------- |
| Davinci          | stage | preferred: `vize_davinci`                                                                                                                          |
| S0               | stage | preferred: `vize_s0`<br>compat/code-name: `vize_carton`                                                                                            |
| S1               | stage | preferred: `vize_s1`<br>compat/code-name: `vize_sinopia`                                                                                           |
| S2               | stage | preferred: `vize_s2`<br>compat/code-name: `vize_disegno`                                                                                           |
| S1->S2           | stage | preferred: `vize_s1_to_s2`<br>compat/code-name: `vize_ricalco`                                                                                     |
| old AST/parser   | old   | legacy: `vize_relief`, `vize_armature`                                                                                                             |
| Croquis analysis | old   | legacy: `vize_croquis`, `vize_croquis_cf`                                                                                                          |
| raw OXC          | raw   | raw: `oxc_allocator`, `oxc_ast`, `oxc_ast_visit`, `oxc_codegen`, `oxc_formatter`, `oxc_formatter_core`, `oxc_parser`, `oxc_semantic`, `oxc_syntax` |

## Consumer summary

| consumer                   | stage/Davinci | preferred stage names | compat code names | old AST/Croquis | raw OXC | source/manifest | test/dev | surface files | scanned files |
| -------------------------- | ------------: | --------------------: | ----------------: | --------------: | ------: | --------------: | -------: | ------------: | ------------: |
| Compiler                   |          1129 |                   787 |               342 |             157 |     373 |            1006 |      653 |           556 |           696 |
| Linter                     |           339 |                   339 |                 0 |             295 |     297 |             694 |      237 |           383 |           558 |
| Typechecker                |           898 |                   250 |               648 |             399 |     187 |             812 |      672 |           479 |           690 |
| Typechecker content-mapper |             9 |                     9 |                 0 |               0 |       0 |               9 |        0 |             8 |            20 |
| Formatter                  |            40 |                    40 |                 0 |               0 |      21 |              42 |       19 |            32 |            69 |
| LSP                        |           288 |                   288 |                 0 |             115 |      47 |             336 |      114 |           174 |           410 |

## Consumer details

### Compiler

Scope: build command plus atelier compiler crates. This is a lexical inventory, not a rollout gate.

| surface          | total sites | source/manifest | test/dev |
| ---------------- | ----------: | --------------: | -------: |
| Davinci          |          30 |              10 |       20 |
| S0               |         942 |             495 |      447 |
| S1               |           5 |               1 |        4 |
| S2               |          41 |              21 |       20 |
| S1->S2           |         111 |              13 |       98 |
| old AST/parser   |          85 |              65 |       20 |
| Croquis analysis |          72 |              57 |       15 |
| raw OXC          |         373 |             344 |       29 |

#### Top source and manifest files

| file                                                                         | class    | surfaces                                                                                             | sites |
| ---------------------------------------------------------------------------- | -------- | ---------------------------------------------------------------------------------------------------- | ----: |
| `crates/vize_atelier_sfc/src/rewrite_default.rs:6`                           | source   | S0 1<br>raw OXC 27                                                                                   |    28 |
| `crates/vize_atelier_core/src/codegen/expression/prefix_visitor.rs:7`        | source   | S0 3<br>Croquis analysis 1<br>raw OXC 17                                                             |    21 |
| `crates/vize_atelier_core/Cargo.toml:17`                                     | manifest | Davinci 1<br>S0 1<br>S1 1<br>S2 1<br>S1->S2 1<br>old AST/parser 4<br>Croquis analysis 1<br>raw OXC 7 |    17 |
| `crates/vize_atelier_sfc/src/script/define_props_destructure/collector.rs:6` | source   | S0 2<br>raw OXC 15                                                                                   |    17 |
| `crates/vize_atelier_core/src/steps/expression/prefix.rs:6`                  | source   | S0 1<br>Croquis analysis 1<br>raw OXC 14                                                             |    16 |

Additional source/manifest rows are in the TSV: 324 omitted.

#### Top test/dev files

| file                                                          | class    | surfaces                                      | sites |
| ------------------------------------------------------------- | -------- | --------------------------------------------- | ----: |
| `crates/vize_atelier_vapor/src/tests.rs:4`                    | test/dev | S0 42<br>raw OXC 2                            |    44 |
| `crates/vize_atelier_core/tests/davinci_s2_transform.rs:119`  | test/dev | Davinci 3<br>S0 6<br>S1 3<br>S1->S2 13        |    25 |
| `crates/vize_atelier_core/src/codegen/tests.rs:5`             | test/dev | S0 20<br>old AST/parser 1                     |    21 |
| `crates/vize_atelier_sfc/src/compile_script/props/tests.rs:4` | test/dev | S0 15                                         |    15 |
| `crates/vize_atelier_core/tests/s2_support/compare.rs:10`     | test/dev | Davinci 3<br>S0 4<br>S1 1<br>S2 1<br>S1->S2 4 |    13 |

Additional test/dev rows are in the TSV: 260 omitted.

### Linter

Scope: lint command plus Patina rule engine. This is a lexical inventory, not a rollout gate.

| surface          | total sites | source/manifest | test/dev |
| ---------------- | ----------: | --------------: | -------: |
| S0               |         339 |             231 |      108 |
| old AST/parser   |         253 |             213 |       40 |
| Croquis analysis |          42 |              37 |        5 |
| raw OXC          |         297 |             213 |       84 |

#### Top source and manifest files

| file                                                                                | class    | surfaces                                                    | sites |
| ----------------------------------------------------------------------------------- | -------- | ----------------------------------------------------------- | ----: |
| `crates/vize_patina/src/linter/engine.rs:26`                                        | source   | S0 5<br>old AST/parser 2<br>Croquis analysis 1<br>raw OXC 5 |    13 |
| `crates/vize_patina/Cargo.toml:13`                                                  | manifest | S0 1<br>old AST/parser 2<br>Croquis analysis 1<br>raw OXC 6 |    10 |
| `crates/vize_patina/src/rules/script/no_ref_as_operand.rs:29`                       | source   | S0 1<br>raw OXC 9                                           |    10 |
| `crates/vize_patina/src/rules/opinionated/vue/require_component_registration.rs:46` | source   | S0 2<br>old AST/parser 4<br>Croquis analysis 3              |     9 |
| `crates/vize_patina/src/rules/vue/valid_v_model.rs:29`                              | source   | S0 1<br>old AST/parser 2<br>raw OXC 6                       |     9 |

Additional source/manifest rows are in the TSV: 317 omitted.

#### Top test/dev files

| file                                                                             | class    | surfaces                                       | sites |
| -------------------------------------------------------------------------------- | -------- | ---------------------------------------------- | ----: |
| `crates/vize_patina/src/output/tests.rs:4`                                       | test/dev | S0 23                                          |    23 |
| `crates/vize_patina/src/markup/tests.rs:49`                                      | test/dev | S0 1<br>old AST/parser 3<br>raw OXC 6          |    10 |
| `crates/vize_patina/src/rules/script/no_use_computed_property_like_method.rs:36` | test/dev | S0 1<br>raw OXC 5                              |     6 |
| `crates/vize_patina/src/rules/vue/no_mutating_props.rs:62`                       | test/dev | S0 3<br>old AST/parser 2<br>Croquis analysis 1 |     6 |
| `crates/vize_patina/src/rules/vue/no_unused_components.rs:41`                    | test/dev | S0 1<br>old AST/parser 2<br>Croquis analysis 3 |     6 |

Additional test/dev rows are in the TSV: 69 omitted.

### Typechecker

Scope: check command plus Canon, excluding dedicated content-mapper files. This is a lexical inventory, not a rollout gate.

| surface          | total sites | source/manifest | test/dev |
| ---------------- | ----------: | --------------: | -------: |
| S0               |         898 |             508 |      390 |
| old AST/parser   |         161 |              35 |      126 |
| Croquis analysis |         238 |             118 |      120 |
| raw OXC          |         187 |             151 |       36 |

#### Top source and manifest files

| file                                                                           | class    | surfaces                                                    | sites |
| ------------------------------------------------------------------------------ | -------- | ----------------------------------------------------------- | ----: |
| `crates/vize_canon/src/sfc_typecheck/checks.rs:4`                              | source   | S0 1<br>Croquis analysis 11                                 |    12 |
| `crates/vize/src/commands/check/nuxt/parsing.rs:5`                             | source   | S0 1<br>raw OXC 11                                          |    12 |
| `crates/vize_canon/Cargo.toml:16`                                              | manifest | S0 1<br>old AST/parser 3<br>Croquis analysis 1<br>raw OXC 6 |    11 |
| `crates/vize_canon/src/corsa_bridge/vue_dependencies_alias/context/cache.rs:7` | source   | S0 10                                                       |    10 |
| `crates/vize_canon/src/virtual_ts/expressions/statements.rs:15`                | source   | S0 6<br>Croquis analysis 3                                  |     9 |

Additional source/manifest rows are in the TSV: 314 omitted.

#### Top test/dev files

| file                                                                                         | class    | surfaces                                          | sites |
| -------------------------------------------------------------------------------------------- | -------- | ------------------------------------------------- | ----: |
| `crates/vize_canon/src/virtual_ts/tests.rs:8`                                                | test/dev | S0 37<br>old AST/parser 36<br>Croquis analysis 50 |   123 |
| `crates/vize_canon/src/virtual_ts/tests/options_api_instance.rs:32`                          | test/dev | S0 7<br>old AST/parser 7<br>Croquis analysis 18   |    32 |
| `crates/vize_canon/src/virtual_ts/expressions/component_props_tests.rs:1`                    | test/dev | S0 8<br>old AST/parser 8<br>Croquis analysis 1    |    17 |
| `crates/vize_canon/src/batch/type_checker/tests/recent_issues/template_handler_ts7006.rs:41` | test/dev | S0 16                                             |    16 |
| `crates/vize_canon/src/virtual_ts/strict_template_globals_tests.rs:3`                        | test/dev | S0 8<br>old AST/parser 7<br>Croquis analysis 1    |    16 |

Additional test/dev rows are in the TSV: 193 omitted.

### Typechecker content-mapper

Scope: content-mapper command plus Canon content-mapper protocol files. This is a lexical inventory, not a rollout gate.

| surface | total sites | source/manifest | test/dev |
| ------- | ----------: | --------------: | -------: |
| S0      |           9 |               9 |        0 |

#### Top source and manifest files

| file                                                                             | class  | surfaces | sites |
| -------------------------------------------------------------------------------- | ------ | -------- | ----: |
| `crates/vize_canon/src/batch/virtual_project/content_mapper.rs:8`                | source | S0 2     |     2 |
| `crates/vize_canon/src/batch/virtual_project/content_mapper_alias.rs:1`          | source | S0 1     |     1 |
| `crates/vize_canon/src/batch/virtual_project/content_mapper_component_name.rs:3` | source | S0 1     |     1 |
| `crates/vize_canon/src/batch/virtual_project/content_mapper_directives.rs:12`    | source | S0 1     |     1 |
| `crates/vize_canon/src/batch/virtual_project/content_mapper_protocol.rs:4`       | source | S0 1     |     1 |

Additional source/manifest rows are in the TSV: 3 omitted.

#### Top test/dev files

_No files in this class._

### Formatter

Scope: fmt command, Glyph formatter crate, and LSP format handler. This is a lexical inventory, not a rollout gate.

| surface | total sites | source/manifest | test/dev |
| ------- | ----------: | --------------: | -------: |
| S0      |          40 |              28 |       12 |
| raw OXC |          21 |              14 |        7 |

#### Top source and manifest files

| file                                               | class    | surfaces          | sites |
| -------------------------------------------------- | -------- | ----------------- | ----: |
| `crates/vize_glyph/Cargo.toml:13`                  | manifest | S0 1<br>raw OXC 5 |     6 |
| `crates/vize_glyph/src/options.rs:6`               | source   | S0 1<br>raw OXC 4 |     5 |
| `crates/vize_glyph/src/script/block_identity.rs:4` | source   | S0 1<br>raw OXC 3 |     4 |
| `crates/vize_glyph/src/script.rs:10`               | source   | S0 1<br>raw OXC 2 |     3 |
| `crates/vize_glyph/src/lib.rs:53`                  | source   | S0 2              |     2 |

Additional source/manifest rows are in the TSV: 22 omitted.

#### Top test/dev files

| file                                                 | class    | surfaces          | sites |
| ---------------------------------------------------- | -------- | ----------------- | ----: |
| `crates/vize_glyph/src/script.rs:56`                 | test/dev | S0 1<br>raw OXC 7 |     8 |
| `crates/vize/src/commands/fmt/files.rs:125`          | test/dev | S0 4              |     4 |
| `crates/vize_glyph/src/formatter/block_indent.rs:47` | test/dev | S0 2              |     2 |
| `crates/vize_glyph/src/template.rs:28`               | test/dev | S0 2              |     2 |
| `crates/vize_glyph/src/style/stabilization.rs:226`   | test/dev | S0 1              |     1 |

Additional test/dev rows are in the TSV: 2 omitted.

### LSP

Scope: lsp/ide commands plus Maestro editor/server crate. This is a lexical inventory, not a rollout gate.

| surface          | total sites | source/manifest | test/dev |
| ---------------- | ----------: | --------------: | -------: |
| S0               |         288 |             197 |       91 |
| old AST/parser   |          61 |              48 |       13 |
| Croquis analysis |          54 |              44 |       10 |
| raw OXC          |          47 |              47 |        0 |

#### Top source and manifest files

| file                                                            | class    | surfaces                                                    | sites |
| --------------------------------------------------------------- | -------- | ----------------------------------------------------------- | ----: |
| `crates/vize_maestro/src/server/state/config.rs:6`              | source   | S0 30                                                       |    30 |
| `crates/vize_maestro/src/ide/type_service/type_context.rs:229`  | source   | S0 14                                                       |    14 |
| `crates/vize_maestro/src/ide/corsa_support/html_attribute.rs:2` | source   | S0 10                                                       |    10 |
| `crates/vize_maestro/Cargo.toml:44`                             | manifest | S0 1<br>old AST/parser 2<br>Croquis analysis 1<br>raw OXC 5 |     9 |
| `crates/vize_maestro/src/ide/diagnostics/linter_options.rs:6`   | source   | S0 8                                                        |     8 |

Additional source/manifest rows are in the TSV: 119 omitted.

#### Top test/dev files

| file                                                             | class    | surfaces                   | sites |
| ---------------------------------------------------------------- | -------- | -------------------------- | ----: |
| `crates/vize_maestro/src/server/state/virtual_docs.rs:69`        | test/dev | S0 7<br>old AST/parser 2   |     9 |
| `crates/vize_maestro/src/virtual_code/template_code_tests.rs:12` | test/dev | S0 4<br>old AST/parser 4   |     8 |
| `crates/vize_maestro/src/ide/inlay_hint.rs:21`                   | test/dev | S0 4<br>Croquis analysis 3 |     7 |
| `crates/vize_maestro/src/server/state.rs:37`                     | test/dev | S0 7                       |     7 |
| `crates/vize_maestro/src/server/state/config_tests.rs:1`         | test/dev | S0 5                       |     5 |

Additional test/dev rows are in the TSV: 54 omitted.

## Independently mergeable no-rollout slices

1. `test(davinci): pin consumer migration surfaces` - this artifact and its
   drift test. It makes the current dependency shape reviewable without
   changing command routing or defaults.
2. `refactor(compiler): introduce stage-named compiler boundary adapters` -
   add S0/S1/S2 adapter entrypoints inside the atelier crates while continuing
   to feed the existing Relief/Croquis pipeline. Guard with compiler fixture
   parity and keep the `vize build` path unchanged.
3. `refactor(linter): add template analysis facade` - move rule code toward
   a stable analysis contract while the facade is still backed by
   Relief/Croquis. Guard with lint divergence and rule fixture snapshots; no
   default linter backend switch.
4. `refactor(typechecker): add virtual document boundary` - introduce a
   narrow S0/S1 input contract for virtual TS generation and adapt current
   callers into it. Guard with the existing typecheck fixture matrix and
   real-project rows.
5. `test(content-mapper): pin stage-neutral mapping protocol fixtures` -
   expand content-mapper protocol fixtures around spans, virtual extensions,
   package routes, and declaration-map lookups. Keep the external tsgo protocol
   byte-compatible.
6. `refactor(formatter): isolate region formatting plan` - keep Glyph/OXC
   output unchanged, but put region extraction and script formatting behind a
   stage-neutral formatting plan. Guard with idempotence and range-formatting
   fixtures.
7. `refactor(lsp): add current-backend adapter boundary` - route Maestro
   document/virtual-code feature inputs through a backend trait whose first
   implementation delegates to the current Armature/Croquis/Canon stack. Guard
   hover, definition, diagnostics, semantic tokens, and formatting with
   existing LSP e2e tests.
8. `refactor(davinci): align physical layer names with s0/s1/s2` - migrate
   public internal module/crate references toward S0/S1/S2 naming in small
   aliasing steps. Keep code names only as compatibility aliases until all
   consumers have moved.

Rollout remains explicitly out of scope for these slices: none should switch
user-visible defaults, command dispatch, package exports, editor activation, or
protocol behavior.

## Regeneration

```sh
rust-script tools/commands/davinci/consumer-migration-surfaces.rs --write
rust-script tools/commands/davinci/consumer-migration-surfaces.rs --check
```
