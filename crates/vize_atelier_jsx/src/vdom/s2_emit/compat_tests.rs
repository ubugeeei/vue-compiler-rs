use vize_s0::Allocator;

use crate::{
    BabelJsxCustomizations, BabelJsxOptions, JsxCompatMode, JsxCompileConfig, JsxLang,
    JsxOutputMode, compile_jsx_with_babel_customizations, compile_jsx_with_babel_merge_props,
    compile_jsx_with_babel_object_slots, compile_jsx_with_babel_options,
    compile_jsx_with_babel_pragma, lower_source,
};

use super::super::{VdomCompatOptions, VdomCompileOptions};

const S2_SUPPORTED_SOURCE: &str = "const A = () => <div>{count}</div>;";

#[test]
fn babel_compat_surface_options_decline_s2_emit() {
    assert_s2_emit_declines_with_compat(
        S2_SUPPORTED_SOURCE,
        VdomCompatOptions {
            transform_on_helper: Some("_transformOn"),
            ..Default::default()
        },
        "transformOn",
    );
    assert_s2_emit_declines_with_compat(
        S2_SUPPORTED_SOURCE,
        VdomCompatOptions {
            object_slots_helpers: Some(("_isSlot", "_isVNode")),
            ..Default::default()
        },
        "object slots",
    );
    assert_s2_emit_declines_with_compat(
        S2_SUPPORTED_SOURCE,
        VdomCompatOptions {
            vnode_factory: Some("h"),
            ..Default::default()
        },
        "custom vnode factory",
    );
    assert_s2_emit_declines_with_compat(
        S2_SUPPORTED_SOURCE,
        VdomCompatOptions {
            merge_props: false,
            ..Default::default()
        },
        "mergeProps false",
    );
    assert_s2_emit_declines_with_compat(
        "const A = () => <input v-model={value} />;",
        VdomCompatOptions {
            allow_static_v_model_arg_on_element: true,
            ..Default::default()
        },
        "native input v-model with static element argument compat",
    );

    let custom_element_spans = [(0, 1)];
    assert_s2_emit_declines_with_compat(
        S2_SUPPORTED_SOURCE,
        VdomCompatOptions {
            custom_element_spans: &custom_element_spans,
            ..Default::default()
        },
        "isCustomElement spans",
    );
}

#[test]
fn babel_pragma_fallback_keeps_custom_factory_output() {
    let allocator = Allocator::new();
    let output = compile_jsx_with_babel_pragma(
        &allocator,
        "const A = () => <div id=\"x\"/>;",
        JsxLang::Jsx,
        &babel_vdom_config(),
        &BabelJsxOptions::default(),
        Some("h"),
    );
    let module = output.module_code();

    assert!(
        output.diagnostics.is_empty(),
        "{:?}\n{module}",
        output.diagnostics
    );
    assert!(module.contains("h(\"div\", { id: \"x\" })"), "{module}");
    assert!(!module.contains("_createElementBlock"), "{module}");
    assert!(!module.contains("from \"vue\""), "{module}");
}

#[test]
fn babel_transform_on_fallback_keeps_helper_output() {
    let allocator = Allocator::new();
    let output = compile_jsx_with_babel_options(
        &allocator,
        "const A = () => <button on={{ click: handler }}/>;",
        JsxLang::Jsx,
        &babel_vdom_config(),
        &BabelJsxOptions { transform_on: true },
    );
    let module = output.module_code();

    assert!(
        output.diagnostics.is_empty(),
        "{:?}\n{module}",
        output.diagnostics
    );
    assert_eq!(
        module.matches("@vue/babel-helper-vue-transform-on").count(),
        1,
        "{module}"
    );
    assert!(module.contains("_transformOn("), "{module}");
    assert!(module.contains("click: handler"), "{module}");
}

#[test]
fn babel_merge_props_false_fallback_keeps_object_spread_output() {
    let allocator = Allocator::new();
    let output = compile_jsx_with_babel_merge_props(
        &allocator,
        "const A = () => <div class=\"a\" {...p} class={c}/>;",
        JsxLang::Jsx,
        &babel_vdom_config(),
        &BabelJsxOptions::default(),
        false,
    );
    let module = output.module_code();

    assert!(
        output.diagnostics.is_empty(),
        "{:?}\n{module}",
        output.diagnostics
    );
    assert!(!module.contains("_mergeProps"), "{module}");
    let static_class = module.find("class: \"a\"").expect("static class");
    let spread = module.find("...p").expect("spread props");
    let dynamic_class = module.find("class: c").expect("dynamic class");
    assert!(static_class < spread && spread < dynamic_class, "{module}");
}

#[test]
fn babel_object_slots_fallback_keeps_runtime_slot_guard() {
    let allocator = Allocator::new();
    let output = compile_jsx_with_babel_object_slots(
        &allocator,
        "const A = () => <Comp>{slots}</Comp>;",
        JsxLang::Jsx,
        &babel_vdom_config(),
        &BabelJsxOptions::default(),
        true,
    );
    let module = output.module_code();

    assert!(
        output.diagnostics.is_empty(),
        "{:?}\n{module}",
        output.diagnostics
    );
    assert!(module.contains("function _isSlot"), "{module}");
    assert!(module.contains("_isSlot(slots)"), "{module}");
    assert!(module.contains("default: () => [slots]"), "{module}");
}

#[test]
fn babel_custom_element_fallback_keeps_predicate_tag_output() {
    let allocator = Allocator::new();
    let is_custom_element = |tag: &str| tag == "MyEl";
    let output = compile_jsx_with_babel_customizations(
        &allocator,
        "const A = () => <MyEl foo={1}/>;",
        JsxLang::Jsx,
        &babel_vdom_config(),
        &BabelJsxOptions::default(),
        BabelJsxCustomizations {
            is_custom_element: Some(&is_custom_element),
            ..Default::default()
        },
    );
    let module = output.module_code();

    assert!(
        output.diagnostics.is_empty(),
        "{:?}\n{module}",
        output.diagnostics
    );
    assert!(
        module.contains("_createElementBlock(\"MyEl\", { foo: 1 })"),
        "{module}"
    );
    assert!(!module.contains("_resolveComponent"), "{module}");
}

fn assert_s2_emit_declines_with_compat(
    source: &str,
    compat: VdomCompatOptions<'_>,
    projection: &str,
) {
    let allocator = Allocator::new();
    let mut lowered = lower_source(&allocator, allocator.as_oxc(), source, JsxLang::Jsx);
    assert!(!lowered.has_errors(), "{:?}", lowered.diagnostics);

    let root = lowered.roots.pop().expect("one JSX root");
    assert!(lowered.roots.is_empty(), "expected exactly one JSX root");
    let s2 = root.s2;
    assert!(
        super::root_is_supported(s2.as_ref().expect(projection)),
        "{projection} should be S2-supported before compat fallback"
    );

    let emit = super::try_emit_s2_vdom(
        &allocator,
        s2,
        false,
        None,
        None,
        &VdomCompileOptions::default(),
        &compat,
    );
    assert!(emit.is_none(), "{projection} should stay on Relief");
}

fn babel_vdom_config() -> JsxCompileConfig {
    JsxCompileConfig {
        compat: JsxCompatMode::Babel,
        default_mode: JsxOutputMode::Vdom,
        ..Default::default()
    }
}
