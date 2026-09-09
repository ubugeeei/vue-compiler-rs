//! vue/no-non-component-keep-alive-child
//!
//! Disallow plain element wrappers directly below `<KeepAlive>`.
//!
//! Vue's `<KeepAlive>` caches component VNodes. A native wrapper such as
//! `<div v-if>` between `<KeepAlive>` and the component leaves the wrapped
//! component uncached while still looking intentional in review.

use crate::context::LintContext;
use crate::diagnostic::Severity;
use crate::rule::{Rule, RuleCategory, RuleMeta};
use vize_relief::{ElementNode, ElementType, ExpressionNode, PropNode, TemplateChildNode};

static META: RuleMeta = RuleMeta {
    name: "vue/no-non-component-keep-alive-child",
    description: "Disallow plain element wrappers directly below `<KeepAlive>`",
    category: RuleCategory::Recommended,
    fixable: false,
    default_severity: Severity::Warning,
};

/// Detect native wrappers that make `<KeepAlive>` a no-op.
pub struct NoNonComponentKeepAliveChild;

impl Rule for NoNonComponentKeepAliveChild {
    fn meta(&self) -> &'static RuleMeta {
        &META
    }

    fn enter_element<'a>(&self, ctx: &mut LintContext<'a>, element: &ElementNode<'a>) {
        if !is_keep_alive_tag(element.tag) {
            return;
        }

        check_children(ctx, &element.children);
    }
}

fn check_children<'a>(ctx: &mut LintContext<'a>, children: &[TemplateChildNode<'a>]) {
    for child in children {
        check_child(ctx, child);
    }
}

fn check_child<'a>(ctx: &mut LintContext<'a>, child: &TemplateChildNode<'a>) {
    match child {
        TemplateChildNode::Comment(_) => {}
        TemplateChildNode::Text(text) if text.content.trim().is_empty() => {}
        TemplateChildNode::Element(element) => check_element_child(ctx, element),
        TemplateChildNode::If(if_node) => {
            for branch in &if_node.branches {
                check_children(ctx, &branch.children);
            }
        }
        TemplateChildNode::For(for_node) => check_children(ctx, &for_node.children),
        _ => {}
    }
}

fn check_element_child<'a>(ctx: &mut LintContext<'a>, element: &ElementNode<'a>) {
    if is_allowed_component_child(element) {
        return;
    }

    if element.tag_type == ElementType::Template {
        check_children(ctx, &element.children);
        return;
    }

    if is_keep_alive_transparent_wrapper(element.tag) {
        check_children(ctx, &element.children);
        return;
    }

    if element.tag_type != ElementType::Element || has_v_show_only(element) {
        return;
    }

    ctx.warn_with_help(
        ctx.t("vue/no-non-component-keep-alive-child.message"),
        &element.loc,
        ctx.t("vue/no-non-component-keep-alive-child.help"),
    );
}

fn is_allowed_component_child(element: &ElementNode<'_>) -> bool {
    if is_dynamic_component(element) {
        return true;
    }

    if element.tag_type == ElementType::Component || element.tag_type == ElementType::Slot {
        return !is_keep_alive_transparent_wrapper(element.tag);
    }

    is_allowed_builtin_component_child(element.tag)
}

fn is_dynamic_component(element: &ElementNode<'_>) -> bool {
    element.tag == "component"
        && element.props.iter().any(|prop| match prop {
            PropNode::Attribute(attr) => attr.name == "is",
            PropNode::Directive(dir) => dir.name == "bind" && expression_arg_is(dir, "is"),
        })
}

fn is_keep_alive_tag(tag: &str) -> bool {
    matches!(tag, "KeepAlive" | "keep-alive")
}

fn is_keep_alive_transparent_wrapper(tag: &str) -> bool {
    matches!(
        tag,
        "Transition" | "transition" | "BaseTransition" | "base-transition"
    )
}

fn is_allowed_builtin_component_child(tag: &str) -> bool {
    matches!(
        tag,
        "Suspense" | "suspense" | "Teleport" | "teleport" | "TransitionGroup" | "transition-group"
    )
}

fn has_v_show_only(element: &ElementNode<'_>) -> bool {
    let mut has_show = false;
    for prop in &element.props {
        let PropNode::Directive(dir) = prop else {
            continue;
        };
        match dir.name {
            "show" => has_show = true,
            "if" | "else-if" | "else" | "for" => return false,
            "bind" if is_key_binding(dir) => return false,
            _ => {}
        }
    }
    has_show
}

fn is_key_binding(dir: &vize_relief::DirectiveNode<'_>) -> bool {
    expression_arg_is(dir, "key")
}

fn expression_arg_is(dir: &vize_relief::DirectiveNode<'_>, name: &str) -> bool {
    matches!(
        dir.arg.as_ref(),
        Some(ExpressionNode::Simple(arg)) if arg.content == name
    )
}

#[cfg(test)]
mod tests {
    use super::NoNonComponentKeepAliveChild;
    use crate::linter::Linter;
    use crate::rule::RuleRegistry;

    fn create_linter() -> Linter {
        let mut registry = RuleRegistry::new();
        registry.register(Box::new(NoNonComponentKeepAliveChild));
        Linter::with_registry(registry)
    }

    #[test]
    fn reports_plain_keep_alive_child() {
        let result = create_linter().lint_template(
            r#"<KeepAlive><div><SomeComponent /></div></KeepAlive>"#,
            "test.vue",
        );

        assert_eq!(result.warning_count, 1);
        assert_eq!(
            result.diagnostics[0].rule_name,
            "vue/no-non-component-keep-alive-child"
        );
    }

    #[test]
    fn reports_plain_branch_wrappers() {
        let result = create_linter().lint_template(
            r#"<keep-alive><div v-if="a"><One /></div><section v-else><Two /></section></keep-alive>"#,
            "test.vue",
        );

        assert_eq!(result.warning_count, 2);
    }

    #[test]
    fn reports_plain_child_inside_transition_below_keep_alive() {
        let result = create_linter().lint_template(
            r#"<KeepAlive><Transition><main v-if="ready"><Screen /></main></Transition></KeepAlive>"#,
            "test.vue",
        );

        assert_eq!(result.warning_count, 1);
    }

    #[test]
    fn accepts_component_children() {
        let result = create_linter().lint_template(
            r#"<KeepAlive><One v-if="a" /><Two v-else /></KeepAlive>"#,
            "test.vue",
        );

        assert_eq!(result.warning_count, 0);
    }

    #[test]
    fn accepts_dynamic_component_and_builtins() {
        let result = create_linter().lint_template(
            r#"
            <KeepAlive><component :is="view" /></KeepAlive>
            <KeepAlive><Suspense><Screen /></Suspense></KeepAlive>
            <KeepAlive><Teleport to="body"><Screen /></Teleport></KeepAlive>
            <KeepAlive><TransitionGroup><Screen /></TransitionGroup></KeepAlive>
            "#,
            "test.vue",
        );

        assert_eq!(result.warning_count, 0);
    }

    #[test]
    fn ignores_v_show_only_wrapper() {
        let result = create_linter().lint_template(
            r#"<KeepAlive><div v-show="opened"><SomeComponent /></div></KeepAlive>"#,
            "test.vue",
        );

        assert_eq!(result.warning_count, 0);
    }

    #[test]
    fn reports_v_show_wrapper_when_it_also_controls_identity() {
        let result = create_linter().lint_template(
            r#"<KeepAlive><div v-show="opened" :key="view"><SomeComponent /></div></KeepAlive>"#,
            "test.vue",
        );

        assert_eq!(result.warning_count, 1);
    }

    #[test]
    fn ignores_standalone_transition_native_children() {
        let result = create_linter().lint_template(
            r#"<Transition><div v-if="open">content</div></Transition>"#,
            "test.vue",
        );

        assert_eq!(result.warning_count, 0);
    }
}
