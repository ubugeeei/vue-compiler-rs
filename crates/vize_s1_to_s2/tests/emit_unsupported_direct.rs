//! Direct S2 refusal fixtures for malformed or fact-incomplete handoffs.

#![allow(
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    clippy::disallowed_types
)]

use std::vec::Vec as StdVec;

use vize_davinci::id::NodeId;
use vize_davinci::side_table::SideTable;
use vize_s0::{Allocator, Box as ArenaBox, Span, String as VString, Vec as ArenaVec};
use vize_s1_to_s2::lower::{ForWrapper, LoweringFeatures, WrapperKey};
use vize_s1_to_s2::pass::{
    S2Facts, SlotCarrier, SlotFacts, SlotGroup, SlotName, SlotParams, TextFacts,
};
use vize_s1_to_s2::{EmitError, LegacyCaps, Lowered, UnsupportedReason as Reason, emit_dom};
use vize_s2::expr::{ExprRef, ForeignExpr, OpaqueExpr, OpaqueReason};
use vize_s2::op::{
    Attribute, BindingContract, BindingOp, ComponentOp, ElementOp, ForBinding, ForOp, IfBranch,
    IfOp, InterpolationOp, ModelOp, Namespace, Op, Region, TextOp,
};
use vize_s2::scope::ScopeOrigin;

type Factory = for<'a> fn(&'a Allocator) -> (Lowered<'a>, S2Facts);

const DIRECT: &[(Reason, Factory)] = &[
    (Reason::EmptyCompoundText, empty_compound_text),
    (Reason::ForAliasNotEmittable, for_alias_not_emittable),
    (Reason::ForItemShape, for_item_shape),
    (Reason::IfBranchShape, if_branch_shape),
    (Reason::IfWithoutBranches, if_without_branches),
    (Reason::MissingTextFacts, missing_text_facts),
    (Reason::ModelExpressionNotJs, model_expression_not_js),
    (Reason::SlotFactsMissingGroup, slot_facts_missing_group),
    (Reason::TemplateDynamicKeyEmpty, template_dynamic_key_empty),
];

#[test]
fn direct_s2_refusal_fixtures_are_classified() {
    for (expected, factory) in DIRECT {
        let allocator = Allocator::new();
        let (lowered, facts) = factory(&allocator);
        let error = emit_dom(&lowered, &facts).expect_err(expected.code());
        assert_eq!(reason(error), *expected, "{}", expected.code());
    }
}

fn reason(error: EmitError) -> Reason {
    match error {
        EmitError::Unsupported(refusal) => refusal.reason,
        EmitError::Diagnostics => panic!("direct fixture produced diagnostics"),
    }
}

fn empty_compound_text<'a>(a: &'a Allocator) -> (Lowered<'a>, S2Facts) {
    let mut facts = S2Facts::default();
    facts.text_facts.insert(
        id(1),
        TextFacts {
            parts: StdVec::new(),
        },
    );
    (lowered(a, root_with_compound_child(a)), facts)
}

fn missing_text_facts<'a>(a: &'a Allocator) -> (Lowered<'a>, S2Facts) {
    (lowered(a, root_with_compound_child(a)), S2Facts::default())
}

fn if_without_branches<'a>(a: &'a Allocator) -> (Lowered<'a>, S2Facts) {
    let if_op = IfOp {
        branches: av(a),
        span: sp(),
    };
    (
        lowered(a, region(a, [Op::If(ab(a, if_op))])),
        S2Facts::default(),
    )
}

fn if_branch_shape<'a>(a: &'a Allocator) -> (Lowered<'a>, S2Facts) {
    let branch = IfBranch {
        condition: Some(js(a, "ok")),
        region: empty_region(a),
        span: sp(),
    };
    let if_op = IfOp {
        branches: av1(a, branch),
        span: sp(),
    };
    (
        lowered(a, region(a, [Op::If(ab(a, if_op))])),
        S2Facts::default(),
    )
}

fn for_item_shape<'a>(a: &'a Allocator) -> (Lowered<'a>, S2Facts) {
    (
        lowered(a, region(a, [for_op(a, js(a, "item"), empty_region(a))])),
        S2Facts::default(),
    )
}

fn for_alias_not_emittable<'a>(a: &'a Allocator) -> (Lowered<'a>, S2Facts) {
    (
        lowered(
            a,
            region(a, [for_op(a, foreign(a, "pattern"), empty_region(a))]),
        ),
        S2Facts::default(),
    )
}

fn model_expression_not_js<'a>(a: &'a Allocator) -> (Lowered<'a>, S2Facts) {
    let expr = opaque(a, "bad", OpaqueReason::ParseRejected);
    let model = BindingOp::Model(ab(
        a,
        ModelOp {
            contract: BindingContract {
                read: expr,
                write: expr,
            },
            argument: None,
            attributes: av(a),
            span: sp(),
        },
    ));
    (
        lowered(
            a,
            region(a, [el(a, "input", av1(a, model), empty_region(a))]),
        ),
        S2Facts::default(),
    )
}

fn slot_facts_missing_group<'a>(a: &'a Allocator) -> (Lowered<'a>, S2Facts) {
    let mut facts = S2Facts::default();
    facts.slot_facts.insert(
        NodeId::FIRST,
        SlotFacts {
            groups: vec![SlotGroup {
                name: SlotName::Static {
                    text: VString::from("header"),
                    origin: ScopeOrigin::Synthesized {
                        rule: VString::from("test"),
                    },
                },
                params: SlotParams::Absent,
                carrier: SlotCarrier::Template(None),
            }],
        },
    );
    let child = text(a, "fallback");
    let root = comp(a, "Panel", av(a), region(a, [child]));
    (lowered(a, region(a, [root])), facts)
}

fn template_dynamic_key_empty<'a>(a: &'a Allocator) -> (Lowered<'a>, S2Facts) {
    let body = region(a, [el(a, "div", av(a), empty_region(a))]);
    let mut lowered = lowered(a, region(a, [for_op(a, js(a, "item"), body)]));
    lowered.for_wrappers.insert(
        NodeId::FIRST,
        ForWrapper {
            key: Some(WrapperKey::Dynamic {
                source: VString::default(),
                span: sp(),
            }),
            attributes: StdVec::new(),
            class: None,
        },
    );
    (lowered, S2Facts::default())
}

fn root_with_compound_child<'a>(a: &'a Allocator) -> Region<'a> {
    let child = compound(a);
    region(a, [el(a, "div", av(a), region(a, [child]))])
}

fn for_op<'a>(a: &'a Allocator, value: ExprRef<'a>, body: Region<'a>) -> Op<'a> {
    Op::For(ab(
        a,
        ForOp {
            binding: ForBinding {
                source: js(a, "items"),
                value,
                key: None,
                index: None,
            },
            region: body,
            span: sp(),
        },
    ))
}

fn lowered<'a>(a: &'a Allocator, root: Region<'a>) -> Lowered<'a> {
    Lowered {
        allocator: a,
        source: "",
        root,
        op_count: 0,
        diagnostics: StdVec::new(),
        provenance: StdVec::new(),
        scopes: SideTable::new(),
        texts: SideTable::new(),
        wrappers: SideTable::new(),
        for_wrappers: SideTable::new(),
        features: LoweringFeatures::EMPTY,
        caps: LegacyCaps::VUE3,
    }
}

fn comp<'a>(
    a: &'a Allocator,
    name: &'a str,
    bindings: ArenaVec<'a, BindingOp<'a>>,
    children: Region<'a>,
) -> Op<'a> {
    Op::Component(ab(
        a,
        ComponentOp {
            name,
            attributes: av(a),
            bindings,
            children,
            span: sp(),
        },
    ))
}

fn el<'a>(
    a: &'a Allocator,
    tag: &'a str,
    bindings: ArenaVec<'a, BindingOp<'a>>,
    children: Region<'a>,
) -> Op<'a> {
    Op::Element(ab(
        a,
        ElementOp {
            tag,
            namespace: Namespace::Html,
            attributes: ArenaVec::<Attribute<'a>>::new_in(&a),
            bindings,
            children,
            span: sp(),
        },
    ))
}

fn text<'a>(a: &'a Allocator, content: &'a str) -> Op<'a> {
    Op::Text(ab(
        a,
        TextOp {
            content,
            span: sp(),
        },
    ))
}

fn compound<'a>(a: &'a Allocator) -> Op<'a> {
    Op::Interpolation(ab(
        a,
        InterpolationOp {
            expression: opaque(a, "hello {{ name }}", OpaqueReason::Compound),
            span: sp(),
        },
    ))
}

fn js<'a>(a: &'a Allocator, source: &'a str) -> ExprRef<'a> {
    let expr = ExprRef::parse_js_in(a, source, Span::new(0, source.len() as u32));
    assert!(matches!(expr, ExprRef::Js(_)), "{source:?} must parse");
    expr
}

fn foreign<'a>(a: &'a Allocator, source: &'a str) -> ExprRef<'a> {
    ExprRef::Foreign(a.alloc(ForeignExpr {
        dialect: "test",
        source,
        span: sp(),
        facts: av(a),
    }))
}

fn opaque<'a>(a: &'a Allocator, source: &'a str, reason: OpaqueReason) -> ExprRef<'a> {
    ExprRef::Opaque(a.alloc(OpaqueExpr {
        reason,
        source,
        span: sp(),
    }))
}

fn region<'a, const N: usize>(a: &'a Allocator, ops: [Op<'a>; N]) -> Region<'a> {
    Region {
        ops: ArenaVec::from_iter_in(ops, &a),
    }
}

fn empty_region<'a>(a: &'a Allocator) -> Region<'a> {
    Region { ops: av(a) }
}

fn av<'a, T: 'a>(a: &'a Allocator) -> ArenaVec<'a, T> {
    ArenaVec::new_in(&a)
}

fn av1<'a, T: 'a>(a: &'a Allocator, value: T) -> ArenaVec<'a, T> {
    ArenaVec::from_iter_in([value], &a)
}

fn ab<'a, T: 'a>(a: &'a Allocator, value: T) -> ArenaBox<'a, T> {
    ArenaBox::new_in(value, &a)
}

fn id(index: u32) -> NodeId {
    NodeId::from_index(index).expect("test id is below the node limit")
}

fn sp() -> Span {
    Span::new(0, 1)
}
