//! P2-16 differential witnesses for JSX VDOM S2 migration.

use vize_croquis::Croquis;
use vize_s0::{Allocator, String};

use crate::s2::S2Refusal;
use crate::{JsxLang, lower_source};

use super::{VdomCompatOptions, VdomCompileOptions, compile_root_to_vdom};

#[test]
fn s2_vdom_admitted_cases_match_relief_codegen() {
    let cases = [
        (
            "element",
            "const A = () => <div class=\"card\" />;",
            JsxLang::Jsx,
        ),
        ("text", "const A = () => <p>Hello</p>;", JsxLang::Jsx),
        (
            "interpolation",
            "const A = () => <div>{count}</div>;",
            JsxLang::Jsx,
        ),
        (
            "logical and child",
            "const A = () => <ul>{ok && <li>{item}</li>}</ul>;",
            JsxLang::Jsx,
        ),
        (
            "ternary child",
            "const A = () => <div>{ok ? <a/> : <b/>}</div>;",
            JsxLang::Jsx,
        ),
        (
            "map child",
            "const A = () => <ul>{items.map((item) => <li>{item}</li>)}</ul>;",
            JsxLang::Jsx,
        ),
        (
            "fragment root",
            "const A = () => <><i/><b/></>;",
            JsxLang::Jsx,
        ),
        (
            "element bind",
            "const A = () => <button disabled={off} />;",
            JsxLang::Jsx,
        ),
        (
            "element event",
            "const A = () => <button onClick={save} />;",
            JsxLang::Jsx,
        ),
        (
            "leaf component",
            "const A = () => <B foo={f} title=\"ok\" />;",
            JsxLang::Jsx,
        ),
        (
            "component spread",
            "const A = () => <B {...attrs} foo={f} title=\"ok\" />;",
            JsxLang::Jsx,
        ),
        (
            "component implicit default",
            "const A = () => <Card><h1>Title</h1></Card>;",
            JsxLang::Jsx,
        ),
        (
            "component text default",
            "const A = () => <Card>Title</Card>;",
            JsxLang::Jsx,
        ),
        (
            "component interpolation default",
            "const A = () => <Card>{title}</Card>;",
            JsxLang::Jsx,
        ),
        (
            "component nested component default",
            "const A = () => <Card><B foo={f}/></Card>;",
            JsxLang::Jsx,
        ),
        (
            "component capture event",
            "const A = () => <B onClickCapture={h} />;",
            JsxLang::Jsx,
        ),
        (
            "component once event",
            "const A = () => <B onClickOnce={h} />;",
            JsxLang::Jsx,
        ),
        (
            "component passive capture event",
            "const A = () => <B onInputPassiveCapture={h} />;",
            JsxLang::Jsx,
        ),
        (
            "component v-model",
            "const A = () => <Input v-model={value} />;",
            JsxLang::Jsx,
        ),
        (
            "component v-model argument modifiers",
            "const A = () => <Input v-model={[value, \"foo\", [\"trim\"]]} />;",
            JsxLang::Jsx,
        ),
        (
            "component paramless named slots",
            "const A = () => <Comp>{{ header: () => <h1>Hi</h1>, footer: () => <p>Bye</p> \
             }}</Comp>;",
            JsxLang::Jsx,
        ),
        (
            "component paramless default slot",
            "const A = () => <List>{() => <li>Empty</li>}</List>;",
            JsxLang::Jsx,
        ),
        (
            "dynamic component",
            "const A = () => <Widget.Panel foo={1} />;",
            JsxLang::Jsx,
        ),
        (
            "element v-show",
            "const A = () => <div v-show={visible} />;",
            JsxLang::Jsx,
        ),
        (
            "scoped style",
            r#"const A = () => <><section class="box" /><style scoped>{`.box { color: red; }`}</style></>;"#,
            JsxLang::Jsx,
        ),
        (
            "element v-html",
            "const A = () => <div v-html={raw} />;",
            JsxLang::Jsx,
        ),
        (
            "element v-html with children",
            "const A = () => <div v-html={raw}>fallback</div>;",
            JsxLang::Jsx,
        ),
        (
            "element v-text",
            "const A = () => <div v-text={msg} />;",
            JsxLang::Jsx,
        ),
        (
            "element v-text with children",
            "const A = () => <div v-text={msg}>fallback</div>;",
            JsxLang::Jsx,
        ),
        (
            "element input v-model",
            "const A = () => <input v-model={value} />;",
            JsxLang::Jsx,
        ),
        (
            "element text input v-model",
            "const A = () => <input type=\"text\" v-model={value} />;",
            JsxLang::Jsx,
        ),
        (
            "element input v-model modifier array",
            "const A = () => <input v-model={[value, [\"trim\"]]} />;",
            JsxLang::Jsx,
        ),
        (
            "element input v-model underscore modifiers",
            "const A = () => <input v-model_number_lazy={value} />;",
            JsxLang::Jsx,
        ),
        (
            "element checkbox input v-model",
            "const A = () => <input type=\"checkbox\" v-model={checked} />;",
            JsxLang::Jsx,
        ),
        (
            "element radio input v-model",
            "const A = () => <input type=\"radio\" v-model={picked} />;",
            JsxLang::Jsx,
        ),
        (
            "element textarea v-model",
            "const A = () => <textarea v-model={value} />;",
            JsxLang::Jsx,
        ),
        (
            "element textarea v-model modifier",
            "const A = () => <textarea v-model_lazy={value} />;",
            JsxLang::Jsx,
        ),
        (
            "element select v-model",
            "const A = () => <select v-model={value}></select>;",
            JsxLang::Jsx,
        ),
        (
            "element select v-model modifier",
            "const A = () => <select v-model_number={value}></select>;",
            JsxLang::Jsx,
        ),
        (
            "component v-show",
            "const A = () => <B v-show={visible} />;",
            JsxLang::Jsx,
        ),
        (
            "tsx interpolation cast",
            "const A = () => <div>{count as number}</div>;",
            JsxLang::Tsx,
        ),
        (
            "tsx typed event",
            "const A = () => <button onClick={(event: MouseEvent) => save(event)} />;",
            JsxLang::Tsx,
        ),
    ];

    for (name, source, lang) in cases {
        let s2 = compile_case(source, lang, EmitRoute::ForceS2);
        let relief = compile_case(source, lang, EmitRoute::ForceRelief);

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

fn compile_case(source: &str, lang: JsxLang, route: EmitRoute) -> Output {
    let allocator = Allocator::new();
    let mut lowered = lower_source(&allocator, allocator.as_oxc(), source, lang);
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
        lang.is_typescript(),
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
