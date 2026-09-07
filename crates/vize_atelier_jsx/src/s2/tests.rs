use vize_s0::Allocator;
use vize_s2::op::{BindingOp, Op};

use crate::{JsxLang, lower_source};

use super::S2Refusal;

#[test]
fn lower_source_attaches_static_intrinsic_s2_root() {
    let allocator = Allocator::new();
    let source = "const App = () => <div id=\"x\">hello {name}<span hidden /></div>";
    let lowered = lower_source(&allocator, allocator.as_oxc(), source, JsxLang::Jsx);
    let root = lowered.roots.first().expect("one JSX root");

    let s2 = root.s2.as_ref().expect("static intrinsic S2 root");
    assert_eq!(s2.source, source);
    assert_eq!(s2.op_count, 4);
    let Op::Element(element) = &s2.root.ops[0] else {
        panic!("root is an element");
    };
    assert_eq!(element.tag, "div");
    assert_eq!(element.attributes[0].name, "id");
    assert_eq!(element.attributes[0].value, Some("x"));
    let Op::Interpolation(interpolation) = &element.children.ops[1] else {
        panic!("second child is interpolation");
    };
    assert_eq!(interpolation.expression.source(), "name");
    assert_eq!(
        interpolation.expression.span().start,
        source.find("name").unwrap() as u32
    );
}

#[test]
fn lower_source_attaches_component_s2_root() {
    let allocator = Allocator::new();
    let source = "const App = () => <Panel title=\"x\" />";
    let lowered = lower_source(&allocator, allocator.as_oxc(), source, JsxLang::Jsx);
    let root = lowered.roots.first().expect("one JSX root");

    let s2 = root.s2.as_ref().expect("component S2 root");
    let Op::Component(component) = &s2.root.ops[0] else {
        panic!("root is a component");
    };
    assert_eq!(component.name, "Panel");
    assert_eq!(component.attributes[0].name, "title");
}

#[test]
fn v_show_directive_projects_to_s2_vue_show() {
    let allocator = Allocator::new();
    let source = "const App = () => <div v-show={visible} />";
    let lowered = lower_source(&allocator, allocator.as_oxc(), source, JsxLang::Jsx);
    let root = lowered.roots.first().expect("one JSX root");

    let s2 = root.s2.as_ref().expect("v-show projects to S2");
    assert_eq!(s2.op_count, 2);
    let Op::Element(element) = &s2.root.ops[0] else {
        panic!("root is an element");
    };
    assert_eq!(element.bindings.len(), 1);
    let BindingOp::VueShow(show) = &element.bindings[0] else {
        panic!("binding is vue.show");
    };
    assert_eq!(show.value.source(), "visible");
    assert_eq!(
        show.value.span().start,
        source.find("visible").unwrap() as u32
    );
    assert_eq!(
        show.span.start,
        source.find("v-show={visible}").unwrap() as u32
    );
}

#[test]
fn v_show_without_value_stays_on_directive_refusal_path() {
    let allocator = Allocator::new();
    let lowered = lower_source(
        &allocator,
        allocator.as_oxc(),
        "const App = () => <div v-show />",
        JsxLang::Jsx,
    );
    let root = lowered.roots.first().expect("one JSX root");

    assert!(matches!(root.s2, Err(S2Refusal::Directive)));
}

#[test]
fn v_show_with_argument_stays_on_directive_refusal_path() {
    let allocator = Allocator::new();
    let lowered = lower_source(
        &allocator,
        allocator.as_oxc(),
        "const App = () => <div v-show:display={visible} />",
        JsxLang::Jsx,
    );
    let root = lowered.roots.first().expect("one JSX root");

    assert!(matches!(root.s2, Err(S2Refusal::Directive)));
}
