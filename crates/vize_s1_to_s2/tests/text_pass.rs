//! Lowering-published text facts (P2-9 series 4, fused in P2-12b):
//! semantics pins.
//!
//! Exact-equality oracles over the installment's three products — the
//! condensed surface, the compound ops with their recorded parts, and
//! the published [`TextFacts`] view — plus the comment-boundary parity
//! cases the port's lowering-absorption argument rests on. The TS-17
//! folio snapshots live in `text_pass_snapshot.rs`.

mod support;

use vize_davinci::id::NodeId;
use vize_s0::{Span, String};
use vize_s1_to_s2::lower::{TextPart, rebuild_source};
use vize_s1_to_s2::pass::TextFacts;
use vize_s2::folio::{FolioExpr, FolioOp};

use support::{assert_transformed_sound, with_transformed};

fn id(index: u32) -> NodeId {
    NodeId::from_index(index).expect("test ids fit")
}

/// The span of the one `needle` occurrence in `source`.
fn span_of(source: &str, needle: &str) -> Span {
    let start = source.find(needle).expect("needle exists");
    Span::new(
        u32::try_from(start).expect("fixture fits u32"),
        u32::try_from(start + needle.len()).expect("fixture fits u32"),
    )
}

#[test]
fn a_mixed_run_merges_into_one_compound_with_recorded_parts() {
    let source = "<p>Hi {{ name }}! You have {{ n }} new  mails.</p>";
    with_transformed(source, |lowered, folio, facts, _| {
        // The tree: one element, one compound interpolation child.
        let FolioOp::Element(p) = &folio.ops[0] else {
            panic!("root is not the element: {:?}", folio.ops);
        };
        assert_eq!(p.children.len(), 1, "the run merged into one op");
        let FolioOp::Interpolation(compound) = &p.children[0] else {
            panic!("the merged op is not an interpolation: {:?}", p.children);
        };
        // The opaque payload: reason compound, the canonical rebuild
        // (internal whitespace condensed, delimiters normalized).
        assert_eq!(
            compound.expression,
            FolioExpr::Opaque {
                reason: vize_s2::expr::OpaqueReason::Compound,
                source: "Hi {{ name }}! You have {{ n }} new mails.".into(),
                span: Span::new(3, 46),
            }
        );
        // The recorded parts, exact: texts condensed, spans authored.
        let expected = vec![
            TextPart {
                text: String::from("Hi "),
                span: span_of(source, "Hi "),
                dynamic: false,
            },
            TextPart {
                text: String::from("name"),
                span: span_of(source, "{{ name }}"),
                dynamic: true,
            },
            TextPart {
                text: String::from("! You have "),
                span: span_of(source, "! You have "),
                dynamic: false,
            },
            TextPart {
                text: String::from("n"),
                span: span_of(source, "{{ n }}"),
                dynamic: true,
            },
            TextPart {
                text: String::from(" new mails."),
                span: span_of(source, " new  mails."),
                dynamic: false,
            },
        ];
        assert_eq!(
            facts.text_facts.sorted_entries(),
            vec![(
                id(1),
                &TextFacts {
                    parts: expected.clone()
                }
            )]
        );
        // The consumed view is the recorded view, validated.
        assert_eq!(
            lowered.texts.sorted_entries(),
            vec![(
                id(1),
                &vize_s1_to_s2::lower::TextParts {
                    parts: expected.clone()
                }
            )]
        );
        // The one rebuild rule, exercised from test space too.
        assert_eq!(
            rebuild_source(&expected).as_str(),
            "Hi {{ name }}! You have {{ n }} new mails."
        );
        // The lowering-published fact left its record.
        let rules: Vec<(&str, &str, &str)> = lowered
            .provenance
            .iter()
            .filter(|record| record.rule.as_str() == "lower.text-fact")
            .map(|record| {
                (
                    record.rule.as_str(),
                    record.before.as_str(),
                    record.after.as_str(),
                )
            })
            .collect();
        assert_eq!(
            rules,
            vec![("lower.text-fact", "parts=5", "fact static=3 dynamic=2")]
        );
    });
    assert_transformed_sound(source, "mixed-run");
}

#[test]
fn lone_nodes_never_compound() {
    // A single interpolation and a single text stay themselves — the
    // legacy run grouping's own rule, and the pass's ≥2-parts law.
    let source = "<p>{{ a }}</p><q>b</q>";
    with_transformed(source, |lowered, folio, facts, _| {
        assert!(facts.text_facts.is_empty());
        assert!(lowered.texts.is_empty());
        let FolioOp::Element(p) = &folio.ops[0] else {
            panic!("no p element");
        };
        assert!(
            matches!(&p.children[..], [FolioOp::Interpolation(node)]
                if matches!(&node.expression, FolioExpr::Js { source, .. } if source == "a")),
            "a lone interpolation keeps its retained expression: {:?}",
            p.children
        );
    });
    assert_transformed_sound(source, "lone-nodes");
}

#[test]
fn a_dropped_comment_is_not_a_run_boundary() {
    // `a<!--c-->b {{ x }}` under the default (comments-dropped) lowering
    // is **one** unit. Vue's parser builds no node for a comment it is
    // not preserving, and its `onText` appends to the previous text
    // child without a contiguity check, so the run arrives at condensing
    // as `ab {{ x }}` — measured against `@vue/compiler-dom` 3.5.41 and
    // 3.6.0-beta.10, which both emit `"ab " + _toDisplayString(x)`.
    // `vize_atelier_dom/tests/dropped_comment_text_runs.rs` pins the
    // compiled form; this pins the op the lowering mints.
    let source = "<p>a<!--c-->b {{ x }}</p>";
    with_transformed(source, |_, folio, facts, _| {
        let FolioOp::Element(p) = &folio.ops[0] else {
            panic!("no p element");
        };
        assert_eq!(p.children.len(), 1, "one unit: {:?}", p.children);
        assert!(matches!(
            &p.children[0],
            FolioOp::Interpolation(node)
                if matches!(&node.expression, FolioExpr::Opaque { source, .. }
                    if source == "ab {{ x }}")
        ));
        assert_eq!(facts.text_facts.len(), 1);
    });
    assert_transformed_sound(source, "comment-boundary");
}

#[test]
fn comment_free_neighbours_drive_the_remove_rule() {
    // The dropped comments are not neighbours either: with them gone the
    // newline run has text on both sides, so it condenses to one space
    // rather than being removed. Vue agrees — `"a b"` — and so does this
    // crate's legacy parse/transform lane under the shipped
    // `comments: false`.
    let source = "<p>a<!--c-->\n<!--d-->b</p>";
    with_transformed(source, |_, folio, _, _| {
        let FolioOp::Element(p) = &folio.ops[0] else {
            panic!("no p element");
        };
        assert_eq!(p.children.len(), 1, "one unit: {:?}", p.children);
        assert!(matches!(&p.children[0], FolioOp::Text(text) if text.content == "a b"));
    });
    assert_transformed_sound(source, "comment-remove");
}

#[test]
fn whitespace_condenses_by_the_armature_rules() {
    // No newline between elements: the run condenses to one space; a
    // newline-bearing run between elements is removed; interior runs in
    // mixed text collapse.
    let source = "<i>one</i> <i>two</i>\n<i>three</i><b>x   y</b>";
    with_transformed(source, |_, folio, _, _| {
        let kinds: Vec<String> = folio
            .ops
            .iter()
            .map(|op| match op {
                FolioOp::Element(element) => vize_s0::cstr!("el:{}", element.tag),
                FolioOp::Text(text) => vize_s0::cstr!("text:{:?}", text.content.as_str()),
                other => vize_s0::cstr!("{other:?}"),
            })
            .collect();
        assert_eq!(
            kinds,
            vec![
                String::from("el:i"),
                String::from("text:\" \""),
                String::from("el:i"),
                String::from("el:i"),
                String::from("el:b"),
            ]
        );
        let FolioOp::Element(b) = &folio.ops[4] else {
            panic!("no b element");
        };
        assert!(matches!(&b.children[..], [FolioOp::Text(text)] if text.content == "x y"));
    });
    assert_transformed_sound(source, "condense-rules");
}

#[test]
fn pre_subtrees_keep_their_bytes_and_rawtext_condenses() {
    // `<pre>` (the shipped `is_pre_tag`) is exempt from condensing;
    // rawtext content still follows the shipped DOM lane's condense
    // strategy. Merging still applies inside `<pre>` (the legacy codegen
    // grouping never checked pre), with the parts uncondensed.
    let source = "<pre>  a   {{ x }}  b </pre><textarea> c   d </textarea>";
    with_transformed(source, |_, folio, facts, _| {
        let FolioOp::Element(pre) = &folio.ops[0] else {
            panic!("no pre element");
        };
        assert!(matches!(
            &pre.children[..],
            [FolioOp::Interpolation(node)]
                if matches!(&node.expression, FolioExpr::Opaque { source, .. }
                    if source == "  a   {{ x }}  b ")
        ));
        assert_eq!(facts.text_facts.len(), 1);
        let FolioOp::Element(textarea) = &folio.ops[1] else {
            panic!("no textarea element: {:?}", folio.ops);
        };
        assert!(matches!(&textarea.children[..], [FolioOp::Text(text)] if text.content == " c d "));
    });
    assert_transformed_sound(source, "pre-rawtext");
}

#[test]
fn rawtext_whitespace_only_subtrees_drop_like_the_shipped_dom_lane() {
    let source = "<textarea>\n</textarea><iframe>\n</iframe><noscript>\n</noscript><pre>\n</pre>";
    with_transformed(source, |_, folio, _, _| {
        for (index, tag) in ["textarea", "iframe", "noscript"].into_iter().enumerate() {
            let FolioOp::Element(element) = &folio.ops[index] else {
                panic!("no {tag} element: {:?}", folio.ops);
            };
            assert_eq!(element.tag, tag);
            assert!(
                element.children.is_empty(),
                "{tag} whitespace-only rawtext should lower as empty children: {:?}",
                element.children
            );
        }
        let FolioOp::Element(pre) = &folio.ops[3] else {
            panic!("no pre element: {:?}", folio.ops);
        };
        assert_eq!(pre.tag, "pre");
        assert!(matches!(&pre.children[..], [FolioOp::Text(text)] if text.content == "\n"));
    });
    assert_transformed_sound(source, "rawtext-empty-whitespace");
}

#[test]
fn entities_stay_undecoded_the_s1_scope() {
    // The S1 v1 no-decoding deviation, re-recorded for text parts: the
    // legacy lane decodes `&amp;` at parse; S2 carries the authored
    // bytes, and the differential lane counts the class instead of
    // comparing it.
    let source = "<p>a &amp; b {{ x }}</p>";
    with_transformed(source, |lowered, _, _, _| {
        let entry = lowered.texts.sorted_entries();
        assert_eq!(entry.len(), 1);
        assert_eq!(entry[0].1.parts[0].text.as_str(), "a &amp; b ");
    });
    assert_transformed_sound(source, "entities");
}

#[test]
fn nbsp_only_implicit_component_children_are_slot_fillers() {
    let source = "<Text>&nbsp;</Text><Text>&#160;</Text>";
    with_transformed(source, |_, _, facts, _| {
        assert!(
            facts.slot_facts.is_empty(),
            "NBSP-only implicit component children should not synthesize default slot facts"
        );
    });
    assert_transformed_sound(source, "nbsp-slot-fillers");
}

#[test]
fn the_pass_is_total_over_malformed_text_shapes() {
    for (name, source) in [
        ("unclosed-with-run", "<div>a {{ b }}<span>c {{ d }}"),
        ("stray-end-tag", "<p>a</q> {{ b }}</p>"),
        ("lone-brace-text", "<p>{{ a </p>"),
        ("cdata-neighbour", "<p>a<![CDATA[b]]>c {{ d }}</p>"),
        ("empty", ""),
        ("whitespace-only", "  \n\t  "),
    ] {
        assert_transformed_sound(source, name);
    }
}
