//! TS7006 parity for template callback parameters and handler references (#3756).
//!
//! The fixture shapes were reduced from `vue-tsc 3.3.11` with TypeScript 6.0.3
//! on byte-identical sources. Assertions pin Vize's code/anchor behavior and
//! keep negative rows so the fix cannot pass by filtering diagnostics.

use super::super::{create_project_case, resolve_test_tsgo_binary, snapshot_project_diagnostics};
use super::diagnostic_normalization::normalize_target_parameter_names;

#[test]
fn slot_outlet_callbacks_use_declared_slot_props() {
    if resolve_test_tsgo_binary().is_none() {
        return;
    }
    let project_root = create_project_case(
        "slot-outlet-callbacks-declared-props",
        &[(
            "src/App.vue",
            r#"<script setup lang="ts" generic="T extends { id: string }">
const slots = defineSlots<{
  default(props: { item: T; index: number; dragStart: (ev: DragEvent) => void }): unknown
}>()
const items = [{ id: "one" } as T]
function onDragstart(ev: DragEvent, item: T) {}
</script>

<template>
  <slot v-for="(item, i) in items" :item="item" :index="i" :dragStart="(ev) => onDragstart(ev, item)" />
  <slot :item="items[0]" :index="0" :dragStart="(ev) => ev.missing" />
</template>
"#,
        )],
    );

    let snapshot = normalize_target_parameter_names(snapshot_project_diagnostics(&project_root));
    let _ = std::fs::remove_dir_all(&project_root);

    assert_eq!(
        snapshot,
        Some(vec![(
            vize_s0::String::from("src/App.vue"),
            Some(2339),
            vize_s0::String::from(
                "11:60:error Property 'missing' does not exist on type 'DragEvent'.",
            ),
        )]),
    );
}

#[test]
fn untyped_slot_outlet_callbacks_remain_implicit_any() {
    if resolve_test_tsgo_binary().is_none() {
        return;
    }
    let project_root = create_project_case(
        "slot-outlet-callbacks-untyped",
        &[(
            "src/App.vue",
            r#"<template>
  <slot :dragStart="(ev) => ev" />
</template>
"#,
        )],
    );

    let snapshot = normalize_target_parameter_names(snapshot_project_diagnostics(&project_root));
    let _ = std::fs::remove_dir_all(&project_root);

    assert_eq!(
        snapshot,
        Some(vec![(
            vize_s0::String::from("src/App.vue"),
            Some(7006),
            vize_s0::String::from("2:22:error Parameter 'ev' implicitly has an 'any' type."),
        )]),
    );
}

#[test]
fn external_member_update_model_handlers_match_vue_tsc_listener_contract() {
    if resolve_test_tsgo_binary().is_none() {
        return;
    }
    let project_root = create_project_case(
        "external-member-update-model-handler-listener-contract",
        &[
            (
                "src/global-components.d.ts",
                r#"import type { DefineComponent } from "vue"
export {}
declare module "vue" {
  interface GlobalComponents {
    QSlider: DefineComponent<{ modelValue?: number | null; "onUpdate:modelValue"?: (value: number | null) => any }>
  }
}
"#,
            ),
            (
                "src/App.vue",
                r#"<script setup lang="ts">
const props = {
  qSliderProps: {
    modelValue: 1 as number | null,
    "onUpdate:modelValue": (value: number) => { value.toFixed() },
    stringOnly: (value: string) => value.toUpperCase(),
  }
}
</script>
<template>
  <QSlider :modelValue="props.qSliderProps.modelValue" @update:modelValue="props.qSliderProps['onUpdate:modelValue']" />
  <QSlider :modelValue="props.qSliderProps.modelValue" @update:modelValue="props.qSliderProps.stringOnly" />
</template>
"#,
            ),
        ],
    );

    let snapshot = normalize_target_parameter_names(snapshot_project_diagnostics(&project_root));
    let _ = std::fs::remove_dir_all(&project_root);

    assert_eq!(
        snapshot,
        Some(vec![
            (
                vize_s0::String::from("src/App.vue"),
                Some(2322),
                vize_s0::String::from(
                    "11:57:error Type '(value: number) => void' is not assignable to type '(value: number | null) => any'.\n\
                     Types of parameters 'value' and '<target>' are incompatible.\n\
                     Type 'number | null' is not assignable to type 'number'.\n\
                     Type 'null' is not assignable to type 'number'.",
                ),
            ),
            (
                vize_s0::String::from("src/App.vue"),
                Some(2322),
                vize_s0::String::from(
                    "12:57:error Type '(value: string) => string' is not assignable to type '(value: number | null) => any'.\n\
                     Types of parameters 'value' and '<target>' are incompatible.\n\
                     Type 'number | null' is not assignable to type 'string'.\n\
                     Type 'null' is not assignable to type 'string'.",
                ),
            ),
        ]),
    );
}

#[test]
fn local_sfc_update_model_identifier_handlers_stay_strict() {
    if resolve_test_tsgo_binary().is_none() {
        return;
    }
    let project_root = create_project_case(
        "local-sfc-update-model-identifier-handler-strict",
        &[
            (
                "src/NullableInput.vue",
                r#"<script setup lang="ts">
defineProps<{ modelValue: number | null }>()
defineEmits<{ "update:modelValue": [value: number | null] }>()
</script>
<template><input /></template>
"#,
            ),
            (
                "src/App.vue",
                r#"<script setup lang="ts">
import NullableInput from "./NullableInput.vue"
const selected = 1 as number | null
function numberOnly(value: number) { value.toFixed() }
</script>
<template>
  <NullableInput :modelValue="selected" @update:modelValue="numberOnly" />
</template>
"#,
            ),
        ],
    );

    let snapshot = normalize_target_parameter_names(snapshot_project_diagnostics(&project_root));
    let _ = std::fs::remove_dir_all(&project_root);

    assert_eq!(
        snapshot,
        Some(vec![(
            vize_s0::String::from("src/App.vue"),
            Some(2322),
            vize_s0::String::from(
                "7:42:error Type '(value: number) => void' is not assignable to type '(value: number | null) => any'.\n\
                 Types of parameters 'value' and '<target>' are incompatible.\n\
                 Type 'number | null' is not assignable to type 'number'.\n\
                 Type 'null' is not assignable to type 'number'.",
            ),
        )]),
    );
}

#[test]
fn transition_group_hook_handlers_are_typed_as_elements() {
    if resolve_test_tsgo_binary().is_none() {
        return;
    }
    let project_root = create_project_case(
        "transition-group-hook-handler-elements",
        &[(
            "src/App.vue",
            r#"<script setup lang="ts">
function beforeEnter(el: Element) { el.getBoundingClientRect() }
function enter(el: Element, done: () => void) { done() }
function wrongTransition(el: string) { el.toUpperCase() }
</script>

<template>
  <TransitionGroup
    @before-enter="beforeEnter"
    @enter="enter"
    @after-enter="(el) => el.missing"
    @before-leave="wrongTransition"
  />
</template>
"#,
        )],
    );

    let snapshot = normalize_target_parameter_names(snapshot_project_diagnostics(&project_root));
    let _ = std::fs::remove_dir_all(&project_root);

    assert_eq!(
        snapshot,
        Some(vec![
            (
                vize_s0::String::from("src/App.vue"),
                Some(2322),
                vize_s0::String::from(
                    "12:6:error Type '(el: string) => void' is not assignable to type '(el: Element) => any'.\n\
                     Types of parameters 'el' and '<target>' are incompatible.\n\
                     Type 'Element' is not assignable to type 'string'."
                ),
            ),
            (
                vize_s0::String::from("src/App.vue"),
                Some(2339),
                vize_s0::String::from(
                    "11:30:error Property 'missing' does not exist on type 'Element'.",
                ),
            ),
        ]),
    );
}

#[test]
fn transition_hook_camel_case_aliases_are_typed_as_elements() {
    if resolve_test_tsgo_binary().is_none() {
        return;
    }
    let project_root = create_project_case(
        "transition-hook-camel-case-handler-elements",
        &[(
            "src/App.vue",
            r#"<script setup lang="ts">
function afterEnter(el: Element) { el.getBoundingClientRect() }
function afterLeave(el: Element) { el.getBoundingClientRect() }
function wrongTransition(el: string) { el.toUpperCase() }
</script>

<template>
  <Transition
    @afterEnter="afterEnter"
    @afterLeave="afterLeave"
    @beforeEnter="(el) => el.missing"
    @beforeLeave="wrongTransition"
  />
</template>
"#,
        )],
    );

    let snapshot = normalize_target_parameter_names(snapshot_project_diagnostics(&project_root));
    let _ = std::fs::remove_dir_all(&project_root);

    assert_eq!(
        snapshot,
        Some(vec![
            (
                vize_s0::String::from("src/App.vue"),
                Some(2322),
                vize_s0::String::from(
                    "12:6:error Type '(el: string) => void' is not assignable to type '(el: Element) => any'.\n\
                     Types of parameters 'el' and '<target>' are incompatible.\n\
                     Type 'Element' is not assignable to type 'string'."
                ),
            ),
            (
                vize_s0::String::from("src/App.vue"),
                Some(2339),
                vize_s0::String::from(
                    "11:30:error Property 'missing' does not exist on type 'Element'.",
                ),
            ),
        ]),
    );
}

#[test]
fn dynamic_component_custom_member_handlers_stay_unresolved() {
    if resolve_test_tsgo_binary().is_none() {
        return;
    }
    let project_root = create_project_case(
        "dynamic-component-custom-member-handler",
        &[(
            "src/App.vue",
            r#"<script setup lang="ts">
const current = "section"
function onWheel(ev: WheelEvent) { ev.deltaY }
</script>

<template>
  <component :is="current" @headerWheel="onWheel" />
</template>
"#,
        )],
    );

    let snapshot = snapshot_project_diagnostics(&project_root);
    let _ = std::fs::remove_dir_all(&project_root);

    assert_eq!(snapshot, Some(Vec::new()));
}
