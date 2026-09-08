use super::{create_project_case, resolve_test_tsgo_binary, snapshot_project_diagnostics};

#[test]
fn tsx_accepts_catch_all_emit_listener_props() {
    if resolve_test_tsgo_binary().is_none() {
        return;
    }
    let project_root = create_project_case(
        "tsx-catch-all-emits",
        &[
            (
                "src/Child.vue",
                r#"<script setup lang="ts">
defineProps<{ label?: string }>()
defineEmits<(event: string, ...args: any[]) => void>()
</script>

<template>
  <button>{{ label }}</button>
</template>
"#,
            ),
            (
                "src/Parent.tsx",
                r#"import Child from './Child.vue'

export default () => (
  <Child
    label="Save"
    onClear={() => {}}
    onUpdate:modelValue={(value: unknown) => value}
    onVisible-change={() => {}}
  />
)
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
    "noEmit": true,
    "skipLibCheck": true
  },
  "include": ["src/**/*"]
}"#,
    )
    .unwrap();

    let snapshot = snapshot_project_diagnostics(&project_root);
    let _ = std::fs::remove_dir_all(&project_root);
    let Some(snapshot) = snapshot else {
        return;
    };

    assert_eq!(
        snapshot,
        Vec::new(),
        "catch-all emits should keep TSX listener props assignable, got: {snapshot:#?}"
    );
}

#[test]
fn tsx_accepts_reserved_key_and_ref_on_imported_sfc_components() {
    if resolve_test_tsgo_binary().is_none() {
        return;
    }
    let project_root = create_project_case(
        "tsx-reserved-sfc-component-props",
        &[
            (
                "src/Child.vue",
                r#"<script setup lang="ts">
defineProps<{ label?: string }>()
</script>

<template>
  <button>{{ label }}</button>
</template>
"#,
            ),
            (
                "src/Parent.tsx",
                r#"import Child from './Child.vue'

const childRef = (value: InstanceType<typeof Child> | null) => { void value }

export default () => <Child key="save" ref={childRef} label="Save" />
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
    "noEmit": true,
    "skipLibCheck": true
  },
  "include": ["src/**/*"]
}"#,
    )
    .unwrap();

    let snapshot = snapshot_project_diagnostics(&project_root);
    let _ = std::fs::remove_dir_all(&project_root);
    let Some(snapshot) = snapshot else {
        return;
    };

    assert_eq!(
        snapshot,
        Vec::new(),
        "Vue reserved key/ref should not trip the generated component prop guard"
    );
}

#[test]
fn tsx_accepts_union_component_prop_keys() {
    if resolve_test_tsgo_binary().is_none() {
        return;
    }
    let project_root = create_project_case(
        "tsx-union-component-props",
        &[
            (
                "src/Alpha.vue",
                r#"<script setup lang="ts">
defineProps<{ alpha: string }>()
</script>

<template>
  <div>{{ alpha }}</div>
</template>
"#,
            ),
            (
                "src/Beta.vue",
                r#"<script setup lang="ts">
defineProps<{ beta: number }>()
</script>

<template>
  <div>{{ beta }}</div>
</template>
"#,
            ),
            (
                "src/Parent.tsx",
                r#"import Alpha from './Alpha.vue'
import Beta from './Beta.vue'

declare const useAlpha: boolean
declare const forwarded: { alpha: string; beta: number }

const pick = () => useAlpha ? Alpha : Beta

export default () => {
  const Component = pick()
  return <Component {...forwarded} />
}
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
    "noEmit": true,
    "skipLibCheck": true
  },
  "include": ["src/**/*"]
}"#,
    )
    .unwrap();

    let snapshot = snapshot_project_diagnostics(&project_root);
    let _ = std::fs::remove_dir_all(&project_root);
    let Some(snapshot) = snapshot else {
        return;
    };

    assert_eq!(
        snapshot,
        Vec::new(),
        "dynamic union components should accept the union of candidate prop keys"
    );
}
