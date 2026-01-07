//! Static hoisting transform.
//!
//! Hoists static nodes to reduce runtime overhead.

use vue_allocator::{Box, Bump, Vec};

use crate::ast::*;
use crate::transform::TransformContext;

/// Check if a node is fully static (can be hoisted)
pub fn is_static_node(node: &TemplateChildNode<'_>) -> bool {
    match node {
        TemplateChildNode::Text(_) => true,
        TemplateChildNode::Comment(_) => true,
        TemplateChildNode::Element(el) => is_static_element(el),
        TemplateChildNode::Interpolation(_) => false,
        TemplateChildNode::If(_) => false,
        TemplateChildNode::For(_) => false,
        _ => false,
    }
}

/// Check if an element is fully static
fn is_static_element(el: &ElementNode<'_>) -> bool {
    // Components are not static
    if el.tag_type != ElementType::Element {
        return false;
    }

    // Check for dynamic props
    for prop in el.props.iter() {
        if let PropNode::Directive(_) = prop {
            return false;
        }
    }

    // Check children recursively
    // Comments are not hoisted because create_children_expression doesn't handle them
    for child in el.children.iter() {
        // Comments prevent hoisting since they can't be serialized to VNodeCall children
        if matches!(child, TemplateChildNode::Comment(_)) {
            return false;
        }
        if !is_static_node(child) {
            return false;
        }
    }

    true
}

/// Get the static type of a node
pub fn get_static_type(node: &TemplateChildNode<'_>) -> StaticType {
    match node {
        TemplateChildNode::Text(_) => StaticType::FullyStatic,
        TemplateChildNode::Comment(_) => StaticType::FullyStatic,
        TemplateChildNode::Element(el) => get_element_static_type(el),
        TemplateChildNode::Interpolation(_) => StaticType::NotStatic,
        _ => StaticType::NotStatic,
    }
}

fn get_element_static_type(el: &ElementNode<'_>) -> StaticType {
    if el.tag_type != ElementType::Element {
        return StaticType::NotStatic;
    }

    // Check for any dynamic content
    let mut has_dynamic_text = false;

    for prop in el.props.iter() {
        if let PropNode::Directive(_) = prop {
            // Any directive makes the element dynamic (non-static)
            // This includes v-bind:class, v-bind:style, v-on:*, etc.
            return StaticType::NotStatic;
        }
    }

    // Check children
    for child in el.children.iter() {
        match child {
            TemplateChildNode::Interpolation(_) => {
                has_dynamic_text = true;
            }
            TemplateChildNode::Element(child_el) => {
                if get_element_static_type(child_el) == StaticType::NotStatic {
                    return StaticType::NotStatic;
                }
            }
            TemplateChildNode::If(_) | TemplateChildNode::For(_) => {
                return StaticType::NotStatic;
            }
            // Comments prevent hoisting since they can't be serialized to VNodeCall children
            TemplateChildNode::Comment(_) => {
                return StaticType::NotStatic;
            }
            _ => {}
        }
    }

    if has_dynamic_text {
        StaticType::HasDynamicText
    } else {
        StaticType::FullyStatic
    }
}

/// Static type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StaticType {
    NotStatic = 0,
    FullyStatic = 1,
    HasDynamicText = 2,
}

/// Hoist static nodes in the tree
pub fn hoist_static<'a>(
    ctx: &mut TransformContext<'a>,
    children: &mut Vec<'a, TemplateChildNode<'a>>,
) {
    hoist_static_inner(ctx, children, true)
}

/// Inner implementation with is_root flag
fn hoist_static_inner<'a>(
    ctx: &mut TransformContext<'a>,
    children: &mut Vec<'a, TemplateChildNode<'a>>,
    is_root: bool,
) {
    if !ctx.options.hoist_static {
        return;
    }

    let allocator = ctx.allocator;
    let mut i = 0;

    while i < children.len() {
        let static_type = get_static_type(&children[i]);

        match static_type {
            StaticType::FullyStatic => {
                // Root elements should NOT be fully hoisted as VNodes
                // They must use createElementBlock for proper block tracking
                // Only hoist their props instead
                if is_root {
                    if let TemplateChildNode::Element(el) = &mut children[i] {
                        if has_static_props(el) {
                            hoist_element_props(ctx, el, allocator);
                        }
                    }
                } else {
                    // Non-root static elements can be fully hoisted
                    if let TemplateChildNode::Element(el) = &children[i] {
                        let vnode_call = create_vnode_call_from_element(allocator, el);
                        let hoist_index = ctx.hoist(vnode_call);
                        // Replace with hoisted reference
                        children[i] = TemplateChildNode::Hoisted(hoist_index);
                        ctx.helper(RuntimeHelper::CreateElementVNode);
                    }
                }
            }
            StaticType::HasDynamicText => {
                // Element has static props but dynamic text - hoist the props only
                if let TemplateChildNode::Element(el) = &mut children[i] {
                    if has_static_props(el) {
                        hoist_element_props(ctx, el, allocator);
                    }
                }
            }
            StaticType::NotStatic => {
                // Cannot hoist, but check children recursively (not as root)
                match &mut children[i] {
                    TemplateChildNode::Element(el) => {
                        hoist_static_inner(ctx, &mut el.children, false);
                    }
                    TemplateChildNode::If(if_node) => {
                        // For v-if branches, only hoist nested children, not the branch root
                        // The branch root needs a key and must be created inline
                        for branch in if_node.branches.iter_mut() {
                            for child in branch.children.iter_mut() {
                                if let TemplateChildNode::Element(el) = child {
                                    // Only hoist inside the branch root's children
                                    hoist_static_inner(ctx, &mut el.children, false);
                                }
                            }
                        }
                    }
                    TemplateChildNode::For(for_node) => {
                        hoist_static_inner(ctx, &mut for_node.children, false);
                    }
                    _ => {}
                }
            }
        }
        i += 1;
    }
}

/// Create a VNodeCall from an ElementNode for hoisting
fn create_vnode_call_from_element<'a>(
    allocator: &'a Bump,
    el: &ElementNode<'a>,
) -> JsChildNode<'a> {
    let tag = VNodeTag::String(el.tag.clone());
    let props = create_props_expression(allocator, &el.props);
    let children = create_children_expression(allocator, &el.children);

    let vnode_call = VNodeCall {
        tag,
        props,
        children,
        patch_flag: None,
        dynamic_props: None,
        directives: None,
        is_block: false,
        disable_tracking: false,
        is_component: false,
        loc: el.loc.clone(),
    };

    JsChildNode::VNodeCall(Box::new_in(vnode_call, allocator))
}

/// Create props expression from element props
fn create_props_expression<'a>(
    allocator: &'a Bump,
    props: &[PropNode<'a>],
) -> Option<PropsExpression<'a>> {
    if props.is_empty() {
        return None;
    }

    // Build object properties from attributes
    let mut obj_props = Vec::new_in(allocator);

    for prop in props {
        if let PropNode::Attribute(attr) = prop {
            let key = ExpressionNode::Simple(Box::new_in(
                SimpleExpressionNode::new(attr.name.clone(), true, attr.loc.clone()),
                allocator,
            ));
            let value_exp = if let Some(v) = &attr.value {
                SimpleExpressionNode::new(v.content.clone(), true, v.loc.clone())
            } else {
                SimpleExpressionNode::new("true", true, attr.loc.clone())
            };
            let value = JsChildNode::SimpleExpression(Box::new_in(value_exp, allocator));

            obj_props.push(Property {
                key,
                value,
                loc: attr.loc.clone(),
            });
        }
    }

    if obj_props.is_empty() {
        return None;
    }

    Some(PropsExpression::Object(Box::new_in(
        ObjectExpression {
            properties: obj_props,
            loc: SourceLocation::STUB,
        },
        allocator,
    )))
}

/// Create children expression from template children
fn create_children_expression<'a>(
    allocator: &'a Bump,
    children: &Vec<'a, TemplateChildNode<'a>>,
) -> Option<VNodeChildren<'a>> {
    if children.is_empty() {
        return None;
    }

    // For a single text child, use Single variant with Text
    if children.len() == 1 {
        if let TemplateChildNode::Text(text) = &children[0] {
            let text_node = TextNode::new(text.content.clone(), text.loc.clone());
            return Some(VNodeChildren::Single(TemplateTextChildNode::Text(
                Box::new_in(text_node, allocator),
            )));
        }
    }

    // For multiple text children, combine them
    let mut all_text = true;
    let mut text_content = String::new();

    for child in children.iter() {
        match child {
            TemplateChildNode::Text(text) => {
                text_content.push_str(&text.content);
            }
            _ => {
                all_text = false;
                break;
            }
        }
    }

    if all_text && !text_content.is_empty() {
        let text_node = TextNode::new(text_content, SourceLocation::STUB);
        return Some(VNodeChildren::Single(TemplateTextChildNode::Text(
            Box::new_in(text_node, allocator),
        )));
    }

    // For complex children with nested elements, return as Simple expression for now
    // A full implementation would recursively create VNodeCalls
    None
}

/// Check if an element has static props (all attributes, no dynamic bindings)
fn has_static_props(el: &ElementNode<'_>) -> bool {
    if el.props.is_empty() {
        return false;
    }

    for prop in el.props.iter() {
        if let PropNode::Directive(_) = prop {
            return false;
        }
    }

    true
}

/// Hoist the props of an element with static props
fn hoist_element_props<'a>(
    ctx: &mut TransformContext<'a>,
    el: &mut ElementNode<'a>,
    allocator: &'a Bump,
) {
    // Build props object from element attributes
    let mut obj_props = Vec::new_in(allocator);

    for prop in el.props.iter() {
        if let PropNode::Attribute(attr) = prop {
            let key = ExpressionNode::Simple(Box::new_in(
                SimpleExpressionNode::new(attr.name.clone(), true, attr.loc.clone()),
                allocator,
            ));
            let value_exp = if let Some(v) = &attr.value {
                SimpleExpressionNode::new(v.content.clone(), true, v.loc.clone())
            } else {
                SimpleExpressionNode::new("true", true, attr.loc.clone())
            };
            let value = JsChildNode::SimpleExpression(Box::new_in(value_exp, allocator));

            obj_props.push(Property {
                key,
                value,
                loc: attr.loc.clone(),
            });
        }
    }

    if obj_props.is_empty() {
        return;
    }

    // Create the object expression to hoist
    let obj_expr = ObjectExpression {
        properties: obj_props,
        loc: SourceLocation::STUB,
    };

    let js_node = JsChildNode::Object(Box::new_in(obj_expr, allocator));
    let hoist_index = ctx.hoist(js_node);

    // Mark the element as having hoisted props (1-based index for _hoisted_N)
    el.hoisted_props_index = Some(hoist_index + 1);
}

/// Check if children should use a block
pub fn should_use_block(el: &ElementNode<'_>) -> bool {
    // Use block for elements with v-for, v-if, or components
    for prop in el.props.iter() {
        if let PropNode::Directive(dir) = prop {
            if dir.name == "for" || dir.name == "if" {
                return true;
            }
        }
    }

    el.tag_type == ElementType::Component
}

/// Count dynamic children for optimization hints
pub fn count_dynamic_children(children: &[TemplateChildNode<'_>]) -> usize {
    let mut count = 0;

    for child in children {
        match child {
            TemplateChildNode::Interpolation(_) => count += 1,
            TemplateChildNode::Element(el) => {
                // Check for dynamic props
                for prop in el.props.iter() {
                    if let PropNode::Directive(_) = prop {
                        count += 1;
                        break;
                    }
                }
            }
            TemplateChildNode::If(_) | TemplateChildNode::For(_) => count += 1,
            _ => {}
        }
    }

    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;
    use bumpalo::Bump;

    #[test]
    fn test_static_text() {
        let allocator = Bump::new();
        let (root, _) = parse(&allocator, "hello");

        assert!(is_static_node(&root.children[0]));
    }

    #[test]
    fn test_static_element() {
        let allocator = Bump::new();
        let (root, _) = parse(&allocator, "<div>static</div>");

        assert!(is_static_node(&root.children[0]));
    }

    #[test]
    fn test_dynamic_element() {
        let allocator = Bump::new();
        let (root, _) = parse(&allocator, "<div :class=\"cls\">dynamic</div>");

        assert!(!is_static_node(&root.children[0]));
    }

    #[test]
    fn test_interpolation_not_static() {
        let allocator = Bump::new();
        let (root, _) = parse(&allocator, "{{ msg }}");

        assert!(!is_static_node(&root.children[0]));
    }

    #[test]
    fn test_nested_dynamic_class_not_static() {
        let allocator = Bump::new();
        let (root, _) = parse(
            &allocator,
            r#"<div class="checkbox"><span class="icon" :class="{ active: checked }" /></div>"#,
        );

        // The outer div should NOT be static because it contains a child with dynamic :class
        assert!(!is_static_node(&root.children[0]));
    }

    #[test]
    fn test_sibling_with_v_if() {
        let allocator = Bump::new();
        let (root, _) = parse(
            &allocator,
            r#"<div class="wrapper"><div class="checkbox"><span :class="{ active: checked }" /></div><label v-if="label">{{ label }}</label></div>"#,
        );

        // The outer div is not static because it has dynamic content
        if let TemplateChildNode::Element(el) = &root.children[0] {
            eprintln!(
                "Outer div static type: {:?}",
                get_static_type(&root.children[0])
            );

            // Check first child (div.checkbox)
            if let TemplateChildNode::Element(checkbox_div) = &el.children[0] {
                eprintln!("checkbox div props: {:?}", checkbox_div.props.len());
                eprintln!("checkbox div children: {:?}", checkbox_div.children.len());

                // Check nested span
                if let TemplateChildNode::Element(span) = &checkbox_div.children[0] {
                    eprintln!("span props count: {:?}", span.props.len());
                    for prop in span.props.iter() {
                        match prop {
                            PropNode::Attribute(attr) => eprintln!("  attr: {}", attr.name),
                            PropNode::Directive(dir) => {
                                eprintln!("  directive: {} arg: {:?}", dir.name, dir.arg)
                            }
                        }
                    }
                }
            }
        }

        assert!(!is_static_node(&root.children[0]));
    }
}
