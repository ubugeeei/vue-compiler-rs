//! NAPI bindings for Vue SFC linting.
//!
//! Provides the `lint` function for linting Vue SFC files
//! with native multithreading and .gitignore awareness.
//!
//! FFI boundary code: uses std types for JavaScript interop.
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_methods,
    clippy::disallowed_macros
)]

use napi::bindgen_prelude::{Error, Result, Status};
use napi_derive::napi;
use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use serde_json::{Value, json};
use std::sync::atomic::{AtomicUsize, Ordering};
use vize_s0::append;

use super::lint_fix::{lint_file_with_optional_fix, lint_source};
mod empty_result;
mod file_collection;
mod lint_options;
mod rule_metadata;
use lint_options::{
    LintOptionsNapi, LintResultNapi, PatinaLintOptionsNapi, configure_patina_rule_options,
    configure_type_aware_lint, create_patina_linter, patina_help_level_from_option,
    patina_locale_from_option, patina_preset_from_option,
};
use rule_metadata::collect_patina_rule_metadata;

fn create_position_object(line: u32, column: u32, offset: u32) -> Value {
    json!({
        "line": line,
        "column": column,
        "offset": offset,
    })
}

fn create_location_object(
    start_line: u32,
    start_column: u32,
    start_offset: u32,
    end_line: u32,
    end_column: u32,
    end_offset: u32,
) -> Value {
    json!({
        "start": create_position_object(start_line, start_column, start_offset),
        "end": create_position_object(end_line, end_column, end_offset),
    })
}

/// Lint a single Vue SFC with Patina and return structured diagnostics.
#[napi(js_name = "lintPatinaSfc")]
pub fn lint_patina_sfc(source: String, options: Option<PatinaLintOptionsNapi>) -> Result<Value> {
    use vize_patina::{LspEmitter, Severity};

    let opts = options.unwrap_or_default();
    let filename = opts.filename.unwrap_or_else(|| "anonymous.vue".to_string());
    let locale = patina_locale_from_option(opts.locale.as_deref());
    let help_level = patina_help_level_from_option(opts.help_level.as_deref());
    let preset = patina_preset_from_option(opts.preset.as_deref());
    let component_casing = opts.component_name_in_template_casing.as_deref();
    let event_casing = opts.custom_event_name_casing.as_deref();
    let html_self_closing = opts.html_self_closing;
    let enabled_rules = opts
        .enabled_rules
        .map(|rules| rules.into_iter().map(Into::into).collect());
    let linter = configure_patina_rule_options(
        configure_type_aware_lint(
            create_patina_linter(preset)
                .with_locale(locale)
                .with_help_level(help_level),
            opts.type_aware,
            opts.corsa_path,
        )
        .with_enabled_rules(enabled_rules),
        component_casing,
        event_casing,
        html_self_closing,
    );
    let result = lint_source(&linter, &source, &filename);
    let lsp_diagnostics = LspEmitter::to_lsp_diagnostics_with_source(&result, &source);

    if result.diagnostics.len() != lsp_diagnostics.len() {
        return Err(Error::new(
            Status::GenericFailure,
            "Patina diagnostic conversion produced mismatched location metadata".to_string(),
        ));
    }

    let result_filename: &str = result.filename.as_ref();
    let diagnostics: Vec<_> = result
        .diagnostics
        .iter()
        .zip(lsp_diagnostics.iter())
        .map(|(diagnostic, lsp)| {
            let message: &str = diagnostic.message.as_ref();
            let help = diagnostic
                .help
                .as_ref()
                .map_or(Value::Null, |help| json!(help.as_ref() as &str));

            json!({
                "rule": diagnostic.rule_name,
                "severity": match diagnostic.severity {
                Severity::Error => "error",
                Severity::Warning => "warning",
            },
                "message": message,
                "location": create_location_object(
                    lsp.range.start.line + 1,
                    lsp.range.start.character + 1,
                    diagnostic.start,
                    lsp.range.end.line + 1,
                    lsp.range.end.character + 1,
                    diagnostic.end,
                ),
                "help": help,
            })
        })
        .collect();

    Ok(json!({
        "filename": result_filename,
        "errorCount": result.error_count as u32,
        "warningCount": result.warning_count as u32,
        "diagnostics": diagnostics,
    }))
}

/// Get Patina's currently registered rule metadata.
#[napi(js_name = "getPatinaRules")]
pub fn get_patina_rules() -> Result<Value> {
    let rule_metadata = collect_patina_rule_metadata();
    Ok(json!(
        rule_metadata
            .iter()
            .map(|rule| json!({
                "name": rule.name,
                "description": rule.description,
                "category": rule.category,
                "fixable": rule.fixable,
                "defaultSeverity": rule.default_severity,
                "presets": rule.presets,
            }))
            .collect::<Vec<_>>()
    ))
}

/// Lint Vue SFC files matching patterns (native multithreading, .gitignore-aware)
#[napi]
pub fn lint(patterns: Vec<String>, options: Option<LintOptionsNapi>) -> Result<LintResultNapi> {
    use std::time::Instant;
    use vize_patina::{HelpLevel, OutputFormat, format_results, format_summary};

    let opts = options.unwrap_or_default();
    let start = Instant::now();
    let format = opts
        .format
        .as_deref()
        .and_then(OutputFormat::parse)
        .unwrap_or(OutputFormat::Text);

    let files = file_collection::collect_lint_files(&patterns);

    if files.is_empty() {
        return Ok(LintResultNapi {
            output: empty_result::format_empty_lint_output(&patterns, format),
            error_count: 0,
            warning_count: 0,
            file_count: 0,
            time_ms: start.elapsed().as_secs_f64() * 1000.0,
        });
    }

    let help_level = match opts.help_level.as_deref() {
        Some("none") => HelpLevel::None,
        Some("short") => HelpLevel::Short,
        _ => HelpLevel::Full,
    };
    let preset = patina_preset_from_option(opts.preset.as_deref());
    let linter = configure_type_aware_lint(
        create_patina_linter(preset).with_help_level(help_level),
        opts.type_aware,
        opts.corsa_path,
    );
    let error_count = AtomicUsize::new(0);
    let warning_count = AtomicUsize::new(0);

    // Lint all files in parallel and collect results
    let should_fix = opts.fix.unwrap_or(false);
    let results: Vec<_> = files
        .par_iter()
        .filter_map(|path| {
            let item = lint_file_with_optional_fix(&linter, path, should_fix)?;
            error_count.fetch_add(item.2.error_count, Ordering::Relaxed);
            warning_count.fetch_add(item.2.warning_count, Ordering::Relaxed);
            Some(item)
        })
        .collect();

    let total_errors = error_count.load(Ordering::Relaxed);
    let total_warnings = warning_count.load(Ordering::Relaxed);

    let quiet = opts.quiet.unwrap_or(false);

    // Format output
    let mut output = vize_s0::CompactString::default();
    if format.renders_details_when_quiet() || !quiet || total_errors > 0 || total_warnings > 0 {
        let lint_results: Vec<_> = results.iter().map(|(_, _, r)| r).cloned().collect();
        let sources: Vec<_> = results
            .iter()
            .map(|(f, s, _)| {
                (
                    vize_s0::CompactString::from(f.as_str()),
                    vize_s0::CompactString::from(s.as_str()),
                )
            })
            .collect();

        let formatted = format_results(&lint_results, &sources, format);
        if !formatted.trim().is_empty() {
            output.push_str(&formatted);
        }
    }

    let elapsed = start.elapsed();
    if format == OutputFormat::Text {
        append!(
            output,
            "\n{}\n",
            format_summary(total_errors, total_warnings, files.len())
        );
        append!(output, "Linted {} files in {:.4?}", files.len(), elapsed);
    }

    Ok(LintResultNapi {
        output: output.into(),
        error_count: total_errors as u32,
        warning_count: total_warnings as u32,
        file_count: files.len() as u32,
        time_ms: elapsed.as_secs_f64() * 1000.0,
    })
}

#[cfg(test)]
mod tests;
