//! DOM template compilation: parse, transform, and codegen entry points.

pub mod custom_elements;

use vize_atelier_core::codegen::{CodegenResult, CodegenResultWithSections};
use vize_atelier_core::{
    CompilerError, RootNode,
    codegen::generate_with_sections,
    lane::transform_with_custom_elements_and_template_syntax_quirks_and_hoisted_scope_id,
    options::{CodegenOptions, CustomElementMatcher, TemplateSyntaxMode},
    parser::parse_with_options_custom_elements_and_template_syntax,
    walk_probe::WalkCounts,
};
use vize_croquis::Croquis;
use vize_s0::{Allocator, String, profile, profiler::global_profiler};

mod pipeline;
mod sfc;
mod source_map;
mod stage_options;

use crate::options::DomCompilerOptions;
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

fn compile_template_inner<'a>(
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

fn compile_template_inner_with_sections<'a>(
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
    // Project output options before consuming Croquis; S2 only borrows them.
    let s2_custom_elements = custom_elements.clone();
    let s2_emit_source = use_s2_emit.then(|| (options.clone(), hoisted_scope_id.clone()));
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

    let s2_emit = s2_emit_source.and_then(|(s2_options_source, s2_hoisted_scope_id)| {
        let binding_table =
            stage_options::s2_binding_table(s2_options_source.binding_metadata.as_ref());
        let s2_options = stage_options::s2_emit_options(
            &s2_options_source,
            &codegen_opts,
            &s2_custom_elements,
            binding_table.as_ref(),
            s2_hoisted_scope_id.as_deref(),
        )?;
        let dialect = s2_options_source.dialect;
        Some(profile!(
            "atelier.dom.template.s2_codegen",
            stage_options::emit_s2(allocator, source, dialect, &s2_options, template_walks)
        ))
    });
    let codegen_result = match s2_emit {
        Some(Ok(result)) => source_map::attach_compat_map(&root, &codegen_opts, result),
        Some(Err(_)) | None => profile!(
            "atelier.dom.template.codegen_compat",
            generate_with_sections(&root, codegen_opts)
        ),
    };

    (root, errors, codegen_result)
}
