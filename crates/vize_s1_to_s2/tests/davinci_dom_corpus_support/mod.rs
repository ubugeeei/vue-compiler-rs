mod allowlist;
mod sfc_inputs;

use std::{collections::BTreeMap, fs};

use davinci_test_support::corpus::CorpusSweep;
use sfc_inputs::{binding_table, sfc_bindings, sfc_is_ts};
use vize_atelier_core::options::CodegenMode;
use vize_atelier_dom::errors::ErrorCode;
use vize_atelier_dom::{DomCompilerOptions, compile_template_legacy_with_options};
use vize_atelier_sfc::{SfcParseOptions, parse_sfc};
use vize_s0::Allocator;
use vize_s1_to_s2::{
    DomEmitMode, DomEmitOptions, EmitError, LegacyCaps, emit_dom_source,
    emit_dom_source_with_options,
};

/// Which shipped-lane option surface the comparison runs under.
#[derive(Clone, Copy)]
pub enum Lane {
    /// `compile_template` defaults.
    Default,
    /// `prefix_identifiers: true` on both sides (P2-11 installment 85).
    Prefixed,
    /// Module mode + `prefix_identifiers` + the SFC's own binding metadata
    /// (non-inline) + its own `is_ts` on both sides (P2-11 installments
    /// 86 and 87): the dev-server shape of a `<script setup>` component.
    Bindings,
}

pub use allowlist::old_lane_skip_is_allowed;

#[derive(Default)]
pub struct Report {
    pub files: u64,
    pub unreadable_count: u64,
    pub parsed: u64,
    pub templates: u64,
    pub compared: u64,
    pub old_error_skips: u64,
    pub s2_refusal_count: u64,
    pub divergence_count: u64,
    pub old_error_codes: Vec<ErrorCode>,
    pub old_error_reasons: BTreeMap<String, u64>,
    pub unreadable: Vec<String>,
    pub old_error_samples: Vec<String>,
    pub unexpected_old_error_skips: u64,
    pub unexpected_old_error_samples: Vec<String>,
    pub s2_refusal_reasons: BTreeMap<&'static str, u64>,
    pub s2_refusal_samples: BTreeMap<&'static str, Vec<String>>,
    pub s2_refusals: Vec<String>,
    pub divergences: Vec<String>,
}

pub fn compare_sweep(sweep: &CorpusSweep) -> Report {
    compare_sweep_lane(sweep, Lane::Default)
}

pub fn compare_sweep_lane(sweep: &CorpusSweep, lane: Lane) -> Report {
    let mut report = Report::default();
    for file in &sweep.files {
        let source = match fs::read_to_string(file) {
            Ok(source) => source,
            Err(error) => {
                report.unreadable_count += 1;
                if report.unreadable.len() < 20 {
                    report
                        .unreadable
                        .push(format!("{}: {error}", file.display()));
                }
                continue;
            }
        };
        let context = file.to_string_lossy();
        compare_sfc_template_lane(context.as_ref(), &source, &mut report, lane);
    }
    report
}

/// `extract_component_name`: the filename's stem, `anonymous` when the
/// path has none.
fn component_name_of(path: &str) -> std::string::String {
    std::path::Path::new(path)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("anonymous")
        .to_owned()
}

pub fn compare_sfc_template(name: &str, source: &str, report: &mut Report) {
    compare_sfc_template_lane(name, source, report, Lane::Default)
}

pub fn compare_sfc_template_lane(name: &str, source: &str, report: &mut Report, lane: Lane) {
    report.files += 1;
    let Ok(descriptor) = parse_sfc(source, SfcParseOptions::default()) else {
        return;
    };
    report.parsed += 1;
    let Some(template) = descriptor.template.as_ref() else {
        return;
    };
    report.templates += 1;
    let bindings = match lane {
        Lane::Bindings => match sfc_bindings(&descriptor) {
            Ok(bindings) => bindings,
            Err(reason) => {
                report.old_error_skips += 1;
                *report.old_error_reasons.entry(reason).or_default() += 1;
                return;
            }
        },
        Lane::Default | Lane::Prefixed => None,
    };
    let table = bindings.as_ref().map(binding_table);
    // `compile_template_block` derives `is_ts` from the script blocks' lang.
    let is_ts = matches!(lane, Lane::Bindings) && sfc_is_ts(&descriptor);
    // `extract_component_name`: the SFC filename's stem.
    let component_name = matches!(lane, Lane::Bindings).then(|| component_name_of(name));

    // The **legacy** lane, S2 declined. The ordinary `compile_template*`
    // entry points route through the S2 emitter since the P2-11 production
    // switch, so building the old side on them compares S2 against itself:
    // measured, renaming `Helper::CreateElementVNode`'s alias in the S2
    // emitter left this sweep at `compared=12062 divergences=0`.
    let old_allocator = Allocator::new();
    let (_, errors, old) = match lane {
        Lane::Default => compile_template_legacy_with_options(
            &old_allocator,
            &template.content,
            DomCompilerOptions::default(),
        ),
        Lane::Prefixed => compile_template_legacy_with_options(
            &old_allocator,
            &template.content,
            DomCompilerOptions {
                prefix_identifiers: true,
                ..Default::default()
            },
        ),
        Lane::Bindings => compile_template_legacy_with_options(
            &old_allocator,
            &template.content,
            DomCompilerOptions {
                mode: CodegenMode::Module,
                prefix_identifiers: true,
                is_ts,
                component_name: component_name
                    .as_ref()
                    .map(|name| vize_s0::String::from(name.as_str())),
                binding_metadata: bindings.clone(),
                ..Default::default()
            },
        ),
    };
    let blocking_errors: Vec<_> = errors
        .iter()
        .filter(|error| !error.is_recoverable())
        .collect();
    if !blocking_errors.is_empty() {
        report.old_error_skips += 1;
        for error in &blocking_errors {
            *report
                .old_error_reasons
                .entry(format!("{:?}", error.code))
                .or_default() += 1;
        }
        if report.old_error_codes.len() < 20 {
            report
                .old_error_codes
                .extend(blocking_errors.iter().map(|error| error.code));
        }
        if report.old_error_samples.len() < 20 {
            report.old_error_samples.push(format!(
                "{name}: {} old-lane blocking errors: {blocking_errors:?}",
                blocking_errors.len()
            ));
        }
        let actual_codes = blocking_errors
            .iter()
            .map(|error| format!("{:?}", error.code))
            .collect::<Vec<_>>();
        // Under `prefix_identifiers` the shipped lane parses every expression
        // (JS dialect, no `is_ts`), so TypeScript-syntax templates fail with
        // the non-recoverable `InvalidExpression`; those skips are the lane's
        // own admission boundary, not corpus drift.
        let prefixed_ts_skip = matches!(lane, Lane::Prefixed)
            && actual_codes.iter().all(|code| code == "InvalidExpression");
        if !prefixed_ts_skip && !old_lane_skip_is_allowed(name, &actual_codes) {
            report.unexpected_old_error_skips += 1;
            if report.unexpected_old_error_samples.len() < 20 {
                report.unexpected_old_error_samples.push(format!(
                    "{name}: unexpected old-lane blocking errors: {blocking_errors:?}"
                ));
            }
        }
        return;
    }
    let old = format!("{}\n{}", old.preamble, old.code);

    let new_allocator = Allocator::new();
    let emitted = match lane {
        Lane::Default => emit_dom_source(&new_allocator, &template.content),
        Lane::Prefixed => emit_dom_source_with_options(
            &new_allocator,
            &template.content,
            LegacyCaps::VUE3,
            &DomEmitOptions {
                prefix_identifiers: true,
                ..DomEmitOptions::DEFAULT
            },
        ),
        Lane::Bindings => emit_dom_source_with_options(
            &new_allocator,
            &template.content,
            LegacyCaps::VUE3,
            &DomEmitOptions {
                mode: DomEmitMode::Module,
                prefix_identifiers: true,
                is_ts,
                component_name: component_name.as_deref(),
                bindings: table.as_ref(),
                ..DomEmitOptions::DEFAULT
            },
        ),
    };
    let new = match emitted {
        Ok(emit) => emit.assembled(),
        Err(error) => {
            report.s2_refusal_count += 1;
            let reason = refusal_reason(&error);
            *report.s2_refusal_reasons.entry(reason).or_default() += 1;
            let samples = report.s2_refusal_samples.entry(reason).or_default();
            if samples.len() < 5 {
                samples.push(format!("{name}: {error:?}"));
            }
            if report.s2_refusals.len() < 20 {
                report.s2_refusals.push(format!("{name}: {error:?}"));
            }
            return;
        }
    };

    report.compared += 1;
    if old != new {
        report.divergence_count += 1;
        if report.divergences.len() < 20 {
            report.divergences.push(format!(
                "{name}: old_len={} new_len={} first_diff={} old_window={} new_window={}",
                old.len(),
                new.len(),
                first_diff(&old, &new),
                mismatch_window(&old, &new),
                mismatch_window(&new, &old)
            ));
        }
    }
}

fn refusal_reason(error: &EmitError) -> &'static str {
    error.reason().map_or("diagnostics", |reason| reason.code())
}

fn preview(source: &str) -> String {
    source
        .lines()
        .take(4)
        .collect::<Vec<_>>()
        .join("\\n")
        .chars()
        .take(320)
        .collect()
}

fn first_diff(left: &str, right: &str) -> usize {
    left.as_bytes()
        .iter()
        .zip(right.as_bytes())
        .position(|(left, right)| left != right)
        .unwrap_or_else(|| left.len().min(right.len()))
}

fn mismatch_window(source: &str, other: &str) -> String {
    let diff = first_diff(source, other);
    let start = source[..diff]
        .char_indices()
        .rev()
        .nth(80)
        .map_or(0, |(index, _)| index);
    let end = source[diff..]
        .char_indices()
        .nth(180)
        .map_or(source.len(), |(index, _)| diff + index);
    preview(&source[start..end])
}

pub fn assert_empty(label: &str, values: &[String]) {
    assert!(values.is_empty(), "{label}:\n{}", values.join("\n"));
}

pub fn assert_clean_corpus(report: &Report) {
    assert!(
        report.unreadable_count == 0
            && report.unexpected_old_error_skips == 0
            && report.s2_refusal_count == 0
            && report.divergence_count == 0,
        "corpus unreadable files ({}):\n{}\n\ncorpus old-lane error skips ({}) by reason {:?}:\n{}\n\nunexpected old-lane error skips ({}):\n{}\n\ncorpus S2 refusals ({}) by reason {:?}:\n{}\n\ncorpus divergences ({}):\n{}",
        report.unreadable_count,
        report.unreadable.join("\n"),
        report.old_error_skips,
        report.old_error_reasons,
        report.old_error_samples.join("\n"),
        report.unexpected_old_error_skips,
        report.unexpected_old_error_samples.join("\n"),
        report.s2_refusal_count,
        report.s2_refusal_reasons,
        report.s2_refusals.join("\n"),
        report.divergence_count,
        report.divergences.join("\n"),
    );
}
