//! Code generation context and result types.

use vue_allocator::String;

use crate::ast::RuntimeHelper;
use crate::options::CodegenOptions;

use super::helpers::default_helper_alias;

/// Code generation context
pub struct CodegenContext {
    /// Generated code buffer
    pub(super) code: String,
    /// Current indentation level
    pub(super) indent_level: u32,
    /// Whether we're in SSR mode
    #[allow(dead_code)]
    pub(super) ssr: bool,
    /// Helper function alias map
    pub(super) helper_alias: fn(RuntimeHelper) -> &'static str,
    /// Runtime global name
    pub(super) runtime_global_name: String,
    /// Runtime module name
    pub(super) runtime_module_name: String,
    /// Options
    pub(super) options: CodegenOptions,
    /// Pure annotation for tree-shaking
    pub(super) pure: bool,
    /// Helpers used during codegen
    pub(super) used_helpers: std::collections::HashSet<RuntimeHelper>,
    /// Cache index for v-once
    pub(super) cache_index: usize,
    /// Slot parameters (identifiers that should not be prefixed with _ctx.)
    pub(super) slot_params: std::collections::HashSet<std::string::String>,
}

/// Code generation result
pub struct CodegenResult {
    /// Generated code
    pub code: String,
    /// Preamble (imports)
    pub preamble: String,
    /// Source map (JSON)
    pub map: Option<String>,
}

impl CodegenContext {
    /// Create a new codegen context
    pub fn new(options: CodegenOptions) -> Self {
        Self {
            code: String::with_capacity(4096),
            indent_level: 0,
            ssr: options.ssr,
            helper_alias: default_helper_alias,
            runtime_global_name: options.runtime_global_name.clone(),
            runtime_module_name: options.runtime_module_name.clone(),
            options,
            pure: false,
            used_helpers: std::collections::HashSet::new(),
            cache_index: 0,
            slot_params: std::collections::HashSet::new(),
        }
    }

    /// Add slot parameters (identifiers that should not be prefixed)
    pub fn add_slot_params(&mut self, params: &[std::string::String]) {
        for param in params {
            self.slot_params.insert(param.clone());
        }
    }

    /// Remove slot parameters (when exiting slot scope)
    pub fn remove_slot_params(&mut self, params: &[std::string::String]) {
        for param in params {
            self.slot_params.remove(param);
        }
    }

    /// Check if an identifier is a slot parameter
    pub fn is_slot_param(&self, name: &str) -> bool {
        self.slot_params.contains(name)
    }

    /// Get next cache index for v-once
    pub fn next_cache_index(&mut self) -> usize {
        let index = self.cache_index;
        self.cache_index += 1;
        index
    }

    /// Push code to buffer
    pub fn push(&mut self, code: &str) {
        self.code.push_str(code);
    }

    /// Push code with newline
    pub fn push_line(&mut self, code: &str) {
        self.push(code);
        self.newline();
    }

    /// Add newline with proper indentation
    pub fn newline(&mut self) {
        self.code.push('\n');
        for _ in 0..self.indent_level {
            self.code.push_str("  ");
        }
    }

    /// Increase indentation
    pub fn indent(&mut self) {
        self.indent_level += 1;
    }

    /// Decrease indentation
    pub fn deindent(&mut self) {
        if self.indent_level > 0 {
            self.indent_level -= 1;
        }
    }

    /// Add pure annotation /*#__PURE__*/
    pub fn push_pure(&mut self) {
        if self.pure {
            self.push("/*#__PURE__*/ ");
        }
    }

    /// Get helper name
    pub fn helper(&self, helper: RuntimeHelper) -> &'static str {
        (self.helper_alias)(helper)
    }

    /// Track a helper for preamble generation
    pub fn use_helper(&mut self, helper: RuntimeHelper) {
        self.used_helpers.insert(helper);
    }

    /// Check if a component is in binding metadata (from script setup)
    pub fn is_component_in_bindings(&self, component: &str) -> bool {
        if let Some(ref metadata) = self.options.binding_metadata {
            // Check both the original name and PascalCase version
            metadata.bindings.contains_key(component)
        } else {
            false
        }
    }
}
