//! Script block analysis and compilation.
//!
//! This module handles the compilation of `<script>` and `<script setup>` blocks,
//! including Compiler Macros like `defineProps`, `defineEmits`, etc.
//!
//! Module structure follows Vue.js official implementation.
//! Uses OXC for JavaScript/TypeScript parsing instead of Babel.

mod analyze_script_bindings;
mod context;
mod define_emits;
mod define_expose;
mod define_model;
mod define_model_metadata;
mod define_options;
mod define_props;
mod define_props_destructure;
mod define_slots;
mod import_usage_check;
mod static_expression;
pub(crate) mod type_resolution;
mod utils;

// Re-export main types
pub use analyze_script_bindings::{analyze_script_bindings, get_object_or_array_expression_keys};
pub use context::{ScriptCompileContext, TypeResolutionBatchGuard, begin_type_resolution_batch};
pub use define_emits::{
    DefineEmitsResult, extract_runtime_emits, gen_runtime_emits, process_define_emits,
};
pub(crate) use define_model_metadata::{
    add_model_to_croquis, define_model_metadata, define_model_name, define_model_prop_option_spans,
};
pub use define_props_destructure::{
    PropsDestructureBinding, PropsDestructuredBindings, gen_props_access_exp,
    process_props_destructure, transform_destructured_props,
};
pub use import_usage_check::{
    TemplateUsedIdentifiers, is_used_in_template, resolve_template_read_identifiers,
    resolve_template_used_identifiers, resolve_template_v_model_identifiers,
};
pub(crate) use static_expression::{is_static_enum, register_enum};
pub(crate) use type_resolution::{
    build_interface_type_source, resolve_type_args, resolve_type_to_object_body,
};
pub(crate) use utils::model_modifiers_binding_name;
pub use utils::{
    MacroCall, ScriptSetupMacros, get_escaped_prop_name, is_compiler_macro_line,
    is_valid_identifier,
};

// Re-export constants
pub use define_emits::DEFINE_EMITS;
pub use define_expose::DEFINE_EXPOSE;
pub use define_model::DEFINE_MODEL;
pub use define_options::DEFINE_OPTIONS;
pub use define_props::{DEFINE_PROPS, WITH_DEFAULTS};
pub use define_slots::DEFINE_SLOTS;

use crate::types::BindingMetadata;
use vize_croquis::croquis::Croquis as CroquisSummary;
use vize_croquis::script_parser::ScriptParseResult;

/// Analyze script setup and extract bindings
pub fn analyze_script_setup(content: &str) -> BindingMetadata {
    let mut ctx = ScriptCompileContext::new(content);
    ctx.analyze();
    ctx.bindings
}

/// Extract macro calls from script setup
pub fn extract_macros(content: &str) -> ScriptSetupMacros {
    let mut ctx = ScriptCompileContext::new(content);
    ctx.extract_all_macros();
    ctx.macros
}

// =============================================================================
// vize_croquis Integration
// =============================================================================

/// Fast script setup analysis using vize_croquis OXC parser.
///
/// This provides a high-performance analysis path that returns
/// a `ScriptParseResult` directly from vize_croquis.
///
/// Use this for:
/// - Quick analysis in linter
/// - Playground/editor integrations
/// - When full macro transformation is not needed
///
/// For full compilation with macro transformations, use `ScriptCompileContext`.
#[inline]
pub fn analyze_script_setup_fast(content: &str) -> ScriptParseResult {
    vize_croquis::script_parser::parse_script_setup(content)
}

/// Analyze script setup and return a croquis Croquis.
///
/// This uses vize_croquis for the core analysis and converts
/// the result to the shared Croquis format.
pub fn analyze_script_setup_to_summary(content: &str) -> CroquisSummary {
    vize_croquis::script_parser::parse_script_setup(content).into_croquis()
}

/// Parse `<script setup>` content once for reuse across compile stages.
///
/// The SFC compiler needs the same oxc AST in three places: croquis binding
/// analysis, `ScriptCompileContext` macro analysis, and statement sectioning
/// in the inline script compiler. Parsing here once and lending the program
/// out replaces three identical parses of the same content.
///
/// Returns `None` when the parser panicked; callers fall back to the legacy
/// per-stage parse paths, which reproduce the historical panicked behavior.
pub fn parse_script_setup_program<'a>(
    allocator: &'a oxc_allocator::Allocator,
    content: &'a str,
) -> Option<oxc_ast::ast::Program<'a>> {
    use oxc_parser::Parser;
    use oxc_span::SourceType;
    use vize_carton::profile;

    // Same source type as the per-stage parsers this replaces
    // (croquis `parse_script_setup`, `ScriptCompileContext::parse_with_oxc`,
    // and `extract_script_sections` all parse as "script.ts").
    let source_type = SourceType::from_path("script.ts").unwrap_or_default();
    let ret = profile!(
        "atelier.sfc.script_setup.oxc_parse",
        Parser::new(allocator, content, source_type).parse()
    );
    (!ret.panicked).then_some(ret.program)
}

/// Analyze an already-parsed script setup program into a croquis Croquis.
///
/// Parse-free variant of [`analyze_script_setup_to_summary`] used by the
/// parse-once SFC pipeline. `content` must be the exact text `program` was
/// parsed from.
pub fn analyze_script_setup_program_to_summary(
    program: &oxc_ast::ast::Program<'_>,
    content: &str,
) -> CroquisSummary {
    vize_croquis::script_parser::analyze_script_setup_program(program, content, None).into_croquis()
}

/// Convert a full ScriptCompileContext analysis to Croquis.
///
/// This uses the full atelier_sfc analysis (which includes more detailed
/// type resolution) and converts to the shared format.
#[inline]
pub fn analyze_script_setup_full(content: &str) -> CroquisSummary {
    let mut ctx = ScriptCompileContext::new(content);
    ctx.analyze();
    ctx.to_analysis_summary()
}
