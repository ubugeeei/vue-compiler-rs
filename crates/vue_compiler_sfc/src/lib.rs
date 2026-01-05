//! Vue Single File Component (.vue) compiler.
//!
//! This module provides parsing and compilation of Vue SFCs, following the
//! Vue.js core structure:
//!
//! - `parse` - SFC parsing into descriptor blocks
//! - `compile_script` - Script/script setup compilation
//! - `compile_template` - Template block compilation (DOM and Vapor)
//! - `compile` - Main SFC compilation orchestration
//! - `style` - Style block compilation with scoped CSS
//! - `css` - Low-level CSS compilation with LightningCSS
//!
//! # Example
//!
//! ```ignore
//! use vue_compiler_sfc::{parse_sfc, compile_sfc, SfcParseOptions, SfcCompileOptions};
//!
//! let source = r#"
//! <script setup>
//! import { ref } from 'vue'
//! const count = ref(0)
//! </script>
//! <template>
//!   <button @click="count++">{{ count }}</button>
//! </template>
//! "#;
//!
//! let descriptor = parse_sfc(source, SfcParseOptions::default()).unwrap();
//! let result = compile_sfc(&descriptor, SfcCompileOptions::default()).unwrap();
//! println!("{}", result.code);
//! ```

#![allow(clippy::collapsible_match)]
#![allow(clippy::type_complexity)]
#![allow(clippy::redundant_field_names)]
#![allow(clippy::unnecessary_lazy_evaluations)]
#![allow(clippy::field_reassign_with_default)]
#![allow(clippy::only_used_in_recursion)]

// Core modules - following Vue.js compiler-sfc structure
pub mod compile;
pub mod compile_script;
pub mod compile_template;
pub mod css;
pub mod parse;
pub mod script;
pub mod style;
pub mod types;

// Re-exports for public API
pub use compile::*;
pub use css::{compile_css, compile_style_block, CssCompileOptions, CssCompileResult, CssTargets};
pub use parse::*;
pub use types::*;

// Re-export key types from dependencies
pub use vue_compiler_core::CompilerError;
pub use vue_compiler_dom::compile_template;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_sfc() {
        let source = r#"
<template>
  <div>Hello World</div>
</template>

<script>
export default {
  name: 'HelloWorld'
}
</script>

<style>
.hello { color: red; }
</style>
"#;
        let descriptor = parse_sfc(source, Default::default()).unwrap();

        assert!(descriptor.template.is_some());
        assert!(descriptor.script.is_some());
        assert_eq!(descriptor.styles.len(), 1);
    }

    #[test]
    fn test_parse_script_setup() {
        let source = r#"
<template>
  <div>{{ msg }}</div>
</template>

<script setup>
import { ref } from 'vue'
const msg = ref('Hello')
</script>
"#;
        let descriptor = parse_sfc(source, Default::default()).unwrap();

        assert!(descriptor.template.is_some());
        assert!(descriptor.script_setup.is_some());
    }

    #[test]
    fn test_parse_scoped_style() {
        let source = r#"
<template>
  <div class="container">Scoped</div>
</template>

<style scoped>
.container { background: blue; }
</style>
"#;
        let descriptor = parse_sfc(source, Default::default()).unwrap();

        assert_eq!(descriptor.styles.len(), 1);
        assert!(descriptor.styles[0].scoped);
    }

    #[test]
    fn test_compile_sfc_with_define_emits() {
        let source = r#"
<template>
  <button @click="onClick">{{ count }}</button>
</template>

<script setup>
import { ref } from 'vue'
const emit = defineEmits(['update'])
const count = ref(0)
function onClick() {
    emit('update', count.value)
}
</script>
"#;
        let descriptor = parse_sfc(source, Default::default()).unwrap();
        let result = compile_sfc(&descriptor, SfcCompileOptions::default()).unwrap();

        println!("Full SFC output:\n{}", result.code);

        // defineEmits should NOT be in the output
        assert!(
            !result.code.contains("defineEmits"),
            "defineEmits should be removed"
        );
        // emits should be defined at component level
        assert!(
            result.code.contains("emits:"),
            "emits should be at component level"
        );
        // emit should be bound to __emit
        assert!(
            result.code.contains("const emit = __emit"),
            "emit should be bound to __emit"
        );
    }

    #[test]
    fn test_compile_sfc_with_component_import() {
        let source = r#"
<template>
  <div>
    <MyComponent />
    {{ msg }}
  </div>
</template>

<script setup>
import MyComponent from './MyComponent.vue'

const msg = "hello"
</script>
"#;
        let descriptor = parse_sfc(source, Default::default()).unwrap();
        let result = compile_sfc(&descriptor, SfcCompileOptions::default()).unwrap();

        println!("Component import SFC output:\n{}", result.code);

        // Import should be preserved
        assert!(
            result.code.contains("import MyComponent from"),
            "Component import should be preserved. Got:\n{}",
            result.code
        );
        // MyComponent should be referenced directly (not $setup.MyComponent in inline mode)
        assert!(
            !result.code.contains("$setup.MyComponent"),
            "Should NOT have $setup.MyComponent prefix in inline mode. Got:\n{}",
            result.code
        );
        // Component should be referenced directly
        assert!(
            result.code.contains("MyComponent") && !result.code.contains("_component_MyComponent"),
            "Component should be referenced directly (not _component_). Got:\n{}",
            result.code
        );
    }

    #[test]
    fn test_compile_sfc_with_component_import_typescript() {
        let source = r#"
<template>
  <div>
    <MyComponent />
    {{ msg }}
  </div>
</template>

<script setup lang="ts">
import MyComponent from './MyComponent.vue'
import { type SomeType } from './types'

const msg: string = "hello"
</script>
"#;
        let descriptor = parse_sfc(source, Default::default()).unwrap();
        let result = compile_sfc(&descriptor, SfcCompileOptions::default()).unwrap();

        println!("TypeScript SFC output:\n{}", result.code);

        // Import should be preserved (type imports should be stripped)
        assert!(
            result.code.contains("import MyComponent from"),
            "Component import should be preserved. Got:\n{}",
            result.code
        );
        // Type imports should be stripped
        assert!(
            !result.code.contains("type SomeType"),
            "Type imports should be stripped. Got:\n{}",
            result.code
        );
        // TypeScript type annotations should be stripped
        assert!(
            !result.code.contains(": string"),
            "TypeScript annotations should be stripped. Got:\n{}",
            result.code
        );
        // MyComponent should be referenced directly
        assert!(
            result.code.contains("_createVNode(MyComponent)"),
            "Component should be referenced directly. Got:\n{}",
            result.code
        );
    }

    #[test]
    fn test_compile_sfc_with_define_props_typescript() {
        let source = r#"
<template>
  <div>{{ modelValue }}</div>
</template>

<script setup lang="ts">
const props = defineProps<{
  modelValue: string;
  language?: string;
}>();

const count = 42;
</script>
"#;
        let descriptor = parse_sfc(source, Default::default()).unwrap();
        let result = compile_sfc(&descriptor, SfcCompileOptions::default()).unwrap();

        println!("defineProps TypeScript output:\n{}", result.code);

        // Type annotations should be stripped
        assert!(
            !result.code.contains(": string"),
            "TypeScript annotations should be stripped. Got:\n{}",
            result.code
        );
        // defineProps should be removed (as a macro call, transformed to __props binding)
        assert!(
            !result.code.contains("defineProps<"),
            "defineProps should be transformed. Got:\n{}",
            result.code
        );
        // props should be available via __props
        assert!(
            result.code.contains("__props"),
            "__props should be used. Got:\n{}",
            result.code
        );
    }
}
