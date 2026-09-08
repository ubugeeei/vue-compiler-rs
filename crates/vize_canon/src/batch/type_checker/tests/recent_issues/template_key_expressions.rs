//! Template `:key` expressions follow the dialect's own key contract (#4149).
//!
//! `template` is an HTML tag, so a `<template v-for="x in xs" :key="…">`
//! resolved its `key` through Vue's element table, where `ReservedProps`
//! declares `key?: PropertyKey | undefined`.
//!
//! Current `vue-tsc` checks that reserved `key` on fragment-hosting templates,
//! including Voicevox's `ToolBar.vue` shape (`:key="button.text"` over a
//! `string | null`) and Elk's `CommonTabs.vue` shape (`:key="option"` over an
//! object). Other fragment-hosted bindings, such as `:id`, still avoid the HTML
//! `<template>` element prop table. A bare `<template>` and every real element
//! keep their normal native prop checks.
//!
//! Every expectation below follows the repo's current `vue-tsc 3.3.11`
//! (`typescript` 6.0.3, `vue` 3.6.0-beta.10) parity baseline.

use super::super::{
    create_project_case_without_node_modules, resolve_test_tsgo_binary,
    snapshot_project_diagnostics, write_test_vue_stub,
};
use crate::batch::runtime_deps::VUE_RUNTIME_DOM_STUB_TYPES;
use std::path::PathBuf;
use vize_s0::String;

mod dialect_baselines;

/// `@vue/runtime-dom`'s own element table, reduced to the tags these fixtures
/// use. `ReservedProps` and the `NativeElements` mapping are the shipped
/// declarations verbatim, so `<div>`'s `key` is the same
/// `PropertyKey | undefined` an installed Vue provides.
const NATIVE_ELEMENTS_WITH_RESERVED_PROPS: &str = r#"export interface ReservedProps {
  key?: PropertyKey | undefined;
  ref?: unknown;
  ref_for?: boolean | undefined;
  ref_key?: string | undefined;
}
export interface IntrinsicElementAttributes {
  div: { id?: string | undefined };
  span: { id?: string | undefined };
  template: { id?: string | undefined };
}
export type NativeElements = {
  [K in keyof IntrinsicElementAttributes]: IntrinsicElementAttributes[K] & ReservedProps;
};"#;

fn create_key_project(name: &str, files: &[(&str, &str)]) -> PathBuf {
    // `<template>` must resolve through the same element table an installed Vue
    // ships, or the fixtures could not observe the contract at all.
    let project_root = create_project_case_without_node_modules(name, files);
    let node_modules = project_root.join("node_modules");
    write_test_vue_stub(&node_modules).expect("write isolated Vue stub");
    let vue_types = VUE_RUNTIME_DOM_STUB_TYPES.replace(
        "export type NativeElements = Record<string, Record<string, unknown>>;",
        NATIVE_ELEMENTS_WITH_RESERVED_PROPS,
    );
    std::fs::write(node_modules.join("@vue/runtime-dom/index.d.ts"), vue_types)
        .expect("pin the reserved-prop element table");
    project_root
}

/// The two real-corpus shapes, plus the key kinds `PropertyKey` does admit and
/// the other fragment hosts. Fragment `:id` remains silent.
#[test]
fn template_fragment_keys_match_the_dialect_key_contract() {
    if resolve_test_tsgo_binary().is_none() {
        return;
    }
    let project_root = create_key_project(
        "template-fragment-key-contract",
        &[(
            "src/App.vue",
            r#"<script setup lang="ts">
import { computed, ref } from 'vue'
const buttons: { text: string | null }[] = []
const tabs: { id: number }[] = []
const strings: string[] = []
const numbers: number[] = []
const symbols: symbol[] = []
const holder = ref<{ id: number } | null>(null)
const derived = computed(() => holder.value)
const flag = true
</script>

<template>
  <template v-for="button in buttons" :key="button.text">
    <span>{{ button.text }}</span>
  </template>
  <template v-for="option in tabs" :key="option">
    <span>{{ option.id }}</span>
  </template>
  <template v-for="s in strings" :key="s"><span>{{ s }}</span></template>
  <template v-for="n in numbers" :key="n"><span>{{ n }}</span></template>
  <template v-for="sy in symbols" :key="sy"><span>x</span></template>
  <template v-for="item in tabs" :key="holder"><span>{{ item.id }}</span></template>
  <template v-for="item in tabs" :key="derived"><span>{{ item.id }}</span></template>
  <template v-for="item in tabs" :key="item.id" :id="item.id">
    <span>{{ item.id }}</span>
  </template>
  <template v-if="flag" :key="tabs"><span>a</span></template>
</template>
"#,
        )],
    );

    let snapshot = snapshot_project_diagnostics(&project_root);
    let _ = std::fs::remove_dir_all(&project_root);
    let snapshot = snapshot.expect("type-check the template fragment key project");

    assert_eq!(
        snapshot,
        vec![
            (
                String::from("src/App.vue"),
                Some(2322),
                String::from(
                    "14:40:error Type 'string | null' is not assignable to type 'PropertyKey | undefined'.\nType 'null' is not assignable to type 'PropertyKey | undefined'."
                ),
            ),
            (
                String::from("src/App.vue"),
                Some(2322),
                String::from(
                    "17:37:error Type '{ id: number; }' is not assignable to type 'PropertyKey | undefined'."
                ),
            ),
            (
                String::from("src/App.vue"),
                Some(2322),
                String::from(
                    "23:35:error Type '{ id: number; } | null' is not assignable to type 'PropertyKey | undefined'.\nType 'null' is not assignable to type 'PropertyKey | undefined'."
                ),
            ),
            (
                String::from("src/App.vue"),
                Some(2322),
                String::from(
                    "24:35:error Type '{ id: number; } | null' is not assignable to type 'PropertyKey | undefined'.\nType 'null' is not assignable to type 'PropertyKey | undefined'."
                ),
            ),
            (
                String::from("src/App.vue"),
                Some(2322),
                String::from(
                    "28:26:error Type '{ id: number; }[]' is not assignable to type 'PropertyKey | undefined'."
                ),
            ),
        ]
    );
}

/// The contract the dialect *does* impose, which the fix must not weaken.
///
/// ```text
/// src/Elements.vue(12,35): error TS2322: Type 'string | null' is not assignable to type 'PropertyKey | undefined'.
///   Type 'null' is not assignable to type 'PropertyKey | undefined'.
/// src/Elements.vue(13,32): error TS2322: Type '{ id: number; }' is not assignable to type 'PropertyKey | undefined'.
/// src/Elements.vue(14,30): error TS2322: Type '{ id: number; } | null' is not assignable to type 'PropertyKey | undefined'.
///   Type 'null' is not assignable to type 'PropertyKey | undefined'.
/// src/Elements.vue(15,30): error TS2322: Type '{ id: number; } | null' is not assignable to type 'PropertyKey | undefined'.
///   Type 'null' is not assignable to type 'PropertyKey | undefined'.
/// src/Elements.vue(19,14): error TS2322: Type 'number' is not assignable to type 'string'.
/// ```
#[test]
fn element_keys_keep_the_reserved_prop_contract() {
    if resolve_test_tsgo_binary().is_none() {
        return;
    }
    let project_root = create_key_project(
        "element-key-reserved-prop-contract",
        &[(
            "src/Elements.vue",
            r#"<script setup lang="ts">
import { computed, ref } from 'vue'
const buttons: { text: string | null }[] = []
const tabs: { id: number }[] = []
const strings: string[] = []
const holder = ref<{ id: number } | null>(null)
const derived = computed(() => holder.value)
</script>

<template>
  <div v-for="s in strings" :key="s">{{ s }}</div>
  <div v-for="button in buttons" :key="button.text">{{ button.text }}</div>
  <div v-for="option in tabs" :key="option">{{ option.id }}</div>
  <div v-for="item in tabs" :key="holder">{{ item.id }}</div>
  <div v-for="item in tabs" :key="derived">{{ item.id }}</div>
  <template :id="'ok'">
    <span>a</span>
  </template>
  <template :id="1">
    <span>b</span>
  </template>
</template>
"#,
        )],
    );

    let snapshot = snapshot_project_diagnostics(&project_root);
    let _ = std::fs::remove_dir_all(&project_root);
    let snapshot = snapshot.expect("type-check the element key project");

    assert_eq!(
        snapshot,
        vec![
            (
                String::from("src/Elements.vue"),
                Some(2322),
                String::from(
                    "12:35:error Type 'string | null' is not assignable to type 'PropertyKey | undefined'.\nType 'null' is not assignable to type 'PropertyKey | undefined'."
                ),
            ),
            (
                String::from("src/Elements.vue"),
                Some(2322),
                String::from(
                    "13:32:error Type '{ id: number; }' is not assignable to type 'PropertyKey | undefined'."
                ),
            ),
            (
                String::from("src/Elements.vue"),
                Some(2322),
                String::from(
                    "14:30:error Type '{ id: number; } | null' is not assignable to type 'PropertyKey | undefined'.\nType 'null' is not assignable to type 'PropertyKey | undefined'."
                ),
            ),
            (
                String::from("src/Elements.vue"),
                Some(2322),
                String::from(
                    "15:30:error Type '{ id: number; } | null' is not assignable to type 'PropertyKey | undefined'.\nType 'null' is not assignable to type 'PropertyKey | undefined'."
                ),
            ),
            (
                String::from("src/Elements.vue"),
                Some(2322),
                String::from("19:14:error Type 'number' is not assignable to type 'string'."),
            ),
        ]
    );
}

/// Dropping the key's prop type must not stop checking the key *expression*.
///
/// ```text
/// src/Members.vue(6,41): error TS2339: Property 'missing' does not exist on type '{ id: number; }'.
/// src/Members.vue(7,46): error TS2339: Property 'missing' does not exist on type '{ id: number; }'.
/// ```
#[test]
fn key_expression_members_stay_checked_inside_a_fragment() {
    if resolve_test_tsgo_binary().is_none() {
        return;
    }
    let project_root = create_key_project(
        "template-fragment-key-members",
        &[(
            "src/Members.vue",
            r#"<script setup lang="ts">
const items: { id: number }[] = []
</script>

<template>
  <div v-for="item in items" :key="item.missing">{{ item.id }}</div>
  <template v-for="item in items" :key="item.missing">
    <span>{{ item.id }}</span>
  </template>
</template>
"#,
        )],
    );

    let snapshot = snapshot_project_diagnostics(&project_root);
    let _ = std::fs::remove_dir_all(&project_root);
    let snapshot = snapshot.expect("type-check the key member project");

    assert_eq!(
        snapshot,
        vec![
            (
                String::from("src/Members.vue"),
                Some(2339),
                String::from(
                    "6:41:error Property 'missing' does not exist on type '{ id: number; }'."
                ),
            ),
            (
                String::from("src/Members.vue"),
                Some(2339),
                String::from(
                    "7:46:error Property 'missing' does not exist on type '{ id: number; }'."
                ),
            ),
        ]
    );
}
