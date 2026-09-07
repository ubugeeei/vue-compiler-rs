use vize_s0::Allocator;
use vize_s2::op::{BindingOp, DynamicName, Op};

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
    assert!(!s2.features.has_if_ops());
    assert!(!s2.features.has_for_ops());
    assert!(!s2.features.has_slot_carriers());
    assert!(!s2.features.has_text_compounds());
    assert!(!s2.features.has_model_bindings());
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
    assert!(s2.features.has_slot_carriers());
    assert!(!s2.features.has_if_ops());
    assert!(!s2.features.has_for_ops());
    assert!(!s2.features.has_text_compounds());
    assert!(!s2.features.has_model_bindings());
    let Op::Component(component) = &s2.root.ops[0] else {
        panic!("root is a component");
    };
    assert_eq!(component.name, "Panel");
    assert_eq!(component.attributes[0].name, "title");
}

#[test]
fn paramless_static_slots_project_to_s2_slot_content() {
    let allocator = Allocator::new();
    let source = "const App = () => <Comp>{{ header: () => <h1>Hi</h1> }}</Comp>;";
    let lowered = lower_source(&allocator, allocator.as_oxc(), source, JsxLang::Jsx);
    let root = lowered.roots.first().expect("one JSX root");

    let s2 = root.s2.as_ref().expect("paramless slot projects to S2");
    assert!(s2.features.has_slot_carriers());
    let Op::Component(component) = &s2.root.ops[0] else {
        panic!("root is a component");
    };
    let Op::Element(template) = &component.children.ops[0] else {
        panic!("slot carrier is a template element");
    };
    assert_eq!(template.tag, "template");
    let BindingOp::SlotContent(content) = &template.bindings[0] else {
        panic!("binding is ui.slot-content");
    };
    assert!(content.params.is_none());
    assert!(content.modifiers.is_empty());
    assert!(matches!(content.name, Some(DynamicName::Static("header"))));
    assert!(template.span.start <= content.span.start);
    assert!(template.span.end >= content.span.end);
}

#[test]
fn scoped_slots_stay_on_directive_refusal_path() {
    let allocator = Allocator::new();
    let lowered = lower_source(
        &allocator,
        allocator.as_oxc(),
        "const App = () => <Comp>{{ item: (row) => <span>{row}</span> }}</Comp>;",
        JsxLang::Jsx,
    );
    let root = lowered.roots.first().expect("one JSX root");

    assert!(matches!(root.s2, Err(S2Refusal::Directive)));
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
fn v_html_directive_projects_to_s2_vue_html() {
    let allocator = Allocator::new();
    let source = "const App = () => <div v-html={raw} />";
    let lowered = lower_source(&allocator, allocator.as_oxc(), source, JsxLang::Jsx);
    let root = lowered.roots.first().expect("one JSX root");

    let s2 = root.s2.as_ref().expect("v-html projects to S2");
    let Op::Element(element) = &s2.root.ops[0] else {
        panic!("root is an element");
    };
    let BindingOp::VueHtml(html) = &element.bindings[0] else {
        panic!("binding is vue.html");
    };
    assert_eq!(html.value.as_ref().map(|value| value.source()), Some("raw"));
    assert_eq!(html.span.start, source.find("v-html={raw}").unwrap() as u32);
}

#[test]
fn v_text_directive_projects_to_s2_vue_text() {
    let allocator = Allocator::new();
    let source = "const App = () => <div v-text={msg} />";
    let lowered = lower_source(&allocator, allocator.as_oxc(), source, JsxLang::Jsx);
    let root = lowered.roots.first().expect("one JSX root");

    let s2 = root.s2.as_ref().expect("v-text projects to S2");
    let Op::Element(element) = &s2.root.ops[0] else {
        panic!("root is an element");
    };
    let BindingOp::VueText(text) = &element.bindings[0] else {
        panic!("binding is vue.text");
    };
    assert_eq!(text.value.as_ref().map(|value| value.source()), Some("msg"));
    assert_eq!(text.span.start, source.find("v-text={msg}").unwrap() as u32);
}

#[test]
fn component_v_model_projects_to_s2_model() {
    let allocator = Allocator::new();
    let source = "const App = () => <Input v-model={value} />";
    let lowered = lower_source(&allocator, allocator.as_oxc(), source, JsxLang::Jsx);
    let root = lowered.roots.first().expect("one JSX root");

    let s2 = root.s2.as_ref().expect("component v-model projects to S2");
    assert!(s2.features.has_model_bindings());
    let Op::Component(component) = &s2.root.ops[0] else {
        panic!("root is a component");
    };
    let BindingOp::Model(model) = &component.bindings[0] else {
        panic!("binding is ui.model");
    };
    assert_eq!(model.contract.read.source(), "value");
    assert_eq!(model.contract.write.source(), "value");
    assert!(model.argument.is_none());
    assert_eq!(model.attributes[0].name, "element-kind");
    assert_eq!(model.attributes[0].value, Some("component"));
}

#[test]
fn component_v_model_static_arg_modifiers_project_to_s2_model() {
    let allocator = Allocator::new();
    let source = "const App = () => <Input v-model={[value, \"foo\", [\"trim\"]]} />";
    let lowered = lower_source(&allocator, allocator.as_oxc(), source, JsxLang::Jsx);
    let root = lowered.roots.first().expect("one JSX root");

    let s2 = root.s2.as_ref().expect("component v-model projects to S2");
    assert!(s2.features.has_model_bindings());
    let Op::Component(component) = &s2.root.ops[0] else {
        panic!("root is a component");
    };
    let BindingOp::Model(model) = &component.bindings[0] else {
        panic!("binding is ui.model");
    };
    assert_eq!(model.contract.read.source(), "value");
    assert!(matches!(model.argument, Some(DynamicName::Static("foo"))));
    assert_eq!(model.attributes[0].value, Some("component"));
    assert_eq!(model.attributes[1].name, "trim");
    assert!(model.attributes[1].value.is_none());
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
fn element_v_model_stays_on_directive_refusal_path() {
    let allocator = Allocator::new();
    let lowered = lower_source(
        &allocator,
        allocator.as_oxc(),
        "const App = () => <input v-model={value} />",
        JsxLang::Jsx,
    );
    let root = lowered.roots.first().expect("one JSX root");

    assert!(matches!(root.s2, Err(S2Refusal::Directive)));
}

#[test]
fn v_html_without_value_stays_on_directive_refusal_path() {
    let allocator = Allocator::new();
    let lowered = lower_source(
        &allocator,
        allocator.as_oxc(),
        "const App = () => <div v-html />",
        JsxLang::Jsx,
    );
    let root = lowered.roots.first().expect("one JSX root");

    assert!(matches!(root.s2, Err(S2Refusal::Directive)));
}

#[test]
fn component_v_html_stays_on_directive_refusal_path() {
    let allocator = Allocator::new();
    let lowered = lower_source(
        &allocator,
        allocator.as_oxc(),
        "const App = () => <Panel v-html={raw} />",
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
