use vize_croquis::Croquis;
use vize_s0::{Allocator, String};

use crate::s2::S2Refusal;
use crate::{JsxLang, lower_source};

use super::super::{VdomCompatOptions, VdomCompileOptions, compile_root_to_vdom};

#[test]
fn component_option_event_modifiers_emit_from_s2() {
    let source = "const A = () => <B onClickCapture={h} />;";
    let s2 = compile_case(source, EmitRoute::ForceS2);
    let relief = compile_case(source, EmitRoute::ForceRelief);

    assert_eq!(s2.preamble, relief.preamble);
    assert_eq!(s2.code, relief.code);
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
        EmitRoute::ForceS2 => {
            let s2 = root
                .s2
                .as_ref()
                .expect("component event option modifiers project to S2");
            assert_eq!(super::root_is_supported(s2), true);
            root.root.children.clear();
        }
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

    Output {
        preamble: component.preamble,
        code: component.code,
    }
}
