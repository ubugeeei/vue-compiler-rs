use super::generate_virtual_ts_with_offsets;
use vize_croquis::{Analyzer, AnalyzerOptions};

mod imported_props;

#[test]
fn test_define_props_typeof_setup_binding_deferred_to_setup_scope() {
    let script = r#"const someDefinition = {
  foo: 'fooVal',
} as const;

type SomeGenericType<T extends Record<string, unknown>> = {
  baz: string;
  items: T;
};

const props = defineProps<SomeGenericType<typeof someDefinition>>();
"#;

    let mut analyzer = Analyzer::with_options(AnalyzerOptions::full());
    analyzer.analyze_script_setup(script);
    let summary = analyzer.finish();

    let output =
        generate_virtual_ts_with_offsets(&summary, Some(script), None, 0, 0, &Default::default());
    let (module_scope, setup_and_after) = output
        .code
        .split_once("// ========== Setup Scope ==========")
        .expect("setup scope marker present");

    assert!(
        !module_scope.contains("typeof someDefinition"),
        "setup-scope value must not be referenced from module scope:\n{}",
        output.code
    );
    assert!(
        setup_and_after.contains("type __VizeSetupProps = SomeGenericType<typeof someDefinition>;"),
        "setup-local props artifact should preserve the concrete props type:\n{}",
        output.code
    );
    assert!(
        output.code.contains(
            "export type Props = Awaited<ReturnType<typeof __setup>>[\"__vize_setup_props\"];"
        ),
        "module Props should be exported through __setup return type:\n{}",
        output.code
    );
}

#[test]
fn test_deferred_define_props_does_not_redeclare_hoisted_props_type() {
    let script = r#"interface Props {
  baz: string;
}

const someDefinition = {
  foo: 'fooVal',
} as const;

defineProps<Props & { items: typeof someDefinition }>();
"#;

    let mut analyzer = Analyzer::with_options(AnalyzerOptions::full());
    analyzer.analyze_script_setup(script);
    let summary = analyzer.finish();

    let output =
        generate_virtual_ts_with_offsets(&summary, Some(script), None, 0, 0, &Default::default());

    assert!(
        output.code.contains("interface Props {\n  baz: string;\n}"),
        "existing hoisted Props should remain in module scope:\n{}",
        output.code
    );
    assert!(
        !output
            .code
            .contains("export type Props = Awaited<ReturnType<typeof __setup>>"),
        "deferred props must not redeclare an existing module-scope Props:\n{}",
        output.code
    );
    assert!(
        output.code.contains(
            "type __VizeResolvedProps = Awaited<ReturnType<typeof __setup>>[\"__vize_setup_props\"];"
        ),
        "default export should use a private resolved props alias:\n{}",
        output.code
    );
    assert!(
        output.code.contains("  $props: __VizeResolvedProps;"),
        "component instance should use the full deferred props type:\n{}",
        output.code
    );
}

#[test]
fn test_define_props_non_hoisted_type_ref_deferred_to_setup_scope() {
    let script = r#"type WidgetProps = {
  items: typeof widgetDefinitions;
};

const widgetDefinitions = {
  clock: {},
} as const;

defineProps<WidgetProps>();
"#;

    let mut analyzer = Analyzer::with_options(AnalyzerOptions::full());
    analyzer.analyze_script_setup(script);
    let summary = analyzer.finish();

    let output =
        generate_virtual_ts_with_offsets(&summary, Some(script), None, 0, 0, &Default::default());
    let (module_scope, setup_and_after) = output
        .code
        .split_once("// ========== Setup Scope ==========")
        .expect("setup scope marker present");

    assert!(
        !module_scope.contains("WidgetProps"),
        "non-hoisted setup type must not be referenced from module scope:\n{}",
        output.code
    );
    assert!(
        setup_and_after.contains("type __VizeSetupProps = WidgetProps;"),
        "setup-local props artifact should reference the non-hoisted type:\n{}",
        output.code
    );
}

#[test]
fn test_define_props_local_type_alias_is_visible_to_hoisted_props() {
    let script = r#"type FormViewState = 'input' | 'confirm' | 'complete';

interface Props {
  formViewState: FormViewState;
}

const props = defineProps<Props>();
"#;

    let mut analyzer = Analyzer::with_options(AnalyzerOptions::full());
    analyzer.analyze_script_setup(script);
    let summary = analyzer.finish();

    let output =
        generate_virtual_ts_with_offsets(&summary, Some(script), None, 0, 0, &Default::default());
    let (module_scope, setup_and_after) = output
        .code
        .split_once("// ========== Setup Scope ==========")
        .expect("setup scope marker present");

    assert!(
        module_scope.contains("type FormViewState = 'input' | 'confirm' | 'complete';"),
        "local type alias should be emitted before hoisted Props:\n{}",
        output.code
    );
    assert!(
        module_scope.contains("interface Props {\n  formViewState: FormViewState;\n}"),
        "hoisted Props should retain the local alias reference:\n{}",
        output.code
    );
    assert!(
        !setup_and_after.contains("type __VizeSetupProps"),
        "plain local type aliases should not force setup-scoped Props:\n{}",
        output.code
    );
}

#[test]
fn test_define_props_transitive_typeof_alias_stays_with_props_interface() {
    let script = r#"import { pickProperties } from '~/src/shared/objectOperationUtil';
import { FormViewStateEnum as _FormViewStateEnum } from '~/src/domain/models/form';

const FormViewStateEnum = pickProperties(_FormViewStateEnum, [
  _FormViewStateEnum.TextbookSelection_OtherStudentTextbook,
  _FormViewStateEnum.DetailForm_OtherStudentTextbook,
]);

type FormViewState = (typeof FormViewStateEnum)[keyof typeof FormViewStateEnum];

interface Props {
  formViewState: FormViewState;
  updateStepperStateToTextbookInput: (s: FormViewState) => void;
}

const props = defineProps<Props>();
"#;

    let mut analyzer = Analyzer::with_options(AnalyzerOptions::full());
    analyzer.analyze_script_setup(script);
    let summary = analyzer.finish();

    let output =
        generate_virtual_ts_with_offsets(&summary, Some(script), None, 0, 0, &Default::default());
    let (module_scope, setup_and_after) = output
        .code
        .split_once("// ========== Setup Scope ==========")
        .expect("setup scope marker present");

    assert!(
        !module_scope.contains("interface Props")
            && !module_scope.contains("type FormViewState =")
            && !module_scope.contains("formViewState: FormViewState"),
        "setup-derived type aliases must not leak into module scope:\n{}",
        output.code
    );
    assert!(
        setup_and_after.contains(
            "type FormViewState = (typeof FormViewStateEnum)[keyof typeof FormViewStateEnum];"
        ),
        "setup scope must retain the local alias derived from the local const:\n{}",
        output.code
    );
    assert!(
        setup_and_after.contains("interface Props {")
            && setup_and_after.contains("formViewState: FormViewState;")
            && setup_and_after
                .contains("updateStepperStateToTextbookInput: (s: FormViewState) => void;"),
        "Props must stay in the same setup scope as the alias it references:\n{}",
        output.code
    );
    assert!(
        setup_and_after.contains("type __VizeSetupProps = Props;"),
        "defineProps<Props>() must resolve through the setup-local Props artifact:\n{}",
        output.code
    );
}

#[test]
fn test_setup_local_type_alias_is_emitted_before_ref_type_usage() {
    let script = r#"import { ref } from 'vue';

const DialogOpenStatusEnum = {
  None: 'None',
  Confirm: 'Confirm',
} as const;

type DialogOpenStatus = (typeof DialogOpenStatusEnum)[keyof typeof DialogOpenStatusEnum];

const dialogOpenStatus = ref<DialogOpenStatus>(DialogOpenStatusEnum.None);
"#;

    let mut analyzer = Analyzer::with_options(AnalyzerOptions::full());
    analyzer.analyze_script_setup(script);
    let summary = analyzer.finish();

    let output =
        generate_virtual_ts_with_offsets(&summary, Some(script), None, 0, 0, &Default::default());
    let (module_scope, setup_and_after) = output
        .code
        .split_once("// ========== Setup Scope ==========")
        .expect("setup scope marker present");

    assert!(
        !module_scope.contains("DialogOpenStatus"),
        "setup-local type alias must not be emitted where its const is missing:\n{}",
        output.code
    );
    let alias_at = setup_and_after
        .find("type DialogOpenStatus = (typeof DialogOpenStatusEnum)")
        .expect("setup scope should contain DialogOpenStatus alias");
    let usage_at = setup_and_after
        .find("ref<DialogOpenStatus>(DialogOpenStatusEnum.None)")
        .expect("setup scope should contain ref typed with DialogOpenStatus");
    assert!(
        alias_at < usage_at,
        "local alias must be emitted before same-scope type usage:\n{}",
        output.code
    );
}
