//! Ref, slot-listener, and vnode-hook template handler parity.

use super::super::{create_project_case, resolve_test_tsgo_binary, snapshot_project_diagnostics};
use super::diagnostic_normalization::normalize_target_parameter_names;

#[test]
fn slot_outlet_listeners_use_declared_slot_props() {
    if resolve_test_tsgo_binary().is_none() {
        return;
    }
    let project_root = create_project_case(
        "slot-outlet-listeners-declared-props",
        &[(
            "src/App.vue",
            r#"<script setup lang="ts">
defineSlots<{
  default(props: {
    onPick?: (value: number, visible: boolean) => unknown
    onSetPickerOption?: (name: string, value: number) => unknown
  }): unknown
}>()

function onPick(value: number, visible: boolean) {
  value.toFixed()
  visible.valueOf()
}

function onSetPickerOption(name: string, value: number) {
  name.toUpperCase()
  value.toFixed()
}
</script>

<template>
  <slot @pick="onPick" @set-picker-option="onSetPickerOption" />
</template>
"#,
        )],
    );

    let snapshot = normalize_target_parameter_names(snapshot_project_diagnostics(&project_root));
    let _ = std::fs::remove_dir_all(&project_root);

    assert_eq!(
        snapshot,
        Some(Vec::new()),
        "slot outlet listeners should use the declared slot event props"
    );
}

#[test]
fn vue_vnode_hooks_use_vnode_payloads() {
    if resolve_test_tsgo_binary().is_none() {
        return;
    }
    let project_root = create_project_case(
        "vue-vnode-hook-payloads",
        &[
            (
                "src/vue-shim.d.ts",
                r#"import 'vue'

declare module 'vue' {
  export interface VNode {
    type: unknown
  }
}
"#,
            ),
            (
                "src/App.vue",
                r#"<script setup lang="ts">
import type { VNode } from 'vue'

function handleVNodeMounted(vnode: VNode) {
  vnode.type
}
</script>

<template>
  <transition @vue:mounted="handleVNodeMounted">
    <span />
  </transition>
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
        "vue vnode hooks should pass VNode payloads instead of DOM Events"
    );
}

#[test]
fn component_ref_callbacks_are_contextually_typed() {
    if resolve_test_tsgo_binary().is_none() {
        return;
    }
    let project_root = create_project_case(
        "component-ref-callback-context",
        &[
            (
                "src/Child.vue",
                r#"<script setup lang="ts">
defineProps<{ label?: string }>()
</script>
<template><button>{{ label }}</button></template>
"#,
            ),
            (
                "src/App.vue",
                r#"<script setup lang="ts">
import Child from "./Child.vue"
const refs: unknown[] = []
</script>
<template>
  <Child :ref="(item) => refs.push(item)" />
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
        "component ref callbacks should not lose contextual typing"
    );
}

#[test]
fn component_ref_callbacks_preserve_authored_ts_ignore_comments() {
    if resolve_test_tsgo_binary().is_none() {
        return;
    }
    let project_root = create_project_case(
        "component-ref-callback-ts-ignore",
        &[
            (
                "src/Child.vue",
                r#"<script setup lang="ts">
defineProps<{ label?: string }>()
</script>
<template><button>{{ label }}</button></template>
"#,
            ),
            (
                "src/App.vue",
                r#"<script setup lang="ts">
import type { ComponentPublicInstance } from "vue"
import Child from "./Child.vue"

function acceptElement(element: Element) {
  element.nodeName
}
</script>
<template>
  <Child :ref="
    (vnode: Element | ComponentPublicInstance | null) => {
      if (!vnode) return
      // @ts-ignore
      acceptElement(vnode.$el)
    }
  " />
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
        "component ref callback generation should preserve authored TypeScript ignore comments"
    );
}

#[test]
fn unresolved_global_component_ref_callbacks_do_not_emit_standalone_any() {
    if resolve_test_tsgo_binary().is_none() {
        return;
    }
    let project_root = create_project_case(
        "unresolved-global-component-ref-callback",
        &[(
            "src/App.vue",
            r#"<script setup lang="ts">
import type { ComponentPublicInstance } from "vue"

type MenuInstance = ComponentPublicInstance & { focus(): void }
const refs: unknown[] = []
</script>
<template>
  <el-cascader-menu :ref="(item) => refs.push(item as MenuInstance)" />
</template>
"#,
        )],
    );

    let snapshot = normalize_target_parameter_names(snapshot_project_diagnostics(&project_root));
    let _ = std::fs::remove_dir_all(&project_root);

    assert_eq!(
        snapshot,
        Some(Vec::new()),
        "component ref callbacks owned by Vue should not be emitted as standalone expressions"
    );
}
