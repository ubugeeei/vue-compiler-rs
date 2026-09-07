//! DOM template compilation: parse, transform, and codegen entry points.

pub mod custom_elements;

use vize_atelier_core::codegen::{CodegenResult, CodegenResultWithSections};
use vize_atelier_core::{
    CompilerError, RootNode,
    options::{CodegenOptions, CustomElementMatcher, TemplateSyntaxMode},
};
use vize_s0::{Allocator, String};

mod inner;
#[cfg(feature = "davinci-differential")]
pub(crate) mod legacy;
mod pipeline;
mod sfc;
mod source_map;
mod stage_options;

use crate::options::DomCompilerOptions;
use inner::{compile_template_inner, compile_template_inner_with_sections};
use pipeline::DomCompilePipelineOptions;

/// Compile a Vue template for DOM with default options
pub fn compile_template<'a>(
    allocator: &'a Allocator,
    source: &'a str,
) -> (RootNode<'a>, Vec<CompilerError>, CodegenResult) {
    compile_template_with_options(allocator, source, DomCompilerOptions::default())
}

/// Compile a Vue template for DOM with custom options
pub fn compile_template_with_options<'a>(
    allocator: &'a Allocator,
    source: &'a str,
    options: DomCompilerOptions,
) -> (RootNode<'a>, Vec<CompilerError>, CodegenResult) {
    compile_template_inner(
        allocator,
        source,
        options,
        TemplateSyntaxMode::Standard,
        None,
        CustomElementMatcher::default(),
        CodegenOptions::default(),
    )
}

/// Compile a Vue template for DOM with Vue parser quirk compatibility.
#[deprecated(note = "use compile_template_with_template_syntax instead")]
pub fn compile_template_with_vue_parser_quirks<'a>(
    allocator: &'a Allocator,
    source: &'a str,
    options: DomCompilerOptions,
) -> (RootNode<'a>, Vec<CompilerError>, CodegenResult) {
    compile_template_inner(
        allocator,
        source,
        options,
        TemplateSyntaxMode::Quirks,
        None,
        CustomElementMatcher::default(),
        CodegenOptions::default(),
    )
}

/// Compile a Vue template for DOM with an explicit template syntax mode.
#[doc(hidden)]
pub fn compile_template_with_template_syntax<'a>(
    allocator: &'a Allocator,
    source: &'a str,
    options: DomCompilerOptions,
    template_syntax: TemplateSyntaxMode,
) -> (RootNode<'a>, Vec<CompilerError>, CodegenResult) {
    compile_template_inner(
        allocator,
        source,
        options,
        template_syntax,
        None,
        CustomElementMatcher::default(),
        CodegenOptions::default(),
    )
}

/// Compile a Vue template with adapter-provided codegen defaults.
///
/// DOM-owned settings such as mode, source maps, and binding metadata still
/// take precedence. This hook lets binding facades provide emission-only
/// settings (for example runtime names and the source-map filename) without
/// growing [`DomCompilerOptions`] and breaking downstream struct literals.
#[doc(hidden)]
pub fn compile_template_with_template_syntax_and_codegen_options<'a>(
    allocator: &'a Allocator,
    source: &'a str,
    options: DomCompilerOptions,
    template_syntax: TemplateSyntaxMode,
    codegen_options: CodegenOptions,
) -> (RootNode<'a>, Vec<CompilerError>, CodegenResult) {
    compile_template_inner(
        allocator,
        source,
        options,
        template_syntax,
        None,
        CustomElementMatcher::default(),
        codegen_options,
    )
}

/// Compile a Vue template for DOM with an explicit scope ID for hoisted static VNodes.
#[doc(hidden)]
pub fn compile_template_with_options_and_hoisted_scope_id<'a>(
    allocator: &'a Allocator,
    source: &'a str,
    options: DomCompilerOptions,
    hoisted_scope_id: Option<String>,
) -> (RootNode<'a>, Vec<CompilerError>, CodegenResult) {
    compile_template_inner(
        allocator,
        source,
        options,
        TemplateSyntaxMode::Standard,
        hoisted_scope_id,
        CustomElementMatcher::default(),
        CodegenOptions::default(),
    )
}

/// Compile a Vue template for DOM with Vue parser quirks and an explicit hoisted scope ID.
#[doc(hidden)]
#[deprecated(note = "use compile_template_with_template_syntax_and_hoisted_scope_id instead")]
pub fn compile_template_with_vue_parser_quirks_and_hoisted_scope_id<'a>(
    allocator: &'a Allocator,
    source: &'a str,
    options: DomCompilerOptions,
    hoisted_scope_id: Option<String>,
) -> (RootNode<'a>, Vec<CompilerError>, CodegenResult) {
    compile_template_inner(
        allocator,
        source,
        options,
        TemplateSyntaxMode::Quirks,
        hoisted_scope_id,
        CustomElementMatcher::default(),
        CodegenOptions::default(),
    )
}

/// Compile a Vue template for DOM with template syntax mode and hoisted scope ID.
#[doc(hidden)]
pub fn compile_template_with_template_syntax_and_hoisted_scope_id<'a>(
    allocator: &'a Allocator,
    source: &'a str,
    options: DomCompilerOptions,
    template_syntax: TemplateSyntaxMode,
    hoisted_scope_id: Option<String>,
) -> (RootNode<'a>, Vec<CompilerError>, CodegenResult) {
    compile_template_inner(
        allocator,
        source,
        options,
        template_syntax,
        hoisted_scope_id,
        CustomElementMatcher::default(),
        CodegenOptions::default(),
    )
}

/// Compile a Vue template for DOM with template syntax mode, hoisted scope ID,
/// and emission-recorded codegen section boundaries.
#[doc(hidden)]
pub fn compile_template_with_template_syntax_and_hoisted_scope_id_with_sections<'a>(
    allocator: &'a Allocator,
    source: &'a str,
    options: DomCompilerOptions,
    template_syntax: TemplateSyntaxMode,
    hoisted_scope_id: Option<String>,
) -> (RootNode<'a>, Vec<CompilerError>, CodegenResultWithSections) {
    compile_template_inner_with_sections(
        allocator,
        source,
        options,
        template_syntax,
        hoisted_scope_id,
        DomCompilePipelineOptions::allow_s2(
            CustomElementMatcher::default(),
            CodegenOptions::default(),
        ),
    )
}

/// Compile a Vue template with section metadata and adapter-provided codegen
/// defaults. See [`compile_template_with_template_syntax_and_codegen_options`].
#[doc(hidden)]
pub fn compile_template_with_template_syntax_and_hoisted_scope_id_with_sections_and_codegen_options<
    'a,
>(
    allocator: &'a Allocator,
    source: &'a str,
    options: DomCompilerOptions,
    template_syntax: TemplateSyntaxMode,
    hoisted_scope_id: Option<String>,
    codegen_options: CodegenOptions,
) -> (RootNode<'a>, Vec<CompilerError>, CodegenResultWithSections) {
    compile_template_inner_with_sections(
        allocator,
        source,
        options,
        template_syntax,
        hoisted_scope_id,
        DomCompilePipelineOptions::allow_s2(CustomElementMatcher::default(), codegen_options),
    )
}
