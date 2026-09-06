use super::{
    compile_template_inner_with_sections,
    pipeline::{self, DomCompilePipelineOptions},
    stage_options,
};
use crate::options::DomCompilerOptions;
use vize_atelier_core::{
    CompilerError, RootNode,
    codegen::{CodegenResult, CodegenResultWithSections},
    options::{CodegenOptions, CustomElementMatcher, TemplateSyntaxMode},
};
use vize_s0::{Allocator, String, profile};

mod selector;

pub(super) fn compile_template_inner_for_sfc_with_sections<'a>(
    allocator: &'a Allocator,
    source: &'a str,
    options: DomCompilerOptions,
    template_syntax: TemplateSyntaxMode,
    hoisted_scope_id: Option<String>,
    custom_elements: CustomElementMatcher,
    codegen_options: CodegenOptions,
) -> (Vec<CompilerError>, CodegenResultWithSections) {
    let codegen_opts = stage_options::codegen_options(&options, codegen_options.clone());
    let use_s2_emit = stage_options::s2_emit_supported(
        &options,
        &codegen_opts,
        &custom_elements,
        template_syntax,
        options.croquis.is_some(),
        pipeline::S2EmitSelection::RequireSections,
    );

    let mut force_compat_sections = false;
    let fast_path_supported = selector::s2_sfc_fast_path_supported_source(source);

    if use_s2_emit && fast_path_supported && !codegen_opts.source_map {
        let binding_table = stage_options::s2_binding_table(options.binding_metadata.as_ref());
        let s2_options = stage_options::s2_emit_options(
            &options,
            &codegen_opts,
            &custom_elements,
            binding_table.as_ref(),
            hoisted_scope_id.as_deref(),
        );
        if let Some(s2_options) = s2_options
            && let Ok(result) = profile!(
                "atelier.dom.template.s2_codegen_sfc_fast",
                stage_options::emit_s2(allocator, source, options.dialect, &s2_options, None)
            )
        {
            return (Vec::new(), result);
        }
        force_compat_sections = true;
    }

    let pipeline_options = if force_compat_sections {
        DomCompilePipelineOptions::require_sections_compat(custom_elements, codegen_options)
    } else {
        DomCompilePipelineOptions::require_sections(custom_elements, codegen_options)
    };

    let (_, errors, result) = compile_template_inner_with_sections(
        allocator,
        source,
        options,
        template_syntax,
        hoisted_scope_id,
        pipeline_options,
    );
    (errors, result)
}

pub(super) fn compile_template_inner_for_sfc<'a>(
    allocator: &'a Allocator,
    source: &'a str,
    options: DomCompilerOptions,
    template_syntax: TemplateSyntaxMode,
    hoisted_scope_id: Option<String>,
    custom_elements: CustomElementMatcher,
    codegen_options: CodegenOptions,
) -> (RootNode<'a>, Vec<CompilerError>, CodegenResult) {
    let (root, errors, result) = compile_template_inner_with_sections(
        allocator,
        source,
        options,
        template_syntax,
        hoisted_scope_id,
        DomCompilePipelineOptions::require_sections_compat(custom_elements, codegen_options),
    );
    (root, errors, result.into_result())
}
