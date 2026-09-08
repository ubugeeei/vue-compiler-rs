use super::{filter_authored_diagnostics, use_vmodel_passive_false_match};
use crate::batch::Diagnostic;
use std::{
    fs,
    path::{Path, PathBuf},
};
use tempfile::TempDir;

const PASSIVE_FALSE_OVERLOAD: &str = "No overload matches this call.\n\
The last overload gave the following error.\n\
Type 'false' is not assignable to type 'true'.";

#[test]
fn filters_use_vmodel_passive_false_overload_after_consumed_expect_error() {
    let source = r#"useVModel(props, "modelValue", emit, {
  // @ts-expect-error Missing infer for AcceptableValue
  defaultValue: props.defaultValue ?? (multiple.value ? [] : undefined),
  passive: (props.modelValue === undefined) as false,
  deep: true,
});
"#;
    let temp = TempDir::new().unwrap();
    let file = write_vue(&temp, source);
    let diagnostic = overload(&file, line_of(source, "passive:"));

    assert!(filter_authored_diagnostics(vec![diagnostic]).is_empty());
}

#[test]
fn keeps_use_vmodel_passive_false_overload_without_expect_error() {
    let source = r#"useVModel(props, "modelValue", emit, {
  defaultValue: props.defaultValue,
  passive: (props.modelValue === undefined) as false,
  deep: true,
});
"#;

    assert!(use_vmodel_passive_false_match(source, line_of(source, "passive:") as usize).is_none());
}

#[test]
fn keeps_use_vmodel_overload_when_expect_error_is_unused() {
    let source = r#"useVModel(props, "modelValue", emit, {
  // @ts-expect-error Missing infer for AcceptableValue
  defaultValue: props.defaultValue ?? (multiple.value ? [] : undefined),
  passive: (props.modelValue === undefined) as false,
  deep: true,
});
"#;
    let temp = TempDir::new().unwrap();
    let file = write_vue(&temp, source);
    let diagnostics = filter_authored_diagnostics(vec![
        overload(&file, line_of(source, "passive:")),
        unused_directive(&file, line_of(source, "@ts-expect-error")),
    ]);
    let codes: Vec<_> = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect();

    assert_eq!(codes, [Some(2769), Some(2578)]);
}

#[test]
fn matches_multiline_passive_false_option() {
    let source = r#"useVModel(props, "modelValue", emit, {
  // @ts-expect-error Missing infer for AcceptableValue
  defaultValue: props.defaultValue ?? (multiple.value ? [] : undefined),
  passive:
    (props.modelValue === undefined)
      as false,
  deep: true,
});
"#;
    let matched =
        use_vmodel_passive_false_match(source, line_of(source, "passive:") as usize).unwrap();

    assert_eq!(
        matched.expect_error_line,
        line_of(source, "@ts-expect-error") as usize
    );
}

#[test]
fn matches_use_vmodel_options_after_long_gap() {
    let source = r#"useVModel(props, "modelValue", emit, {
  option01: true,
  option02: true,
  option03: true,
  option04: true,
  option05: true,
  option06: true,
  option07: true,
  option08: true,
  option09: true,
  option10: true,
  option11: true,
  option12: true,
  option13: true,
  // @ts-expect-error Missing infer for AcceptableValue
  defaultValue: props.defaultValue ?? (multiple.value ? [] : undefined),
  passive: (props.modelValue === undefined) as false,
  deep: true,
});
"#;
    let matched =
        use_vmodel_passive_false_match(source, line_of(source, "passive:") as usize).unwrap();

    assert_eq!(
        matched.expect_error_line,
        line_of(source, "@ts-expect-error") as usize
    );
}

#[test]
fn filters_ts_ignore_for_multiline_call_argument_diagnostic() {
    let source = r#"const handler = (event: MouseEvent) => void event
if (handler) {
  // @ts-ignore DOM overload is intentionally wider here
  ;(el as HTMLElement).addEventListener(
    'click',
    handler,
    false
  )
}
"#;
    let temp = TempDir::new().unwrap();
    let file = write_vue(&temp, source);
    let diagnostic = overload(&file, line_of(source, "handler,"));

    assert!(filter_authored_diagnostics(vec![diagnostic]).is_empty());
}

#[test]
fn keeps_multiline_call_diagnostic_when_ts_expect_error_is_unused() {
    let source = r#"const handler = (event: MouseEvent) => void event
if (handler) {
  // @ts-expect-error DOM overload is intentionally wider here
  el.addEventListener(
    'click',
    handler,
    false
  )
}
"#;
    let temp = TempDir::new().unwrap();
    let file = write_vue(&temp, source);
    let diagnostics = filter_authored_diagnostics(vec![
        overload(&file, line_of(source, "handler,")),
        unused_directive(&file, line_of(source, "@ts-expect-error")),
    ]);
    let codes: Vec<_> = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect();

    assert_eq!(codes, [Some(2769), Some(2578)]);
}

fn write_vue(temp: &TempDir, source: &str) -> PathBuf {
    let file = temp.path().join("Foo.vue");
    fs::write(&file, source).unwrap();
    file
}

fn overload(file: &Path, line: u32) -> Diagnostic {
    diagnostic(file, line, Some(2769), PASSIVE_FALSE_OVERLOAD)
}

fn unused_directive(file: &Path, line: u32) -> Diagnostic {
    diagnostic(
        file,
        line,
        Some(2578),
        "Unused '@ts-expect-error' directive.",
    )
}

fn diagnostic(file: &Path, line: u32, code: Option<u32>, message: &str) -> Diagnostic {
    Diagnostic {
        file: file.to_path_buf(),
        line,
        column: 2,
        message: message.into(),
        code,
        severity: 1,
        block_type: None,
    }
}

fn line_of(source: &str, needle: &str) -> u32 {
    source
        .lines()
        .position(|line| line.contains(needle))
        .unwrap() as u32
}
