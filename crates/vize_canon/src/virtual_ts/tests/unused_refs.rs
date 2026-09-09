use crate::virtual_ts::{
    VirtualTsGenerationOptions, VirtualTsOptions, generate_virtual_ts_with_offsets_and_checks,
};
use vize_croquis::{Analyzer, AnalyzerOptions};

#[test]
fn test_preserve_unused_diagnostics_does_not_mark_static_template_refs_used() {
    let script = r#"const activatorRef = null
const menuRef = null
const decoy = null
"#;
    let template = r#"<div ref="activatorRef"><div ref="menuRef" /></div>"#;

    let allocator = vize_carton::Allocator::new();
    let (root, _) = vize_armature::parse(&allocator, template);

    let mut analyzer = Analyzer::with_options(AnalyzerOptions::full());
    analyzer.analyze_script_setup(script);
    analyzer.analyze_template(&root);
    let summary = analyzer.finish();

    let output = generate_virtual_ts_with_offsets_and_checks(
        &summary,
        Some(script),
        Some(&root),
        0,
        0,
        &VirtualTsOptions::default(),
        VirtualTsGenerationOptions {
            preserve_unused_diagnostics: true,
            ..Default::default()
        },
    );

    assert!(!output.code.contains("void activatorRef;"));
    assert!(!output.code.contains("void menuRef;"));
    assert!(!output.code.contains("void decoy;"));
}

#[test]
fn test_preserve_unused_diagnostics_does_not_unwrap_static_template_refs() {
    let script = r#"import { ref } from 'vue'
const root = ref(null)
const decoy = null
"#;
    let template = r#"<div ref="root" />"#;

    let allocator = vize_carton::Allocator::new();
    let (root, _) = vize_armature::parse(&allocator, template);

    let mut analyzer = Analyzer::with_options(AnalyzerOptions::full());
    analyzer.analyze_script_setup(script);
    analyzer.analyze_template(&root);
    let summary = analyzer.finish();

    let output = generate_virtual_ts_with_offsets_and_checks(
        &summary,
        Some(script),
        Some(&root),
        0,
        0,
        &VirtualTsOptions::default(),
        VirtualTsGenerationOptions {
            preserve_unused_diagnostics: true,
            ..Default::default()
        },
    );

    assert!(!output.code.contains("void root;"));
    assert!(!output.code.contains("type __R_root = typeof root;"));
    assert!(!output.code.contains("var root: __U<__R_root>"));
    assert!(!output.code.contains("void decoy;"));
}

#[test]
fn test_preserve_unused_diagnostics_keeps_expression_reads_for_static_template_refs() {
    let script = r#"const activatorRef = null
const decoy = null
"#;
    let template = r#"<div ref="activatorRef">{{ activatorRef }}</div>"#;

    let allocator = vize_carton::Allocator::new();
    let (root, _) = vize_armature::parse(&allocator, template);

    let mut analyzer = Analyzer::with_options(AnalyzerOptions::full());
    analyzer.analyze_script_setup(script);
    analyzer.analyze_template(&root);
    let summary = analyzer.finish();

    let output = generate_virtual_ts_with_offsets_and_checks(
        &summary,
        Some(script),
        Some(&root),
        0,
        0,
        &VirtualTsOptions::default(),
        VirtualTsGenerationOptions {
            preserve_unused_diagnostics: true,
            ..Default::default()
        },
    );

    assert!(output.code.contains("void activatorRef;"));
    assert!(!output.code.contains("void decoy;"));
}

/// The setup-scope `props` anchor must stay narrowed to templates that spell
/// `props`, even when unused-local diagnostics are not preserved. The LSP
/// generates with `preserve_unused_diagnostics: false` yet still reports
/// TS6133 as a hint, so anchoring on binding kind alone silences a genuine
/// unused `props` (a template reading prop names directly never reads it).
#[test]
fn test_props_shadow_anchor_requires_a_template_reference_to_props() {
    let anchor = "// Anchor before the template scope shadows `props`";
    let script = r#"const props = defineProps<{ label: string }>()
const other = 1
"#;

    let generate = |template: &str| {
        let allocator = vize_carton::Allocator::new();
        let (root, _) = vize_armature::parse(&allocator, template);

        let mut analyzer = Analyzer::with_options(AnalyzerOptions::full());
        analyzer.analyze_script_setup(script);
        analyzer.analyze_template(&root);
        let summary = analyzer.finish();

        generate_virtual_ts_with_offsets_and_checks(
            &summary,
            Some(script),
            Some(&root),
            0,
            0,
            &VirtualTsOptions::default(),
            VirtualTsGenerationOptions::default(),
        )
        .code
    };

    let forwarded = generate(r#"<button v-bind="props"></button>"#);
    assert!(
        forwarded.contains(anchor),
        "a template forwarding `props` reads the setup binding: {forwarded}"
    );

    let unreferenced = generate(r#"<span>{{ other }}</span>"#);
    assert!(
        !unreferenced.contains(anchor),
        "a template that never spells `props` must keep its unused report: {unreferenced}"
    );
}
