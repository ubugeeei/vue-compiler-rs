//! Props generation functions.

use crate::ast::*;

use super::context::CodegenContext;
use super::expression::{generate_event_handler, generate_expression};
use super::helpers::{camelize, capitalize_first, is_valid_js_identifier};

/// Generate props object
pub fn generate_props(ctx: &mut CodegenContext, props: &[PropNode<'_>]) {
    if props.is_empty() {
        ctx.push("null");
        return;
    }

    // Check for dynamic v-model - needs normalizeProps wrapper
    let has_dyn_vmodel = has_dynamic_vmodel(props);
    if has_dyn_vmodel {
        ctx.use_helper(RuntimeHelper::NormalizeProps);
        ctx.push(ctx.helper(RuntimeHelper::NormalizeProps));
        ctx.push("(");
    }

    // Check for static class/style that need to be merged with dynamic
    let static_class = props.iter().find_map(|p| {
        if let PropNode::Attribute(attr) = p {
            if attr.name == "class" {
                return attr.value.as_ref().map(|v| v.content.as_str());
            }
        }
        None
    });

    let static_style = props.iter().find_map(|p| {
        if let PropNode::Attribute(attr) = p {
            if attr.name == "style" {
                return attr.value.as_ref().map(|v| v.content.as_str());
            }
        }
        None
    });

    let has_dynamic_class = props.iter().any(|p| {
        if let PropNode::Directive(dir) = p {
            if dir.name == "bind" {
                if let Some(ExpressionNode::Simple(exp)) = &dir.arg {
                    return exp.content == "class";
                }
            }
        }
        false
    });

    let has_dynamic_style = props.iter().any(|p| {
        if let PropNode::Directive(dir) = p {
            if dir.name == "bind" {
                if let Some(ExpressionNode::Simple(exp)) = &dir.arg {
                    return exp.content == "style";
                }
            }
        }
        false
    });

    // Skip static class/style if we have dynamic version (will merge them)
    let skip_static_class = static_class.is_some() && has_dynamic_class;
    let skip_static_style = static_style.is_some() && has_dynamic_style;

    // Count visible props (attributes + supported directives)
    let visible_count = props
        .iter()
        .filter(|p| match p {
            PropNode::Attribute(attr) => {
                if skip_static_class && attr.name == "class" {
                    return false;
                }
                if skip_static_style && attr.name == "style" {
                    return false;
                }
                true
            }
            PropNode::Directive(dir) => is_supported_directive(dir),
        })
        .count();

    // Check if any prop requires a normalizer (class/style bindings) or uses helper functions (v-text)
    let has_normalizer = props.iter().any(|p| {
        if let PropNode::Directive(dir) = p {
            // v-text uses _toDisplayString, which makes the output multiline
            if dir.name == "text" {
                return true;
            }
            if dir.name == "bind" {
                if let Some(ExpressionNode::Simple(exp)) = &dir.arg {
                    return exp.content == "class" || exp.content == "style";
                }
            }
        }
        false
    });

    // Check if any v-on has inline handler (not just identifier) or has runtime modifiers
    let has_inline_handler = props.iter().any(|p| {
        if let PropNode::Directive(dir) = p {
            if dir.name == "on" {
                // Check for modifiers that will use withModifiers or withKeys (not event option modifiers)
                let has_runtime_modifier = dir.modifiers.iter().any(|m| {
                    let n = m.content.as_str();
                    // Event option modifiers (capture, once, passive) don't require multiline
                    // because they just modify the event name, not wrap the handler
                    !matches!(n, "capture" | "once" | "passive")
                });
                if has_runtime_modifier {
                    return true;
                }
                if let Some(ExpressionNode::Simple(simple)) = &dir.exp {
                    // Inline if contains operators, parens, or is not simple identifier
                    let content = simple.content.as_str();
                    return content.contains('(')
                        || content.contains('+')
                        || content.contains('-')
                        || content.contains('=')
                        || content.contains(' ');
                }
            }
        }
        false
    });

    let multiline = visible_count > 1 || has_normalizer || has_inline_handler;

    if multiline {
        ctx.push("{");
        ctx.indent();
    } else {
        ctx.push("{ ");
    }

    let mut first = true;

    for prop in props {
        match prop {
            PropNode::Attribute(attr) => {
                // Skip static class/style if merging with dynamic
                if skip_static_class && attr.name == "class" {
                    continue;
                }
                if skip_static_style && attr.name == "style" {
                    continue;
                }
                if !first {
                    ctx.push(",");
                }
                if multiline {
                    ctx.newline();
                } else if !first {
                    ctx.push(" ");
                }
                first = false;
                // Keys need quotes if they contain special characters (like hyphens)
                let needs_quotes = !is_valid_js_identifier(&attr.name);
                if needs_quotes {
                    ctx.push("\"");
                }
                ctx.push(&attr.name);
                if needs_quotes {
                    ctx.push("\"");
                }
                ctx.push(": ");
                if let Some(value) = &attr.value {
                    ctx.push("\"");
                    ctx.push(&value.content);
                    ctx.push("\"");
                } else {
                    // Boolean attributes should be empty string, not true
                    ctx.push("\"\"");
                }
            }
            PropNode::Directive(dir) => {
                // Only add comma if directive produces valid output
                if is_supported_directive(dir) {
                    if !first {
                        ctx.push(",");
                    }
                    if multiline {
                        ctx.newline();
                    } else if !first {
                        ctx.push(" ");
                    }
                    first = false;
                    generate_directive_prop_with_static(ctx, dir, static_class, static_style);
                }
                // Skip unsupported directives entirely - don't output comments
                // as they cause syntax errors with trailing commas
            }
        }
    }

    if multiline {
        ctx.deindent();
        ctx.newline();
        ctx.push("}");
    } else {
        ctx.push(" }");
    }

    // Close normalizeProps wrapper if needed
    if has_dyn_vmodel {
        ctx.push(")");
    }
}

/// Check if a directive will produce valid output
pub fn is_supported_directive(dir: &DirectiveNode<'_>) -> bool {
    // v-model with dynamic arg on components needs special props handling
    // Static v-model is handled via withDirectives for native elements or transformed for components
    if dir.name == "model" {
        return dir.arg.as_ref().is_some_and(|arg| match arg {
            ExpressionNode::Simple(exp) => !exp.is_static,
            ExpressionNode::Compound(_) => true,
        });
    }
    matches!(dir.name.as_str(), "bind" | "on" | "html" | "text")
}

/// Check if element has dynamic v-model (with dynamic argument)
pub fn has_dynamic_vmodel(props: &[PropNode<'_>]) -> bool {
    props.iter().any(|p| {
        if let PropNode::Directive(dir) = p {
            if dir.name == "model" {
                return dir.arg.as_ref().is_some_and(|arg| match arg {
                    ExpressionNode::Simple(exp) => !exp.is_static,
                    ExpressionNode::Compound(_) => true,
                });
            }
        }
        false
    })
}

/// Generate directive as prop with optional static class/style merging
pub fn generate_directive_prop_with_static(
    ctx: &mut CodegenContext,
    dir: &DirectiveNode<'_>,
    static_class: Option<&str>,
    static_style: Option<&str>,
) {
    match dir.name.as_str() {
        "bind" => {
            let mut is_class = false;
            let mut is_style = false;

            // Check for modifiers
            let has_camel = dir.modifiers.iter().any(|m| m.content == "camel");
            let has_prop = dir.modifiers.iter().any(|m| m.content == "prop");
            let has_attr = dir.modifiers.iter().any(|m| m.content == "attr");

            if let Some(ExpressionNode::Simple(exp)) = &dir.arg {
                let key = &exp.content;
                is_class = key == "class";
                is_style = key == "style";

                // Transform key based on modifiers
                let transformed_key = if has_camel {
                    // Convert kebab-case to camelCase
                    camelize(key)
                } else if has_prop {
                    // Add . prefix for DOM property binding
                    format!(".{}", key)
                } else if has_attr {
                    // Add ^ prefix for attribute binding
                    format!("^{}", key)
                } else {
                    key.to_string()
                };

                let needs_quotes = !is_valid_js_identifier(&transformed_key);
                if needs_quotes {
                    ctx.push("\"");
                }
                ctx.push(&transformed_key);
                if needs_quotes {
                    ctx.push("\"");
                }
                ctx.push(": ");
            }
            if let Some(exp) = &dir.exp {
                if is_class {
                    ctx.use_helper(RuntimeHelper::NormalizeClass);
                    ctx.push("_normalizeClass(");
                    // Merge static class if present
                    if let Some(static_val) = static_class {
                        ctx.push("[\"");
                        ctx.push(static_val);
                        ctx.push("\", ");
                        generate_expression(ctx, exp);
                        ctx.push("]");
                    } else {
                        generate_expression(ctx, exp);
                    }
                    ctx.push(")");
                } else if is_style {
                    ctx.use_helper(RuntimeHelper::NormalizeStyle);
                    ctx.push("_normalizeStyle(");
                    // Merge static style if present
                    if let Some(static_val) = static_style {
                        ctx.push("[{");
                        // Parse static style and convert to object
                        for (i, part) in static_val
                            .split(';')
                            .filter(|s| !s.trim().is_empty())
                            .enumerate()
                        {
                            if i > 0 {
                                ctx.push(",");
                            }
                            let parts: Vec<&str> = part.splitn(2, ':').collect();
                            if parts.len() == 2 {
                                let key = parts[0].trim();
                                let value = parts[1].trim();
                                ctx.push("\"");
                                ctx.push(key);
                                ctx.push("\":\"");
                                ctx.push(value);
                                ctx.push("\"");
                            }
                        }
                        ctx.push("}, ");
                        generate_expression(ctx, exp);
                        ctx.push("]");
                    } else {
                        generate_expression(ctx, exp);
                    }
                    ctx.push(")");
                } else {
                    generate_expression(ctx, exp);
                }
            } else {
                ctx.push("undefined");
            }
        }
        "on" => {
            // Get event name first to determine context for modifiers
            let event_name = if let Some(ExpressionNode::Simple(exp)) = &dir.arg {
                exp.content.as_str()
            } else {
                ""
            };

            // Check if this is a keyboard event (for context-dependent modifiers)
            let is_keyboard_event = matches!(event_name, "keydown" | "keyup" | "keypress");

            // Collect modifiers into categories
            let mut event_option_modifiers: Vec<&str> = Vec::new();
            let mut system_modifiers: Vec<&str> = Vec::new();
            let mut key_modifiers: Vec<&str> = Vec::new();

            for modifier in dir.modifiers.iter() {
                let mod_name = modifier.content.as_str();
                match mod_name {
                    // Event option modifiers - appended to event name
                    "capture" | "once" | "passive" => {
                        event_option_modifiers.push(mod_name);
                    }
                    // Context-dependent: left/right are arrow keys on keyboard events,
                    // mouse buttons on click events
                    "left" | "right" => {
                        if is_keyboard_event {
                            key_modifiers.push(mod_name);
                        } else {
                            system_modifiers.push(mod_name);
                        }
                    }
                    // System modifiers - wrapped with withModifiers
                    "stop" | "prevent" | "self" | "ctrl" | "shift" | "alt" | "meta" | "middle"
                    | "exact" => {
                        system_modifiers.push(mod_name);
                    }
                    // Key modifiers - wrapped with withKeys
                    "enter" | "tab" | "delete" | "esc" | "space" | "up" | "down" => {
                        key_modifiers.push(mod_name);
                    }
                    _ => {
                        // Unknown modifiers (including numeric keycodes) are treated as key modifiers
                        key_modifiers.push(mod_name);
                    }
                }
            }

            if let Some(ExpressionNode::Simple(exp)) = &dir.arg {
                let mut event_name = exp.content.as_str();

                // Special mouse button modifiers that change the event name
                // @click.right -> onContextmenu, @click.middle -> onMouseup
                let has_right_modifier = system_modifiers.contains(&"right");
                let has_middle_modifier = system_modifiers.contains(&"middle");

                if event_name == "click" && has_right_modifier {
                    event_name = "contextmenu";
                } else if event_name == "click" && has_middle_modifier {
                    event_name = "mouseup";
                }

                // Handle special event names like "update:modelValue"
                if event_name.contains(':') {
                    // Event name with colon needs quotes (e.g., "onUpdate:modelValue")
                    let parts: Vec<&str> = event_name.splitn(2, ':').collect();
                    if parts.len() == 2 {
                        ctx.push("\"on");
                        // Capitalize the first part (e.g., "update" -> "Update")
                        let first_part = parts[0];
                        if let Some(first) = first_part.chars().next() {
                            ctx.push(&first.to_uppercase().to_string());
                            ctx.push(&first_part[1..]);
                        }
                        ctx.push(":");
                        ctx.push(parts[1]);
                        // Append event option modifiers
                        for opt_mod in &event_option_modifiers {
                            ctx.push(&capitalize_first(opt_mod));
                        }
                        ctx.push("\": ");
                    }
                } else {
                    // Simple event names don't need quotes (onUpdate, onClick)
                    ctx.push("on");
                    // Capitalize first letter
                    if let Some(first) = event_name.chars().next() {
                        ctx.push(&first.to_uppercase().to_string());
                        ctx.push(&event_name[1..]);
                    }
                    // Append event option modifiers (Capture, Once, Passive)
                    for opt_mod in &event_option_modifiers {
                        ctx.push(&capitalize_first(opt_mod));
                    }
                    ctx.push(": ");
                }
            }

            // Generate handler with optional withModifiers/withKeys wrappers
            // Order: _withKeys(_withModifiers(handler, [system_mods]), [key_mods])
            let has_system_mods = !system_modifiers.is_empty();
            let has_key_mods = !key_modifiers.is_empty();

            if has_key_mods {
                ctx.use_helper(RuntimeHelper::WithKeys);
                ctx.push("_withKeys(");
            }

            if has_system_mods {
                ctx.use_helper(RuntimeHelper::WithModifiers);
                ctx.push("_withModifiers(");
            }

            // Generate the actual handler
            if let Some(exp) = &dir.exp {
                generate_event_handler(ctx, exp);
            } else {
                ctx.push("() => {}");
            }

            // Close withModifiers wrapper
            if has_system_mods {
                ctx.push(", [");
                for (i, mod_name) in system_modifiers.iter().enumerate() {
                    if i > 0 {
                        ctx.push(",");
                    }
                    ctx.push("\"");
                    ctx.push(mod_name);
                    ctx.push("\"");
                }
                ctx.push("])");
            }

            // Close withKeys wrapper
            if has_key_mods {
                ctx.push(", [");
                for (i, mod_name) in key_modifiers.iter().enumerate() {
                    if i > 0 {
                        ctx.push(",");
                    }
                    ctx.push("\"");
                    ctx.push(mod_name);
                    ctx.push("\"");
                }
                ctx.push("])");
            }
        }
        "model" => {
            // Handle dynamic v-model on component
            // Generate: [_ctx.prop]: _ctx.value, ["onUpdate:" + _ctx.prop]: handler
            if let Some(ExpressionNode::Simple(arg_exp)) = &dir.arg {
                if !arg_exp.is_static {
                    let prop_name = &arg_exp.content;
                    let value_exp = dir
                        .exp
                        .as_ref()
                        .map(|e| match e {
                            ExpressionNode::Simple(s) => s.content.as_str(),
                            ExpressionNode::Compound(c) => c.loc.source.as_str(),
                        })
                        .unwrap_or("undefined");

                    // [_ctx.prop]: _ctx.value
                    ctx.push("[_ctx.");
                    ctx.push(prop_name);
                    ctx.push("]: ");
                    ctx.push(value_exp);
                    ctx.push(",");
                    ctx.newline();

                    // ["onUpdate:" + _ctx.prop]: $event => ((_ctx.value) = $event)
                    ctx.push("[\"onUpdate:\" + _ctx.");
                    ctx.push(prop_name);
                    ctx.push("]: $event => ((");
                    ctx.push(value_exp);
                    ctx.push(") = $event)");

                    // Add modifiers if present
                    if !dir.modifiers.is_empty() {
                        ctx.push(",");
                        ctx.newline();
                        // [_ctx.prop + "Modifiers"]: { modifier: true }
                        ctx.push("[_ctx.");
                        ctx.push(prop_name);
                        ctx.push(" + \"Modifiers\"]: { ");
                        for (i, modifier) in dir.modifiers.iter().enumerate() {
                            if i > 0 {
                                ctx.push(", ");
                            }
                            ctx.push(&modifier.content);
                            ctx.push(": true");
                        }
                        ctx.push(" }");
                    }
                }
            }
        }
        "html" => {
            // v-html="rawHtml" -> innerHTML: _ctx.rawHtml
            ctx.push("innerHTML: ");
            if let Some(exp) = &dir.exp {
                generate_expression(ctx, exp);
            } else {
                ctx.push("undefined");
            }
        }
        "text" => {
            // v-text="message" -> textContent: _toDisplayString(_ctx.message)
            ctx.use_helper(RuntimeHelper::ToDisplayString);
            ctx.push("textContent: ");
            ctx.push(ctx.helper(RuntimeHelper::ToDisplayString));
            ctx.push("(");
            if let Some(exp) = &dir.exp {
                generate_expression(ctx, exp);
            } else {
                ctx.push("undefined");
            }
            ctx.push(")");
        }
        _ => {
            // Other directives are skipped by is_supported_directive()
            // This case should not be reached in normal operation
        }
    }
}
