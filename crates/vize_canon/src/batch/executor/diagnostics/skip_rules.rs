//! Code/message-level suppression rules shared by the LSP and CLI diagnostic
//! paths. These decide whether a TypeScript diagnostic is reportable at all,
//! independent of where it maps back to in the original source.

use std::{
    fs,
    path::{Path, PathBuf},
};

use super::{Diagnostic, OriginalPosition};
use vize_carton::FxHashSet;

type DiagnosticLineKey = (PathBuf, u32);

pub(crate) fn should_skip_diagnostic(code: Option<u32>, message: &str) -> bool {
    match code {
        // TS2666: virtual-TS generation injects helper bindings that can trip
        // this code outside the user's source — suppress to match vue-tsc.
        Some(2666) => true,
        // Native TypeScript currently exposes Node Buffer backing stores as
        // `ArrayBuffer | SharedArrayBuffer`, while projects pinned to older
        // TypeScript/@types/node combinations accepted `buffer.slice(...)` as
        // `ArrayBuffer`. Keep vize aligned with that project baseline until the
        // native checker can select the project's exact lib surface.
        Some(2322) if is_array_buffer_backing_store_lib_mismatch(message) => true,
        // TS7006/TS7043/TS7044 (noImplicitAny family) are user-facing errors
        // and must surface so `vize check` matches vue-tsc under
        // `noImplicitAny`/`strict`. They were previously suppressed (#966).
        _ => false,
    }
}

fn is_array_buffer_backing_store_lib_mismatch(message: &str) -> bool {
    message
        .contains("Type 'ArrayBuffer | SharedArrayBuffer' is not assignable to type 'ArrayBuffer'")
        && message.contains("SharedArrayBuffer")
}

pub(crate) fn should_skip_original_diagnostic(
    code: Option<u32>,
    original: &OriginalPosition,
) -> bool {
    code == Some(6133) && original.block_type.is_none() && is_vue_source(&original.path)
}

pub(crate) fn filter_authored_diagnostics(diagnostics: Vec<Diagnostic>) -> Vec<Diagnostic> {
    if !diagnostics.iter().any(|diagnostic| {
        is_use_vmodel_passive_false_overload_diagnostic(diagnostic)
            || is_multiline_ts_directive_candidate(diagnostic)
    }) {
        return diagnostics;
    }
    let unused_expect_errors = unused_expect_error_lines(&diagnostics);

    diagnostics
        .into_iter()
        .filter(|diagnostic| !should_skip_authored_diagnostic(diagnostic, &unused_expect_errors))
        .collect()
}

fn is_multiline_ts_directive_candidate(diagnostic: &Diagnostic) -> bool {
    diagnostic.code != Some(2578)
        && diagnostic.block_type != Some(crate::sfc_diagnostics::SfcBlockType::Template)
        && is_vue_source(&diagnostic.file)
}

fn should_skip_authored_diagnostic(
    diagnostic: &Diagnostic,
    unused_expect_errors: &FxHashSet<DiagnosticLineKey>,
) -> bool {
    if is_use_vmodel_passive_false_overload_diagnostic(diagnostic)
        && should_skip_use_vmodel_passive_false_diagnostic(diagnostic, unused_expect_errors)
    {
        return true;
    }

    is_multiline_ts_directive_candidate(diagnostic)
        && is_multiline_ts_directive_suppressed(diagnostic, unused_expect_errors)
}

fn should_skip_use_vmodel_passive_false_diagnostic(
    diagnostic: &Diagnostic,
    unused_expect_errors: &FxHashSet<DiagnosticLineKey>,
) -> bool {
    let Ok(source) = fs::read_to_string(&diagnostic.file) else {
        return false;
    };
    let Some(matched) = use_vmodel_passive_false_match(&source, diagnostic.line as usize) else {
        return false;
    };
    let key = (diagnostic.file.clone(), matched.expect_error_line as u32);
    !unused_expect_errors.contains(&key)
}

fn is_use_vmodel_passive_false_overload_diagnostic(diagnostic: &Diagnostic) -> bool {
    // The CLI parser sees only the headline before continuation lines are
    // attached, so the source context is the stable part of this parity rule.
    diagnostic.code == Some(2769)
        && is_vue_source(&diagnostic.file)
        && diagnostic.message.contains("No overload matches this call")
}

struct UseVModelPassiveFalseMatch {
    expect_error_line: usize,
}

fn use_vmodel_passive_false_match(
    source: &str,
    diagnostic_line: usize,
) -> Option<UseVModelPassiveFalseMatch> {
    let lines: Vec<_> = source.lines().collect();
    let (start, end) = containing_use_vmodel_call(&lines, diagnostic_line)?;
    if !call_has_passive_false(&lines, start, end) {
        return None;
    }
    let expect_error_line = expect_error_before_default_value(&lines, start, end)?;

    Some(UseVModelPassiveFalseMatch { expect_error_line })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TsDirectiveKind {
    Ignore,
    ExpectError,
}

struct MultilineTsDirectiveMatch {
    directive_line: usize,
    kind: TsDirectiveKind,
}

fn is_multiline_ts_directive_suppressed(
    diagnostic: &Diagnostic,
    unused_expect_errors: &FxHashSet<DiagnosticLineKey>,
) -> bool {
    let Ok(source) = fs::read_to_string(&diagnostic.file) else {
        return false;
    };
    let Some(matched) = multiline_ts_directive_match(&source, diagnostic.line as usize) else {
        return false;
    };
    if matched.kind == TsDirectiveKind::ExpectError {
        let key = (diagnostic.file.clone(), matched.directive_line as u32);
        return !unused_expect_errors.contains(&key);
    }
    true
}

fn multiline_ts_directive_match(
    source: &str,
    diagnostic_line: usize,
) -> Option<MultilineTsDirectiveMatch> {
    let lines: Vec<_> = source.lines().collect();
    if diagnostic_line >= lines.len() {
        return None;
    }

    let lower_bound = diagnostic_line.saturating_sub(16);
    for directive_line in (lower_bound..=diagnostic_line).rev() {
        let line = lines[directive_line].trim();
        let Some(kind) = ts_directive_kind(line) else {
            continue;
        };
        let Some(call_start) = next_non_empty_line(&lines, directive_line + 1) else {
            continue;
        };
        let Some(call_end) = call_end_line(&lines, call_start) else {
            continue;
        };
        if diagnostic_line >= call_start && diagnostic_line <= call_end {
            return Some(MultilineTsDirectiveMatch {
                directive_line,
                kind,
            });
        }
    }

    None
}

fn ts_directive_kind(line: &str) -> Option<TsDirectiveKind> {
    if line.contains("@ts-ignore") {
        Some(TsDirectiveKind::Ignore)
    } else if line.contains("@ts-expect-error") {
        Some(TsDirectiveKind::ExpectError)
    } else {
        None
    }
}

fn next_non_empty_line(lines: &[&str], start: usize) -> Option<usize> {
    lines
        .iter()
        .enumerate()
        .skip(start)
        .find_map(|(index, line)| (!line.trim().is_empty()).then_some(index))
}

fn containing_use_vmodel_call(lines: &[&str], diagnostic_line: usize) -> Option<(usize, usize)> {
    if diagnostic_line >= lines.len() {
        return None;
    }
    for start in (0..=diagnostic_line).rev() {
        if !lines[start].contains("useVModel(") {
            continue;
        }
        let Some(end) = call_end_line(lines, start) else {
            continue;
        };
        if diagnostic_line <= end {
            return Some((start, end));
        }
    }
    None
}

fn call_end_line(lines: &[&str], start: usize) -> Option<usize> {
    let mut depth = 0i32;
    let mut saw_open = false;
    for (index, line) in lines.iter().enumerate().skip(start) {
        for ch in line.chars() {
            if ch == '(' {
                saw_open = true;
                depth += 1;
            } else if ch == ')' && saw_open {
                depth -= 1;
            }
        }
        if saw_open && depth == 0 {
            return Some(index);
        }
    }
    None
}

fn call_has_passive_false(lines: &[&str], start: usize, end: usize) -> bool {
    let mut saw_passive = false;
    for line in &lines[start..=end] {
        let line = line.trim();
        if line.contains("passive:") {
            saw_passive = true;
        }
        if saw_passive && line.contains("as false") {
            return true;
        }
    }
    false
}

fn expect_error_before_default_value(lines: &[&str], start: usize, end: usize) -> Option<usize> {
    let mut expect_error_line = None;
    for (index, line) in lines.iter().enumerate().take(end + 1).skip(start) {
        let line = line.trim();
        if line.contains("@ts-expect-error") {
            expect_error_line = Some(index);
        }
        if line.contains("defaultValue:") {
            return expect_error_line;
        }
    }
    None
}

fn unused_expect_error_lines(diagnostics: &[Diagnostic]) -> FxHashSet<DiagnosticLineKey> {
    diagnostics
        .iter()
        .filter(|diagnostic| {
            diagnostic.code == Some(2578)
                && diagnostic
                    .message
                    .contains("Unused '@ts-expect-error' directive")
        })
        .map(|diagnostic| (diagnostic.file.clone(), diagnostic.line))
        .collect()
}

fn is_vue_source(path: &Path) -> bool {
    path.extension().is_some_and(|extension| extension == "vue")
}

#[cfg(test)]
mod tests;
