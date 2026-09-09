use super::generate_virtual_ts_with_offsets;
use vize_croquis::{Analyzer, AnalyzerOptions};

const TEMPLATE: &str =
    r#"<el-badge><template #content="{ value }">{{ value.missing }}</template></el-badge>"#;

/// Generate the embedded-preamble virtual TS for a template with no script.
fn generate(template: &str) -> vize_carton::String {
    let allocator = vize_carton::Allocator::new();
    let (root, _) = vize_armature::parse(&allocator, template);
    let mut analyzer = Analyzer::with_options(AnalyzerOptions::full());
    analyzer.analyze_template(&root);
    let summary = analyzer.finish();
    generate_virtual_ts_with_offsets(&summary, None, Some(&root), 0, 0, &Default::default()).code
}

/// The slot-payload aliases are roots that nothing else in the preamble
/// reaches, so a module-scope copy in a component that never annotates a slot
/// scope from them is dead code — `TS6196` for a `noUnusedLocals` consumer, and
/// what the exact-tsgo project gate reports. Each is declared exactly when the
/// document references it, and never otherwise.
#[test]
fn slot_payload_helpers_are_declared_only_where_they_are_referenced() {
    let slot_free = generate(r#"<div v-for="item in items">{{ item }}</div>"#);
    for alias in [
        "type __VizeStructuralSlots<",
        "type __VizeSlotsResolver<",
        "type __VizeSlotPayload<",
        "type __VizeAnySlotPayload<",
    ] {
        assert!(
            !slot_free.contains(alias),
            "a component with no v-slot scope must not declare {alias}:\n{slot_free}"
        );
    }

    let static_name = generate(TEMPLATE);
    assert!(
        static_name.contains("type __VizeSlotsResolver<"),
        "{static_name}"
    );
    assert!(
        static_name.contains("type __VizeSlotPayload<"),
        "{static_name}"
    );
    assert!(
        !static_name.contains("type __VizeAnySlotPayload<"),
        "a statically named slot must not declare the dynamic-name alias:\n{static_name}"
    );

    let dynamic_name = generate(
        r#"<el-badge><template #[slotName]="{ value }">{{ value }}</template></el-badge>"#,
    );
    assert!(
        dynamic_name.contains("type __VizeSlotsResolver<"),
        "{dynamic_name}"
    );
    assert!(
        dynamic_name.contains("type __VizeAnySlotPayload<"),
        "{dynamic_name}"
    );
    assert!(
        dynamic_name.contains("[__K in keyof __S]-?:"),
        "dynamic payload extraction must not preserve optional slot markers:\n{dynamic_name}"
    );
    assert!(
        !dynamic_name.contains("type __VizeSlotPayload<"),
        "a dynamically named slot must not declare the static-name alias:\n{dynamic_name}"
    );
}

#[test]
fn kebab_case_slot_host_uses_pascal_case_setup_binding() {
    let script = r#"import { ElBadge } from 'element-plus'"#;
    let allocator = vize_carton::Allocator::new();
    let (root, _) = vize_armature::parse(&allocator, TEMPLATE);
    let mut analyzer = Analyzer::with_options(AnalyzerOptions::full());
    analyzer.analyze_script_setup(script);
    analyzer.analyze_template(&root);
    let summary = analyzer.finish();

    let output = generate_virtual_ts_with_offsets(
        &summary,
        Some(script),
        Some(&root),
        0,
        0,
        &Default::default(),
    );

    assert_eq!(
        output
            .code
            .matches("__VizeSlotsResolver<typeof ElBadge>")
            .count(),
        1,
        "{}",
        output.code,
    );
    assert!(
        !output.code.contains("__VizeSlotsResolver<typeof el_badge>"),
        "{}",
        output.code,
    );
    assert!(
        !output.code.contains("declare const el_badge:"),
        "{}",
        output.code
    );
}

#[test]
fn kebab_case_slot_host_uses_ambient_pascal_global_component() {
    let allocator = vize_carton::Allocator::new();
    let (root, _) = vize_armature::parse(&allocator, TEMPLATE);
    let mut analyzer = Analyzer::with_options(AnalyzerOptions::full());
    analyzer.analyze_template(&root);
    let summary = analyzer.finish();

    let options = super::VirtualTsOptions {
        external_template_bindings: vec!["ElBadge".into()],
        ..Default::default()
    };
    let output = generate_virtual_ts_with_offsets(&summary, None, Some(&root), 0, 0, &options);

    assert_eq!(
        output
            .code
            .matches("__VizeSlotsResolver<typeof ElBadge>")
            .count(),
        2,
        "{}",
        output.code,
    );
    assert!(
        !output.code.contains("declare const ElBadge:"),
        "{}",
        output.code
    );
    assert!(
        !output
            .code
            .contains("const el_badge: any = undefined as any;"),
        "{}",
        output.code,
    );
    assert!(
        !output.code.contains("declare const el_badge:"),
        "{}",
        output.code
    );
}

#[test]
fn type_only_local_component_import_keeps_ambient_global_component_value() {
    let script = r#"import type { ElBadge } from '#components'
type BadgeInstance = typeof ElBadge
"#;
    let allocator = vize_carton::Allocator::new();
    let (root, _) = vize_armature::parse(&allocator, TEMPLATE);
    let mut analyzer = Analyzer::with_options(AnalyzerOptions::full());
    analyzer.analyze_script_setup(script);
    analyzer.analyze_template(&root);
    let summary = analyzer.finish();

    let options = super::VirtualTsOptions {
        external_template_bindings: vec!["ElBadge".into()],
        ..Default::default()
    };
    let output =
        generate_virtual_ts_with_offsets(&summary, Some(script), Some(&root), 0, 0, &options);

    assert!(
        output.code.contains(
            "declare const __VizeComponent_el_badge: import(\"vue\").GlobalComponents extends { \"el-badge\": infer __C } ? __C : import(\"vue\").GlobalComponents extends { \"ElBadge\": infer __C } ? __C : any;"
        ),
        "{}",
        output.code,
    );
    assert!(
        !output.code.contains("declare const ElBadge:"),
        "{}",
        output.code,
    );
    assert_eq!(
        output
            .code
            .matches("__VizeSlotsResolver<typeof __VizeComponent_el_badge>")
            .count(),
        2,
        "{}",
        output.code,
    );
    assert!(
        !output
            .code
            .contains("const el_badge: any = undefined as any;"),
        "{}",
        output.code,
    );
}

#[test]
fn unresolved_slot_host_uses_vue_global_components_fallback() {
    let allocator = vize_carton::Allocator::new();
    let (root, _) = vize_armature::parse(&allocator, TEMPLATE);
    let mut analyzer = Analyzer::with_options(AnalyzerOptions::full());
    analyzer.analyze_template(&root);
    let summary = analyzer.finish();

    let output =
        generate_virtual_ts_with_offsets(&summary, None, Some(&root), 0, 0, &Default::default());

    assert!(
        output.code.contains(
            "declare const el_badge: import(\"vue\").GlobalComponents extends { \"el-badge\": infer __C } ? __C : import(\"vue\").GlobalComponents extends { \"ElBadge\": infer __C } ? __C : any;"
        ),
        "{}",
        output.code,
    );
    assert_eq!(
        output
            .code
            .matches("__VizeSlotsResolver<typeof el_badge>")
            .count(),
        1,
        "{}",
        output.code,
    );
}

#[test]
fn editor_reference_paths_are_escaped_and_deduplicated() {
    let summary = Analyzer::with_options(AnalyzerOptions::full()).finish();
    let options = super::VirtualTsOptions {
        reference_paths: vec![
            "/tmp/app&docs/components.d.ts".into(),
            "/tmp/app&docs/components.d.ts".into(),
            "invalid\npath.d.ts".into(),
        ],
        ..Default::default()
    };

    let output = generate_virtual_ts_with_offsets(&summary, None, None, 0, 0, &options);

    assert_eq!(
        output
            .code
            .matches("/// <reference path=\"/tmp/app&amp;docs/components.d.ts\" />")
            .count(),
        1,
        "{}",
        output.code,
    );
    assert!(!output.code.contains("invalid"), "{}", output.code);
}
