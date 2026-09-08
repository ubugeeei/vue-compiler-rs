//! Component `ref` callback checks for component usages that do not enter the
//! full prop checker.
//!
//! Vue owns the parameters of a function-valued `ref` binding. Emitting the
//! expression as a bare `void ((ref) => ...)` loses that contextual type and
//! reports `TS7006`, while dropping the expression entirely can make type-only
//! references inside the callback appear unused. A small typed statement keeps
//! the authored body alive and uses `any` for the Vue-owned callback payload.

use super::super::types::{VizeMapping, VizeSubSpan};
use crate::virtual_ts::scope::is_direct_inline_function_prop_value;
use vize_carton::{FxHashMap, append, is_native_tag};
use vize_croquis::TemplateExpression;
use vize_relief::{
    DirectiveNode, ElementNode, ExpressionNode, PropNode, RootNode, TemplateChildNode,
};

pub(crate) type ComponentRefCallbackBindings = FxHashMap<(u32, u32), ComponentRefCallbackBinding>;

#[derive(Clone, Copy)]
pub(crate) struct ComponentRefCallbackBinding;

pub(crate) fn collect_component_ref_callback_bindings(
    root: Option<&RootNode<'_>>,
    enabled: bool,
) -> ComponentRefCallbackBindings {
    let mut bindings = ComponentRefCallbackBindings::default();
    let Some(root) = root.filter(|_| enabled) else {
        return bindings;
    };
    for child in &root.children {
        collect_child_bindings(child, &mut bindings);
    }
    bindings
}

fn collect_child_bindings(
    child: &TemplateChildNode<'_>,
    bindings: &mut ComponentRefCallbackBindings,
) {
    match child {
        TemplateChildNode::Element(element) => collect_element_bindings(element, bindings),
        TemplateChildNode::If(node) => {
            for branch in &node.branches {
                for child in &branch.children {
                    collect_child_bindings(child, bindings);
                }
            }
        }
        TemplateChildNode::IfBranch(branch) => {
            for child in &branch.children {
                collect_child_bindings(child, bindings);
            }
        }
        TemplateChildNode::For(node) => {
            for child in &node.children {
                collect_child_bindings(child, bindings);
            }
        }
        _ => {}
    }
}

fn collect_element_bindings(
    element: &ElementNode<'_>,
    bindings: &mut ComponentRefCallbackBindings,
) {
    if !is_native_tag(element.tag) {
        for prop in &element.props {
            let PropNode::Directive(directive) = prop else {
                continue;
            };
            if let Some(range) = component_ref_callback_range(directive) {
                bindings.insert(range, ComponentRefCallbackBinding);
            }
        }
    }
    for child in &element.children {
        collect_child_bindings(child, bindings);
    }
}

fn component_ref_callback_range(directive: &DirectiveNode<'_>) -> Option<(u32, u32)> {
    if directive.name != "bind" {
        return None;
    }
    let ExpressionNode::Simple(argument) = directive.arg.as_ref()? else {
        return None;
    };
    if !argument.is_static || argument.content != "ref" {
        return None;
    }
    let expression_node = directive.exp.as_ref()?;
    let ExpressionNode::Simple(expression) = expression_node else {
        return None;
    };
    if !is_direct_inline_function_prop_value(expression.content) {
        return None;
    }
    let location = expression_node.loc();
    Some((location.span.start, location.span.end))
}

pub(super) fn generate_component_ref_callback_statement(
    ts: &mut vize_carton::String,
    mappings: &mut Vec<VizeMapping>,
    expr: &TemplateExpression,
    generated_expression: &str,
    template_offset: u32,
    indent: &str,
) {
    let value_src_start = (template_offset + expr.start) as usize;
    let value_src_end = (template_offset + expr.end) as usize;
    let gen_stmt_start = ts.len();

    append!(*ts, "{indent}const ");
    let check_name_start = ts.len();
    append!(*ts, "__vize_component_ref_check_{}", expr.start);
    let check_name_end = ts.len();
    ts.push_str(": (ref: any, refs: Record<string, any>) => void = (");
    let value_gen_start = ts.len();
    ts.push_str(generated_expression);
    let value_gen_end = ts.len();
    ts.push_str(");\n");
    let gen_stmt_end = ts.len();
    append!(
        *ts,
        "{indent}void __vize_component_ref_check_{}; // VBind\n",
        expr.start
    );

    mappings.push(VizeMapping {
        gen_range: gen_stmt_start..gen_stmt_end,
        src_range: value_src_start..value_src_end,
        sub_spans: vec![
            VizeSubSpan {
                gen_range: check_name_start..check_name_end,
                src_range: value_src_start..value_src_end,
            },
            VizeSubSpan {
                gen_range: value_gen_start..value_gen_end,
                src_range: value_src_start..value_src_end,
            },
        ],
    });
}
