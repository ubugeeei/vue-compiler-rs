//! Shared S2 vs shipped DOM-lane differential harness.

#![allow(
    dead_code,
    clippy::disallowed_macros,
    clippy::disallowed_types,
    clippy::disallowed_methods
)]

pub mod battery;
pub mod bindings;
pub mod profile;

use vize_atelier_core::options::{CodegenOptions, TemplateSyntaxMode};
use vize_atelier_dom::{
    DomCompilerOptions, compile_template, compile_template_with_options,
    compile_template_with_template_syntax_and_codegen_options,
};
use vize_s0::Allocator;
use vize_s0::config::VueVersion;
use vize_s1_to_s2::{
    DomEmitOptions, EmitError, LegacyCaps, UnsupportedReason, emit_dom_source,
    emit_dom_source_with_caps, emit_dom_source_with_options,
};

#[derive(Clone, Copy)]
#[allow(dead_code)]
pub enum ExpectedRefusal {
    Diagnostics,
    Unsupported(UnsupportedReason),
}

pub fn shipped(src: &str) -> String {
    shipped_with_dialect(src, VueVersion::V3)
}

pub fn shipped_with_dialect(src: &str, dialect: VueVersion) -> String {
    shipped_with_dialect_and_prefix(src, dialect, false)
}

pub fn shipped_prefixed_with_dialect(src: &str, dialect: VueVersion) -> String {
    shipped_with_dialect_and_prefix(src, dialect, true)
}

fn shipped_with_dialect_and_prefix(
    src: &str,
    dialect: VueVersion,
    prefix_identifiers: bool,
) -> String {
    let allocator = Allocator::new();
    let options = DomCompilerOptions {
        dialect,
        prefix_identifiers,
        ..Default::default()
    };
    let (_, errors, result) = if dialect == VueVersion::V3 && !prefix_identifiers {
        compile_template(&allocator, src)
    } else {
        compile_template_with_options(&allocator, src, options)
    };
    assert!(errors.is_empty(), "shipped lane errors: {errors:?}");
    format!("{}\n{}", result.preamble, result.code)
}

/// The shipped lane under explicit DOM and codegen options — the entry
/// `vize_atelier_sfc` compiles template blocks through.
pub fn shipped_with_options(
    src: &str,
    options: &DomCompilerOptions,
    codegen: &CodegenOptions,
) -> String {
    let allocator = Allocator::new();
    let (_, errors, result) = compile_template_with_template_syntax_and_codegen_options(
        &allocator,
        src,
        options.clone(),
        TemplateSyntaxMode::Standard,
        codegen.clone(),
    );
    assert!(errors.is_empty(), "shipped lane errors: {errors:?}");
    format!("{}\n{}", result.preamble, result.code)
}

/// Dual-run under explicit options on both sides: the shipped lane's
/// `DomCompilerOptions` + `CodegenOptions` against the S2 emitter's
/// `DomEmitOptions`, byte-for-byte, with the comparison count pinned.
pub fn assert_s2_matches_shipped_with_options(
    battery: &[(&str, &str)],
    options: &DomCompilerOptions,
    codegen: &CodegenOptions,
    emit: &DomEmitOptions<'_>,
) {
    let mut compared = 0u64;
    let allocator = Allocator::new();
    let caps = LegacyCaps::for_version(options.dialect);
    for (name, src) in battery {
        let old = shipped_with_options(src, options, codegen);
        let new = emit_dom_source_with_options(&allocator, src, caps, emit)
            .unwrap_or_else(|error| panic!("{name}: S2 emit refused: {error:?}"))
            .assembled();
        assert_eq!(
            old.as_str(),
            new.as_str(),
            "{name}: S2 DOM emit diverged from the shipped lane"
        );
        compared += 1;
    }
    assert_eq!(
        compared,
        battery.len() as u64,
        "the S2 dual-run must remain armed"
    );
}

pub fn assert_s2_matches_shipped(battery: &[(&str, &str)]) {
    let mut compared = 0u64;
    let allocator = Allocator::new();
    for (name, src) in battery {
        let old = shipped(src);
        let new = emit_dom_source(&allocator, src)
            .unwrap_or_else(|error| panic!("{name}: S2 emit refused: {error:?}"))
            .assembled();
        assert_eq!(
            old.as_str(),
            new.as_str(),
            "{name}: S2 DOM emit diverged from the shipped lane"
        );
        compared += 1;
    }
    assert_eq!(
        compared,
        battery.len() as u64,
        "the S2 dual-run must remain armed"
    );
}

pub fn assert_s2_matches_shipped_with_dialect(battery: &[(&str, &str)], dialect: VueVersion) {
    assert_s2_matches_shipped_with_dialect_inner(battery, dialect, false)
}

pub fn assert_s2_matches_prefixed_shipped_literals_with_dialect(
    battery: &[(&str, &str)],
    dialect: VueVersion,
) {
    assert_s2_matches_shipped_with_dialect_inner(battery, dialect, true)
}

pub fn patch_sites(source: &str) -> Vec<String> {
    let bytes = source.as_bytes();
    let mut sites = Vec::new();
    let mut cursor = 0usize;
    while let Some(comment_rel) = source[cursor..].find(" /* ") {
        let comment_start = cursor + comment_rel;
        let Some(comment_end_rel) = source[comment_start..].find(" */") else {
            break;
        };
        let comment_end = comment_start + comment_end_rel + " */".len();
        let Some(number_start) = flag_number_start(bytes, comment_start) else {
            cursor = comment_end;
            continue;
        };
        let mut site_end = comment_end;
        if let Some(array_end) = dynamic_props_array_end(source, comment_end) {
            site_end = array_end;
        }
        sites.push(source[number_start..site_end].trim().to_string());
        cursor = site_end;
    }
    sites
}

fn flag_number_start(bytes: &[u8], comment_start: usize) -> Option<usize> {
    let mut index = comment_start;
    while index > 0 && bytes[index - 1].is_ascii_whitespace() {
        index -= 1;
    }
    if index == 0 || !bytes[index - 1].is_ascii_digit() {
        return None;
    }
    while index > 0 && bytes[index - 1].is_ascii_digit() {
        index -= 1;
    }
    if index > 0 && bytes[index - 1] == b'-' {
        index -= 1;
    }
    Some(index)
}

fn dynamic_props_array_end(source: &str, comment_end: usize) -> Option<usize> {
    let tail = &source[comment_end..];
    let trimmed = tail.trim_start();
    if !trimmed.starts_with(", [") {
        return None;
    }
    let offset = tail.len() - trimmed.len();
    let array_start = comment_end + offset + ", ".len();
    let array_tail = &source[array_start..];
    Some(array_start + array_tail.find(']')? + 1)
}

fn assert_s2_matches_shipped_with_dialect_inner(
    battery: &[(&str, &str)],
    dialect: VueVersion,
    prefix_identifiers: bool,
) {
    let mut compared = 0u64;
    let allocator = Allocator::new();
    let caps = LegacyCaps::for_version(dialect);
    for (name, src) in battery {
        let old = shipped_with_dialect_and_prefix(src, dialect, prefix_identifiers);
        let new = emit_dom_source_with_caps(&allocator, src, caps)
            .unwrap_or_else(|error| panic!("{name}: S2 emit refused: {error:?}"))
            .assembled();
        assert_eq!(
            old.as_str(),
            new.as_str(),
            "{name}: S2 DOM emit diverged from the shipped lane"
        );
        compared += 1;
    }
    assert_eq!(
        compared,
        battery.len() as u64,
        "the S2 dual-run must remain armed"
    );
}

#[allow(dead_code)]
pub fn assert_s2_refuses(battery: &[(&str, &str, ExpectedRefusal)]) {
    let allocator = Allocator::new();
    for (name, src, expected) in battery {
        let error = emit_dom_source(&allocator, src)
            .map(|emit| emit.assembled())
            .expect_err(name);
        match expected {
            ExpectedRefusal::Diagnostics => assert_eq!(
                error,
                EmitError::Diagnostics,
                "{name}: S2 DOM refused with the wrong reason"
            ),
            ExpectedRefusal::Unsupported(reason) => assert_eq!(
                error.reason(),
                Some(*reason),
                "{name}: S2 DOM refused with the wrong reason: {error:?}"
            ),
        }
    }
}
