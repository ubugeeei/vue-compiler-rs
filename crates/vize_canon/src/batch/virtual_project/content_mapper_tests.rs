use std::path::Path;

use super::ContentMapperSpanKind;
use super::span_features::{
    CONTENT_MAPPER_SPAN_FEATURE_BITS, CONTENT_MAPPER_SPAN_FEATURES_ALL,
    CONTENT_MAPPER_SPAN_FEATURES_ATOM, CONTENT_MAPPER_SPAN_FEATURES_COMPLETION,
    CONTENT_MAPPER_SPAN_FEATURES_NAVIGATION_TARGET, CONTENT_MAPPER_SPAN_FEATURES_WHOLE_SYMBOL,
    content_mapper_span_features,
};
use crate::batch::{
    CONTENT_MAPPER_GENERATED_DIAGNOSTIC_CODE, CONTENT_MAPPER_VIRTUAL_EXTENSION,
    ContentMapperTransformOptions, generate_vue_content_mapper_transform,
    generate_vue_content_mapper_transform_with_options,
};

#[path = "content_mapper_component_export_tests.rs"]
mod component_exports;
#[path = "content_mapper_model_tests.rs"]
mod models;
#[path = "content_mapper_navigation_tests.rs"]
mod navigation;
#[path = "content_mapper_protocol_tests.rs"]
mod protocol;
#[path = "content_mapper_scoped_event_navigation_tests.rs"]
mod scoped_event_navigation;
#[path = "content_mapper_slot_outlet_navigation_tests.rs"]
mod slot_outlet_navigation;

#[test]
fn keeps_mapper_offsets_in_utf8_bytes() {
    let source = r#"<script setup lang="ts">
const emoji = "😀"
</script>
<template>{{ emoji }}</template>
"#;
    let result =
        generate_vue_content_mapper_transform(Path::new("Unicode.vue"), source).expect("transform");
    let original = source.rfind("emoji").expect("template identifier");
    assert!(
        result
            .mappings
            .iter()
            .any(|mapping| mapping.0[2] == original),
        "expected a UTF-8 byte mapping at {original}: {:?}",
        result.mappings
    );
}

#[test]
fn emits_stable_semantic_links_for_ref_unwraps() {
    let source = r#"<script setup lang="ts">
import { ref } from 'vue'
const café = ref(1)
</script>
<template>{{ café }}</template>
"#;
    let result = generate_vue_content_mapper_transform(Path::new("UnicodeRef.vue"), source)
        .expect("transform");
    let link = result
        .semantic_links
        .iter()
        .find(|link| {
            &result.text.as_str()[link.source_start..link.source_start + link.source_length]
                == "café"
                && &result.text.as_str()[link.target_start..link.target_start + link.target_length]
                    == "café"
        })
        .unwrap_or_else(|| {
            panic!(
                "expected a semantic link for café ref unwrap:\ntext:\n{}\nlinks:\n{:#?}",
                result.text, result.semantic_links
            )
        });

    assert_eq!(link.kind, "vueSetupTemplateRefUnwrap");
}

#[test]
fn keeps_tsx_semantic_link_ranges_aligned_after_jsx_reference_prefix() {
    let source = r#"<script setup lang="tsx">
import { ref } from 'vue'
const café = ref(<span />)
</script>
<template>{{ café }}</template>
"#;
    let result = generate_vue_content_mapper_transform(Path::new("UnicodeRef.vue"), source)
        .expect("transform");
    let source_starts = result
        .semantic_links
        .iter()
        .filter(|link| {
            &result.text.as_str()[link.source_start..link.source_start + link.source_length]
                == "café"
                && &result.text.as_str()[link.target_start..link.target_start + link.target_length]
                    == "café"
        })
        .count();

    assert!(
        source_starts > 0,
        "expected both semantic-link endpoints to point at generated café ranges:\ntext:\n{}\nlinks:\n{:#?}",
        result.text,
        result.semantic_links
    );
}

#[test]
fn maps_synthetic_prop_bindings_to_the_authored_declaration() {
    let source = r#"<script setup lang="ts">
defineProps<{ count: number }>();
</script>
<template>{{ count.toFixed(0) }}</template>
"#;
    let result =
        generate_vue_content_mapper_transform(Path::new("Props.vue"), source).expect("transform");
    let original = source.find("count: number").unwrap();
    let matching = result
        .mappings
        .iter()
        .filter(|mapping| mapping.0[2] == original && mapping.0[3] == "count".len())
        .collect::<Vec<_>>();
    let exported = result.text.find("export type Props").unwrap();
    let exported = exported + result.text[exported..].find("count").unwrap();

    assert!(
        matching.len() >= 3,
        "expected exported, authored, and synthetic projections: {matching:?}"
    );
    assert!(matching.iter().any(|mapping| mapping.0[0] == exported));
    assert!(matching.iter().all(|mapping| mapping.0[4] == 0));
}

#[test]
fn maps_synthetic_props_after_a_plain_script_to_the_setup_block() {
    let source = r#"<script lang="ts">
export const marker = true;
</script>
<script setup lang="ts">
defineProps<{ count: number }>();
</script>
<template>{{ count.toFixed(0) }}</template>
"#;
    let result = generate_vue_content_mapper_transform(Path::new("SplitProps.vue"), source)
        .expect("transform");
    let original = source.find("count: number").unwrap();

    assert!(
        result
            .mappings
            .iter()
            .filter(|mapping| {
                mapping.0[2] == original && mapping.0[3] == "count".len() && mapping.0[4] == 0
            })
            .count()
            >= 2
    );
}

#[test]
fn split_script_setup_spans_start_at_the_authored_block() {
    let source = r#"<script lang="ts">
export type SearchQuery = { value: string };
</script>

<script setup lang="ts">
const values: any = [];
values.map(it => it);
</script>
"#;
    let result =
        generate_vue_content_mapper_transform(Path::new("Split.vue"), source).expect("transform");
    let generated = result.text.find("it => it").expect("generated parameter");
    let original = source.find("it => it").expect("authored parameter");
    let span = result
        .mappings
        .iter()
        .map(|mapping| mapping.0)
        .find(|span| generated >= span[0] && generated < span[0] + span[1])
        .expect("parameter mapping");

    assert_eq!(span[4], 0, "setup line should remain verbatim: {span:?}");
    assert_eq!(span[2] + generated - span[0], original);
}

#[test]
fn authored_parse_errors_are_mapper_diagnostics() {
    let source = "<template><div></template>";
    let result =
        generate_vue_content_mapper_transform(Path::new("Broken.vue"), source).expect("transform");

    assert!(!result.diagnostics.is_empty());
    assert!(result.mappings.is_empty());
    assert!(result.text.contains("__vize_component"));
    assert!(result.diagnostics[0].start <= source.len());
    assert_eq!(
        result.diagnostics[0].code,
        CONTENT_MAPPER_GENERATED_DIAGNOSTIC_CODE
    );
}

/// Opening line of the generated template scope.
const TEMPLATE_SCOPE: &str = "  ;(function __template() {\n";

/// The first four lines the template scope emits for a Vue 3 component that has
/// a setup binding to shadow, when the shared preamble is not hoisted.
const TEMPLATE_REF_UNWRAP_PRELUDE: &str = r#"    // Auto-unwrap Vue refs in template scope
    type __VizeIsUnion<T, __U = T> = T extends unknown ? ([__U] extends [T] ? false : true) : false;
    type __VizeWidenTemplateRef<T> = __VizeIsAny<T> extends true ? T : __VizeIsUnion<T> extends true ? T : T extends string ? string extends T ? string : T : T extends number ? number extends T ? number : T : T extends boolean ? boolean extends T ? boolean : T : T;
    type __U<T> = T extends import('vue').Ref ? __VizeWidenTemplateRef<T['value']> : T;
"#;

fn template_scope_of(text: &str) -> &str {
    text.split_once(TEMPLATE_SCOPE)
        .expect("generated module must open a template scope")
        .1
}

/// This path does not hoist the shared preamble, so every helper it emits is a
/// module-local declaration. The widening conditional types must therefore stay
/// with the `__U` that is their only reference, inside `__template()`: a
/// component with no setup binding in template scope emits no `__U` at all, and
/// a module-scope copy would then be unused — which TypeScript reports as a
/// TS6196 hint on the user's own `.vue` file (#3510).
#[test]
fn widening_helpers_are_declared_with_the_template_scope_that_uses_them() {
    let source = r#"<script setup lang="ts">
import { ref } from 'vue'
const message = ref('hello')
</script>
<template>{{ message }}</template>
"#;
    let result =
        generate_vue_content_mapper_transform(Path::new("Unwrap.vue"), source).expect("transform");

    let prelude = template_scope_of(&result.text)
        .split_inclusive('\n')
        .take(4)
        .collect::<std::string::String>();
    assert_eq!(prelude, TEMPLATE_REF_UNWRAP_PRELUDE);
    for declaration in ["type __VizeIsUnion<", "type __VizeWidenTemplateRef<"] {
        assert_eq!(
            result.text.matches(declaration).count(),
            1,
            "{declaration} must be declared once, in template scope:\n{}",
            result.text
        );
    }
}

/// The other half of the same invariant: nothing to unwrap, nothing declared.
#[test]
fn a_component_without_setup_bindings_declares_no_widening_helpers() {
    let source = "<template><p>static</p></template>\n";
    let result =
        generate_vue_content_mapper_transform(Path::new("Static.vue"), source).expect("transform");

    for declaration in [
        "type __U<",
        "type __VizeIsUnion<",
        "type __VizeWidenTemplateRef<",
    ] {
        assert_eq!(
            result.text.matches(declaration).count(),
            0,
            "{declaration} would be an unused module-local declaration:\n{}",
            result.text
        );
    }
}

#[test]
fn jsx_scripts_preserve_vue_jsx_type_context() {
    let source = "<script lang=\"tsx\">export default () => <div /></script>";
    let result =
        generate_vue_content_mapper_transform(Path::new("Jsx.vue"), source).expect("transform");

    assert!(
        result
            .text
            .starts_with("/// <reference types=\"vue/jsx\" />")
    );
}

#[test]
fn options_api_transform_setting_controls_instance_bindings() {
    let source = r#"<script lang="ts">
export default {
  data() { return { count: 1 } }
}
</script>
<template>{{ count }}</template>
"#;

    let enabled = generate_vue_content_mapper_transform_with_options(
        Path::new("Options.vue"),
        source,
        ContentMapperTransformOptions::default().with_options_api(true),
    )
    .expect("enabled transform");
    let disabled = generate_vue_content_mapper_transform_with_options(
        Path::new("Options.vue"),
        source,
        ContentMapperTransformOptions::default().with_options_api(false),
    )
    .expect("disabled transform");

    assert!(
        enabled
            .text
            .contains("var count: __VizeOptionsBinding<typeof __default__, \"count\">")
    );
    assert!(!disabled.text.contains("__VizeOptionsBinding"));
}

#[test]
fn unused_diagnostic_setting_only_anchors_template_references() {
    let source = r#"<script setup lang="ts">
const used = 1
const unused = 2
</script>
<template>{{ used }}</template>
"#;

    let result = generate_vue_content_mapper_transform_with_options(
        Path::new("Unused.vue"),
        source,
        ContentMapperTransformOptions::default().with_preserve_unused_diagnostics(true),
    )
    .expect("transform");

    assert!(result.text.contains("void used;"), "{}", result.text);
    assert!(!result.text.contains("void unused;"), "{}", result.text);
}

#[test]
fn default_transform_matches_vize_options_api_default() {
    let source = r#"<script lang="ts">
export default { data() { return { count: 1 } } }
</script>
<template>{{ count }}</template>
"#;

    let result = generate_vue_content_mapper_transform(Path::new("DefaultOptions.vue"), source)
        .expect("transform");

    assert!(result.text.contains("__VizeOptionsBinding"));
}
