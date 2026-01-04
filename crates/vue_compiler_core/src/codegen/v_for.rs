//! v-for generation functions.

use crate::ast::*;

use super::children::generate_children;
use super::context::CodegenContext;
use super::element::{generate_vshow_closing, has_vshow_directive};
use super::expression::generate_expression;
use super::node::generate_node;

/// Check if source is a numeric literal (for v-for range)
pub fn is_numeric_source(source: &ExpressionNode<'_>) -> bool {
    if let ExpressionNode::Simple(exp) = source {
        exp.content.chars().all(|c| c.is_ascii_digit())
    } else {
        false
    }
}

/// Check if element has a :key binding
pub fn get_element_key<'a, 'b>(el: &'b ElementNode<'a>) -> Option<&'b ExpressionNode<'a>>
where
    'a: 'b,
{
    for prop in &el.props {
        if let PropNode::Directive(dir) = prop {
            if dir.name == "bind" {
                if let Some(ExpressionNode::Simple(arg)) = &dir.arg {
                    if arg.content == "key" {
                        return dir.exp.as_ref();
                    }
                }
            }
        }
    }
    None
}

/// Generate for node
pub fn generate_for(ctx: &mut CodegenContext, for_node: &ForNode<'_>) {
    ctx.use_helper(RuntimeHelper::OpenBlock);
    ctx.use_helper(RuntimeHelper::CreateElementBlock);
    ctx.use_helper(RuntimeHelper::Fragment);
    ctx.use_helper(RuntimeHelper::RenderList);

    // Determine if this is a numeric range (stable) or dynamic list
    let is_stable = is_numeric_source(&for_node.source);

    // Check if children have keys
    let has_key = for_node.children.iter().any(|child| {
        if let TemplateChildNode::Element(el) = child {
            get_element_key(el).is_some()
        } else {
            false
        }
    });

    // Fragment type: 64 = STABLE, 128 = KEYED, 256 = UNKEYED
    let fragment_flag = if is_stable {
        64 // STABLE_FRAGMENT
    } else if has_key {
        128 // KEYED_FRAGMENT
    } else {
        256 // UNKEYED_FRAGMENT
    };

    ctx.push("(");
    ctx.push(ctx.helper(RuntimeHelper::OpenBlock));
    if is_stable {
        ctx.push("(), ");
    } else {
        ctx.push("(true), ");
    }
    ctx.push(ctx.helper(RuntimeHelper::CreateElementBlock));
    ctx.push("(");
    ctx.push(ctx.helper(RuntimeHelper::Fragment));
    ctx.push(", null, ");
    ctx.push(ctx.helper(RuntimeHelper::RenderList));
    ctx.push("(");
    generate_expression(ctx, &for_node.source);
    ctx.push(", (");

    // Value alias
    if let Some(value) = &for_node.value_alias {
        generate_expression(ctx, value);
    } else {
        ctx.push("_item");
    }

    // Key alias
    if let Some(key) = &for_node.key_alias {
        ctx.push(", ");
        generate_expression(ctx, key);
    }

    // Index alias
    if let Some(index) = &for_node.object_index_alias {
        ctx.push(", ");
        generate_expression(ctx, index);
    }

    ctx.push(") => {");
    ctx.indent();
    ctx.newline();
    ctx.push("return ");

    // Generate child as block (not regular node)
    if for_node.children.len() == 1 {
        generate_for_item(ctx, &for_node.children[0], is_stable);
    } else {
        generate_children(ctx, &for_node.children);
    }

    ctx.deindent();
    ctx.newline();
    // Close with fragment flag
    let flag_name = match fragment_flag {
        64 => "STABLE_FRAGMENT",
        128 => "KEYED_FRAGMENT",
        256 => "UNKEYED_FRAGMENT",
        _ => "FRAGMENT",
    };
    ctx.push("}), ");
    ctx.push(&fragment_flag.to_string());
    ctx.push(" /* ");
    ctx.push(flag_name);
    ctx.push(" */))");
}

/// Generate item for v-for (as block, not regular vnode)
pub fn generate_for_item(ctx: &mut CodegenContext, node: &TemplateChildNode<'_>, is_stable: bool) {
    match node {
        TemplateChildNode::Element(el) => {
            let key_exp = get_element_key(el);
            let is_template = el.tag_type == ElementType::Template;
            let is_component = el.tag_type == ElementType::Component;

            // Check for v-show directive
            let has_vshow = has_vshow_directive(el);
            if has_vshow {
                ctx.use_helper(RuntimeHelper::WithDirectives);
                ctx.use_helper(RuntimeHelper::VShow);
                ctx.push(ctx.helper(RuntimeHelper::WithDirectives));
                ctx.push("(");
            }

            if is_stable {
                // Stable fragment: use createElementVNode without block wrapper
                ctx.use_helper(RuntimeHelper::CreateElementVNode);
                ctx.push(ctx.helper(RuntimeHelper::CreateElementVNode));
                ctx.push("(\"");
                ctx.push(&el.tag);
                ctx.push("\"");

                // Props with key
                if let Some(key) = key_exp {
                    ctx.push(", { key: ");
                    generate_expression(ctx, key);
                    ctx.push(" }");
                } else if !el.children.is_empty() {
                    ctx.push(", null");
                }

                // Children
                if !el.children.is_empty() {
                    ctx.push(", ");
                    generate_children(ctx, &el.children);
                }

                // Add TEXT patch flag if has interpolation
                let has_interpolation = el
                    .children
                    .iter()
                    .any(|c| matches!(c, TemplateChildNode::Interpolation(_)));
                if has_interpolation {
                    ctx.push(", 1 /* TEXT */");
                }

                ctx.push(")");

                // Close withDirectives for v-show
                if has_vshow {
                    generate_vshow_closing(ctx, el);
                }
            } else {
                // Dynamic list: wrap in block
                ctx.use_helper(RuntimeHelper::OpenBlock);
                ctx.push("(");
                ctx.push(ctx.helper(RuntimeHelper::OpenBlock));
                ctx.push("(), ");

                if is_component {
                    // Component: use createBlock
                    ctx.use_helper(RuntimeHelper::CreateBlock);
                    ctx.push(ctx.helper(RuntimeHelper::CreateBlock));
                    ctx.push("(_component_");
                    ctx.push(&el.tag.replace('-', "_"));
                } else if is_template {
                    // Template: use Fragment
                    ctx.use_helper(RuntimeHelper::CreateElementBlock);
                    ctx.use_helper(RuntimeHelper::Fragment);
                    ctx.push(ctx.helper(RuntimeHelper::CreateElementBlock));
                    ctx.push("(");
                    ctx.push(ctx.helper(RuntimeHelper::Fragment));
                } else {
                    // Regular element
                    ctx.use_helper(RuntimeHelper::CreateElementBlock);
                    ctx.push(ctx.helper(RuntimeHelper::CreateElementBlock));
                    ctx.push("(\"");
                    ctx.push(&el.tag);
                    ctx.push("\"");
                }

                // Props with key (inline for simple identifiers, multiline for complex)
                if let Some(key) = key_exp {
                    let is_simple_key = if let ExpressionNode::Simple(exp) = key {
                        !exp.content.contains('.')
                    } else {
                        false
                    };

                    if is_simple_key {
                        ctx.push(", { key: ");
                        generate_expression(ctx, key);
                        ctx.push(" }");
                    } else {
                        ctx.push(", {");
                        ctx.indent();
                        ctx.newline();
                        ctx.push("key: ");
                        generate_expression(ctx, key);
                        ctx.deindent();
                        ctx.newline();
                        ctx.push("}");
                    }
                } else if !el.children.is_empty() || is_template {
                    ctx.push(", null");
                }

                // Children
                if !el.children.is_empty() {
                    ctx.push(", ");
                    if is_template {
                        // Template children are array
                        ctx.push("[");
                        ctx.indent();
                        for (i, child) in el.children.iter().enumerate() {
                            if i > 0 {
                                ctx.push(",");
                            }
                            ctx.newline();
                            generate_node(ctx, child);
                        }
                        ctx.deindent();
                        ctx.newline();
                        ctx.push("]");
                    } else {
                        generate_children(ctx, &el.children);
                    }
                }

                // Add patch flag
                let has_interpolation = el
                    .children
                    .iter()
                    .any(|c| matches!(c, TemplateChildNode::Interpolation(_)));

                if is_template {
                    ctx.push(", 64 /* STABLE_FRAGMENT */");
                } else if has_interpolation {
                    ctx.push(", 1 /* TEXT */");
                }

                ctx.push("))");

                // Close withDirectives for v-show
                if has_vshow {
                    generate_vshow_closing(ctx, el);
                }
            }
        }
        _ => generate_node(ctx, node),
    }
}
