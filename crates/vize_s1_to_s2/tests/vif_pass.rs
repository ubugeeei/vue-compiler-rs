//! The `v-if` pass (P2-9 series 1): semantics pins.
//!
//! Exact-equality oracles over the pass's three products — the moved
//! branch-key facts, the duplicate-key diagnostics, and the provenance
//! trail — plus the classification and rigor laws the review point
//! names. The TS-17 folio snapshots live in `vif_pass_snapshot.rs`.

mod support;

use vize_davinci::diagnostic::{Severity, Stage};
use vize_davinci::id::NodeId;
use vize_davinci::pass::PassEvent;
use vize_s0::Span;
use vize_s1_to_s2::pass::{BranchKey, BranchKeyKind, TRANSFORM, vif};
use vize_s2::verify::{Rigor, VerifyObserver};

use support::{assert_transformed_sound, with_transformed};
use vize_s2::folio::{DisegnoFolio, FolioAttribute, FolioElement, FolioOp};

/// The single root element of branch `index` of the `chain`-th root op,
/// reached through a `ui.for` wrap when the branch carries one.
fn branch_root(folio: &DisegnoFolio, chain: usize, index: usize) -> &FolioElement {
    let FolioOp::If(if_op) = &folio.ops[chain] else {
        panic!("root op {chain} is not ui.if");
    };
    match &if_op.branches[index].ops[..] {
        [FolioOp::Element(element)] => element,
        [FolioOp::For(for_op)] => match &for_op.ops[..] {
            [FolioOp::Element(element)] => element,
            other => panic!("for region root is not one element: {other:?}"),
        },
        other => panic!("branch root is not one element: {other:?}"),
    }
}

/// The span of the `occurrence`-th `needle` in `source` (0-based), as
/// the lowering records attribute spans (name through closing quote).
fn span_of(source: &str, needle: &str, occurrence: usize) -> Span {
    let mut from = 0usize;
    for _ in 0..occurrence {
        from = source[from..].find(needle).expect("occurrence exists") + from + needle.len();
    }
    let start = source[from..].find(needle).expect("occurrence exists") + from;
    Span::new(
        u32::try_from(start).expect("fixture fits u32"),
        u32::try_from(start + needle.len()).expect("fixture fits u32"),
    )
}

#[test]
fn extraction_moves_the_key_from_the_attribute_surface_to_a_fact() {
    let source = r#"<div v-if="a" key="x">1</div><div v-else key="y">2</div>"#;
    with_transformed(source, |lowered, folio, facts, _| {
        // The attribute surface no longer carries the keys: both branch
        // roots hold exactly no attributes (exact oracle, TS-13).
        for index in [0usize, 1] {
            assert_eq!(
                branch_root(folio, 0, index).attributes,
                vec![],
                "branch {index} must have an empty attribute surface"
            );
        }
        // The fact rides beside the tree, keyed by the `ui.if` op (%0).
        assert_eq!(
            facts.if_facts.sorted_entries(),
            vec![(
                NodeId::from_index(0).expect("op 0 has an id"),
                &vif::IfFacts {
                    branches: vec![
                        Some(BranchKey {
                            kind: BranchKeyKind::Static(Some("x".into())),
                            span: span_of(source, r#"key="x""#, 0),
                        }),
                        Some(BranchKey {
                            kind: BranchKeyKind::Static(Some("y".into())),
                            span: span_of(source, r#"key="y""#, 0),
                        }),
                    ],
                }
            )]
        );
        // No collision, no diagnostics.
        assert_eq!(lowered.diagnostics, vec![]);
        // Both extractions left records, in branch order; the series-6
        // analysis pass records its per-owner facts behind them (both
        // carriers fully static once their keys moved to facts).
        let rules: Vec<(&str, &str)> = lowered
            .provenance
            .iter()
            .filter(|record| record.rule.starts_with("pass."))
            .map(|record| (record.rule.as_str(), record.before.as_str()))
            .collect();
        assert_eq!(
            rules,
            vec![
                ("pass.v-if.branch-key", "key=\"x\""),
                ("pass.v-if.branch-key", "key=\"y\""),
                ("pass.hoist-static.fact", "ui.element"),
                ("pass.hoist-static.fact", "ui.element"),
            ]
        );
    });
    assert_transformed_sound(source, "extraction");
}

#[test]
fn a_bare_key_is_a_fact_that_never_collides() {
    let source = r#"<p v-if="a" key>1</p><p v-else key>2</p>"#;
    with_transformed(source, |lowered, _, facts, _| {
        assert_eq!(lowered.diagnostics, vec![]);
        let entries = facts.if_facts.sorted_entries();
        assert_eq!(entries.len(), 1);
        let branches = &entries[0].1.branches;
        assert_eq!(branches.len(), 2);
        for branch in branches {
            let key = branch.as_ref().expect("both bare keys extract");
            assert_eq!(
                key.kind,
                BranchKeyKind::Static(None),
                "a bare key has no authored value"
            );
            assert_eq!(key.collision_text(), None);
        }
    });
}

#[test]
fn duplicate_authored_keys_flag_every_later_branch_exactly() {
    let source =
        r#"<p v-if="a" key="dup">1</p><p v-else-if="b" key="dup">2</p><p v-else key="dup">3</p>"#;
    with_transformed(source, |lowered, _, _, _| {
        assert_eq!(lowered.diagnostics.len(), 2, "branches 1 and 2 collide");
        for (diagnostic, occurrence) in lowered.diagnostics.iter().zip([1usize, 2]) {
            assert_eq!(diagnostic.severity, Severity::Error);
            assert_eq!(diagnostic.stage, Stage::Semantic);
            assert_eq!(diagnostic.message.as_str(), vif::SAME_KEY_MESSAGE);
            assert_eq!(diagnostic.span, span_of(source, r#"key="dup""#, occurrence));
        }
        let errors: Vec<&str> = lowered
            .provenance
            .iter()
            .filter(|record| record.rule.as_str() == "error.v-if-same-key")
            .map(|record| record.before.as_str())
            .collect();
        assert_eq!(errors, vec!["key=\"dup\"", "key=\"dup\""]);
    });
    assert_transformed_sound(source, "collision");
}

#[test]
fn a_for_wrapped_branch_keeps_its_key_with_the_iteration() {
    // Vue 3 precedence: v-if wraps v-for, so the branch region's root is
    // the `ui.for` and the key belongs to the iteration - exactly the
    // legacy transform's has-v-for skip.
    let source = r#"<li v-if="a" v-for="i in xs" key="k">{{ i }}</li>"#;
    with_transformed(source, |_, folio, facts, _| {
        assert!(facts.if_facts.is_empty(), "no branch fact may exist");
        // The iterated element keeps exactly its key (exact oracle).
        assert_eq!(
            branch_root(folio, 0, 0).attributes,
            vec![FolioAttribute {
                name: "key".into(),
                value: Some("k".into()),
                span: span_of(source, r#"key="k""#, 0),
            }]
        );
    });
}

#[test]
fn an_unwrapped_single_child_keeps_its_own_key() {
    // The lowering unwraps the template wrapper, leaving the inner div
    // as the region's only root - but that div is not the branch
    // carrier (its span is strictly inside the branch's), and its key
    // belongs to its own vnode, exactly as the legacy transform leaves
    // it. The span-identity guard is what this pins.
    let source = r#"<template v-if="a"><div key="z">x</div></template><p v-else>y</p>"#;
    with_transformed(source, |lowered, folio, facts, _| {
        assert!(facts.if_facts.is_empty(), "no branch fact may exist");
        // The inner element keeps exactly its own key (exact oracle).
        assert_eq!(
            branch_root(folio, 0, 0).attributes,
            vec![FolioAttribute {
                name: "key".into(),
                value: Some("z".into()),
                span: span_of(source, r#"key="z""#, 0),
            }]
        );
        assert_eq!(lowered.diagnostics, vec![]);
    });
    assert_transformed_sound(source, "unwrapped-child-key");
}

#[test]
fn the_pipeline_reports_one_walk_per_barrier_pass() {
    // Four mandatory barriers plus the series-6 fusable singleton
    // (v-if, v-for, v-slot, text, hoist-static): five walks for an
    // artifact with no ui.model bindings.
    with_transformed(r#"<div v-if="a">x</div>"#, |_, _, _, budget| {
        assert_eq!(
            vize_davinci::folio::Folio::print_to_string(
                budget,
                vize_davinci::folio::FolioMode::Full
            )
            .as_str(),
            "[budget-observer]\nwalks=5\npasses=5\nanalyses=0\npipelines=1\nfailures=0\n\n"
        );
    });
}

#[test]
fn the_vif_pass_is_the_canonicalizing_pass() {
    // Rigor escalates to Canonical exactly when the v-if pass completes:
    // the canonical `ui.if` invariant set (S2V004-S2V006) is this pass's
    // to establish, which is the MandatoryLowering half of the review
    // point's classification.
    let mut verify = VerifyObserver::new();
    assert_eq!(verify.rigor(), Rigor::Raw);
    let event = PassEvent {
        pipeline: &TRANSFORM,
        group_index: 0,
        group: TRANSFORM.group(0).expect("the plan has one group"),
        pass_index: 0,
    };
    verify.note(&event);
    assert_eq!(verify.rigor(), Rigor::Canonical);
}

#[test]
fn the_pass_is_total_over_malformed_chains() {
    // Orphan branches, missing expressions, unclosed elements: the
    // lowering already diagnosed them; the pass must stay sound.
    for (name, source) in [
        ("orphan-else", r#"<p v-else key="k">x</p>"#),
        ("missing-expression", r#"<p v-if key="k">x</p>"#),
        ("unclosed-branch", r#"<div v-if="a" key="k"><span>x</div>"#),
        ("empty", ""),
    ] {
        assert_transformed_sound(source, name);
    }
}
