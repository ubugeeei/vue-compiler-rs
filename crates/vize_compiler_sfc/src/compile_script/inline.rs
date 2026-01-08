//! Inline mode script compilation.
//!
//! This module handles compilation of script setup with inline template mode,
//! where the render function is inlined into the setup function.

use crate::script::{transform_destructured_props, ScriptCompileContext};
use crate::types::SfcError;

use super::import_utils::process_import_for_types;
use super::macros::{
    is_macro_call_line, is_multiline_macro_start, is_paren_macro_start, is_props_destructure_line,
};
use super::props::{
    extract_emit_names_from_type, extract_prop_types_from_type, extract_with_defaults_defaults,
};
use super::typescript::transform_typescript_to_js;
use super::{ScriptCompileResult, TemplateParts};

/// Compile script setup with inline template (Vue's inline template mode)
pub fn compile_script_setup_inline(
    content: &str,
    component_name: &str,
    is_ts: bool,
    template: TemplateParts<'_>,
    normal_script_content: Option<&str>,
) -> Result<ScriptCompileResult, SfcError> {
    let mut ctx = ScriptCompileContext::new(content);
    ctx.analyze();

    // Use arena-allocated Vec for better performance
    let bump = vize_allocator::Bump::new();
    let mut output: vize_allocator::Vec<u8> = vize_allocator::Vec::with_capacity_in(4096, &bump);

    // Store normal script content to add AFTER TypeScript transformation
    // This preserves type definitions that would otherwise be stripped
    let preserved_normal_script = normal_script_content
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    // Check if we need mergeDefaults import (props destructure with defaults)
    let has_props_destructure = ctx.macros.props_destructure.is_some();
    let needs_merge_defaults = has_props_destructure
        && ctx
            .macros
            .props_destructure
            .as_ref()
            .map(|d| d.bindings.values().any(|b| b.default.is_some()))
            .unwrap_or(false);

    // Check if defineModel was used
    let has_define_model = !ctx.macros.define_models.is_empty();

    // mergeDefaults import comes first if needed
    if needs_merge_defaults {
        output.extend_from_slice(b"import { mergeDefaults as _mergeDefaults } from 'vue'\n");
    }

    // useModel import if defineModel was used
    if has_define_model {
        output.extend_from_slice(b"import { useModel as _useModel } from 'vue'\n");
    }

    // defineComponent and PropType imports for TypeScript
    if is_ts {
        // Check if we need PropType (when there are typed props)
        let needs_prop_type = ctx
            .macros
            .define_props
            .as_ref()
            .is_some_and(|p| p.type_args.is_some() || !p.args.is_empty())
            || !ctx.macros.define_models.is_empty();

        if needs_prop_type {
            output.extend_from_slice(b"import { defineComponent, PropType } from 'vue'\n");
        } else {
            output.extend_from_slice(b"import { defineComponent } from 'vue'\n");
        }
    }

    // Template imports (Vue helpers)
    if !template.imports.is_empty() {
        output.extend_from_slice(template.imports.as_bytes());
        // Blank line after template imports
        output.push(b'\n');
    }

    // Extract user imports
    let mut user_imports = Vec::new();
    let mut setup_lines = Vec::new();

    // Parse script content - extract imports and setup code
    let mut in_import = false;
    let mut import_buffer = String::new();
    let mut in_destructure = false;
    let mut destructure_buffer = String::new();
    let mut brace_depth: i32 = 0;
    let mut in_macro_call = false;
    let mut macro_angle_depth: i32 = 0;
    let mut in_paren_macro_call = false;
    let mut paren_macro_depth: i32 = 0;
    let mut waiting_for_macro_close = false;
    // Track multiline object literals: const xxx = { ... }
    let mut in_object_literal = false;
    let mut object_literal_buffer = String::new();
    let mut object_literal_brace_depth: i32 = 0;
    // Track TypeScript-only declarations (interface, type) to skip them
    let mut in_ts_interface = false;
    let mut ts_interface_brace_depth: i32 = 0;
    let mut in_ts_type = false;
    let mut ts_type_depth: i32 = 0; // Track angle brackets and parens for complex types

    for line in content.lines() {
        let trimmed = line.trim();

        // Handle multi-line macro calls
        if in_macro_call {
            // Count angle brackets but ignore => (arrow functions)
            let line_no_arrow = trimmed.replace("=>", "");
            macro_angle_depth += line_no_arrow.matches('<').count() as i32;
            macro_angle_depth -= line_no_arrow.matches('>').count() as i32;
            if macro_angle_depth <= 0 && (trimmed.contains("()") || trimmed.ends_with(')')) {
                in_macro_call = false;
            }
            continue;
        }

        if in_paren_macro_call {
            paren_macro_depth += trimmed.matches('(').count() as i32;
            paren_macro_depth -= trimmed.matches(')').count() as i32;
            if paren_macro_depth <= 0 {
                in_paren_macro_call = false;
            }
            continue;
        }

        if waiting_for_macro_close {
            destructure_buffer.push_str(line);
            destructure_buffer.push('\n');
            // Track angle brackets for type args (ignore => arrow functions)
            let line_no_arrow = trimmed.replace("=>", "");
            macro_angle_depth += line_no_arrow.matches('<').count() as i32;
            macro_angle_depth -= line_no_arrow.matches('>').count() as i32;
            if macro_angle_depth <= 0 && (trimmed.ends_with("()") || trimmed.ends_with(')')) {
                waiting_for_macro_close = false;
                destructure_buffer.clear();
            }
            continue;
        }

        if in_destructure {
            destructure_buffer.push_str(line);
            destructure_buffer.push('\n');
            // Track both braces and angle brackets for type args (ignore => arrow functions)
            let line_no_arrow = trimmed.replace("=>", "");
            brace_depth += trimmed.matches('{').count() as i32;
            brace_depth -= trimmed.matches('}').count() as i32;
            macro_angle_depth += line_no_arrow.matches('<').count() as i32;
            macro_angle_depth -= line_no_arrow.matches('>').count() as i32;
            // Only consider closed when BOTH braces and angle brackets are balanced
            // and we have the closing parentheses
            if brace_depth <= 0 && macro_angle_depth <= 0 {
                let is_props_macro = destructure_buffer.contains("defineProps")
                    || destructure_buffer.contains("withDefaults");
                if is_props_macro && !trimmed.ends_with("()") && !trimmed.ends_with(')') {
                    waiting_for_macro_close = true;
                    continue;
                }
                in_destructure = false;
                destructure_buffer.clear();
            }
            continue;
        }

        // Detect macro call starts
        if is_paren_macro_start(trimmed)
            && !trimmed.starts_with("const {")
            && !trimmed.starts_with("let {")
        {
            in_paren_macro_call = true;
            paren_macro_depth =
                trimmed.matches('(').count() as i32 - trimmed.matches(')').count() as i32;
            continue;
        }

        if is_multiline_macro_start(trimmed)
            && !trimmed.starts_with("const {")
            && !trimmed.starts_with("let {")
        {
            in_macro_call = true;
            macro_angle_depth =
                trimmed.matches('<').count() as i32 - trimmed.matches('>').count() as i32;
            continue;
        }

        // Detect destructure start with type args: const { x } = defineProps<{...}>()
        // This pattern has both the destructure closing brace AND type arg opening angle bracket
        if (trimmed.starts_with("const {")
            || trimmed.starts_with("let {")
            || trimmed.starts_with("var {"))
            && (trimmed.contains("defineProps<") || trimmed.contains("withDefaults("))
        {
            // Check if it's complete on a single line
            if !trimmed.ends_with("()") && !trimmed.ends_with(')') {
                // Multi-line: wait for completion
                in_destructure = true;
                destructure_buffer = line.to_string() + "\n";
                brace_depth =
                    trimmed.matches('{').count() as i32 - trimmed.matches('}').count() as i32;
                macro_angle_depth =
                    trimmed.matches('<').count() as i32 - trimmed.matches('>').count() as i32;
                continue;
            } else {
                // Single line, complete - skip it
                continue;
            }
        }

        // Detect destructure start (without type args)
        if (trimmed.starts_with("const {")
            || trimmed.starts_with("let {")
            || trimmed.starts_with("var {"))
            && !trimmed.contains('}')
        {
            in_destructure = true;
            destructure_buffer = line.to_string() + "\n";
            brace_depth = trimmed.matches('{').count() as i32 - trimmed.matches('}').count() as i32;
            macro_angle_depth = 0;
            continue;
        }

        // Skip single-line props destructure
        if is_props_destructure_line(trimmed) {
            continue;
        }

        // Handle multiline object literals: const xxx = { ... }
        if in_object_literal {
            object_literal_buffer.push_str(line);
            object_literal_buffer.push('\n');
            object_literal_brace_depth += trimmed.matches('{').count() as i32;
            object_literal_brace_depth -= trimmed.matches('}').count() as i32;
            if object_literal_brace_depth <= 0 {
                // Object literal is complete, add to setup_lines
                for buf_line in object_literal_buffer.lines() {
                    setup_lines.push(buf_line.to_string());
                }
                in_object_literal = false;
                object_literal_buffer.clear();
            }
            continue;
        }

        // Detect multiline object literal start: const xxx = { or const xxx: Type = {
        if (trimmed.starts_with("const ")
            || trimmed.starts_with("let ")
            || trimmed.starts_with("var "))
            && trimmed.contains('=')
            && trimmed.ends_with('{')
            && !trimmed.contains("defineProps")
            && !trimmed.contains("defineEmits")
            && !trimmed.contains("defineModel")
        {
            in_object_literal = true;
            object_literal_buffer = line.to_string() + "\n";
            object_literal_brace_depth =
                trimmed.matches('{').count() as i32 - trimmed.matches('}').count() as i32;
            continue;
        }

        // Handle imports
        if trimmed.starts_with("import ") {
            in_import = true;
            import_buffer.clear();
        }

        if in_import {
            import_buffer.push_str(line);
            import_buffer.push('\n');
            if trimmed.ends_with(';') || (trimmed.contains(" from ") && !trimmed.ends_with(',')) {
                user_imports.push(import_buffer.clone());
                in_import = false;
            }
            continue;
        }

        // Handle TypeScript interface declarations (skip them)
        if in_ts_interface {
            ts_interface_brace_depth += trimmed.matches('{').count() as i32;
            ts_interface_brace_depth -= trimmed.matches('}').count() as i32;
            if ts_interface_brace_depth <= 0 {
                in_ts_interface = false;
            }
            continue;
        }

        // Detect TypeScript interface start
        if trimmed.starts_with("interface ") || trimmed.starts_with("export interface ") {
            in_ts_interface = true;
            ts_interface_brace_depth =
                trimmed.matches('{').count() as i32 - trimmed.matches('}').count() as i32;
            if ts_interface_brace_depth <= 0 {
                in_ts_interface = false;
            }
            continue;
        }

        // Handle TypeScript type declarations (skip them)
        if in_ts_type {
            // Track balanced brackets for complex types like: type X = { a: string } | { b: number }
            ts_type_depth += trimmed.matches('{').count() as i32;
            ts_type_depth -= trimmed.matches('}').count() as i32;
            ts_type_depth += trimmed.matches('<').count() as i32;
            ts_type_depth -= trimmed.matches('>').count() as i32;
            ts_type_depth += trimmed.matches('(').count() as i32;
            ts_type_depth -= trimmed.matches(')').count() as i32;
            // Type declaration ends when balanced and ends with semicolon or newline with no continuation
            if ts_type_depth <= 0
                && (trimmed.ends_with(';')
                    || (!trimmed.ends_with('|')
                        && !trimmed.ends_with('&')
                        && !trimmed.ends_with(',')
                        && !trimmed.ends_with('{')))
            {
                in_ts_type = false;
            }
            continue;
        }

        // Detect TypeScript type alias start
        if trimmed.starts_with("type ") || trimmed.starts_with("export type ") {
            // Check if it's a single-line type
            let has_equals = trimmed.contains('=');
            if has_equals {
                ts_type_depth = trimmed.matches('{').count() as i32
                    - trimmed.matches('}').count() as i32
                    + trimmed.matches('<').count() as i32
                    - trimmed.matches('>').count() as i32
                    + trimmed.matches('(').count() as i32
                    - trimmed.matches(')').count() as i32;
                // Check if complete on one line
                // A type is NOT complete if:
                // - brackets/parens aren't balanced (depth > 0)
                // - line ends with continuation characters (|, &, ,, {, =)
                if ts_type_depth <= 0
                    && (trimmed.ends_with(';')
                        || (!trimmed.ends_with('|')
                            && !trimmed.ends_with('&')
                            && !trimmed.ends_with(',')
                            && !trimmed.ends_with('{')
                            && !trimmed.ends_with('=')))
                {
                    // Single line type, just skip
                    continue;
                }
                in_ts_type = true;
            }
            continue;
        }

        if !trimmed.is_empty() && !is_macro_call_line(trimmed) {
            // All user code goes to setup_lines
            // Hoisting user-defined consts is problematic without proper AST-based scope tracking
            // Template-generated _hoisted_X consts are handled separately by template.hoisted
            setup_lines.push(line.to_string());
        }
    }

    // User imports (with blank line after template imports)
    for import in &user_imports {
        if let Some(processed) = process_import_for_types(import) {
            if !processed.is_empty() {
                output.extend_from_slice(processed.as_bytes());
            }
        }
    }

    // Template hoisted consts (e.g., const _hoisted_1 = { class: "..." })
    if !template.hoisted.is_empty() {
        output.push(b'\n');
        output.extend_from_slice(template.hoisted.as_bytes());
    }

    // Start export default (blank line before)
    output.push(b'\n');
    let has_options = ctx.macros.define_options.is_some();
    if has_options {
        // Use Object.assign for defineOptions
        output.extend_from_slice(b"export default /*@__PURE__*/Object.assign(");
        let options_args = ctx.macros.define_options.as_ref().unwrap().args.trim();
        output.extend_from_slice(options_args.as_bytes());
        output.extend_from_slice(b", {\n");
    } else if is_ts {
        // TypeScript: use defineComponent
        output.extend_from_slice(b"export default defineComponent({\n");
    } else {
        output.extend_from_slice(b"export default {\n");
    }
    // Use 'name' for defineComponent (TypeScript), '__name' for plain object (JavaScript)
    if is_ts {
        output.extend_from_slice(b"  name: '");
    } else {
        output.extend_from_slice(b"  __name: '");
    }
    output.extend_from_slice(component_name.as_bytes());
    output.extend_from_slice(b"',\n");

    // Props definition
    // Extract defaults from withDefaults if present
    let with_defaults_args = ctx
        .macros
        .with_defaults
        .as_ref()
        .map(|wd| extract_with_defaults_defaults(&wd.args));

    // Collect model names from defineModel calls (needed before props)
    let model_infos: Vec<(String, String, Option<String>)> = ctx
        .macros
        .define_models
        .iter()
        .map(|m| {
            // Extract model name from args: defineModel('count') -> 'count', defineModel() -> 'modelValue'
            let model_name = if m.args.trim().is_empty() {
                "modelValue".to_string()
            } else {
                // Check if first arg is a string literal (model name)
                let args = m.args.trim();
                if args.starts_with('\'') || args.starts_with('"') {
                    // Extract the string literal
                    args.trim_matches(|c| c == '\'' || c == '"')
                        .split(',')
                        .next()
                        .unwrap_or("modelValue")
                        .trim_matches(|c| c == '\'' || c == '"')
                        .to_string()
                } else {
                    "modelValue".to_string()
                }
            };
            let binding_name = m.binding_name.clone().unwrap_or_else(|| model_name.clone());
            let options = if m.args.trim().is_empty() {
                None
            } else {
                // Extract options (second argument or first if not a string)
                let args = m.args.trim();
                if args.starts_with('{') {
                    Some(args.to_string())
                } else if args.contains(',') {
                    // defineModel('name', { options })
                    args.split_once(',')
                        .map(|(_, opts)| opts.trim().to_string())
                } else {
                    None
                }
            };
            (model_name, binding_name, options)
        })
        .collect();

    if let Some(ref props_macro) = ctx.macros.define_props {
        if let Some(ref type_args) = props_macro.type_args {
            // Type-based props: extract prop definitions from type
            let prop_types = extract_prop_types_from_type(type_args);
            if !prop_types.is_empty() || !model_infos.is_empty() {
                output.extend_from_slice(b"  props: {\n");
                // Sort props for deterministic output
                let mut sorted_props: Vec<_> = prop_types.iter().collect();
                sorted_props.sort_by(|a, b| a.0.cmp(b.0));
                for (name, prop_type) in sorted_props {
                    output.extend_from_slice(b"    ");
                    output.extend_from_slice(name.as_bytes());
                    output.extend_from_slice(b": { type: ");
                    output.extend_from_slice(prop_type.js_type.as_bytes());
                    // Add PropType for TypeScript output
                    if is_ts {
                        if let Some(ref ts_type) = prop_type.ts_type {
                            output.extend_from_slice(b" as PropType<");
                            output.extend_from_slice(ts_type.as_bytes());
                            output.push(b'>');
                        }
                    }
                    output.extend_from_slice(b", required: ");
                    output.extend_from_slice(if prop_type.optional {
                        b"false"
                    } else {
                        b"true"
                    });
                    // Add default value from withDefaults or props destructure
                    let mut has_default = false;
                    if let Some(ref defaults) = with_defaults_args {
                        if let Some(default_val) = defaults.get(name.as_str()) {
                            output.extend_from_slice(b", default: ");
                            output.extend_from_slice(default_val.as_bytes());
                            has_default = true;
                        }
                    }
                    // Also check props destructure defaults (Vue 3.4+ reactive props destructure)
                    if !has_default {
                        if let Some(ref destructure) = ctx.macros.props_destructure {
                            if let Some(binding) = destructure.bindings.get(name) {
                                if let Some(ref default_val) = binding.default {
                                    output.extend_from_slice(b", default: ");
                                    output.extend_from_slice(default_val.as_bytes());
                                }
                            }
                        }
                    }
                    output.extend_from_slice(b" },\n");
                }
                // Add model props if any
                for (model_name, _, options) in &model_infos {
                    output.extend_from_slice(b"    ");
                    output.extend_from_slice(model_name.as_bytes());
                    output.extend_from_slice(b": ");
                    if let Some(opts) = options {
                        output.extend_from_slice(opts.as_bytes());
                    } else {
                        output.extend_from_slice(b"{}");
                    }
                    output.extend_from_slice(b",\n");
                }
                output.extend_from_slice(b"  },\n");
            }
        } else if !props_macro.args.is_empty() {
            if needs_merge_defaults {
                // Use mergeDefaults format: _mergeDefaults(['prop1', 'prop2'], { prop2: default })
                let destructure = ctx.macros.props_destructure.as_ref().unwrap();
                output.extend_from_slice(b"  props: /*@__PURE__*/_mergeDefaults(");
                output.extend_from_slice(props_macro.args.as_bytes());
                output.extend_from_slice(b", {\n");
                // Collect defaults
                let defaults: Vec<_> = destructure
                    .bindings
                    .iter()
                    .filter_map(|(k, b)| b.default.as_ref().map(|d| (k.as_str(), d.as_str())))
                    .collect();
                for (i, (key, default_val)) in defaults.iter().enumerate() {
                    output.extend_from_slice(b"  ");
                    output.extend_from_slice(key.as_bytes());
                    output.extend_from_slice(b": ");
                    output.extend_from_slice(default_val.as_bytes());
                    if i < defaults.len() - 1 {
                        output.push(b',');
                    }
                    output.push(b'\n');
                }
                output.extend_from_slice(b"}),\n");
            } else {
                output.extend_from_slice(b"  props: ");
                output.extend_from_slice(props_macro.args.as_bytes());
                output.extend_from_slice(b",\n");
            }
        }
    }

    // Add model props to props definition if defineModel was used and no defineProps
    if !model_infos.is_empty() && ctx.macros.define_props.is_none() {
        output.extend_from_slice(b"  props: {\n");
        for (model_name, _binding_name, options) in &model_infos {
            output.extend_from_slice(b"    ");
            output.extend_from_slice(model_name.as_bytes());
            output.extend_from_slice(b": ");
            if let Some(opts) = options {
                output.extend_from_slice(opts.as_bytes());
            } else {
                output.extend_from_slice(b"{}");
            }
            output.extend_from_slice(b",\n");
        }
        output.extend_from_slice(b"  },\n");
    }

    // Emits definition - combine defineEmits and defineModel emits
    let mut all_emits: Vec<String> = Vec::new();

    // Collect emits from defineEmits
    if let Some(ref emits_macro) = ctx.macros.define_emits {
        if !emits_macro.args.is_empty() {
            // Runtime array syntax: defineEmits(['click', 'update'])
            // Parse the array to extract event names
            let args = emits_macro.args.trim();
            if args.starts_with('[') && args.ends_with(']') {
                let inner = &args[1..args.len() - 1];
                for part in inner.split(',') {
                    let name = part.trim().trim_matches(|c| c == '\'' || c == '"');
                    if !name.is_empty() {
                        all_emits.push(name.to_string());
                    }
                }
            }
        } else if let Some(ref type_args) = emits_macro.type_args {
            // Type-based syntax: defineEmits<{ (e: 'click'): void }>()
            let emit_names = extract_emit_names_from_type(type_args);
            all_emits.extend(emit_names);
        }
    }

    // Add model update events
    for (model_name, _, _) in &model_infos {
        all_emits.push(format!("update:{}", model_name));
    }

    // Output combined emits
    if !all_emits.is_empty() {
        output.extend_from_slice(b"  emits: [");
        for (i, name) in all_emits.iter().enumerate() {
            if i > 0 {
                output.extend_from_slice(b", ");
            }
            output.push(b'"');
            output.extend_from_slice(name.as_bytes());
            output.push(b'"');
        }
        output.extend_from_slice(b"],\n");
    }

    // Setup function - include destructured args based on macros used
    let has_emit = ctx.macros.define_emits.is_some();
    let has_emit_binding = ctx
        .macros
        .define_emits
        .as_ref()
        .map(|e| e.binding_name.is_some())
        .unwrap_or(false);
    let has_expose = ctx.macros.define_expose.is_some();

    // Build setup function signature based on what macros are used
    let mut setup_args = Vec::new();
    if has_expose {
        setup_args.push("expose: __expose");
    }
    if has_emit {
        if has_emit_binding {
            setup_args.push("emit: __emit");
        } else {
            setup_args.push("emit: $emit");
        }
    }

    if setup_args.is_empty() {
        output.extend_from_slice(b"  setup(__props) {\n");
    } else {
        output.extend_from_slice(b"  setup(__props, { ");
        output.extend_from_slice(setup_args.join(", ").as_bytes());
        output.extend_from_slice(b" }) {\n");
    }

    // Always add a blank line after setup signature
    output.push(b'\n');

    // Emit binding: const emit = __emit
    if let Some(ref emits_macro) = ctx.macros.define_emits {
        if let Some(ref binding_name) = emits_macro.binding_name {
            output.extend_from_slice(b"const ");
            output.extend_from_slice(binding_name.as_bytes());
            output.extend_from_slice(b" = __emit\n");
        }
    }

    // Props binding: const props = __props
    if let Some(ref props_macro) = ctx.macros.define_props {
        if let Some(ref binding_name) = props_macro.binding_name {
            output.extend_from_slice(b"const ");
            output.extend_from_slice(binding_name.as_bytes());
            output.extend_from_slice(b" = __props\n");
        }
    }

    // Model bindings: const model = _useModel(__props, 'modelValue')
    if !model_infos.is_empty() {
        for (model_name, binding_name, _) in &model_infos {
            output.extend_from_slice(b"const ");
            output.extend_from_slice(binding_name.as_bytes());
            output.extend_from_slice(b" = _useModel(__props, \"");
            output.extend_from_slice(model_name.as_bytes());
            output.extend_from_slice(b"\")\n");
        }
    }

    // Setup code body - transform props destructure references
    let setup_code = setup_lines.join("\n");
    let transformed_setup = if let Some(ref destructure) = ctx.macros.props_destructure {
        transform_destructured_props(&setup_code, destructure)
    } else {
        setup_code
    };
    for line in transformed_setup.lines() {
        output.extend_from_slice(line.as_bytes());
        output.push(b'\n');
    }

    // defineExpose: transform to __expose(...)
    if let Some(ref expose_macro) = ctx.macros.define_expose {
        let args = expose_macro.args.trim();
        output.extend_from_slice(b"__expose(");
        output.extend_from_slice(args.as_bytes());
        output.extend_from_slice(b")\n");
    }

    // Inline render function as return (blank line before)
    output.push(b'\n');
    if !template.render_body.is_empty() {
        output.extend_from_slice(b"return (_ctx, _cache) => {\n");

        // Output component/directive resolution statements (preamble)
        for line in template.preamble.lines() {
            if !line.trim().is_empty() {
                output.extend_from_slice(b"  ");
                output.extend_from_slice(line.as_bytes());
                output.push(b'\n');
            }
        }
        if !template.preamble.is_empty() {
            output.push(b'\n');
        }

        // Indent the render body properly
        let mut first_line = true;
        for line in template.render_body.lines() {
            if first_line {
                output.extend_from_slice(b"  return ");
                output.extend_from_slice(line.as_bytes());
                first_line = false;
            } else {
                output.push(b'\n');
                // Preserve existing indentation by adding 2 spaces (setup indent)
                if !line.trim().is_empty() {
                    output.extend_from_slice(b"  ");
                }
                output.extend_from_slice(line.as_bytes());
            }
        }
        output.push(b'\n');
        output.extend_from_slice(b"}\n");
    }

    output.extend_from_slice(b"}\n");
    output.push(b'\n');
    if has_options || is_ts {
        // Close defineComponent() or Object.assign()
        output.extend_from_slice(b"})\n");
    } else {
        output.extend_from_slice(b"}\n");
    }

    // Convert arena Vec<u8> to String - SAFETY: we only push valid UTF-8
    let output_str = unsafe { String::from_utf8_unchecked(output.into_iter().collect()) };

    // Transform TypeScript to JavaScript
    // is_ts here indicates whether to preserve TypeScript output (true) or transpile to JS (false)
    // When is_ts = false, we always run the transform to strip any TypeScript syntax
    // When is_ts = true, we keep the code as-is (preserve TypeScript)
    let transformed_code = if is_ts {
        // Preserve TypeScript output - no transformation
        output_str
    } else {
        // Transpile to JavaScript - always run transform to strip TypeScript syntax
        transform_typescript_to_js(&output_str)
    };

    // Prepend preserved normal script content
    // If transpiling to JS (is_ts = false), also transform the normal script content
    let final_code = if let Some(normal_script) = preserved_normal_script {
        let transformed_normal = if is_ts {
            normal_script
        } else {
            transform_typescript_to_js(&normal_script)
        };
        format!("{}\n\n{}", transformed_normal, transformed_code)
    } else {
        transformed_code
    };

    Ok(ScriptCompileResult {
        code: final_code,
        bindings: Some(ctx.bindings),
    })
}
