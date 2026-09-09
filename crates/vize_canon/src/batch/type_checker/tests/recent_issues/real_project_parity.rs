//! Focused parity cases reduced from real-project divergence shards.

use super::super::{create_project_case, resolve_test_tsgo_binary, snapshot_project_diagnostics};
use super::diagnostic_normalization::normalize_target_parameter_names;

#[test]
fn runtime_constructor_define_model_infers_defaulted_string_value() {
    if resolve_test_tsgo_binary().is_none() {
        return;
    }
    let project_root = create_project_case(
        "define-model-runtime-string",
        &[(
            "src/App.vue",
            r#"<script setup lang="ts">
const model = defineModel({ default: "", type: String })
const value: string = model.value
void value
</script>
"#,
        )],
    );

    let snapshot = snapshot_project_diagnostics(&project_root);
    let _ = std::fs::remove_dir_all(&project_root);

    assert_eq!(
        snapshot,
        Some(Vec::new()),
        "runtime defineModel should infer string, got: {snapshot:#?}"
    );
}

#[test]
fn runtime_constructor_define_model_keeps_undefined_when_not_resolved() {
    if resolve_test_tsgo_binary().is_none() {
        return;
    }
    let project_root = create_project_case(
        "define-model-runtime-optional-string",
        &[(
            "src/App.vue",
            r#"<script setup lang="ts">
const model = defineModel({ type: String })
const value: string | undefined = model.value
void value
</script>
"#,
        )],
    );

    let snapshot = snapshot_project_diagnostics(&project_root);
    let _ = std::fs::remove_dir_all(&project_root);

    assert_eq!(
        snapshot,
        Some(Vec::new()),
        "runtime defineModel without a default should preserve undefined, got: {snapshot:#?}"
    );
}

#[test]
fn script_bound_unresolved_component_events_are_contextually_any() {
    if resolve_test_tsgo_binary().is_none() {
        return;
    }
    let project_root = create_project_case(
        "script-bound-unresolved-component-inline-event-callback",
        &[
            (
                "src/menu.ts",
                "export const NormalMenu: unknown = undefined as unknown\n",
            ),
            (
                "src/App.vue",
                r#"<script setup lang="ts">
import { NormalMenu } from './menu'

const emit = defineEmits<{ enter: [value: unknown] }>()
</script>

<template>
  <NormalMenu @enter="(menu) => emit('enter', menu)" />
</template>
"#,
            ),
        ],
    );

    let snapshot = normalize_target_parameter_names(snapshot_project_diagnostics(&project_root));
    let _ = std::fs::remove_dir_all(&project_root);

    assert_eq!(
        snapshot,
        Some(Vec::new()),
        "unresolved component event callback params should not report TS7006: {snapshot:#?}"
    );
}

#[test]
fn script_bound_resolved_component_events_still_report_bad_handlers() {
    if resolve_test_tsgo_binary().is_none() {
        return;
    }
    let project_root = create_project_case(
        "script-bound-resolved-component-bad-handler",
        &[
            (
                "src/Child.vue",
                r#"<script setup lang="ts">
defineEmits<{ change: [value: string] }>()
</script>
"#,
            ),
            (
                "src/App.vue",
                r#"<script setup lang="ts">
import Child from './Child.vue'
function wrong(value: number) {
  void value
}
</script>

<template>
  <Child @change="wrong" />
</template>
"#,
            ),
        ],
    );

    let snapshot = snapshot_project_diagnostics(&project_root);
    let _ = std::fs::remove_dir_all(&project_root);

    assert!(
        snapshot.as_ref().is_some_and(|diagnostics| {
            diagnostics.iter().any(|(_, code, message)| {
                *code == Some(2322) && message.contains("number") && message.contains("string")
            })
        }),
        "resolved component events must still report incompatible handlers: {snapshot:#?}"
    );
}

#[test]
fn dynamic_slot_outlet_spread_accepts_unknown_payload_objects() {
    if resolve_test_tsgo_binary().is_none() {
        return;
    }
    let project_root = create_project_case(
        "dynamic-slot-outlet-spread-unknown",
        &[(
            "src/App.vue",
            r#"<script setup lang="ts">
defineSlots<{ [name: string]: (props: unknown) => unknown }>()
const slotName = 'default'
const slotProps = undefined as unknown
</script>

<template>
  <slot :name="slotName" v-bind="slotProps" />
</template>
"#,
        )],
    );

    let snapshot = snapshot_project_diagnostics(&project_root);
    let _ = std::fs::remove_dir_all(&project_root);

    assert_eq!(
        snapshot,
        Some(Vec::new()),
        "dynamic slot outlet spread should not report TS2698, got: {snapshot:#?}"
    );
}

#[test]
fn dynamic_slot_outlet_spread_still_checks_explicit_props() {
    if resolve_test_tsgo_binary().is_none() {
        return;
    }
    let project_root = create_project_case(
        "dynamic-slot-outlet-spread-explicit-prop",
        &[(
            "src/App.vue",
            r#"<script setup lang="ts">
defineSlots<{ default(props: { count: number }): unknown }>()
const slotProps = undefined as unknown
</script>

<template>
  <slot v-bind="slotProps" :count="'bad'" />
</template>
"#,
        )],
    );

    let snapshot = snapshot_project_diagnostics(&project_root);
    let _ = std::fs::remove_dir_all(&project_root);

    assert!(
        snapshot.as_ref().is_some_and(|diagnostics| {
            diagnostics.iter().any(|(_, code, message)| {
                *code == Some(2322) && message.contains("string") && message.contains("number")
            })
        }),
        "explicit slot outlet props must remain checked after spreads: {snapshot:#?}"
    );
}

#[test]
fn unconfigured_alias_vue_type_import_preserves_the_authored_specifier() {
    if resolve_test_tsgo_binary().is_none() {
        return;
    }
    let project_root = create_project_case(
        "unconfigured-alias-vue-type-import",
        &[(
            "src/App.vue",
            r#"<script setup lang="ts">
import type { Item } from "@/components/app/list/List.vue"
defineProps<{ items: Item[] }>()
</script>
"#,
        )],
    );

    let snapshot = snapshot_project_diagnostics(&project_root);
    let _ = std::fs::remove_dir_all(&project_root);

    assert_eq!(
        snapshot,
        Some(vec![(
            vize_s0::String::from("src/App.vue"),
            Some(2307),
            vize_s0::String::from(
                "2:27:error Cannot find module '@/components/app/list/List.vue' or its corresponding type declarations.",
            ),
        )]),
    );
}
