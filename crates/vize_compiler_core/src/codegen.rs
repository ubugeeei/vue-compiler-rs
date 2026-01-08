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
mod slots;
mod v_for;
mod v_if;

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
    // Only include specific helpers from root.helpers that are known to be
    // added during transform but not tracked during codegen (like Unref)
    // We don't merge ALL root.helpers because transform may add helpers that
    // get optimized away during codegen (e.g., createElementVNode -> createElementBlock)
    let mut all_helpers: Vec<RuntimeHelper> = ctx.used_helpers.iter().copied().collect();
    if root.helpers.contains(&RuntimeHelper::Unref) && !all_helpers.contains(&RuntimeHelper::Unref)
    {
        all_helpers.push(RuntimeHelper::Unref);
    }
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
        code: ctx.into_code(),
        preamble,
        map: None,
    }
}

/// Generate preamble from a list of helpers
fn generate_preamble_from_helpers(ctx: &CodegenContext, helpers: &[RuntimeHelper]) -> String {
    if helpers.is_empty() {
        return String::new();
    }

    // Pre-calculate capacity: each helper needs ~20 chars on average
    let estimated_capacity = 32 + helpers.len() * 24;
    let mut preamble = Vec::with_capacity(estimated_capacity);

    match ctx.options.mode {
        crate::options::CodegenMode::Module => {
            // ES module imports - build string directly without intermediate Vec
            preamble.extend_from_slice(b"import { ");
            for (i, h) in helpers.iter().enumerate() {
                if i > 0 {
                    preamble.extend_from_slice(b", ");
                }
                preamble.extend_from_slice(h.name().as_bytes());
                preamble.extend_from_slice(b" as ");
                preamble.extend_from_slice(ctx.helper(*h).as_bytes());
            }
            preamble.extend_from_slice(b" } from \"");
            preamble.extend_from_slice(ctx.runtime_module_name.as_bytes());
            preamble.extend_from_slice(b"\"\n");
        }
        crate::options::CodegenMode::Function => {
            // Destructuring from global - build string directly without intermediate Vec
            preamble.extend_from_slice(b"const { ");
            for (i, h) in helpers.iter().enumerate() {
                if i > 0 {
                    preamble.extend_from_slice(b", ");
                }
                preamble.extend_from_slice(h.name().as_bytes());
                preamble.extend_from_slice(b": ");
                preamble.extend_from_slice(ctx.helper(*h).as_bytes());
            }
            preamble.extend_from_slice(b" } = ");
            preamble.extend_from_slice(ctx.runtime_global_name.as_bytes());
            preamble.push(b'\n');
        }
    }

    // SAFETY: We only push valid UTF-8 strings
    unsafe { String::from_utf8_unchecked(preamble) }
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
    let mut hoists_code: Vec<u8> = Vec::new();

    for (i, hoist) in root.hoists.iter().enumerate() {
        if let Some(node) = hoist {
            hoists_code.extend_from_slice(b"const _hoisted_");
            hoists_code.extend_from_slice((i + 1).to_string().as_bytes());
            hoists_code.extend_from_slice(b" = ");
            // Only add /*#__PURE__*/ for VNodeCall (createElementVNode calls)
            if matches!(node, JsChildNode::VNodeCall(_)) {
                hoists_code.extend_from_slice(b"/*#__PURE__*/ ");
            }
            generate_js_child_node_to_bytes(ctx, node, &mut hoists_code);
            hoists_code.push(b'\n');
        }
    }

    // SAFETY: We only push valid UTF-8 strings
    unsafe { String::from_utf8_unchecked(hoists_code) }
}

/// Generate JsChildNode to bytes
fn generate_js_child_node_to_bytes(
    ctx: &CodegenContext,
    node: &JsChildNode<'_>,
    out: &mut Vec<u8>,
) {
    match node {
        JsChildNode::VNodeCall(vnode) => generate_vnode_call_to_bytes(ctx, vnode, out),
        JsChildNode::SimpleExpression(exp) => {
            if exp.is_static {
                out.push(b'"');
                out.extend_from_slice(exp.content.as_bytes());
                out.push(b'"');
            } else {
                // Expression should already be processed by transform
                out.extend_from_slice(exp.content.as_bytes());
            }
        }
        JsChildNode::Object(obj) => {
            out.extend_from_slice(b"{ ");
            for (i, prop) in obj.properties.iter().enumerate() {
                if i > 0 {
                    out.extend_from_slice(b", ");
                }
                // Key
                match &prop.key {
                    ExpressionNode::Simple(exp) => {
                        out.extend_from_slice(exp.content.as_bytes());
                        out.extend_from_slice(b": ");
                    }
                    ExpressionNode::Compound(_) => out.extend_from_slice(b"null: "),
                }
                // Value
                generate_js_child_node_to_bytes(ctx, &prop.value, out);
            }
            out.extend_from_slice(b" }");
        }
        _ => out.extend_from_slice(b"null /* unsupported */"),
    }
}

/// Generate VNodeCall to bytes
fn generate_vnode_call_to_bytes(ctx: &CodegenContext, vnode: &VNodeCall<'_>, out: &mut Vec<u8>) {
    // Block nodes use openBlock + createBlock/createElementBlock
    if vnode.is_block {
        out.push(b'(');
        out.extend_from_slice(ctx.helper(RuntimeHelper::OpenBlock).as_bytes());
        out.extend_from_slice(b"(), ");
        if vnode.is_component {
            out.extend_from_slice(ctx.helper(RuntimeHelper::CreateBlock).as_bytes());
        } else {
            out.extend_from_slice(ctx.helper(RuntimeHelper::CreateElementBlock).as_bytes());
        }
    } else if vnode.is_component {
        out.extend_from_slice(ctx.helper(RuntimeHelper::CreateVNode).as_bytes());
    } else {
        out.extend_from_slice(ctx.helper(RuntimeHelper::CreateElementVNode).as_bytes());
    }
    out.push(b'(');

    // Tag
    match &vnode.tag {
        VNodeTag::String(s) => {
            out.push(b'"');
            out.extend_from_slice(s.as_bytes());
            out.push(b'"');
        }
        VNodeTag::Symbol(helper) => out.extend_from_slice(ctx.helper(*helper).as_bytes()),
        VNodeTag::Call(_) => out.extend_from_slice(b"null"),
    }

    // Props
    if let Some(props) = &vnode.props {
        out.extend_from_slice(b", ");
        generate_props_expression_to_bytes(ctx, props, out);
    } else if vnode.children.is_some() || vnode.patch_flag.is_some() {
        out.extend_from_slice(b", null");
    }

    // Children
    if let Some(children) = &vnode.children {
        out.extend_from_slice(b", ");
        generate_vnode_children_to_bytes(ctx, children, out);
    } else if vnode.patch_flag.is_some() {
        out.extend_from_slice(b", null");
    }

    // Patch flag
    if let Some(patch_flag) = &vnode.patch_flag {
        out.extend_from_slice(b", ");
        out.extend_from_slice(patch_flag.bits().to_string().as_bytes());
        out.extend_from_slice(b" /* ");
        out.extend_from_slice(format!("{:?}", patch_flag).as_bytes());
        out.extend_from_slice(b" */");
    }

    // Dynamic props
    if let Some(dynamic_props) = &vnode.dynamic_props {
        out.extend_from_slice(b", ");
        match dynamic_props {
            DynamicProps::String(s) => {
                out.extend_from_slice(s.as_bytes());
            }
            DynamicProps::Simple(exp) => {
                out.extend_from_slice(exp.content.as_bytes());
            }
        }
    }

    out.push(b')');

    // Close block wrapper
    if vnode.is_block {
        out.push(b')');
    }
}

/// Generate PropsExpression to bytes
fn generate_props_expression_to_bytes(
    ctx: &CodegenContext,
    props: &PropsExpression<'_>,
    out: &mut Vec<u8>,
) {
    match props {
        PropsExpression::Object(obj) => {
            out.extend_from_slice(b"{ ");
            for (i, prop) in obj.properties.iter().enumerate() {
                if i > 0 {
                    out.extend_from_slice(b", ");
                }
                // Key (no quotes for valid identifiers)
                match &prop.key {
                    ExpressionNode::Simple(exp) => {
                        out.extend_from_slice(exp.content.as_bytes());
                        out.extend_from_slice(b": ");
                    }
                    ExpressionNode::Compound(_) => out.extend_from_slice(b"null: "),
                }
                // Value
                generate_js_child_node_to_bytes(ctx, &prop.value, out);
            }
            out.extend_from_slice(b" }");
        }
        PropsExpression::Simple(exp) => {
            if exp.is_static {
                out.push(b'"');
                out.extend_from_slice(exp.content.as_bytes());
                out.push(b'"');
            } else {
                // Expression should already be processed by transform
                out.extend_from_slice(exp.content.as_bytes());
            }
        }
        PropsExpression::Call(_) => out.extend_from_slice(b"null"),
    }
}

/// Generate VNodeChildren to bytes
fn generate_vnode_children_to_bytes(
    _ctx: &CodegenContext,
    children: &VNodeChildren<'_>,
    out: &mut Vec<u8>,
) {
    match children {
        VNodeChildren::Single(text_child) => match text_child {
            TemplateTextChildNode::Text(text) => {
                out.push(b'"');
                out.extend_from_slice(escape_js_string(&text.content).as_bytes());
                out.push(b'"');
            }
            TemplateTextChildNode::Interpolation(_) => out.extend_from_slice(b"null"),
            TemplateTextChildNode::Compound(_) => out.extend_from_slice(b"null"),
        },
        VNodeChildren::Simple(exp) => {
            if exp.is_static {
                out.push(b'"');
                out.extend_from_slice(escape_js_string(&exp.content).as_bytes());
                out.push(b'"');
            } else {
                // Expression should already be processed by transform
                out.extend_from_slice(exp.content.as_bytes());
            }
        }
        _ => out.extend_from_slice(b"null"),
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

        // Skip dynamic component (<component :is="...">) - it uses resolveDynamicComponent
        if component == "component" {
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
        let options = super::CodegenOptions {
            mode: CodegenMode::Module,
            ..Default::default()
        };
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

    #[test]
    fn test_codegen_slot_fallback() {
        // Slot element with fallback content should include fallback function
        assert_codegen!(r#"<slot name="label">{{ label }}</slot>"# => contains: [
            "_renderSlot",
            "\"label\"",
            "{}"
        ]);
        // Check that the fallback function is present
        let result = compile!(r#"<slot name="label">{{ label }}</slot>"#);
        assert!(
            result.code.contains("() => ["),
            "Should have fallback function: {}",
            result.code
        );
        assert!(
            result.code.contains("_toDisplayString"),
            "Should have toDisplayString for interpolation: {}",
            result.code
        );
    }

    #[test]
    fn test_codegen_slot_without_fallback() {
        // Slot element without fallback should not have empty object or function
        let result = compile!(r#"<slot name="header"></slot>"#);
        assert!(
            result.code.contains("_renderSlot"),
            "Should have renderSlot"
        );
        assert!(result.code.contains("\"header\""), "Should have slot name");
        // Should not have fallback function
        assert!(
            !result.code.contains("() => ["),
            "Should not have fallback function for empty slot: {}",
            result.code
        );
    }
}
