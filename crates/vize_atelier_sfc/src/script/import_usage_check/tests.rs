use super::{
    TemplateUsedIdentifiers, is_used_in_template, resolve_template_read_identifiers,
    resolve_template_used_identifiers,
};
use vize_atelier_core::parser::parse;
use vize_carton::Allocator;

fn analyze_template(source: &str) -> TemplateUsedIdentifiers {
    let allocator = Allocator::new();
    let (root, _) = parse(&allocator, source);
    resolve_template_used_identifiers(&root)
}

fn read_identifiers(source: &str) -> vize_carton::FxHashSet<vize_carton::String> {
    let allocator = Allocator::new();
    let (root, _) = parse(&allocator, source);
    resolve_template_read_identifiers(&root)
}

fn snapshot_identifiers(result: &TemplateUsedIdentifiers) -> (Vec<&str>, Vec<&str>) {
    let mut used_ids: Vec<_> = result.used_ids.iter().map(|id| id.as_str()).collect();
    used_ids.sort_unstable();

    let mut v_model_ids: Vec<_> = result.v_model_ids.iter().map(|id| id.as_str()).collect();
    v_model_ids.sort_unstable();

    (used_ids, v_model_ids)
}

fn sorted_read_identifiers(identifiers: &vize_carton::FxHashSet<vize_carton::String>) -> Vec<&str> {
    let mut identifiers: Vec<_> = identifiers.iter().map(|id| id.as_str()).collect();
    identifiers.sort_unstable();
    identifiers
}

fn assert_identifiers_snapshot(name: &str, result: &TemplateUsedIdentifiers) {
    insta::with_settings!({ snapshot_path => "../snapshots" }, {
        insta::assert_debug_snapshot!(name, snapshot_identifiers(result));
    });
}

#[test]
fn test_component_usage() {
    let result = analyze_template("<MyComponent />");
    assert_identifiers_snapshot("component_usage", &result);
}

#[test]
fn test_component_usage_kebab() {
    let result = analyze_template("<my-component />");
    assert_identifiers_snapshot("component_usage_kebab", &result);
}

#[test]
fn test_component_with_dot() {
    let result = analyze_template("<Foo.Bar />");
    assert_identifiers_snapshot("component_with_dot", &result);
}

#[test]
fn test_interpolation() {
    let result = analyze_template("<div>{{ msg }}</div>");
    assert_identifiers_snapshot("interpolation", &result);
}

#[test]
fn test_v_bind() {
    let result = analyze_template("<div :class=\"classes\"></div>");
    assert_identifiers_snapshot("v_bind", &result);
}

#[test]
fn test_v_on() {
    let result = analyze_template("<div @click=\"handleClick\"></div>");
    assert_identifiers_snapshot("v_on", &result);
}

#[test]
fn test_v_model() {
    let result = analyze_template("<input v-model=\"value\" />");
    assert_identifiers_snapshot("v_model", &result);
}

#[test]
fn test_v_model_complex() {
    // Complex expressions should not be added to v_model_ids.
    let result = analyze_template("<input v-model=\"obj.value\" />");
    assert_identifiers_snapshot("v_model_complex", &result);
}

#[test]
fn test_v_for() {
    let result = analyze_template("<div v-for=\"item in items\">{{ item }}</div>");
    assert_identifiers_snapshot("v_for", &result);
}

#[test]
fn test_v_if() {
    let result = analyze_template("<div v-if=\"show\">content</div>");
    assert_identifiers_snapshot("v_if", &result);
}

#[test]
fn test_custom_directive() {
    let result = analyze_template("<div v-focus></div>");
    assert_identifiers_snapshot("custom_directive", &result);
}

#[test]
fn test_ref_attribute() {
    let result = analyze_template("<div ref=\"myRef\"></div>");
    let read_ids = read_identifiers("<div ref=\"myRef\"></div>");
    assert_eq!(sorted_read_identifiers(&read_ids), Vec::<&str>::new());
    assert_identifiers_snapshot("ref_attribute", &result);
}

#[test]
fn test_ref_attribute_with_expression_read() {
    let result = analyze_template("<div ref=\"myRef\">{{ myRef }}</div>");
    let read_ids = read_identifiers("<div ref=\"myRef\">{{ myRef }}</div>");
    assert_eq!(sorted_read_identifiers(&read_ids), vec!["myRef"]);
    assert_identifiers_snapshot("ref_attribute_with_expression_read", &result);
}

#[test]
fn test_native_tag_not_added() {
    let result = analyze_template("<div></div>");
    assert_identifiers_snapshot("native_tag_not_added", &result);
}

#[test]
fn test_builtin_directive_not_added() {
    let result = analyze_template("<div v-if=\"show\" v-show=\"visible\"></div>");
    assert_identifiers_snapshot("builtin_directive_not_added", &result);
}

#[test]
fn test_is_used_in_template() {
    let allocator = Allocator::new();
    let (root, _) = parse(&allocator, "<div>{{ msg }}</div>");
    assert_eq!(is_used_in_template("msg", &root), true);
    assert_eq!(is_used_in_template("other", &root), false);
}
