use super::helpers::generate_template_context;
use super::{TemplateGlobal, VirtualTsOptions};
use vize_carton::config::VueVersion;

#[test]
fn template_global_helper_uses_fallback_for_unknown_public_instance_members() {
    let ctx = generate_template_context(
        &VirtualTsOptions {
            strict_instance_globals: true,
            template_globals: vec![TemplateGlobal {
                name: "$route".into(),
                type_annotation: "ReturnType<typeof useRoute>".into(),
                default_value: "undefined as any".into(),
            }],
            ..Default::default()
        },
        VueVersion::V3,
        false,
    );

    assert!(
        ctx.contains("type __VizeGlobalContextProperty<T, F>"),
        "{ctx}"
    );
    assert!(ctx.contains("unknown extends T ? F : T"), "{ctx}");
    assert!(
        ctx.contains("const $route: __Global<'$route', ReturnType<typeof useRoute>>"),
        "{ctx}"
    );
}
