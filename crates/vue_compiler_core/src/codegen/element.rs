//! Element generation functions.

use crate::ast::*;
use crate::transforms::v_model::{get_vmodel_helper, parse_model_modifiers};

use super::children::generate_children;
use super::context::CodegenContext;
use super::expression::generate_expression;
use super::helpers::{escape_js_string, is_builtin_component};
use super::node::generate_node;
use super::patch_flag::{calculate_element_patch_info, patch_flag_name};
use super::props::{generate_props, is_supported_directive};
use super::slots::{generate_slots, has_dynamic_slots_flag, has_slot_children};
use super::v_for::generate_for;
use super::v_if::generate_if;

/// Check if element has v-once directive
pub fn has_v_once(el: &ElementNode<'_>) -> bool {
    el.props.iter().any(|prop| {
        if let PropNode::Directive(dir) = prop {
            dir.name.as_str() == "once"
        } else {
            false
        }
    })
}

/// Check if element has v-show directive
pub fn has_vshow_directive(el: &ElementNode<'_>) -> bool {
    el.props.iter().any(|prop| {
        if let PropNode::Directive(dir) = prop {
            dir.name.as_str() == "show"
        } else {
            false
        }
    })
}

/// Check if a directive is a built-in directive (not custom)
pub fn is_builtin_directive(name: &str) -> bool {
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

/// Check if element has custom directives
pub fn has_custom_directives(el: &ElementNode<'_>) -> bool {
    el.props.iter().any(|prop| {
        if let PropNode::Directive(dir) = prop {
            !is_builtin_directive(&dir.name)
        } else {
            false
        }
    })
}

/// Get custom directives from element
pub fn get_custom_directives<'a, 'b>(el: &'b ElementNode<'a>) -> Vec<&'b DirectiveNode<'a>> {
    el.props
        .iter()
        .filter_map(|prop| {
            if let PropNode::Directive(dir) = prop {
                if !is_builtin_directive(&dir.name) {
                    return Some(dir.as_ref());
                }
            }
            None
        })
        .collect()
}

/// Check if native element has v-model directive
pub fn has_vmodel_directive(el: &ElementNode<'_>) -> bool {
    // Only native elements use withDirectives for v-model
    if el.tag_type != ElementType::Element {
        return false;
    }
    // Only input, textarea, select support v-model
    if !matches!(el.tag.as_str(), "input" | "textarea" | "select") {
        return false;
    }
    el.props.iter().any(|prop| {
        if let PropNode::Directive(dir) = prop {
            dir.name.as_str() == "model"
        } else {
            false
        }
    })
}

/// Get v-model directive from element
fn get_vmodel_directive<'a, 'b>(el: &'b ElementNode<'a>) -> Option<&'b DirectiveNode<'a>> {
    el.props.iter().find_map(|prop| {
        if let PropNode::Directive(dir) = prop {
            if dir.name.as_str() == "model" {
                return Some(dir.as_ref());
            }
        }
        None
    })
}

/// Generate v-model directive closing
pub fn generate_vmodel_closing(ctx: &mut CodegenContext, el: &ElementNode<'_>) {
    if let Some(dir) = get_vmodel_directive(el) {
        let helper = get_vmodel_helper(el);
        ctx.use_helper(helper);

        ctx.push(", [");
        ctx.newline();

        // Check for modifiers
        let modifiers: Vec<_> = dir.modifiers.iter().map(|m| m.content.as_str()).collect();
        let parsed_mods = parse_model_modifiers(&dir.modifiers);
        let has_modifiers = parsed_mods.lazy || parsed_mods.number || parsed_mods.trim;

        if has_modifiers {
            // Count active modifiers
            let active_modifiers: Vec<_> = modifiers
                .iter()
                .filter(|m| matches!(*m, &"lazy" | &"number" | &"trim"))
                .collect();
            let is_single_modifier = active_modifiers.len() == 1;

            // Multi-line format with modifiers
            ctx.push("  [");
            ctx.newline();
            ctx.push("    ");
            ctx.push(ctx.helper(helper));
            ctx.push(",");
            ctx.newline();
            ctx.push("    ");
            // Value expression
            if let Some(exp) = &dir.exp {
                generate_expression(ctx, exp);
            }
            ctx.push(",");
            ctx.newline();
            ctx.push("    void 0,");
            ctx.newline();

            if is_single_modifier {
                // Single modifier: inline format { lazy: true }
                ctx.push("    { ");
                ctx.push(active_modifiers[0]);
                ctx.push(": true }");
            } else {
                // Multiple modifiers: multiline format
                ctx.push("    {");
                for (i, modifier) in active_modifiers.iter().enumerate() {
                    ctx.newline();
                    ctx.push("      ");
                    ctx.push(modifier);
                    ctx.push(": true");
                    if i < active_modifiers.len() - 1 {
                        ctx.push(",");
                    }
                }
                ctx.newline();
                ctx.push("    }");
            }
            ctx.newline();
            ctx.push("  ]");
        } else {
            // Simple format without modifiers
            ctx.push("  [");
            ctx.push(ctx.helper(helper));
            ctx.push(", ");
            if let Some(exp) = &dir.exp {
                generate_expression(ctx, exp);
            }
            ctx.push("]");
        }

        ctx.newline();
        ctx.push("])");
    }
}

/// Generate v-show directive closing if present
pub fn generate_vshow_closing(ctx: &mut CodegenContext, el: &ElementNode<'_>) {
    for prop in &el.props {
        if let PropNode::Directive(dir) = prop {
            if dir.name.as_str() == "show" {
                if let Some(exp) = &dir.exp {
                    ctx.push(", [");
                    ctx.newline();
                    ctx.push("  [");
                    ctx.push(ctx.helper(RuntimeHelper::VShow));
                    ctx.push(", ");
                    generate_expression(ctx, exp);
                    ctx.push("]");
                    ctx.newline();
                    ctx.push("])");
                }
                return;
            }
        }
    }
}

/// Generate custom directives closing
pub fn generate_custom_directives_closing(ctx: &mut CodegenContext, el: &ElementNode<'_>) {
    let custom_dirs = get_custom_directives(el);
    if custom_dirs.is_empty() {
        return;
    }

    ctx.push(", [");
    ctx.newline();

    for (i, dir) in custom_dirs.iter().enumerate() {
        if i > 0 {
            ctx.push(",");
            ctx.newline();
        }
        ctx.push("  [_directive_");
        ctx.push(&dir.name.replace('-', "_"));

        // Add value if present
        if let Some(exp) = &dir.exp {
            ctx.push(", ");
            generate_expression(ctx, exp);
        }

        // Add argument if present
        if let Some(arg) = &dir.arg {
            // Need to add value placeholder if not present
            if dir.exp.is_none() {
                ctx.push(", void 0");
            }
            ctx.push(", ");
            match arg {
                ExpressionNode::Simple(simple) => {
                    if simple.is_static {
                        ctx.push("\"");
                        ctx.push(&simple.content);
                        ctx.push("\"");
                    } else {
                        ctx.push(&simple.content);
                    }
                }
                ExpressionNode::Compound(compound) => {
                    ctx.push(&compound.loc.source);
                }
            }
        }

        // Add modifiers if present
        if !dir.modifiers.is_empty() {
            // Need to add placeholders if not present
            if dir.exp.is_none() && dir.arg.is_none() {
                ctx.push(", void 0, void 0");
            } else if dir.arg.is_none() {
                ctx.push(", void 0");
            }
            ctx.push(", { ");
            for (j, modifier) in dir.modifiers.iter().enumerate() {
                if j > 0 {
                    ctx.push(", ");
                }
                ctx.push(&modifier.content);
                ctx.push(": true");
            }
            ctx.push(" }");
        }

        ctx.push("]");
    }

    ctx.newline();
    ctx.push("])");
}

/// Check if element has any renderable props (excluding v-show and other handled-separately directives)
pub fn has_renderable_props(el: &ElementNode<'_>) -> bool {
    el.props.iter().any(|prop| match prop {
        PropNode::Attribute(_) => true,
        PropNode::Directive(dir) => is_supported_directive(dir),
    })
}

/// Generate v-once element with cache wrapper
pub fn generate_v_once_element(ctx: &mut CodegenContext, el: &ElementNode<'_>) {
    let cache_index = ctx.next_cache_index();

    ctx.use_helper(RuntimeHelper::SetBlockTracking);

    // _cache[0] || (...)
    ctx.push("_cache[");
    ctx.push(&cache_index.to_string());
    ctx.push("] || (");
    ctx.indent();
    ctx.newline();

    // _setBlockTracking(-1, true),
    ctx.push(ctx.helper(RuntimeHelper::SetBlockTracking));
    ctx.push("(-1, true),");
    ctx.newline();

    // (_cache[0] = _createElementVNode(...)).cacheIndex = 0,
    ctx.push("(_cache[");
    ctx.push(&cache_index.to_string());
    ctx.push("] = ");

    // Generate the element content
    if el.tag_type == ElementType::Component {
        ctx.use_helper(RuntimeHelper::CreateVNode);
        ctx.use_helper(RuntimeHelper::ResolveComponent);
        ctx.push(ctx.helper(RuntimeHelper::CreateVNode));
        ctx.push("(_component_");
        ctx.push(&el.tag.replace('-', "_"));
        ctx.push(")");
    } else {
        ctx.use_helper(RuntimeHelper::CreateElementVNode);
        ctx.push(ctx.helper(RuntimeHelper::CreateElementVNode));
        ctx.push("(\"");
        ctx.push(&el.tag);
        ctx.push("\"");

        // Generate props (excluding v-once)
        let has_props = el.props.iter().any(|p| match p {
            PropNode::Directive(dir) => dir.name != "once" && is_supported_directive(dir),
            PropNode::Attribute(_) => true,
        });

        if has_props {
            ctx.push(", ");
            generate_v_once_props(ctx, el);
        } else if !el.children.is_empty() {
            ctx.push(", null");
        }

        // Generate children
        if !el.children.is_empty() {
            ctx.push(", [");
            ctx.indent();
            for (i, child) in el.children.iter().enumerate() {
                if i > 0 {
                    ctx.push(",");
                }
                ctx.newline();
                generate_v_once_child(ctx, child);
            }
            ctx.deindent();
            ctx.newline();
            ctx.push("]");
        }

        // v-once still needs patch flag for dynamic bindings (class/style)
        let (patch_flag, _) =
            calculate_element_patch_info(el, ctx.options.binding_metadata.as_ref());
        if let Some(flag) = patch_flag {
            // Only emit CLASS/STYLE flags for v-once, ignore PROPS
            let filtered_flag = flag & (2 | 4); // CLASS | STYLE
            if filtered_flag > 0 {
                if el.children.is_empty() && !has_props {
                    ctx.push(", null");
                }
                ctx.push(", ");
                ctx.push(&format!(
                    "{} /* {} */",
                    filtered_flag,
                    patch_flag_name(filtered_flag)
                ));
            }
        }
        ctx.push(")");
    }

    ctx.push(").cacheIndex = ");
    ctx.push(&cache_index.to_string());
    ctx.push(",");
    ctx.newline();

    // _setBlockTracking(1),
    ctx.push(ctx.helper(RuntimeHelper::SetBlockTracking));
    ctx.push("(1),");
    ctx.newline();

    // _cache[0]
    ctx.push("_cache[");
    ctx.push(&cache_index.to_string());
    ctx.push("]");

    ctx.deindent();
    ctx.newline();
    ctx.push(")");
}

/// Generate props for v-once element (excludes v-once directive)
pub fn generate_v_once_props(ctx: &mut CodegenContext, el: &ElementNode<'_>) {
    ctx.push("{");
    ctx.indent();

    let mut first = true;
    for prop in &el.props {
        match prop {
            PropNode::Directive(dir) if dir.name == "once" => continue,
            PropNode::Directive(dir) if dir.name == "bind" => {
                if let Some(ExpressionNode::Simple(arg)) = &dir.arg {
                    if !first {
                        ctx.push(",");
                    }
                    first = false;
                    ctx.newline();

                    if arg.content == "class" {
                        ctx.use_helper(RuntimeHelper::NormalizeClass);
                        ctx.push("class: ");
                        ctx.push(ctx.helper(RuntimeHelper::NormalizeClass));
                        ctx.push("(");
                        if let Some(exp) = &dir.exp {
                            generate_expression(ctx, exp);
                        }
                        ctx.push(")");
                    } else if arg.content == "style" {
                        ctx.use_helper(RuntimeHelper::NormalizeStyle);
                        ctx.push("style: ");
                        ctx.push(ctx.helper(RuntimeHelper::NormalizeStyle));
                        ctx.push("(");
                        if let Some(exp) = &dir.exp {
                            generate_expression(ctx, exp);
                        }
                        ctx.push(")");
                    } else {
                        ctx.push(&arg.content);
                        ctx.push(": ");
                        if let Some(exp) = &dir.exp {
                            generate_expression(ctx, exp);
                        }
                    }
                }
            }
            PropNode::Attribute(attr) => {
                if !first {
                    ctx.push(",");
                }
                first = false;
                ctx.newline();
                ctx.push(&attr.name);
                ctx.push(": ");
                if let Some(value) = &attr.value {
                    ctx.push("\"");
                    ctx.push(&escape_js_string(&value.content));
                    ctx.push("\"");
                } else {
                    ctx.push("true");
                }
            }
            _ => {}
        }
    }

    ctx.deindent();
    ctx.newline();
    ctx.push("}");
}

/// Generate child node for v-once (uses createTextVNode instead of interpolation)
pub fn generate_v_once_child(ctx: &mut CodegenContext, node: &TemplateChildNode<'_>) {
    match node {
        TemplateChildNode::Text(text) => {
            ctx.use_helper(RuntimeHelper::CreateText);
            ctx.push(ctx.helper(RuntimeHelper::CreateText));
            ctx.push("(\"");
            ctx.push(&escape_js_string(&text.content));
            ctx.push("\")");
        }
        TemplateChildNode::Interpolation(interp) => {
            ctx.use_helper(RuntimeHelper::CreateText);
            ctx.use_helper(RuntimeHelper::ToDisplayString);
            ctx.push(ctx.helper(RuntimeHelper::CreateText));
            ctx.push("(");
            ctx.push(ctx.helper(RuntimeHelper::ToDisplayString));
            ctx.push("(");
            generate_expression(ctx, &interp.content);
            ctx.push("), 1 /* TEXT */)");
        }
        _ => generate_node(ctx, node),
    }
}

/// Generate element as a block
pub fn generate_element_block(ctx: &mut CodegenContext, el: &ElementNode<'_>) {
    // Check for v-once directive - handle it specially
    if has_v_once(el) {
        generate_v_once_element(ctx, el);
        return;
    }

    // Check for custom directives
    let has_custom_dirs = has_custom_directives(el);
    if has_custom_dirs {
        ctx.use_helper(RuntimeHelper::WithDirectives);
        ctx.use_helper(RuntimeHelper::ResolveDirective);
        ctx.push(ctx.helper(RuntimeHelper::WithDirectives));
        ctx.push("(");
    }

    // Check for v-model directive on native elements (only if no custom directives)
    let has_vmodel = has_vmodel_directive(el) && !has_custom_dirs;
    if has_vmodel {
        ctx.use_helper(RuntimeHelper::WithDirectives);
        ctx.push(ctx.helper(RuntimeHelper::WithDirectives));
        ctx.push("(");
    }

    // Check for v-show directive (only if no custom directives or vmodel)
    let has_vshow = has_vshow_directive(el) && !has_vmodel && !has_custom_dirs;
    if has_vshow {
        ctx.use_helper(RuntimeHelper::WithDirectives);
        ctx.use_helper(RuntimeHelper::VShow);
        ctx.push(ctx.helper(RuntimeHelper::WithDirectives));
        ctx.push("(");
    }

    // Slots are not blocks - handle them separately
    if el.tag_type == ElementType::Slot {
        let helper = ctx.helper(RuntimeHelper::RenderSlot);
        ctx.use_helper(RuntimeHelper::RenderSlot);
        ctx.push(helper);
        ctx.push("(_ctx.$slots, ");

        // Get slot name from props
        let slot_name = el
            .props
            .iter()
            .find_map(|p| match p {
                PropNode::Attribute(attr) if attr.name == "name" => {
                    attr.value.as_ref().map(|v| v.content.as_str())
                }
                _ => None,
            })
            .unwrap_or("default");
        ctx.push("\"");
        ctx.push(slot_name);
        ctx.push("\"");

        // Generate fallback content if present
        if !el.children.is_empty() {
            ctx.push(", {}, () => [");
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
            ctx.push("])");
        } else {
            ctx.push(")");
        }
        return;
    }

    // Track helpers for preamble
    ctx.use_helper(RuntimeHelper::OpenBlock);

    // Open block wrapper
    ctx.push("(");
    ctx.push(ctx.helper(RuntimeHelper::OpenBlock));
    ctx.push("(), ");

    match el.tag_type {
        ElementType::Element => {
            ctx.use_helper(RuntimeHelper::CreateElementBlock);
            ctx.push(ctx.helper(RuntimeHelper::CreateElementBlock));
            ctx.push("(\"");
            ctx.push(&el.tag);
            ctx.push("\"");

            // Calculate patch flag and dynamic props
            let (patch_flag, dynamic_props) =
                calculate_element_patch_info(el, ctx.options.binding_metadata.as_ref());
            let has_patch_info = patch_flag.is_some() || dynamic_props.is_some();

            // Generate props (only if there are renderable props, not just v-show)
            // If props are hoisted, use the hoisted reference
            if let Some(hoisted_index) = el.hoisted_props_index {
                ctx.push(", _hoisted_");
                ctx.push(&hoisted_index.to_string());
            } else if has_renderable_props(el) {
                ctx.push(", ");
                generate_props(ctx, &el.props);
            } else if !el.children.is_empty() || has_patch_info {
                ctx.push(", null");
            }

            // Generate children
            // When props are hoisted and only TEXT flag is set, omit the patch flag
            // (Vue optimizes block elements with hoisted static props)
            let should_emit_patch_flag = if let Some(flag) = patch_flag {
                !(el.hoisted_props_index.is_some() && flag == 1)
            } else {
                false
            };
            let effective_has_patch_info = has_patch_info && should_emit_patch_flag;
            if !el.children.is_empty() {
                ctx.push(", ");
                generate_children(ctx, &el.children);
            } else if effective_has_patch_info {
                ctx.push(", null");
            }

            // Generate patch flag
            if should_emit_patch_flag {
                if let Some(flag) = patch_flag {
                    ctx.push(", ");
                    ctx.push(&format!("{} /* {} */", flag, patch_flag_name(flag)));
                }
            }

            // Generate dynamic props
            if let Some(props) = dynamic_props {
                ctx.push(", [");
                for (i, prop) in props.iter().enumerate() {
                    if i > 0 {
                        ctx.push(", ");
                    }
                    ctx.push("\"");
                    ctx.push(prop);
                    ctx.push("\"");
                }
                ctx.push("]");
            }

            ctx.push("))");

            // Close withDirectives for custom directives
            if has_custom_dirs {
                generate_custom_directives_closing(ctx, el);
            }

            // Close withDirectives for v-model
            if has_vmodel {
                generate_vmodel_closing(ctx, el);
            }

            // Close withDirectives for v-show
            if has_vshow {
                generate_vshow_closing(ctx, el);
            }
        }
        ElementType::Component => {
            ctx.use_helper(RuntimeHelper::CreateBlock);
            ctx.push(ctx.helper(RuntimeHelper::CreateBlock));
            ctx.push("(");

            // Check for dynamic component (<component :is="..."> or <component is="...">)
            let is_dynamic_component = el.tag == "component";
            let (dynamic_is, static_is) = if is_dynamic_component {
                // Check for :is="..." (dynamic binding)
                let dynamic = el.props.iter().find_map(|p| {
                    if let PropNode::Directive(dir) = p {
                        if dir.name == "bind" {
                            if let Some(ExpressionNode::Simple(arg)) = &dir.arg {
                                if arg.content == "is" {
                                    return dir.exp.as_ref();
                                }
                            }
                        }
                    }
                    None
                });
                // Check for is="..." (static attribute)
                let static_val = el.props.iter().find_map(|p| {
                    if let PropNode::Attribute(attr) = p {
                        if attr.name == "is" {
                            return attr.value.as_ref().map(|v| v.content.as_str());
                        }
                    }
                    None
                });
                (dynamic, static_val)
            } else {
                (None, None)
            };

            if let Some(is_exp) = dynamic_is {
                // Dynamic component: resolveDynamicComponent(_ctx.component)
                ctx.use_helper(RuntimeHelper::ResolveDynamicComponent);
                ctx.push(ctx.helper(RuntimeHelper::ResolveDynamicComponent));
                ctx.push("(");
                generate_expression(ctx, is_exp);
                ctx.push(")");
            } else if let Some(component_name) = static_is {
                // Static is: resolveDynamicComponent("componentName")
                ctx.use_helper(RuntimeHelper::ResolveDynamicComponent);
                ctx.push(ctx.helper(RuntimeHelper::ResolveDynamicComponent));
                ctx.push("(\"");
                ctx.push(component_name);
                ctx.push("\")");
            } else if let Some(builtin) = is_builtin_component(&el.tag) {
                // Check for built-in components (Teleport, KeepAlive, Suspense)
                ctx.use_helper(builtin);
                ctx.push(ctx.helper(builtin));
            } else if ctx.is_component_in_bindings(&el.tag) {
                // Component is in binding metadata (from script setup imports)
                // In inline mode, reference directly; in non-inline mode, use $setup. prefix
                if !ctx.options.inline {
                    ctx.push("$setup.");
                }
                ctx.push(&el.tag);
            } else {
                ctx.push("_component_");
                ctx.push(&el.tag.replace('-', "_"));
            }

            // Calculate patch flag and dynamic props for component
            let (mut patch_flag, dynamic_props) =
                calculate_element_patch_info(el, ctx.options.binding_metadata.as_ref());

            // For components with slot children, remove TEXT flag (1) since text is inside slot
            if has_slot_children(el) {
                if let Some(flag) = patch_flag {
                    let new_flag = flag & !1; // Remove TEXT flag
                    patch_flag = if new_flag > 0 { Some(new_flag) } else { None };
                }
            }

            // Add DYNAMIC_SLOTS flag (1024) if component has dynamic slots
            if has_dynamic_slots_flag(el) {
                let dynamic_slots_flag = 1024;
                patch_flag = Some(patch_flag.unwrap_or(0) | dynamic_slots_flag);
            }

            let has_patch_info = patch_flag.is_some() || dynamic_props.is_some();

            // Generate props (only if there are renderable props, not just v-show)
            if has_renderable_props(el) {
                ctx.push(", ");
                generate_props(ctx, &el.props);
            } else if !el.children.is_empty() || has_patch_info {
                ctx.push(", null");
            }

            // Generate children/slots - use slot generation for component children
            if has_slot_children(el) {
                ctx.push(", ");
                generate_slots(ctx, el);
            } else if has_patch_info {
                ctx.push(", null");
            }

            // Generate patch flag
            if let Some(flag) = patch_flag {
                ctx.push(", ");
                ctx.push(&format!("{} /* {} */", flag, patch_flag_name(flag)));
            }

            // Generate dynamic props
            if let Some(props) = dynamic_props {
                ctx.push(", [");
                for (i, prop) in props.iter().enumerate() {
                    if i > 0 {
                        ctx.push(", ");
                    }
                    ctx.push("\"");
                    ctx.push(prop);
                    ctx.push("\"");
                }
                ctx.push("]");
            }

            ctx.push("))");

            // Close withDirectives for custom directives on component
            if has_custom_dirs {
                generate_custom_directives_closing(ctx, el);
            }

            // Close withDirectives for v-show on component
            if has_vshow {
                generate_vshow_closing(ctx, el);
            }
        }
        ElementType::Slot => {
            // Slots don't use blocks - use renderSlot directly
            let helper = ctx.helper(RuntimeHelper::RenderSlot);
            ctx.use_helper(RuntimeHelper::RenderSlot);
            ctx.push(helper);
            ctx.push("(_ctx.$slots, ");

            // Get slot name from props
            let slot_name = el
                .props
                .iter()
                .find_map(|p| match p {
                    PropNode::Attribute(attr) if attr.name == "name" => {
                        attr.value.as_ref().map(|v| v.content.as_str())
                    }
                    _ => None,
                })
                .unwrap_or("default");
            ctx.push("\"");
            ctx.push(slot_name);
            ctx.push("\"");

            // Generate fallback content if present
            if !el.children.is_empty() {
                ctx.push(", {}, () => [");
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
                ctx.push("])");
            } else {
                ctx.push(")");
            }
        }
        ElementType::Template => {
            ctx.use_helper(RuntimeHelper::CreateElementBlock);
            ctx.use_helper(RuntimeHelper::Fragment);
            ctx.push(ctx.helper(RuntimeHelper::CreateElementBlock));
            ctx.push("(");
            ctx.push(ctx.helper(RuntimeHelper::Fragment));
            ctx.push(", null, ");
            generate_children(ctx, &el.children);
            ctx.push("))");
        }
    }
}

/// Generate element code
pub fn generate_element(ctx: &mut CodegenContext, el: &ElementNode<'_>) {
    match el.tag_type {
        ElementType::Element => {
            // Check for v-show directive
            let has_vshow = has_vshow_directive(el);
            if has_vshow {
                ctx.use_helper(RuntimeHelper::WithDirectives);
                ctx.use_helper(RuntimeHelper::VShow);
                ctx.push(ctx.helper(RuntimeHelper::WithDirectives));
                ctx.push("(");
            }

            ctx.push_pure();
            let helper = ctx.helper(RuntimeHelper::CreateElementVNode);
            ctx.use_helper(RuntimeHelper::CreateElementVNode);
            ctx.push(helper);
            ctx.push("(\"");
            ctx.push(&el.tag);
            ctx.push("\"");

            // Calculate patch flag for v-show (NEED_PATCH)
            let (patch_flag, _) =
                calculate_element_patch_info(el, ctx.options.binding_metadata.as_ref());
            let has_patch_info = patch_flag.is_some();

            // Generate props (only if there are renderable props, not just v-show)
            // If props are hoisted, use the hoisted reference
            if let Some(hoisted_index) = el.hoisted_props_index {
                ctx.push(", _hoisted_");
                ctx.push(&hoisted_index.to_string());
            } else if has_renderable_props(el) {
                ctx.push(", ");
                generate_props(ctx, &el.props);
            } else if !el.children.is_empty() || has_patch_info {
                ctx.push(", null");
            }

            // Generate children
            if !el.children.is_empty() {
                ctx.push(", ");
                generate_children(ctx, &el.children);
            } else if has_patch_info {
                ctx.push(", null");
            }

            // Generate patch flag for v-show
            if let Some(flag) = patch_flag {
                ctx.push(", ");
                ctx.push(&format!("{} /* {} */", flag, patch_flag_name(flag)));
            }

            ctx.push(")");

            // Close withDirectives for v-show
            if has_vshow {
                generate_vshow_closing(ctx, el);
            }
        }
        ElementType::Component => {
            ctx.push_pure();
            let helper = ctx.helper(RuntimeHelper::CreateVNode);
            ctx.use_helper(RuntimeHelper::CreateVNode);
            ctx.push(helper);
            ctx.push("(");

            // Component reference: use $setup. prefix only in non-inline mode
            if ctx.is_component_in_bindings(&el.tag) {
                if !ctx.options.inline {
                    ctx.push("$setup.");
                }
                ctx.push(&el.tag);
            } else {
                ctx.push("_component_");
                ctx.push(&el.tag.replace('-', "_"));
            }

            // Generate props
            if !el.props.is_empty() {
                ctx.push(", ");
                generate_props(ctx, &el.props);
            } else if !el.children.is_empty() {
                ctx.push(", null");
            }

            // Generate children/slots - use slot generation for component children
            if has_slot_children(el) {
                ctx.push(", ");
                generate_slots(ctx, el);
            }

            ctx.push(")");
        }
        ElementType::Slot => {
            let helper = ctx.helper(RuntimeHelper::RenderSlot);
            ctx.use_helper(RuntimeHelper::RenderSlot);
            ctx.push(helper);
            ctx.push("(_ctx.$slots, ");
            // Get slot name from props
            let slot_name = el
                .props
                .iter()
                .find_map(|p| match p {
                    PropNode::Attribute(attr) if attr.name == "name" => {
                        attr.value.as_ref().map(|v| v.content.as_str())
                    }
                    _ => None,
                })
                .unwrap_or("default");
            ctx.push("\"");
            ctx.push(slot_name);
            ctx.push("\"");

            // Generate slot props
            let slot_props: Vec<_> = el
                .props
                .iter()
                .filter(|p| match p {
                    PropNode::Attribute(attr) => attr.name != "name",
                    PropNode::Directive(_) => true,
                })
                .collect();

            if !slot_props.is_empty() {
                ctx.push(", ");
                generate_props(ctx, &el.props);
            }

            ctx.push(")");
        }
        ElementType::Template => {
            // Template elements render their children directly
            if el.children.len() == 1 {
                generate_node(ctx, &el.children[0]);
            } else {
                ctx.push("[");
                for (i, child) in el.children.iter().enumerate() {
                    if i > 0 {
                        ctx.push(", ");
                    }
                    generate_node(ctx, child);
                }
                ctx.push("]");
            }
        }
    }
}

/// Generate root node (wrapped in block)
pub fn generate_root_node(ctx: &mut CodegenContext, node: &TemplateChildNode<'_>) {
    match node {
        TemplateChildNode::Element(el) => generate_element_block(ctx, el),
        TemplateChildNode::If(if_node) => generate_if(ctx, if_node),
        TemplateChildNode::For(for_node) => generate_for(ctx, for_node),
        _ => generate_node(ctx, node),
    }
}
