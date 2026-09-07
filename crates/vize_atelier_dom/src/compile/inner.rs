use vize_atelier_core::{
    CompilerError, RootNode,
    codegen::{CodegenResult, CodegenResultWithSections, generate_with_sections},
    lane::transform_with_custom_elements_and_template_syntax_quirks_and_hoisted_scope_id,
    options::{CodegenOptions, CustomElementMatcher, TemplateSyntaxMode},
    parser::parse_with_options_custom_elements_and_template_syntax,
    walk_probe::WalkCounts,
};
use vize_croquis::Croquis;
use vize_s0::{Allocator, String, profile, profiler::global_profiler};

use super::{pipeline::DomCompilePipelineOptions, source_map, stage_options};
use crate::options::DomCompilerOptions;

pub(super) fn compile_template_inner<'a>(
    allocator: &'a Allocator,
    source: &'a str,
    options: DomCompilerOptions,
    template_syntax: TemplateSyntaxMode,
    hoisted_scope_id: Option<String>,
    custom_elements: CustomElementMatcher,
    codegen_options: CodegenOptions,
) -> (RootNode<'a>, Vec<CompilerError>, CodegenResult) {
    let (root, errors, codegen_result) = compile_template_inner_with_sections(
        allocator,
        source,
        options,
        template_syntax,
        hoisted_scope_id,
        DomCompilePipelineOptions::allow_s2(custom_elements, codegen_options),
    );
    (root, errors, codegen_result.into_result())
}

pub(super) fn compile_template_inner_with_sections<'a>(
    allocator: &'a Allocator,
    source: &'a str,
    options: DomCompilerOptions,
    template_syntax: TemplateSyntaxMode,
    hoisted_scope_id: Option<String>,
    pipeline_options: DomCompilePipelineOptions,
) -> (RootNode<'a>, Vec<CompilerError>, CodegenResultWithSections) {
    let DomCompilePipelineOptions {
        custom_elements,
        codegen_options,
        s2_emit_selection,
    } = pipeline_options;
    let parser_opts = stage_options::parser_options(&options);

    let (mut root, errors) = profile!(
        "atelier.dom.template.parse",
        parse_with_options_custom_elements_and_template_syntax(
            allocator,
            source,
            parser_opts,
            custom_elements.clone(),
            template_syntax,
        )
    );

    // Parser-level diagnostics that are recoverable (e.g. duplicate
    // attribute — Vue keeps the first and continues) must NOT gate
    // codegen, or downstream callers see a 0-byte module reported as a
    // success. (#958) The recoverable diagnostics still ride along in
    // the returned errors vec so the caller can surface them as
    // warnings or test for parity.
    let fatal_count = errors.iter().filter(|e| !e.is_recoverable()).count();
    if fatal_count > 0 {
        let codegen_result = CodegenResult {
            code: String::default(),
            preamble: String::default(),
            map: None,
        };
        return (
            root,
            errors.to_vec(),
            CodegenResultWithSections {
                result: codegen_result,
                sections: None,
            },
        );
    }

    let has_croquis = options.croquis.is_some();
    let codegen_opts = stage_options::codegen_options(&options, codegen_options);
    let template_syntax_quirks = template_syntax.is_quirks();
    let use_s2_emit = stage_options::s2_emit_supported(
        &options,
        &codegen_opts,
        &custom_elements,
        template_syntax,
        has_croquis,
        s2_emit_selection,
    );
    let s2_custom_elements = custom_elements.clone();
    if use_s2_emit
        && !codegen_opts.source_map
        && let Some(result) = stage_options::try_emit_s2(
            allocator,
            source,
            &options,
            &codegen_opts,
            &s2_custom_elements,
            hoisted_scope_id.as_deref(),
            None,
        )
    {
        return (root, errors.to_vec(), result);
    }

    let s2_emit_after_transform = (use_s2_emit && codegen_opts.source_map)
        .then(|| (options.clone(), hoisted_scope_id.clone()));
    let transform_opts = stage_options::transform_options(&options);
    // Park the summary on the allocator so it shares the allocator lifetime.
    let analysis: Option<&Croquis> = options.croquis.map(|c| allocator.alloc_owned(*c));
    let profiling_s2 = use_s2_emit && global_profiler().is_enabled();
    let template_walk_before = profiling_s2.then(WalkCounts::snapshot);
    let transform_errors = profile!(
        "atelier.dom.template.transform",
        transform_with_custom_elements_and_template_syntax_quirks_and_hoisted_scope_id(
            allocator,
            &mut root,
            transform_opts,
            analysis,
            custom_elements,
            template_syntax_quirks,
            hoisted_scope_id,
        )
    );
    let template_walks = template_walk_before.map(|before| WalkCounts::snapshot().since(before));

    // Surface transform diagnostics (e.g. invalid expressions) alongside
    // parse errors instead of dropping them — the official compiler reports
    // both through the same `errors` channel.
    let mut errors = errors.to_vec();
    errors.extend(transform_errors);

    let s2_emit = s2_emit_after_transform.and_then(|(options, hoisted_scope_id)| {
        stage_options::try_emit_s2(
            allocator,
            source,
            &options,
            &codegen_opts,
            &s2_custom_elements,
            hoisted_scope_id.as_deref(),
            template_walks,
        )
    });
    let codegen_result = match s2_emit {
        Some(result) => source_map::attach_compat_map(&root, &codegen_opts, result),
        None => profile!(
            "atelier.dom.template.codegen_compat",
            generate_with_sections(&root, codegen_opts)
        ),
    };

    (root, errors, codegen_result)
}
