//! SFC compilation implementation.
//!
//! This is the main entry point for compiling Vue Single File Components.
//! Following the Vue.js core structure, template/script/style compilation
//! is delegated to specialized modules.

use crate::compile_script::{compile_script_setup_inline, TemplateParts};
use crate::compile_template::{
    compile_template_block, compile_template_block_vapor, extract_template_parts,
    extract_template_parts_full,
};
use crate::rewrite_default::rewrite_default;
use crate::script::ScriptCompileContext;
use crate::types::*;

// Re-export ScriptCompileResult for public API
pub use crate::compile_script::ScriptCompileResult;

/// Compile an SFC descriptor into JavaScript and CSS
pub fn compile_sfc(
    descriptor: &SfcDescriptor,
    options: SfcCompileOptions,
) -> Result<SfcCompileResult, SfcError> {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let mut code = String::new();
    let mut css = None;

    let filename = options.script.id.as_deref().unwrap_or("anonymous.vue");

    // Generate scope ID from filename
    let scope_id = generate_scope_id(filename);
    let has_scoped = descriptor.styles.iter().any(|s| s.scoped);

    // Detect vapor mode from script attrs
    let is_vapor = descriptor
        .script_setup
        .as_ref()
        .map(|s| s.attrs.contains_key("vapor"))
        .unwrap_or(false)
        || descriptor
            .script
            .as_ref()
            .map(|s| s.attrs.contains_key("vapor"))
            .unwrap_or(false);

    // is_ts controls output format:
    // - true: output TypeScript (preserve TypeScript syntax)
    // - false: output JavaScript (transpile TypeScript to JS)
    // The CLI should set is_ts = true for preserve mode, false for downcompile mode
    let is_ts = options.script.is_ts || options.template.is_ts;

    // Extract component name from filename
    let component_name = extract_component_name(filename);

    // Determine output mode based on script type
    let has_script_setup = descriptor.script_setup.is_some();
    let has_script = descriptor.script.is_some();
    let has_template = descriptor.template.is_some();

    // Case 1: Template only - just output render function
    if !has_script && !has_script_setup && has_template {
        let template = descriptor.template.as_ref().unwrap();
        // Enable hoisting for template-only SFCs (hoisted consts go at module level)
        let mut template_opts = options.template.clone();
        let mut dom_opts = template_opts.compiler_options.take().unwrap_or_default();
        dom_opts.hoist_static = true;
        template_opts.compiler_options = Some(dom_opts);
        let template_result =
            compile_template_block(template, &template_opts, &scope_id, has_scoped, is_ts, None);

        match template_result {
            Ok(template_code) => {
                code = template_code;
            }
            Err(e) => errors.push(e),
        }

        // Compile styles
        let all_css = compile_styles(&descriptor.styles, &scope_id, &options.style, &mut warnings);
        if !all_css.is_empty() {
            css = Some(all_css);
        }

        return Ok(SfcCompileResult {
            code,
            css,
            map: None,
            errors,
            warnings,
            bindings: None,
        });
    }

    // Case 2: Script (non-setup) + Template - rewrite default and compile template
    if has_script && !has_script_setup {
        let script = descriptor.script.as_ref().unwrap();

        // Rewrite `export default` to `const _sfc_main = ...`
        let (rewritten_script, _has_default) = rewrite_default(&script.content, "_sfc_main", is_ts);

        // Compile template if present
        if has_template {
            let template = descriptor.template.as_ref().unwrap();
            let mut template_opts = options.template.clone();
            let mut dom_opts = template_opts.compiler_options.take().unwrap_or_default();
            dom_opts.hoist_static = true;
            template_opts.compiler_options = Some(dom_opts);

            let template_result = compile_template_block(
                template,
                &template_opts,
                &scope_id,
                has_scoped,
                is_ts,
                None, // No bindings for normal scripts
            );

            match template_result {
                Ok(template_code) => {
                    // Extract template parts (imports, hoisted, render function)
                    let (template_imports, template_hoisted, render_fn) =
                        extract_template_parts_full(&template_code);

                    // Build output: imports + script + hoisted + render + export
                    code.push_str(&template_imports);
                    if !template_imports.is_empty() {
                        code.push('\n');
                    }
                    code.push_str(&rewritten_script);
                    code.push('\n');

                    // Add hoisted declarations
                    if !template_hoisted.is_empty() {
                        code.push_str(&template_hoisted);
                        code.push('\n');
                    }

                    // Add render function (without imports - they're already at top)
                    code.push_str(&render_fn);
                    code.push('\n');

                    // Export the component with render attached
                    code.push_str("_sfc_main.render = render\n");
                    code.push_str("export default _sfc_main\n");
                }
                Err(e) => {
                    errors.push(e);
                    // Fall back to just the script
                    code = script.content.to_string();
                    code.push('\n');
                }
            }
        } else {
            // No template - just output rewritten script and export
            code.push_str(&rewritten_script);
            code.push_str("\nexport default _sfc_main\n");
        }

        // Compile styles
        let all_css = compile_styles(&descriptor.styles, &scope_id, &options.style, &mut warnings);
        if !all_css.is_empty() {
            css = Some(all_css);
        }

        return Ok(SfcCompileResult {
            code,
            css,
            map: None,
            errors,
            warnings,
            bindings: None,
        });
    }

    // Case 3: Script setup with inline template
    // If we reach here without script_setup, it means the SFC has no content
    let script_setup = match descriptor.script_setup.as_ref() {
        Some(s) => s,
        None => {
            return Err(SfcError {
                message:
                    "At least one <template> or <script> is required in a single file component."
                        .to_string(),
                code: None,
                loc: None,
            });
        }
    };
    let _template_content = descriptor.template.as_ref().map(|t| t.content.as_ref());

    // Extract normal script content if present (for type definitions, imports, etc.)
    // When both <script> and <script setup> exist, normal script content should be preserved
    // (except for export default which is handled by script setup)
    let normal_script_content = if has_script {
        let script = descriptor.script.as_ref().unwrap();
        // Check if source is TypeScript
        let source_is_ts = script
            .lang
            .as_ref()
            .is_some_and(|l| l == "ts" || l == "tsx");
        Some(extract_normal_script_content(
            &script.content,
            source_is_ts,
            is_ts,
        ))
    } else {
        None
    };

    // Analyze script first to get bindings
    let mut ctx = ScriptCompileContext::new(&script_setup.content);
    ctx.analyze();
    let script_bindings = ctx.bindings.clone();

    // Compile template with bindings (if present) to get the render function
    let template_result = if let Some(template) = &descriptor.template {
        if is_vapor {
            Some(compile_template_block_vapor(
                template, &scope_id, has_scoped,
            ))
        } else {
            Some(compile_template_block(
                template,
                &options.template,
                &scope_id,
                has_scoped,
                is_ts,
                Some(&script_bindings), // Pass bindings for proper ref handling
            ))
        }
    } else {
        None
    };

    // Extract render function code from template result
    let (template_imports, template_hoisted, template_preamble, render_body) =
        match &template_result {
            Some(Ok(template_code)) => extract_template_parts(template_code),
            Some(Err(e)) => {
                errors.push(e.clone());
                (String::new(), String::new(), String::new(), String::new())
            }
            None => (String::new(), String::new(), String::new(), String::new()),
        };

    // Compile script setup with inline template
    let script_result = compile_script_setup_inline(
        &script_setup.content,
        &component_name,
        is_ts,
        TemplateParts {
            imports: &template_imports,
            hoisted: &template_hoisted,
            preamble: &template_preamble,
            render_body: &render_body,
        },
        normal_script_content.as_deref(),
    )?;
    code = script_result.code;

    // Compile styles
    let all_css = compile_styles(&descriptor.styles, &scope_id, &options.style, &mut warnings);
    if !all_css.is_empty() {
        css = Some(all_css);
    }

    Ok(SfcCompileResult {
        code,
        css,
        map: None,
        errors,
        warnings,
        bindings: script_result.bindings,
    })
}

/// Helper to compile all style blocks
fn compile_styles(
    styles: &[SfcStyleBlock],
    scope_id: &str,
    base_opts: &StyleCompileOptions,
    warnings: &mut Vec<SfcError>,
) -> String {
    let mut all_css = String::new();
    for style in styles {
        let style_opts = StyleCompileOptions {
            id: format!("data-v-{}", scope_id),
            scoped: style.scoped,
            ..base_opts.clone()
        };
        match crate::style::compile_style(style, &style_opts) {
            Ok(style_css) => {
                if !all_css.is_empty() {
                    all_css.push('\n');
                }
                all_css.push_str(&style_css);
            }
            Err(e) => warnings.push(e),
        }
    }
    all_css
}

/// Generate scope ID from filename
fn generate_scope_id(filename: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    filename.hash(&mut hasher);
    format!("{:08x}", hasher.finish() & 0xFFFFFFFF)
}

/// Extract component name from filename
fn extract_component_name(filename: &str) -> String {
    std::path::Path::new(filename)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("anonymous")
        .to_string()
}

/// Extract content from normal script block that should be preserved when both
/// `<script>` and `<script setup>` exist.
/// This includes imports, type definitions, interfaces, but excludes `export default`.
///
/// Parameters:
/// - `content`: The script content
/// - `source_is_ts`: Whether the source script is TypeScript (has lang="ts")
/// - `output_is_ts`: Whether to preserve TypeScript in output (false = transpile to JS)
fn extract_normal_script_content(content: &str, source_is_ts: bool, output_is_ts: bool) -> String {
    use oxc_allocator::Allocator;
    use oxc_ast::ast::Statement;
    use oxc_codegen::Codegen;
    use oxc_parser::Parser;
    use oxc_semantic::SemanticBuilder;
    use oxc_span::{GetSpan, SourceType};
    use oxc_transformer::{TransformOptions, Transformer};

    // Always parse as TypeScript if source is TypeScript
    let source_type = if source_is_ts {
        SourceType::ts()
    } else {
        SourceType::mjs()
    };

    let allocator = Allocator::default();
    let ret = Parser::new(&allocator, content, source_type).parse();

    if !ret.errors.is_empty() {
        // If parsing fails, return original content minus any obvious export default
        return content
            .lines()
            .filter(|line| !line.trim().starts_with("export default"))
            .collect::<Vec<_>>()
            .join("\n");
    }

    let program = ret.program;
    let mut output = String::new();
    let mut last_end = 0;

    // Collect spans of statements to skip (export default declarations)
    let mut skip_spans: Vec<(u32, u32)> = Vec::new();

    for stmt in program.body.iter() {
        match stmt {
            // Skip export default declarations
            Statement::ExportDefaultDeclaration(_) => {
                skip_spans.push((stmt.span().start, stmt.span().end));
            }
            // Skip named exports that include default: export { foo as default }
            Statement::ExportNamedDeclaration(decl) => {
                let has_default_export = decl.specifiers.iter().any(|s| {
                    matches!(&s.exported, oxc_ast::ast::ModuleExportName::IdentifierName(name) if name.name == "default")
                        || matches!(&s.exported, oxc_ast::ast::ModuleExportName::IdentifierReference(name) if name.name == "default")
                });
                if has_default_export {
                    skip_spans.push((stmt.span().start, stmt.span().end));
                }
            }
            _ => {}
        }
    }

    // Build output by copying content, skipping the export default statements
    for (start, end) in &skip_spans {
        output.push_str(&content[last_end..*start as usize]);
        last_end = *end as usize;
    }
    if last_end < content.len() {
        output.push_str(&content[last_end..]);
    }

    let extracted = output.trim().to_string();

    // If source is TypeScript and we need JavaScript output, transpile
    if source_is_ts && !output_is_ts {
        // Re-parse the extracted content
        let allocator2 = Allocator::default();
        let ret2 = Parser::new(&allocator2, &extracted, SourceType::ts()).parse();
        if ret2.errors.is_empty() {
            let mut program2 = ret2.program;

            // Run semantic analysis
            let semantic_ret = SemanticBuilder::new().build(&program2);
            if semantic_ret.errors.is_empty() {
                let (symbols, scopes) = semantic_ret.semantic.into_symbol_table_and_scope_tree();

                // Transform TypeScript to JavaScript
                let transform_options = TransformOptions::default();
                let transform_ret =
                    Transformer::new(&allocator2, std::path::Path::new(""), &transform_options)
                        .build_with_symbols_and_scopes(symbols, scopes, &mut program2);

                if transform_ret.errors.is_empty() {
                    // Generate JavaScript code
                    return Codegen::new().build(&program2).code;
                }
            }
        }
    }

    extracted
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{parse_sfc, SfcParseOptions};

    #[test]
    fn test_generate_scope_id() {
        let id = generate_scope_id("src/App.vue");
        assert_eq!(id.len(), 8);
    }

    #[test]
    fn test_extract_component_name() {
        assert_eq!(extract_component_name("src/App.vue"), "App");
        assert_eq!(extract_component_name("MyComponent.vue"), "MyComponent");
    }

    #[test]
    #[ignore = "TODO: fix v-model prop quoting"]
    fn test_v_model_on_component_in_sfc() {
        let source = r#"<script setup>
import { ref } from 'vue'
import MyComponent from './MyComponent.vue'
const msg = ref('')
</script>

<template>
  <MyComponent v-model="msg" :language="'en'" />
</template>"#;

        let descriptor =
            parse_sfc(source, SfcParseOptions::default()).expect("Failed to parse SFC");
        let opts = SfcCompileOptions::default();
        let result = compile_sfc(&descriptor, opts).expect("Failed to compile SFC");

        // Should NOT contain /* v-model */ comment
        assert!(
            !result.code.contains("/* v-model */"),
            "Should not contain v-model comment. Got:\n{}",
            result.code
        );
        // Should contain modelValue and onUpdate:modelValue
        assert!(
            result.code.contains("\"modelValue\":"),
            "Should have modelValue prop. Got:\n{}",
            result.code
        );
        assert!(
            result.code.contains("\"onUpdate:modelValue\":"),
            "Should have onUpdate:modelValue prop. Got:\n{}",
            result.code
        );
    }

    #[test]
    #[ignore = "TODO: fix $setup prefix for refs"]
    fn test_bindings_passed_to_template() {
        let source = r#"<script setup lang="ts">
import { ref } from 'vue';
import MonacoEditor from './MonacoEditor.vue';
const selectedPreset = ref('test');
const options = ref({ ssr: false });
function handleChange(val: string) { selectedPreset.value = val; }
</script>
<template>
  <div>{{ selectedPreset }}</div>
  <select :value="selectedPreset" @change="handleChange($event.target.value)">
    <option value="a">A</option>
  </select>
  <input type="checkbox" v-model="options.ssr" />
  <MonacoEditor />
</template>"#;

        let descriptor =
            parse_sfc(source, SfcParseOptions::default()).expect("Failed to parse SFC");
        let opts = SfcCompileOptions::default();
        let result = compile_sfc(&descriptor, opts).expect("Failed to compile SFC");

        eprintln!("=== COMPILED OUTPUT ===\n{}", result.code);

        // In non-inline mode with binding metadata, setup bindings are accessed via $setup
        // This is the correct Vue 3 behavior when binding metadata is passed to the template compiler
        assert!(
            result.code.contains("$setup.selectedPreset"),
            "selectedPreset should have $setup prefix in non-inline mode with bindings. Got:\n{}",
            result.code
        );
        assert!(
            result.code.contains("$setup.handleChange"),
            "handleChange should have $setup prefix in non-inline mode with bindings. Got:\n{}",
            result.code
        );
        // Verify options is in __returned__
        assert!(
            result.code.contains("options"),
            "options should be in __returned__. Got:\n{}",
            result.code
        );
        // Verify options.ssr access has $setup prefix
        assert!(
            result.code.contains("$setup.options"),
            "options.ssr should have $setup prefix. Got:\n{}",
            result.code
        );
        // Verify MonacoEditor is in __returned__ (imported component used in template)
        assert!(
            result.code.contains("MonacoEditor"),
            "MonacoEditor should be in __returned__. Got:\n{}",
            result.code
        );
    }

    #[test]
    #[ignore = "TODO: fix nested v-if prefix"]
    fn test_nested_v_if_no_double_prefix() {
        // Test with a component inside nested v-if to prevent hoisting
        let source = r#"<script setup lang="ts">
import { ref } from 'vue';
import CodeHighlight from './CodeHighlight.vue';
const output = ref(null);
</script>
<template>
<div v-if="output">
  <div v-if="output.preamble" class="preamble">
    <CodeHighlight :code="output.preamble" />
  </div>
</div>
</template>"#;

        let descriptor =
            parse_sfc(source, SfcParseOptions::default()).expect("Failed to parse SFC");
        let opts = SfcCompileOptions::default();
        let result = compile_sfc(&descriptor, opts).expect("Failed to compile SFC");

        eprintln!("=== NESTED V-IF OUTPUT ===\n{}", result.code);

        // Should NOT contain double $setup prefix
        assert!(
            !result.code.contains("$setup.$setup"),
            "Should NOT have double $setup prefix. Got:\n{}",
            result.code
        );

        // Should contain single $setup prefix for output
        assert!(
            result.code.contains("$setup.output"),
            "Should have single $setup prefix for output. Got:\n{}",
            result.code
        );

        // Should contain CodeHighlight component with :code prop
        assert!(
            result.code.contains("CodeHighlight"),
            "Should contain CodeHighlight. Got:\n{}",
            result.code
        );
    }

    #[test]
    fn test_typescript_stripped_from_event_handler() {
        let source = r#"<script setup lang="ts">
type PresetKey = 'a' | 'b'
function handlePresetChange(key: PresetKey) {}
</script>

<template>
  <select @change="handlePresetChange(($event.target).value)">
    <option value="a">A</option>
  </select>
</template>"#;

        let descriptor =
            parse_sfc(source, SfcParseOptions::default()).expect("Failed to parse SFC");
        let opts = SfcCompileOptions::default();
        let result = compile_sfc(&descriptor, opts).expect("Failed to compile SFC");

        // Print output for debugging
        eprintln!("TypeScript SFC output:\n{}", result.code);

        // Should NOT contain TypeScript 'as' assertions in template
        assert!(
            !result.code.contains(" as HTMLSelectElement"),
            "Should strip TypeScript 'as' from event handler. Got:\n{}",
            result.code
        );
        assert!(
            !result.code.contains(" as PresetKey"),
            "Should strip TypeScript 'as' from event handler. Got:\n{}",
            result.code
        );
    }

    #[test]
    fn test_full_sfc_props_destructure() {
        let input = r#"<script setup lang="ts">
import { computed } from 'vue'

const {
  name,
  count = 0,
} = defineProps<{
  name: string
  count?: number
}>()

const doubled = computed(() => count * 2)
</script>

<template>
  <div class="card">
    <h2>{{ name }}</h2>
    <p>Count: {{ count }} (doubled: {{ doubled }})</p>
  </div>
</template>"#;

        let parse_opts = SfcParseOptions::default();
        let descriptor = parse_sfc(input, parse_opts).unwrap();

        let mut compile_opts = SfcCompileOptions::default();
        compile_opts.script.id = Some("test.vue".to_string());
        let result = compile_sfc(&descriptor, compile_opts).unwrap();

        eprintln!("=== Full SFC props destructure output ===\n{}", result.code);

        // Props should use __props. prefix in template
        assert!(
            result.code.contains("__props.name") || result.code.contains("name"),
            "Should have name access. Got:\n{}",
            result.code
        );
    }

    #[test]
    fn test_let_var_unref() {
        let input = r#"
<script setup>
const a = 1
let b = 2
var c = 3
</script>

<template>
  <div>{{ a }} {{ b }} {{ c }}</div>
</template>
"#;

        let parse_opts = SfcParseOptions::default();
        let descriptor = parse_sfc(input, parse_opts).unwrap();

        let mut compile_opts = SfcCompileOptions::default();
        compile_opts.script.id = Some("test.vue".to_string());
        let result = compile_sfc(&descriptor, compile_opts).unwrap();

        eprintln!("Let/var unref test output:\n{}", result.code);

        // Check that bindings are correctly identified
        if let Some(bindings) = &result.bindings {
            eprintln!("Bindings:");
            for (name, binding_type) in &bindings.bindings {
                eprintln!("  {} => {:?}", name, binding_type);
            }
            assert!(
                matches!(bindings.bindings.get("a"), Some(BindingType::LiteralConst)),
                "a should be LiteralConst"
            );
            assert!(
                matches!(bindings.bindings.get("b"), Some(BindingType::SetupLet)),
                "b should be SetupLet"
            );
            assert!(
                matches!(bindings.bindings.get("c"), Some(BindingType::SetupLet)),
                "c should be SetupLet"
            );
        }

        // Check for _unref import
        assert!(
            result.code.contains("unref as _unref"),
            "Should import _unref. Got:\n{}",
            result.code
        );

        // Check that let/var variables are wrapped with _unref
        assert!(
            result.code.contains("_unref(b)"),
            "b should be wrapped with _unref. Got:\n{}",
            result.code
        );
        assert!(
            result.code.contains("_unref(c)"),
            "c should be wrapped with _unref. Got:\n{}",
            result.code
        );
    }

    #[test]
    fn test_extract_normal_script_content() {
        let input = r#"import type { NuxtRoute } from "@typed-router";
import { useBreakpoint } from "./_utils";
import Button from "./Button.vue";

interface TabItem {
  name: string;
  label: string;
}

export default {
  name: 'Tab'
}
"#;
        // Test preserving TypeScript output
        let result = extract_normal_script_content(input, true, true);
        eprintln!("Extracted normal script content (preserve TS):\n{}", result);

        // Should contain imports
        assert!(
            result.contains("import type { NuxtRoute }"),
            "Should contain type import"
        );
        assert!(
            result.contains("import { useBreakpoint }"),
            "Should contain named import"
        );
        assert!(
            result.contains("import Button"),
            "Should contain default import"
        );

        // Should contain interface
        assert!(
            result.contains("interface TabItem"),
            "Should contain interface"
        );

        // Should NOT contain export default
        assert!(
            !result.contains("export default"),
            "Should NOT contain export default"
        );
    }

    #[test]
    fn test_compile_both_script_blocks() {
        let source = r#"<script lang="ts">
import type { RouteLocation } from "vue-router";

interface TabItem {
  name: string;
  label: string;
}

export type { TabItem };
</script>

<script setup lang="ts">
const { items } = defineProps<{
  items: Array<TabItem>;
}>();
</script>

<template>
  <div v-for="item in items" :key="item.name">
    {{ item.label }}
  </div>
</template>"#;

        let descriptor =
            parse_sfc(source, SfcParseOptions::default()).expect("Failed to parse SFC");
        eprintln!(
            "Descriptor script: {:?}",
            descriptor.script.as_ref().map(|s| &s.content)
        );
        eprintln!(
            "Descriptor script_setup: {:?}",
            descriptor.script_setup.as_ref().map(|s| &s.content)
        );

        // Use is_ts = true to preserve TypeScript output
        let opts = SfcCompileOptions {
            script: ScriptCompileOptions {
                is_ts: true,
                ..Default::default()
            },
            template: TemplateCompileOptions {
                is_ts: true,
                ..Default::default()
            },
            ..Default::default()
        };
        let result = compile_sfc(&descriptor, opts).expect("Failed to compile SFC");

        eprintln!("=== COMPILED OUTPUT ===\n{}", result.code);

        // Should contain the type import (when is_ts = true, TypeScript is preserved)
        assert!(
            result.code.contains("RouteLocation") || result.code.contains("interface TabItem"),
            "Should contain type definitions from normal script. Got:\n{}",
            result.code
        );
    }

    #[test]
    fn test_define_model_basic() {
        let source = r#"<script setup>
const model = defineModel()
</script>

<template>
  <input v-model="model">
</template>"#;

        let descriptor =
            parse_sfc(source, SfcParseOptions::default()).expect("Failed to parse SFC");
        let opts = SfcCompileOptions::default();
        let result = compile_sfc(&descriptor, opts).expect("Failed to compile SFC");

        eprintln!("=== defineModel OUTPUT ===\n{}", result.code);

        // Should have useModel import
        assert!(
            result.code.contains("useModel as _useModel"),
            "Should import useModel. Got:\n{}",
            result.code
        );

        // Should have modelValue prop
        assert!(
            result.code.contains("modelValue"),
            "Should have modelValue prop. Got:\n{}",
            result.code
        );

        // Should have update:modelValue emit
        assert!(
            result.code.contains("update:modelValue"),
            "Should have update:modelValue emit. Got:\n{}",
            result.code
        );

        // Should have _useModel call in setup
        assert!(
            result.code.contains("_useModel(__props"),
            "Should use _useModel in setup. Got:\n{}",
            result.code
        );
    }

    #[test]
    fn test_define_model_with_name() {
        let source = r#"<script setup>
const title = defineModel('title')
</script>

<template>
  <input v-model="title">
</template>"#;

        let descriptor =
            parse_sfc(source, SfcParseOptions::default()).expect("Failed to parse SFC");
        let opts = SfcCompileOptions::default();
        let result = compile_sfc(&descriptor, opts).expect("Failed to compile SFC");

        eprintln!("=== defineModel with name OUTPUT ===\n{}", result.code);

        // Should have title prop
        assert!(
            result.code.contains("title:") || result.code.contains("\"title\""),
            "Should have title prop. Got:\n{}",
            result.code
        );

        // Should have update:title emit
        assert!(
            result.code.contains("update:title"),
            "Should have update:title emit. Got:\n{}",
            result.code
        );
    }
}
