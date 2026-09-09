use super::{
    HtmlSelfClosing, HtmlSelfClosingHtmlOptions, HtmlSelfClosingNuxt, HtmlSelfClosingOptions,
    HtmlSelfClosingStyle,
};
use crate::linter::Linter;
use crate::rule::RuleRegistry;

fn create_linter() -> Linter {
    let mut registry = RuleRegistry::new();
    registry.register(Box::new(HtmlSelfClosing::default()));
    Linter::with_registry(registry)
}

fn create_configured_linter(options: HtmlSelfClosingOptions) -> Linter {
    let mut registry = RuleRegistry::new();
    registry.register(Box::new(HtmlSelfClosing::new(options)));
    Linter::with_registry(registry)
}

#[test]
fn test_valid_self_closing_void() {
    let linter = create_linter();
    let result = linter.lint_template(r#"<img />"#, "test.vue");
    assert_eq!(result.warning_count, 0);
}

#[test]
fn test_invalid_void_not_self_closing() {
    let linter = create_linter();
    let result = linter.lint_template(r#"<img>"#, "test.vue");
    assert_eq!(result.warning_count, 1);
}

#[test]
fn test_valid_component_self_closing() {
    let linter = create_linter();
    let result = linter.lint_template(r#"<MyComponent />"#, "test.vue");
    assert_eq!(result.warning_count, 0);
}

#[test]
fn test_invalid_empty_component() {
    let linter = create_linter();
    let result = linter.lint_template(r#"<MyComponent></MyComponent>"#, "test.vue");
    assert_eq!(result.warning_count, 1);
}

#[test]
fn test_valid_component_with_content() {
    let linter = create_linter();
    let result = linter.lint_template(r#"<MyComponent>content</MyComponent>"#, "test.vue");
    assert_eq!(result.warning_count, 0);
}

#[test]
fn test_valid_nuxt_child_builtin() {
    let linter = create_linter();
    let result = linter.lint_template(r#"<nuxt-child id="index"></nuxt-child>"#, "test.vue");
    assert_eq!(result.warning_count, 0);
}

fn create_nuxt_linter() -> Linter {
    let mut registry = RuleRegistry::new();
    registry.register(Box::new(HtmlSelfClosingNuxt::default()));
    Linter::with_registry(registry)
}

#[test]
fn test_nuxt_preset_allows_vuetify_tags() {
    let linter = create_nuxt_linter();
    let result = linter.lint_template(
        r#"<v-dialog><v-btn></v-btn><v-icon></v-icon><v-spacer /></v-dialog>"#,
        "test.vue",
    );
    assert_eq!(result.warning_count, 0);
}

#[test]
fn test_default_still_flags_empty_vuetify_tags() {
    let linter = create_linter();
    let result = linter.lint_template(r#"<v-btn></v-btn>"#, "test.vue");
    assert_eq!(result.warning_count, 1);
}

#[test]
fn test_nuxt_preset_still_flags_other_components() {
    let linter = create_nuxt_linter();
    let result = linter.lint_template(r#"<MyComponent></MyComponent>"#, "test.vue");
    assert_eq!(result.warning_count, 1);
}

#[test]
fn test_configured_option_matches_requested_eslint_vue_shape() {
    let linter = create_configured_linter(HtmlSelfClosingOptions {
        html: HtmlSelfClosingHtmlOptions {
            void: HtmlSelfClosingStyle::Any,
            normal: HtmlSelfClosingStyle::Never,
            component: HtmlSelfClosingStyle::Any,
        },
        svg: HtmlSelfClosingStyle::Any,
        math: HtmlSelfClosingStyle::Any,
    });
    let result = linter.lint_template(
        r#"<img><div /><MyComponent></MyComponent><svg><path></path></svg><math><mi></mi></math>"#,
        "test.vue",
    );
    assert_eq!(result.warning_count, 1);
    assert_eq!(
        result.diagnostics[0].message,
        "Element must not use self-closing syntax"
    );
}

#[test]
fn test_configured_never_flags_self_closing_components_svg_and_math() {
    let linter = create_configured_linter(HtmlSelfClosingOptions {
        html: HtmlSelfClosingHtmlOptions {
            component: HtmlSelfClosingStyle::Never,
            ..HtmlSelfClosingHtmlOptions::default()
        },
        svg: HtmlSelfClosingStyle::Never,
        math: HtmlSelfClosingStyle::Never,
    });
    let result = linter.lint_template(
        r#"<MyComponent /><svg><path /></svg><math><mi /></math>"#,
        "test.vue",
    );
    assert_eq!(result.warning_count, 3);
}
