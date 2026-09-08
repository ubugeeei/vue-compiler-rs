//! Builds an owned [`RegisteredFile`] so SFC/template parsing and virtual-TS generation can run
//! across rayon workers without shared `&mut` state.

use std::path::{Path, PathBuf};

use oxc_span::SourceType;
use vize_atelier_core::TemplateSyntaxMode;
use vize_carton::{String as CompactString, ToCompactString, cstr, profile};

use vize_atelier_sfc::{SfcParseOptions, parse_sfc};

use crate::batch::Diagnostic;
use crate::batch::declaration_path::is_declaration_file;
use crate::batch::error::{CorsaError, CorsaResult};
use crate::batch::import_rewriter::ImportRewriter;
use crate::batch::source_map::{CompositeSourceMap, SfcSourceMap};
use crate::virtual_ts::{VirtualTsCheckOptions, VirtualTsOptions, VizeMapping};

mod css_modules;
pub(super) use super::paths::source_type_for_path;
pub(super) use css_modules::virtual_ts_options_for_descriptor;

use super::VirtualFile;
use super::diagnostics::collect_sfc_block_ranges;
use super::esm_declaration_spelling::should_preserve_esm_declaration_spelling;
use super::javascript_sfc::descriptor_is_unchecked_javascript;
pub(super) use super::javascript_sfc::descriptor_uses_jsx_script;
use super::jsx_build::build_jsx_registered_file;
use super::passthrough::collect_passthrough_modules;
use super::setup_props::RuntimePropResolveCache;
use super::vue_codegen::{GeneratedVueFile, VueCodegenOptions, generate_vue_virtual_ts};

pub(super) const VUE_JSX_REFERENCE_DIRECTIVE: &str = "/// <reference types=\"vue/jsx\" />\n";

/// Result of building a virtual file for a registered path, owned and
/// independent of any `&mut VirtualProject` so it can be produced in parallel.
pub(super) struct RegisteredFile {
    pub(super) file: VirtualFile,
    pub(super) extra_virtual_files: Vec<VirtualFile>,
    /// Original source text as registered, retained for offset<->line/col
    /// mapping without a disk re-read. Stored on the project, not the public
    /// `VirtualFile`.
    pub(super) original_content: CompactString,
    pub(super) passthrough_files: Vec<(PathBuf, PathBuf)>,
    pub(super) diagnostics: Vec<Diagnostic>,
    /// SFC whose script block is JavaScript: TypeScript diagnostics on it are
    /// reportable only under `checkJs` (see `javascript_sfc`, #3322).
    pub(super) unchecked_javascript: bool,
}

#[derive(Clone, Copy)]
pub(super) struct VirtualBuildContext<'a> {
    pub(super) project_root: &'a Path,
    pub(super) virtual_root: &'a Path,
    pub(super) virtual_ts_options: &'a VirtualTsOptions,
    pub(super) virtual_ts_check_options: VirtualTsCheckOptions,
    pub(super) preserve_unused_diagnostics: bool,
    pub(super) options_api: bool,
    pub(super) legacy_vue2: bool,
    pub(super) jsx_typecheck: bool,
    pub(super) dialect: vize_carton::config::VueVersion,
    pub(super) template_syntax: TemplateSyntaxMode,
    pub(super) experimental_in_tag_comments: bool,
    pub(super) hoist_shared_preamble: bool,
    pub(super) preserve_relative_declarations: bool,
    pub(super) preserve_declaration_spelling: bool,
    pub(super) rewriter: &'a ImportRewriter,
    pub(super) runtime_prop_resolve_cache: Option<&'a RuntimePropResolveCache>,
}

pub(super) fn build_registered_file(
    path: &Path,
    content: &str,
    context: VirtualBuildContext<'_>,
) -> CorsaResult<RegisteredFile> {
    if path.extension().and_then(|extension| extension.to_str()) == Some("vue") {
        return build_vue_registered_file(path, content, context);
    }

    if is_declaration_file(path) {
        return build_script_registered_file(
            path,
            content,
            SourceType::ts(),
            (context.project_root, context.virtual_root),
            context.rewriter,
            context.preserve_relative_declarations,
            context.preserve_declaration_spelling,
        );
    }

    // Opt-in Vize JSX/TSX type-checking (#1497). Only when the user explicitly
    // enabled `typeChecker.jsxTypecheck`: otherwise `.jsx`/`.tsx` is passed to
    // TypeScript verbatim (React passthrough) by the script path below.
    if context.jsx_typecheck
        && let Some(name) = path.file_name().and_then(|name| name.to_str())
        && (name.ends_with(".jsx") || name.ends_with(".tsx"))
    {
        return build_jsx_registered_file(path, content, context);
    }

    let source_type = source_type_for_path(path).ok_or_else(|| CorsaError::PathError {
        path: path.to_path_buf(),
    })?;
    build_script_registered_file(
        path,
        content,
        source_type,
        (context.project_root, context.virtual_root),
        context.rewriter,
        context.preserve_relative_declarations,
        context.preserve_declaration_spelling,
    )
}

pub(super) fn build_vue_registered_file(
    path: &Path,
    content: &str,
    context: VirtualBuildContext<'_>,
) -> CorsaResult<RegisteredFile> {
    let descriptor = profile!(
        "canon.sfc.parse",
        parse_sfc(
            content,
            SfcParseOptions {
                filename: path.to_string_lossy().to_compact_string(),
                ..Default::default()
            },
        )
        .map_err(|error| CorsaError::SfcParse(error.message.to_compact_string()))
    )?;

    let effective_options =
        virtual_ts_options_for_descriptor(context.virtual_ts_options, &descriptor);
    let use_tsx_virtual = descriptor_uses_jsx_script(&descriptor);
    let source_type = if use_tsx_virtual {
        SourceType::tsx()
    } else {
        SourceType::ts()
    };
    let generated = profile!(
        "canon.vue.virtual_ts",
        generate_vue_virtual_ts(
            path,
            content,
            &descriptor,
            &effective_options,
            VueCodegenOptions {
                check_options: context.virtual_ts_check_options,
                preserve_unused_diagnostics: context.preserve_unused_diagnostics,
                options_api: context.options_api,
                // Batch check/declaration codegen must keep the authored
                // default export so Options API instance members survive
                // `InstanceType<typeof Component>` (#4010).
                preserve_authored_component: true,
                component_name: None,
                preserve_event_navigation: false,
                legacy_vue2: context.legacy_vue2,
                dialect: context.dialect,
                template_syntax: context.template_syntax,
                experimental_in_tag_comments: context.experimental_in_tag_comments,
                hoist_shared_preamble: context.hoist_shared_preamble,
                omit_vite_client_reference: false,
                runtime_prop_resolve_cache: context.runtime_prop_resolve_cache,
            },
        )
    )?;
    let GeneratedVueFile {
        mut code,
        mut mappings,
        mut semantic_links,
        diagnostics,
    } = generated;
    if use_tsx_virtual {
        prepend_vue_jsx_reference(&mut code, &mut mappings, &mut semantic_links);
    }
    let rewritten = profile!(
        "canon.import.rewrite.vue",
        context.rewriter.rewrite(&code, source_type, path.parent())
    );
    let source_map = CompositeSourceMap::new_vue(
        SfcSourceMap::new_with_semantic_links(
            mappings,
            collect_sfc_block_ranges(&descriptor),
            semantic_links,
        ),
        rewritten.source_map,
    );
    let virtual_path = virtual_vue_path(
        context.project_root,
        context.virtual_root,
        path,
        use_tsx_virtual,
    )?;
    let extra_virtual_files = if use_tsx_virtual {
        vec![build_tsx_vue_import_shim(
            context.project_root,
            context.virtual_root,
            path,
            &virtual_path,
        )?]
    } else {
        Vec::new()
    };

    Ok(RegisteredFile {
        file: VirtualFile {
            content: rewritten.code,
            source_map,
            original_path: path.to_path_buf(),
            virtual_path,
        },
        extra_virtual_files,
        original_content: content.to_compact_string(),
        passthrough_files: collect_passthrough_modules(
            path,
            content,
            context.project_root,
            context.virtual_root,
        ),
        diagnostics,
        unchecked_javascript: descriptor_is_unchecked_javascript(&descriptor),
    })
}

pub(super) fn build_script_registered_file(
    path: &Path,
    content: &str,
    source_type: SourceType,
    roots: (&Path, &Path),
    rewriter: &ImportRewriter,
    preserve_relative_declarations: bool,
    preserve_declaration_spelling: bool,
) -> CorsaResult<RegisteredFile> {
    let rewritten = profile!("canon.import.rewrite.script", {
        if preserve_relative_declarations {
            rewriter.rewrite_for_package_shadow(content, source_type, roots, path.parent())
        } else {
            rewriter.rewrite_for_virtual_project(content, source_type, roots, path.parent())
        }
    });
    let preserve_declaration_spelling =
        preserve_declaration_spelling || should_preserve_esm_declaration_spelling(path, content);
    let virtual_path =
        super::paths::script_virtual_path(roots, path, content, preserve_declaration_spelling)?;

    Ok(RegisteredFile {
        file: VirtualFile {
            content: rewritten.code,
            source_map: CompositeSourceMap::new_script(rewritten.source_map),
            original_path: path.to_path_buf(),
            virtual_path,
        },
        extra_virtual_files: Vec::new(),
        original_content: content.to_compact_string(),
        passthrough_files: collect_passthrough_modules(path, content, roots.0, roots.1),
        diagnostics: Vec::new(),
        unchecked_javascript: false,
    })
}

pub(super) fn prepend_vue_jsx_reference(
    code: &mut CompactString,
    mappings: &mut [VizeMapping],
    semantic_links: &mut [crate::virtual_ts::VizeSemanticLink],
) {
    if code.starts_with(VUE_JSX_REFERENCE_DIRECTIVE) {
        return;
    }
    let offset = VUE_JSX_REFERENCE_DIRECTIVE.len();
    let mut prefixed = CompactString::with_capacity(offset + code.len());
    prefixed.push_str(VUE_JSX_REFERENCE_DIRECTIVE);
    prefixed.push_str(code.as_str());
    *code = prefixed;
    for mapping in mappings {
        mapping.gen_range.start += offset;
        mapping.gen_range.end += offset;
        for sub_span in &mut mapping.sub_spans {
            sub_span.gen_range.start += offset;
            sub_span.gen_range.end += offset;
        }
    }
    for link in semantic_links {
        link.shift_generated_ranges(offset);
    }
}

pub(super) fn mirrored_virtual_path(
    project_root: &Path,
    virtual_root: &Path,
    path: &Path,
) -> CorsaResult<PathBuf> {
    if let Ok(relative) = path.strip_prefix(project_root) {
        return Ok(virtual_root.join(relative));
    }
    // Out-of-root files land in the external escape subtree (#3887).
    super::external_mirror::external_mirror_path(virtual_root, path)
}

fn build_tsx_vue_import_shim(
    project_root: &Path,
    virtual_root: &Path,
    path: &Path,
    target_path: &Path,
) -> CorsaResult<VirtualFile> {
    let shim_path = virtual_vue_path(project_root, virtual_root, path, false)?;
    let target_name = target_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| CorsaError::PathError {
            path: path.to_path_buf(),
        })?;
    let content = cstr!(
        "export {{ default }} from \"./{target_name}\";\nexport * from \"./{target_name}\";\n"
    );

    Ok(VirtualFile {
        content,
        source_map: CompositeSourceMap::empty(),
        original_path: shim_path.clone(),
        virtual_path: shim_path,
    })
}

fn virtual_vue_path(
    project_root: &Path,
    virtual_root: &Path,
    path: &Path,
    use_tsx_virtual: bool,
) -> CorsaResult<PathBuf> {
    let mut virtual_path = mirrored_virtual_path(project_root, virtual_root, path)?;
    let file_name = virtual_path
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| {
            if use_tsx_virtual {
                cstr!("{name}.tsx")
            } else {
                cstr!("{name}.ts")
            }
        })
        .ok_or_else(|| CorsaError::PathError {
            path: path.to_path_buf(),
        })?;
    virtual_path.set_file_name(file_name.as_str());
    Ok(virtual_path)
}
