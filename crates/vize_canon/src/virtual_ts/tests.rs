use super::helpers::{VUE_SETUP_HELPERS, generate_template_context, get_dom_event_type};
use super::{
    TemplateGlobal, VirtualTsCheckOptions, VirtualTsGenerationOptions, VirtualTsOptions,
    generate_virtual_ts, generate_virtual_ts_with_offsets,
    generate_virtual_ts_with_offsets_and_checks, generate_virtual_ts_with_offsets_legacy_vue2,
    generate_virtual_ts_with_offsets_options_api,
};
use vize_carton::config::VueVersion;
mod auto_import_shadowing;
mod component_navigation;
mod component_spread_prop_order;
mod define_props_scope;
mod generic_module_type_exports;
mod legacy_nuxt2_page_context;
mod model_update_payload;
mod no_check_template_bindings;
mod options_api_instance;
mod options_api_props_spread;
mod options_api_setup_spread;
mod options_api_this_bridge;
mod slot_component_bindings;
mod template_ref_unwrap;
mod unused_refs;
mod vif_chain;
fn assert_virtual_ts_snapshot(name: &str, value: &str) {
    insta::with_settings!({ snapshot_path => "../../snapshots" }, {
        insta::assert_snapshot!(name, value);
    });
}
#[test]
fn test_vue_setup_helpers_are_actual_functions() {
    assert_virtual_ts_snapshot("virtual_ts_vue_setup_helpers", VUE_SETUP_HELPERS);
}

#[test]
fn test_vue_template_context() {
    let ctx = generate_template_context(&VirtualTsOptions::default(), VueVersion::V3, false);
    assert_virtual_ts_snapshot("virtual_ts_vue_template_context", ctx.as_str());
}

#[test]
fn test_vue_template_context_v3_default_is_unchanged() {
    // The default Vue 3 dialect must emit the exact same context as the
    // dialect-unaware default — no Vue 2-only members leak into Vue 3.
    let v3 = generate_template_context(&VirtualTsOptions::default(), VueVersion::V3, false);
    assert!(!v3.contains("$listeners"));
    assert!(!v3.contains("$children"));
    assert!(!v3.contains("$scopedSlots"));
    assert!(!v3.contains("$createElement"));
    assert!(!v3.contains("Vue 2 instance members"));
}

#[test]
fn test_vue_template_context_v2_dialect_adds_vue2_members() {
    // A Vue 2 dialect augments the template context with Vue 2-only public
    // instance members so legacy templates ($listeners, $children, the
    // $on/$off/$once emitter, $set/$delete, $createElement, ...) type-check.
    let v2 = generate_template_context(&VirtualTsOptions::default(), VueVersion::V2, false);
    assert!(!v2.contains("import('vue').ComponentPublicInstance"));
    for member in [
        "$listeners",
        "$children",
        "$scopedSlots",
        "$on",
        "$off",
        "$once",
        "$set",
        "$delete",
        "$createElement",
        "_c",
    ] {
        assert!(
            v2.contains(&format!("const {member} = undefined as any;")),
            "Vue 2 context should declare `{member}`"
        );
        assert!(
            v2.contains(&format!("void {member};")),
            "Vue 2 context should mark `{member}` as used"
        );
    }
    // Vue 2.7 shares the same template-instance shape.
    let v2_7 = generate_template_context(&VirtualTsOptions::default(), VueVersion::V2_7, false);
    assert!(v2_7.contains("const $listeners = undefined as any;"));

    // Vue 3 must NOT contain any of these (byte-identical to before).
    let v3 = generate_template_context(&VirtualTsOptions::default(), VueVersion::V3, false);
    assert!(!v3.contains("$listeners"));
    assert!(!v3.contains("$createElement"));
}

#[test]
fn test_vue_template_context_with_globals() {
    // Plugin globals should appear when configured
    let options = VirtualTsOptions {
        template_globals: vec![
            TemplateGlobal {
                name: "$t".into(),
                type_annotation: "(...args: any[]) => string".into(),
                default_value: "(() => '') as any".into(),
            },
            TemplateGlobal {
                name: "$route".into(),
                type_annotation: "any".into(),
                default_value: "{} as any".into(),
            },
        ],
        ..Default::default()
    };
    let ctx = generate_template_context(&options, VueVersion::V3, false);
    assert_virtual_ts_snapshot("virtual_ts_vue_template_context_with_globals", ctx.as_str());
}

fn analyze_options_api_script(script: &str) -> vize_croquis::Croquis {
    use vize_croquis::{Analyzer, AnalyzerOptions};

    let mut analyzer = Analyzer::with_options(AnalyzerOptions::full()).with_options_api();
    analyzer.analyze_script_plain(script);
    analyzer.finish()
}

#[test]
fn test_options_api_unresolved_extends_exposes_template_refs_as_inherited_any() {
    use vize_croquis::{Analyzer, AnalyzerOptions};

    let script = r#"import BaseButton from './BaseButton.vue'

export default {
    name: 'Button',
    extends: BaseButton
}
"#;
    let template = r#"<component v-if="!asChild" :is="as" :class="cx('root')">
  <span v-if="label">{{ label }}</span>
</component>"#;

    let allocator = vize_carton::Allocator::new();
    let (root, _) = vize_armature::parse(&allocator, template);

    let mut analyzer = Analyzer::with_options(AnalyzerOptions::full()).with_options_api();
    analyzer.analyze_script_plain(script);
    analyzer.analyze_template(&root);
    let summary = analyzer.finish();
    let output = generate_virtual_ts_with_offsets_options_api(
        &summary,
        Some(script),
        Some(&root),
        0,
        0,
        &Default::default(),
    );

    for name in ["asChild", "as", "cx", "label"] {
        assert!(
            output
                .code
                .contains(&format!("const {name}: any = undefined as any;")),
            "unresolved imported extends should expose inherited `{name}`:\n{}",
            output.code
        );
    }
}

#[test]
fn test_options_api_emits_real_props_type_from_object_option() {
    // Runtime props are values, so keep the object in setup scope and derive a
    // real `export type Props` from the captured value.
    let script = r#"export default {
    props: {
        initial: Number,
        label: { type: String, required: true },
    },
}
"#;
    let summary = analyze_options_api_script(script);
    let output = generate_virtual_ts_with_offsets_options_api(
        &summary,
        Some(script),
        None,
        0,
        0,
        &Default::default(),
    );

    assert!(
        output.code.contains(
            "const __vize_options_props = ({\n        initial: Number,\n        label: { type: String, required: true },\n    } as const);"
        ),
        "expected the runtime props object to remain in value scope:\n{}",
        output.code
    );
    assert!(
        !output.code.contains("export type Props = {};"),
        "Options API props must not fall back to the `{{}}` no-op:\n{}",
        output.code
    );
}

#[test]
fn test_options_api_emits_props_type_from_array_option() {
    // The array form carries no runtime type info, so each prop is emitted as an
    // optional `unknown` member rather than the `{}` no-op.
    let script = r#"export default {
    props: ['initial', 'label'],
}
"#;
    let summary = analyze_options_api_script(script);
    let output = generate_virtual_ts_with_offsets_options_api(
        &summary,
        Some(script),
        None,
        0,
        0,
        &Default::default(),
    );

    assert!(
        output.code.contains(
            "export type Props = {\n  \"initial\"?: unknown;\n  \"label\"?: unknown;\n};"
        ),
        "expected array-form props to be emitted as optional unknown members:\n{}",
        output.code
    );
}

#[test]
fn test_options_api_props_type_skipped_without_options_api_flag() {
    // Without the Options API opt-in, the historical `{}` no-op is preserved so
    // existing default behavior (and snapshots) is unchanged.
    let script = r#"export default {
    props: {
        initial: Number,
    },
}
"#;
    let summary = analyze_options_api_script(script);
    let output =
        generate_virtual_ts_with_offsets(&summary, Some(script), None, 0, 0, &Default::default());

    assert!(
        output.code.contains("export type Props = {};"),
        "Props derivation must be gated behind the Options API opt-in:\n{}",
        output.code
    );
}

#[test]
fn test_class_component_default_export_keeps_decorators_on_class() {
    // vue-class-component / vue-property-decorator: a decorated class default
    // export must become a standalone class declaration plus a
    // `const __default__ = Foo` alias. The decorator must stay on a real class
    // declaration (never on a `const`/class expression — TS1206).
    let script = r#"import { Vue } from 'vue-class-component'
import { Component, Prop } from 'vue-property-decorator'

@Component
export default class Counter extends Vue {
  @Prop() readonly initial!: number
  count = 0
  get doubled() {
    return this.count * 2
  }
  bump() {
    this.count += 1
  }
}
"#;
    let allocator = vize_carton::Allocator::new();
    let (root, _) = vize_armature::parse(&allocator, "<div>{{ count }}{{ doubled }}</div>");
    let mut analyzer = vize_croquis::Analyzer::with_options(vize_croquis::AnalyzerOptions::full())
        .with_options_api();
    analyzer.analyze_script_plain(script);
    analyzer.analyze_template(&root);
    let summary = analyzer.finish();
    let output = generate_virtual_ts_with_offsets_options_api(
        &summary,
        Some(script),
        Some(&root),
        0,
        0,
        &Default::default(),
    );

    assert!(
        output.code.contains("@Component"),
        "the class-level decorator must be preserved:\n{}",
        output.code
    );
    assert!(
        output.code.contains("class Counter extends Vue {"),
        "the class must stay a standalone class declaration:\n{}",
        output.code
    );
    assert!(
        output.code.contains("const __default__ = Counter"),
        "the class must be aliased to __default__ by name:\n{}",
        output.code
    );
    assert!(
        !output.code.contains("__default__ = class"),
        "the class must not become a class expression assigned to a const (TS1206):\n{}",
        output.code
    );
    assert!(
        !output.code.contains("export default class"),
        "the `export default` keyword must be stripped off the class declaration:\n{}",
        output.code
    );
    // The instance-type bridge resolves template bindings against the class
    // instance (`typeof __default__` -> InstanceType), not bare `any`.
    assert!(
        output
            .code
            .contains("var count: __VizeOptionsBinding<typeof __default__, \"count\">"),
        "class members must be typed from the class instance:\n{}",
        output.code
    );
}

#[test]
fn test_script_setup_output_does_not_emit_options_api_bridge() {
    use vize_croquis::{Analyzer, AnalyzerOptions};

    let script = r#"import { ref } from 'vue'
const count = ref(0)
"#;
    let mut analyzer = Analyzer::with_options(AnalyzerOptions::full());
    analyzer.analyze_script_setup(script);
    let summary = analyzer.finish();

    let output =
        generate_virtual_ts_with_offsets(&summary, Some(script), None, 0, 0, &Default::default());

    assert!(
        !output.code.contains("__VizeThis"),
        "`<script setup>` output must not contain the Options API bridge:\n{}",
        output.code
    );
    assert!(
        !output.code.contains("__vize_method_") && !output.code.contains("__vize_computed_"),
        "`<script setup>` output must not contain Options API wrappers:\n{}",
        output.code
    );
}

#[test]
fn test_script_setup_virtual_ts_byte_identical_with_options_api_default_on() {
    // Zero-cost guarantee for the benchmarked path: now that Options API
    // resolution is default-on, the `_options_api` generator must produce output
    // byte-identical to the plain generator for a `<script setup>` component (the
    // bridge only runs for non-`<script setup>` components).
    use vize_croquis::{Analyzer, AnalyzerOptions};

    let script = r#"import { ref, computed } from 'vue'
const count = ref(0)
const doubled = computed(() => count.value * 2)
function inc() { count.value++ }
"#;
    let mut analyzer = Analyzer::with_options(AnalyzerOptions::full());
    analyzer.analyze_script_setup(script);
    let summary = analyzer.finish();

    let plain =
        generate_virtual_ts_with_offsets(&summary, Some(script), None, 0, 0, &Default::default());
    let options_api = generate_virtual_ts_with_offsets_options_api(
        &summary,
        Some(script),
        None,
        0,
        0,
        &Default::default(),
    );

    assert_eq!(
        plain.code, options_api.code,
        "`<script setup>` virtual TS must be byte-identical with Options API default-on"
    );
}

#[test]
fn test_script_setup_with_define_props_type_ref_byte_identical_with_options_api_default_on() {
    // Regression: `defineProps<Props>()` (where `Props` is a type reference,
    // not an inline `TSTypeLiteral`) registers destructured names as
    // `BindingType::Props` without populating `summary.macros.props()`. Before
    // the `is_script_setup` gate, those names slipped through the Options API
    // template-binding filter and produced spurious `__VizeOptionsBinding`
    // declarations on a `<script setup>` SFC, breaking the byte-identical
    // guarantee.
    use vize_croquis::{Analyzer, AnalyzerOptions};

    let script = r#"interface Props {
  title: string;
  count?: number;
  tags?: string[];
}

const { title, count = 0, tags = [] } = defineProps<Props>();

console.log(title.toUpperCase(), count.toFixed(0), tags.join(","));
"#;
    let mut analyzer = Analyzer::with_options(AnalyzerOptions::full());
    analyzer.analyze_script_setup(script);
    let summary = analyzer.finish();

    let plain =
        generate_virtual_ts_with_offsets(&summary, Some(script), None, 0, 0, &Default::default());
    let options_api = generate_virtual_ts_with_offsets_options_api(
        &summary,
        Some(script),
        None,
        0,
        0,
        &Default::default(),
    );

    assert_eq!(
        plain.code, options_api.code,
        "`<script setup>` virtual TS must be byte-identical with Options API default-on, \
         even when defineProps uses a type reference"
    );
}

fn generate_script_setup_virtual_ts(script: &str) -> String {
    use vize_croquis::{Analyzer, AnalyzerOptions};

    let mut analyzer = Analyzer::with_options(AnalyzerOptions::full());
    analyzer.analyze_script_setup(script);
    let summary = analyzer.finish();

    generate_virtual_ts_with_offsets(&summary, Some(script), None, 0, 0, &Default::default())
        .code
        .to_string()
}

#[test]
fn test_define_props_local_interface_fields_resolved_via_ast() {
    // Verify-real for #1394: a local `interface Props` whose body the old
    // raw-text scanner mis-handled is now resolved through the OXC AST that
    // croquis registers into the TypeResolver. The script exercises three
    // failure modes of the deleted scanner at once:
    //   1. A leading comment that literally contains `interface Props { ... }`.
    //      The old `find_type_body` anchored on the *first* substring match of
    //      `interface Props `, so it would extract the decoy field from the
    //      comment instead of the real declaration.
    //   2. A field whose type is a generic with a comma (`Map<string, number>`).
    //   3. A nested object type and a trailing line comment containing a `}`.
    use vize_croquis::{Analyzer, AnalyzerOptions};

    let script = r#"
// A real interface Props { decoy: never } lives further down.
interface Props {
  title: string;
  meta: {
    createdAt: number;
    tags: string[];
  };
  lookup: Map<string, number>;
  count?: number; // counts things } and quotes "}"
}

defineProps<Props>();
"#;
    let template = r#"<div>{{ title }}{{ meta }}{{ lookup }}{{ count }}</div>"#;

    let allocator = vize_carton::Allocator::new();
    let (root, _) = vize_armature::parse(&allocator, template);

    let mut analyzer = Analyzer::with_options(AnalyzerOptions::full());
    analyzer.analyze_script_setup(script);
    analyzer.analyze_template(&root);
    let summary = analyzer.finish();

    // The interface was registered into the TypeResolver from the OXC AST, so
    // its top-level fields resolve directly — no raw-text scan.
    let props = summary.types.extract_properties("Props");
    let names: Vec<&str> = props.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["title", "meta", "lookup", "count"],
        "AST-backed TypeResolver must yield exactly the top-level interface fields"
    );

    let output = generate_virtual_ts(&summary, Some(script), Some(&root), 0);

    for field in ["title", "meta", "lookup", "count"] {
        assert!(
            output.code.contains(&format!("void {field};")),
            "expected AST-resolved prop `{field}` to be bound in template scope:\n{}",
            output.code
        );
    }
    assert!(
        !output.code.contains("void decoy;"),
        "the decoy field from the comment must not be extracted as a prop:\n{}",
        output.code
    );
}

#[test]
fn test_define_props_local_intersection_alias_emits_keyed_bindings() {
    // A local `type Props = A & B` resolves through the now-populated
    // TypeResolver: its body carries a top-level `&`, so the generator falls
    // back to keyed `satisfies keyof Props` bindings rather than field-by-field
    // extraction. This path previously relied on the definitions map being
    // empty; registering local types from the AST drives it correctly.
    use vize_croquis::{Analyzer, AnalyzerOptions};

    let script = r#"
interface Base { id: string }
interface Extra { label: string }
type Props = Base & Extra;

defineProps<Props>();
"#;
    let template = r#"<div>{{ id }}{{ label }}</div>"#;

    let allocator = vize_carton::Allocator::new();
    let (root, _) = vize_armature::parse(&allocator, template);

    let mut analyzer = Analyzer::with_options(AnalyzerOptions::full());
    analyzer.analyze_script_setup(script);
    analyzer.analyze_template(&root);
    let summary = analyzer.finish();

    let output = generate_virtual_ts(&summary, Some(script), Some(&root), 0);

    assert!(
        output.code.contains("satisfies keyof Props"),
        "an intersection prop alias should emit keyed template bindings:\n{}",
        output.code
    );
}

#[test]
fn test_script_setup_top_level_await_emits_async_setup() {
    let output = generate_script_setup_virtual_ts("const data = await fetchData()\n");

    assert!(
        output.contains("async function __setup()"),
        "expected top-level await to emit async setup:\n{output}"
    );
}

#[test]
fn test_script_setup_top_level_for_await_emits_async_setup() {
    let output = generate_script_setup_virtual_ts(
        r#"
for await (const item of items) {
    console.log(item)
}
"#,
    );

    assert!(
        output.contains("async function __setup()"),
        "expected top-level for-await to emit async setup:\n{output}"
    );
}

#[test]
fn test_script_setup_nested_await_and_text_do_not_emit_async_setup() {
    let output = generate_script_setup_virtual_ts(
        r#"
const message = "await should not force async setup"
// await should not force async setup
async function load() {
    await fetchData()
}
const run = async () => {
    await fetchData()
}
"#,
    );

    assert!(
        output.contains("function __setup()"),
        "expected setup function to be emitted:\n{output}"
    );
    assert!(
        !output.contains("async function __setup()"),
        "nested/text await must not force async setup:\n{output}"
    );
}

#[test]
fn test_const_auto_import_stubs_skip_imported_names() {
    use vize_croquis::{Analyzer, AnalyzerOptions};

    let script = r#"import { currentUser } from './users'
const count = 1
"#;

    let mut analyzer = Analyzer::with_options(AnalyzerOptions::full());
    analyzer.analyze_script_setup(script);
    let summary = analyzer.finish();

    let options = VirtualTsOptions {
        auto_import_stubs: vec![
            "declare const currentUser: any;".into(),
            "declare const useHydratedHead: any;".into(),
        ],
        ..Default::default()
    };

    let output = generate_virtual_ts_with_offsets(&summary, Some(script), None, 0, 0, &options);

    assert_virtual_ts_snapshot(
        "virtual_ts_auto_import_stubs_skip_imported_names",
        output.code.as_str(),
    );
}

#[test]
fn test_external_template_bindings_do_not_shadow_auto_imported_components() {
    use vize_croquis::{Analyzer, AnalyzerOptions};

    let script = "const count = 'oops'\n";
    let template = r#"<auto-card :count="count" />"#;

    let allocator = vize_carton::Allocator::new();
    let (root, _) = vize_armature::parse(&allocator, template);

    let mut analyzer = Analyzer::with_options(AnalyzerOptions::full());
    analyzer.analyze_script_setup(script);
    analyzer.analyze_template(&root);
    let summary = analyzer.finish();

    let options = VirtualTsOptions {
        auto_import_stubs: vec![
            "declare const AutoCard: typeof import('./components/AutoCard.vue.ts')['default'];"
                .into(),
        ],
        external_template_bindings: vec!["AutoCard".into()],
        ..Default::default()
    };
    let output =
        generate_virtual_ts_with_offsets(&summary, Some(script), Some(&root), 0, 0, &options);

    assert!(
        output
            .code
            .contains("declare const AutoCard: typeof import")
    );
    assert!(!output.code.contains("const auto_card: any"));
    assert!(
        output
            .code
            .contains("type __auto_card_Props_0 = typeof AutoCard extends { __vizeCheck: infer __F } ? (__VizeIsAny<__F> extends true ? __VizeComponentRawProps<typeof AutoCard> : Record<string, unknown>) : __VizeComponentRawProps<typeof AutoCard>;")
    );
}

#[test]
fn test_unknown_pascal_component_props_use_the_ambient_fallback() {
    use vize_croquis::{Analyzer, AnalyzerOptions};
    let script = "const count = 'unknown'\n";
    let template = r#"<AutoCard :count="count" />"#;
    let allocator = vize_carton::Allocator::new();
    let (root, _) = vize_armature::parse(&allocator, template);

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
        &VirtualTsOptions::default(),
    );

    assert!(output.code.contains(
        "declare const AutoCard: import(\"vue\").GlobalComponents extends { \"AutoCard\": infer __C } ? __C : any;"
    ));
    assert!(output.code.contains("type __AutoCard_Props_0"));
    assert!(output.code.contains("__AutoCard_Check_0"));
}

#[test]
fn test_template_instance_globals_delegate_to_component_public_instance() {
    use vize_croquis::{Analyzer, AnalyzerOptions};

    let template = r#"<button :title="$t('hello')">{{ missing }}</button>"#;

    let allocator = vize_carton::Allocator::new();
    let (root, _) = vize_armature::parse(&allocator, template);

    let mut analyzer = Analyzer::with_options(AnalyzerOptions::full());
    analyzer.analyze_template(&root);
    let summary = analyzer.finish();

    let output = generate_virtual_ts_with_offsets(
        &summary,
        None,
        Some(&root),
        0,
        0,
        &VirtualTsOptions::default(),
    );

    assert!(
        output
            .code
            .contains("const $t: __VizeInstanceGlobal<'$t'> = undefined as any;"),
        "{}",
        output.code
    );
    assert!(output.code.contains("= ($t('hello'));")); // native prop check initializer
    assert!(output.code.contains("void (missing);"));
    assert!(!output.code.contains("void ($t);"));

    let configured_output = generate_virtual_ts_with_offsets(
        &summary,
        None,
        Some(&root),
        0,
        0,
        &VirtualTsOptions {
            template_globals: vec![TemplateGlobal {
                name: "$t".into(),
                type_annotation: "(key: string) => string".into(),
                default_value: "(() => '') as any".into(),
            }],
            ..Default::default()
        },
    );

    assert!(
        !configured_output
            .code
            .contains("__VizeInstanceGlobal<'$t'>")
    );
    assert!(
        configured_output
            .code
            .contains("const $t: __Global<'$t', (key: string) => string>")
    );
}

#[test]
fn test_template_instance_globals_skip_setup_bindings() {
    use vize_croquis::{Analyzer, AnalyzerOptions};

    let script = r#"function functionCall(): any {}
const $q = functionCall()
"#;
    let template = r#"<div v-if="$q">None</div>"#;

    let allocator = vize_carton::Allocator::new();
    let (root, _) = vize_armature::parse(&allocator, template);

    let mut analyzer = Analyzer::with_options(AnalyzerOptions::full());
    analyzer.analyze_script_setup(script);
    analyzer.analyze_template(&root);
    let summary = analyzer.finish();

    let output = generate_virtual_ts(&summary, Some(script), Some(&root), 0);

    assert!(
        !output
            .code
            .contains("const $q: __VizeInstanceGlobal<'$q'> = undefined as any;"),
        "setup binding named like an instance global must not be redeclared:\n{}",
        output.code
    );
    assert!(
        output.code.contains("if (($q))"),
        "template expression should still resolve the setup binding:\n{}",
        output.code
    );
}

#[test]
fn test_define_expose_is_part_of_component_instance() {
    use vize_croquis::{Analyzer, AnalyzerOptions};

    let script = r#"defineExpose({
  hide: () => {
    console.log()
  },
})
"#;

    let mut analyzer = Analyzer::with_options(AnalyzerOptions::full());
    analyzer.analyze_script_setup(script);
    let summary = analyzer.finish();

    let output = generate_virtual_ts(&summary, Some(script), None, 0);

    assert!(
        output.code.contains(
            "export type Exposed = Awaited<ReturnType<typeof __setup>>[\"__vize_exposed\"];"
        ),
        "runtime defineExpose should emit an Exposed type:\n{}",
        output.code
    );
    assert!(
        output.code.contains("type __VizeComponentInstance = {\n  $props: Props;\n  readonly __vizeRawProps?: Props;\n  $emit: __VizeStrictPublicEmit<Emits>;\n  $slots: Slots;\n} & __VizeComponentPublicBase & __VizeShallowUnwrapRef<Exposed>;"),
        "component instance should include exposed bindings:\n{}",
        output.code
    );
}

#[test]
fn test_kebab_case_component_names_are_sanitized_in_type_helpers() {
    use vize_croquis::{Analyzer, AnalyzerOptions};

    let script = r#"const value = 'hello'
function handleUpdate(value: string) {
  void value
}
"#;
    let template = r#"<my-widget :label="value" @update:model-value="handleUpdate" />"#;

    let allocator = vize_carton::Allocator::new();
    let (root, _) = vize_armature::parse(&allocator, template);

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

    assert_virtual_ts_snapshot(
        "virtual_ts_kebab_case_component_names",
        output.code.as_str(),
    );
}
#[test]
fn test_check_props_option_disables_component_prop_checks() {
    use vize_croquis::{Analyzer, AnalyzerOptions};

    let script = r#"import Child from './Child.vue'
const wrong = 'not a number'
"#;
    let template = r#"<Child :count="wrong" />"#;

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
            check_options: VirtualTsCheckOptions {
                check_props: false,
                ..Default::default()
            },
            ..Default::default()
        },
    );

    assert!(!output.code.contains("__vize_prop_check"));
    assert!(!output.code.contains("type __Child_Props_0"));
}

#[test]
fn test_dom_event_type_mapping() {
    // Mouse events
    assert_eq!(get_dom_event_type("dblclick"), "MouseEvent");
    assert_eq!(get_dom_event_type("mousedown"), "MouseEvent");
    assert_eq!(get_dom_event_type("mouseup"), "MouseEvent");
    assert_eq!(get_dom_event_type("mousemove"), "MouseEvent");

    // Pointer events
    assert_eq!(get_dom_event_type("click"), "PointerEvent");
    assert_eq!(get_dom_event_type("auxclick"), "PointerEvent");
    assert_eq!(get_dom_event_type("contextmenu"), "PointerEvent");
    assert_eq!(get_dom_event_type("pointerdown"), "PointerEvent");
    assert_eq!(get_dom_event_type("pointerup"), "PointerEvent");

    // Touch events
    assert_eq!(get_dom_event_type("touchstart"), "TouchEvent");
    assert_eq!(get_dom_event_type("touchend"), "TouchEvent");

    // Keyboard events
    assert_eq!(get_dom_event_type("keydown"), "KeyboardEvent");
    assert_eq!(get_dom_event_type("keyup"), "KeyboardEvent");
    assert_eq!(get_dom_event_type("keypress"), "KeyboardEvent");

    // Focus events
    assert_eq!(get_dom_event_type("focus"), "FocusEvent");
    assert_eq!(get_dom_event_type("blur"), "FocusEvent");

    // Input events
    assert_eq!(get_dom_event_type("input"), "InputEvent");
    assert_eq!(get_dom_event_type("beforeinput"), "InputEvent");

    // Form events
    assert_eq!(get_dom_event_type("submit"), "SubmitEvent");
    assert_eq!(get_dom_event_type("change"), "Event");

    // Drag events
    assert_eq!(get_dom_event_type("drag"), "DragEvent");
    assert_eq!(get_dom_event_type("drop"), "DragEvent");

    // Clipboard events
    assert_eq!(get_dom_event_type("copy"), "ClipboardEvent");
    assert_eq!(get_dom_event_type("paste"), "ClipboardEvent");

    // Wheel events
    assert_eq!(get_dom_event_type("wheel"), "WheelEvent");

    // Animation events
    assert_eq!(get_dom_event_type("animationstart"), "AnimationEvent");
    assert_eq!(get_dom_event_type("animationend"), "AnimationEvent");

    // Transition events
    assert_eq!(get_dom_event_type("transitionend"), "TransitionEvent");

    // Unknown/custom events fallback to Event
    assert_eq!(get_dom_event_type("customEvent"), "Event");
    assert_eq!(get_dom_event_type("unknown"), "Event");
}

#[test]
fn test_vfor_destructuring_scope() {
    use vize_croquis::{Analyzer, AnalyzerOptions};

    let script = r#"import { ref } from 'vue'
const items = ref([{ id: 1, name: 'Hello' }])
"#;
    let template = r#"<ul>
  <li v-for="{ id, name } in items" :key="id">
    {{ id }}: {{ name }}
  </li>
</ul>"#;

    let allocator = vize_carton::Allocator::new();
    let (root, _) = vize_armature::parse(&allocator, template);

    let mut analyzer = Analyzer::with_options(AnalyzerOptions::full());
    analyzer.analyze_script_setup(script);
    analyzer.analyze_template(&root);
    let summary = analyzer.finish();

    let output = generate_virtual_ts(&summary, Some(script), Some(&root), 0);

    assert_virtual_ts_snapshot("virtual_ts_vfor_destructuring_scope", output.code.as_str());
}

#[test]
fn test_vfor_source_nested_in_vif_is_wrapped_for_narrowing() {
    // Regression for #1511: a v-for whose source expression depends on a value
    // narrowed by an *enclosing* v-if must have its whole
    // `__vForList(source).forEach(...)` loop emitted INSIDE the `if (guard) {}`
    // block, so TypeScript narrows identifiers used in the source expression.
    use vize_croquis::{Analyzer, AnalyzerOptions};

    let script = r#"const elems = { a: [1], b: ["a"] } as const;
const key = "a" as "a" | "b";"#;
    let template = r#"<div v-if="key === 'b'">
  <button v-for="value in elems[key]" :key="value">{{ value }}</button>
</div>"#;

    let allocator = vize_carton::Allocator::new();
    let (root, _) = vize_armature::parse(&allocator, template);

    let mut analyzer = Analyzer::with_options(AnalyzerOptions::full());
    analyzer.analyze_script_setup(script);
    analyzer.analyze_template(&root);
    let summary = analyzer.finish();

    let output = generate_virtual_ts(&summary, Some(script), Some(&root), 0);
    assert_virtual_ts_snapshot(
        "virtual_ts_vfor_source_nested_in_vif_is_wrapped_for_narrowing",
        output.code.as_str(),
    );
}

#[test]
fn test_vfor_with_nested_vif_in_body_is_not_wrapped() {
    // Guard for #1511: a `v-if` *inside* the v-for body must NOT cause the whole
    // loop to be wrapped — only an *enclosing* v-if narrows the source. Here the
    // loop source `items` has no enclosing guard, so `__vForList(items)` must be
    // emitted at the scope's own indentation, not inside any `if (...)`.
    use vize_croquis::{Analyzer, AnalyzerOptions};

    let script = r#"const items = [1, 2, 3];
const show = true;"#;
    let template = r#"<ul>
  <li v-for="item in items" :key="item">
    <span v-if="show">{{ item }}</span>
  </li>
</ul>"#;

    let allocator = vize_carton::Allocator::new();
    let (root, _) = vize_armature::parse(&allocator, template);

    let mut analyzer = Analyzer::with_options(AnalyzerOptions::full());
    analyzer.analyze_script_setup(script);
    analyzer.analyze_template(&root);
    let summary = analyzer.finish();

    let output = generate_virtual_ts(&summary, Some(script), Some(&root), 0);
    assert_virtual_ts_snapshot(
        "virtual_ts_vfor_with_nested_vif_in_body_is_not_wrapped",
        output.code.as_str(),
    );
}

#[test]
fn test_nested_vif_velse_chain() {
    use vize_croquis::{Analyzer, AnalyzerOptions};

    let script = r#"import { ref } from 'vue'
const status = ref('loading')
const message = ref('')
"#;
    let template = r#"<div>
  <div v-if="status === 'loading'">Loading</div>
  <div v-else-if="status === 'error'">{{ message }}</div>
  <div v-else>Done</div>
</div>"#;

    let allocator = vize_carton::Allocator::new();
    let (root, _) = vize_armature::parse(&allocator, template);

    let mut analyzer = Analyzer::with_options(AnalyzerOptions::full());
    analyzer.analyze_script_setup(script);
    analyzer.analyze_template(&root);
    let summary = analyzer.finish();

    let output = generate_virtual_ts(&summary, Some(script), Some(&root), 0);

    assert_virtual_ts_snapshot("virtual_ts_nested_vif_velse_chain", output.code.as_str());
}

#[test]
fn test_v_else_if_chain_uses_linear_control_flow() {
    use vize_croquis::{Analyzer, AnalyzerOptions};

    let script = r#"type Log =
  | { type: 't0'; info: { value0: string } }
  | { type: 't1'; info: { value1: string } }
  | { type: 't2'; info: { value2: string } }

defineProps<{ log: Log }>()
"#;
    let template = r#"<div>
  <span v-if="log.type === 't0'">{{ log.info.value0 }}</span>
  <span v-else-if="log.type === 't1'">{{ log.info.value1 }}</span>
  <span v-else-if="log.type === 't2'">{{ log.info.value2 }}</span>
</div>"#;

    let allocator = vize_carton::Allocator::new();
    let (root, _) = vize_armature::parse(&allocator, template);

    let mut analyzer = Analyzer::with_options(AnalyzerOptions::full());
    analyzer.analyze_script_setup(script);
    analyzer.analyze_template(&root);
    let summary = analyzer.finish();

    let output = generate_virtual_ts(&summary, Some(script), Some(&root), 0);

    assert!(
        output.code.contains("if (log.type === 't0') {"),
        "expected first branch to use native control flow:\n{}",
        output.code
    );
    assert!(
        output.code.contains("} else if (log.type === 't1') {")
            && output.code.contains("} else if (log.type === 't2') {"),
        "expected else-if branches to use native control flow:\n{}",
        output.code
    );
    assert!(
        !output.code.contains("!(log.type === 't0') &&"),
        "virtual TS should not repeat cumulative negated branch guards:\n{}",
        output.code
    );
    assert!(
        !output.code.contains("void (log.type === 't1'); // VIf"),
        "branch conditions should not be emitted again inside the branch body:\n{}",
        output.code
    );
}

#[test]
fn test_scoped_slot_expressions() {
    use vize_croquis::{Analyzer, AnalyzerOptions};

    let script = r#"import MyList from './MyList.vue'
const items = ['a', 'b']
"#;
    let template = r#"<MyList :items="items">
  <template #default="{ item }">
    {{ item }}
  </template>
</MyList>"#;

    let allocator = vize_carton::Allocator::new();
    let (root, _) = vize_armature::parse(&allocator, template);

    let mut analyzer = Analyzer::with_options(AnalyzerOptions::full());
    analyzer.analyze_script_setup(script);
    analyzer.analyze_template(&root);
    let summary = analyzer.finish();

    let output = generate_virtual_ts(&summary, Some(script), Some(&root), 0);

    assert_virtual_ts_snapshot("virtual_ts_scoped_slot_expressions", output.code.as_str());
}

#[test]
fn test_repeated_default_slots_use_unique_helper_names() {
    use vize_croquis::{Analyzer, AnalyzerOptions};

    let script = r#"import Child from './Child.vue'
"#;
    let template = r#"<div>
  <Child>
    <template #default="{ item }">
      {{ item }}
    </template>
  </Child>
  <Child>
    <template #default="{ other }">
      {{ other }}
    </template>
  </Child>
</div>"#;

    let allocator = vize_carton::Allocator::new();
    let (root, _) = vize_armature::parse(&allocator, template);

    let mut analyzer = Analyzer::with_options(AnalyzerOptions::full());
    analyzer.analyze_script_setup(script);
    analyzer.analyze_template(&root);
    let summary = analyzer.finish();

    let output = generate_virtual_ts(&summary, Some(script), Some(&root), 0);

    let slot_helpers: Vec<_> = output
        .code
        .lines()
        .filter_map(|line| {
            line.trim_start()
                .strip_prefix("void function _slot_default_")
                .and_then(|rest| rest.split_once('(').map(|(id, _)| id.to_string()))
        })
        .collect();
    assert_eq!(
        slot_helpers.len(),
        2,
        "expected both default slot scopes to emit helpers:\n{}",
        output.code
    );
    assert_ne!(
        slot_helpers[0], slot_helpers[1],
        "default slot helpers must include the scope id:\n{}",
        output.code
    );
    assert!(
        output
            .code
            .matches("void function _slot_props_default_")
            .count()
            >= 2,
        "slot prop helper names should also include the scope id:\n{}",
        output.code
    );
}

#[test]
fn test_v_if_narrows_nullable_binding() {
    // `<div v-if="user">{{ user.name }}</div>` must produce a virtual TS
    // closure that opens an `if (user) { … }` block so TypeScript narrows
    // `user` from `User | null` to `User` for the inner expression. See
    // #693. The snapshot captures the generated narrowing structure.
    use vize_croquis::{Analyzer, AnalyzerOptions};

    let script = r#"interface User { name: string }
const user: User | null = null as any
"#;
    let template = r#"<div v-if="user">
  <p>{{ user.name }}</p>
</div>"#;

    let allocator = vize_carton::Allocator::new();
    let (root, _) = vize_armature::parse(&allocator, template);

    let mut analyzer = Analyzer::with_options(AnalyzerOptions::full());
    analyzer.analyze_script_setup(script);
    analyzer.analyze_template(&root);
    let summary = analyzer.finish();

    let output = generate_virtual_ts(&summary, Some(script), Some(&root), 0);

    // The narrowing wrapper must appear in the generated TS so TS can
    // narrow `user` for the inner property access.
    assert!(
        output.code.contains("if ((user))"),
        "expected `if ((user))` narrowing wrapper in virtual TS, got:\n{}",
        output.code
    );
}

#[test]
fn test_reserved_prop_and_hyphenated_slot_names() {
    use vize_croquis::{Analyzer, AnalyzerOptions};

    let script = r#"import TrendChart from './TrendChart.vue'
defineProps<{
  class?: string
}>()
"#;
    let template = r#"<TrendChart :class="class">
  <template #area-gradient="{ id }">
    {{ id }}
  </template>
</TrendChart>"#;

    let allocator = vize_carton::Allocator::new();
    let (root, _) = vize_armature::parse(&allocator, template);

    let mut analyzer = Analyzer::with_options(AnalyzerOptions::full());
    analyzer.analyze_script_setup(script);
    analyzer.analyze_template(&root);
    let summary = analyzer.finish();

    let output = generate_virtual_ts(&summary, Some(script), Some(&root), 0);

    let expression_start = template.find("\"class\"").unwrap() + 1;
    let expression_end = expression_start + "class".len();
    let mapping = output
        .mappings
        .iter()
        .find(|mapping| mapping.src_range == (expression_start..expression_end))
        .expect("should map the rewritten class prop expression");
    assert_eq!(&output.code[mapping.gen_range.clone()], "props[\"class\"]");

    assert_virtual_ts_snapshot(
        "virtual_ts_reserved_prop_and_hyphenated_slot_names",
        output.code.as_str(),
    );
}

#[test]
fn test_multiple_event_handlers() {
    use vize_croquis::{Analyzer, AnalyzerOptions};

    let script = r#"import { ref } from 'vue'
const count = ref(0)
function handleClick() { count.value++ }
function handleHover() {}
"#;
    let template = r#"<div>
  <button @click="handleClick" @mouseenter="handleHover">{{ count }}</button>
</div>"#;

    let allocator = vize_carton::Allocator::new();
    let (root, _) = vize_armature::parse(&allocator, template);

    let mut analyzer = Analyzer::with_options(AnalyzerOptions::full());
    analyzer.analyze_script_setup(script);
    analyzer.analyze_template(&root);
    let summary = analyzer.finish();

    let output = generate_virtual_ts(&summary, Some(script), Some(&root), 0);

    assert_virtual_ts_snapshot("virtual_ts_multiple_event_handlers", output.code.as_str());
}

#[test]
fn test_v_if_guard_wraps_same_element_event_handler() {
    use vize_croquis::{Analyzer, AnalyzerOptions};

    let script = r#"type UnionType = { type: "a" } | { type: "b", bSpecific: () => void }
const val = 0 as unknown as UnionType;
"#;
    let template = r#"<div v-if="val.type === 'b'" @click="val.bSpecific"></div>"#;

    let allocator = vize_carton::Allocator::new();
    let (root, _) = vize_armature::parse(&allocator, template);

    let mut analyzer = Analyzer::with_options(AnalyzerOptions::full());
    analyzer.analyze_script_setup(script);
    analyzer.analyze_template(&root);
    let summary = analyzer.finish();

    let output = generate_virtual_ts(&summary, Some(script), Some(&root), 0);

    assert!(
        output
            .code
            .contains("if ((val.type === 'b')) {\n      const __vize_handler"),
        "same-element event handler should preserve the v-if guard while type-checking the handler:\n{}",
        output.code
    );
}

#[test]
fn test_inline_arrow_event_handler_is_called_with_event() {
    use vize_croquis::{Analyzer, AnalyzerOptions};

    let template = r#"<button @click="(payload) => console.log(payload)">Click</button>"#;

    let allocator = vize_carton::Allocator::new();
    let (root, _) = vize_armature::parse(&allocator, template);

    let mut analyzer = Analyzer::with_options(AnalyzerOptions::full());
    analyzer.analyze_template(&root);
    let summary = analyzer.finish();

    let output = generate_virtual_ts(&summary, None, Some(&root), 0);

    assert!(
        output
            .code
            .contains("((payload) => console.log(payload))($event);"),
        "inline arrow handler should be invoked with the event:\n{}",
        output.code
    );
    assert!(
        !output
            .code
            .contains("(payload) => console.log(payload);  // handler expression"),
        "inline arrow handler must not be emitted as a void expression:\n{}",
        output.code
    );
}

#[test]
fn test_inline_arrow_event_handler_body_can_reference_dollar_event() {
    use vize_croquis::{Analyzer, AnalyzerOptions};

    // Repro for #2224: when a user writes an inline arrow handler whose body
    // references `$event` (e.g. mixing the explicit callback parameter with the
    // implicit Vue event alias), the generated TS must declare `$event` in the
    // scope that wraps the inline-callback invocation. Without this inner wrap,
    // the user's reference to `$event` inside the arrow body can be reported as
    // `TS2552: Cannot find name '$event'` in nested-scope refactors of the
    // virtual TS, so pin the binding immediately around the user's callback.
    let script = r#"function handleInput(_a: Event, _b: Event) { void _a; void _b; }
"#;
    let template = r#"<input @input="(e) => handleInput($event, e)" />"#;

    let allocator = vize_carton::Allocator::new();
    let (root, _) = vize_armature::parse(&allocator, template);

    let mut analyzer = Analyzer::with_options(AnalyzerOptions::full());
    analyzer.analyze_script_setup(script);
    analyzer.analyze_template(&root);
    let summary = analyzer.finish();

    let output = generate_virtual_ts(&summary, Some(script), Some(&root), 0);

    assert!(
        output.code.contains(
            "(($event: InputEvent) => { ((e) => handleInput($event, e))($event); })($event);"
        ),
        "inline arrow handler invocation must be wrapped in a closure that \
         re-declares `$event` (#2224):\n{}",
        output.code
    );
}

#[test]
fn test_computed_member_event_handler_reference_is_called_with_event() {
    use vize_croquis::{Analyzer, AnalyzerOptions};

    let script = r#"const handlers = { x: 42 } as const
const arr = [(event: PointerEvent) => event.preventDefault()]
"#;
    let template =
        r#"<button @click="handlers['x']">Bad</button><button @click="arr[0]">Good</button>"#;

    let allocator = vize_carton::Allocator::new();
    let (root, _) = vize_armature::parse(&allocator, template);

    let mut analyzer = Analyzer::with_options(AnalyzerOptions::full());
    analyzer.analyze_script_setup(script);
    analyzer.analyze_template(&root);
    let summary = analyzer.finish();

    let output = generate_virtual_ts(&summary, Some(script), Some(&root), 0);

    assert!(
        output.code.contains(
            "((__vize_cb: ((_e: PointerEvent) => unknown) | null | undefined) => __vize_cb)((handlers['x']));"
        ),
        "computed-member handler references should be checked as callable:\n{}",
        output.code
    );
    assert!(
        output
            .code
            .contains("((__vize_cb: ((_e: PointerEvent) => unknown) | null | undefined) => __vize_cb)((arr[0]));"),
        "index handler references should be checked as callable:\n{}",
        output.code
    );
    assert!(
        !output
            .code
            .contains("handlers['x'];  // handler expression"),
        "computed-member handler must not be emitted as a bare statement:\n{}",
        output.code
    );
}

#[test]
fn test_component_event_fallback_uses_any_when_args_stay_unknown() {
    use vize_croquis::{Analyzer, AnalyzerOptions};

    let script = r#"import Child from './Child.vue'
function eventHandler(event: Event) {
  void event
}
"#;
    let template = r#"<Child @keydown="eventHandler" />"#;

    let allocator = vize_carton::Allocator::new();
    let (root, _) = vize_armature::parse(&allocator, template);

    let mut analyzer = Analyzer::with_options(AnalyzerOptions::full());
    analyzer.analyze_script_setup(script);
    analyzer.analyze_template(&root);
    let summary = analyzer.finish();

    let standard_output = generate_virtual_ts_with_offsets(
        &summary,
        Some(script),
        Some(&root),
        0,
        0,
        &VirtualTsOptions::default(),
    );
    assert!(
        standard_output.code.contains("unknown[] extends __Child_")
            && standard_output.code.contains("? any : __Child_"),
        "standard component event fallback should stay loose when args stay unknown:\n{}",
        standard_output.code
    );

    let quirks_output = generate_virtual_ts_with_offsets_and_checks(
        &summary,
        Some(script),
        Some(&root),
        0,
        0,
        &VirtualTsOptions::default(),
        VirtualTsGenerationOptions {
            template_syntax_quirks: true,
            ..Default::default()
        },
    );
    assert!(
        quirks_output.code.contains("unknown[] extends __Child_")
            && quirks_output.code.contains("? any : __Child_"),
        "quirks component event fallback should stay loose when args stay unknown:\n{}",
        quirks_output.code
    );
}

#[test]
fn test_multiline_statement_event_handler_uses_handler_scope() {
    use vize_croquis::{Analyzer, AnalyzerOptions};

    let script = r#"const keys = ['a']
function selectWord(key: string) {}
function editWord() {}
"#;
    let template = r#"<button
  v-for="key in keys"
  @click.stop="
    selectWord(key);
    editWord();
  "
>edit</button>"#;

    let allocator = vize_carton::Allocator::new();
    let (root, _) = vize_armature::parse(&allocator, template);

    let mut analyzer = Analyzer::with_options(AnalyzerOptions::full());
    analyzer.analyze_script_setup(script);
    analyzer.analyze_template(&root);
    let summary = analyzer.finish();

    let output = generate_virtual_ts(&summary, Some(script), Some(&root), 0);

    assert!(
        output.code.contains("// @click handler"),
        "statement-list handlers should get an event handler scope:\n{}",
        output.code
    );
    assert!(
        output.code.contains("selectWord(key);") && output.code.contains("editWord();"),
        "handler statements should be preserved:\n{}",
        output.code
    );
    assert!(
        !output.code.contains("void (selectWord(key);")
            && !output.code.contains("void (\n    selectWord(key);"),
        "statement-list handlers must not be emitted as parenthesized expressions:\n{}",
        output.code
    );
}

#[test]
fn test_object_form_v_on_is_preserved_as_expression() {
    use vize_croquis::{Analyzer, AnalyzerOptions};

    let script = r#"const props = defineProps<{
  handlers?: {
    'update:modelValue'?: () => void
  }
}>()
"#;
    let template = r#"<button v-on="{ 'update:modelValue': props.handlers?.['update:modelValue'] }">Click</button>"#;

    let allocator = vize_carton::Allocator::new();
    let (root, _) = vize_armature::parse(&allocator, template);

    let mut analyzer = Analyzer::with_options(AnalyzerOptions::full());
    analyzer.analyze_script_setup(script);
    analyzer.analyze_template(&root);
    let summary = analyzer.finish();

    let output = generate_virtual_ts(&summary, Some(script), Some(&root), 0);

    assert!(
        output.code.contains(
            "void ({ 'update:modelValue': props.handlers?.['update:modelValue'] }); // VOn"
        ),
        "object-form v-on should be emitted as an expression:\n{}",
        output.code
    );
    assert!(
        !output.code.contains("@unknown handler"),
        "object-form v-on must not create a synthetic event handler:\n{}",
        output.code
    );
}

#[test]
fn test_source_mappings_generated() {
    use vize_croquis::{Analyzer, AnalyzerOptions};

    let script = r#"import { ref } from 'vue'
const msg = ref('Hello')
"#;
    let template = r#"<div>{{ msg }}</div>"#;

    let allocator = vize_carton::Allocator::new();
    let (root, _) = vize_armature::parse(&allocator, template);

    let mut analyzer = Analyzer::with_options(AnalyzerOptions::full());
    analyzer.analyze_script_setup(script);
    analyzer.analyze_template(&root);
    let summary = analyzer.finish();

    let output = generate_virtual_ts(&summary, Some(script), Some(&root), 0);

    // Should have at least one mapping for the template expression
    assert!(
        !output.mappings.is_empty(),
        "Should generate source mappings for template expressions"
    );
    // All mappings should have valid ranges
    for mapping in &output.mappings {
        assert!(
            mapping.gen_range.start < mapping.gen_range.end,
            "Generated range should be non-empty"
        );
        assert!(
            mapping.src_range.start < mapping.src_range.end,
            "Source range should be non-empty"
        );
    }
}

#[test]
fn test_source_mappings_target_expression_text() {
    use vize_croquis::{Analyzer, AnalyzerOptions};

    let script = r#"import { useTemplateRef } from 'vue'
const inputRef = useTemplateRef<HTMLInputElement>('input')
"#;
    let template = r#"<div :data-active="inputRef && inputRef.focus()"></div>"#;

    let allocator = vize_carton::Allocator::new();
    let (root, _) = vize_armature::parse(&allocator, template);

    let mut analyzer = Analyzer::with_options(AnalyzerOptions::full());
    analyzer.analyze_script_setup(script);
    analyzer.analyze_template(&root);
    let summary = analyzer.finish();

    let output = generate_virtual_ts(&summary, Some(script), Some(&root), 0);

    let expression = "inputRef && inputRef.focus()";
    let source_start = template.find(expression).unwrap();
    let source_end = source_start + expression.len();
    let mappings = output.mappings.iter();
    let mapping = mappings
        .flat_map(|mapping| &mapping.sub_spans)
        .find(|span| span.src_range == (source_start..source_end))
        .expect("the native prop check should map the authored expression sub-span");

    assert_eq!(&output.code[mapping.gen_range.clone()], expression);
}

#[test]
fn test_template_shadow_bindings_only_unwrap_vue_refs() {
    use vize_croquis::{Analyzer, AnalyzerOptions};

    let script = r#"import { ref, useTemplateRef } from 'vue'
const users = ref([{ id: 1 }])
const inputRef = useTemplateRef<HTMLInputElement>('input')
"#;
    let template = r#"<div>{{ users.length }} {{ inputRef && inputRef.focus() }}</div>"#;

    let allocator = vize_carton::Allocator::new();
    let (root, _) = vize_armature::parse(&allocator, template);

    let mut analyzer = Analyzer::with_options(AnalyzerOptions::full());
    analyzer.analyze_script_setup(script);
    analyzer.analyze_template(&root);
    let summary = analyzer.finish();

    let output = generate_virtual_ts(&summary, Some(script), Some(&root), 0);

    assert_virtual_ts_snapshot("virtual_ts_template_binding_unwraps", output.code.as_str());
}

#[test]
fn test_virtual_ts_generation_survives_unicode_script_comments() {
    use vize_croquis::{Analyzer, AnalyzerOptions};

    let script = r#"const reasgnSubMenuOpen = debounce(() => {
  console.log(1222222222222222222222222222222);
}, 100);

// あいうえおかきくけこさしすせそたちつてとなにぬねの
const heightLimit = "65vh";
// はひふへほまみむめもやいゆえよらりるれろわをん
"#;
    let template = r#"<div>{{ heightLimit }}</div>"#;

    let allocator = vize_carton::Allocator::new();
    let (root, _) = vize_armature::parse(&allocator, template);

    let mut analyzer = Analyzer::with_options(AnalyzerOptions::full());
    analyzer.analyze_script_setup(script);
    analyzer.analyze_template(&root);
    let summary = analyzer.finish();

    let output = generate_virtual_ts(&summary, Some(script), Some(&root), 0);

    assert!(output.code.contains("heightLimit"));
}

#[test]
fn test_script_setup_type_reexport_lifted_to_module_scope() {
    // `export type { X }` re-exports must be emitted at module top level,
    // not inside `__setup()` where `export` is a syntax error (TS1233).
    use vize_croquis::{Analyzer, AnalyzerOptions};

    let script = r#"import { type FilterType } from './ReExportType'

export type { FilterType }

defineProps<{ kind?: FilterType }>()
"#;

    let mut analyzer = Analyzer::with_options(AnalyzerOptions::full());
    analyzer.analyze_script_setup(script);
    let summary = analyzer.finish();

    let output =
        generate_virtual_ts_with_offsets(&summary, Some(script), None, 0, 0, &Default::default());

    let (module_scope, setup_scope) = output
        .code
        .split_once("// ========== Setup Scope ==========")
        .expect("setup scope marker present");

    assert!(
        module_scope.contains("export type { FilterType }"),
        "re-export should be lifted to module scope:\n{}",
        output.code
    );
    assert!(
        !setup_scope.contains("export type { FilterType }"),
        "re-export must not be trapped inside __setup():\n{}",
        output.code
    );
}

#[test]
fn test_vfor_component_props_in_scope() {
    // Component inside v-for should have prop checks inside the forEach closure
    use vize_croquis::{Analyzer, AnalyzerOptions};

    let script = r#"import { ref } from 'vue'
import TodoItem from './TodoItem.vue'

const todos = ref([{ id: 1, text: 'Hello' }])
"#;
    let template = r#"<div>
  <TodoItem v-for="todo in todos" :key="todo.id" :item="todo" />
</div>"#;

    let allocator = vize_carton::Allocator::new();
    let (root, _) = vize_armature::parse(&allocator, template);

    let mut analyzer = Analyzer::with_options(AnalyzerOptions::full());
    analyzer.analyze_script_setup(script);
    analyzer.analyze_template(&root);
    let summary = analyzer.finish();

    let output = generate_virtual_ts(&summary, Some(script), Some(&root), 0);

    assert_virtual_ts_snapshot(
        "virtual_ts_vfor_component_props_in_scope",
        output.code.as_str(),
    );
}

#[test]
fn test_vfor_component_props_source_nested_in_vif_is_wrapped_for_narrowing() {
    // Regression for #1565: component prop checks have their own v-for loop.
    // That loop must receive the same enclosing v-if wrapper as template
    // expression checks so the v-for source (`elems[key]`) is evaluated under
    // the narrowed `key === 'b'` type.
    use vize_croquis::{Analyzer, AnalyzerOptions};

    let script = r#"import Test from "./test.vue";
const elems = { a: [1], b: ["a"] } as const;
const key = "a" as "a" | "b";"#;
    let template = r#"<div v-if="key === 'b'">
  <button v-for="value in elems[key]" :key="value">
    <Test :value1="value" />
  </button>
</div>"#;

    let allocator = vize_carton::Allocator::new();
    let (root, _) = vize_armature::parse(&allocator, template);

    let mut analyzer = Analyzer::with_options(AnalyzerOptions::full());
    analyzer.analyze_script_setup(script);
    analyzer.analyze_template(&root);
    let summary = analyzer.finish();

    let output = generate_virtual_ts(&summary, Some(script), Some(&root), 0);

    assert_virtual_ts_snapshot(
        "virtual_ts_vfor_component_props_source_nested_in_vif_is_wrapped_for_narrowing",
        output.code.as_str(),
    );
}

#[test]
fn test_component_prop_checks_respect_same_element_vif_guard() {
    use vize_croquis::{Analyzer, AnalyzerOptions};

    let script = r#"import { ref } from 'vue'
import LinkComp from './LinkComp.vue'

const item = ref<{ name: string } | undefined>()
"#;
    let template = r#"<LinkComp v-if="item" :to="item.name" />"#;

    let allocator = vize_carton::Allocator::new();
    let (root, _) = vize_armature::parse(&allocator, template);

    let mut analyzer = Analyzer::with_options(AnalyzerOptions::full());
    analyzer.analyze_script_setup(script);
    analyzer.analyze_template(&root);
    let summary = analyzer.finish();

    let output = generate_virtual_ts(&summary, Some(script), Some(&root), 0);

    assert_virtual_ts_snapshot(
        "virtual_ts_component_prop_checks_respect_same_element_vif_guard",
        output.code.as_str(),
    );
}

#[test]
fn test_plain_options_object_default_export_is_wrapped_with_define_component() {
    use vize_croquis::{Analyzer, AnalyzerOptions};

    let script = r#"export default {
  data() {
    return { count: 0 }
  },
  computed: {
    doubled() {
      return this.count * 2
    },
  },
}
"#;
    let mut analyzer = Analyzer::with_options(AnalyzerOptions::full());
    analyzer.analyze_script_plain(script);
    let summary = analyzer.finish();

    let output =
        generate_virtual_ts_with_offsets(&summary, Some(script), None, 0, 0, &Default::default());

    assert!(
        output
            .code
            .contains("declare const __vizeDefineComponent: typeof import('vue').defineComponent;"),
        "expected the defineComponent helper declaration:\n{}",
        output.code
    );
    assert!(
        output
            .code
            .contains("const __default__ = __vizeDefineComponent({"),
        "expected the options object to be wrapped with defineComponent:\n{}",
        output.code
    );
    assert!(
        output.code.contains("\n  })\n"),
        "expected the wrap to be closed after the options object:\n{}",
        output.code
    );
}

#[test]
fn test_single_line_options_object_default_export_wrap_keeps_trailing_text() {
    use vize_croquis::{Analyzer, AnalyzerOptions};

    let script = "export default { name: 'Foo' };\n";
    let mut analyzer = Analyzer::with_options(AnalyzerOptions::full());
    analyzer.analyze_script_plain(script);
    let summary = analyzer.finish();

    let output =
        generate_virtual_ts_with_offsets(&summary, Some(script), None, 0, 0, &Default::default());

    assert!(
        output
            .code
            .contains("const __default__ = __vizeDefineComponent({ name: 'Foo' });"),
        "expected a single-line wrap that preserves the trailing semicolon:\n{}",
        output.code
    );
}

#[test]
fn test_define_component_default_export_is_not_double_wrapped() {
    use vize_croquis::{Analyzer, AnalyzerOptions};

    let script = r#"import { defineComponent } from 'vue'

export default defineComponent({
  data() {
    return { count: 0 }
  },
})
"#;
    let mut analyzer = Analyzer::with_options(AnalyzerOptions::full());
    analyzer.analyze_script_plain(script);
    let summary = analyzer.finish();

    let output =
        generate_virtual_ts_with_offsets(&summary, Some(script), None, 0, 0, &Default::default());

    assert!(
        !output.code.contains("__vizeDefineComponent"),
        "an explicit defineComponent call must not be re-wrapped:\n{}",
        output.code
    );
    assert!(
        output
            .code
            .contains("const __default__ = defineComponent({"),
        "expected the existing rewrite to stay untouched:\n{}",
        output.code
    );
}

#[test]
fn test_non_object_default_export_is_not_wrapped() {
    use vize_croquis::{Analyzer, AnalyzerOptions};

    let script = r#"const component = { name: 'Foo' }
export default component
"#;
    let mut analyzer = Analyzer::with_options(AnalyzerOptions::full());
    analyzer.analyze_script_plain(script);
    let summary = analyzer.finish();

    let output =
        generate_virtual_ts_with_offsets(&summary, Some(script), None, 0, 0, &Default::default());

    assert!(
        !output.code.contains("__vizeDefineComponent"),
        "identifier default exports must not be wrapped:\n{}",
        output.code
    );
    assert!(
        output.code.contains("const __default__ = component"),
        "expected the existing rewrite to stay untouched:\n{}",
        output.code
    );
}

#[test]
fn test_component_event_listener_uses_full_emit_arg_tuple() {
    use vize_croquis::{Analyzer, AnalyzerOptions};

    // A child component emit declared as a multi-element tuple must type its
    // parent `@event` listeners against the FULL argument tuple, not just the
    // first element (regression test for #1512). Both a bare callable reference
    // and an inline arrow with multiple parameters must be checked against the
    // synthesized listener type and invoked with every argument spread into a
    // rest parameter.
    let script = r#"import Test from './Test.vue'
function handleTest(value1: string, value2: number) {
  void value1
  void value2
}
"#;
    let template = r#"<Test @test="handleTest" /><Test @test="(value1, value2) => handleTest(value1, value2)" />"#;

    let allocator = vize_carton::Allocator::new();
    let (root, _) = vize_armature::parse(&allocator, template);

    let mut analyzer = Analyzer::with_options(AnalyzerOptions::full());
    analyzer.analyze_script_setup(script);
    analyzer.analyze_template(&root);
    let summary = analyzer.finish();

    let output = generate_virtual_ts(&summary, Some(script), Some(&root), 0);

    // The listener type expands to the full emit argument tuple (and falls back
    // to variadic arguments when the emit stays unresolved).
    assert!(
        output.code.contains(
            "type __Test_8_test_listener = unknown[] extends __Test_8_test_args ? ((...args: any[]) => any) : ((...args: __Test_8_test_args) => any);"
        ),
        "component event listener must expand to the full emit argument tuple:\n{}",
        output.code
    );

    // The closure receives every emit argument via a rest parameter typed by
    // `Parameters<listener>`, instead of a single `$event` parameter.
    assert!(
        output
            .code
            .contains("((...__vize_args: Parameters<__Test_8_test_listener>) => {"),
        "component event closure must receive the full listener parameter tuple:\n{}",
        output.code
    );
    assert!(
        !output.code.contains("(($event: __Test_8_test_event) => {"),
        "component event closure must not collapse the emit tuple to a single \
         $event parameter:\n{}",
        output.code
    );

    assert!(
        output.code.contains(
            "type __Test_8_test_handler = unknown[] extends __Test_8_test_args ? ((...args: any[]) => any) : __Test_8_test_listener;"
        ) && output.code.contains(
            "const __vize_handler_8_13: __Test_8_test_handler | null | undefined = (handleTest);"
        ),
        "bare handler reference must be typed through the emit handler alias:\n{}",
        output.code
    );
    assert!(
        output
            .code
            .contains("(__vize_handler_8_13 as __Test_8_test_listener)(...__vize_args);"),
        "bare handler reference must be invoked with the full argument spread:\n{}",
        output.code
    );
    assert!(
        output.code.contains(
            "const __vize_handler_9_40: __Test_9_test_handler | null | undefined = ((value1, value2) => handleTest(value1, value2));"
        ),
        "inline multi-arg arrow must be typed through the emit handler alias:\n{}",
        output.code
    );
    assert!(
        output
            .code
            .contains("(__vize_handler_9_40 as __Test_9_test_listener)(...__vize_args);"),
        "inline multi-arg arrow must be invoked with the full argument spread:\n{}",
        output.code
    );
    assert!(
        !output
            .code
            .contains("(value1, value2) => handleTest(value1, value2))($event)"),
        "inline multi-arg arrow must not be invoked with only the first \
         emit argument:\n{}",
        output.code
    );
}

#[test]
fn test_native_event_handler_keeps_single_event_parameter() {
    use vize_croquis::{Analyzer, AnalyzerOptions};

    // Native DOM events must keep the single `$event` parameter typed by the DOM
    // event type; the emit-tuple expansion only applies to component `@event`
    // listeners (guards against #1512 regressing native handlers).
    let script = r#"function handleClick(event: PointerEvent) {
  void event
}
"#;
    let template = r#"<button @click="handleClick">Click</button>"#;

    let allocator = vize_carton::Allocator::new();
    let (root, _) = vize_armature::parse(&allocator, template);

    let mut analyzer = Analyzer::with_options(AnalyzerOptions::full());
    analyzer.analyze_script_setup(script);
    analyzer.analyze_template(&root);
    let summary = analyzer.finish();

    let output = generate_virtual_ts(&summary, Some(script), Some(&root), 0);

    assert!(
        output.code.contains("(($event: PointerEvent) => {"),
        "native event handler must keep the single $event parameter:\n{}",
        output.code
    );
    assert!(
        output
            .code
            .contains("((__vize_cb: ((_e: PointerEvent) => unknown) | null | undefined) => __vize_cb)((handleClick));"),
        "native event handler reference must be typed by the DOM event:\n{}",
        output.code
    );
    assert!(
        !output.code.contains("...__vize_args"),
        "native event handler must not use the emit argument tuple spread:\n{}",
        output.code
    );
}
