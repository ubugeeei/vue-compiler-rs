//! The differential lanes' old side.
//!
//! Gated behind `davinci-differential` so no production build can reach
//! it: the legacy lane is the thing the strangler is replacing, and an
//! ungated entry point would be a way back onto the old path rather than
//! a measurement of it.

use vize_atelier_core::codegen::CodegenResult;
use vize_atelier_core::options::{CodegenOptions, CustomElementMatcher, TemplateSyntaxMode};
use vize_atelier_core::{CompilerError, RootNode};
use vize_s0::Allocator;

use super::inner::compile_template_inner_with_sections;
use super::pipeline::DomCompilePipelineOptions;
use crate::options::DomCompilerOptions;

/// Compile a Vue template for DOM through the **legacy** lane: parse,
/// transform, then `vize_atelier_core`'s codegen, with the S2 emitter
/// declined.
///
/// This is the differential lanes' *old* side and has no other caller. The
/// ordinary entry points above route through S2 wherever
/// `stage_options::s2_emit_supported` allows it (the P2-11 production
/// switch), so a comparator built on them compares S2 against itself and
/// cannot fail — measured: renaming a core helper alias in the S2 emitter
/// left `davinci_dom_corpus` at `divergences=0`.
///
/// Gated so no production build can reach it: the legacy lane is the thing
/// being strangled, and an entry point that resurrects it would be a way
/// back onto the old path rather than a measurement of it.
pub fn compile_template_legacy_with_options<'a>(
    allocator: &'a Allocator,
    source: &'a str,
    options: DomCompilerOptions,
) -> (RootNode<'a>, Vec<CompilerError>, CodegenResult) {
    let (root, errors, result) = compile_template_inner_with_sections(
        allocator,
        source,
        options,
        TemplateSyntaxMode::Standard,
        None,
        DomCompilePipelineOptions::deny_s2(
            CustomElementMatcher::default(),
            CodegenOptions::default(),
        ),
    );
    (root, errors, result.into_result())
}
