//! Legacy virtual TypeScript generator used by older batch tests.
//!
//! The production path now uses `crate::virtual_ts`, but a lightweight version
//! is kept here for the focused unit tests that exercise import rewriting and
//! baseline batch behavior in isolation.

use super::SfcBlockType;
use super::import_rewriter::ImportRewriter;
use super::source_map::{SfcBlockRange, SfcSourceMap};
use crate::virtual_ts::VizeMapping;
use vize_atelier_sfc::{
    SfcDescriptor,
    croquis::{SfcCroquisOptions, analyze_sfc_descriptor},
};
use vize_carton::String;
use vize_carton::append;
use vize_carton::cstr;
use vize_croquis::Croquis;

/// Result of virtual TypeScript generation.
#[derive(Debug)]
pub struct VirtualTsResult {
    /// Generated TypeScript code.
    pub code: String,
    /// Source map for position mapping.
    pub source_map: SfcSourceMap,
}

/// Vue setup-scope helpers - defined with parameters marked as used.
const VUE_SETUP_HELPERS: &str = r#"// Compiler macros (transformed at compile time by Vue)
type __RuntimePropValue<T> = T extends abstract new (...args: any[]) => infer V ? V : T extends (...args: any[]) => infer V ? V : never;
type __RuntimePropCtorInner<T> = T extends null | undefined ? never : T extends readonly (infer U)[] ? __RuntimePropCtorInner<U> : T extends { type: infer U } ? __RuntimePropCtorInner<U> : T extends StringConstructor ? string : T extends NumberConstructor ? number : T extends BooleanConstructor ? boolean : T extends ArrayConstructor ? unknown[] : T extends ObjectConstructor ? Record<string, any> : T extends DateConstructor ? Date : T extends FunctionConstructor ? (...args: any[]) => any : __RuntimePropValue<T>;
type __RuntimePropCtor<T> = [__RuntimePropCtorInner<T>] extends [never] ? unknown : __RuntimePropCtorInner<T>;
type __RuntimePropHasBoolean<T> = T extends BooleanConstructor ? true : T extends readonly (infer U)[] ? __RuntimePropHasBoolean<U> : T extends { type: infer U } ? __RuntimePropHasBoolean<U> : false;
type __RuntimePropResolved<T> = T extends { required: true } ? true : T extends { default: any } ? true : __RuntimePropHasBoolean<T>;
type __RuntimePropShape<T extends Record<string, any>> = { [K in keyof T]: __RuntimePropResolved<T[K]> extends true ? __RuntimePropCtor<T[K]> : __RuntimePropCtor<T[K]> | undefined; };
type __BatchEmitShape<T> = T extends (...args: any[]) => any ? T : T extends Record<string, any> ? { [K in keyof T]: T[K] extends (...args: infer A) => any ? A : T[K] extends any[] ? T[K] : any[]; } : Record<string, any[]>;
type __BatchEmitArgs<T, K extends keyof T> = T[K] extends any[] ? T[K] : any[];
type __BatchEmitFn<T, __S = __BatchEmitShape<T>, __K extends keyof __S & string = keyof __S & string, __U = { [K in __K]: (event: K, ...args: __BatchEmitArgs<__S, K>) => void }[__K]> = __S extends (...args: any[]) => any ? __S : [__K] extends [never] ? (event: never, ...args: any[]) => void : (__U extends unknown ? (fn: __U) => void : never) extends (fn: infer __I) => void ? __I : never;
type __DefaultFactory<T> = (props: any) => T;
type __WithDefaultValue<T> = T | __DefaultFactory<T>;
type __LooseRequired<T> = { [P in keyof (T & Required<T>)]: T[P] };
type __DefineProps<T> = __LooseRequired<T>;
type __WithDefaultsArgs<T> = { [K in keyof T]?: __WithDefaultValue<T[K]> };
type __WithDefaultsResult<T, D> = T extends unknown ? Omit<__LooseRequired<T>, keyof D> & { [K in keyof D & keyof T]-?: [D[K]] extends [undefined] ? __LooseRequired<T>[K] : Exclude<__LooseRequired<T>[K], undefined> } : never;
function defineProps<T>(): __DefineProps<T>;
function defineProps<const T extends readonly string[]>(_props: T): { [K in T[number]]?: any };
function defineProps<const T extends Record<string, any>>(_props: T): __RuntimePropShape<T>;
function defineProps(_props?: any) { void _props; return undefined as any; }
function defineEmits<T>(): __BatchEmitFn<T> { return (() => {}) as any; }
function defineEmits<const T extends readonly string[]>(_events: T): (event: T[number], ...args: any[]) => void { void _events; return (() => {}) as any; }
function defineEmits<const T extends Record<string, any>>(_events: T): __BatchEmitFn<T> { void _events; return (() => {}) as any; }
function defineExpose<T>(_exposed?: T): void { void _exposed; }
function defineModel<T>(): $Vue['Ref']<T | undefined> { return undefined as unknown as $Vue['Ref']<T | undefined>; }
function defineModel<T>(_options: any): $Vue['Ref']<T> { void _options; return undefined as unknown as $Vue['Ref']<T>; }
function defineModel<T>(_name: string, _options?: any): $Vue['Ref']<T> { void _name; void _options; return undefined as unknown as $Vue['Ref']<T>; }
function defineSlots<T>(): T { return undefined as unknown as T; }
function withDefaults<T, D extends __WithDefaultsArgs<T>>(_props: T, _defaults: D): __WithDefaultsResult<T, D>; function withDefaults<T, D extends Record<string, any>>(_props: T, _defaults: D): __WithDefaultsResult<T, D>; function withDefaults(_props: any, _defaults: any) { void _props; void _defaults; return undefined as any; }
function useTemplateRef<T = any>(_key: string): $Vue['ShallowRef']<T | null> { void _key; return undefined as unknown as $Vue['ShallowRef']<T | null>; }
void defineProps; void defineEmits; void defineExpose; void defineModel; void defineSlots; void withDefaults; void useTemplateRef;
"#;

/// Vue template context available in template expressions.
const VUE_TEMPLATE_CONTEXT: &str = r#"// Vue instance context (available in template)
const $attrs: Record<string, unknown> = {} as any;
const $slots: Record<string, (...args: any[]) => any> = {} as any;
const $refs: Record<string, any> = {} as any;
const $emit: (...args: any[]) => void = (() => {}) as any;
void $attrs; void $slots; void $refs; void $emit;
"#;

/// Virtual TypeScript generator.
pub struct VirtualTsGenerator;

impl VirtualTsGenerator {
    /// Create a new generator.
    pub fn new() -> Self {
        Self
    }

    /// Generate virtual TypeScript from SFC descriptor.
    pub fn generate(&self, descriptor: &SfcDescriptor, analysis: &Croquis) -> VirtualTsResult {
        let mut code = String::default();
        let mut mappings = Vec::new();
        let mut blocks = Vec::new();

        code.push_str("// ============================================\n");
        code.push_str("// Virtual TypeScript for Vue SFC Type Checking\n");
        code.push_str("// Generated by vize\n");
        code.push_str("// ============================================\n\n");
        code.push_str("type $Vue = import('vue');\n\n");
        code.push_str(VUE_SETUP_HELPERS);
        code.push('\n');
        code.push_str(VUE_TEMPLATE_CONTEXT);
        code.push('\n');

        code.push_str("// ========== Imports ==========\n");
        if let Some(ref script_setup) = descriptor.script_setup {
            self.emit_imports(&mut code, &script_setup.content);
        } else if let Some(ref script) = descriptor.script {
            self.emit_imports(&mut code, &script.content);
        }
        code.push('\n');

        code.push_str("// ========== Script Content ==========\n");
        if let Some(ref script_setup) = descriptor.script_setup {
            let start = code.len();
            self.emit_script_content_module_scope(&mut code, &script_setup.content);
            let end = code.len();
            mappings.push(VizeMapping {
                gen_range: start..end,
                src_range: script_setup.loc.start
                    ..script_setup.loc.start + script_setup.content.len(),
                sub_spans: Vec::new(),
            });
            blocks.push(SfcBlockRange {
                start: script_setup.loc.start as u32,
                end: script_setup.loc.start as u32 + script_setup.content.len() as u32,
                block_type: SfcBlockType::ScriptSetup,
            });
        } else if let Some(ref script) = descriptor.script {
            let start = code.len();
            self.emit_script_content_module_scope(&mut code, &script.content);
            let end = code.len();
            mappings.push(VizeMapping {
                gen_range: start..end,
                src_range: script.loc.start..script.loc.start + script.content.len(),
                sub_spans: Vec::new(),
            });
            blocks.push(SfcBlockRange {
                start: script.loc.start as u32,
                end: script.loc.start as u32 + script.content.len() as u32,
                block_type: SfcBlockType::Script,
            });
        }
        code.push('\n');

        if let Some(ref template) = descriptor.template {
            code.push_str("// ========== Template Bindings ==========\n");
            code.push_str("(function __template() {\n");
            let start = code.len();
            self.emit_template_bindings(&mut code, analysis);
            let end = code.len();
            mappings.push(VizeMapping {
                gen_range: start..end,
                src_range: template.loc.start..template.loc.start + template.content.len(),
                sub_spans: Vec::new(),
            });
            blocks.push(SfcBlockRange {
                start: template.loc.start as u32,
                end: template.loc.start as u32 + template.content.len() as u32,
                block_type: SfcBlockType::Template,
            });
            code.push_str("})();\n\n");
        }

        code.push_str("// ========== Component Export ==========\n");
        code.push_str("import { DefineComponent } from 'vue';\n\n");
        self.emit_component_types(&mut code, analysis);
        code.push_str("declare const __component: DefineComponent<__Props, {}, {}, {}, {}, {}, {}, __Emits>;\n");
        code.push_str("export default __component;\n");

        VirtualTsResult {
            code,
            source_map: SfcSourceMap::new(mappings, blocks),
        }
    }

    /// Emit imports with `.vue -> .vue.ts` rewrite.
    fn emit_imports(&self, code: &mut String, script: &str) {
        let rewritten = ImportRewriter::new().rewrite(script, oxc_span::SourceType::ts(), None);
        for line in rewritten.code.lines() {
            if line.trim().starts_with("import ") {
                code.push_str("  ");
                code.push_str(line);
                code.push('\n');
            }
        }
    }

    /// Emit script content excluding imports (module scope, no indentation).
    fn emit_script_content_module_scope(&self, code: &mut String, script: &str) {
        let mut first_content = true;
        for line in script.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("import ") {
                continue;
            }
            if first_content && trimmed.is_empty() {
                continue;
            }
            first_content = false;
            code.push_str(line);
            code.push('\n');
        }
    }

    /// Emit template binding references.
    fn emit_template_bindings(&self, code: &mut String, analysis: &Croquis) {
        for (name, _) in analysis.bindings.iter() {
            append!(*code, "    void {name};\n");
        }
    }

    /// Emit component type definitions.
    fn emit_component_types(&self, code: &mut String, analysis: &Croquis) {
        code.push_str("export interface __Props {\n");
        for prop in analysis.macros.props() {
            let optional = if !prop.required { "?" } else { "" };
            let prop_type = prop.prop_type.as_deref().unwrap_or("unknown");
            append!(*code, "  {}{}: {};\n", prop.name, optional, prop_type);
        }
        code.push_str("}\n\n");

        code.push_str("export interface __Emits {\n");
        for emit in analysis.macros.emits() {
            if let Some(ref payload_type) = emit.payload_type {
                append!(
                    *code,
                    "  (e: '{}', payload: {}): void;\n",
                    emit.name,
                    payload_type
                );
            } else {
                append!(*code, "  (e: '{}'): void;\n", emit.name);
            }
        }
        code.push_str("}\n\n");
    }

    /// Generate virtual TypeScript from SFC content string.
    pub fn generate_from_content(&self, content: &str) -> Result<VirtualTsResult, String> {
        let descriptor =
            vize_atelier_sfc::parse_sfc(content, vize_atelier_sfc::SfcParseOptions::default())
                .map_err(|error| cstr!("{error:?}"))?;

        let analysis = if let Some(ref template) = descriptor.template {
            let allocator = vize_carton::Allocator::new();
            let (root, _) = vize_armature::parse(&allocator, &template.content);
            analyze_sfc_descriptor(&descriptor, Some(&root), SfcCroquisOptions::full())
        } else {
            analyze_sfc_descriptor(&descriptor, None, SfcCroquisOptions::full())
        };

        Ok(self.generate(&descriptor, &analysis))
    }
}

impl Default for VirtualTsGenerator {
    fn default() -> Self {
        Self
    }
}

#[cfg(test)]
mod tests {
    use super::VirtualTsGenerator;

    #[test]
    fn test_generate_from_content() {
        let generator = VirtualTsGenerator::new();
        let content = r#"<template>
  <div>{{ message }}</div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import Child from './Child.vue'

const message = ref('Hello')
</script>
"#;

        let result = generator.generate_from_content(content).unwrap();
        insta::assert_snapshot!(result.code.as_str());
    }
}
