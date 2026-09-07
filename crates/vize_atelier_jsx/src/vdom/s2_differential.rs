//! P2-16 differential witnesses for JSX VDOM S2 migration.

use vize_croquis::Croquis;
use vize_s0::{Allocator, String};

use crate::s2::S2Refusal;
use crate::{JsxLang, lower_source};

use super::{VdomCompatOptions, VdomCompileOptions, compile_root_to_vdom};

#[test]
fn s2_vdom_admitted_cases_match_relief_codegen() {
    let cases = [
        ("element", "const A = () => <div class=\"card\" />;"),
        ("text", "const A = () => <p>Hello</p>;"),
        ("interpolation", "const A = () => <div>{count}</div>;"),
        ("element bind", "const A = () => <button disabled={off} />;"),
        (
            "element event",
            "const A = () => <button onClick={save} />;",
        ),
        (
            "leaf component",
            "const A = () => <B foo={f} title=\"ok\" />;",
        ),
        (
            "component spread",
            "const A = () => <B {...attrs} foo={f} title=\"ok\" />;",
        ),
    ];

    for (name, source) in cases {
        let s2 = compile_case(source, EmitRoute::ForceS2);
        let relief = compile_case(source, EmitRoute::ForceRelief);

        assert_eq!(
            s2.preamble, relief.preamble,
            "{name}: S2 VDOM preamble diverged from Relief"
        );
        assert_eq!(
            s2.code, relief.code,
            "{name}: S2 VDOM code diverged from Relief"
        );
    }
}

#[derive(Clone, Copy)]
enum EmitRoute {
    ForceS2,
    ForceRelief,
}

struct Output {
    preamble: String,
    code: String,
}

fn compile_case(source: &str, route: EmitRoute) -> Output {
    let allocator = Allocator::new();
    let mut lowered = lower_source(&allocator, allocator.as_oxc(), source, JsxLang::Jsx);
    assert!(!lowered.has_errors(), "{:?}", lowered.diagnostics);

    let analysis: &Croquis = allocator.alloc_owned(lowered.analysis);
    let mut root = lowered.roots.pop().expect("one JSX root");
    assert!(lowered.roots.is_empty(), "expected exactly one JSX root");

    match route {
        EmitRoute::ForceS2 => root.root.children.clear(),
        EmitRoute::ForceRelief => root.s2 = Err(S2Refusal::UnsupportedChild),
    }

    let mut diagnostics = Vec::new();
    let component = compile_root_to_vdom(
        &allocator,
        root,
        analysis,
        false,
        &VdomCompileOptions::default(),
        VdomCompatOptions::default(),
        &mut diagnostics,
        source,
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert!(component.map.is_none(), "S2 parity lane is source-map-free");

    Output {
        preamble: component.preamble,
        code: component.code,
    }
}
