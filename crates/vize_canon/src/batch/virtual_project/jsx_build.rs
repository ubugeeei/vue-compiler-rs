use std::path::{Path, PathBuf};

use oxc_span::SourceType;
use vize_carton::{ToCompactString, cstr, profile};

use crate::batch::error::{CorsaError, CorsaResult};
use crate::batch::source_map::{CompositeSourceMap, SfcSourceMap};

use super::VirtualFile;
use super::build::{RegisteredFile, VirtualBuildContext, mirrored_virtual_path};
use super::passthrough::collect_passthrough_modules;

/// Build a virtual file for a `.jsx`/`.tsx` Vize component (#1497, opt-in).
pub(super) fn build_jsx_registered_file(
    path: &Path,
    content: &str,
    context: VirtualBuildContext<'_>,
) -> CorsaResult<RegisteredFile> {
    let lang = jsx_lang_for_path(path);
    let generated = profile!(
        "canon.jsx.virtual_ts",
        super::jsx_codegen::generate_jsx_virtual_ts(path, content, lang)
    )?;
    let super::jsx_codegen::GeneratedJsxFile {
        code,
        mappings,
        diagnostics,
    } = generated;

    let rewritten = profile!(
        "canon.import.rewrite.jsx",
        context
            .rewriter
            .rewrite_with_missing_vue_policy_and_alias_policy(
                &code,
                SourceType::ts(),
                path.parent(),
                true,
                context.alias_rewrite_policy,
            )
    );

    let blocks = vec![crate::batch::source_map::SfcBlockRange {
        start: 0,
        end: content.len() as u32,
        block_type: crate::batch::SfcBlockType::Script,
    }];
    let source_map =
        CompositeSourceMap::new_vue(SfcSourceMap::new(mappings, blocks), rewritten.source_map);
    let virtual_path = virtual_jsx_path(context.project_root, context.virtual_root, path)?;

    Ok(RegisteredFile {
        file: VirtualFile {
            content: rewritten.code,
            source_map,
            original_path: path.to_path_buf(),
            virtual_path,
        },
        extra_virtual_files: Vec::new(),
        original_content: content.to_compact_string(),
        passthrough_files: collect_passthrough_modules(
            path,
            content,
            context.project_root,
            context.virtual_root,
        ),
        diagnostics,
        // `.jsx`/`.tsx` routing through this path is the explicit
        // `typeChecker.jsxTypecheck` opt-in (#1497), so it is never gated on
        // `checkJs` the way a JavaScript `.vue` script block is (#3322).
        unchecked_javascript: false,
    })
}

fn jsx_lang_for_path(path: &Path) -> vize_atelier_jsx::JsxLang {
    if path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with(".tsx"))
    {
        vize_atelier_jsx::JsxLang::Tsx
    } else {
        vize_atelier_jsx::JsxLang::Jsx
    }
}

fn virtual_jsx_path(project_root: &Path, virtual_root: &Path, path: &Path) -> CorsaResult<PathBuf> {
    let mut virtual_path = mirrored_virtual_path(project_root, virtual_root, path)?;
    let file_name = virtual_path
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| cstr!("{name}.ts"))
        .ok_or_else(|| CorsaError::PathError {
            path: path.to_path_buf(),
        })?;
    virtual_path.set_file_name(file_name.as_str());
    Ok(virtual_path)
}
