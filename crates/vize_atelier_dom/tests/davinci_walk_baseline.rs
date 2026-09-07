//! P2-12a pre-S2 traversal baseline and the DOM production-selector floor.
//!
//! One fused compile per ladder fixture, diffing
//! `vize_atelier_core::walk_probe` around it: the template-node visits and
//! stage tree-walks the shipped pipeline makes. `BASELINE` is the pre-S2
//! sweep that produced `davinci-road/plan/walk-baseline.md` and filled
//! `budgets.toml [traversal]`; it stays fixed as the "before" budget.
//! `PRODUCTION_FLOOR` pins the source-map-free DOM selector after the S2
//! switch. Any increase means a stage started walking the legacy tree again
//! and must be re-derived deliberately (`--nocapture` prints every row and
//! its per-stage breakdown).
//!
//! The same run also pins Davinci's side of the tie: the walks measured here
//! must not exceed `vize_atelier_core::legacy_plan::DOM.group_count()`, so the
//! pass-manager plan that described the old shipped pipeline remains the
//! upper bound. The plans live in `vize_davinci` and are read from the
//! dev-dependencies: a published crate cannot depend on an unpublished one.
//!
//! The probe is process-global and monotone, so this file holds a single
//! `#[test]` in its own binary - the `davinci_expr_reparse_floor.rs` shape.

use davinci_harness::fixtures::{LADDER, template_block};
use std::fmt::Write as _;
use vize_atelier_core::walk_probe::{WALK_STAGES, WalkCounts};
use vize_atelier_dom::{DomCompilerOptions, compile_template_with_options};
use vize_davinci::legacy_plan;
use vize_s0::{Allocator, String};

/// fixture name -> (stage tree-walks, template-node visits) per fused compile.
const BASELINE: [(&str, u64, u64); 6] = [
    ("small", 2, 11),
    ("medium", 2, 62),
    ("large", 2, 86),
    ("stress-deep", 2, 134),
    ("stress-wide", 2, 3),
    ("stress-interp", 2, 1102),
];

/// fixture name -> (stage tree-walks, template-node visits) after the
/// source-map-free DOM production selector routes supported compiles through S2.
const PRODUCTION_FLOOR: [(&str, u64, u64); 6] = [
    ("small", 0, 0),
    ("medium", 0, 0),
    ("large", 0, 0),
    ("stress-deep", 0, 0),
    ("stress-wide", 0, 0),
    ("stress-interp", 0, 0),
];

#[test]
fn dom_walk_baseline_holds() {
    let mut measured: Vec<(&str, u64, u64)> = Vec::new();

    for fixture in &LADDER {
        let template =
            template_block(fixture.source).expect("every ladder fixture has a template block");
        let allocator = Allocator::new();
        let before = WalkCounts::snapshot();
        let _compiled =
            compile_template_with_options(&allocator, template, DomCompilerOptions::default());
        let delta = WalkCounts::snapshot().since(before);

        let mut breakdown = String::default();
        for stage in WALK_STAGES.iter().filter(|stage| {
            delta.visits[**stage as usize] != 0 || delta.walks[**stage as usize] != 0
        }) {
            let _ = write!(
                &mut breakdown,
                " {}={}/{}",
                stage.as_str(),
                delta.walks[*stage as usize],
                delta.visits[*stage as usize]
            );
        }
        println!(
            "davinci.walk dom {} walks={} visits={}{}",
            fixture.name,
            delta.total_walks(),
            delta.total_visits(),
            breakdown
        );

        // The pre-S2 plan is now the upper bound for the production selector.
        assert!(
            delta.total_walks() as usize <= legacy_plan::DOM.group_count(),
            "dom {}: the measured walks exceed legacy_plan::DOM",
            fixture.name
        );

        measured.push((fixture.name, delta.total_walks(), delta.total_visits()));
    }

    let expected = rows_for_ladder(&PRODUCTION_FLOOR, "production floor");

    assert_eq!(
        measured, expected,
        "dom: the S2 production traversal floor moved from its pin"
    );
    let baseline = rows_for_ladder(&BASELINE, "pre-S2 baseline");
    for ((fixture, walks, visits), (_, baseline_walks, baseline_visits)) in
        measured.iter().zip(baseline.iter())
    {
        assert!(
            walks <= baseline_walks && visits <= baseline_visits,
            "dom {fixture}: production traversal exceeded the pre-S2 baseline"
        );
    }
}

fn rows_for_ladder(
    rows: &[(&'static str, u64, u64)],
    label: &str,
) -> Vec<(&'static str, u64, u64)> {
    LADDER
        .iter()
        .map(|fixture| {
            *rows
                .iter()
                .find(|(name, _, _)| *name == fixture.name)
                .unwrap_or_else(|| panic!("ladder fixture {} has no pinned {label}", fixture.name))
        })
        .collect()
}
