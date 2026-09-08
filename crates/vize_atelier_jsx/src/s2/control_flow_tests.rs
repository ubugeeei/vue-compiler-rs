use vize_davinci::id::NodeId;
use vize_s0::Allocator;
use vize_s2::op::Op;
use vize_s2::scope::ScopeOrigin;

use crate::{JsxLang, lower_source};

#[test]
fn logical_and_child_projects_to_s2_if() {
    let allocator = Allocator::new();
    let source = "const App = () => <ul>{ok && <li>{item}</li>}</ul>;";
    let lowered = lower_source(&allocator, allocator.as_oxc(), source, JsxLang::Jsx);
    let root = lowered.roots.first().expect("one JSX root");

    let s2 = root.s2.as_ref().expect("logical child projects to S2");
    assert_eq!(s2.op_count, 4);
    assert!(s2.features.has_if_ops());
    assert!(!s2.features.has_for_ops());
    let Op::Element(root_element) = &s2.root.ops[0] else {
        panic!("root is an element");
    };
    let Op::If(if_op) = &root_element.children.ops[0] else {
        panic!("child is ui.if");
    };
    assert_eq!(if_op.branches.len(), 1);
    let branch = &if_op.branches[0];
    assert_eq!(
        branch
            .condition
            .as_ref()
            .map(|condition| condition.source()),
        Some("ok")
    );
    let Op::Element(branch_element) = &branch.region.ops[0] else {
        panic!("branch carries one element");
    };
    assert_eq!(branch_element.tag, "li");
    let Op::Interpolation(interpolation) = &branch_element.children.ops[0] else {
        panic!("branch child is interpolation");
    };
    assert_eq!(interpolation.expression.source(), "item");
}

#[test]
fn map_child_projects_to_s2_for_scope() {
    let allocator = Allocator::new();
    let source = "const App = () => <ul>{items.map((item, index) => <li>{item}</li>)}</ul>;";
    let lowered = lower_source(&allocator, allocator.as_oxc(), source, JsxLang::Jsx);
    let root = lowered.roots.first().expect("one JSX root");

    let s2 = root.s2.as_ref().expect("map child projects to S2");
    assert!(s2.features.has_for_ops());
    assert!(!s2.features.has_if_ops());
    let Op::Element(root_element) = &s2.root.ops[0] else {
        panic!("root is an element");
    };
    let Op::For(for_op) = &root_element.children.ops[0] else {
        panic!("child is ui.for");
    };
    assert_eq!(for_op.binding.source.source(), "items");
    assert_eq!(for_op.binding.value.source(), "item");
    assert_eq!(
        for_op.binding.key.as_ref().map(|alias| alias.source()),
        Some("index")
    );
    assert!(for_op.binding.index.is_none());
    let Op::Element(item_element) = &for_op.region.ops[0] else {
        panic!("for body carries one element");
    };
    assert_eq!(item_element.tag, "li");

    let for_id = NodeId::from_index(1).expect("ui.for is second op");
    let facts = s2.scopes.get(for_id).expect("ui.for records scope facts");
    assert_eq!(facts.tag.index(), 0);
    assert_eq!(facts.bindings.len(), 2);
    assert_eq!(facts.bindings[0].name, "item");
    assert_eq!(facts.bindings[1].name, "index");
    assert!(matches!(
        facts.bindings[0].origin,
        ScopeOrigin::Authored { span } if span == for_op.binding.value.span()
    ));
    assert!(matches!(
        facts.bindings[1].origin,
        ScopeOrigin::Authored { span } if Some(span) == for_op.binding.key.as_ref().map(|alias| alias.span())
    ));
}
