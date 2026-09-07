use vize_s0::Allocator;

use crate::{JsxLang, lower_source};

use super::super::{VdomCompatOptions, VdomCompileOptions, VdomComponent, compile_to_vdom};

const SOURCE_MAP_SOURCE: &str = "const A = () => <div>{count}</div>;";
const HOIST_STATIC_SOURCE: &str = "const A = () => <Card><span class=\"label\">Hi</span></Card>;";
const CACHE_HANDLERS_SOURCE: &str = "const A = () => <button v-on:click={save()} />;";

#[test]
fn source_map_option_stays_on_relief_vdom_fallback() {
    let options = VdomCompileOptions {
        source_map: true,
        ..Default::default()
    };
    assert_s2_emit_declines(SOURCE_MAP_SOURCE, &options, "source map");

    let component = compile(SOURCE_MAP_SOURCE, options);
    let map = component.map.as_deref().expect("Relief emits source maps");
    let value: serde_json::Value = serde_json::from_str(map).expect("valid source-map JSON");
    assert_eq!(value["version"], 3);
    assert!(
        value["mappings"]
            .as_str()
            .is_some_and(|mappings| !mappings.is_empty()),
        "{map}"
    );
}

#[test]
fn hoist_static_option_stays_on_relief_vdom_fallback() {
    let options = VdomCompileOptions {
        hoist_static: true,
        ..Default::default()
    };
    assert_s2_emit_declines(HOIST_STATIC_SOURCE, &options, "static hoisting");

    let component = compile(HOIST_STATIC_SOURCE, options);
    assert!(
        component.preamble.contains("const _hoisted_1"),
        "{}",
        component.preamble
    );
    assert!(component.code.contains("_hoisted_1"), "{}", component.code);
}

#[test]
fn cache_handlers_option_stays_on_relief_vdom_fallback() {
    let default = compile(CACHE_HANDLERS_SOURCE, VdomCompileOptions::default());
    assert!(
        !default.code.contains("_cache[0] || (_cache[0] ="),
        "{}",
        default.code
    );

    let options = VdomCompileOptions {
        cache_handlers: true,
        ..Default::default()
    };
    assert_s2_emit_declines(CACHE_HANDLERS_SOURCE, &options, "handler caching");

    let component = compile(CACHE_HANDLERS_SOURCE, options);
    assert!(
        component.code.contains("_cache[0] || (_cache[0] ="),
        "{}",
        component.code
    );
}

fn assert_s2_emit_declines(source: &str, options: &VdomCompileOptions, projection: &str) {
    let allocator = Allocator::new();
    let mut lowered = lower_source(&allocator, allocator.as_oxc(), source, JsxLang::Jsx);
    assert!(!lowered.has_errors(), "{:?}", lowered.diagnostics);

    let root = lowered.roots.pop().expect("one JSX root");
    assert!(lowered.roots.is_empty(), "expected exactly one JSX root");
    let s2 = root.s2;
    assert!(
        super::root_is_supported(s2.as_ref().expect(projection)),
        "{projection} should be S2-supported before option fallback"
    );

    let emit = super::try_emit_s2_vdom(
        &allocator,
        s2,
        false,
        None,
        None,
        options,
        &VdomCompatOptions::default(),
    );
    assert!(emit.is_none(), "{projection} should stay on Relief");
}

fn compile(source: &str, options: VdomCompileOptions) -> VdomComponent {
    let allocator = Allocator::new();
    let out = compile_to_vdom(&allocator, source, JsxLang::Jsx, options);
    assert!(!out.has_errors(), "diagnostics: {:?}", out.diagnostics);
    assert_eq!(out.components.len(), 1, "expected one component");
    out.components.into_iter().next().unwrap()
}
