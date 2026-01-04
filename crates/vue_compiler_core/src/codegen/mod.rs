//! VDom code generation.
//!
//! This module generates JavaScript render function code from the transformed AST.

mod children;
mod context;
mod element;
mod expression;
mod helpers;
mod node;
mod patch_flag;
mod props;
mod v_for;
mod v_if;

use vue_allocator::String;

use crate::ast::*;
use crate::options::CodegenOptions;

pub use context::{CodegenContext, CodegenResult};
use element::generate_root_node;
use helpers::escape_js_string;
use node::generate_node;

/// Generate code from root AST
pub fn generate(root: &RootNode<'_>, options: CodegenOptions) -> CodegenResult {
    let mut ctx = CodegenContext::new(options);

    // Generate function signature
    generate_function_signature(&mut ctx);

    // Generate body
    ctx.indent();
    ctx.newline();

    // Generate component/directive resolution
    generate_assets(&mut ctx, root);

    // Generate return statement
    ctx.push("return ");

    // Generate root node
    if root.children.is_empty() {
        ctx.push("null");
    } else if root.children.len() == 1 {
        // Single root child - wrap in block
        generate_root_node(&mut ctx, &root.children[0]);
    } else {
        // Multiple root children - wrap in fragment block
        ctx.use_helper(RuntimeHelper::OpenBlock);
        ctx.use_helper(RuntimeHelper::CreateElementBlock);
        ctx.use_helper(RuntimeHelper::Fragment);
        ctx.push("(");
        ctx.push(ctx.helper(RuntimeHelper::OpenBlock));
        ctx.push("(), ");
        ctx.push(ctx.helper(RuntimeHelper::CreateElementBlock));
        ctx.push("(");
        ctx.push(ctx.helper(RuntimeHelper::Fragment));
        ctx.push(", null, [");
        ctx.indent();
        for (i, child) in root.children.iter().enumerate() {
            if i > 0 {
                ctx.push(",");
            }
            ctx.newline();
            generate_node(&mut ctx, child);
        }
        ctx.deindent();
        ctx.newline();
        ctx.push("], 64 /* STABLE_FRAGMENT */))");
    }

    ctx.deindent();
    ctx.newline();
    ctx.push("}");

    // Now generate preamble after we know all used helpers
    // Only use helpers that were actually used during codegen
    // (root.helpers from transform may include unused helpers like createVNode when we use createBlock)
    let mut all_helpers: Vec<RuntimeHelper> = ctx.used_helpers.iter().copied().collect();
    // Sort helpers for consistent output order
    all_helpers.sort();

    let mut preamble = generate_preamble_from_helpers(&ctx, &all_helpers);

    // Generate hoisted variable declarations (appended to preamble)
    let hoists_code = generate_hoists(&ctx, root);
    if !hoists_code.is_empty() {
        preamble.push('\n');
        preamble.push_str(&hoists_code);
    }

    CodegenResult {
        code: ctx.code,
        preamble,
        map: None,
    }
}

/// Generate preamble from a list of helpers
fn generate_preamble_from_helpers(ctx: &CodegenContext, helpers: &[RuntimeHelper]) -> String {
    if helpers.is_empty() {
        return String::default();
    }

    // Pre-calculate capacity: each helper needs ~20 chars on average
    let estimated_capacity = 32 + helpers.len() * 24;
    let mut preamble = String::with_capacity(estimated_capacity);

    match ctx.options.mode {
        crate::options::CodegenMode::Module => {
            // ES module imports - build string directly without intermediate Vec
            preamble.push_str("import { ");
            for (i, h) in helpers.iter().enumerate() {
                if i > 0 {
                    preamble.push_str(", ");
                }
                preamble.push_str(h.name());
                preamble.push_str(" as ");
                preamble.push_str(ctx.helper(*h));
            }
            preamble.push_str(" } from \"");
            preamble.push_str(&ctx.runtime_module_name);
            preamble.push_str("\"\n");
        }
        crate::options::CodegenMode::Function => {
            // Destructuring from global - build string directly without intermediate Vec
            preamble.push_str("const { ");
            for (i, h) in helpers.iter().enumerate() {
                if i > 0 {
                    preamble.push_str(", ");
                }
                preamble.push_str(h.name());
                preamble.push_str(": ");
                preamble.push_str(ctx.helper(*h));
            }
            preamble.push_str(" } = ");
            preamble.push_str(&ctx.runtime_global_name);
            preamble.push('\n');
        }
    }

    preamble
}

/// Generate function signature
fn generate_function_signature(ctx: &mut CodegenContext) {
    if ctx.options.ssr {
        ctx.push("function ssrRender(_ctx, _push, _parent, _attrs) {");
    } else {
        match ctx.options.mode {
            crate::options::CodegenMode::Module => {
                // Module mode: export with simpler signature
                ctx.push("export function render(_ctx, _cache) {");
            }
            crate::options::CodegenMode::Function => {
                // Function mode: include $props and $setup
                ctx.push("function render(_ctx, _cache, $props, $setup) {");
            }
        }
    }
}

/// Generate hoisted variable declarations
fn generate_hoists(ctx: &CodegenContext, root: &RootNode<'_>) -> String {
    let mut hoists_code = String::default();

    for (i, hoist) in root.hoists.iter().enumerate() {
        if let Some(node) = hoist {
            hoists_code.push_str(&format!("const _hoisted_{} = /*#__PURE__*/ ", i + 1));
            generate_js_child_node_to_string(ctx, node, &mut hoists_code);
            hoists_code.push('\n');
        }
    }

    hoists_code
}

/// Generate JsChildNode to a string
fn generate_js_child_node_to_string(
    ctx: &CodegenContext,
    node: &JsChildNode<'_>,
    out: &mut String,
) {
    match node {
        JsChildNode::VNodeCall(vnode) => generate_vnode_call_to_string(ctx, vnode, out),
        JsChildNode::SimpleExpression(exp) => {
            if exp.is_static {
                out.push('"');
                out.push_str(&exp.content);
                out.push('"');
            } else {
                // Expression should already be processed by transform
                out.push_str(&exp.content);
            }
        }
        _ => out.push_str("null /* unsupported */"),
    }
}

/// Generate VNodeCall to a string
fn generate_vnode_call_to_string(ctx: &CodegenContext, vnode: &VNodeCall<'_>, out: &mut String) {
    // Block nodes use openBlock + createBlock/createElementBlock
    if vnode.is_block {
        out.push('(');
        out.push_str(ctx.helper(RuntimeHelper::OpenBlock));
        out.push_str("(), ");
        if vnode.is_component {
            out.push_str(ctx.helper(RuntimeHelper::CreateBlock));
        } else {
            out.push_str(ctx.helper(RuntimeHelper::CreateElementBlock));
        }
    } else if vnode.is_component {
        out.push_str(ctx.helper(RuntimeHelper::CreateVNode));
    } else {
        out.push_str(ctx.helper(RuntimeHelper::CreateElementVNode));
    }
    out.push('(');

    // Tag
    match &vnode.tag {
        VNodeTag::String(s) => {
            out.push('"');
            out.push_str(s);
            out.push('"');
        }
        VNodeTag::Symbol(helper) => out.push_str(ctx.helper(*helper)),
        VNodeTag::Call(_) => out.push_str("null"),
    }

    // Props
    if let Some(props) = &vnode.props {
        out.push_str(", ");
        generate_props_expression_to_string(ctx, props, out);
    } else if vnode.children.is_some() || vnode.patch_flag.is_some() {
        out.push_str(", null");
    }

    // Children
    if let Some(children) = &vnode.children {
        out.push_str(", ");
        generate_vnode_children_to_string(ctx, children, out);
    } else if vnode.patch_flag.is_some() {
        out.push_str(", null");
    }

    // Patch flag
    if let Some(patch_flag) = &vnode.patch_flag {
        out.push_str(", ");
        out.push_str(&patch_flag.bits().to_string());
        out.push_str(" /* ");
        out.push_str(&format!("{:?}", patch_flag));
        out.push_str(" */");
    }

    // Dynamic props
    if let Some(dynamic_props) = &vnode.dynamic_props {
        out.push_str(", ");
        match dynamic_props {
            DynamicProps::String(s) => {
                out.push_str(s);
            }
            DynamicProps::Simple(exp) => {
                out.push_str(&exp.content);
            }
        }
    }

    out.push(')');

    // Close block wrapper
    if vnode.is_block {
        out.push(')');
    }
}

/// Generate PropsExpression to a string
fn generate_props_expression_to_string(
    ctx: &CodegenContext,
    props: &PropsExpression<'_>,
    out: &mut String,
) {
    match props {
        PropsExpression::Object(obj) => {
            out.push_str("{ ");
            for (i, prop) in obj.properties.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                // Key (no quotes for valid identifiers)
                match &prop.key {
                    ExpressionNode::Simple(exp) => {
                        out.push_str(&exp.content);
                        out.push_str(": ");
                    }
                    ExpressionNode::Compound(_) => out.push_str("null: "),
                }
                // Value
                generate_js_child_node_to_string(ctx, &prop.value, out);
            }
            out.push_str(" }");
        }
        PropsExpression::Simple(exp) => {
            if exp.is_static {
                out.push('"');
                out.push_str(&exp.content);
                out.push('"');
            } else {
                // Expression should already be processed by transform
                out.push_str(&exp.content);
            }
        }
        PropsExpression::Call(_) => out.push_str("null"),
    }
}

/// Generate VNodeChildren to a string
fn generate_vnode_children_to_string(
    _ctx: &CodegenContext,
    children: &VNodeChildren<'_>,
    out: &mut String,
) {
    match children {
        VNodeChildren::Single(text_child) => match text_child {
            TemplateTextChildNode::Text(text) => {
                out.push('"');
                out.push_str(&escape_js_string(&text.content));
                out.push('"');
            }
            TemplateTextChildNode::Interpolation(_) => out.push_str("null"),
            TemplateTextChildNode::Compound(_) => out.push_str("null"),
        },
        VNodeChildren::Simple(exp) => {
            if exp.is_static {
                out.push('"');
                out.push_str(&escape_js_string(&exp.content));
                out.push('"');
            } else {
                // Expression should already be processed by transform
                out.push_str(&exp.content);
            }
        }
        _ => out.push_str("null"),
    }
}

/// Generate asset resolution (components, directives)
fn generate_assets(ctx: &mut CodegenContext, root: &RootNode<'_>) {
    let mut has_resolved_assets = false;

    // Resolve components (only those not in binding metadata)
    for component in root.components.iter() {
        // Skip components that are in binding metadata (from script setup imports)
        if ctx.is_component_in_bindings(component) {
            continue;
        }

        // Skip built-in components - they are imported directly, not resolved
        if helpers::is_builtin_component(component).is_some() {
            continue;
        }

        ctx.use_helper(RuntimeHelper::ResolveComponent);
        ctx.push("const _component_");
        ctx.push(&component.replace('-', "_"));
        ctx.push(" = ");
        ctx.push(ctx.helper(RuntimeHelper::ResolveComponent));
        ctx.push("(\"");
        ctx.push(component);
        ctx.push("\")");
        ctx.newline();
        has_resolved_assets = true;
    }

    // Resolve directives
    for directive in root.directives.iter() {
        ctx.use_helper(RuntimeHelper::ResolveDirective);
        ctx.push("const _directive_");
        ctx.push(&directive.replace('-', "_"));
        ctx.push(" = ");
        ctx.push(ctx.helper(RuntimeHelper::ResolveDirective));
        ctx.push("(\"");
        ctx.push(directive);
        ctx.push("\")");
        ctx.newline();
        has_resolved_assets = true;
    }

    if has_resolved_assets {
        ctx.newline();
    }
}

#[cfg(test)]
mod tests {
    use crate::{assert_codegen, compile};

    #[test]
    fn test_codegen_simple_element() {
        assert_codegen!("<div>hello</div>" => contains: [
            "_createElementBlock",
            "\"div\"",
            "\"hello\""
        ]);
    }

    #[test]
    fn test_codegen_interpolation() {
        // When prefix_identifiers is false (default), expressions are not prefixed with _ctx.
        assert_codegen!("<div>{{ msg }}</div>" => contains: [
            "_toDisplayString",
            "msg"
        ]);
    }

    #[test]
    fn test_codegen_with_props() {
        assert_codegen!(r#"<div id="app" class="container"></div>"# => contains: [
            "id: \"app\"",
            "class: \"container\""
        ]);
    }

    #[test]
    fn test_codegen_component() {
        assert_codegen!("<MyComponent />" => contains: [
            "_resolveComponent",
            "_createBlock",
            "_component_MyComponent"
        ]);
    }

    #[test]
    fn test_codegen_preamble_module() {
        use crate::options::CodegenMode;
        let mut options = super::CodegenOptions::default();
        options.mode = CodegenMode::Module;
        let result = compile!("<div>hello</div>", options);
        assert!(result.preamble.contains("import {"));
        assert!(result.preamble.contains("from \"vue\""));
    }

    #[test]
    fn test_codegen_v_model_on_component() {
        // v-model on component should expand to modelValue + onUpdate:modelValue
        assert_codegen!(r#"<MyComponent v-model="msg" />"# => contains: [
            "_createBlock",
            "_component_MyComponent",
            "modelValue:",
            "msg",
            "\"onUpdate:modelValue\":"
        ]);
    }

    #[test]
    fn test_codegen_v_model_with_arg() {
        // v-model:title should expand to title + onUpdate:title
        assert_codegen!(r#"<MyComponent v-model:title="pageTitle" />"# => contains: [
            "title:",
            "pageTitle",
            "\"onUpdate:title\":"
        ]);
    }

    #[test]
    fn test_codegen_v_model_on_input() {
        // v-model on input uses withDirectives + vModelText
        assert_codegen!(r#"<input v-model="inputValue" />"# => contains: [
            "_withDirectives",
            "_vModelText",
            "inputValue",
            "\"onUpdate:modelValue\":"
        ]);
    }

    #[test]
    fn test_codegen_v_model_with_other_props() {
        // v-model with other props should not produce comments
        let result = compile!(r#"<MonacoEditor v-model="source" :language="editorLanguage" />"#);
        // Should NOT contain /* v-model */
        assert!(
            !result.code.contains("/* v-model */"),
            "Should not contain v-model comment"
        );
        // Should contain the expanded props
        assert!(
            result.code.contains("modelValue:"),
            "Should have modelValue prop"
        );
        assert!(
            result.code.contains("\"onUpdate:modelValue\":"),
            "Should have onUpdate:modelValue prop"
        );
        assert!(
            result.code.contains("language:"),
            "Should have language prop"
        );
    }
}
