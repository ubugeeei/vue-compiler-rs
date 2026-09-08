use std::path::Path;

use vize_canon::{BatchTypeChecker, BatchTypeCheckerTrait, SfcTypeCheckOptions, type_check_sfc};
use vize_s0::{String, ToCompactString};

#[test]
fn unresolved_component_events_accept_multi_argument_handlers() {
    let source = r#"<script setup lang="ts">
import UnknownChild from './UnknownChild.vue'
function handleSelect(key: string, path: string[]) {
  void key
  void path
}
</script>
<template><UnknownChild @select="handleSelect" /></template>
"#;
    let virtual_ts = type_check_sfc(
        source,
        &SfcTypeCheckOptions::new("App.vue").with_virtual_ts(),
    )
    .virtual_ts
    .expect("virtual TypeScript should be generated");
    assert!(
        virtual_ts.contains("? ((...args: any[]) => any) :"),
        "an unresolved component emit must preserve unknown arity:\n{virtual_ts}"
    );

    let project = create_project(&[
        ("src/App.vue", source),
        (
            "src/UnknownChild.vue",
            "<script setup lang=\"ts\"></script>\n<template><div /></template>\n",
        ),
    ]);
    // The listener type annotates the synthetic const, so a rejected handler
    // would surface as an assignment error (`TS2322`) at the declared name
    // rather than the call-argument `TS2345` this used to watch for (#3462).
    assert_no_diagnostic(
        project.path(),
        "App.vue",
        2322,
        "an unresolved component emit must not reject a valid multi-argument handler",
    );
}

#[test]
fn resolved_component_events_still_check_every_argument() {
    let parent = r#"<script setup lang="ts">
import KnownChild from './KnownChild.vue'
function handleSelect(key: number, path: string[]) {
  void key
  void path
}
</script>
<template><KnownChild @select="handleSelect" /></template>
"#;
    let child = r#"<script setup lang="ts">
defineEmits<{ select: [key: string, path: string[]] }>()
</script>
<template><div /></template>
"#;
    let project = create_project(&[("src/App.vue", parent), ("src/KnownChild.vue", child)]);
    // The listener type is an annotation on the synthetic const, so a wrongly
    // shaped handler is an assignment error (`TS2322`) at the declared name,
    // which is what vue-tsc reports at the `@event` attribute (#3462).
    assert_has_diagnostic(
        project.path(),
        "App.vue",
        2322,
        "a resolved component emit must reject a handler with the wrong first argument type",
    );
}

#[test]
fn resolved_component_events_accept_the_exact_handler_tuple() {
    let parent = r#"<script setup lang="ts">
import KnownChild from './KnownChild.vue'
function handleSelect(key: string, path: string[]) {
  void key
  void path
}
</script>
<template><KnownChild @select="handleSelect" /></template>
"#;
    let child = r#"<script setup lang="ts">
defineEmits<{ select: [key: string, path: string[]] }>()
</script>
<template><div /></template>
"#;
    let project = create_project(&[("src/App.vue", parent), ("src/KnownChild.vue", child)]);
    assert_no_diagnostic(
        project.path(),
        "App.vue",
        2322,
        "a resolved component emit must accept its exact handler tuple",
    );
}

#[test]
fn native_events_still_reject_extra_required_arguments() {
    let source = r#"<script setup lang="ts">
function handleClick(event: PointerEvent, required: string) {
  void event
  void required
}
</script>
<template><button @click="handleClick">Click</button></template>
"#;
    let project = create_project(&[("src/App.vue", source)]);
    assert_has_diagnostic(
        project.path(),
        "App.vue",
        2345,
        "a native event must retain its single-event listener contract",
    );
}

#[test]
fn optional_component_event_handler_stays_optional() {
    let project = create_project(&[
        (
            "src/Child.vue",
            r#"<script setup lang="ts">
defineEmits<{ click: [payload: PointerEvent] }>()
</script>
<template><button /></template>
"#,
        ),
        (
            "src/App.vue",
            r#"<script setup lang="ts">
import Child from './Child.vue'

const item = {} as {
  onClick?: (payload: PointerEvent) => Promise<void>
}
</script>
<template><Child @click="item.onClick" /></template>
"#,
        ),
    ]);
    let mut checker = BatchTypeChecker::new(project.path()).unwrap();
    checker.scan_project().unwrap();
    let app = checker
        .virtual_files()
        .into_iter()
        .find(|file| file.original_path.ends_with("App.vue"))
        .expect("App.vue should be registered");
    assert!(
        app.content.contains("| null | undefined = (item.onClick);"),
        "optional event references should remain optional:\n{}",
        app.content
    );
    assert!(
        app.content.contains("if (__vize_handler_"),
        "optional event references should be guarded before invocation:\n{}",
        app.content
    );

    let result = checker.check_project().unwrap();
    let relevant = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.file.ends_with("App.vue"))
        .collect::<Vec<_>>();
    assert!(
        relevant.is_empty(),
        "optional event references should not produce diagnostics: {relevant:#?}"
    );
}

#[test]
fn optional_native_event_handler_stays_optional() {
    let source = r#"<script setup lang="ts">
const item = {} as {
  onClick?: (payload: PointerEvent) => Promise<void>
}
</script>
<template><button @click="item.onClick">Click</button></template>
"#;
    let project = create_project(&[("src/App.vue", source)]);
    let mut checker = BatchTypeChecker::new(project.path()).unwrap();
    checker.scan_project().unwrap();
    let app = checker
        .virtual_files()
        .into_iter()
        .find(|file| file.original_path.ends_with("App.vue"))
        .expect("App.vue should be registered");
    assert!(
        app.content.contains("| null | undefined) => __vize_cb"),
        "optional native event references should remain optional:\n{}",
        app.content
    );
    assert!(
        app.content.contains("if (__vize_handler_"),
        "optional native event references should be guarded before invocation:\n{}",
        app.content
    );

    let result = checker.check_project().unwrap();
    let relevant = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.file.ends_with("App.vue"))
        .collect::<Vec<_>>();
    assert!(
        relevant.is_empty(),
        "optional native event references should not produce diagnostics: {relevant:#?}"
    );
}

#[test]
fn event_handler_mapping_targets_the_user_operand() {
    use vize_croquis::{Analyzer, AnalyzerOptions};

    let script = r#"const __vize_cb = undefined as
  | ((event: PointerEvent) => void)
  | undefined
"#;
    let template = r#"<button @click="__vize_cb">Click</button>"#;

    let allocator = vize_s0::Allocator::new();
    let (root, _) = vize_armature::parse(&allocator, template);

    let mut analyzer = Analyzer::with_options(AnalyzerOptions::full());
    analyzer.analyze_script_setup(script);
    analyzer.analyze_template(&root);
    let summary = analyzer.finish();
    let output =
        vize_canon::virtual_ts::generate_virtual_ts(&summary, Some(script), Some(&root), 0);

    let expression = "__vize_cb";
    let source_start = template.find(expression).unwrap();
    let source_end = source_start + expression.len();
    let mapping = output
        .mappings
        .iter()
        .find(|mapping| mapping.src_range == (source_start..source_end))
        .expect("should map the event handler expression");
    let operand_start = output
        .code
        .find("((__vize_cb));")
        .expect("the user handler should be emitted as the wrapper operand")
        + 2;

    assert_eq!(
        mapping.gen_range,
        operand_start..operand_start + expression.len()
    );
    assert_eq!(&output.code[mapping.gen_range.clone()], expression);
}

fn create_project(files: &[(&str, &str)]) -> tempfile::TempDir {
    let project = tempfile::tempdir().expect("temporary project should be created");
    write_file(
        project.path(),
        "tsconfig.json",
        r#"{
  "compilerOptions": {
    "strict": true,
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "noEmit": true
  },
  "include": ["src/**/*"]
}"#,
    );
    write_file(
        project.path(),
        "node_modules/vue/package.json",
        r#"{ "name": "vue", "types": "index.d.ts" }"#,
    );
    write_file(
        project.path(),
        "node_modules/vue/index.d.ts",
        r#"export interface ComponentPublicInstance {
  $attrs: Record<string, unknown>;
  $slots: Record<string, unknown>;
  $refs: Record<string, unknown>;
  $emit: (...args: unknown[]) => void;
}
"#,
    );
    for (path, source) in files {
        write_file(project.path(), path, source);
    }
    project
}

fn project_diagnostics(root: &Path) -> Vec<(String, Option<u32>, String)> {
    let mut checker = BatchTypeChecker::new(root).expect("type checker should start");
    checker.scan_project().expect("project should scan");
    checker
        .check_project()
        .expect("project should type check")
        .diagnostics
        .into_iter()
        .map(|diagnostic| {
            (
                diagnostic.file.display().to_compact_string(),
                diagnostic.code,
                diagnostic.message,
            )
        })
        .collect()
}

fn assert_no_diagnostic(root: &Path, file: &str, code: u32, reason: &str) {
    let diagnostics = project_diagnostics(root);
    assert!(
        !diagnostics
            .iter()
            .any(|(path, actual, _)| path.ends_with(file) && *actual == Some(code)),
        "{reason}: {diagnostics:#?}"
    );
}

fn assert_has_diagnostic(root: &Path, file: &str, code: u32, reason: &str) {
    let diagnostics = project_diagnostics(root);
    assert!(
        diagnostics
            .iter()
            .any(|(path, actual, _)| path.ends_with(file) && *actual == Some(code)),
        "{reason}: {diagnostics:#?}"
    );
}

fn write_file(root: &Path, path: &str, source: &str) {
    let path = root.join(path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("parent directory should be created");
    }
    std::fs::write(path, source).expect("fixture should be written");
}
