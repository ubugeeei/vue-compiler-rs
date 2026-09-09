//! The synthesized `update:` emit payload for `defineModel` (#3904): an
//! optional model without a default carries `T | undefined` — its `ModelRef`
//! type, and what vue-tsc's synthesized listener accepts — while required
//! models and models with defaults keep the bare payload.

use crate::virtual_ts::generate_virtual_ts;

fn emits_of(script: &str) -> std::string::String {
    let allocator = vize_carton::Allocator::new();
    let (root, _) = vize_armature::parse(&allocator, "<div>{{ model }}</div>");
    let mut analyzer = vize_croquis::Analyzer::with_options(vize_croquis::AnalyzerOptions::full());
    analyzer.analyze_script_setup(script);
    analyzer.analyze_template(&root);
    let summary = analyzer.finish();
    let output = generate_virtual_ts(&summary, Some(script), Some(&root), 0);
    let code = output.code.as_str();
    let start = code.find("export type Emits").expect("Emits alias");
    let end = code[start..].find(";\n").map_or(code.len(), |e| start + e);
    code[start..end].into()
}

fn code_of(script: &str) -> std::string::String {
    let allocator = vize_carton::Allocator::new();
    let (root, _) = vize_armature::parse(&allocator, "<div>{{ model }}</div>");
    let mut analyzer = vize_croquis::Analyzer::with_options(vize_croquis::AnalyzerOptions::full());
    analyzer.analyze_script_setup(script);
    analyzer.analyze_template(&root);
    let summary = analyzer.finish();
    generate_virtual_ts(&summary, Some(script), Some(&root), 0)
        .code
        .into()
}

#[test]
fn an_optional_model_update_payload_carries_undefined() {
    let emits = emits_of("const model = defineModel<string>()\nvoid model;\n");
    assert!(
        emits.contains("\"update:modelValue\": [value: (string) | undefined]"),
        "optional model must accept undefined in its update payload:\n{emits}"
    );
}

#[test]
fn required_and_defaulted_models_keep_the_bare_payload() {
    let required = emits_of("const model = defineModel<string>({ required: true })\nvoid model;\n");
    assert!(
        required.contains("\"update:modelValue\": [value: string]"),
        "required model keeps the bare payload:\n{required}"
    );
    let defaulted =
        emits_of("const model = defineModel<string>({ default: \"x\" })\nvoid model;\n");
    assert!(
        defaulted.contains("\"update:modelValue\": [value: string]"),
        "defaulted model keeps the bare payload:\n{defaulted}"
    );
}

#[test]
fn an_untyped_model_keeps_the_bare_unknown_payload() {
    let emits = emits_of("const model = defineModel()\nvoid model;\n");
    assert!(
        emits.contains("\"update:modelValue\": [value: unknown]"),
        "an untyped model keeps the bare `unknown` payload:\n{emits}"
    );
}

#[test]
fn runtime_constructor_model_flows_into_public_props_and_emits() {
    let code = code_of("const model = defineModel({ type: String, default: '' })\nvoid model;\n");

    assert!(
        code.contains("\"modelValue\"?: string;"),
        "runtime constructor model should expose a string public prop:\n{code}"
    );
    assert!(
        code.contains("\"update:modelValue\": [value: string]"),
        "runtime constructor model should expose a string update payload:\n{code}"
    );
}

#[test]
fn optional_runtime_constructor_model_update_payload_carries_undefined() {
    let emits = emits_of("const model = defineModel({ type: String })\nvoid model;\n");
    assert!(
        emits.contains("\"update:modelValue\": [value: (string) | undefined]"),
        "optional runtime constructor model should accept undefined:\n{emits}"
    );
}

#[test]
fn a_function_typed_model_parenthesizes_the_base() {
    let emits = emits_of("const model = defineModel<() => string>()\nvoid model;\n");
    assert!(
        emits.contains("\"update:modelValue\": [value: (() => string) | undefined]"),
        "a function-typed model must not absorb the union into its return type:\n{emits}"
    );
}

#[test]
fn a_typed_define_emits_intersection_carries_the_same_payload() {
    let optional = emits_of(
        "const emit = defineEmits<{ change: [] }>()\nconst model = defineModel<string>()\nvoid emit;\nvoid model;\n",
    );
    assert!(
        optional.contains("\"update:modelValue\": [value: (string) | undefined]"),
        "the typed `defineEmits` intersection accepts undefined for an optional model:\n{optional}"
    );
    let required = emits_of(
        "const emit = defineEmits<{ change: [] }>()\nconst model = defineModel<string>({ required: true })\nvoid emit;\nvoid model;\n",
    );
    assert!(
        required.contains("\"update:modelValue\": [value: string]"),
        "the typed `defineEmits` intersection keeps the bare payload for a required model:\n{required}"
    );
}

#[test]
fn runtime_emits_carry_the_same_payload() {
    let optional = emits_of(
        "const emit = defineEmits([\"change\"])\nconst model = defineModel<string>()\nvoid emit;\nvoid model;\n",
    );
    assert!(
        optional.contains("(event: \"update:modelValue\", value: (string) | undefined) => void"),
        "runtime emits accept undefined for an optional model:\n{optional}"
    );
    let required = emits_of(
        "const emit = defineEmits([\"change\"])\nconst model = defineModel<string>({ required: true })\nvoid emit;\nvoid model;\n",
    );
    assert!(
        required.contains("(event: \"update:modelValue\", value: string) => void"),
        "runtime emits keep the bare payload for a required model:\n{required}"
    );
}
