use super::super::generate_virtual_ts_with_offsets_options_api;
use crate::virtual_ts::generate_virtual_ts_with_offsets;
use vize_croquis::{Analyzer, AnalyzerOptions};

#[test]
fn imported_props_type_is_not_redeclared_when_models_extend_it() {
    let script = r#"import type { Props } from "./types";

defineProps<Props>();
defineModel<string>();
"#;

    let mut analyzer = Analyzer::with_options(AnalyzerOptions::full());
    analyzer.analyze_script_setup(script);
    let summary = analyzer.finish();

    let output =
        generate_virtual_ts_with_offsets(&summary, Some(script), None, 0, 0, &Default::default());

    assert!(
        !output.code.contains("export type Props = Props &"),
        "imported Props must not be redeclared by generated model props:\n{}",
        output.code
    );
    assert!(
        output
            .code
            .contains("type __VizeResolvedProps = Props & {\n"),
        "model props should extend the imported Props through a private alias:\n{}",
        output.code
    );
    assert!(
        output
            .code
            .contains("  $props: __VizeResolvedProps & __EmitProps<Emits>;")
            && output
                .code
                .contains("readonly __vizeRawProps?: __VizeResolvedProps;"),
        "component instance should use the private resolved props alias:\n{}",
        output.code
    );
}

#[test]
fn imported_props_name_does_not_replace_a_different_define_props_type() {
    let script = r#"import type { Props } from "./types";
type Other = { count: number };

defineProps<Other>();
defineModel<string>();
"#;

    let mut analyzer = Analyzer::with_options(AnalyzerOptions::full());
    analyzer.analyze_script_setup(script);
    let summary = analyzer.finish();

    let output =
        generate_virtual_ts_with_offsets(&summary, Some(script), None, 0, 0, &Default::default());

    assert!(
        output
            .code
            .contains("type __VizeResolvedProps = Other & {\n"),
        "the authored defineProps type should win over an unrelated imported Props name:\n{}",
        output.code
    );
    assert!(
        output
            .code
            .contains("  $props: __VizeResolvedProps & __EmitProps<Emits>;"),
        "component instance should use the private resolved props alias:\n{}",
        output.code
    );
}

#[test]
fn imported_generic_props_type_is_not_degraded_to_a_bare_props_name() {
    let script = r#"import type { Props } from "./types";
type Item = { id: string };

defineProps<Props<Item>>();
defineModel<string>();
"#;

    let mut analyzer = Analyzer::with_options(AnalyzerOptions::full());
    analyzer.analyze_script_setup(script);
    let summary = analyzer.finish();

    let output =
        generate_virtual_ts_with_offsets(&summary, Some(script), None, 0, 0, &Default::default());

    assert!(
        output
            .code
            .contains("type __VizeResolvedProps = Props<Item> & {\n"),
        "the imported Props instantiation should be preserved:\n{}",
        output.code
    );
}

fn options_api_code_with_component_usage(script: &str) -> std::string::String {
    let allocator = vize_carton::Allocator::new();
    let (root, _) = vize_armature::parse(&allocator, "<Widget />");
    let mut analyzer = Analyzer::with_options(AnalyzerOptions::full()).with_options_api();
    analyzer.analyze_script_plain(script);
    analyzer.analyze_template(&root);
    let summary = analyzer.finish();

    generate_virtual_ts_with_offsets_options_api(
        &summary,
        Some(script),
        Some(&root),
        0,
        0,
        &Default::default(),
    )
    .code
    .into()
}

#[test]
fn imported_props_type_does_not_suppress_options_api_array_props() {
    let script = r#"import type { Props } from "./types";
import Widget from "./Widget.vue";

export default {
  components: { Widget },
  props: ["initial", "label"],
};
"#;
    let code = options_api_code_with_component_usage(script);

    assert!(
        !code.contains("export type Props = {"),
        "imported Props must not be redeclared:\n{code}"
    );
    assert!(
        code.contains("type __VizeResolvedProps = {\n  \"initial\"?: unknown;\n"),
        "Options API array props should be preserved through a private alias:\n{code}"
    );
    assert!(
        code.contains("  $props: __VizeResolvedProps;"),
        "component instance should read the private Options API props alias:\n{code}"
    );
}

#[test]
fn imported_props_type_keeps_deferred_options_api_props() {
    let script = r#"import type { Props } from "./types";
import Widget from "./Widget.vue";

const sharedProps = {
  meta: { type: Object, required: true as const },
};
export default {
  components: { Widget },
  props: { ...sharedProps },
};
"#;
    let code = options_api_code_with_component_usage(script);

    assert!(
        code.contains("const __vize_options_props = ({ ...sharedProps } as const);"),
        "deferred Options API props should still be captured in setup scope:\n{code}"
    );
    assert!(
        code.contains(
            "type __VizeResolvedProps = __VizeOptionsPropShape<Awaited<ReturnType<typeof __setup>>[\"__vize_options_props\"]>;"
        ),
        "deferred Options API props should resolve through a private alias:\n{code}"
    );
    assert!(
        code.contains("  $props: __VizeResolvedProps;"),
        "component instance should use the private deferred props alias:\n{code}"
    );
}
