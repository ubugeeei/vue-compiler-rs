//! Slot generation functions.
//!
//! Generates slot objects for component children.

use crate::ast::*;
use crate::transforms::v_slot::{collect_slots, get_slot_name, has_v_slot};

use super::context::CodegenContext;
use super::node::generate_node;

/// Get slot props expression as raw source (not transformed)
fn get_slot_props_raw(dir: &DirectiveNode<'_>) -> Option<vue_allocator::String> {
    dir.exp.as_ref().map(|exp| match exp {
        ExpressionNode::Simple(s) => s.loc.source.clone(),
        ExpressionNode::Compound(c) => c.loc.source.clone(),
    })
}

/// Extract parameter names from slot props expression
/// e.g., "{ item }" -> ["item"], "{ item, index }" -> ["item", "index"]
/// e.g., "slotProps" -> ["slotProps"]
fn extract_slot_params(props_str: &str) -> Vec<std::string::String> {
    let trimmed = props_str.trim();
    let mut params = Vec::new();

    if trimmed.starts_with('{') && trimmed.ends_with('}') {
        // Destructuring pattern: { item, index }
        let inner = &trimmed[1..trimmed.len() - 1];
        for part in inner.split(',') {
            let part = part.trim();
            // Handle default values like "item = default"
            let name = if let Some(pos) = part.find('=') {
                part[..pos].trim()
            } else if let Some(pos) = part.find(':') {
                // Handle renaming like "user: { name }" - take the first part
                part[..pos].trim()
            } else {
                part
            };
            if !name.is_empty() && is_identifier(name) {
                params.push(name.to_string());
            }
        }
    } else if is_identifier(trimmed) {
        // Simple identifier: slotProps
        params.push(trimmed.to_string());
    }

    params
}

/// Check if a string is a valid JavaScript identifier
fn is_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_alphabetic() || c == '_' || c == '$' => {}
        _ => return false,
    }
    chars.all(|c| c.is_alphanumeric() || c == '_' || c == '$')
}

/// Check if component has slot children that need to be generated as slots object
pub fn has_slot_children(el: &ElementNode<'_>) -> bool {
    if el.children.is_empty() {
        return false;
    }

    // Check for v-slot on component root
    for prop in &el.props {
        if let PropNode::Directive(dir) = prop {
            if dir.name.as_str() == "slot" {
                return true;
            }
        }
    }

    // Check for any children (default slot) or template slots
    true
}

/// Check if component has dynamic slots (requires DYNAMIC_SLOTS patch flag)
pub fn has_dynamic_slots_flag(el: &ElementNode<'_>) -> bool {
    let collected_slots = collect_slots(el);
    collected_slots.iter().any(|s| s.is_dynamic)
}

/// Generate slots object for component
pub fn generate_slots(ctx: &mut CodegenContext, el: &ElementNode<'_>) {
    ctx.use_helper(RuntimeHelper::WithCtx);

    // Check for v-slot on component root (shorthand for default slot)
    let root_slot = el.props.iter().find_map(|p| {
        if let PropNode::Directive(dir) = p {
            if dir.name.as_str() == "slot" {
                return Some(dir.as_ref());
            }
        }
        None
    });

    let collected_slots = collect_slots(el);
    let has_dynamic_slots = collected_slots.iter().any(|s| s.is_dynamic);

    ctx.push("{");
    ctx.indent();

    if let Some(slot_dir) = root_slot {
        // v-slot on component root - all children go to default slot
        ctx.newline();
        ctx.push("default: ");
        ctx.push(ctx.helper(RuntimeHelper::WithCtx));
        ctx.push("(");
        // Slot props (scoped slot params) - use raw source, not transformed
        let params = if let Some(props_str) = get_slot_props_raw(slot_dir) {
            ctx.push("(");
            ctx.push(&props_str);
            ctx.push(")");
            extract_slot_params(&props_str)
        } else {
            ctx.push("()");
            vec![]
        };

        // Track slot params for stripping _ctx. prefix
        ctx.add_slot_params(&params);

        ctx.push(" => [");
        ctx.indent();
        generate_slot_children(ctx, &el.children);
        ctx.deindent();
        ctx.newline();
        ctx.push("])");

        // Remove slot params
        ctx.remove_slot_params(&params);
    } else {
        // Check for named slots via template#slotName
        let mut has_generated_default = false;
        let mut first_slot = true;

        for child in &el.children {
            if let TemplateChildNode::Element(template_el) = child {
                if template_el.tag.as_str() == "template" && has_v_slot(template_el) {
                    // This is a named slot template
                    if let Some(slot_dir) = template_el.props.iter().find_map(|p| {
                        if let PropNode::Directive(dir) = p {
                            if dir.name.as_str() == "slot" {
                                return Some(dir.as_ref());
                            }
                        }
                        None
                    }) {
                        if !first_slot {
                            ctx.push(",");
                        }
                        first_slot = false;
                        ctx.newline();

                        let slot_name = get_slot_name(slot_dir);
                        let is_dynamic = slot_dir
                            .arg
                            .as_ref()
                            .map(|arg| match arg {
                                ExpressionNode::Simple(exp) => !exp.is_static,
                                ExpressionNode::Compound(_) => true,
                            })
                            .unwrap_or(false);

                        if is_dynamic {
                            // Dynamic slot name: [_ctx.slotName]
                            ctx.push("[");
                            ctx.push("_ctx.");
                            ctx.push(&slot_name);
                            ctx.push("]");
                        } else {
                            ctx.push(&slot_name);
                        }

                        if slot_name.as_str() == "default" {
                            has_generated_default = true;
                        }

                        ctx.push(": ");
                        ctx.push(ctx.helper(RuntimeHelper::WithCtx));
                        ctx.push("(");

                        // Slot props - use raw source, not transformed
                        let params = if let Some(props_str) = get_slot_props_raw(slot_dir) {
                            ctx.push("(");
                            ctx.push(&props_str);
                            ctx.push(")");
                            extract_slot_params(&props_str)
                        } else {
                            ctx.push("()");
                            vec![]
                        };

                        // Track slot params for stripping _ctx. prefix
                        ctx.add_slot_params(&params);

                        ctx.push(" => [");
                        ctx.indent();
                        generate_slot_children(ctx, &template_el.children);
                        ctx.deindent();
                        ctx.newline();
                        ctx.push("])");

                        // Remove slot params
                        ctx.remove_slot_params(&params);
                    }
                }
            }
        }

        // Generate default slot for non-template children
        let default_children: Vec<_> = el
            .children
            .iter()
            .filter(|child| {
                if let TemplateChildNode::Element(template_el) = child {
                    !(template_el.tag.as_str() == "template" && has_v_slot(template_el))
                } else {
                    true
                }
            })
            .collect();

        if !default_children.is_empty() && !has_generated_default {
            if !first_slot {
                ctx.push(",");
            }
            ctx.newline();
            ctx.push("default: ");
            ctx.push(ctx.helper(RuntimeHelper::WithCtx));
            ctx.push("(() => [");
            ctx.indent();
            for (i, child) in default_children.iter().enumerate() {
                if i > 0 {
                    ctx.push(",");
                }
                ctx.newline();
                generate_slot_child_node(ctx, child);
            }
            ctx.deindent();
            ctx.newline();
            ctx.push("])");
        }
    }

    // Add slot stability flag
    ctx.push(",");
    ctx.newline();
    if has_dynamic_slots {
        ctx.push("_: 2 /* DYNAMIC */");
    } else {
        ctx.push("_: 1 /* STABLE */");
    }

    ctx.deindent();
    ctx.newline();
    ctx.push("}");
}

/// Generate children for a slot
fn generate_slot_children(ctx: &mut CodegenContext, children: &[TemplateChildNode<'_>]) {
    // Check if all children are text/interpolation - if so, concatenate into single _createTextVNode
    let all_text_or_interp = children.iter().all(|child| {
        matches!(
            child,
            TemplateChildNode::Text(_) | TemplateChildNode::Interpolation(_)
        )
    });

    if all_text_or_interp && !children.is_empty() {
        ctx.newline();
        ctx.use_helper(RuntimeHelper::CreateText);
        ctx.push(ctx.helper(RuntimeHelper::CreateText));
        ctx.push("(");

        let has_interpolation = children
            .iter()
            .any(|c| matches!(c, TemplateChildNode::Interpolation(_)));

        for (i, child) in children.iter().enumerate() {
            if i > 0 {
                ctx.push(" + ");
            }
            match child {
                TemplateChildNode::Text(text) => {
                    ctx.push("\"");
                    ctx.push(&super::helpers::escape_js_string(&text.content));
                    ctx.push("\"");
                }
                TemplateChildNode::Interpolation(interp) => {
                    ctx.use_helper(RuntimeHelper::ToDisplayString);
                    ctx.push(ctx.helper(RuntimeHelper::ToDisplayString));
                    ctx.push("(");
                    generate_slot_expression(ctx, &interp.content);
                    ctx.push(")");
                }
                _ => {}
            }
        }

        if has_interpolation {
            ctx.push(", 1 /* TEXT */)");
        } else {
            ctx.push(")");
        }
    } else {
        for (i, child) in children.iter().enumerate() {
            if i > 0 {
                ctx.push(",");
            }
            ctx.newline();
            generate_slot_child_node(ctx, child);
        }
    }
}

/// Generate a single child node for slot content
fn generate_slot_child_node(ctx: &mut CodegenContext, child: &TemplateChildNode<'_>) {
    match child {
        TemplateChildNode::Text(text) => {
            ctx.use_helper(RuntimeHelper::CreateText);
            ctx.push(ctx.helper(RuntimeHelper::CreateText));
            ctx.push("(\"");
            ctx.push(&super::helpers::escape_js_string(&text.content));
            ctx.push("\")");
        }
        TemplateChildNode::Interpolation(interp) => {
            ctx.use_helper(RuntimeHelper::CreateText);
            ctx.use_helper(RuntimeHelper::ToDisplayString);
            ctx.push(ctx.helper(RuntimeHelper::CreateText));
            ctx.push("(");
            ctx.push(ctx.helper(RuntimeHelper::ToDisplayString));
            ctx.push("(");
            // Generate expression, stripping _ctx. prefix for slot params
            generate_slot_expression(ctx, &interp.content);
            ctx.push("), 1 /* TEXT */)");
        }
        _ => {
            generate_node(ctx, child);
        }
    }
}

/// Generate expression for slot content, stripping _ctx. prefix for slot parameters
fn generate_slot_expression(ctx: &mut CodegenContext, expr: &ExpressionNode<'_>) {
    match expr {
        ExpressionNode::Simple(exp) => {
            if exp.is_static {
                ctx.push("\"");
                ctx.push(&exp.content);
                ctx.push("\"");
            } else {
                // Strip _ctx. prefix for slot parameters
                let content = strip_ctx_prefix_for_slot_params(ctx, &exp.content);
                ctx.push(&content);
            }
        }
        ExpressionNode::Compound(comp) => {
            for child in comp.children.iter() {
                match child {
                    crate::ast::CompoundExpressionChild::Simple(exp) => {
                        if exp.is_static {
                            ctx.push("\"");
                            ctx.push(&exp.content);
                            ctx.push("\"");
                        } else {
                            let content = strip_ctx_prefix_for_slot_params(ctx, &exp.content);
                            ctx.push(&content);
                        }
                    }
                    crate::ast::CompoundExpressionChild::String(s) => {
                        ctx.push(s);
                    }
                    crate::ast::CompoundExpressionChild::Symbol(helper) => {
                        ctx.push(ctx.helper(*helper));
                    }
                    _ => {}
                }
            }
        }
    }
}

/// Strip _ctx. prefix from identifiers that are slot parameters
fn strip_ctx_prefix_for_slot_params(ctx: &CodegenContext, content: &str) -> std::string::String {
    let mut result = content.to_string();
    for param in &ctx.slot_params {
        // Replace _ctx.paramName with paramName
        let prefixed = format!("_ctx.{}", param);
        result = result.replace(&prefixed, param);
    }
    result
}
