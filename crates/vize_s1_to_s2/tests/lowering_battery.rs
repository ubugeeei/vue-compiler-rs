//! TS-20's deterministic half, plain lane: the S1 battery (well-formed
//! and malformed alike) through the full parse → lower pipeline, plus
//! every prefix and suffix truncation — adversarial committed inputs,
//! no fuzzer required, same soundness oracle everywhere.
//!
//! The battery is `vize_s1`'s own committed fixture set, imported
//! from the shared Davinci test-support crate. The aggregate counts below are
//! the cfg-regression witness the corpus lane re-pins: a change to the
//! lowering's decision surface moves a pinned number loudly in both lanes.

mod support;

use davinci_test_support::surface_fixture as battery;
use support::{assert_authored_artifact, assert_sound, with_lowered};
use vize_davinci::diagnostic::{Diagnostic, Severity, Stage};
use vize_davinci::folio::{Folio, FolioMode};
use vize_s0::{Allocator, SourceRoot, Span};
use vize_s1::parse;
use vize_s2::folio::DisegnoFolio;
use vize_s2::verify::{Rigor, Violation, verify};

#[test]
fn the_battery_scope_is_pinned() {
    assert_eq!(battery::WELL_FORMED.len(), 16);
    assert_eq!(battery::MALFORMED.len(), 26);
}

#[test]
fn every_battery_fixture_lowers_soundly() {
    for fixture in battery::WELL_FORMED.iter().chain(battery::MALFORMED) {
        assert_sound(fixture.source, fixture.name);
    }
}

#[test]
fn plain_lowering_soundness_covers_every_lowering_side_table() {
    let source = r#"<template v-if="ok" key="branch">hi {{ name }}</template><template v-for="item in items" :key="item.id"><span>{{ item }}</span></template>"#;
    with_lowered(source, |lowered, _folio| {
        assert_eq!(
            (
                lowered.texts.len(),
                lowered.wrappers.len(),
                lowered.for_wrappers.len()
            ),
            (1, 1, 1),
            "the canary must exercise every lowering-owned side table"
        );
    });
    assert_sound(source, "lowering-side-table-canary");
}

#[test]
fn every_truncation_of_every_fixture_lowers_soundly() {
    // The EOF-adversarial lane: each fixture cut at every char boundary,
    // from both ends — the recovery paths TS-19 hammers, continued into
    // S2. Deterministic, committed, and panic-free by the totality
    // contract.
    for fixture in battery::WELL_FORMED.iter().chain(battery::MALFORMED) {
        let source = fixture.source;
        for (end, _) in source.char_indices() {
            assert_sound(&source[..end], fixture.name);
        }
        for (start, _) in source.char_indices() {
            assert_sound(&source[start..], fixture.name);
        }
    }
}

#[test]
fn a_source_block_lowering_keeps_file_absolute_spans() {
    let source = "prelude\n<template><div><span>x</div></template>";
    let template_start = source.find("<template>").expect("template") + "<template>".len();
    let template_end = source.find("</template>").expect("template close");
    let template = &source[template_start..template_end];
    let root = SourceRoot::new(source).expect("source root");
    let block = root
        .block(template, template_start as u32)
        .expect("template block is a root slice");

    let allocator = Allocator::new();
    let (tree, errors) = parse(&allocator, template);
    let lowered = vize_s1_to_s2::lower_source_block(&allocator, &tree, &errors, block);
    let folio = DisegnoFolio::of(&lowered.root.ops);

    assert_authored_artifact(source, &lowered);
    assert_eq!(u64::from(lowered.op_count), folio.op_count());
    assert_eq!(verify(&folio, Rigor::Canonical), Vec::<Violation>::new());
    assert_eq!(
        folio.print_to_string(FolioMode::Full).as_str(),
        "\
[disegno]
ops=3

[disegno.ops]
ui.element div @18:36
  ui.element span @23:30
    ui.text \"x\" @29:30

"
    );
    assert_eq!(
        lowered.diagnostics,
        vec![Diagnostic::new(
            Severity::Error,
            Stage::Surface,
            Span::new(23, 28),
            "Element is missing end tag.",
        )]
    );
}

#[test]
fn source_block_tokenizer_errors_are_file_absolute() {
    let source = "prelude\n<template><div / a>x</div></template>";
    let template_start = source.find("<template>").expect("template") + "<template>".len();
    let template_end = source.find("</template>").expect("template close");
    let template = &source[template_start..template_end];
    let root = SourceRoot::new(source).expect("source root");
    let block = root
        .block(template, template_start as u32)
        .expect("template block is a root slice");

    let allocator = Allocator::new();
    let (tree, errors) = parse(&allocator, template);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].offset, 4);
    assert_eq!(errors[0].code.message(), "Unexpected solidus in tag.");

    let lowered = vize_s1_to_s2::lower_source_block(&allocator, &tree, &errors, block);
    let folio = DisegnoFolio::of(&lowered.root.ops);

    assert_authored_artifact(source, &lowered);
    assert_eq!(u64::from(lowered.op_count), folio.op_count());
    assert_eq!(verify(&folio, Rigor::Canonical), Vec::<Violation>::new());
    assert_eq!(
        lowered.diagnostics,
        vec![Diagnostic::new(
            Severity::Error,
            Stage::Surface,
            Span::new(22, 22),
            "Unexpected solidus in tag.",
        )]
    );
}

#[test]
fn tokenizer_error_diagnostics_snap_to_utf8_boundary() {
    let source = std::str::from_utf8(
        b"\n  <div>{{ props.title }}</div\x1d\"vm\xd7\xa3p=\n\xd7\xa3\0s_bme\">plate v-if=\"-vhow=\"\n  <te-plate v-if=\"vm\xd7\xa3p>\n",
    )
    .expect("reproducer is valid UTF-8");
    let mid_char_offset = 40;
    assert!(!source.is_char_boundary(mid_char_offset));

    let allocator = Allocator::new();
    let (tree, errors) = parse(&allocator, source);
    assert!(
        errors
            .iter()
            .any(|error| error.offset == mid_char_offset as u32),
        "the committed reproducer must keep covering a tokenizer error inside a UTF-8 scalar"
    );

    let lowered = vize_s1_to_s2::lower(&allocator, &tree, &errors);
    assert_authored_artifact(source, &lowered);
    assert!(
        lowered
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.span == Span::new(39, 39)),
        "the mid-scalar tokenizer offset should snap down to the preceding char boundary"
    );
}

#[test]
fn the_battery_aggregates_are_pinned() {
    // The decision-surface census over the whole battery: ops minted,
    // diagnostics raised, provenance records written, scopes tagged.
    // Exact numbers, so any lowering change (or a cfg regression that
    // disarms a lane) moves them loudly.
    let mut ops = 0u64;
    let mut diagnostics = 0usize;
    let mut provenance = 0usize;
    let mut scopes = 0usize;
    for fixture in battery::WELL_FORMED.iter().chain(battery::MALFORMED) {
        with_lowered(fixture.source, |lowered, _folio| {
            ops += u64::from(lowered.op_count);
            diagnostics += lowered.diagnostics.len();
            provenance += lowered.provenance.len();
            scopes += lowered.scopes.len();
        });
    }
    // Re-pinned at the element/binding-family installment (P2-9 series
    // 5): every battery `v-bind`/`v-on` now lowers to a `ui.bind`/`ui.on`
    // op instead of its `defer.v-bind`/`defer.v-on` Info (78 -> 83 ops,
    // 33 -> 28 diagnostics — five retired deferrals across the battery).
    // Scopes are unchanged on purpose. Records first kept parity because
    // each retired `defer.*` record was replaced by exactly one
    // `lower.bind`/`lower.on` record, then fact fusion added four
    // `lower.text-fact` provenance records for battery text-bearing nodes and
    // one `lower.if` record for the battery's branch root.
    // (Series-4 history: condense/merge re-pinned 89 -> 78 ops,
    // 107 -> 101 records.)
    assert_eq!(
        (ops, diagnostics, provenance, scopes),
        (83, 28, 106, 1),
        "battery census moved: re-pin deliberately"
    );
}
