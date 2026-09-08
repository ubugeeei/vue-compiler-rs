//! Generating virtual TypeScript for `.vue` SFCs: parsing the template, running
//! Croquis analysis, augmenting type-based props, and emitting the `.vue.ts`
//! source consumed by Corsa. Parse/compile errors are surfaced as diagnostics
//! and replaced with a typed fallback module.

use std::path::Path;
use vize_carton::config::VueVersion;
use vize_carton::{Allocator, String as CompactString, cstr, profile};

use vize_atelier_core::{
    ParserOptions, TemplateSyntaxMode, parser::parse_with_options_and_template_syntax,
};
use vize_atelier_sfc::{
    SfcDescriptor,
    croquis::{
        SfcCroquisOptions, analyze_sfc_descriptor_with_context,
        analyze_sfc_descriptor_with_context_legacy_vue2,
        analyze_sfc_descriptor_with_context_options_api,
    },
};

use crate::batch::error::CorsaResult;
use crate::batch::{Diagnostic, SfcBlockType};
use crate::script_parse::collect_script_parse_diagnostics;
use crate::virtual_ts::{
    VirtualTsCheckOptions, VirtualTsGenerationOptions, VirtualTsOptions,
    generate_virtual_ts_with_offsets_and_checks,
};

use super::diagnostics::{
    collect_sfc_compile_diagnostic, diagnostic_for_offset, invalid_sfc_fallback_virtual_ts,
};
use super::{
    art_usage::collect_art_template_referenced_names,
    css_var_usage::collect_css_var_referenced_names,
    setup_props::{RuntimePropResolveCache, augment_type_based_props_from_script_context},
};

pub(super) struct GeneratedVueFile {
    pub(super) code: CompactString,
    pub(super) mappings: Vec<crate::virtual_ts::VizeMapping>,
    pub(super) semantic_links: Vec<crate::virtual_ts::VizeSemanticLink>,
    pub(super) diagnostics: Vec<Diagnostic>,
}

#[derive(Clone, Copy)]
pub(super) struct VueCodegenOptions<'a> {
    pub(super) check_options: VirtualTsCheckOptions,
    pub(super) preserve_unused_diagnostics: bool,
    pub(super) options_api: bool,
    pub(super) preserve_authored_component: bool,
    pub(super) component_name: Option<&'a str>,
    pub(super) preserve_event_navigation: bool,
    pub(super) legacy_vue2: bool,
    pub(super) dialect: VueVersion,
    pub(super) template_syntax: TemplateSyntaxMode,
    pub(super) experimental_in_tag_comments: bool,
    /// Hoist shared helpers to the batch ambient `.d.ts`; socket sessions keep
    /// them inline because they do not materialize that file.
    pub(super) hoist_shared_preamble: bool,
    /// Content-mapper transforms can run outside Vite projects.
    pub(super) omit_vite_client_reference: bool,
    /// Batch generation can share imported runtime prop/default resolution
    /// across rayon workers. Document/content-mapper callers use a per-file
    /// cache to avoid keeping stale source-derived state.
    pub(super) runtime_prop_resolve_cache: Option<&'a RuntimePropResolveCache>,
}

pub(super) fn generate_vue_virtual_ts(
    path: &Path,
    source: &str,
    descriptor: &SfcDescriptor,
    options: &VirtualTsOptions,
    codegen_options: VueCodegenOptions<'_>,
) -> CorsaResult<GeneratedVueFile> {
    let allocator = Allocator::new();
    let mut diagnostics = Vec::new();

    if let Some(ref script) = descriptor.script {
        let script_diagnostics = collect_script_parse_diagnostics(
            &script.content,
            script.loc.start as u32,
            script.lang.as_deref(),
        );
        if !script_diagnostics.is_empty() {
            diagnostics.extend(script_diagnostics.into_iter().map(|diagnostic| {
                diagnostic_for_offset(
                    path,
                    source,
                    diagnostic.start,
                    cstr!("Script parse error: {}", diagnostic.message),
                    SfcBlockType::Script,
                )
            }));
        }
    }

    if let Some(ref script_setup) = descriptor.script_setup {
        let script_diagnostics = collect_script_parse_diagnostics(
            &script_setup.content,
            script_setup.loc.start as u32,
            script_setup.lang.as_deref(),
        );
        if !script_diagnostics.is_empty() {
            diagnostics.extend(script_diagnostics.into_iter().map(|diagnostic| {
                diagnostic_for_offset(
                    path,
                    source,
                    diagnostic.start,
                    cstr!("Script parse error: {}", diagnostic.message),
                    SfcBlockType::ScriptSetup,
                )
            }));
        }
    }

    let template_offset = descriptor
        .template
        .as_ref()
        .map(|template| template.loc.start as u32)
        .unwrap_or(0);
    // Script parse errors leave no AST, so they always abort to the fallback
    // stub. Template diagnostics do not: they are counted separately below and
    // only *hard* ones abort.
    let script_hard_error = !diagnostics.is_empty();
    // Track whether the template produced any *hard* parse error. Only hard
    // errors abort codegen and collapse the file to the fallback stub.
    // Recovery-level diagnostics keep the real virtual TS:
    //   - `ErrorCode::ExtendPoint`, pushed by the HTML tree-construction
    //     recovery path for self-closing rewrites, fostered elements,
    //     auto-closed `<p>`, etc. (#1065/#1090 regression);
    //   - every code in `RECOVERED_PARSE_CODES`, for which the parser
    //     documents a concrete recovery and still yields a complete tree.
    //     Treating those as hard meant a single missing space between
    //     attributes collapsed the file to the stub and silenced every script
    //     type diagnostic in it (#3323) — the same false-negative shape #3294
    //     fixed in the linter, keyed off the same shared classification.
    let mut template_hard_error = false;
    let template_ast = descriptor.template.as_ref().and_then(|template| {
        profile!("canon.template.parse", {
            let (root, errors) = parse_with_options_and_template_syntax(
                &allocator,
                &template.content,
                ParserOptions {
                    experimental_in_tag_comments: codegen_options.experimental_in_tag_comments,
                    ..ParserOptions::default()
                },
                codegen_options.template_syntax,
            );
            for error in errors {
                if error.code.is_recovery() {
                    continue;
                }
                // A documented recovery still yields a complete tree, so the
                // defect is reported without suppressing the rest of the file.
                if !error.code.has_documented_parse_recovery() {
                    template_hard_error = true;
                }
                let start = error
                    .loc
                    .as_ref()
                    .map(|loc| template_offset + loc.span.start)
                    .unwrap_or(template_offset);
                diagnostics.push(diagnostic_for_offset(
                    path,
                    source,
                    start,
                    cstr!("Template parse error: {}", error.message),
                    SfcBlockType::Template,
                ));
            }
            // Drop the AST only when a hard error occurred; recovery-level
            // diagnostics leave a fully usable tree.
            (!template_hard_error).then_some(root)
        })
    });

    // Abort to the fallback stub only on hard errors — from any block. Pure
    // recovery-level template diagnostics must not suppress real codegen: the
    // parse diagnostic is still reported, alongside the script's own type
    // diagnostics, which is what `vize check` and the linter now agree on.
    if script_hard_error || template_hard_error {
        return Ok(GeneratedVueFile {
            code: invalid_sfc_fallback_virtual_ts(),
            mappings: Vec::new(),
            semantic_links: Vec::new(),
            diagnostics,
        });
    }

    let croquis_options = SfcCroquisOptions::full();
    let vue2_compat = codegen_options.legacy_vue2
        || matches!(codegen_options.dialect, VueVersion::V2 | VueVersion::V2_7);

    let analysis = profile!(
        "canon.croquis.analyze_sfc",
        if vue2_compat {
            analyze_sfc_descriptor_with_context_legacy_vue2(
                descriptor,
                template_ast.as_ref(),
                croquis_options,
            )
        } else if codegen_options.options_api {
            analyze_sfc_descriptor_with_context_options_api(
                descriptor,
                template_ast.as_ref(),
                croquis_options,
            )
        } else {
            analyze_sfc_descriptor_with_context(descriptor, template_ast.as_ref(), croquis_options)
        }
    );
    let split_script_setup_offsets = analysis.split_script_setup_offsets(descriptor);
    let mut croquis = analysis.croquis;
    let script_content = analysis.script_content;
    let script_offset = analysis.script_offset;
    let local_runtime_prop_resolve_cache;
    let runtime_prop_resolve_cache = if let Some(cache) = codegen_options.runtime_prop_resolve_cache
    {
        cache
    } else {
        local_runtime_prop_resolve_cache = RuntimePropResolveCache::default();
        &local_runtime_prop_resolve_cache
    };
    profile!(
        "canon.croquis.augment_type_props",
        augment_type_based_props_from_script_context(
            &mut croquis,
            descriptor,
            path,
            runtime_prop_resolve_cache
        )
    );
    // Names the generated component surface consumes outside `<template>`:
    // `<art>` variant templates and CSS `v-bind()` expressions. Both feed the
    // unused-binding anchors, which are otherwise narrowed to template-referenced
    // names so a genuinely unused binding still reports TS6133.
    let extra_template_referenced_names = codegen_options.preserve_unused_diagnostics.then(|| {
        let mut names = collect_art_template_referenced_names(
            descriptor,
            codegen_options.template_syntax,
            codegen_options.experimental_in_tag_comments,
        );
        names.extend(collect_css_var_referenced_names(descriptor));
        names
    });

    let hoist_shared_preamble = codegen_options.hoist_shared_preamble && !vue2_compat;
    let output = profile!(
        "canon.virtual_ts.generate",
        generate_virtual_ts_with_offsets_and_checks(
            &croquis,
            script_content.as_deref(),
            template_ast.as_ref(),
            script_offset,
            template_offset,
            options,
            VirtualTsGenerationOptions {
                check_options: codegen_options.check_options,
                dialect: codegen_options.dialect,
                preserve_unused_diagnostics: codegen_options.preserve_unused_diagnostics,
                extra_template_referenced_names: extra_template_referenced_names.as_ref(),
                options_api: codegen_options.options_api || vue2_compat,
                preserve_authored_component: codegen_options.preserve_authored_component,
                component_name: codegen_options.component_name,
                preserve_event_navigation: codegen_options.preserve_event_navigation,
                legacy_vue2: vue2_compat,
                template_syntax_quirks: matches!(
                    codegen_options.template_syntax,
                    TemplateSyntaxMode::Quirks
                ),
                hoist_shared_preamble,
                lib_references: None,
                omit_vite_client_reference: codegen_options.omit_vite_client_reference,
                split_script_setup_offsets,
            },
        )
    );

    // Surface Vue-specific semantic errors (e.g. DEFINE_PROPS_DESTRUCTURE_DEFAULT_TYPE)
    // that the SFC compiler catches but TypeScript itself does not. Without this,
    // `vize check` would silently accept SFCs that `vize build` rejects.
    if let Some(diagnostic) = profile!(
        "canon.sfc.compile_validate",
        collect_sfc_compile_diagnostic(path, source, descriptor)
    ) {
        diagnostics.push(diagnostic);
    }

    Ok(GeneratedVueFile {
        code: output.code,
        mappings: output.mappings,
        semantic_links: output.semantic_links,
        diagnostics,
    })
}
