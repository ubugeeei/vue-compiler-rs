use super::{SfcElementOrder, SfcElementOrderGroup, SfcElementOrderOptions};
use crate::linter::Linter;
use crate::rule::RuleRegistry;

fn create_linter() -> Linter {
    create_linter_with_rule(SfcElementOrder::default())
}

fn create_linter_with_order(order: Vec<Vec<&str>>) -> Linter {
    create_linter_with_rule(SfcElementOrder::new(SfcElementOrderOptions {
        order: order
            .into_iter()
            .map(|selectors| {
                SfcElementOrderGroup::new(selectors.into_iter().map(Into::into).collect())
            })
            .collect(),
    }))
}

fn create_linter_with_rule(rule: SfcElementOrder) -> Linter {
    let mut registry = RuleRegistry::new();
    registry.register(Box::new(rule));
    Linter::with_registry(registry)
}

#[test]
fn test_valid_order_script_template_style() {
    let linter = create_linter();
    let result = linter.lint_sfc(
        r#"<script setup></script>
<template><div></div></template>
<style></style>"#,
        "test.vue",
    );
    assert_eq!(result.warning_count, 0);
}

/// `eslint-plugin-vue`'s `vue/block-order` default is
/// `[["script", "template"], "style"]`, so a template-first component — the
/// shape used by the official Vue docs and by `create-vue`'s templates — is
/// valid upstream and must stay silent here (#3223).
#[test]
fn test_valid_template_before_script() {
    let linter = create_linter();
    let result = linter.lint_sfc(
        r#"<template><div></div></template>
<script setup></script>"#,
        "test.vue",
    );
    assert_eq!(result.warning_count, 0);
    assert!(result.diagnostics.is_empty());
}

/// Pinned reproduction from `tests/_fixtures/_git/epic-spinners`
/// (`packages/docs/src/App.vue`, revision 3a4dda1d). Before #3223 this exact
/// shape produced one warning per component across 92 corpus projects.
#[test]
fn test_pinned_template_first_real_component_stays_clean() {
    let linter = create_linter();
    let result = linter.lint_sfc(
        r#"<template>
  <loaders-header class="container"/>
  <router-view class="container"/>
  <loaders-footer/>
</template>

<script lang="ts">
export default {
  name: 'app',
}
</script>

<style lang="scss">
.container { margin: 0; }
</style>"#,
        "App.vue",
    );
    assert_eq!(result.warning_count, 0);
    assert!(result.diagnostics.is_empty());
}

#[test]
fn test_invalid_style_before_script() {
    let linter = create_linter();
    let result = linter.lint_sfc(
        r#"<style></style>
<script setup></script>"#,
        "test.vue",
    );
    assert_eq!(result.warning_count, 1);
    assert_eq!(result.diagnostics[0].rule_name, "vue/sfc-element-order");
    insta::assert_debug_snapshot!(result.diagnostics);
}

/// Upstream anchors every block against the first earlier block that
/// outranks it, so both the script and the template are reported here — not
/// just the one adjacent to `<style>`.
#[test]
fn test_style_first_reports_every_later_block() {
    let linter = create_linter();
    let result = linter.lint_sfc(
        r#"<style></style>
<script setup></script>
<template><div></div></template>"#,
        "test.vue",
    );
    assert_eq!(result.warning_count, 2);
    assert_eq!(
        result
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect::<Vec<_>>(),
        vec![
            "<script setup> should come before <style>",
            "<template> should come before <style>",
        ],
    );
}

#[test]
fn test_invalid_style_before_template() {
    let linter = create_linter();
    let result = linter.lint_sfc(
        r#"<script setup></script>
<style></style>
<template><div></div></template>"#,
        "test.vue",
    );
    assert_eq!(result.warning_count, 1);
    assert_eq!(result.diagnostics[0].rule_name, "vue/sfc-element-order");
}

#[test]
fn test_custom_blocks_are_ignored_for_ordering() {
    let linter = create_linter();
    let result = linter.lint_sfc(
        r#"<docs>hello</docs>
<script setup></script>
<template><div></div></template>
<style></style>"#,
        "test.vue",
    );
    assert_eq!(result.warning_count, 0);
}

#[test]
fn test_configured_order_distinguishes_normal_script_from_script_setup() {
    let linter = create_linter_with_order(vec![
        vec!["template"],
        vec!["script:not([setup])"],
        vec!["script[setup]"],
        vec!["style"],
    ]);
    let result = linter.lint_sfc(
        r#"<template><div></div></template>
<script setup></script>
<script></script>
<style></style>"#,
        "test.vue",
    );
    assert_eq!(result.warning_count, 1);
    assert_eq!(
        result.diagnostics[0].message,
        "<script> should come before <script setup>"
    );
}

#[test]
fn test_configured_order_can_include_custom_blocks() {
    let linter = create_linter_with_order(vec![
        vec!["template"],
        vec!["script"],
        vec!["i18n"],
        vec!["style"],
    ]);
    let result = linter.lint_sfc(
        r#"<template><div></div></template>
<i18n>{}</i18n>
<script setup></script>
<style></style>"#,
        "test.vue",
    );
    assert_eq!(result.warning_count, 1);
    assert_eq!(
        result.diagnostics[0].message,
        "<script setup> should come before <i18n>"
    );
}
