//! v-slot directive transform.
//!
//! Transforms v-slot (# shorthand) directives for slot content.

use vue_allocator::String;

use crate::ast::*;
use crate::transform::TransformContext;

/// Check if element has v-slot directive
pub fn has_v_slot(el: &ElementNode<'_>) -> bool {
    el.props
        .iter()
        .any(|prop| matches!(prop, PropNode::Directive(dir) if dir.name == "slot"))
}

/// Get slot name from v-slot directive
/// For dynamic slots, returns the raw source (without _ctx. prefix)
/// For static slots, returns the content
pub fn get_slot_name(dir: &DirectiveNode<'_>) -> String {
    dir.arg
        .as_ref()
        .map(|arg| match arg {
            ExpressionNode::Simple(exp) => {
                if exp.is_static {
                    exp.content.clone()
                } else {
                    // For dynamic slot names, use raw source to avoid double _ctx. prefix
                    exp.loc.source.clone()
                }
            }
            ExpressionNode::Compound(exp) => exp.loc.source.clone(),
        })
        .unwrap_or_else(|| String::new("default"))
}

/// Get slot props expression as string from v-slot directive
pub fn get_slot_props_string(dir: &DirectiveNode<'_>) -> Option<String> {
    dir.exp.as_ref().map(|exp| match exp {
        ExpressionNode::Simple(s) => s.content.clone(),
        ExpressionNode::Compound(c) => c.loc.source.clone(),
    })
}

/// Check if slot is dynamic (has dynamic name)
pub fn is_dynamic_slot(dir: &DirectiveNode<'_>) -> bool {
    if let Some(arg) = &dir.arg {
        match arg {
            ExpressionNode::Simple(exp) => !exp.is_static,
            ExpressionNode::Compound(_) => true,
        }
    } else {
        false
    }
}

/// Slot outlet info for codegen
#[derive(Debug)]
pub struct SlotOutletInfo {
    pub name: String,
    pub props_expr: Option<String>,
    pub has_fallback: bool,
}

/// Transform v-slot directive for slot outlet (<slot>)
pub fn transform_slot_outlet<'a>(
    ctx: &mut TransformContext<'a>,
    dir: &DirectiveNode<'a>,
    el: &ElementNode<'a>,
) -> Option<SlotOutletInfo> {
    ctx.helper(RuntimeHelper::RenderSlot);

    // Only for <slot> elements
    if el.tag != "slot" {
        return None;
    }

    let slot_name = get_slot_name(dir);
    let props_expr = get_slot_props_string(dir);
    let has_fallback = !el.children.is_empty();

    Some(SlotOutletInfo {
        name: slot_name,
        props_expr,
        has_fallback,
    })
}

/// Slot info for component slots
#[derive(Debug)]
pub struct SlotInfo {
    pub name: String,
    pub params_expr: Option<String>,
    pub is_dynamic: bool,
}

/// Collect slot information from component children
pub fn collect_slots<'a>(el: &ElementNode<'a>) -> Vec<SlotInfo> {
    let mut slots = Vec::new();

    for child in el.children.iter() {
        if let TemplateChildNode::Element(child_el) = child {
            if child_el.tag == "template" {
                // Check for v-slot on template
                for prop in child_el.props.iter() {
                    if let PropNode::Directive(dir) = prop {
                        if dir.name == "slot" {
                            let name = get_slot_name(dir);
                            let params_expr = get_slot_props_string(dir);
                            let is_dynamic = is_dynamic_slot(dir);

                            slots.push(SlotInfo {
                                name,
                                params_expr,
                                is_dynamic,
                            });
                        }
                    }
                }
            }
        }
    }

    // Check for implicit default slot
    let has_non_slot_children = el.children.iter().any(|child| {
        if let TemplateChildNode::Element(el) = child {
            !(el.tag == "template" && has_v_slot(el))
        } else {
            true
        }
    });

    if has_non_slot_children && !slots.iter().any(|s| s.name == "default") {
        slots.push(SlotInfo {
            name: String::new("default"),
            params_expr: None,
            is_dynamic: false,
        });
    }

    slots
}

/// Check if component has dynamic slots
pub fn has_dynamic_slots<'a>(el: &ElementNode<'a>) -> bool {
    for child in el.children.iter() {
        if let TemplateChildNode::Element(child_el) = child {
            if child_el.tag == "template" {
                for prop in child_el.props.iter() {
                    if let PropNode::Directive(dir) = prop {
                        if dir.name == "slot" && is_dynamic_slot(dir) {
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;
    use bumpalo::Bump;

    #[test]
    fn test_has_v_slot() {
        let allocator = Bump::new();
        let (root, _) = parse(&allocator, r#"<template v-slot:header>content</template>"#);

        if let TemplateChildNode::Element(el) = &root.children[0] {
            assert!(has_v_slot(el));
        }
    }

    #[test]
    fn test_default_slot_name() {
        let allocator = Bump::new();
        let dir = DirectiveNode::new(&allocator, "slot", SourceLocation::STUB);
        assert_eq!(get_slot_name(&dir).as_str(), "default");
    }

    #[test]
    fn test_collect_slots() {
        let allocator = Bump::new();
        let (root, _) = parse(
            &allocator,
            r#"<Comp><template #header>H</template><template #footer>F</template></Comp>"#,
        );

        if let TemplateChildNode::Element(el) = &root.children[0] {
            let slots = collect_slots(el);
            assert_eq!(slots.len(), 2);
            assert!(slots.iter().any(|s| s.name == "header"));
            assert!(slots.iter().any(|s| s.name == "footer"));
        }
    }
}
