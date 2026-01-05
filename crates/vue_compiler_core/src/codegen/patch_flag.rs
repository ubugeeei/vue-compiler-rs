//! Patch flag calculation and naming functions.

use super::helpers::camelize;
use crate::ast::*;
use crate::options::{BindingMetadata, BindingType};

/// Check if an interpolation references only constant bindings (LiteralConst or SetupConst)
/// These bindings never change at runtime, so no TEXT patch flag is needed.
fn is_constant_interpolation(
    expr: &ExpressionNode<'_>,
    bindings: Option<&BindingMetadata>,
) -> bool {
    let bindings = match bindings {
        Some(b) => b,
        None => return false, // No binding info, assume dynamic
    };

    match expr {
        ExpressionNode::Simple(simple) => {
            // Check if the expression is a simple identifier that's a constant
            // Both LiteralConst (e.g., const x = 'hello') and SetupConst (e.g., class Foo {})
            // are constant at runtime and don't need TEXT patch flag
            let name = simple.content.as_str();
            matches!(
                bindings.bindings.get(name),
                Some(BindingType::LiteralConst | BindingType::SetupConst)
            )
        }
        ExpressionNode::Compound(_) => false, // Compound expressions are dynamic
    }
}

/// Check if an event handler references a constant binding (SetupConst or LiteralConst)
fn is_const_handler(expr: &ExpressionNode<'_>, bindings: Option<&BindingMetadata>) -> bool {
    let bindings = match bindings {
        Some(b) => b,
        None => return false, // No binding info, assume dynamic
    };

    match expr {
        ExpressionNode::Simple(simple) => {
            // Check if the expression is a simple identifier that's a constant
            let name = simple.content.as_str();
            matches!(
                bindings.bindings.get(name),
                Some(BindingType::SetupConst | BindingType::LiteralConst)
            )
        }
        ExpressionNode::Compound(_) => false, // Compound expressions are dynamic
    }
}

/// Check if a directive is a built-in directive (not custom)
fn is_builtin_directive(name: &str) -> bool {
    matches!(
        name,
        "bind"
            | "on"
            | "if"
            | "else"
            | "else-if"
            | "for"
            | "show"
            | "model"
            | "slot"
            | "cloak"
            | "pre"
            | "memo"
            | "once"
            | "text"
            | "html"
    )
}

/// Calculate patch flag and dynamic props for an element
pub fn calculate_element_patch_info(
    el: &ElementNode<'_>,
    bindings: Option<&BindingMetadata>,
) -> (Option<i32>, Option<Vec<String>>) {
    let mut flag: i32 = 0;
    // Pre-allocate with small capacity - most elements have few dynamic props
    let mut dynamic_props: Vec<String> = Vec::with_capacity(4);
    let mut has_vshow = false;
    let mut has_custom_directive = false;
    let mut has_ref = false;

    for prop in el.props.iter() {
        // Check for ref attribute (static)
        if let PropNode::Attribute(attr) = prop {
            if attr.name == "ref" {
                has_ref = true;
            }
        }
        if let PropNode::Directive(dir) = prop {
            match dir.name.as_str() {
                "bind" => {
                    // Check for modifiers
                    let has_camel = dir.modifiers.iter().any(|m| m.content == "camel");
                    let has_prop = dir.modifiers.iter().any(|m| m.content == "prop");
                    let has_attr = dir.modifiers.iter().any(|m| m.content == "attr");

                    if let Some(arg) = &dir.arg {
                        if let ExpressionNode::Simple(exp) = arg {
                            if !exp.is_static {
                                // Dynamic key - FULL_PROPS
                                flag |= 16;
                            } else {
                                let key = exp.content.as_str();
                                match key {
                                    "class" => flag |= 2, // CLASS
                                    "style" => flag |= 4, // STYLE
                                    // key and ref are special props - don't add to patch flags
                                    "key" | "ref" => {}
                                    _ => {
                                        // Skip modelModifiers and *Modifiers props (they are static)
                                        if !key.ends_with("Modifiers") {
                                            flag |= 8; // PROPS

                                            // Transform key based on modifiers
                                            let prop_name = if has_camel {
                                                camelize(key)
                                            } else if has_prop {
                                                format!(".{}", key)
                                            } else if has_attr {
                                                format!("^{}", key)
                                            } else {
                                                key.to_string()
                                            };
                                            dynamic_props.push(prop_name);

                                            // .prop modifier requires NEED_HYDRATION
                                            if has_prop {
                                                flag |= 32; // NEED_HYDRATION
                                            }
                                        }
                                    }
                                }
                            }
                        } else {
                            // Compound expression as key - FULL_PROPS
                            flag |= 16;
                        }
                    } else {
                        // No arg (v-bind without argument) - FULL_PROPS
                        flag |= 16;
                    }
                }
                "on" => {
                    // Event handlers are considered dynamic props
                    if let Some(arg) = &dir.arg {
                        if let ExpressionNode::Simple(exp) = arg {
                            if !exp.is_static {
                                // Dynamic event name
                                flag |= 16;
                            } else {
                                // Check for mouse button modifiers that transform the event name
                                let base_event = exp.content.as_str();
                                let has_right_modifier =
                                    dir.modifiers.iter().any(|m| m.content == "right");
                                let has_middle_modifier =
                                    dir.modifiers.iter().any(|m| m.content == "middle");

                                // Transform event name for special mouse button modifiers
                                let actual_event = if base_event == "click" && has_right_modifier {
                                    "contextmenu"
                                } else if base_event == "click" && has_middle_modifier {
                                    "mouseup"
                                } else {
                                    base_event
                                };

                                // Build event name
                                let mut event_name = String::with_capacity(2 + actual_event.len());
                                event_name.push_str("on");
                                // Capitalize first letter inline
                                let mut chars = actual_event.chars();
                                if let Some(c) = chars.next() {
                                    for uc in c.to_uppercase() {
                                        event_name.push(uc);
                                    }
                                    event_name.push_str(chars.as_str());
                                }

                                // Check for event option modifiers that affect the event name
                                for modifier in dir.modifiers.iter() {
                                    let mod_name = modifier.content.as_str();
                                    if mod_name == "capture"
                                        || mod_name == "once"
                                        || mod_name == "passive"
                                    {
                                        let mut cap_mod = String::new();
                                        let mut chars = mod_name.chars();
                                        if let Some(c) = chars.next() {
                                            for uc in c.to_uppercase() {
                                                cap_mod.push(uc);
                                            }
                                            cap_mod.push_str(chars.as_str());
                                        }
                                        event_name.push_str(&cap_mod);
                                    }
                                }

                                // Check if the handler references a constant binding
                                // If so, we don't need PROPS flag since the handler won't change
                                let handler_is_const = if let Some(handler_exp) = &dir.exp {
                                    is_const_handler(handler_exp, bindings)
                                } else {
                                    false
                                };

                                if !handler_is_const {
                                    flag |= 8; // PROPS
                                    dynamic_props.push(event_name.clone());
                                }

                                // Check if this is a custom event (non-standard DOM event)
                                // Custom events, events with option modifiers, and events with key modifiers need NEED_HYDRATION
                                let has_option_modifier = dir.modifiers.iter().any(|m| {
                                    let n = m.content.as_str();
                                    n == "capture" || n == "once" || n == "passive"
                                });
                                // Check for key modifiers (will use withKeys)
                                let has_key_modifier = dir.modifiers.iter().any(|m| {
                                    let n = m.content.as_str();
                                    matches!(n, "enter" | "tab" | "delete" | "esc" | "space" | "up" | "down")
                                        || n.chars().all(|c| c.is_ascii_digit()) // numeric keycodes
                                        || !matches!(n, "capture" | "once" | "passive" | "stop" | "prevent" | "self" | "ctrl" | "shift" | "alt" | "meta" | "left" | "middle" | "right" | "exact")
                                });

                                // Events that don't need NEED_HYDRATION:
                                // - Basic click/dblclick without special modifiers
                                // - update:* events (v-model internal events)
                                // - Component events (non-DOM element events)
                                // Note: event name can be "update:modelValue" or "Update:modelValue"
                                let lower_event = base_event.to_lowercase();
                                let is_vmodel_update = lower_event.starts_with("update:");
                                let is_simple_click = matches!(actual_event, "click" | "dblclick")
                                    && !has_option_modifier
                                    && !has_key_modifier
                                    && !has_right_modifier
                                    && !has_middle_modifier;
                                let is_component_event = el.tag_type == ElementType::Component;

                                if !is_simple_click
                                    && !is_vmodel_update
                                    && !is_component_event
                                    && !handler_is_const
                                {
                                    flag |= 32; // NEED_HYDRATION
                                }
                            }
                        } else {
                            flag |= 16;
                        }
                    }
                }
                "show" => {
                    // v-show requires NEED_PATCH, but only if no other flags are set
                    has_vshow = true;
                }
                "html" => {
                    // v-html sets innerHTML - dynamic prop
                    flag |= 8; // PROPS
                    dynamic_props.push("innerHTML".to_string());
                }
                "text" => {
                    // v-text sets textContent - dynamic prop
                    flag |= 8; // PROPS
                    dynamic_props.push("textContent".to_string());
                }
                _ => {
                    // Custom directive - requires NEED_PATCH
                    if !is_builtin_directive(&dir.name) {
                        has_custom_directive = true;
                    }
                }
            }
        }
    }

    // Add NEED_PATCH for v-show, custom directives, or ref only if no other dynamic bindings exist
    if (has_vshow || has_custom_directive || has_ref) && flag == 0 {
        flag |= 512; // NEED_PATCH
    }

    // Check for dynamic text children
    // TEXT flag should be set when children contain interpolations and only consist of text/interpolation
    // But skip if all interpolations reference only LiteralConst bindings (compile-time constants)
    let has_interpolation = el
        .children
        .iter()
        .any(|child| matches!(child, TemplateChildNode::Interpolation(_)));
    let all_text_or_interp = el.children.iter().all(|child| {
        matches!(
            child,
            TemplateChildNode::Text(_) | TemplateChildNode::Interpolation(_)
        )
    });
    if has_interpolation && all_text_or_interp {
        // Check if all interpolations reference only constant bindings
        let all_constant = el.children.iter().all(|child| {
            if let TemplateChildNode::Interpolation(interp) = child {
                is_constant_interpolation(&interp.content, bindings)
            } else {
                true // Text nodes are always "constant"
            }
        });
        if !all_constant {
            flag |= 1; // TEXT
        }
    }

    let patch_flag = if flag > 0 { Some(flag) } else { None };
    let dynamic_props_result = if !dynamic_props.is_empty() {
        Some(dynamic_props)
    } else {
        None
    };

    (patch_flag, dynamic_props_result)
}

/// Get patch flag name for comment
pub fn patch_flag_name(flag: i32) -> String {
    // Single flag matches
    match flag {
        1 => return "TEXT".to_string(),
        2 => return "CLASS".to_string(),
        4 => return "STYLE".to_string(),
        8 => return "PROPS".to_string(),
        16 => return "FULL_PROPS".to_string(),
        32 => return "NEED_HYDRATION".to_string(),
        64 => return "STABLE_FRAGMENT".to_string(),
        128 => return "KEYED_FRAGMENT".to_string(),
        256 => return "UNKEYED_FRAGMENT".to_string(),
        512 => return "NEED_PATCH".to_string(),
        1024 => return "DYNAMIC_SLOTS".to_string(),
        _ => {}
    }

    // Multiple flags - build combined string
    let mut names = Vec::new();
    if flag & 1 != 0 {
        names.push("TEXT");
    }
    if flag & 2 != 0 {
        names.push("CLASS");
    }
    if flag & 4 != 0 {
        names.push("STYLE");
    }
    if flag & 8 != 0 {
        names.push("PROPS");
    }
    if flag & 16 != 0 {
        names.push("FULL_PROPS");
    }
    if flag & 32 != 0 {
        names.push("NEED_HYDRATION");
    }
    if flag & 64 != 0 {
        names.push("STABLE_FRAGMENT");
    }
    if flag & 128 != 0 {
        names.push("KEYED_FRAGMENT");
    }
    if flag & 256 != 0 {
        names.push("UNKEYED_FRAGMENT");
    }
    if flag & 512 != 0 {
        names.push("NEED_PATCH");
    }
    if flag & 1024 != 0 {
        names.push("DYNAMIC_SLOTS");
    }

    if names.is_empty() {
        "UNKNOWN".to_string()
    } else {
        names.join(", ")
    }
}
