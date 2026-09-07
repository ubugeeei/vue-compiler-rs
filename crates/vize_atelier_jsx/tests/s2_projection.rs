//! P2-16 JSX-to-S2 projection witnesses.

use vize_atelier_jsx::{JsxLang, lower_source};
use vize_s0::Allocator;
use vize_s2::op::{BindingOp, DynamicName, Namespace, Op};

#[test]
fn dynamic_bind_props_project_to_s2_bindings() {
    let allocator = Allocator::new();
    let source = "const App = () => <div id={name} {...attrs} disabled />";
    let lowered = lower_source(&allocator, allocator.as_oxc(), source, JsxLang::Jsx);
    assert!(!lowered.has_errors(), "{:?}", lowered.diagnostics);
    let root = lowered.roots.first().expect("one JSX root");

    let s2 = root.s2.as_ref().expect("dynamic bindings are admitted");
    assert_eq!(s2.op_count, 3);
    let Op::Element(element) = &s2.root.ops[0] else {
        panic!("root is an element");
    };
    assert_eq!(element.attributes.len(), 1);
    assert_eq!(element.attributes[0].name, "disabled");
    assert_eq!(element.bindings.len(), 2);

    let BindingOp::Bind(id) = &element.bindings[0] else {
        panic!("first binding is ui.bind");
    };
    assert!(matches!(id.name, Some(DynamicName::Static("id"))));
    let id_value = id.value.expect("id binding has a value");
    assert_eq!(id_value.source(), "name");
    assert_eq!(id_value.span().start, source.find("name").unwrap() as u32);
    assert_eq!(id.span.start, source.find("id={name}").unwrap() as u32);

    let BindingOp::Bind(spread) = &element.bindings[1] else {
        panic!("second binding is ui.bind");
    };
    assert!(spread.name.is_none());
    let spread_value = spread.value.expect("spread binding has a value");
    assert_eq!(spread_value.source(), "attrs");
    assert_eq!(
        spread_value.span().start,
        source.find("attrs").unwrap() as u32
    );
    assert_eq!(spread.span.start, source.find("{...attrs}").unwrap() as u32);
}

#[test]
fn v_on_directives_project_to_s2_on_bindings() {
    let allocator = Allocator::new();
    let source = "const App = () => <button v-on:click={handle} />";
    let lowered = lower_source(&allocator, allocator.as_oxc(), source, JsxLang::Jsx);
    assert!(!lowered.has_errors(), "{:?}", lowered.diagnostics);
    let root = lowered.roots.first().expect("one JSX root");

    let s2 = root.s2.as_ref().expect("event binding is admitted");
    assert_eq!(s2.op_count, 2);
    let Op::Element(element) = &s2.root.ops[0] else {
        panic!("root is an element");
    };
    assert_eq!(element.bindings.len(), 1);

    let BindingOp::On(click) = &element.bindings[0] else {
        panic!("binding is ui.on");
    };
    assert!(matches!(click.name, Some(DynamicName::Static("click"))));
    assert!(click.modifiers.is_empty());
    let handler = click.handler.expect("event binding has a handler");
    assert_eq!(handler.source(), "handle");
    assert_eq!(handler.span().start, source.find("handle").unwrap() as u32);
    let event_attr = "v-on:click={handle}";
    let event_attr_start = source.find(event_attr).unwrap() as u32;
    assert_eq!(click.span.start, event_attr_start);
    assert_eq!(click.span.end, event_attr_start + event_attr.len() as u32);
}

#[test]
fn event_handler_directives_project_to_s2_on_bindings() {
    let allocator = Allocator::new();
    let source = "const App = () => <button id=\"save\" disabled={isDisabled} \
                  onClickPassiveCapture={handle} />";
    let lowered = lower_source(&allocator, allocator.as_oxc(), source, JsxLang::Jsx);
    assert!(!lowered.has_errors(), "{:?}", lowered.diagnostics);
    let root = lowered.roots.first().expect("one JSX root");

    let s2 = root.s2.as_ref().expect("event bindings are admitted");
    assert_eq!(s2.op_count, 3);
    let Op::Element(element) = &s2.root.ops[0] else {
        panic!("root is an element");
    };
    assert_eq!(element.attributes.len(), 1);
    assert_eq!(element.attributes[0].name, "id");
    assert_eq!(element.attributes[0].value, Some("save"));
    assert_eq!(element.bindings.len(), 2);

    let BindingOp::Bind(disabled) = &element.bindings[0] else {
        panic!("first binding is ui.bind");
    };
    assert!(matches!(
        disabled.name,
        Some(DynamicName::Static("disabled"))
    ));
    let disabled_value = disabled.value.expect("disabled binding has a value");
    assert_eq!(disabled_value.source(), "isDisabled");

    let BindingOp::On(click) = &element.bindings[1] else {
        panic!("second binding is ui.on");
    };
    assert!(matches!(click.name, Some(DynamicName::Static("click"))));
    assert_eq!(click.modifiers.as_slice(), ["passive", "capture"]);
    let handler = click.handler.expect("event binding has a handler");
    assert_eq!(handler.source(), "handle");
    assert_eq!(handler.span().start, source.find("handle").unwrap() as u32);
    let event_attr = "onClickPassiveCapture={handle}";
    let event_attr_start = source.find(event_attr).unwrap() as u32;
    assert_eq!(click.span.start, event_attr_start);
    assert_eq!(click.span.end, event_attr_start + event_attr.len() as u32);
}

#[test]
fn component_default_children_project_to_s2_regions() {
    let allocator = Allocator::new();
    let source = "const App = () => <Panel title=\"Hi\"><span>{label}</span></Panel>;";
    let lowered = lower_source(&allocator, allocator.as_oxc(), source, JsxLang::Jsx);
    assert!(!lowered.has_errors(), "{:?}", lowered.diagnostics);
    let root = lowered.roots.first().expect("one JSX root");

    let s2 = root.s2.as_ref().expect("component children are admitted");
    assert_eq!(s2.op_count, 3);
    assert!(s2.features.has_slot_carriers());
    let Op::Component(component) = &s2.root.ops[0] else {
        panic!("root is a component");
    };
    assert_eq!(component.name, "Panel");
    assert_eq!(component.attributes.len(), 1);
    assert_eq!(component.attributes[0].name, "title");
    assert_eq!(component.attributes[0].value, Some("Hi"));
    assert_eq!(component.children.ops.len(), 1);

    let Op::Element(span) = &component.children.ops[0] else {
        panic!("default child is an element");
    };
    assert_eq!(span.tag, "span");
    assert_eq!(span.namespace, Namespace::Html);
    assert_eq!(span.children.ops.len(), 1);
    let Op::Interpolation(label) = &span.children.ops[0] else {
        panic!("span child is an interpolation");
    };
    assert_eq!(label.expression.source(), "label");
    assert_eq!(
        label.expression.span().start,
        source.find("label").unwrap() as u32
    );
}

#[test]
fn fragment_children_project_to_one_s2_region() {
    let allocator = Allocator::new();
    let source = "const App = () => <><span />{count}</>;";
    let lowered = lower_source(&allocator, allocator.as_oxc(), source, JsxLang::Jsx);
    assert!(!lowered.has_errors(), "{:?}", lowered.diagnostics);
    let root = lowered.roots.first().expect("one JSX root");

    let s2 = root.s2.as_ref().expect("fragment children are admitted");
    assert_eq!(s2.op_count, 2);
    assert_eq!(s2.root.ops.len(), 2);
    let Op::Element(span) = &s2.root.ops[0] else {
        panic!("first fragment child is an element");
    };
    assert_eq!(span.tag, "span");
    assert!(span.children.ops.is_empty());
    let Op::Interpolation(count) = &s2.root.ops[1] else {
        panic!("second fragment child is an interpolation");
    };
    assert_eq!(count.expression.source(), "count");
}
