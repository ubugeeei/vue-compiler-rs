use vize_croquis::Croquis;
use vize_s0::Allocator;

use crate::{JsxLang, lower_source};

use super::super::{VdomCompatOptions, VdomCompileOptions, compile_root_to_vdom};

#[test]
fn plain_input_v_model_emits_from_s2() {
    let allocator = Allocator::new();
    let source = "const A = () => <input v-model={value} />;";
    let mut lowered = lower_source(&allocator, allocator.as_oxc(), source, JsxLang::Jsx);
    let analysis: &Croquis = allocator.alloc_owned(lowered.analysis);
    let mut root = lowered.roots.pop().expect("one JSX root");
    let s2 = root.s2.as_ref().expect("input v-model projects to S2");

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
