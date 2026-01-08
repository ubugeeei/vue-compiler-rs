//! Function mode script compilation.
//!
//! This module handles compilation of script setup in function mode,
//! where the setup function returns bindings for use by a separate render function.

use std::collections::HashSet;

use vize_allocator::Bump;

use crate::script::{
    resolve_template_used_identifiers, transform_destructured_props, ScriptCompileContext,
    TemplateUsedIdentifiers,
};
use crate::types::{BindingType, SfcError};

use super::import_utils::{extract_import_identifiers, process_import_for_types};
use super::macros::{
    is_macro_call_line, is_multiline_macro_start, is_paren_macro_start, is_props_destructure_line,
};
use super::props::{extract_emit_names_from_type, extract_prop_types_from_type};
use super::typescript::transform_typescript_to_js;
use super::ScriptCompileResult;

/// Compile script setup content following Vue.js core format
#[allow(dead_code)]
pub fn compile_script_setup(
    content: &str,
    component_name: &str,
    is_vapor: bool,
    is_ts: bool,
    template_content: Option<&str>,
) -> Result<ScriptCompileResult, SfcError> {
    let mut ctx = ScriptCompileContext::new(content);
    ctx.analyze();

    // Use arena-allocated Vec for better performance
    let bump = vize_allocator::Bump::new();
    let mut output: vize_allocator::Vec<u8> = vize_allocator::Vec::with_capacity_in(4096, &bump);

    // Check if we have props destructure
    let has_props_destructure = ctx.macros.props_destructure.is_some();

    // Extract and output imports
    let mut imports = Vec::new();
    let mut setup_lines = Vec::new();
    let mut in_import = false;
    let mut import_buffer = String::new();

    // For multi-line statement tracking
    let mut in_destructure = false;
    let mut destructure_buffer = String::new();
    let mut brace_depth: i32 = 0;
    let mut waiting_for_macro_close = false; // After destructure closes, waiting for macro call to complete

    // For multi-line macro call tracking (e.g., defineEmits<{ ... }>())
    let mut in_macro_call = false;
    let mut macro_buffer = String::new();
    let mut macro_angle_depth: i32 = 0;

    // For multi-line paren-based macro call tracking (e.g., defineExpose({ ... }))
    let mut in_paren_macro_call = false;
    let mut paren_macro_depth: i32 = 0;

    for line in content.lines() {
        let trimmed = line.trim();

        // Handle multi-line macro call: const emit = defineEmits<{ ... }>()
        if in_macro_call {
            macro_buffer.push_str(line);
            macro_buffer.push('\n');
            // Track angle brackets but ignore => (arrow functions)
            let line_no_arrow = trimmed.replace("=>", "");
            macro_angle_depth += line_no_arrow.matches('<').count() as i32;
            macro_angle_depth -= line_no_arrow.matches('>').count() as i32;

            // Check if macro call is complete (angle brackets closed and has ())
            if macro_angle_depth <= 0 && (trimmed.contains("()") || trimmed.ends_with(')')) {
                // Skip the entire macro call
                in_macro_call = false;
                macro_buffer.clear();
                continue;
            }
            continue;
        }

        // Handle multi-line paren-based macro call: defineExpose({ ... })
        if in_paren_macro_call {
            paren_macro_depth += trimmed.matches('(').count() as i32;
            paren_macro_depth -= trimmed.matches(')').count() as i32;

            // Check if macro call is complete (parentheses balanced)
            if paren_macro_depth <= 0 {
                in_paren_macro_call = false;
                continue;
            }
            continue;
        }

        // Detect start of multi-line paren-based macro call (e.g., defineExpose({)
        // But not if it's part of a destructure pattern (const { ... } = defineProps)
        if !in_destructure
            && is_paren_macro_start(trimmed)
            && !trimmed.starts_with("const {")
            && !trimmed.starts_with("let {")
            && !trimmed.starts_with("var {")
        {
            in_paren_macro_call = true;
            paren_macro_depth =
                trimmed.matches('(').count() as i32 - trimmed.matches(')').count() as i32;
            continue;
        }

        // Detect start of multi-line macro call (e.g., defineEmits<{ or defineProps<{)
        // But not if it's part of a destructure pattern
        if !in_destructure
            && is_multiline_macro_start(trimmed)
            && !trimmed.starts_with("const {")
            && !trimmed.starts_with("let {")
            && !trimmed.starts_with("var {")
        {
            in_macro_call = true;
            macro_buffer.clear();
            macro_buffer.push_str(line);
            macro_buffer.push('\n');
            macro_angle_depth =
                trimmed.matches('<').count() as i32 - trimmed.matches('>').count() as i32;
            continue;
        }

        // Handle waiting for macro close after destructure (e.g., waiting for }>() )
        if waiting_for_macro_close {
            destructure_buffer.push_str(line);
            destructure_buffer.push('\n');

            let open_angles = destructure_buffer.matches('<').count();
            let close_angles = destructure_buffer.matches('>').count();

            // If angle brackets aren't balanced, keep going
            if open_angles > close_angles {
                continue;
            }

            // Angle brackets are balanced, check for closing ()
            if trimmed.ends_with("()") || trimmed.ends_with(')') {
                // Skip the entire destructure + macro call
                waiting_for_macro_close = false;
                in_destructure = false;
                destructure_buffer.clear();
                continue;
            }
            continue;
        }

        // Handle multi-line destructure pattern: const { ... } = defineProps(...)
        if in_destructure {
            destructure_buffer.push_str(line);
            destructure_buffer.push('\n');
            brace_depth += trimmed.matches('{').count() as i32;
            brace_depth -= trimmed.matches('}').count() as i32;

            // Check if the destructure pattern is closing (brace_depth reaches 0)
            if brace_depth <= 0 {
                // Now check if it's a defineProps/withDefaults call
                let is_props_macro = destructure_buffer.contains("defineProps")
                    || destructure_buffer.contains("withDefaults");

                if is_props_macro {
                    // Check if there are type args that need to close
                    let has_unclosed_type_args = destructure_buffer.contains('<')
                        && destructure_buffer.matches('<').count()
                            > destructure_buffer.matches('>').count();

                    if has_unclosed_type_args {
                        // Switch to waiting for macro close
                        waiting_for_macro_close = true;
                        continue;
                    }

                    // Check if we need to wait for ()
                    if !trimmed.ends_with("()") && !trimmed.ends_with(')') {
                        // Still waiting for the function call parens
                        waiting_for_macro_close = true;
                        continue;
                    }

                    // Skip the entire destructure - it's a props destructure
                    in_destructure = false;
                    destructure_buffer.clear();
                    continue;
                } else {
                    // Not a props destructure, add to setup lines
                    for buf_line in destructure_buffer.lines() {
                        setup_lines.push(buf_line.to_string());
                    }
                    in_destructure = false;
                    destructure_buffer.clear();
                    continue;
                }
            }
            continue;
        }

        // Detect start of destructure pattern
        if (trimmed.starts_with("const {")
            || trimmed.starts_with("let {")
            || trimmed.starts_with("var {"))
            && !trimmed.contains('}')
        {
            in_destructure = true;
            destructure_buffer.clear();
            destructure_buffer.push_str(line);
            destructure_buffer.push('\n');
            brace_depth = trimmed.matches('{').count() as i32 - trimmed.matches('}').count() as i32;
            continue;
        }

        // Handle single-line props destructure
        if is_props_destructure_line(trimmed) {
            continue;
        }

        if trimmed.starts_with("import ") {
            in_import = true;
            import_buffer.clear();
        }

        if in_import {
            import_buffer.push_str(line);
            import_buffer.push('\n');

            if trimmed.ends_with(';') || (trimmed.contains(" from ") && !trimmed.ends_with(',')) {
                imports.push(import_buffer.clone());
                in_import = false;
            }
        } else if !trimmed.is_empty() {
            // Skip compiler macro calls
            if is_macro_call_line(trimmed) {
                continue;
            }
            setup_lines.push(line.to_string());
        }
    }

    // Add Vapor-specific import
    if is_vapor {
        output.extend_from_slice(
            b"import { defineVaporComponent as _defineVaporComponent } from 'vue'\n",
        );
    }

    // Add mergeDefaults import if props destructure has defaults
    let needs_merge_defaults = has_props_destructure
        && ctx
            .macros
            .props_destructure
            .as_ref()
            .map(|d| d.bindings.values().any(|b| b.default.is_some()))
            .unwrap_or(false);
    if needs_merge_defaults {
        output.extend_from_slice(b"import { mergeDefaults as _mergeDefaults } from 'vue'\n");
    }

    // Output imports (filtering out type-only imports)
    for import in &imports {
        if let Some(processed) = process_import_for_types(import) {
            if !processed.is_empty() {
                output.extend_from_slice(processed.as_bytes());
            }
        }
    }

    output.push(b'\n');

    // Add comment for props destructure
    if has_props_destructure {
        output.extend_from_slice(b"// Reactive Props Destructure (Vue 3.5+)\n\n");
    }

    // Start __sfc__ definition
    if is_vapor {
        output.extend_from_slice(b"const __sfc__ = /*@__PURE__*/_defineVaporComponent({\n");
    } else {
        output.extend_from_slice(b"const __sfc__ = {\n");
    }
    output.extend_from_slice(b"  __name: '");
    output.extend_from_slice(component_name.as_bytes());
    output.extend_from_slice(b"',\n");

    // Props definition - handle both regular defineProps and destructure
    if has_props_destructure {
        let destructure = ctx.macros.props_destructure.as_ref().unwrap();

        // Check if there are any defaults
        let has_defaults = destructure.bindings.values().any(|b| b.default.is_some());

        if has_defaults {
            // Use mergeDefaults format: _mergeDefaults(['prop1', 'prop2'], { prop2: default })
            // Get the original props argument from defineProps
            let original_props = ctx
                .macros
                .define_props
                .as_ref()
                .map(|p| p.args.as_str())
                .unwrap_or("[]");

            output.extend_from_slice(b"  props: /*@__PURE__*/_mergeDefaults(");
            output.extend_from_slice(original_props.as_bytes());
            output.extend_from_slice(b", {\n");

            // Add defaults
            for (key, binding) in &destructure.bindings {
                if let Some(ref default_val) = binding.default {
                    output.extend_from_slice(b"  ");
                    output.extend_from_slice(key.as_bytes());
                    output.extend_from_slice(b": ");
                    output.extend_from_slice(default_val.as_bytes());
                    output.push(b'\n');
                }
            }
            output.extend_from_slice(b"}),\n");
        } else {
            // No defaults - just use the original props array
            if let Some(ref props_macro) = ctx.macros.define_props {
                if !props_macro.args.is_empty() {
                    output.extend_from_slice(b"  props: ");
                    output.extend_from_slice(props_macro.args.as_bytes());
                    output.extend_from_slice(b",\n");
                }
            }
        }
    } else if let Some(ref props_macro) = ctx.macros.define_props {
        if let Some(ref type_args) = props_macro.type_args {
            // For type-based props, extract full prop definitions
            let prop_types = extract_prop_types_from_type(type_args);
            if !prop_types.is_empty() {
                output.extend_from_slice(b"  props: {\n");
                // Sort props for deterministic output
                let mut sorted_props: Vec<_> = prop_types.iter().collect();
                sorted_props.sort_by(|a, b| a.0.cmp(b.0));
                for (name, prop_type) in sorted_props {
                    output.extend_from_slice(b"    ");
                    output.extend_from_slice(name.as_bytes());
                    output.extend_from_slice(b": { type: ");
                    output.extend_from_slice(prop_type.js_type.as_bytes());
                    output.extend_from_slice(b", required: ");
                    output.extend_from_slice(if prop_type.optional {
                        b"false"
                    } else {
                        b"true"
                    });
                    output.extend_from_slice(b" },\n");
                }
                output.extend_from_slice(b"  },\n");
            }
        } else if !props_macro.args.is_empty() {
            output.extend_from_slice(b"  props: ");
            output.extend_from_slice(props_macro.args.as_bytes());
            output.extend_from_slice(b",\n");
        }
    }

    // Emits definition if defineEmits was used
    if let Some(ref emits_macro) = ctx.macros.define_emits {
        if let Some(ref type_args) = emits_macro.type_args {
            // Extract emit names from type
            let emit_names = extract_emit_names_from_type(type_args);
            if !emit_names.is_empty() {
                output.extend_from_slice(b"  emits: [");
                for (i, name) in emit_names.iter().enumerate() {
                    if i > 0 {
                        output.extend_from_slice(b", ");
                    }
                    output.push(b'"');
                    output.extend_from_slice(name.as_bytes());
                    output.push(b'"');
                }
                output.extend_from_slice(b"],\n");
            }
        } else if !emits_macro.args.is_empty() {
            output.extend_from_slice(b"  emits: ");
            output.extend_from_slice(emits_macro.args.as_bytes());
            output.extend_from_slice(b",\n");
        }
    }

    // Setup function
    output.extend_from_slice(b"  setup(__props, { expose: __expose, emit: __emit }) {\n");

    // defineExpose: transform to __expose(...)
    if let Some(ref expose_macro) = ctx.macros.define_expose {
        // args contains the argument content (e.g., "{ foo, bar }")
        let args = expose_macro.args.trim();
        if args.is_empty() {
            output.extend_from_slice(b"  __expose();\n");
        } else {
            output.extend_from_slice(b"  __expose(");
            output.extend_from_slice(args.as_bytes());
            output.extend_from_slice(b");\n");
        }
    }

    // Collect emit binding name for inclusion in __returned__
    let emit_binding_name = ctx
        .macros
        .define_emits
        .as_ref()
        .and_then(|m| m.binding_name.clone());

    // defineEmits binding: const emit = __emit
    if let Some(ref binding_name) = emit_binding_name {
        output.extend_from_slice(b"  const ");
        output.extend_from_slice(binding_name.as_bytes());
        output.extend_from_slice(b" = __emit\n");
    }

    // Collect props binding for exclusion from __returned__ (props themselves shouldn't be in returned)
    let mut props_binding_names: HashSet<String> = HashSet::new();

    // defineProps binding: const props = __props (only if not destructured)
    if !has_props_destructure {
        if let Some(ref props_macro) = ctx.macros.define_props {
            if let Some(ref binding_name) = props_macro.binding_name {
                output.extend_from_slice(b"  const ");
                output.extend_from_slice(binding_name.as_bytes());
                output.extend_from_slice(b" = __props\n");
                props_binding_names.insert(binding_name.clone());
            }
        }
    }

    // Output setup code, transforming props destructure references
    let setup_code = setup_lines.join("\n");

    // Debug: Log props destructure status
    #[cfg(debug_assertions)]
    {
        if ctx.macros.props_destructure.is_some() {
            eprintln!(
                "[DEBUG] Props destructure found: {:?}",
                ctx.macros.props_destructure
            );
        } else {
            eprintln!("[DEBUG] No props destructure found");
        }
        eprintln!("[DEBUG] Setup code before transform:\n{}", setup_code);
    }

    let transformed_setup = if let Some(ref destructure) = ctx.macros.props_destructure {
        let result = transform_destructured_props(&setup_code, destructure);
        #[cfg(debug_assertions)]
        eprintln!("[DEBUG] Setup code after transform:\n{}", result);
        result
    } else {
        setup_code
    };

    // Indent the setup code
    for line in transformed_setup.lines() {
        if !line.trim().is_empty() {
            output.extend_from_slice(b"  ");
            output.extend_from_slice(line.as_bytes());
        }
        output.push(b'\n');
    }

    // Compiler macros preset - these are compile-time only and should not be in __returned__
    let compiler_macros: HashSet<&str> = [
        "defineProps",
        "defineEmits",
        "defineExpose",
        "defineOptions",
        "defineSlots",
        "defineModel",
        "withDefaults",
    ]
    .into_iter()
    .collect();

    // Collect destructured prop local names to exclude from __returned__
    let destructured_prop_locals: HashSet<String> = ctx
        .macros
        .props_destructure
        .as_ref()
        .map(|d| d.bindings.values().map(|b| b.local.clone()).collect())
        .unwrap_or_default();

    // Generate __returned__ object
    let mut returned_bindings: Vec<String> = ctx
        .bindings
        .bindings
        .keys()
        .filter(|name| {
            // Exclude compiler macros, destructured props, and props bindings
            !compiler_macros.contains(name.as_str())
                && !destructured_prop_locals.contains(*name)
                && !props_binding_names.contains(*name)
        })
        .cloned()
        .collect();

    // Add emit binding to returned (it's a runtime value that should be exposed)
    if let Some(ref emit_name) = emit_binding_name {
        if !returned_bindings.contains(emit_name) {
            returned_bindings.push(emit_name.clone());
        }
    }

    returned_bindings.sort();

    // Parse template to get used identifiers
    let template_used_ids: TemplateUsedIdentifiers = if let Some(template_src) = template_content {
        let allocator = Bump::new();
        let (root, _) = vize_compiler_core::parser::parse(&allocator, template_src);
        resolve_template_used_identifiers(&root)
    } else {
        TemplateUsedIdentifiers::default()
    };

    // Extract all imported identifiers (both named and default imports)
    let mut imported_identifiers: Vec<String> = Vec::new();
    for import in &imports {
        // Extract names using OXC parser for accuracy
        let extracted = extract_import_identifiers(import);
        for name in extracted {
            // Exclude compiler macros from imports
            if !compiler_macros.contains(name.as_str()) {
                imported_identifiers.push(name);
            }
        }
    }

    // Include imported identifiers that are used in template
    let mut all_bindings = returned_bindings.clone();
    for name in &imported_identifiers {
        // Include if used in template OR if no template (include all for safety)
        if template_content.is_none() || template_used_ids.used_ids.contains(name) {
            if !all_bindings.contains(name) {
                all_bindings.push(name.clone());
            }
            // Also add to binding metadata so template compiler knows about it
            if !ctx.bindings.bindings.contains_key(name) {
                ctx.bindings
                    .bindings
                    .insert(name.clone(), BindingType::SetupConst);
            }
        }
    }
    all_bindings.sort();
    all_bindings.dedup();

    output.extend_from_slice(b"  const __returned__ = { ");
    output.extend_from_slice(all_bindings.join(", ").as_bytes());
    output.extend_from_slice(b" }\n");
    output.extend_from_slice(b"  Object.defineProperty(__returned__, '__isScriptSetup', { enumerable: false, value: true })\n");
    output.extend_from_slice(b"  return __returned__\n");

    output.extend_from_slice(b"  }\n\n");
    // Close the component definition
    if is_vapor {
        output.extend_from_slice(b"});\n"); // Close _defineVaporComponent(
    } else {
        output.extend_from_slice(b"};\n");
    }

    // Convert arena Vec<u8> to String - SAFETY: we only push valid UTF-8
    let output_str = unsafe { String::from_utf8_unchecked(output.into_iter().collect()) };

    // Transform TypeScript to JavaScript using OXC if lang="ts"
    let final_code = if is_ts {
        transform_typescript_to_js(&output_str)
    } else {
        output_str
    };

    Ok(ScriptCompileResult {
        code: final_code,
        bindings: Some(ctx.bindings),
    })
}
