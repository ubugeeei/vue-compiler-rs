//! Focused Davinci parity pins for bound literal SVG static-props hoists.

#![allow(
    clippy::disallowed_macros,
    clippy::disallowed_types,
    clippy::disallowed_methods
)]

mod support;

use support::with_transformed;
use vize_s0::Allocator;
use vize_s1_to_s2::emit_dom;

fn shipped(source: &str) -> String {
    let allocator = Allocator::new();
    let (_, errors, old) = vize_atelier_dom::compile_template(&allocator, source);
    let blocking: Vec<_> = errors
        .iter()
        .filter(|error| !error.is_compatibility_notice())
        .collect();
    assert!(blocking.is_empty(), "{source:?}: {blocking:?}");
    format!("{}\n{}", old.preamble, old.code)
}

fn emitted(source: &str) -> String {
    with_transformed(source, |lowered, _folio, facts, _budget| {
        emit_dom(lowered, facts)
            .unwrap_or_else(|error| panic!("emit refused {source:?}: {error:?}"))
            .assembled()
            .to_string()
    })
}

fn assert_shipped_parity(source: &str) {
    assert_eq!(emitted(source), shipped(source), "{source}");
}

#[test]
fn root_svg_static_bind_props_use_legacy_hoist() {
    assert_shipped_parity(r#"<svg xmlns="http://www.w3.org/2000/svg" :width="0"></svg>"#);
}

#[test]
fn child_svg_static_bind_props_use_legacy_hoist() {
    assert_shipped_parity(
        r#"<div><svg xmlns="http://www.w3.org/2000/svg" :width="0"></svg></div>"#,
    );
}
