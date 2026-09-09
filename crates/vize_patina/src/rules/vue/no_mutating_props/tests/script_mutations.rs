use super::{NoMutatingProps, NoMutatingPropsOptions, findings, lint_sfc, lint_sfc_with_rule};
use crate::{LintPreset, Linter, Severity};
use vize_s0::String;

fn expected_script_finding<'a>(
    sfc: &'a str,
    target: &'a str,
    mutation: &'a str,
    occurrence: usize,
) -> (&'static str, Severity, u32, u32, String) {
    let start = sfc
        .match_indices(mutation)
        .nth(occurrence)
        .map(|(start, _)| start)
        .expect("mutation target");
    (
        "vue/no-mutating-props",
        Severity::Error,
        start as u32,
        (start + mutation.len()) as u32,
        String::new(format!(
            "Unexpected mutation of prop '{target}' in <script setup>"
        )),
    )
}

#[test]
fn reports_assignment_compound_assignment_and_update_on_a_props_object() {
    let sfc = r#"<script setup lang="ts">
const props = defineProps<{ count: number; profile: { name: string } }>()
props.count = 1
props.count += 1
props.count++
props.profile.name = 'Ada'
</script>
"#;
    let result = lint_sfc(sfc);
    let actual = findings(&result);
    let expected = [
        expected_script_finding(sfc, "props.count", "props.count = 1", 0),
        expected_script_finding(sfc, "props.count", "props.count += 1", 0),
        expected_script_finding(sfc, "props.count", "props.count++", 0),
        expected_script_finding(sfc, "props.profile.name", "props.profile.name = 'Ada'", 0),
    ];

    assert_eq!(actual.len(), expected.len());
    for (actual, expected) in actual.iter().zip(expected.iter()) {
        assert_eq!(actual.0, expected.0);
        assert_eq!(actual.1, expected.1);
        assert_eq!(actual.2, expected.2);
        assert_eq!(actual.3, expected.3);
        assert_eq!(actual.4, expected.4);
    }
}

#[test]
fn reports_mutations_of_destructured_and_aliased_props() {
    let sfc = r#"<script setup lang="ts">
let { count, enabled: isEnabled } = defineProps<{
  count: number
  enabled: boolean
}>()
count = 1
isEnabled--
</script>
"#;
    let result = lint_sfc(sfc);
    let actual = findings(&result);
    let expected = [
        expected_script_finding(sfc, "count", "count = 1", 0),
        expected_script_finding(sfc, "isEnabled", "isEnabled--", 0),
    ];

    assert_eq!(actual.len(), expected.len());
    for (actual, expected) in actual.iter().zip(expected.iter()) {
        assert_eq!(
            (actual.2, actual.3, actual.4),
            (expected.2, expected.3, expected.4.as_str())
        );
    }
}

#[test]
fn reports_mutations_from_with_defaults() {
    let sfc = r#"<script setup lang="ts">
const props = withDefaults(defineProps<{ count?: number }>(), { count: 0 })
props.count *= 2
</script>
"#;
    let expected = expected_script_finding(sfc, "props.count", "props.count *= 2", 0);
    let result = lint_sfc(sfc);
    let actual = findings(&result);

    assert_eq!(actual.len(), 1);
    assert_eq!(
        (actual[0].2, actual[0].3, actual[0].4),
        (expected.2, expected.3, expected.4.as_str())
    );
}

#[test]
fn reports_delete_and_mutating_calls_on_props() {
    let sfc = r#"<script setup lang="ts">
const props = defineProps<{
  items: string[]
  profile: { name?: string }
  options: Record<string, boolean>
}>()
props.items.push('x')
props.items.splice(0, 1)
props.items["push"]('y')
Object.assign(props.options, { enabled: true })
Object["assign"](props.options, { archived: false })
delete props.profile.name
</script>
"#;
    let result = lint_sfc(sfc);
    let actual = findings(&result);
    let expected = [
        expected_script_finding(sfc, "props.items", "props.items.push('x')", 0),
        expected_script_finding(sfc, "props.items", "props.items.splice(0, 1)", 0),
        expected_script_finding(sfc, "props.items", "props.items[\"push\"]('y')", 0),
        expected_script_finding(
            sfc,
            "props.options",
            "Object.assign(props.options, { enabled: true })",
            0,
        ),
        expected_script_finding(
            sfc,
            "props.options",
            "Object[\"assign\"](props.options, { archived: false })",
            0,
        ),
        expected_script_finding(sfc, "props.profile.name", "delete props.profile.name", 0),
    ];

    assert_eq!(actual.len(), expected.len());
    for (actual, expected) in actual.iter().zip(expected.iter()) {
        assert_eq!(
            (actual.2, actual.3, actual.4),
            (expected.2, expected.3, expected.4.as_str())
        );
    }
}

#[test]
fn ignores_non_literal_computed_mutating_calls_on_props() {
    let sfc = r#"<script setup lang="ts">
const props = defineProps<{ items: string[]; options: Record<string, boolean> }>()
const method = 'push'
const assign = 'assign'
props.items[method]('x')
Object[assign](props.options, { enabled: true })
</script>
"#;

    assert!(findings(&lint_sfc(sfc)).is_empty());
}

#[test]
fn reports_mutating_calls_on_destructured_props() {
    let sfc = r#"<script setup lang="ts">
const { items } = defineProps<{ items: string[] }>()
items.sort()
</script>
"#;
    let result = lint_sfc(sfc);
    let actual = findings(&result);
    let expected = expected_script_finding(sfc, "items", "items.sort()", 0);

    assert_eq!(actual.len(), 1);
    assert_eq!(
        (actual[0].2, actual[0].3, actual[0].4),
        (expected.2, expected.3, expected.4.as_str())
    );
}

#[test]
fn shallow_only_reports_direct_script_mutations_but_allows_nested_values() {
    let sfc = r#"<script setup lang="ts">
const props = defineProps<{
  count: number
  items: string[]
  profile: { name?: string }
}>()
props.count = 1
props.profile.name = 'Ada'
props.items.push('x')
delete props.profile.name
</script>
"#;
    let result = lint_sfc_with_rule(
        sfc,
        NoMutatingProps::new(NoMutatingPropsOptions { shallow_only: true }),
    );
    let expected = expected_script_finding(sfc, "props.count", "props.count = 1", 0);
    let actual = findings(&result);

    assert_eq!(actual.len(), 1);
    assert_eq!(
        (actual[0].2, actual[0].3, actual[0].4),
        (expected.2, expected.3, expected.4.as_str())
    );
}

#[test]
fn ignores_unrelated_and_shadowed_bindings() {
    let sfc = r#"<script setup lang="ts">
const props = defineProps<{ count: number }>()
const local = { count: 0 }
local.count++

function mutate(props: { count: number }) {
  props.count += 1
}
</script>
"#;

    assert!(findings(&lint_sfc(sfc)).is_empty());
}

#[test]
fn ignores_a_user_defined_define_props_function() {
    let sfc = r#"<script setup lang="ts">
function defineProps<T>(): T {
  return {} as T
}
const ordinary = defineProps<{ count: number }>()
ordinary.count++
</script>
"#;

    assert!(findings(&lint_sfc(sfc)).is_empty());
}

#[test]
fn honors_an_sfc_level_eslint_disable_comment() {
    let sfc = r#"<script setup lang="ts">
/* eslint-disable vue/no-mutating-props */
const props = defineProps<{ count: number }>()
props.count++
</script>
"#;

    assert!(findings(&lint_sfc(sfc)).is_empty());
}

#[test]
fn runs_in_every_expected_preset() {
    let sfc = r#"<script setup lang="ts">
const props = defineProps<{ count: number }>()
props.count += 1
</script>
"#;

    for preset in [
        LintPreset::HappyPath,
        LintPreset::Essential,
        LintPreset::Opinionated,
        LintPreset::Ecosystem,
    ] {
        let result = Linter::with_preset(preset).lint_sfc(sfc, "Probe.vue");
        let count = result
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.rule_name == "vue/no-mutating-props")
            .count();
        assert_eq!(count, 1, "preset {preset:?}");
    }
}
