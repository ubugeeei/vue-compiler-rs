//! Single-document Vue virtual TS generation for editor/socket paths.

use std::path::Path;

use oxc_span::SourceType;
use vize_atelier_core::TemplateSyntaxMode;
use vize_atelier_sfc::{SfcParseOptions, parse_sfc};
use vize_carton::{String as CompactString, ToCompactString};

use crate::batch::error::{CorsaError, CorsaResult};
use crate::batch::import_rewriter::{ImportRewriter, ImportSourceMap};
use crate::virtual_ts::{VirtualTsCheckOptions, VirtualTsOptions, VizeMapping, VizeSemanticLink};

use super::build::{
    descriptor_uses_jsx_script, prepend_vue_jsx_reference, virtual_ts_options_for_descriptor,
};
use super::vue_codegen::{GeneratedVueFile, VueCodegenOptions, generate_vue_virtual_ts};

/// Rewritten virtual TypeScript for a single in-memory `.vue` document.
pub struct VueDocumentVirtualTs {
    /// `.vue.ts` source after `.vue -> .vue.ts` import rewriting.
    pub code: CompactString,
    /// Generated source before import rewriting, used for sibling overlays.
    pub pre_rewrite_code: CompactString,
    /// Byte-range source mappings in pre-rewrite generated TS coordinates.
    pub mappings: Vec<VizeMapping>,
    /// Semantic links in pre-rewrite generated TS coordinates.
    pub semantic_links: Vec<VizeSemanticLink>,
    /// Source map for `.vue -> .vue.ts` import rewrites.
    pub import_source_map: ImportSourceMap,
    /// Source type used for parsing the generated virtual document.
    pub source_type: SourceType,
    /// Suffix appended to the original `.vue` URI/path for socket-mode Corsa.
    pub virtual_suffix: &'static str,
}

/// Vue single-document generation options used by editor/socket callers.
#[derive(Clone, Copy, Debug)]
pub struct VueDocumentVirtualTsOptions {
    pub options_api: bool,
    pub legacy_vue2: bool,
    pub preserve_event_navigation: bool,
    pub dialect: vize_carton::config::VueVersion,
    pub preserve_missing_vue_diagnostics: bool,
}

impl Default for VueDocumentVirtualTsOptions {
    fn default() -> Self {
        Self {
            options_api: false,
            legacy_vue2: false,
            preserve_event_navigation: false,
            dialect: vize_carton::config::VueVersion::default(),
            preserve_missing_vue_diagnostics: true,
        }
    }
}

/// Generate the rewritten virtual TypeScript for one in-memory `.vue` document.
pub fn generate_vue_document_virtual_ts(
    path: &Path,
    content: &str,
    options: &VirtualTsOptions,
    rewriter: &ImportRewriter,
    hoist_shared_preamble: bool,
) -> CorsaResult<VueDocumentVirtualTs> {
    generate_vue_document_virtual_ts_with_options(
        path,
        content,
        options,
        rewriter,
        hoist_shared_preamble,
        VueDocumentVirtualTsOptions::default(),
    )
}

pub fn generate_vue_document_virtual_ts_with_options(
    path: &Path,
    content: &str,
    virtual_ts_options: &VirtualTsOptions,
    rewriter: &ImportRewriter,
    hoist_shared_preamble: bool,
    options: VueDocumentVirtualTsOptions,
) -> CorsaResult<VueDocumentVirtualTs> {
    generate_vue_document_virtual_ts_with_options_and_alias_resolver(
        path,
        content,
        virtual_ts_options,
        rewriter,
        hoist_shared_preamble,
        options,
        None,
    )
}

pub(crate) fn generate_vue_document_virtual_ts_with_options_and_alias_resolver(
    path: &Path,
    content: &str,
    options: &VirtualTsOptions,
    rewriter: &ImportRewriter,
    hoist_shared_preamble: bool,
    document_options: VueDocumentVirtualTsOptions,
    alias_resolver: Option<crate::batch::import_rewriter_alias::AliasSpecifierResolver<'_>>,
) -> CorsaResult<VueDocumentVirtualTs> {
    let descriptor = parse_sfc(
        content,
        SfcParseOptions {
            filename: path.to_string_lossy().to_compact_string(),
            ..Default::default()
        },
    )
    .map_err(|error| CorsaError::SfcParse(error.message.to_compact_string()))?;

    let effective_options = virtual_ts_options_for_descriptor(options, &descriptor);
    let use_tsx_virtual = descriptor_uses_jsx_script(&descriptor);
    let source_type = if use_tsx_virtual {
        SourceType::tsx()
    } else {
        SourceType::ts()
    };
    let GeneratedVueFile {
        mut code,
        mut mappings,
        mut semantic_links,
        ..
    } = generate_vue_virtual_ts(
        path,
        content,
        &descriptor,
        &effective_options,
        VueCodegenOptions {
            check_options: VirtualTsCheckOptions::default(),
            preserve_unused_diagnostics: false,
            options_api: document_options.options_api,
            preserve_authored_component: false,
            component_name: None,
            preserve_event_navigation: document_options.preserve_event_navigation,
            legacy_vue2: document_options.legacy_vue2,
            dialect: document_options.dialect,
            template_syntax: TemplateSyntaxMode::default(),
            experimental_in_tag_comments: false,
            hoist_shared_preamble,
            omit_vite_client_reference: false,
            runtime_prop_resolve_cache: None,
        },
    )?;
    if use_tsx_virtual {
        prepend_vue_jsx_reference(&mut code, &mut mappings, &mut semantic_links);
    }

    let rewritten = match alias_resolver {
        Some(resolver) => rewriter.rewrite_with_alias_resolver_and_missing_vue_policy(
            &code,
            source_type,
            path.parent(),
            resolver,
            document_options.preserve_missing_vue_diagnostics,
        ),
        None => rewriter.rewrite_with_missing_vue_policy(
            &code,
            source_type,
            path.parent(),
            document_options.preserve_missing_vue_diagnostics,
        ),
    };
    Ok(VueDocumentVirtualTs {
        code: rewritten.code,
        pre_rewrite_code: code,
        mappings,
        semantic_links,
        import_source_map: rewritten.source_map,
        source_type,
        virtual_suffix: if use_tsx_virtual { ".tsx" } else { ".ts" },
    })
}
