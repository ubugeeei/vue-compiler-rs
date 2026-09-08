//! TypeScript content-mapper projection for Vue single-file components.

use std::ops::Range;
use std::path::Path;

use vize_atelier_core::TemplateSyntaxMode;
use vize_atelier_sfc::{SfcError, SfcParseOptions, parse_sfc};
use vize_s0::{ToCompactString, config::VueVersion};

use crate::batch::Diagnostic;
use crate::batch::error::CorsaResult;
use crate::virtual_ts::{VirtualTsCheckOptions, VirtualTsOptions, VizeMapping};

#[path = "content_mapper_alias.rs"]
mod alias;
use alias::{is_alias_projection, is_synthetic_content_mapper_identifier};

#[path = "content_mapper_protocol.rs"]
mod protocol;
use protocol::protocol_semantic_links;
pub use protocol::{
    CONTENT_MAPPER_GENERATED_DIAGNOSTIC_CODE, CONTENT_MAPPER_SFC_PARSE_ERROR_CODE,
    CONTENT_MAPPER_VIRTUAL_EXTENSION, ContentMapperDiagnostic, ContentMapperDiagnosticDirective,
    ContentMapperDiagnosticDirectives, ContentMapperSemanticLink, ContentMapperSpan,
    ContentMapperTransform, ContentMapperUnusedExpectDiagnostic,
};

#[path = "content_mapper_directives.rs"]
mod directives;
use directives::template_diagnostic_directives;

#[path = "content_mapper_component_name.rs"]
mod component_name;
use component_name::content_mapper_component_name;

use super::build::{
    descriptor_uses_jsx_script, prepend_vue_jsx_reference, virtual_ts_options_for_descriptor,
};
use super::diagnostics::invalid_sfc_fallback_virtual_ts;
use super::vue_codegen::{GeneratedVueFile, VueCodegenOptions, generate_vue_virtual_ts};

#[path = "content_mapper_span_features.rs"]
mod span_features;
#[path = "content_mapper_span_normalize.rs"]
mod span_normalize;

use span_features::content_mapper_span_features;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
enum ContentMapperSpanKind {
    Verbatim = 0,
    Atom = 1,
    Alias = 2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ContentMapperTransformOptions {
    options_api: bool,
    preserve_unused_diagnostics: bool,
}

impl Default for ContentMapperTransformOptions {
    fn default() -> Self {
        Self {
            options_api: true,
            preserve_unused_diagnostics: false,
        }
    }
}

impl ContentMapperTransformOptions {
    /// Enable or disable Vue Options API instance bindings.
    #[must_use]
    pub const fn with_options_api(mut self, enabled: bool) -> Self {
        self.options_api = enabled;
        self
    }

    /// Whether Vue Options API instance bindings are enabled.
    #[must_use]
    pub const fn options_api(self) -> bool {
        self.options_api
    }

    /// Preserve diagnostics for user-authored unused locals.
    #[must_use]
    pub const fn with_preserve_unused_diagnostics(mut self, enabled: bool) -> Self {
        self.preserve_unused_diagnostics = enabled;
        self
    }
}

/// Generate self-contained TypeScript and protocol-v1 mappings for one Vue SFC.
///
/// Authored parse failures are returned as mapper diagnostics with a safe
/// fallback module. They are not transport errors, so TypeScript can keep the
/// mapper process alive while the user edits an invalid document.
pub fn generate_vue_content_mapper_transform(
    path: &Path,
    content: &str,
) -> CorsaResult<ContentMapperTransform> {
    generate_vue_content_mapper_transform_with_options(
        path,
        content,
        ContentMapperTransformOptions::default(),
    )
}

/// Generate a content-mapper transform with Vize-specific mapper settings.
pub fn generate_vue_content_mapper_transform_with_options(
    path: &Path,
    content: &str,
    transform_options: ContentMapperTransformOptions,
) -> CorsaResult<ContentMapperTransform> {
    let descriptor = match parse_sfc(
        content,
        SfcParseOptions {
            filename: path.to_string_lossy().to_compact_string(),
            ..Default::default()
        },
    ) {
        Ok(descriptor) => descriptor,
        Err(error) => {
            return Ok(ContentMapperTransform {
                text: invalid_sfc_fallback_virtual_ts(),
                extension: CONTENT_MAPPER_VIRTUAL_EXTENSION,
                mappings: Vec::new(),
                semantic_links: Vec::new(),
                diagnostic_directives: None,
                diagnostics: vec![sfc_parse_diagnostic(content, &error)],
            });
        }
    };

    let options = virtual_ts_options_for_descriptor(&VirtualTsOptions::default(), &descriptor);
    let use_tsx = descriptor_uses_jsx_script(&descriptor);
    let component_name = content_mapper_component_name(path);
    let GeneratedVueFile {
        mut code,
        mut mappings,
        mut semantic_links,
        diagnostics,
    } = generate_vue_virtual_ts(
        path,
        content,
        &descriptor,
        &options,
        VueCodegenOptions {
            check_options: VirtualTsCheckOptions::default(),
            preserve_unused_diagnostics: transform_options.preserve_unused_diagnostics,
            options_api: transform_options.options_api(),
            preserve_authored_component: true,
            component_name: Some(component_name.as_str()),
            preserve_event_navigation: true,
            legacy_vue2: false,
            dialect: VueVersion::default(),
            template_syntax: TemplateSyntaxMode::default(),
            experimental_in_tag_comments: false,
            hoist_shared_preamble: false,
            omit_vite_client_reference: true,
            runtime_prop_resolve_cache: None,
        },
    )?;

    if use_tsx {
        prepend_vue_jsx_reference(&mut code, &mut mappings, &mut semantic_links);
    }

    let spans = protocol_spans(content, &code, &mappings);
    Ok(ContentMapperTransform {
        extension: CONTENT_MAPPER_VIRTUAL_EXTENSION,
        diagnostic_directives: template_diagnostic_directives(
            content,
            descriptor.template.as_ref(),
            &spans,
        ),
        mappings: spans,
        semantic_links: protocol_semantic_links(&semantic_links),
        diagnostics: diagnostics
            .iter()
            .map(|diagnostic| generated_diagnostic(content, diagnostic))
            .collect(),
        text: code,
    })
}

fn sfc_parse_diagnostic(source: &str, error: &SfcError) -> ContentMapperDiagnostic {
    let (start, length) = error
        .loc
        .as_ref()
        .map(|loc| checked_source_span(source, loc.start, loc.end))
        .unwrap_or_else(|| checked_source_span(source, 0, 0));
    ContentMapperDiagnostic {
        message_text: error.message.clone(),
        start,
        length,
        code: CONTENT_MAPPER_SFC_PARSE_ERROR_CODE,
    }
}

fn generated_diagnostic(source: &str, diagnostic: &Diagnostic) -> ContentMapperDiagnostic {
    let index = vize_s0::line_index::LineIndex::new(source);
    let start = index
        .line_col_to_offset(diagnostic.line, diagnostic.column)
        .unwrap_or(0);
    let (start, length) = checked_source_span(source, start, start);
    ContentMapperDiagnostic {
        message_text: diagnostic.message.clone(),
        start,
        length,
        code: CONTENT_MAPPER_GENERATED_DIAGNOSTIC_CODE,
    }
}

fn checked_source_span(source: &str, start: usize, end: usize) -> (usize, usize) {
    let start = start.min(source.len());
    let start = source.floor_char_boundary(start);
    let end = end.min(source.len()).max(start);
    let end = source.ceil_char_boundary(end);
    let default_length = source[start..].chars().next().map_or(0, char::len_utf8);
    (start, end.saturating_sub(start).max(default_length))
}

#[derive(Clone, Debug)]
struct SpanCandidate {
    generated: Range<usize>,
    original: Range<usize>,
    kind: ContentMapperSpanKind,
}

fn protocol_spans(
    source: &str,
    generated: &str,
    mappings: &[VizeMapping],
) -> Vec<ContentMapperSpan> {
    let candidates = mappings
        .iter()
        .filter_map(|mapping| {
            if mapping.sub_spans.is_empty() {
                Some(vec![candidate(
                    source,
                    generated,
                    mapping.gen_range.clone(),
                    mapping.src_range.clone(),
                )?])
            } else {
                Some(
                    mapping
                        .sub_spans
                        .iter()
                        .filter_map(|span| {
                            candidate(
                                source,
                                generated,
                                span.gen_range.clone(),
                                span.src_range.clone(),
                            )
                        })
                        .collect(),
                )
            }
        })
        .flatten()
        .collect::<Vec<_>>();

    let mut accepted = span_normalize::normalize(candidates);
    accepted.sort_by_key(|candidate| candidate.generated.start);
    accepted
        .into_iter()
        .map(|candidate| {
            let features =
                content_mapper_span_features(generated, candidate.generated.start, candidate.kind);
            ContentMapperSpan([
                candidate.generated.start,
                candidate.generated.len(),
                candidate.original.start,
                candidate.original.len(),
                candidate.kind as usize,
                features,
            ])
        })
        .collect()
}

fn candidate(
    source: &str,
    generated: &str,
    generated_range: Range<usize>,
    original_range: Range<usize>,
) -> Option<SpanCandidate> {
    if generated_range.is_empty()
        || original_range.is_empty()
        || generated_range.end > generated.len()
        || original_range.end > source.len()
        || !generated.is_char_boundary(generated_range.start)
        || !generated.is_char_boundary(generated_range.end)
        || !source.is_char_boundary(original_range.start)
        || !source.is_char_boundary(original_range.end)
    {
        return None;
    }

    let generated_text = &generated[generated_range.clone()];
    let original_text = &source[original_range.clone()];
    if !is_synthetic_content_mapper_identifier(generated_text)
        && let Some(relative_start) = generated_text.find(original_text)
    {
        let start = generated_range.start + relative_start;
        return Some(SpanCandidate {
            generated: start..start + original_text.len(),
            original: original_range,
            kind: ContentMapperSpanKind::Verbatim,
        });
    }

    Some(SpanCandidate {
        generated: generated_range,
        original: original_range,
        kind: if is_alias_projection(generated_text, original_text) {
            ContentMapperSpanKind::Alias
        } else {
            ContentMapperSpanKind::Atom
        },
    })
}

fn ranges_overlap(left: &Range<usize>, right: &Range<usize>) -> bool {
    left.start < right.end && right.start < left.end
}

#[cfg(test)]
#[path = "content_mapper_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "content_mapper_directive_tests.rs"]
mod directive_tests;
