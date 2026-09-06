//! TS-17 for the hoist-static analysis (P2-9 series 6): committed
//! fixture in, pipeline out, **full normalized folio** snapshot — the
//! P2-4 harness shape. An analysis pass moves no surface, so the folio
//! shows exactly what the lowering built (the snapshot IS the
//! fact-not-mutation proof); the published lattice, the walk
//! accounting, and the empty diagnostics channel are the structural
//! supplements (assurance §4).

mod support;

use std::path::{Path, PathBuf};

use vize_davinci::assert_folio_snapshot;
use vize_davinci::folio::{Folio, FolioMode};
use vize_s1_to_s2::pass::{StaticFacts, StaticLevel};

use support::{assert_transformed_sound, with_transformed};

fn fixture(name: &str) -> vize_s0::String {
    let path: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("hoist")
        .join(name);
    let text = std::fs::read_to_string(path).expect("committed fixture reads");
    vize_s0::String::from(text.as_str())
}

fn rows(facts: &vize_davinci::side_table::SideTable<StaticFacts>) -> Vec<(u32, StaticFacts)> {
    facts
        .sorted_entries()
        .into_iter()
        .map(|(id, fact)| (id.index(), *fact))
        .collect()
}

#[test]
fn the_levels_fixture_snapshots_the_post_pass_folio() {
    let source = fixture("levels.vue");
    with_transformed(&source, |lowered, folio, facts, budget| {
        // The oracle: the full normalized folio after the pipeline ran
        // — byte-identical to the pre-pass folio, because the analysis
        // mutates nothing.
        assert_folio_snapshot!(*folio);

        // Supplements: five model-free walks (four barriers plus the
        // fusable analysis singleton).
        assert_eq!(
            budget.print_to_string(FolioMode::Full).as_str(),
            "[budget-observer]\nwalks=5\npasses=5\nanalyses=0\npipelines=1\nfailures=0\n\n"
        );
        // The lattice, row by row: the section is dynamic (its children
        // are), the h1 subtree fully static with hoistable props, the
        // greeting on the dynamic-text rung, the `<svg>` blocked by its
        // directive with a still-hoistable surface, the `ref` anchor
        // blocked, and the mixed `<i>` blocked by the recorded weaker
        // const rule (`Math.PI` refuses) while `.value`/`:pad` alone
        // would have hoisted.
        use StaticLevel::{FullyStatic, HasDynamicText, NotStatic};
        let levels: Vec<(u32, StaticLevel, bool)> = rows(&facts.static_facts)
            .into_iter()
            .map(|(id, fact)| (id, fact.level, fact.props_hoistable))
            .collect();
        assert_eq!(
            levels,
            vec![
                (0, NotStatic, true),
                (1, FullyStatic, true),
                (2, FullyStatic, false),
                (4, HasDynamicText, true),
                (6, NotStatic, true),
                (8, FullyStatic, true),
                (9, NotStatic, false),
                (11, NotStatic, false),
            ]
        );
        // An analysis pass emits no diagnostics — the Optional
        // classification's ground.
        assert_eq!(lowered.diagnostics, vec![]);
    });
    assert_transformed_sound(&source, "levels.vue");
}

#[test]
fn the_positions_fixture_snapshots_the_post_pass_folio() {
    let source = fixture("positions.vue");
    with_transformed(&source, |_, folio, facts, _| {
        assert_folio_snapshot!(*folio);

        // The owner census: article, header, em, the branch p, the
        // iterated li and its u, Card and its carrier template with the
        // s inside — every element/component op carries exactly one
        // fact, outlets and text-family ops none.
        let rows = rows(&facts.static_facts);
        assert_eq!(rows.len(), 9, "nine owners carry facts: {rows:?}");
        // The structural carriers lowered out of element position
        // (template v-if / v-for wrappers) leave no owner behind — the
        // facts live on the unwrapped content.
        assert!(
            rows.iter()
                .all(|(_, fact)| fact.native_descendants || fact.level == StaticLevel::NotStatic),
            "components and carriers break the native predicate: {rows:?}"
        );
    });
    assert_transformed_sound(&source, "positions.vue");
}
