use vize_croquis::Croquis;
use vize_s0::Allocator;

use crate::{JsxLang, lower_source};

use super::super::{VdomCompatOptions, VdomCompileOptions, VdomComponent, compile_root_to_vdom};

#[test]
fn plain_input_v_model_emits_from_s2() {
    let source = "const A = () => <input v-model={value} />;";
    let component = compile_native_model_from_s2(source);
    assert_eq!(
        component.preamble.as_str(),
        "import { vModelText as _vModelText, withDirectives as _withDirectives, \
         openBlock as _openBlock, createElementBlock as _createElementBlock } from \"vue\"\n"
    );
    assert_eq!(
        component.code.as_str(),
        "export function render(_ctx, _cache) {\n  return _withDirectives((_openBlock(), \
         _createElementBlock(\"input\", {\n    \"onUpdate:modelValue\": $event => ((value) = \
         $event)\n  }, null, 8 /* PROPS */, [\"onUpdate:modelValue\"])), [\n    \
         [_vModelText, value]\n  ])\n}"
    );
}

#[test]
fn text_input_v_model_emits_from_s2() {
    let source = "const A = () => <input type=\"text\" v-model={value} />;";
    let component = compile_native_model_from_s2(source);
    assert_eq!(
        component.preamble.as_str(),
        "import { vModelText as _vModelText, withDirectives as _withDirectives, \
         openBlock as _openBlock, createElementBlock as _createElementBlock } from \"vue\"\n"
    );
    assert_eq!(
        component.code.as_str(),
        "export function render(_ctx, _cache) {\n  return _withDirectives((_openBlock(), \
         _createElementBlock(\"input\", {\n    type: \"text\",\n    \"onUpdate:modelValue\": \
         $event => ((value) = $event)\n  }, null, 8 /* PROPS */, \
         [\"onUpdate:modelValue\"])), [\n    [_vModelText, value]\n  ])\n}"
    );
}

#[test]
fn input_v_model_modifiers_emit_from_s2() {
    let source = "const A = () => <input v-model_number_lazy={value} />;";
    let component = compile_native_model_from_s2(source);
    assert_eq!(
        component.preamble.as_str(),
        "import { vModelText as _vModelText, withDirectives as _withDirectives, \
         openBlock as _openBlock, createElementBlock as _createElementBlock } from \"vue\"\n"
    );
    assert_eq!(
        component.code.as_str(),
        "export function render(_ctx, _cache) {\n  return _withDirectives((_openBlock(), \
         _createElementBlock(\"input\", {\n    \"onUpdate:modelValue\": $event => ((value) = \
         $event)\n  }, null, 8 /* PROPS */, [\"onUpdate:modelValue\"])), [\n    [\n      \
         _vModelText,\n      value,\n      void 0,\n      {\n        number: true,\n        lazy: \
         true\n      }\n    ]\n  ])\n}"
    );
}

#[test]
fn checkbox_input_v_model_emits_from_s2() {
    let source = "const A = () => <input type=\"checkbox\" v-model={checked} />;";
    let component = compile_native_model_from_s2(source);
    assert_eq!(
        component.preamble.as_str(),
        "import { vModelCheckbox as _vModelCheckbox, withDirectives as _withDirectives, \
         openBlock as _openBlock, createElementBlock as _createElementBlock } from \"vue\"\n"
    );
    assert_eq!(
        component.code.as_str(),
        "export function render(_ctx, _cache) {\n  return _withDirectives((_openBlock(), \
         _createElementBlock(\"input\", {\n    type: \"checkbox\",\n    \
         \"onUpdate:modelValue\": $event => ((checked) = $event)\n  }, null, 8 /* PROPS */, \
         [\"onUpdate:modelValue\"])), [\n    [_vModelCheckbox, checked]\n  ])\n}"
    );
}

#[test]
fn radio_input_v_model_emits_from_s2() {
    let source = "const A = () => <input type=\"radio\" v-model={picked} />;";
    let component = compile_native_model_from_s2(source);
    assert_eq!(
        component.preamble.as_str(),
        "import { vModelRadio as _vModelRadio, withDirectives as _withDirectives, \
         openBlock as _openBlock, createElementBlock as _createElementBlock } from \"vue\"\n"
    );
    assert_eq!(
        component.code.as_str(),
        "export function render(_ctx, _cache) {\n  return _withDirectives((_openBlock(), \
         _createElementBlock(\"input\", {\n    type: \"radio\",\n    \"onUpdate:modelValue\": \
         $event => ((picked) = $event)\n  }, null, 8 /* PROPS */, \
         [\"onUpdate:modelValue\"])), [\n    [_vModelRadio, picked]\n  ])\n}"
    );
}

#[test]
fn textarea_v_model_emits_from_s2() {
    let source = "const A = () => <textarea v-model={value} />;";
    let component = compile_native_model_from_s2(source);
    assert_eq!(
        component.preamble.as_str(),
        "import { vModelText as _vModelText, withDirectives as _withDirectives, \
         openBlock as _openBlock, createElementBlock as _createElementBlock } from \"vue\"\n"
    );
    assert_eq!(
        component.code.as_str(),
        "export function render(_ctx, _cache) {\n  return _withDirectives((_openBlock(), \
         _createElementBlock(\"textarea\", {\n    \"onUpdate:modelValue\": $event => ((value) = \
         $event)\n  }, null, 8 /* PROPS */, [\"onUpdate:modelValue\"])), [\n    \
         [_vModelText, value]\n  ])\n}"
    );
}

#[test]
fn select_v_model_emits_from_s2() {
    let source = "const A = () => <select v-model={value}></select>;";
    let component = compile_native_model_from_s2(source);
    assert_eq!(
        component.preamble.as_str(),
        "import { vModelSelect as _vModelSelect, withDirectives as _withDirectives, \
         openBlock as _openBlock, createElementBlock as _createElementBlock } from \"vue\"\n"
    );
    assert_eq!(
        component.code.as_str(),
        "export function render(_ctx, _cache) {\n  return _withDirectives((_openBlock(), \
         _createElementBlock(\"select\", {\n    \"onUpdate:modelValue\": $event => ((value) = \
         $event)\n  }, null, 8 /* PROPS */, [\"onUpdate:modelValue\"])), [\n    \
         [_vModelSelect, value]\n  ])\n}"
    );
}

fn compile_native_model_from_s2(source: &str) -> VdomComponent {
    let allocator = Allocator::new();
    let mut lowered = lower_source(&allocator, allocator.as_oxc(), source, JsxLang::Jsx);
    let analysis: &Croquis = allocator.alloc_owned(lowered.analysis);
    let mut root = lowered.roots.pop().expect("one JSX root");
    let s2 = root.s2.as_ref().expect("native v-model projects to S2");

    assert!(super::root_is_supported(s2));

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
    component
}
