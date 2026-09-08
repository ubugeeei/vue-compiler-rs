use super::{create_project_case, resolve_test_tsgo_binary, snapshot_project_diagnostics};
mod component_event_assignment_scope;
mod component_options_index_signature;
mod component_prop_regressions;
mod component_vif_guard_ts7006;
mod css_module_classes;
mod css_side_effect_import;
mod define_model_template_unwrap;
mod diagnostic_normalization;
mod directive_anchors;
mod directive_values;
mod empty_required_props;
mod exact_optional_props;
mod external_esm_module_augmentation;
mod external_slot_payloads;
mod global_component_callbacks;
mod imported_component_ref_expose;
mod imported_runtime_props;
mod literal_dynamic_slot_names;
mod native_prop_anchors;
mod open_slot_index_signature;
mod optional_boolean_props;
mod options_api_bridge_anchors;
mod options_api_data_assignment;
mod options_api_declaration_instance;
mod options_api_inherited_members;
mod pascal_case_prop_names;
mod public_instance_contract;
mod sequence_prop_expressions;
mod single_required_camel_prop;
mod split_script_diagnostic_anchors;
mod spread_props;
mod spread_scope_bindings;
mod strict_route_instance_global;
mod template_handler_ts7006;
mod template_instance_props;
mod template_key_expressions;
mod template_ref_slot_vnode_handlers;
mod ts_extension_substitution;
mod tsx_catch_all_emits;
mod unmapped_template_fallback;
mod v_for_source_callbacks;
mod vapor_anchors;

fn normalize_component_check_props_tail(message: &str) -> std::string::String {
    const START: &str = "__VizeComponentCheckProps<Props, ";
    const STABLE_TYPE: &str = "__VizeComponentCheckProps<Props, __VizeFallthroughAttrs>";

    let mut output = std::string::String::with_capacity(message.len());
    let mut rest = message;
    while let Some(start) = rest.find(START) {
        output.push_str(&rest[..start]);
        let from_type = &rest[start..];
        let Some(end) = from_type.find("'.") else {
            output.push_str(from_type);
            return output;
        };
        output.push_str(STABLE_TYPE);
        rest = &from_type[end..];
    }
    output.push_str(rest);
    output
}

#[test]
fn issue_2645_infers_generic_sfc_props_in_tsx() {
    if resolve_test_tsgo_binary().is_none() {
        return;
    }
    let project_root = create_project_case(
        "issue-2645-generic-sfc-tsx",
        &[
            (
                "src/Child.vue",
                r#"<script setup lang="ts" generic="T = string">
defineProps<{ value: T }>();
</script>
"#,
            ),
            (
                "src/usage.tsx",
                r#"import Child from "./Child.vue";
export default () => <Child value={123} />;
"#,
            ),
        ],
    );
    std::fs::write(
        project_root.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "strict": true,
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "jsx": "preserve",
    "jsxImportSource": "vue",
    "noEmit": true
  },
  "include": ["src/**/*"]
}"#,
    )
    .unwrap();

    let Some(snapshot) = snapshot_project_diagnostics(&project_root) else {
        let _ = std::fs::remove_dir_all(&project_root);
        return;
    };

    assert!(
        snapshot.iter().all(|(file, code, message)| {
            !(file == "src/usage.tsx" && *code == Some(2322) && message.contains("never"))
        }),
        "generic SFC props should infer from TSX usage, got: {snapshot:#?}"
    );

    let _ = std::fs::remove_dir_all(&project_root);
}

#[test]
fn issue_2646_does_not_widen_generic_template_refs() {
    if resolve_test_tsgo_binary().is_none() {
        return;
    }
    let project_root = create_project_case(
        "issue-2646-generic-template-ref",
        &[(
            "src/Foo.vue",
            r#"<script setup lang="ts" generic="T">
import { ref } from "vue";

const value = ref<T>();

function update(value: T | undefined) {}
</script>

<template>
  <button @click="update(value)" />
</template>
"#,
        )],
    );

    let Some(snapshot) = snapshot_project_diagnostics(&project_root) else {
        let _ = std::fs::remove_dir_all(&project_root);
        return;
    };

    assert!(
        snapshot.iter().all(|(file, code, message)| {
            !(file == "src/Foo.vue"
                && *code == Some(2345)
                && message.contains("__VizeWidenTemplateRef"))
        }),
        "generic refs should keep T | undefined in templates, got: {snapshot:#?}"
    );

    let _ = std::fs::remove_dir_all(&project_root);
}

#[test]
fn issue_2647_keeps_setup_type_imports_and_local_aliases_separate() {
    if resolve_test_tsgo_binary().is_none() {
        return;
    }
    let project_root = create_project_case(
        "issue-2647-setup-type-import-scope",
        &[
            (
                "src/types.ts",
                "export type MaybeElement = Element | null;\n",
            ),
            (
                "src/Foo.vue",
                r#"<script lang="ts">
export type FooContext = MaybeElement;
</script>

<script setup lang="ts">
import type { MaybeElement } from "./types";

type MaybeElement = HTMLElement | null;
</script>
"#,
            ),
        ],
    );

    let Some(snapshot) = snapshot_project_diagnostics(&project_root) else {
        let _ = std::fs::remove_dir_all(&project_root);
        return;
    };

    assert!(
        snapshot.iter().all(|(file, code, message)| {
            !(file == "src/Foo.vue" && *code == Some(2440) && message.contains("MaybeElement"))
        }),
        "setup type imports and local aliases should not collide, got: {snapshot:#?}"
    );

    let _ = std::fs::remove_dir_all(&project_root);
}

#[test]
fn issue_2648_exposes_props_inherited_from_imported_interfaces() {
    if resolve_test_tsgo_binary().is_none() {
        return;
    }
    let project_root = create_project_case(
        "issue-2648-imported-interface-props",
        &[
            (
                "src/Base.vue",
                r#"<script lang="ts">
export interface BaseProps {
  decorative?: boolean;
}
</script>
"#,
            ),
            (
                "src/Foo.vue",
                r#"<script setup lang="ts">
import type { BaseProps as ImportedProps } from "./Base.vue";

interface Props extends ImportedProps {}
defineProps<Props>();
</script>

<template>
  {{ decorative }}
</template>
"#,
            ),
        ],
    );

    let Some(snapshot) = snapshot_project_diagnostics(&project_root) else {
        let _ = std::fs::remove_dir_all(&project_root);
        return;
    };

    assert!(
        snapshot.iter().all(|(file, code, message)| {
            !(file == "src/Foo.vue" && *code == Some(2304) && message.contains("decorative"))
        }),
        "imported interface props should be template bindings, got: {snapshot:#?}"
    );

    let _ = std::fs::remove_dir_all(&project_root);
}

#[test]
fn issue_2649_keeps_v_slot_scope_spread_object_compatible() {
    if resolve_test_tsgo_binary().is_none() {
        return;
    }
    let project_root = create_project_case(
        "issue-2649-v-slot-spread",
        &[
            (
                "src/Base.vue",
                r#"<template>
  <slot value="one" />
</template>
"#,
            ),
            (
                "src/Foo.vue",
                r#"<script setup lang="ts">
import Base from "./Base.vue";
</script>

<template>
  <Base v-slot="slotData">
    <slot v-bind="{ ...slotData }" />
  </Base>
</template>
"#,
            ),
        ],
    );

    let Some(snapshot) = snapshot_project_diagnostics(&project_root) else {
        let _ = std::fs::remove_dir_all(&project_root);
        return;
    };

    assert!(
        snapshot.iter().all(|(file, code, message)| {
            !(file == "src/Foo.vue" && *code == Some(2698) && message.contains("Spread types"))
        }),
        "v-slot scope should be spread-compatible, got: {snapshot:#?}"
    );

    let _ = std::fs::remove_dir_all(&project_root);
}

#[test]
fn issue_2650_preserves_exposed_component_instance_casts() {
    if resolve_test_tsgo_binary().is_none() {
        return;
    }
    let project_root = create_project_case(
        "issue-2650-exposed-instance-cast",
        &[
            (
                "src/Base.vue",
                r#"<script setup lang="ts">
function syncModelValue() {}
defineExpose({ syncModelValue });
</script>
"#,
            ),
            (
                "src/Foo.vue",
                r#"<script setup lang="ts">
import { ref, watchEffect } from "vue";
import type { ComponentPublicInstance } from "vue";
import Base from "./Base.vue";

type MaybeElement = ComponentPublicInstance | null;

const current = ref<MaybeElement>(null);

watchEffect(() => {
  if (current.value) {
    (current.value as InstanceType<typeof Base>).syncModelValue();
  }
});
</script>
"#,
            ),
        ],
    );

    let Some(snapshot) = snapshot_project_diagnostics(&project_root) else {
        let _ = std::fs::remove_dir_all(&project_root);
        return;
    };

    assert!(
        snapshot.iter().all(|(file, code, message)| {
            !(file == "src/Foo.vue" && *code == Some(2352) && message.contains("syncModelValue"))
        }),
        "exposed component instance casts should stay accepted, got: {snapshot:#?}"
    );

    let _ = std::fs::remove_dir_all(&project_root);
}
