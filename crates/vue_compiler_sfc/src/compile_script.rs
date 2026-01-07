//! Script compilation for Vue SFCs.
//!
//! This module handles compilation of `<script>` and `<script setup>` blocks,
//! following the Vue.js core output format.

use oxc_allocator::Allocator;
use oxc_ast::ast::{ImportDeclarationSpecifier, Statement};
use oxc_codegen::Codegen;
use oxc_parser::Parser;
use oxc_semantic::SemanticBuilder;
use oxc_span::SourceType;
use oxc_transformer::{TransformOptions, Transformer};
use vue_allocator::Bump;

use crate::script::{
    resolve_template_used_identifiers, transform_destructured_props, ScriptCompileContext,
    TemplateUsedIdentifiers,
};
use crate::types::*;

/// Script compilation result
pub struct ScriptCompileResult {
    pub code: String,
    pub bindings: Option<BindingMetadata>,
}

/// Template parts for inline compilation
pub(crate) struct TemplateParts<'a> {
    pub imports: &'a str,
    pub hoisted: &'a str,
    /// Component/directive resolution statements (inside render function, before return)
    pub preamble: &'a str,
    pub render_body: &'a str,
}

/// Prop type information
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub(crate) struct PropTypeInfo {
    /// JavaScript type constructor name (String, Number, Boolean, Array, Object, Function)
    pub js_type: String,
    /// Original TypeScript type (for PropType<T> usage)
    pub ts_type: Option<String>,
    /// Whether the prop is optional
    pub optional: bool,
}

/// Compile script setup with inline template (Vue's inline template mode)
pub(crate) fn compile_script_setup_inline(
    content: &str,
    component_name: &str,
    is_ts: bool,
    template: TemplateParts<'_>,
    normal_script_content: Option<&str>,
) -> Result<ScriptCompileResult, SfcError> {
    let mut ctx = ScriptCompileContext::new(content);
    ctx.analyze();

    let mut output = String::new();

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
        output.push_str("import { mergeDefaults as _mergeDefaults } from 'vue'\n");
    }

    // useModel import if defineModel was used
    if has_define_model {
        output.push_str("import { useModel as _useModel } from 'vue'\n");
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
            output.push_str("import { defineComponent, PropType } from 'vue'\n");
        } else {
            output.push_str("import { defineComponent } from 'vue'\n");
        }
    }

    // Template imports (Vue helpers)
    if !template.imports.is_empty() {
        output.push_str(template.imports);
        // Blank line after template imports
        output.push('\n');
    }

    // Extract user imports
    let mut user_imports = Vec::new();
    let mut setup_lines = Vec::new();
    let mut hoisted_lines = Vec::new(); // const with literals go outside export default

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
            macro_angle_depth += trimmed.matches('<').count() as i32;
            macro_angle_depth -= trimmed.matches('>').count() as i32;
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
            // Track angle brackets for type args
            macro_angle_depth += trimmed.matches('<').count() as i32;
            macro_angle_depth -= trimmed.matches('>').count() as i32;
            if macro_angle_depth <= 0 && (trimmed.ends_with("()") || trimmed.ends_with(')')) {
                waiting_for_macro_close = false;
                destructure_buffer.clear();
            }
            continue;
        }

        if in_destructure {
            destructure_buffer.push_str(line);
            destructure_buffer.push('\n');
            // Track both braces and angle brackets for type args
            brace_depth += trimmed.matches('{').count() as i32;
            brace_depth -= trimmed.matches('}').count() as i32;
            macro_angle_depth += trimmed.matches('<').count() as i32;
            macro_angle_depth -= trimmed.matches('>').count() as i32;
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
                if ts_type_depth <= 0
                    && (trimmed.ends_with(';')
                        || (!trimmed.ends_with('|')
                            && !trimmed.ends_with('&')
                            && !trimmed.ends_with(',')
                            && !trimmed.ends_with('{')))
                {
                    // Single line type, just skip
                    continue;
                }
                in_ts_type = true;
            }
            continue;
        }

        if !trimmed.is_empty() && !is_macro_call_line(trimmed) {
            // Check if this is a hoistable const (const with literal value, no function calls)
            if is_hoistable_const(trimmed) {
                hoisted_lines.push(line.to_string());
            } else {
                setup_lines.push(line.to_string());
            }
        }
    }

    // User imports (with blank line after template imports)
    for import in &user_imports {
        if let Some(processed) = process_import_for_types(import) {
            if !processed.is_empty() {
                output.push_str(&processed);
            }
        }
    }

    // Template hoisted consts (e.g., const _hoisted_1 = { class: "..." })
    if !template.hoisted.is_empty() {
        output.push('\n');
        output.push_str(template.hoisted);
    }

    // User hoisted const declarations (outside export default)
    if !hoisted_lines.is_empty() {
        output.push('\n');
        for line in &hoisted_lines {
            output.push_str(line);
            output.push('\n');
        }
    }

    // Start export default (blank line before)
    output.push('\n');
    let has_options = ctx.macros.define_options.is_some();
    if has_options {
        // Use Object.assign for defineOptions
        output.push_str("export default /*@__PURE__*/Object.assign(");
        let options_args = ctx.macros.define_options.as_ref().unwrap().args.trim();
        output.push_str(options_args);
        output.push_str(", {\n");
    } else if is_ts {
        // TypeScript: use defineComponent
        output.push_str("export default defineComponent({\n");
    } else {
        output.push_str("export default {\n");
    }
    // Use 'name' for defineComponent (TypeScript), '__name' for plain object (JavaScript)
    if is_ts {
        output.push_str("  name: '");
    } else {
        output.push_str("  __name: '");
    }
    output.push_str(component_name);
    output.push_str("',\n");

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
                output.push_str("  props: {\n");
                // Sort props for deterministic output
                let mut sorted_props: Vec<_> = prop_types.iter().collect();
                sorted_props.sort_by(|a, b| a.0.cmp(b.0));
                for (name, prop_type) in sorted_props {
                    output.push_str("    ");
                    output.push_str(name);
                    output.push_str(": { type: ");
                    output.push_str(&prop_type.js_type);
                    // Add PropType for TypeScript output
                    if is_ts {
                        if let Some(ref ts_type) = prop_type.ts_type {
                            output.push_str(" as PropType<");
                            output.push_str(ts_type);
                            output.push('>');
                        }
                    }
                    output.push_str(", required: ");
                    output.push_str(if prop_type.optional { "false" } else { "true" });
                    // Add default value from withDefaults or props destructure
                    let mut has_default = false;
                    if let Some(ref defaults) = with_defaults_args {
                        if let Some(default_val) = defaults.get(name.as_str()) {
                            output.push_str(", default: ");
                            output.push_str(default_val);
                            has_default = true;
                        }
                    }
                    // Also check props destructure defaults (Vue 3.4+ reactive props destructure)
                    if !has_default {
                        if let Some(ref destructure) = ctx.macros.props_destructure {
                            if let Some(binding) = destructure.bindings.get(name) {
                                if let Some(ref default_val) = binding.default {
                                    output.push_str(", default: ");
                                    output.push_str(default_val);
                                }
                            }
                        }
                    }
                    output.push_str(" },\n");
                }
                // Add model props if any
                for (model_name, _, options) in &model_infos {
                    output.push_str("    ");
                    output.push_str(model_name);
                    output.push_str(": ");
                    if let Some(opts) = options {
                        output.push_str(opts);
                    } else {
                        output.push_str("{}");
                    }
                    output.push_str(",\n");
                }
                output.push_str("  },\n");
            }
        } else if !props_macro.args.is_empty() {
            if needs_merge_defaults {
                // Use mergeDefaults format: _mergeDefaults(['prop1', 'prop2'], { prop2: default })
                let destructure = ctx.macros.props_destructure.as_ref().unwrap();
                output.push_str("  props: /*@__PURE__*/_mergeDefaults(");
                output.push_str(&props_macro.args);
                output.push_str(", {\n");
                // Collect defaults
                let defaults: Vec<_> = destructure
                    .bindings
                    .iter()
                    .filter_map(|(k, b)| b.default.as_ref().map(|d| (k.as_str(), d.as_str())))
                    .collect();
                for (i, (key, default_val)) in defaults.iter().enumerate() {
                    output.push_str("  ");
                    output.push_str(key);
                    output.push_str(": ");
                    output.push_str(default_val);
                    if i < defaults.len() - 1 {
                        output.push(',');
                    }
                    output.push('\n');
                }
                output.push_str("}),\n");
            } else {
                output.push_str("  props: ");
                output.push_str(&props_macro.args);
                output.push_str(",\n");
            }
        }
    }

    // Add model props to props definition if defineModel was used and no defineProps
    if !model_infos.is_empty() && ctx.macros.define_props.is_none() {
        output.push_str("  props: {\n");
        for (model_name, _binding_name, options) in &model_infos {
            output.push_str("    ");
            output.push_str(model_name);
            output.push_str(": ");
            if let Some(opts) = options {
                output.push_str(opts);
            } else {
                output.push_str("{}");
            }
            output.push_str(",\n");
        }
        output.push_str("  },\n");
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
        output.push_str("  emits: [");
        for (i, name) in all_emits.iter().enumerate() {
            if i > 0 {
                output.push_str(", ");
            }
            output.push('"');
            output.push_str(name);
            output.push('"');
        }
        output.push_str("],\n");
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
        output.push_str("  setup(__props) {\n");
    } else {
        output.push_str("  setup(__props, { ");
        output.push_str(&setup_args.join(", "));
        output.push_str(" }) {\n");
    }

    // Always add a blank line after setup signature
    output.push('\n');

    // Emit binding: const emit = __emit
    if let Some(ref emits_macro) = ctx.macros.define_emits {
        if let Some(ref binding_name) = emits_macro.binding_name {
            output.push_str("const ");
            output.push_str(binding_name);
            output.push_str(" = __emit\n");
        }
    }

    // Props binding: const props = __props
    if let Some(ref props_macro) = ctx.macros.define_props {
        if let Some(ref binding_name) = props_macro.binding_name {
            output.push_str("const ");
            output.push_str(binding_name);
            output.push_str(" = __props\n");
        }
    }

    // Model bindings: const model = _useModel(__props, 'modelValue')
    if !model_infos.is_empty() {
        for (model_name, binding_name, _) in &model_infos {
            output.push_str("const ");
            output.push_str(binding_name);
            output.push_str(" = _useModel(__props, \"");
            output.push_str(model_name);
            output.push_str("\")\n");
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
        output.push_str(line);
        output.push('\n');
    }

    // defineExpose: transform to __expose(...)
    if let Some(ref expose_macro) = ctx.macros.define_expose {
        let args = expose_macro.args.trim();
        output.push_str("__expose(");
        output.push_str(args);
        output.push_str(")\n");
    }

    // Inline render function as return (blank line before)
    output.push('\n');
    if !template.render_body.is_empty() {
        output.push_str("return (_ctx, _cache) => {\n");

        // Output component/directive resolution statements (preamble)
        for line in template.preamble.lines() {
            if !line.trim().is_empty() {
                output.push_str("  ");
                output.push_str(line);
                output.push('\n');
            }
        }
        if !template.preamble.is_empty() {
            output.push('\n');
        }

        // Indent the render body properly
        let mut first_line = true;
        for line in template.render_body.lines() {
            if first_line {
                output.push_str("  return ");
                output.push_str(line);
                first_line = false;
            } else {
                output.push('\n');
                // Preserve existing indentation by adding 2 spaces (setup indent)
                if !line.trim().is_empty() {
                    output.push_str("  ");
                }
                output.push_str(line);
            }
        }
        output.push('\n');
        output.push_str("}\n");
    }

    output.push_str("}\n");
    output.push('\n');
    if has_options || is_ts {
        // Close defineComponent() or Object.assign()
        output.push_str("})\n");
    } else {
        output.push_str("}\n");
    }

    // Transform TypeScript to JavaScript only for non-TypeScript output
    // For TypeScript output, keep the code as-is
    let transformed_code = if is_ts {
        // Keep TypeScript as-is (no transformation)
        output
    } else {
        // Transform TypeScript syntax to JavaScript
        transform_typescript_to_js(&output)
    };

    // Prepend preserved normal script content (type definitions, interfaces, etc.)
    // This is added AFTER transformation to preserve TypeScript-only constructs
    let final_code = if let Some(normal_script) = preserved_normal_script {
        format!("{}\n\n{}", normal_script, transformed_code)
    } else {
        transformed_code
    };

    Ok(ScriptCompileResult {
        code: final_code,
        bindings: Some(ctx.bindings),
    })
}

/// Compile script block(s)
#[allow(dead_code)]
pub(crate) fn compile_script(
    descriptor: &SfcDescriptor,
    _options: &ScriptCompileOptions,
    component_name: &str,
    is_vapor: bool,
    is_ts: bool,
) -> Result<ScriptCompileResult, SfcError> {
    // Handle script setup
    if let Some(script_setup) = &descriptor.script_setup {
        let template_content = descriptor.template.as_ref().map(|t| t.content.as_ref());
        compile_script_setup(
            &script_setup.content,
            component_name,
            is_vapor,
            is_ts,
            template_content,
        )
    } else if let Some(script) = &descriptor.script {
        // Use regular script, wrapped in __sfc__
        let mut code = String::new();
        code.push_str(&script.content);
        if is_vapor {
            code.push_str("\nconst __sfc__ = { ...(__default__ || {}), __vapor: true }\n");
        } else {
            code.push_str("\nconst __sfc__ = __default__\n");
        }
        // Transform TypeScript to JavaScript using OXC if lang="ts"
        let final_code = if is_ts {
            transform_typescript_to_js(&code)
        } else {
            code
        };
        Ok(ScriptCompileResult {
            code: final_code,
            bindings: None,
        })
    } else {
        // No script - generate empty component
        if is_vapor {
            Ok(ScriptCompileResult {
                code: "const __sfc__ = { __vapor: true }\n".to_string(),
                bindings: None,
            })
        } else {
            Ok(ScriptCompileResult {
                code: "const __sfc__ = {}\n".to_string(),
                bindings: None,
            })
        }
    }
}

/// Compile script setup content following Vue.js core format
#[allow(dead_code)]
pub(crate) fn compile_script_setup(
    content: &str,
    component_name: &str,
    is_vapor: bool,
    is_ts: bool,
    template_content: Option<&str>,
) -> Result<ScriptCompileResult, SfcError> {
    let mut ctx = ScriptCompileContext::new(content);
    ctx.analyze();

    let mut output = String::new();

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
            macro_angle_depth += trimmed.matches('<').count() as i32;
            macro_angle_depth -= trimmed.matches('>').count() as i32;

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
        output.push_str("import { defineVaporComponent as _defineVaporComponent } from 'vue'\n");
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
        output.push_str("import { mergeDefaults as _mergeDefaults } from 'vue'\n");
    }

    // Output imports (filtering out type-only imports)
    for import in &imports {
        if let Some(processed) = process_import_for_types(import) {
            if !processed.is_empty() {
                output.push_str(&processed);
            }
        }
    }

    output.push('\n');

    // Add comment for props destructure
    if has_props_destructure {
        output.push_str("// Reactive Props Destructure (Vue 3.5+)\n\n");
    }

    // Start __sfc__ definition
    if is_vapor {
        output.push_str("const __sfc__ = /*@__PURE__*/_defineVaporComponent({\n");
    } else {
        output.push_str("const __sfc__ = {\n");
    }
    output.push_str("  __name: '");
    output.push_str(component_name);
    output.push_str("',\n");

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

            output.push_str("  props: /*@__PURE__*/_mergeDefaults(");
            output.push_str(original_props);
            output.push_str(", {\n");

            // Add defaults
            for (key, binding) in &destructure.bindings {
                if let Some(ref default_val) = binding.default {
                    output.push_str("  ");
                    output.push_str(key);
                    output.push_str(": ");
                    output.push_str(default_val);
                    output.push('\n');
                }
            }
            output.push_str("}),\n");
        } else {
            // No defaults - just use the original props array
            if let Some(ref props_macro) = ctx.macros.define_props {
                if !props_macro.args.is_empty() {
                    output.push_str("  props: ");
                    output.push_str(&props_macro.args);
                    output.push_str(",\n");
                }
            }
        }
    } else if let Some(ref props_macro) = ctx.macros.define_props {
        if let Some(ref type_args) = props_macro.type_args {
            // For type-based props, extract full prop definitions
            let prop_types = extract_prop_types_from_type(type_args);
            if !prop_types.is_empty() {
                output.push_str("  props: {\n");
                // Sort props for deterministic output
                let mut sorted_props: Vec<_> = prop_types.iter().collect();
                sorted_props.sort_by(|a, b| a.0.cmp(b.0));
                for (name, prop_type) in sorted_props {
                    output.push_str("    ");
                    output.push_str(name);
                    output.push_str(": { type: ");
                    output.push_str(&prop_type.js_type);
                    output.push_str(", required: ");
                    output.push_str(if prop_type.optional { "false" } else { "true" });
                    output.push_str(" },\n");
                }
                output.push_str("  },\n");
            }
        } else if !props_macro.args.is_empty() {
            output.push_str("  props: ");
            output.push_str(&props_macro.args);
            output.push_str(",\n");
        }
    }

    // Emits definition if defineEmits was used
    if let Some(ref emits_macro) = ctx.macros.define_emits {
        if let Some(ref type_args) = emits_macro.type_args {
            // Extract emit names from type
            let emit_names = extract_emit_names_from_type(type_args);
            if !emit_names.is_empty() {
                output.push_str("  emits: [");
                for (i, name) in emit_names.iter().enumerate() {
                    if i > 0 {
                        output.push_str(", ");
                    }
                    output.push('"');
                    output.push_str(name);
                    output.push('"');
                }
                output.push_str("],\n");
            }
        } else if !emits_macro.args.is_empty() {
            output.push_str("  emits: ");
            output.push_str(&emits_macro.args);
            output.push_str(",\n");
        }
    }

    // Setup function
    output.push_str("  setup(__props, { expose: __expose, emit: __emit }) {\n");

    // defineExpose: transform to __expose(...)
    if let Some(ref expose_macro) = ctx.macros.define_expose {
        // args contains the argument content (e.g., "{ foo, bar }")
        let args = expose_macro.args.trim();
        if args.is_empty() {
            output.push_str("  __expose();\n");
        } else {
            output.push_str("  __expose(");
            output.push_str(args);
            output.push_str(");\n");
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
        output.push_str("  const ");
        output.push_str(binding_name);
        output.push_str(" = __emit\n");
    }

    // Collect props binding for exclusion from __returned__ (props themselves shouldn't be in returned)
    let mut props_binding_names: std::collections::HashSet<String> =
        std::collections::HashSet::new();

    // defineProps binding: const props = __props (only if not destructured)
    if !has_props_destructure {
        if let Some(ref props_macro) = ctx.macros.define_props {
            if let Some(ref binding_name) = props_macro.binding_name {
                output.push_str("  const ");
                output.push_str(binding_name);
                output.push_str(" = __props\n");
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
            output.push_str("  ");
            output.push_str(line);
        }
        output.push('\n');
    }

    // Compiler macros preset - these are compile-time only and should not be in __returned__
    let compiler_macros: std::collections::HashSet<&str> = [
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
    let destructured_prop_locals: std::collections::HashSet<String> = ctx
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
        let (root, _) = vue_compiler_core::parser::parse(&allocator, template_src);
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

    output.push_str("  const __returned__ = { ");
    output.push_str(&all_bindings.join(", "));
    output.push_str(" }\n");
    output.push_str("  Object.defineProperty(__returned__, '__isScriptSetup', { enumerable: false, value: true })\n");
    output.push_str("  return __returned__\n");

    output.push_str("  }\n\n");
    // Close the component definition
    if is_vapor {
        output.push_str("});\n"); // Close _defineVaporComponent(
    } else {
        output.push_str("};\n");
    }

    // Transform TypeScript to JavaScript using OXC if lang="ts"
    let final_code = if is_ts {
        transform_typescript_to_js(&output)
    } else {
        output
    };

    Ok(ScriptCompileResult {
        code: final_code,
        bindings: Some(ctx.bindings),
    })
}

/// Process import statement to remove TypeScript type-only imports using OXC
/// Returns None if the entire import should be removed, Some(processed) otherwise
pub(crate) fn process_import_for_types(import: &str) -> Option<String> {
    let import = import.trim();

    // Parse the import statement with OXC
    let allocator = Allocator::default();
    let source_type = SourceType::ts();
    let parser = Parser::new(&allocator, import, source_type);
    let result = parser.parse();

    if result.errors.is_empty() {
        for stmt in &result.program.body {
            if let Statement::ImportDeclaration(decl) = stmt {
                // Skip type-only imports: import type { ... } from '...'
                if decl.import_kind.is_type() {
                    return None;
                }

                // Check if there are any specifiers
                if let Some(specifiers) = &decl.specifiers {
                    // Filter out type-only specifiers
                    let value_specifiers: Vec<&ImportDeclarationSpecifier> = specifiers
                        .iter()
                        .filter(|spec| match spec {
                            ImportDeclarationSpecifier::ImportSpecifier(s) => {
                                !s.import_kind.is_type()
                            }
                            _ => true,
                        })
                        .collect();

                    if value_specifiers.is_empty() {
                        // All specifiers were type imports
                        return None;
                    }

                    if value_specifiers.len() != specifiers.len() {
                        // Some specifiers were filtered out, rebuild the import
                        let source = decl.source.value.as_str();
                        let specifier_strs: Vec<String> = value_specifiers
                            .iter()
                            .map(|spec| match spec {
                                ImportDeclarationSpecifier::ImportSpecifier(s) => {
                                    let imported = s.imported.name().as_str();
                                    let local = s.local.name.as_str();
                                    if imported == local {
                                        imported.to_string()
                                    } else {
                                        format!("{} as {}", imported, local)
                                    }
                                }
                                ImportDeclarationSpecifier::ImportDefaultSpecifier(s) => {
                                    s.local.name.to_string()
                                }
                                ImportDeclarationSpecifier::ImportNamespaceSpecifier(s) => {
                                    format!("* as {}", s.local.name)
                                }
                            })
                            .collect();

                        let new_import = format!(
                            "import {{ {} }} from '{}'\n",
                            specifier_strs.join(", "),
                            source
                        );
                        return Some(new_import);
                    }
                }
            }
        }
    }

    // Regular import or parse failed, return as-is
    Some(import.to_string() + "\n")
}

/// Extract all identifiers from an import statement (including default imports)
pub(crate) fn extract_import_identifiers(import: &str) -> Vec<String> {
    let import = import.trim();
    let mut identifiers = Vec::new();

    // Parse the import statement with OXC
    let allocator = Allocator::default();
    let source_type = SourceType::ts();
    let parser = Parser::new(&allocator, import, source_type);
    let result = parser.parse();

    if result.errors.is_empty() {
        for stmt in &result.program.body {
            if let Statement::ImportDeclaration(decl) = stmt {
                // Skip type-only imports
                if decl.import_kind.is_type() {
                    continue;
                }

                if let Some(specifiers) = &decl.specifiers {
                    for spec in specifiers {
                        match spec {
                            ImportDeclarationSpecifier::ImportSpecifier(s) => {
                                // Skip type-only specifiers
                                if !s.import_kind.is_type() {
                                    identifiers.push(s.local.name.to_string());
                                }
                            }
                            ImportDeclarationSpecifier::ImportDefaultSpecifier(s) => {
                                identifiers.push(s.local.name.to_string());
                            }
                            ImportDeclarationSpecifier::ImportNamespaceSpecifier(s) => {
                                identifiers.push(s.local.name.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    identifiers
}

/// Transform TypeScript code to JavaScript using OXC
pub(crate) fn transform_typescript_to_js(code: &str) -> String {
    let allocator = Allocator::default();
    let source_type = SourceType::ts();
    let parser = Parser::new(&allocator, code, source_type);
    let parse_result = parser.parse();

    if !parse_result.errors.is_empty() {
        // If parsing fails, return original code
        return code.to_string();
    }

    let mut program = parse_result.program;

    // Run semantic analysis to get symbols and scopes
    let semantic_ret = SemanticBuilder::new()
        .with_excess_capacity(2.0)
        .build(&program);

    if !semantic_ret.errors.is_empty() {
        // If semantic analysis fails, return original code
        return code.to_string();
    }

    let (symbols, scopes) = semantic_ret.semantic.into_symbol_table_and_scope_tree();

    // Transform TypeScript to JavaScript
    let transform_options = TransformOptions::default();
    let ret = Transformer::new(&allocator, std::path::Path::new(""), &transform_options)
        .build_with_symbols_and_scopes(symbols, scopes, &mut program);

    if !ret.errors.is_empty() {
        // If transformation fails, return original code
        return code.to_string();
    }

    // Generate JavaScript code
    // Replace tabs with 2 spaces for consistent indentation
    Codegen::new().build(&program).code.replace('\t', "  ")
}

/// Check if a line is a compiler macro call
pub(crate) fn is_macro_call_line(line: &str) -> bool {
    let macros = [
        "defineProps",
        "defineEmits",
        "defineExpose",
        "defineOptions",
        "defineSlots",
        "defineModel",
        "withDefaults",
    ];

    // Check if line contains a macro that is being called (not just imported)
    for macro_name in macros {
        if line.contains(macro_name) && line.contains('(') {
            // Make sure it's not an import
            if !line.trim().starts_with("import") {
                return true;
            }
        }
    }
    false
}

/// Check if a line starts a multi-line paren-based macro call (e.g., defineExpose({)
pub(crate) fn is_paren_macro_start(line: &str) -> bool {
    let macros = [
        "defineProps",
        "defineEmits",
        "defineExpose",
        "defineOptions",
        "defineSlots",
        "defineModel",
        "withDefaults",
    ];

    // Check if line contains a macro call that isn't complete on the same line
    for macro_name in macros {
        if line.contains(macro_name) && !line.trim().starts_with("import") {
            // Check for unbalanced parentheses (call spans multiple lines)
            if line.contains('(') {
                let open_count = line.matches('(').count();
                let close_count = line.matches(')').count();
                if open_count > close_count {
                    return true;
                }
            }
        }
    }
    false
}

/// Check if a line is a hoistable const (const with literal value, no function calls)
/// These can be placed outside of setup() for optimization
pub(crate) fn is_hoistable_const(line: &str) -> bool {
    let trimmed = line.trim();

    // Must start with "const " (not let/var)
    if !trimmed.starts_with("const ") {
        return false;
    }

    // Must not contain function calls (parentheses)
    if trimmed.contains('(') {
        return false;
    }

    // Must not contain arrow functions
    if trimmed.contains("=>") {
        return false;
    }

    // Must be a simple assignment (contains =)
    if !trimmed.contains('=') {
        return false;
    }

    // Must not be a destructure
    if trimmed.starts_with("const {") || trimmed.starts_with("const [") {
        return false;
    }

    // Must not be an incomplete multiline generic (e.g., "const x = ref<")
    // This would be part of a multiline type declaration
    let open_angles = trimmed.matches('<').count();
    let close_angles = trimmed.matches('>').count();
    if open_angles > close_angles {
        return false;
    }

    // Must end with a semicolon or be a complete statement
    // (avoid hoisting incomplete lines)
    if !trimmed.ends_with(';') && !trimmed.ends_with('}') && !trimmed.ends_with(']') {
        // Check if it looks like a complete simple literal assignment
        // const x = "value" or const x = 123 or const x = true
        let after_eq = trimmed.split('=').nth(1).map(|s| s.trim()).unwrap_or("");
        if after_eq.is_empty()
            || after_eq.ends_with('<')
            || (!after_eq.starts_with('"')
                && !after_eq.starts_with('\'')
                && !after_eq.starts_with('`')
                && after_eq.parse::<f64>().is_err()
                && after_eq != "true"
                && after_eq != "false"
                && after_eq != "null"
                && after_eq != "undefined")
        {
            return false;
        }
    }

    true
}

/// Check if a line starts a multi-line macro call (e.g., defineEmits<{ ... }>())
pub(crate) fn is_multiline_macro_start(line: &str) -> bool {
    let macros = [
        "defineProps",
        "defineEmits",
        "defineExpose",
        "defineOptions",
        "defineSlots",
        "defineModel",
        "withDefaults",
    ];

    // Check if line contains a macro with type args that spans multiple lines
    // Pattern: contains macro name, contains '<', but doesn't have matching '>' on same line
    // or has '>' but no '()' yet
    for macro_name in macros {
        if line.contains(macro_name) && !line.trim().starts_with("import") {
            // Check for type args that might span multiple lines
            if line.contains('<') {
                let open_count = line.matches('<').count();
                let close_count = line.matches('>').count();
                // If angle brackets aren't balanced, it's multi-line
                if open_count > close_count {
                    return true;
                }
                // If balanced but no () at the end, might still be multi-line
                if open_count == close_count && !line.contains("()") && !line.ends_with(')') {
                    // Check if this is a complete single-line call
                    // e.g., defineEmits<(e: 'click') => void>() - this has ()
                    // vs defineEmits<{ - this doesn't have () yet
                    if !line.trim().ends_with("()") && !line.trim().ends_with(')') {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// Check if a line is a props destructure pattern
pub(crate) fn is_props_destructure_line(line: &str) -> bool {
    let trimmed = line.trim();
    // Match: const { ... } = defineProps or const { ... } = withDefaults
    (trimmed.starts_with("const {") || trimmed.starts_with("let {") || trimmed.starts_with("var {"))
        && (trimmed.contains("defineProps") || trimmed.contains("withDefaults"))
}

/// Extract prop types from TypeScript type definition
pub(crate) fn extract_prop_types_from_type(
    type_args: &str,
) -> std::collections::HashMap<String, PropTypeInfo> {
    let mut props = std::collections::HashMap::new();

    let content = type_args.trim();
    let content = if content.starts_with('{') && content.ends_with('}') {
        &content[1..content.len() - 1]
    } else {
        content
    };

    // Split by commas/semicolons/newlines (but not inside nested braces)
    let mut depth = 0;
    let mut current = String::new();

    for c in content.chars() {
        match c {
            '{' | '<' | '(' | '[' => {
                depth += 1;
                current.push(c);
            }
            '}' | '>' | ')' | ']' => {
                depth -= 1;
                current.push(c);
            }
            ',' | ';' | '\n' if depth == 0 => {
                extract_prop_type_info(&current, &mut props);
                current.clear();
            }
            _ => current.push(c),
        }
    }
    extract_prop_type_info(&current, &mut props);

    props
}

fn extract_prop_type_info(
    segment: &str,
    props: &mut std::collections::HashMap<String, PropTypeInfo>,
) {
    let trimmed = segment.trim();
    if trimmed.is_empty() {
        return;
    }

    // Parse "name?: Type" or "name: Type"
    if let Some(colon_pos) = trimmed.find(':') {
        let name_part = &trimmed[..colon_pos];
        let type_part = &trimmed[colon_pos + 1..];

        let optional = name_part.ends_with('?');
        let name = name_part.trim().trim_end_matches('?').trim();

        if !name.is_empty() && is_valid_identifier(name) {
            let ts_type_str = type_part.trim().to_string();
            let js_type = ts_type_to_js_type(&ts_type_str);
            props.insert(
                name.to_string(),
                PropTypeInfo {
                    js_type,
                    ts_type: Some(ts_type_str),
                    optional,
                },
            );
        }
    }
}

/// Convert TypeScript type to JavaScript type constructor
fn ts_type_to_js_type(ts_type: &str) -> String {
    let ts_type = ts_type.trim();

    // Handle string literal types: "foo" or 'bar' -> String
    if (ts_type.starts_with('"') && ts_type.ends_with('"'))
        || (ts_type.starts_with('\'') && ts_type.ends_with('\''))
    {
        return "String".to_string();
    }

    // Handle numeric literal types: 123, 1.5 -> Number
    if ts_type.parse::<f64>().is_ok() {
        return "Number".to_string();
    }

    // Handle boolean literal types: true, false -> Boolean
    if ts_type == "true" || ts_type == "false" {
        return "Boolean".to_string();
    }

    // Handle union types - take the first non-undefined/null type
    if ts_type.contains('|') {
        let parts: Vec<&str> = ts_type.split('|').collect();
        for part in parts {
            let part = part.trim();
            if part != "undefined" && part != "null" {
                return ts_type_to_js_type(part);
            }
        }
    }

    // Map TypeScript types to JavaScript constructors
    match ts_type.to_lowercase().as_str() {
        "string" => "String".to_string(),
        "number" => "Number".to_string(),
        "boolean" => "Boolean".to_string(),
        "object" => "Object".to_string(),
        "function" => "Function".to_string(),
        "symbol" => "Symbol".to_string(),
        _ => {
            // Handle array types
            if ts_type.ends_with("[]") || ts_type.starts_with("Array<") {
                "Array".to_string()
            } else if ts_type.starts_with('{') || ts_type.contains(':') {
                // Object literal type
                "Object".to_string()
            } else if ts_type.starts_with('(') && ts_type.contains("=>") {
                // Function type
                "Function".to_string()
            } else {
                // Default to the type name with first letter capitalized
                let mut chars = ts_type.chars();
                match chars.next() {
                    Some(first) => first.to_uppercase().chain(chars).collect(),
                    None => "Object".to_string(),
                }
            }
        }
    }
}

/// Extract emit names from TypeScript type definition
pub(crate) fn extract_emit_names_from_type(type_args: &str) -> Vec<String> {
    let mut emits = Vec::new();

    // Match patterns like: (e: 'eventName') or (event: 'eventName', ...)
    let mut in_string = false;
    let mut quote_char = ' ';
    let mut current_string = String::new();

    for c in type_args.chars() {
        if !in_string && (c == '\'' || c == '"') {
            in_string = true;
            quote_char = c;
            current_string.clear();
        } else if in_string && c == quote_char {
            in_string = false;
            if !current_string.is_empty() {
                emits.push(current_string.clone());
            }
        } else if in_string {
            current_string.push(c);
        }
    }

    emits
}

/// Extract default values from withDefaults second argument
/// Input: "withDefaults(defineProps<{...}>(), { prop1: default1, prop2: default2 })"
/// Returns: HashMap of prop name to default value string
pub(crate) fn extract_with_defaults_defaults(
    with_defaults_args: &str,
) -> std::collections::HashMap<String, String> {
    let mut defaults = std::collections::HashMap::new();

    // Find the second argument (the defaults object)
    // withDefaults(defineProps<...>(), { ... })
    // We need to find the { after "defineProps<...>()"

    let content = with_defaults_args.trim();
    let chars: Vec<char> = content.chars().collect();

    // First, find "defineProps" and then its closing parenthesis
    let define_props_pos = content.find("defineProps");
    if define_props_pos.is_none() {
        return defaults;
    }

    let start_search = define_props_pos.unwrap();
    let mut paren_depth = 0;
    let mut in_define_props_call = false;
    let mut found_define_props_end = false;
    let mut defaults_start = None;

    let mut i = start_search;
    while i < chars.len() {
        let c = chars[i];

        if !in_define_props_call {
            // Looking for the opening paren of defineProps()
            if c == '(' {
                in_define_props_call = true;
                paren_depth = 1;
            }
        } else if !found_define_props_end {
            match c {
                '(' => paren_depth += 1,
                ')' => {
                    paren_depth -= 1;
                    if paren_depth == 0 {
                        found_define_props_end = true;
                    }
                }
                _ => {}
            }
        } else {
            // Looking for the defaults object start
            if c == '{' {
                defaults_start = Some(i);
                break;
            }
        }
        i += 1;
    }

    if let Some(start) = defaults_start {
        // Find matching closing brace
        let mut brace_depth = 0;
        let mut end = start;

        for (j, &c) in chars.iter().enumerate().skip(start) {
            match c {
                '{' => brace_depth += 1,
                '}' => {
                    brace_depth -= 1;
                    if brace_depth == 0 {
                        end = j;
                        break;
                    }
                }
                _ => {}
            }
        }

        // Extract the defaults object content (without braces)
        let defaults_content: String = chars[start + 1..end].iter().collect();
        parse_defaults_object(&defaults_content, &mut defaults);
    }

    defaults
}

/// Parse a JavaScript object literal to extract key-value pairs
fn parse_defaults_object(content: &str, defaults: &mut std::collections::HashMap<String, String>) {
    let content = content.trim();
    if content.is_empty() {
        return;
    }

    // Split by commas, but respect nested braces/parens/brackets
    let mut depth = 0;
    let mut current = String::new();

    for c in content.chars() {
        match c {
            '{' | '(' | '[' => {
                depth += 1;
                current.push(c);
            }
            '}' | ')' | ']' => {
                depth -= 1;
                current.push(c);
            }
            ',' if depth == 0 => {
                extract_default_pair(&current, defaults);
                current.clear();
            }
            _ => current.push(c),
        }
    }
    extract_default_pair(&current, defaults);
}

/// Extract a single key: value pair from a default definition
fn extract_default_pair(pair: &str, defaults: &mut std::collections::HashMap<String, String>) {
    let trimmed = pair.trim();
    if trimmed.is_empty() {
        return;
    }

    // Find the first : that's not inside a nested structure
    let mut depth = 0;
    let mut colon_pos = None;

    for (i, c) in trimmed.chars().enumerate() {
        match c {
            '{' | '(' | '[' | '<' => depth += 1,
            '}' | ')' | ']' | '>' => depth -= 1,
            ':' if depth == 0 => {
                colon_pos = Some(i);
                break;
            }
            _ => {}
        }
    }

    if let Some(pos) = colon_pos {
        let key = trimmed[..pos].trim();
        let value = trimmed[pos + 1..].trim();

        if !key.is_empty() && !value.is_empty() {
            defaults.insert(key.to_string(), value.to_string());
        }
    }
}

/// Check if a string is a valid JS identifier
pub(crate) fn is_valid_identifier(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }

    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_alphabetic() || c == '_' || c == '$' => {}
        _ => return false,
    }

    chars.all(|c| c.is_alphanumeric() || c == '_' || c == '$')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compile_empty_script() {
        let descriptor = SfcDescriptor::default();
        let result =
            compile_script(&descriptor, &Default::default(), "Test", false, false).unwrap();
        assert!(result.code.contains("__sfc__"));
    }

    #[test]
    fn test_is_valid_identifier() {
        assert!(is_valid_identifier("foo"));
        assert!(is_valid_identifier("_bar"));
        assert!(is_valid_identifier("$baz"));
        assert!(is_valid_identifier("foo123"));
        assert!(!is_valid_identifier("123foo"));
        assert!(!is_valid_identifier(""));
        assert!(!is_valid_identifier("foo-bar"));
    }

    #[test]
    fn test_extract_with_defaults_defaults() {
        // Test simple case
        let input = r#"withDefaults(defineProps<{ msg?: string }>(), { msg: "hello" })"#;
        let defaults = extract_with_defaults_defaults(input);
        eprintln!("Defaults: {:?}", defaults);
        assert_eq!(defaults.get("msg"), Some(&r#""hello""#.to_string()));

        // Test multiple defaults
        let input2 = r#"withDefaults(defineProps<{ msg?: string, count?: number }>(), { msg: "hello", count: 42 })"#;
        let defaults2 = extract_with_defaults_defaults(input2);
        assert_eq!(defaults2.get("msg"), Some(&r#""hello""#.to_string()));
        assert_eq!(defaults2.get("count"), Some(&"42".to_string()));

        // Test multiline input like AfCheckbox
        let input3 = r#"withDefaults(
  defineProps<{
    checked: boolean;
    label?: string;
    color?: "primary" | "secondary";
  }>(),
  {
    label: undefined,
    color: "primary",
  },
)"#;
        let defaults3 = extract_with_defaults_defaults(input3);
        eprintln!("Defaults3: {:?}", defaults3);
        assert_eq!(defaults3.get("label"), Some(&"undefined".to_string()));
        assert_eq!(defaults3.get("color"), Some(&r#""primary""#.to_string()));
    }

    #[test]
    fn test_compile_script_setup_with_define_props() {
        let content = r#"
import { ref } from 'vue'
const props = defineProps(['msg'])
const count = ref(0)
"#;
        let result = compile_script_setup(content, "Test", false, false, None).unwrap();

        // Should have __sfc__
        assert!(
            result.code.contains("const __sfc__ ="),
            "Should have __sfc__"
        );
        // Should have __name
        assert!(result.code.contains("__name: 'Test'"), "Should have __name");
        // Should have props definition
        assert!(
            result.code.contains("props: ['msg']"),
            "Should have props definition"
        );
        // Should have setup function with proper signature
        assert!(
            result
                .code
                .contains("setup(__props, { expose: __expose, emit: __emit })"),
            "Should have proper setup signature"
        );
        // __expose is only called if defineExpose is used (not in this test)
        // Should have __returned__
        assert!(
            result.code.contains("const __returned__ ="),
            "Should have __returned__"
        );
    }

    #[test]
    fn test_compile_script_setup_with_define_emits() {
        let content = r#"
const emit = defineEmits(['click', 'update'])
"#;
        let result = compile_script_setup(content, "Test", false, false, None).unwrap();

        println!("Full output:\n{}", result.code);

        assert!(
            result.code.contains("emits:"),
            "Should contain emits definition"
        );
        assert!(
            result.code.contains("const emit = __emit"),
            "Should bind emit to __emit"
        );
        // emit should be in __returned__ as it's a runtime value used in templates
        assert!(
            result.code.contains("emit"),
            "emit should be accessible in template"
        );
        // defineEmits should NOT be in the setup function
        assert!(
            !result.code.contains("defineEmits"),
            "defineEmits should be removed from setup"
        );
    }

    #[test]
    fn test_compile_script_setup_with_define_emits_usage() {
        let content = r#"
import { ref } from 'vue'
const emit = defineEmits(['click', 'update'])
const count = ref(0)
function onClick() {
    emit('click', count.value)
}
"#;
        let result = compile_script_setup(content, "Test", false, false, None).unwrap();

        // defineEmits call should NOT be in the setup function
        assert!(
            !result.code.contains("defineEmits"),
            "defineEmits call should be removed from setup"
        );
        // emit binding should be present
        assert!(
            result.code.contains("const emit = __emit"),
            "Should bind emit to __emit"
        );
        // onClick function should be in setup
        assert!(
            result.code.contains("function onClick()"),
            "onClick should be in setup"
        );
        // emits definition should be present
        assert!(
            result.code.contains("emits: ['click', 'update']"),
            "Should have emits definition"
        );
    }

    #[test]
    fn test_compile_script_setup_without_macros() {
        let content = r#"
import { ref } from 'vue'
const msg = ref('hello')
"#;
        let result = compile_script_setup(content, "Test", false, false, None).unwrap();

        // Should have setup
        assert!(result.code.contains("setup(__props"), "Should have setup");
        // Should NOT have props or emits definitions
        assert!(
            !result.code.contains("  props:"),
            "Should not contain props"
        );
        assert!(!result.code.contains("emits:"), "Should not contain emits");
    }

    #[test]
    fn test_compile_script_setup_with_props_destructure() {
        let content = r#"
import { computed } from 'vue'
const { count } = defineProps({ count: Number })
const double = computed(() => count * 2)
"#;
        let result = compile_script_setup(content, "Test", false, false, None).unwrap();

        println!("Compiled output:\n{}", result.code);

        // Should transform count to __props.count inside computed
        assert!(
            result.code.contains("__props.count"),
            "Should transform destructured prop to __props.count"
        );
        // The original `count` reference should be replaced
        assert!(
            result.code.contains("computed(() => __props.count * 2)"),
            "Should have transformed computed expression"
        );
        // Destructured props should NOT be in __returned__
        assert!(
            !result.code.contains("__returned__ = { computed, count,"),
            "Destructured props should not be in __returned__"
        );
        // Should have double and computed in __returned__
        assert!(
            result.code.contains("computed") && result.code.contains("double"),
            "Should have computed and double in __returned__"
        );
    }

    #[test]
    fn test_compiler_macros_not_in_returned() {
        let content = r#"
import { defineProps, ref } from 'vue'
const props = defineProps(['msg'])
const count = ref(0)
"#;
        let result = compile_script_setup(content, "Test", false, false, None).unwrap();

        println!("Compiled output:\n{}", result.code);

        // Find the __returned__ line and check its contents
        let returned_line = result
            .code
            .lines()
            .find(|line| line.contains("__returned__"))
            .expect("Should have __returned__ line");

        println!("__returned__ line: {}", returned_line);

        // Compiler macros should NOT be in __returned__
        assert!(
            !returned_line.contains("defineProps"),
            "Compiler macros should not be in __returned__"
        );
        // But regular imports should be
        assert!(
            returned_line.contains("ref"),
            "Regular imports should be in __returned__"
        );
    }

    #[test]
    fn test_props_destructure_with_defaults() {
        let content = r#"
import { computed, watch } from 'vue'

const {
  name,
  count = 0,
  disabled = false,
  items = () => []
} = defineProps<{
  name: string
  count?: number
  disabled?: boolean
  items?: string[]
}>()

const doubled = computed(() => count * 2)
const itemCount = computed(() => items.length)
"#;

        // First check context analysis
        let mut ctx = crate::script::ScriptCompileContext::new(content);
        ctx.analyze();

        println!("=== Context Analysis ===");
        println!("props_destructure: {:?}", ctx.macros.props_destructure);
        println!("bindings: {:?}", ctx.bindings.bindings);

        let result = compile_script_setup(content, "Test", false, false, None).unwrap();

        println!("\n=== Compiled output ===\n{}", result.code);

        // Should NOT contain the destructure statement
        assert!(
            !result.code.contains("const {"),
            "Should not contain destructure statement"
        );
        assert!(
            !result.code.contains("} = defineProps"),
            "Should not contain defineProps assignment"
        );

        // Should have props definition with defaults
        assert!(
            result.code.contains("props:"),
            "Should have props definition"
        );

        // Should transform props to __props
        assert!(
            result.code.contains("__props.count"),
            "Should transform count to __props.count"
        );
        assert!(
            result.code.contains("__props.items"),
            "Should transform items to __props.items"
        );

        // Should have the computed expressions transformed
        assert!(
            result.code.contains("computed(() => __props.count * 2)"),
            "Should transform count in computed. Got:\n{}",
            result.code
        );
    }

    #[test]
    fn test_extract_prop_types() {
        let type_args = r#"{
  name: string
  count?: number
  disabled?: boolean
  items?: string[]
}"#;
        let props = extract_prop_types_from_type(type_args);
        assert!(props.contains_key("name"), "Should extract name");
        assert!(props.contains_key("count"), "Should extract count");
        assert!(props.contains_key("disabled"), "Should extract disabled");
        assert!(props.contains_key("items"), "Should extract items");

        // Check types
        assert_eq!(props.get("name").unwrap().js_type, "String");
        assert_eq!(props.get("count").unwrap().js_type, "Number");
        assert_eq!(props.get("disabled").unwrap().js_type, "Boolean");
        assert_eq!(props.get("items").unwrap().js_type, "Array");

        // Check optionality
        assert!(!props.get("name").unwrap().optional);
        assert!(props.get("count").unwrap().optional);
        assert!(props.get("disabled").unwrap().optional);
        assert!(props.get("items").unwrap().optional);
    }

    #[test]
    fn test_compile_script_setup_with_multiline_define_emits() {
        let content = r#"
const emit = defineEmits<{
  (e: 'click', payload: MouseEvent): void
  (e: 'update', value: string): void
}>()

function handleClick(e: MouseEvent) {
    emit('click', e)
}
"#;
        let result = compile_script_setup(content, "Test", false, false, None).unwrap();

        println!("Multi-line defineEmits output:\n{}", result.code);

        // defineEmits should NOT be in the setup function
        assert!(
            !result.code.contains("defineEmits"),
            "defineEmits should be removed from setup"
        );
        // emit binding should be present
        assert!(
            result.code.contains("const emit = __emit"),
            "Should bind emit to __emit"
        );
        // handleClick function should be in setup
        assert!(
            result.code.contains("function handleClick"),
            "handleClick should be in setup"
        );
        // emits definition should be present
        assert!(
            result.code.contains("emits:"),
            "Should have emits definition"
        );
    }

    #[test]
    fn test_compile_script_setup_with_typed_define_emits_single_line() {
        let content = r#"
const emit = defineEmits<(e: 'click') => void>()
"#;
        let result = compile_script_setup(content, "Test", false, false, None).unwrap();

        println!("Typed defineEmits output:\n{}", result.code);

        // defineEmits should NOT be in the setup function
        assert!(
            !result.code.contains("defineEmits"),
            "defineEmits should be removed from setup"
        );
        // emit binding should be present
        assert!(
            result.code.contains("const emit = __emit"),
            "Should bind emit to __emit"
        );
    }

    #[test]
    fn test_compile_script_setup_with_define_expose() {
        let content = r#"
import { ref } from 'vue'
const count = ref(0)
const reset = () => count.value = 0
defineExpose({ count, reset })
"#;
        let result = compile_script_setup(content, "Test", false, false, None).unwrap();

        println!("defineExpose output:\n{}", result.code);

        // defineExpose should be transformed to __expose(...)
        assert!(
            result.code.contains("__expose({"),
            "Should have __expose call with arguments"
        );
        assert!(
            result.code.contains("count"),
            "__expose should include count"
        );
        assert!(
            result.code.contains("reset"),
            "__expose should include reset"
        );
        // defineExpose should NOT be in the setup function
        assert!(
            !result.code.contains("defineExpose"),
            "defineExpose should be removed from setup"
        );
    }

    #[test]
    fn test_compile_script_setup_without_define_expose() {
        let content = r#"
import { ref } from 'vue'
const count = ref(0)
"#;
        let result = compile_script_setup(content, "Test", false, false, None).unwrap();

        // __expose should NOT be called if defineExpose is not used
        assert!(
            !result.code.contains("__expose("),
            "Should not have __expose call without defineExpose"
        );
    }
}
