<!-- GENERATED FILE — do not edit by hand.
     Regenerate: rust-script tools/commands/davinci/sourcelocation-inventory.rs --write
     Verify:     rust-script tools/commands/davinci/sourcelocation-inventory.rs --check
     Generator:  tools/davinci/sourcelocation-inventory.mjs -->

# `SourceLocation` consumer inventory

Every textual read of the relief `SourceLocation` members that
`vize_carton::Span` (`crates/vize_carton/src/span.rs`) deleted —
`source`, `start.line`, `start.column`, `end.line`, `end.column` —
across `crates/*/src`, plus the migration group each consumer moved to as
relief nodes switched to two-u32 byte spans instead of owned
`{ start: Position, end: Position, source: String }` triples
([architecture.md](../architecture.md), S0). This was the migration map
Davinci P1 executed (P0-9). P1-3 executed group 1 (`source` reads to
`Span::slice`); P1-4 executed groups 2 and 3 (line/column reads to
offset-derived rendering, the `Position` type and its converters deleted).
The scan now counts zero across all five members, and regeneration fails if
any read — or any deleted carrier — comes back.

## Resolution method (and its limits)

- Comments and string literals are stripped first
  (`tools/davinci/lib/rust-source.mjs`); member paths are then counted
  **textually** on loc-shaped receivers only: chained `.loc.<member>`,
  `.loc().<member>`, `.location.<member>` accesses, and bare locals named
  `loc` / `location` / `*_loc` / `*_location`.
- Reads of `span.start` / `span.end` are **not** inventoried: they are
  the surviving offset representation — the pre-migration
  `start.offset` / `end.offset` reads moved to them verbatim
  (292 loc-shaped span-read sites across
  8 crates at generation time).
- `#[cfg(test)]` code inside `src/` is included and reported in the
  "in test code" column: a site counts as test code when its file is a test
  module by name (`tests.rs`, `*_tests.rs`, `/tests/`) or sits at or
  after the file's first `#[cfg(test)]` attribute.
- The scan is not type-resolved. Known imprecision, spot-checked:
  - `vize_doctor` defines a namesake path+offset `SourceLocation`
    (`crates/vize_doctor/src/model/evidence.rs`) that is already
    span-shaped; its members (`path`, `start: u32`, `end: u32`) never
    form any of the five member paths, so it cannot contribute rows here and
    needed no migration.
  - `BlockLocation` (`crates/vize_atelier_sfc/src/types.rs`) also sits
    behind `loc` fields, but its `start`/`end` are plain `usize`
    offsets with flat `start_line`/`start_column` siblings — none of the
    five member paths exist on it, so it cannot collide either.

## Reads per crate × member

| crate     | `source` | `start.line` | `start.column` | `end.line` | `end.column` | total | in test code |
| --------- | -------: | -----------: | -------------: | ---------: | -----------: | ----: | -----------: |
| **total** |        0 |            0 |              0 |          0 |            0 |     0 |            0 |

## Migration groups

### Group 1 — content reads moved to `Span::slice` (migrated by P1-3: 106 sites at P0-9, 0 remain)

The dominant consumer class by far: code that wants **the text a node
covers** — codegen re-emitting an expression, croquis capturing a binding
name, a lint rule inspecting raw expression text, a test asserting what the
parser captured. Each read used to pay for an owned `String` copied into
the node at parse time; the node now stores 8 bytes and the read is
`span.slice(source)` against the one authored source string (or, for
block-relative spans, against that block's text). Representative migrated
sites:

- `crates/vize_atelier_core/src/codegen/expression/generate.rs:39` — codegen reads the recorded expression text verbatim
- `crates/vize_croquis/src/drawer/template/components.rs:48` — croquis captures component/expression text into
  analysis products
- `crates/vize_patina/src/rules/script/template_scan.rs:193` — lint rule matches against raw expression text
- `crates/vize_atelier_vapor/src/steps/v_on.rs:46` — vapor transform re-wraps an expression from the covered
  text (the owned copy the pre-span node stored is gone; see also group 3)

### Group 2 — line/column reads moved to offset-derived rendering (migrated by P1-4: 3 direct sites + 4 known-missed at P0-9, 0 remain)

Line/column exist only at diagnostic- or LSP-rendering time under Davinci:
derived from byte offsets via `vize_carton::line_index::LineIndex`
(`crates/vize_carton/src/line_index.rs:23`) at the edge that needs them — exactly how the
source-map `finish()` step and Patina's output layer already worked. The
eagerly-stored `Position { line, column }` pairs deleted with the type.
Where each read went:

- `crates/vize_armature/src/parser/element/comment.rs:21` — the only production read seeded
  `parse_vize_directive` with the comment's start line, which that caller
  discards (only the directive kind survives); it now passes the constant
  line the retired tracking always reported
- `crates/vize_relief/src/errors/render.rs:86` — the one output path that printed stored
  line/column (the SFC gate / binding-boundary debug rendering) now derives
  display coordinates from the rendered source text via `LineIndex`. This
  keeps `SourceLocation` span-only while making multiline diagnostics point
  at the actual line and column instead of the retired frozen
  `line: 1, column: offset + 1` approximation
- `crates/vize_maestro/src/utils/position.rs` — `source_location_to_range`
  / `internal_to_lsp_position` converted stored `Position`s to LSP
  positions; they had no callers and are deleted (regeneration asserts they
  stay gone)
- `crates/vize_relief/src/relief/tests.rs:360` — relief tests that pinned the 1-indexed line/column
  convention of stub locations now pin span offsets (the 1-based convention
  itself is a rendering-layer concern)

### Group 3 — deleted outright

Structures whose entire job was carrying or converting the eager
representation:

- `crates/vize_relief/src/relief/core.rs` — the `Position` type itself
  is gone; `SourceLocation` is the 8-byte `{ span: Span }` (regeneration
  asserts `pub struct Position` stays gone from the module)
- `crates/vize_atelier_jsx/src/span.rs:15` — `SpanMapper` no longer expands oxc byte spans into
  eager positions; `location()` is a direct offset carry-over and the
  `LineIndex` it built per module is deleted
- `crates/vize_relief/src/relief/core.rs:104` — `STUB_LOCATION` / `SourceLocation::STUB` collapsed to
  `Span::new(0, 0)`
- `crates/vize_relief/src/relief/expressions.rs:33` — `SimpleExpressionNode` stored `content: String`
  **and** `loc.source` duplicating it; since P1-3 the node keeps one span
  next to `content`, and every group-1 site that cloned `loc.source` to
  build another node (e.g. `crates/vize_atelier_vapor/src/steps/v_on.rs:46`) slices on demand instead
