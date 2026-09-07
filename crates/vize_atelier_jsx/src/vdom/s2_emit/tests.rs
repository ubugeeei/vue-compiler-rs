use vize_croquis::Croquis;
use vize_s0::Allocator;

use crate::{JsxLang, JsxOutputMode, lower_source};

use super::super::{VdomCompatOptions, VdomCompileOptions, compile_root_to_vdom};

#[test]
fn native_vdom_admitted_roots_emit_from_s2() {
    let allocator = Allocator::new();
    let source = "const A = () => <div>{count}</div>;";
    let mut lowered = lower_source(&allocator, allocator.as_oxc(), source, JsxLang::Jsx);
    let analysis: &Croquis = allocator.alloc_owned(lowered.analysis);
    let mut root = lowered.roots.pop().expect("one JSX root");
    root.root.children.clear();

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
    assert_eq!(component.mode, JsxOutputMode::Vdom);
    assert_eq!(
        component.code.as_str(),
        "export function render(_ctx, _cache) {\n  return (_openBlock(), \
         _createElementBlock(\"div\", null, _toDisplayString(count), 1 /* TEXT */))\n}"
    );
}

#[test]
fn tsx_admitted_roots_emit_from_s2() {
    let allocator = Allocator::new();
    let source = "const A = () => <div>{count as number}</div>;";
    let mut lowered = lower_source(&allocator, allocator.as_oxc(), source, JsxLang::Tsx);
    let analysis: &Croquis = allocator.alloc_owned(lowered.analysis);
    let mut root = lowered.roots.pop().expect("one JSX root");
    let s2 = root.s2.as_ref().expect("TSX root projects to S2");

    assert_eq!(super::root_is_supported(s2), true);

    root.root.children.clear();
    let mut diagnostics = Vec::new();
    let component = compile_root_to_vdom(
        &allocator,
        root,
        analysis,
        true,
        &VdomCompileOptions::default(),
        VdomCompatOptions::default(),
        &mut diagnostics,
        source,
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert_eq!(component.mode, JsxOutputMode::Vdom);
    assert_eq!(
        component.code.as_str(),
        "export function render(_ctx, _cache) {\n  return (_openBlock(), \
         _createElementBlock(\"div\", null, _toDisplayString(count), 1 /* TEXT */))\n}"
    );
}

#[test]
fn component_plain_children_emit_from_s2_with_slot_facts() {
    let allocator = Allocator::new();
    let source = "const A = () => <Card><h1>Title</h1></Card>;";
    let mut lowered = lower_source(&allocator, allocator.as_oxc(), source, JsxLang::Jsx);
    let analysis: &Croquis = allocator.alloc_owned(lowered.analysis);
    let mut root = lowered.roots.pop().expect("one JSX root");
    let s2 = root.s2.as_ref().expect("component child projects to S2");

    assert_eq!(super::root_is_supported(s2), true);

    root.root.children.clear();
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
    assert_eq!(
        component.preamble.as_str(),
        "import { resolveComponent as _resolveComponent, createElementVNode as \
         _createElementVNode, openBlock as _openBlock, createBlock as _createBlock, withCtx as \
         _withCtx } from \"vue\"\n"
    );
    assert_eq!(
        component.code.as_str(),
        "export function render(_ctx, _cache) {\n  const _component_Card = \
         _resolveComponent(\"Card\")\n  \n  return (_openBlock(), _createBlock(_component_Card, \
         null, {\n    default: _withCtx(() => [\n      _createElementVNode(\"h1\", null, \
         \"Title\")\n    ]),\n    _: 1 /* STABLE */\n  }))\n}"
    );
}

#[test]
fn component_paramless_static_slots_emit_from_s2() {
    let allocator = Allocator::new();
    let source = "const A = () => <Comp>{{ header: () => <h1>Hi</h1>, footer: () => \
                  <p>Bye</p> }}</Comp>;";
    let mut lowered = lower_source(&allocator, allocator.as_oxc(), source, JsxLang::Jsx);
    let analysis: &Croquis = allocator.alloc_owned(lowered.analysis);
    let mut root = lowered.roots.pop().expect("one JSX root");
    let s2 = root.s2.as_ref().expect("paramless slots project to S2");

    assert_eq!(super::root_is_supported(s2), true);

    root.root.children.clear();
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
    assert_eq!(
        component.preamble.as_str(),
        "import { resolveComponent as _resolveComponent, createElementVNode as \
         _createElementVNode, openBlock as _openBlock, createBlock as _createBlock, withCtx as \
         _withCtx } from \"vue\"\n"
    );
    assert_eq!(
        component.code.as_str(),
        "export function render(_ctx, _cache) {\n  const _component_Comp = \
         _resolveComponent(\"Comp\")\n  \n  return (_openBlock(), _createBlock(_component_Comp, \
         null, {\n    header: _withCtx(() => [\n      _createElementVNode(\"h1\", null, \
         \"Hi\")\n    ]),\n    footer: _withCtx(() => [\n      _createElementVNode(\"p\", null, \
         \"Bye\")\n    ]),\n    _: 1 /* STABLE */\n  }))\n}"
    );
}

#[test]
fn component_scoped_slots_still_refuse_s2_projection() {
    let allocator = Allocator::new();
    let source = "const A = () => <List>{{ item: ({ x }) => <li>{x}</li> }}</List>;";
    let mut lowered = lower_source(&allocator, allocator.as_oxc(), source, JsxLang::Jsx);
    let root = lowered.roots.pop().expect("one JSX root");

    assert_eq!(root.s2.err(), Some(crate::s2::S2Refusal::Directive));
}

#[test]
fn leaf_root_component_with_static_props_emits_from_s2() {
    let allocator = Allocator::new();
    let source = "const A = () => <B foo={f} title=\"ok\" />";
    let mut lowered = lower_source(&allocator, allocator.as_oxc(), source, JsxLang::Jsx);
    let analysis: &Croquis = allocator.alloc_owned(lowered.analysis);
    let mut root = lowered.roots.pop().expect("one JSX root");
    let s2 = root.s2.as_ref().expect("leaf component projects to S2");

    assert_eq!(super::root_is_supported(s2), true);

    root.root.children.clear();
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
    assert_eq!(
        component.preamble.as_str(),
        "import { resolveComponent as _resolveComponent, openBlock as _openBlock, \
         createBlock as _createBlock } from \"vue\"\n"
    );
    assert_eq!(
        component.code.as_str(),
        "export function render(_ctx, _cache) {\n  const _component_B = \
         _resolveComponent(\"B\")\n  \n  return (_openBlock(), _createBlock(_component_B, \
         {\n    foo: f,\n    title: \"ok\"\n  }, null, 8 /* PROPS */, [\"foo\"]))\n}"
    );
}

#[test]
fn component_spread_props_emit_from_s2() {
    let allocator = Allocator::new();
    let source = "const A = () => <B {...attrs} foo={f} title=\"ok\" />";
    let mut lowered = lower_source(&allocator, allocator.as_oxc(), source, JsxLang::Jsx);
    let analysis: &Croquis = allocator.alloc_owned(lowered.analysis);
    let mut root = lowered.roots.pop().expect("one JSX root");
    let s2 = root.s2.as_ref().expect("component spread projects to S2");

    assert_eq!(super::root_is_supported(s2), true);

    root.root.children.clear();
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
    assert_eq!(
        component.preamble.as_str(),
        "import { resolveComponent as _resolveComponent, mergeProps as _mergeProps, \
         openBlock as _openBlock, createBlock as _createBlock } from \"vue\"\n"
    );
    assert_eq!(
        component.code.as_str(),
        "export function render(_ctx, _cache) {\n  const _component_B = \
         _resolveComponent(\"B\")\n  \n  return (_openBlock(), _createBlock(_component_B, \
         _mergeProps(attrs, {\n    foo: f,\n    title: \"ok\"\n  }), null, 16 /* FULL_PROPS */, \
         [\"foo\"]))\n}"
    );
}

#[test]
fn dynamic_component_tags_emit_from_s2() {
    let allocator = Allocator::new();
    let source = "const A = () => <Widget.Panel foo={1} />";
    let mut lowered = lower_source(&allocator, allocator.as_oxc(), source, JsxLang::Jsx);
    let analysis: &Croquis = allocator.alloc_owned(lowered.analysis);
    let mut root = lowered.roots.pop().expect("one JSX root");
    let s2 = root.s2.as_ref().expect("dynamic component projects to S2");

    assert_eq!(super::root_is_supported(s2), true);

    root.root.children.clear();
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
    assert_eq!(
        component.preamble.as_str(),
        "import { resolveDynamicComponent as _resolveDynamicComponent, openBlock as _openBlock, \
         createBlock as _createBlock } from \"vue\"\n"
    );
    assert_eq!(
        component.code.as_str(),
        "export function render(_ctx, _cache) {\n  return (_openBlock(), \
         _createBlock(_resolveDynamicComponent(Widget.Panel), { foo: 1 }))\n}"
    );
}

#[test]
fn element_v_show_emits_from_s2() {
    let allocator = Allocator::new();
    let source = "const A = () => <div v-show={visible} />";
    let mut lowered = lower_source(&allocator, allocator.as_oxc(), source, JsxLang::Jsx);
    let analysis: &Croquis = allocator.alloc_owned(lowered.analysis);
    let mut root = lowered.roots.pop().expect("one JSX root");
    let s2 = root.s2.as_ref().expect("v-show projects to S2");

    assert_eq!(super::root_is_supported(s2), true);

    root.root.children.clear();
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
    assert_eq!(
        component.preamble.as_str(),
        "import { withDirectives as _withDirectives, openBlock as _openBlock, \
         createElementBlock as _createElementBlock, vShow as _vShow } from \"vue\"\n"
    );
    assert_eq!(
        component.code.as_str(),
        "export function render(_ctx, _cache) {\n  return _withDirectives((_openBlock(), \
         _createElementBlock(\"div\", null, null, 512 /* NEED_PATCH */)), [\n    \
         [_vShow, visible]\n  ])\n}"
    );
}

#[test]
fn component_v_slots_still_refuses_s2_projection() {
    let allocator = Allocator::new();
    let source = "const A = () => <B v-slots={slots} />";
    let mut lowered = lower_source(&allocator, allocator.as_oxc(), source, JsxLang::Jsx);
    let root = lowered.roots.pop().expect("one JSX root");

    assert_eq!(root.s2.err(), Some(crate::s2::S2Refusal::Directive));
}
