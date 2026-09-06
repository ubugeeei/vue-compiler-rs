//! TS-17 for the `v-for` pass (P2-9 series 2): committed fixture in,
//! pipeline out, **full normalized folio** snapshot — the P2-4 harness
//! shape, as `vif_pass_snapshot.rs` applied it to installment 1.
//!
//! The snapshot is the oracle; the walk accounting, the consumed-scope
//! facts and the diagnostics are the targeted structural supplements
//! (assurance §4). Since the v-for pass preserves the tree, what the
//! snapshots pin is the *lowered loop shape surviving the pipeline
//! untouched* — the iterated `li`'s `key` still on its attribute
//! surface — while the vif pass's extractions (the `term` branch key in
//! `loops.vue`) still show as moved.

mod support;

use std::path::{Path, PathBuf};

use vize_davinci::assert_folio_snapshot;
use vize_davinci::folio::{Folio, FolioMode};
use vize_s1_to_s2::pass::vfor::ForName;

use support::{assert_transformed_sound, with_transformed};

fn fixture(name: &str) -> vize_s0::String {
    let path: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("vfor")
        .join(name);
    let text = std::fs::read_to_string(path).expect("committed fixture reads");
    vize_s0::String::from(text.as_str())
}

#[test]
fn the_loops_fixture_snapshots_the_post_pass_folio() {
    let source = fixture("loops.vue");
    with_transformed(&source, |lowered, folio, facts, budget| {
        // The oracle: the full normalized folio after the pipeline ran.
        assert_folio_snapshot!(*folio);

        // Supplements: the model-free plan's walk accounting through
        // the budget observer's own derived page.
        assert_eq!(
            budget.print_to_string(FolioMode::Full).as_str(),
            "[budget-observer]\nwalks=5\npasses=5\nanalyses=0\npipelines=1\nfailures=0\n\n"
        );
        // Three loops, consumed in document order with fresh tags: the
        // keyed `li`, the destructuring `<template v-for>`, the
        // absent-alias `p`.
        let entries = facts.for_facts.sorted_entries();
        assert_eq!(entries.len(), 3);
        assert_eq!(
            entries
                .iter()
                .map(|(_, fact)| (
                    fact.tag.index(),
                    fact.value.clone(),
                    fact.key.clone(),
                    fact.index.clone()
                ))
                .collect::<Vec<_>>(),
            vec![
                (
                    0,
                    ForName::Named("item".into()),
                    ForName::Named("i".into()),
                    ForName::Absent
                ),
                (
                    1,
                    ForName::Pending,
                    ForName::Named("index".into()),
                    ForName::Absent
                ),
                (2, ForName::Absent, ForName::Absent, ForName::Absent),
            ]
        );
        // The vif pass still extracted the `dt` branch key inside the
        // template loop; the two passes compose.
        assert_eq!(facts.if_facts.len(), 1);
        // All three loops are valid Vue: no diagnostics.
        assert_eq!(lowered.diagnostics, vec![]);
    });
    assert_transformed_sound(&source, "loops.vue");
}

#[test]
fn the_holes_fixture_snapshots_the_post_pass_folio() {
    let source = fixture("holes.vue");
    with_transformed(&source, |lowered, folio, facts, _| {
        // The undecomposable value keeps its op (pessimal escape); the
        // expressionless v-for keeps only its element. Both errors are
        // the lowering's — the pass adds none.
        assert_folio_snapshot!(*folio);
        assert_eq!(lowered.diagnostics.len(), 2);
        assert_eq!(
            lowered.diagnostics[0].message.as_str(),
            "v-for has invalid expression."
        );
        assert_eq!(
            lowered.diagnostics[1].message.as_str(),
            "v-for is missing expression."
        );
        let entries = facts.for_facts.sorted_entries();
        assert_eq!(entries.len(), 1, "only the kept op is a consumption site");
        assert_eq!(entries[0].1.value, ForName::Pending);
    });
    assert_transformed_sound(&source, "holes.vue");
}
