use vize_s0::Allocator;
use vize_s2::op::{BindingOp, Op};

use crate::{JsxLang, lower_source};

use super::S2Refusal;

#[test]
fn plain_input_v_model_projects_to_s2_model() {
    let allocator = Allocator::new();
    let source = "const App = () => <input v-model={value} />";
    let lowered = lower_source(&allocator, allocator.as_oxc(), source, JsxLang::Jsx);
    let root = lowered.roots.first().expect("one JSX root");

    let s2 = root.s2.as_ref().expect("input v-model projects to S2");
    assert_eq!(s2.op_count, 2);
    let Op::Element(element) = &s2.root.ops[0] else {
        panic!("root is an element");
    };
    assert_eq!(element.tag, "input");
    assert_eq!(element.bindings.len(), 1);
    let BindingOp::Model(model) = &element.bindings[0] else {
        panic!("binding is ui.model");
    };
    assert_eq!(model.contract.read.source(), "value");
    assert_eq!(model.contract.write.source(), "value");
    assert!(model.argument.is_none());
    assert_eq!(model.attributes.len(), 1);
    assert_eq!(model.attributes[0].name, "element-kind");
    assert_eq!(model.attributes[0].value, Some("input"));
}

#[test]
fn text_input_v_model_projects_to_s2_model() {
    let allocator = Allocator::new();
    let source = "const App = () => <input type=\"text\" v-model={value} />";
    let lowered = lower_source(&allocator, allocator.as_oxc(), source, JsxLang::Jsx);
    let root = lowered.roots.first().expect("one JSX root");

    let s2 = root.s2.as_ref().expect("text input v-model projects to S2");
    let Op::Element(element) = &s2.root.ops[0] else {
        panic!("root is an element");
    };
    assert_eq!(element.attributes.len(), 1);
    assert_eq!(element.attributes[0].name, "type");
    assert_eq!(element.attributes[0].value, Some("text"));
    let BindingOp::Model(model) = &element.bindings[0] else {
        panic!("binding is ui.model");
    };
    assert_eq!(model.contract.read.source(), "value");
    assert_eq!(model.attributes[0].value, Some("input"));
}

#[test]
fn textarea_v_model_projects_to_s2_model() {
    let allocator = Allocator::new();
    let source = "const App = () => <textarea v-model={value} />";
    let lowered = lower_source(&allocator, allocator.as_oxc(), source, JsxLang::Jsx);
    let root = lowered.roots.first().expect("one JSX root");

    let s2 = root.s2.as_ref().expect("textarea v-model projects to S2");
    let Op::Element(element) = &s2.root.ops[0] else {
        panic!("root is an element");
    };
    assert_eq!(element.tag, "textarea");
    let BindingOp::Model(model) = &element.bindings[0] else {
        panic!("binding is ui.model");
    };
    assert_eq!(model.contract.read.source(), "value");
    assert!(model.argument.is_none());
    assert_eq!(model.attributes.len(), 1);
    assert_eq!(model.attributes[0].name, "element-kind");
    assert_eq!(model.attributes[0].value, Some("textarea"));
}

#[test]
fn unsupported_native_text_model_shapes_stay_on_directive_refusal_path() {
    let allocator = Allocator::new();
    let cases = [
        "const App = () => <input v-model:checked={value} />",
        "const App = () => <input v-model={[value, [\"trim\"]]} />",
        "const App = () => <input v-model_lazy={value} />",
        "const App = () => <input type=\"checkbox\" v-model={checked} />",
        "const App = () => <input type=\"email\" v-model={value} />",
        "const App = () => <input type={kind} v-model={value} />",
        "const App = () => <input {...attrs} v-model={value} />",
        "const App = () => <textarea v-model:foo={value} />",
        "const App = () => <textarea v-model={[value, [\"trim\"]]} />",
        "const App = () => <textarea v-model_lazy={value} />",
        "const App = () => <select v-model={value} />",
    ];

    for source in cases {
        let lowered = lower_source(&allocator, allocator.as_oxc(), source, JsxLang::Jsx);
        let root = lowered.roots.first().expect("one JSX root");

        assert!(matches!(root.s2, Err(S2Refusal::Directive)), "{source}");
    }
}
