use super::{AttributeHyphenation, HyphenationStyle};
use crate::linter::Linter;
use crate::rule::RuleRegistry;

fn create_linter() -> Linter {
    let mut registry = RuleRegistry::new();
    registry.register(Box::new(AttributeHyphenation::default()));
    Linter::with_registry(registry)
}

fn warning_count(template: &str) -> usize {
    create_linter()
        .lint_template(template, "test.vue")
        .warning_count
}

fn never_warning_count(template: &str) -> usize {
    let mut registry = RuleRegistry::new();
    registry.register(Box::new(AttributeHyphenation::new(HyphenationStyle::Never)));
    Linter::with_registry(registry)
        .lint_template(template, "test.vue")
        .warning_count
}

#[test]
fn test_valid_hyphenated() {
    assert_eq!(warning_count(r#"<MyComponent my-prop="value" />"#), 0);
}

#[test]
fn test_invalid_camel_case() {
    assert_eq!(warning_count(r#"<MyComponent myProp="value" />"#), 1);
}

#[test]
fn test_invalid_uppercase_initial_and_lowercase_custom_component() {
    assert_eq!(
        warning_count(r#"<div><MyComponent DOMId="value" /><draggable itemKey="id" /></div>"#,),
        2,
    );
}

#[test]
fn test_invalid_bound_camel_case_argument() {
    assert_eq!(warning_count(r#"<MyComponent :activeKey="value" />"#), 1);
}

#[test]
fn test_invalid_model_camel_case_argument() {
    assert_eq!(
        warning_count(r#"<MyComponent v-model:activeKey.trim="value" />"#,),
        1,
    );
}

#[test]
fn test_invalid_on_prefixed_prop() {
    assert_eq!(warning_count(r#"<MyComponent :onUndo="undo" />"#), 1);
}

#[test]
fn test_valid_event_argument() {
    assert_eq!(warning_count(r#"<MyComponent @onUndo="undo" />"#), 0);
}

#[test]
fn test_valid_dynamic_argument() {
    assert_eq!(warning_count(r#"<MyComponent :[myProp]="value" />"#), 0);
}

#[test]
fn test_valid_dynamic_model_argument() {
    assert_eq!(
        warning_count(r#"<MyComponent v-model:[activeKey]="value" />"#),
        0
    );
}

#[test]
fn test_valid_svg_weird_case_attributes() {
    assert_eq!(
        warning_count(
            r#"<MyIcon viewBox="0 0 16 16" :preserveAspectRatio="ratio" customCamel="x" />"#,
        ),
        1,
    );
}

#[test]
fn test_invalid_customized_builtin() {
    assert_eq!(
        warning_count(r#"<div is="vue:MyRow" :rowData="row" /><div :is="MyRow" rowData />"#),
        2,
    );
}

#[test]
fn test_valid_html_element() {
    assert_eq!(warning_count(r#"<div onClick="handler"></div>"#), 0);
}

#[test]
fn test_valid_data_attribute() {
    assert_eq!(warning_count(r#"<MyComponent data-testId="123" />"#), 0);
}

#[test]
fn test_never_invalid_hyphenated_attribute() {
    assert_eq!(never_warning_count(r#"<MyComponent my-prop="value" />"#), 1);
    assert_eq!(
        never_warning_count(r#"<MyComponent :active-key="value" />"#),
        1
    );
}

#[test]
fn test_never_valid_camel_case_attribute_and_native_elements() {
    assert_eq!(
        never_warning_count(r#"<MyComponent myProp="value" /><div data-test="value" />"#),
        0
    );
}
