//! Virtual TypeScript generation from Vue SFCs and `.art.vue` files.

use tower_lsp::lsp_types::Url;

use super::super::{DiagnosticService, SourceMapping, VirtualTsResult};
use vize_canon::{CorsaVueVirtualDocument, ImportRewriter, ImportSourceMap};

struct VirtualTsMetadata {
    user_code_start_line: u32,
    sfc_script_start_line: u32,
    template_scope_start_line: u32,
    line_mappings: Vec<Option<SourceMapping>>,
    skipped_import_lines: u32,
}

/// Apply `ImportRewriter` to the generated virtual TS so `.vue` imports
/// resolve to the generated `.vue.ts` mirrors in the editor Corsa session.
///
/// The rewrite only changes bytes *inside* import specifier strings (single
/// line), so line numbers are preserved — only column offsets within affected
/// lines shift. Returns the rewritten code and a byte-offset source map that
/// `map_diagnostic_with_source_mappings` uses to translate post-rewrite
/// diagnostic offsets back into pre-rewrite virtual TS offsets (which are the
/// coordinate system the byte-range source mappings operate in).
pub(super) fn rewrite_vue_imports(code: &str) -> (std::string::String, ImportSourceMap) {
    use oxc_span::SourceType;
    let result = ImportRewriter::new().rewrite(code, SourceType::ts(), None);
    #[allow(clippy::disallowed_methods)]
    (result.code.to_string(), result.source_map)
}

impl DiagnosticService {
    pub(in crate::ide) fn virtual_ts_result_from_corsa_vue_document(
        uri: &Url,
        content: &str,
        opened: CorsaVueVirtualDocument,
    ) -> Option<(std::string::String, VirtualTsResult)> {
        let metadata = Self::virtual_ts_metadata(uri, content, &opened.pre_rewrite_code)?;
        Some((
            opened.request_uri.to_string(),
            VirtualTsResult {
                code: opened.code.to_string(),
                source_mappings: opened.mappings,
                semantic_links: semantic_links_after_import_rewrite(
                    opened.semantic_links,
                    &opened.import_source_map,
                ),
                import_source_map: opened.import_source_map,
                user_code_start_line: metadata.user_code_start_line,
                sfc_script_start_line: metadata.sfc_script_start_line,
                template_scope_start_line: metadata.template_scope_start_line,
                line_mappings: metadata.line_mappings,
                skipped_import_lines: metadata.skipped_import_lines,
            },
        ))
    }

    /// Generate virtual TypeScript for a Vue SFC.
    #[cfg(test)]
    pub(in crate::ide) fn generate_virtual_ts(
        uri: &Url,
        content: &str,
        options_api: bool,
        legacy_vue2: bool,
    ) -> Option<VirtualTsResult> {
        use std::path::Path;
        use vize_canon::{
            batch::{VueDocumentVirtualTsOptions, generate_vue_document_virtual_ts_with_options},
            virtual_ts::VirtualTsOptions,
        };

        let virtual_ts_options = VirtualTsOptions::default();
        let rewriter = ImportRewriter::new();
        let generated = generate_vue_document_virtual_ts_with_options(
            Path::new(uri.path()),
            content,
            &virtual_ts_options,
            &rewriter,
            false,
            VueDocumentVirtualTsOptions {
                options_api,
                legacy_vue2,
                preserve_event_navigation: false,
                dialect: Default::default(),
                preserve_missing_vue_diagnostics: false,
            },
        )
        .ok()?;
        let code = generated.pre_rewrite_code;
        let metadata = Self::virtual_ts_metadata(uri, content, &code)?;

        // The generated code is the same rewritten `.vue.ts` document that
        // CorsaBridge syncs for editor sessions; this helper keeps the mapping
        // metadata available to tests without owning dependency synchronization.
        Some(VirtualTsResult {
            code: generated.code.to_string(),
            source_mappings: generated.mappings,
            semantic_links: semantic_links_after_import_rewrite(
                generated.semantic_links,
                &generated.import_source_map,
            ),
            import_source_map: generated.import_source_map,
            user_code_start_line: metadata.user_code_start_line,
            sfc_script_start_line: metadata.sfc_script_start_line,
            template_scope_start_line: metadata.template_scope_start_line,
            line_mappings: metadata.line_mappings,
            skipped_import_lines: metadata.skipped_import_lines,
        })
    }

    fn virtual_ts_metadata(
        uri: &Url,
        content: &str,
        pre_rewrite_code: &str,
    ) -> Option<VirtualTsMetadata> {
        use vize_atelier_sfc::{
            SfcParseOptions,
            croquis::{SfcCroquisOptions, script_content_for_descriptor},
            parse_sfc,
        };

        let options = SfcParseOptions {
            filename: uri.path().to_string().into(),
            ..Default::default()
        };
        let descriptor = parse_sfc(content, options).ok()?;
        let (script_content, script_offset) =
            script_content_for_descriptor(&descriptor, SfcCroquisOptions::full());
        let sfc_script_start_line = if script_content.as_ref().is_some_and(|s| s.is_empty()) {
            1
        } else {
            crate::ide::offset_to_position(content, script_offset as usize).0 + 1
        };
        let user_code_start_line = pre_rewrite_code
            .lines()
            .enumerate()
            .find(|(_, line)| line.contains("// User setup code"))
            .map(|(i, _)| i as u32 + 1)
            .unwrap_or(0);
        let template_scope_start_line = pre_rewrite_code
            .lines()
            .enumerate()
            .find(|(_, line)| line.contains("Template Scope"))
            .map(|(i, _)| i as u32)
            .unwrap_or(u32::MAX);

        Some(VirtualTsMetadata {
            user_code_start_line,
            sfc_script_start_line,
            template_scope_start_line,
            line_mappings: Self::parse_vize_map_comments(pre_rewrite_code),
            skipped_import_lines: Self::count_import_lines(
                script_content.as_deref().unwrap_or_default(),
            ),
        })
    }

    /// Count the number of import lines in script content.
    /// Handles multi-line imports.
    pub(in crate::ide::diagnostics) fn count_import_lines(script: &str) -> u32 {
        let lines: Vec<&str> = script.lines().collect();
        let mut count = 0u32;
        let mut in_import = false;

        for line in lines {
            let trimmed = line.trim();

            if trimmed.starts_with("import ") {
                in_import = true;
                count += 1;
                // Check if this is a single-line import
                if trimmed.ends_with(';') || trimmed.contains(" from ") {
                    in_import = false;
                }
            } else if in_import {
                count += 1;
                // Check if this line ends the import
                if trimmed.ends_with(';') {
                    in_import = false;
                }
            }
        }

        count
    }

    /// Parse @vize-map comments from generated virtual TS code.
    /// Returns a vector where index is line number and value is source mapping.
    pub(in crate::ide::diagnostics) fn parse_vize_map_comments(
        code: &str,
    ) -> Vec<Option<SourceMapping>> {
        let mut mappings: Vec<Option<SourceMapping>> = vec![None; code.lines().count()];
        let mut found_count = 0;

        // Parse @vize-map comments without regex
        // Format: // @vize-map: TYPE -> START:END
        for (line_idx, line) in code.lines().enumerate() {
            // Find @vize-map comment
            if let Some(map_idx) = line.find("@vize-map:") {
                // Extract the part after @vize-map:
                let rest = &line[map_idx + "@vize-map:".len()..];

                // Find -> separator
                if let Some(arrow_idx) = rest.find("->") {
                    // Extract START:END part after ->
                    let offsets_part = rest[arrow_idx + 2..].trim();

                    // Parse START:END
                    if let Some(colon_idx) = offsets_part.find(':') {
                        let start_str = offsets_part[..colon_idx].trim();
                        let end_str = offsets_part[colon_idx + 1..].trim();

                        // Remove any trailing non-digit characters
                        let end_str = end_str
                            .chars()
                            .take_while(|c| c.is_ascii_digit())
                            .collect::<String>();

                        if let (Ok(start_val), Ok(end_val)) =
                            (start_str.parse::<u32>(), end_str.parse::<u32>())
                        {
                            // The mapping applies to the line BEFORE the comment
                            // (the actual code that will produce the error)
                            if line_idx > 0 {
                                mappings[line_idx - 1] = Some(SourceMapping {
                                    start: start_val,
                                    end: end_val,
                                });
                                found_count += 1;
                                tracing::debug!(
                                    "vize-map: line {} -> offset {}:{} (from: {})",
                                    line_idx - 1,
                                    start_val,
                                    end_val,
                                    &line[..line.len().min(80)]
                                );
                            }
                        }
                    }
                }
            }
        }

        tracing::info!("parse_vize_map_comments: found {} mappings", found_count);
        mappings
    }
}

pub(in crate::ide) fn semantic_links_after_import_rewrite(
    links: Vec<vize_canon::virtual_ts::VizeSemanticLink>,
    import_source_map: &ImportSourceMap,
) -> Vec<vize_canon::virtual_ts::VizeSemanticLink> {
    links
        .into_iter()
        .map(|mut link| {
            link.source_range = rewrite_range(link.source_range, import_source_map);
            link.target_range = rewrite_range(link.target_range, import_source_map);
            link
        })
        .collect()
}

/// Map a pre-rewrite generated TypeScript range into the post-rewrite code
/// held by [`VirtualTsResult::code`].
fn rewrite_range(
    range: std::ops::Range<usize>,
    import_source_map: &ImportSourceMap,
) -> std::ops::Range<usize> {
    import_source_map.get_virtual_offset(range.start as u32) as usize
        ..import_source_map.get_virtual_offset(range.end as u32) as usize
}
