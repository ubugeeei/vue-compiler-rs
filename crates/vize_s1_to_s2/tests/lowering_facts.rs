//! Exact-pinned side-table facts: hygiene scopes on binding-introducing
//! ops (with the #4365 pattern boundary) and provenance pairs that
//! survive failure, keyed by page-order ids that resolve.

mod support;

use support::{artifact, with_lowered};
use vize_davinci::id::NodeId;
use vize_s0::{Span, String};
use vize_s2::provenance::ProvenanceRecord;
use vize_s2::scope::{ScopeBinding, ScopeFacts, ScopeOrigin, ScopeTag};

fn authored(name: &str, start: u32, end: u32) -> ScopeBinding {
    ScopeBinding {
        name: String::from(name),
        origin: ScopeOrigin::Authored {
            span: Span::new(start, end),
        },
    }
}

#[test]
fn a_v_for_scope_records_its_authored_bindings_in_position_order() {
    let art = artifact("<li v-for=\"(item, i) in items\" :key=\"item.id\">{{ item.name }}</li>");
    assert_eq!(
        art.scopes,
        vec![(
            0,
            ScopeFacts {
                tag: ScopeTag::from_index(0),
                bindings: vec![authored("item", 12, 16), authored("i", 18, 19)],
            }
        )]
    );
}

#[test]
fn aliases_after_the_three_position_contract_do_not_change_lowering() {
    // The splitter retains the complete authored spelling (and spills its
    // inline buffer), while lowering intentionally consumes only Vue's
    // value/key/index contract. Pin that established behavior so the
    // storage optimization cannot turn into silent positional drift.
    let art = artifact("<li v-for=\"(value, key, index, extra) in items\">x</li>");
    assert_eq!(
        art.scopes,
        vec![(
            0,
            ScopeFacts {
                tag: ScopeTag::from_index(0),
                bindings: vec![
                    authored("value", 12, 17),
                    authored("key", 19, 22),
                    authored("index", 24, 29),
                ],
            }
        )]
    );
    assert_eq!(art.diagnostics, Vec::new());
}

#[test]
fn a_destructuring_alias_contributes_no_names_but_the_scope_stands() {
    // Names inside patterns wait for the one identifier-enumeration seam
    // (`ExprDialect::enumerate_bindings`, #4365); the simple-identifier
    // key still records, and the introduction site keeps its tag.
    let art = artifact("<a v-for=\"({ a, b }, i) in xs\">y</a>");
    assert_eq!(
        art.scopes,
        vec![(
            0,
            ScopeFacts {
                tag: ScopeTag::from_index(0),
                bindings: vec![authored("i", 21, 22)],
            }
        )]
    );
}

#[test]
fn slot_props_are_a_tagged_introduction_site_on_their_own_binding_op() {
    // Since the v-slot installment the spelling lowers to a
    // `ui.slot-content` binding op, and the slot-props scope keys that
    // op (P2-8 recorded it on the carrying element while no op
    // existed): each spelling is its own introduction site, so two
    // `v-slot`s on one carrier never share an entry.
    let art = artifact("<Comp><template #head=\"props\"><b>x</b></template></Comp>");
    // Comp=0, template=1, its slot-content binding=2, b=3, text=4.
    assert_eq!(
        art.scopes,
        vec![(
            2,
            ScopeFacts {
                tag: ScopeTag::from_index(0),
                bindings: vec![authored("props", 23, 28)],
            }
        )]
    );
}

#[test]
fn provenance_pairs_are_recorded_per_decision_in_page_order() {
    let art = artifact("<p v-if=\"ok\" v-for=\"i in is\">t</p>");
    assert_eq!(
        art.provenance,
        vec![
            ProvenanceRecord {
                rule: String::from("lower.if"),
                node: NodeId::from_index(0),
                before: String::from("v-if=\"ok\""),
                after: String::from("ui.if branches=1"),
                span: Span::new(0, 34),
            },
            ProvenanceRecord {
                rule: String::from("lower.for"),
                node: NodeId::from_index(1),
                before: String::from("i in is"),
                after: String::from("ui.for source=js value=js"),
                span: Span::new(20, 27),
            },
            ProvenanceRecord {
                rule: String::from("lower.for-fact"),
                node: NodeId::from_index(1),
                before: String::from("scope #0 bindings=1"),
                after: String::from("fact value=i key=- index=-"),
                span: Span::new(20, 27),
            },
            ProvenanceRecord {
                rule: String::from("lower.element"),
                node: NodeId::from_index(2),
                before: String::from("<p v-if=\"ok\" v-for=\"i in is\">"),
                after: String::from("ui.element p"),
                span: Span::new(0, 34),
            },
            ProvenanceRecord {
                rule: String::from("lower.text"),
                node: NodeId::from_index(3),
                before: String::from("t"),
                after: String::from("ui.text"),
                span: Span::new(29, 30),
            },
        ]
    );
}

#[test]
fn provenance_survives_failure_with_the_fragment_kept() {
    // The malformed v-for: the record names the rule, keeps the op link,
    // and the after-form states the escape classification — while the op
    // tree still holds the `ui.for` and its region (no rollback).
    let art = artifact("<i v-for=\"items\">t</i>");
    assert_eq!(art.op_count, 3);
    assert_eq!(
        art.provenance[0],
        ProvenanceRecord {
            rule: String::from("error.v-for-malformed"),
            node: NodeId::from_index(0),
            before: String::from("items"),
            after: String::from("ui.for source=opaque(for-value) value=opaque(for-value)"),
            span: Span::new(10, 15),
        }
    );

    // The orphan v-else: the decision produced no op (`node: None`,
    // empty `after`), and the element fragment is still lowered.
    let orphan = artifact("<div v-else>x</div>");
    assert_eq!(orphan.op_count, 2);
    assert_eq!(
        orphan.provenance[0],
        ProvenanceRecord {
            rule: String::from("error.v-else-orphan"),
            node: None,
            before: String::from("v-else"),
            after: String::default(),
            span: Span::new(5, 11),
        }
    );
}

#[test]
fn dropped_content_is_never_silent() {
    let art = artifact("<!-- note --><div>x</div>");
    assert_eq!(
        art.provenance[0],
        ProvenanceRecord {
            rule: String::from("drop.comment"),
            node: None,
            before: String::from("<!-- note -->"),
            after: String::default(),
            span: Span::new(0, 13),
        }
    );

    // An unwrapped <template> wrapper has no op to carry its remaining
    // attributes; each drop is a record plus an Info diagnostic.
    let wrapper = artifact("<template v-if=\"x\" class=\"a\"><b>y</b></template>");
    assert_eq!(
        wrapper.provenance[1],
        ProvenanceRecord {
            rule: String::from("drop.template-attribute"),
            node: None,
            before: String::from("class=\"a\""),
            after: String::default(),
            span: Span::new(19, 28),
        }
    );
    assert_eq!(wrapper.diagnostics.len(), 1);
    assert_eq!(
        wrapper.diagnostics[0].message.as_str(),
        "`class` on an unwrapped `<template>` wrapper has no S2 home; it is dropped under provenance"
    );
}

#[test]
fn page_order_ids_key_every_table_and_match_the_folio_count() {
    with_lowered(
        "<div><span v-for=\"i in is\">{{i}}</span></div>",
        |lowered, folio| {
            assert_eq!(lowered.op_count, 4);
            assert_eq!(folio.op_count(), 4);
            // div=0, for=1 (page order: the wrap precedes the element),
            // span=2, interpolation=3; the scope keys the ui.for.
            let keys: Vec<u32> = lowered
                .scopes
                .sorted_entries()
                .into_iter()
                .map(|(id, _)| id.index())
                .collect();
            assert_eq!(keys, vec![1]);
        },
    );
}
