use std::path::Path;

use oxc_span::SourceType;
use vize_carton::{ToCompactString, profile};

use crate::batch::error::CorsaResult;
use crate::batch::import_rewriter::{ImportRewriter, VirtualAliasRewritePolicy};
use crate::batch::source_map::CompositeSourceMap;

use super::super::VirtualFile;
use super::super::esm_declaration_spelling::should_preserve_esm_declaration_spelling;
use super::super::passthrough::collect_passthrough_modules;
use super::RegisteredFile;

pub(in crate::batch::virtual_project) fn build_script_registered_file(
    path: &Path,
    content: &str,
    source_type: SourceType,
    roots: (&Path, &Path),
    rewriter: &ImportRewriter,
    alias_rewrite_policy: Option<&VirtualAliasRewritePolicy>,
    preserve_relative_declarations: bool,
    preserve_declaration_spelling: bool,
) -> CorsaResult<RegisteredFile> {
    let rewritten = profile!("canon.import.rewrite.script", {
        if preserve_relative_declarations {
            rewriter.rewrite_for_package_shadow_with_alias_policy(
                content,
                source_type,
                roots,
                path.parent(),
                alias_rewrite_policy,
            )
        } else {
            rewriter.rewrite_for_virtual_project_with_alias_policy(
                content,
                source_type,
                roots,
                path.parent(),
                alias_rewrite_policy,
            )
        }
    });
    let preserve_declaration_spelling =
        preserve_declaration_spelling || should_preserve_esm_declaration_spelling(path, content);
    let virtual_path = super::super::paths::script_virtual_path(
        roots,
        path,
        content,
        preserve_declaration_spelling,
    )?;

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
