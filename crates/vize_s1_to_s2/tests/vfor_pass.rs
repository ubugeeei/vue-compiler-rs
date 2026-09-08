//! Lowering-published `v-for` facts (P2-9 series 2): semantics pins.
//!
//! Exact-equality oracles over the fact mirror's products — the
//! consumed-scope facts, the provenance trail, and the things it
//! deliberately does *not* do (no mutation, no user diagnostic, no
//! synthesized name).
//! The TS-17 folio snapshots live in `vfor_pass_snapshot.rs`.

mod support;

use vize_davinci::id::NodeId;
use vize_s1_to_s2::pass::vfor::{ForFacts, ForName};
use vize_s2::folio::{FolioAttribute, FolioOp};
use vize_s2::scope::ScopeOrigin;

use support::{assert_transformed_sound, with_lowered, with_transformed};

/// Every `ui.for` in `ops`, counted recursively.
fn count_fors(ops: &[FolioOp]) -> usize {
    ops.iter()
        .map(|op| match op {
            FolioOp::Element(element) => count_fors(&element.children),
            FolioOp::Component(component) => count_fors(&component.children),
            FolioOp::Text(_) | FolioOp::Interpolation(_) | FolioOp::Comment(_) => 0,
            FolioOp::If(if_op) => if_op
                .branches
                .iter()
                .map(|branch| count_fors(&branch.ops))
                .sum(),
            FolioOp::For(for_op) => 1 + count_fors(&for_op.ops),
            FolioOp::Slot(slot) => count_fors(&slot.fallback),
        })
        .sum()
}

#[test]
fn consumption_publishes_the_scope_view_per_position() {
    let source = r#"<li v-for="(item, k, idx) in items">{{ item }}</li>"#;
    with_transformed(source, |lowered, _, facts, _| {
        assert_eq!(lowered.diagnostics, vec![]);
        let id = NodeId::from_index(0).expect("op 0 has an id");
        let recorded = lowered.scopes.get(id).expect("the lowering scoped it");
        assert_eq!(
            facts.for_facts.sorted_entries(),
            vec![(
                id,
                &ForFacts {
                    tag: recorded.tag,
                    value: ForName::Named("item".into()),
                    key: ForName::Named("k".into()),
                    index: ForName::Named("idx".into()),
                }
            )]
        );
        // The lowering left exactly one v-for fact record, spelled
        // exactly; the series-6 analysis pass adds its per-owner fact
        // record behind it (the iterated element: dynamic text, no
        // hoistable props).
        let records: Vec<(&str, &str, &str)> = lowered
            .provenance
            .iter()
            .filter(|record| {
                record.rule == "lower.for-fact" || record.rule == "pass.hoist-static.fact"
            })
            .map(|record| {
                (
                    record.rule.as_str(),
                    record.before.as_str(),
                    record.after.as_str(),
                )
            })
            .collect();
        assert_eq!(
            records,
            vec![
                (
                    "lower.for-fact",
                    "scope #0 bindings=3",
                    "fact value=item key=k index=idx",
                ),
                (
                    "pass.hoist-static.fact",
                    "ui.element",
                    "level=dynamic-text props=false nested=true native=true",
                ),
            ]
        );
    });
    assert_transformed_sound(source, "three-positions");
}

#[test]
fn a_destructuring_alias_is_pending_and_named_positions_still_enumerate() {
    // The #4365 boundary, consumed as recorded: the pattern's names are
    // not enumerated anywhere, so the position is pending (pessimize),
    // while the simple-identifier index is the one validated name.
    let source = r#"<tr v-for="({ id, name }, i) in rows">{{ id }}</tr>"#;
    with_transformed(source, |lowered, _, facts, _| {
        let entries = facts.for_facts.sorted_entries();
        assert_eq!(entries.len(), 1);
        let fact = entries[0].1;
        assert_eq!(fact.value, ForName::Pending);
        assert_eq!(fact.key, ForName::Named("i".into()));
        assert_eq!(fact.index, ForName::Absent);
        let recorded = lowered.scopes.get(entries[0].0).expect("scoped");
        assert_eq!(recorded.bindings.len(), 1, "only `i` is enumerated");
    });
    assert_transformed_sound(source, "destructure");
}

#[test]
fn an_absent_alias_consumes_as_absent_never_synthesized() {
    // `v-for=" in xs"` is valid Vue with no value binding. The pass
    // records the absence; it never invents a placeholder name — that
    // would be realization (P2-11's), and the first
    // `ScopeOrigin::Synthesized` producer stays slot normalization
    // (the P2-8 record's assignment).
    let source = r#"<div v-for=" in xs">x</div>"#;
    with_transformed(source, |lowered, _, facts, _| {
        assert_eq!(lowered.diagnostics, vec![]);
        let entries = facts.for_facts.sorted_entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].1.value, ForName::Absent);
        assert_eq!(entries[0].1.key, ForName::Absent);
        assert_eq!(entries[0].1.index, ForName::Absent);
        for (_, scope) in lowered.scopes.sorted_entries() {
            for binding in &scope.bindings {
                assert!(
                    matches!(binding.origin, ScopeOrigin::Authored { .. }),
                    "no synthesized name may exist: {binding:?}"
                );
            }
        }
    });
    assert_transformed_sound(source, "absent-alias");
}

#[test]
fn an_undecomposable_value_is_pessimally_pending() {
    // `v-for="items"` cannot split under Vue's grammar: the lowering
    // kept the op with the classified escape and an error. The value
    // position is *pending*, not absent — under the opaque laws the
    // alias is unknowable, unlike the valid absent alias above.
    let source = r#"<p v-for="items">kept</p>"#;
    with_transformed(source, |lowered, folio, facts, _| {
        assert_eq!(lowered.diagnostics.len(), 1, "the lowering's error");
        assert_eq!(
            lowered.diagnostics[0].message.as_str(),
            "v-for has invalid expression."
        );
        assert_eq!(count_fors(&folio.ops), 1, "op and region kept");
        let entries = facts.for_facts.sorted_entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].1.value, ForName::Pending);
        assert_eq!(entries[0].1.key, ForName::Absent);
        assert_eq!(entries[0].1.index, ForName::Absent);
    });
    assert_transformed_sound(source, "undecomposable");
}

#[test]
fn the_iterated_elements_key_stays_on_its_attribute_surface() {
    // Unlike a v-if branch key, the legacy lane never lifts a v-for
    // element's `key` — codegen reads it per vnode (P2-11's business).
    // The pass must leave the surface untouched.
    let source = r#"<li v-for="item in items" key="row">{{ item }}</li>"#;
    with_transformed(source, |_, folio, facts, _| {
        let FolioOp::For(for_op) = &folio.ops[0] else {
            panic!("root op is not ui.for");
        };
        let [FolioOp::Element(element)] = &for_op.ops[..] else {
            panic!("for region root is not one element");
        };
        assert_eq!(element.attributes.len(), 1);
        assert_eq!(
            element.attributes[0],
            FolioAttribute {
                name: "key".into(),
                value: Some("row".into()),
                span: element.attributes[0].span,
            }
        );
        assert!(facts.if_facts.is_empty(), "no branch fact may exist");
        assert_eq!(facts.for_facts.len(), 1);
    });
    assert_transformed_sound(source, "iterated-key");
}

#[test]
fn every_ui_for_is_consumed_with_a_fresh_tag() {
    // Dense consumption (every `ui.for` is an introduction site), tags
    // pairwise distinct — the hygiene freshness law as data.
    let source = r#"<ul v-for="row in grid"><li v-for="cell in row">{{ cell }}</li></ul><template v-for="n of five"><b>{{ n }}</b></template>"#;
    with_transformed(source, |_, folio, facts, _| {
        assert_eq!(count_fors(&folio.ops), 3);
        let entries = facts.for_facts.sorted_entries();
        assert_eq!(entries.len(), 3);
        let mut tags: Vec<u32> = entries.iter().map(|(_, fact)| fact.tag.index()).collect();
        tags.sort_unstable();
        tags.dedup();
        assert_eq!(tags.len(), 3, "every site minted a fresh tag");
    });
    assert_transformed_sound(source, "dense-consumption");
}

#[test]
fn the_fact_mirror_neither_mutates_nor_diagnoses() {
    // The lowering-published fact mirror is preserving: the transformed
    // folio and diagnostics are byte-for-byte the lowering's own.
    let source = r#"<section><div v-for="(a, b) in xs" key="k" :title="a">{{ b }}</div></section>"#;
    let (before_folio, before_diagnostics) = with_lowered(source, |lowered, folio| {
        (folio.clone(), lowered.diagnostics.clone())
    });
    with_transformed(source, |lowered, folio, _, _| {
        assert_eq!(folio, &before_folio, "the v-for mirror must not mutate");
        assert_eq!(
            lowered.diagnostics, before_diagnostics,
            "the v-for mirror emits no user diagnostic"
        );
    });
}

#[test]
fn a_for_wrapped_branch_is_consumed_behind_the_vif_pass() {
    // The precedence interplay from the vif side, read from this side:
    // v-if wraps v-for, the vif pass leaves the iteration's key alone,
    // and the lowering-published facts consume the wrapped `ui.for`
    // like any other.
    let source = r#"<li v-if="ok" v-for="i in xs" key="k">{{ i }}</li>"#;
    with_transformed(source, |_, _, facts, _| {
        assert!(facts.if_facts.is_empty());
        let entries = facts.for_facts.sorted_entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].1.value, ForName::Named("i".into()));
    });
    assert_transformed_sound(source, "if-wraps-for");
}

#[test]
fn the_pass_is_total_over_malformed_loops() {
    // The lowering already diagnosed these; the fact mirror must stay sound.
    for (name, source) in [
        ("no-expression", r#"<li v-for>x</li>"#),
        ("empty-expression", r#"<li v-for="">x</li>"#),
        ("undecomposable", r#"<li v-for="items">x</li>"#),
        ("lone-paren", r#"<li v-for="(item in items">x</li>"#),
        ("unclosed", r#"<ul v-for="x in xs"><li>{{ x }}</ul>"#),
        ("orphan-else-for", r#"<p v-else v-for="i in xs">x</p>"#),
        ("empty", ""),
    ] {
        assert_transformed_sound(source, name);
    }
}
