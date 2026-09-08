use super::{create_project_case, resolve_test_tsgo_binary, snapshot_project_diagnostics};

#[test]
fn imported_runtime_define_props_reach_script_and_template() {
    if resolve_test_tsgo_binary().is_none() {
        return;
    }
    let project_root = create_project_case(
        "imported-runtime-define-props",
        &[
            (
                "packages/hooks/index.ts",
                r#"export * from './use-empty-values'
"#,
            ),
            (
                "packages/hooks/use-empty-values/index.ts",
                r#"export const useEmptyValuesProps = {
  emptyValues: Array,
  valueOnClear: { type: [String, Boolean], default: undefined },
} as const
"#,
            ),
            (
                "src/shared.ts",
                r#"export const sharedProps = {
  title: { type: String, default: 'Ready' },
  disabled: Boolean,
} as const

export const layoutProps = {
  singlePanel: Boolean,
} as const
"#,
            ),
            (
                "src/props.ts",
                r#"import { useEmptyValuesProps } from '@element-plus/hooks'
import { layoutProps, sharedProps } from './shared'

function buildProps<T extends Record<string, unknown>>(props: T): T {
  return props
}

function useAriaProps<const T extends string>(names: readonly T[]) {
  return {} as { readonly [K in T]: { readonly type: StringConstructor } }
}

export const importedProps = buildProps({
  ...sharedProps,
  ...layoutProps,
  ...useEmptyValuesProps,
  ...useAriaProps(['ariaLabel']),
  pageCount: { type: Number, required: true },
  pagerCount: { type: Number, default: 7 },
} as const)
"#,
            ),
            (
                "src/App.vue",
                r#"<script setup lang="ts">
import { importedProps } from './props'

const props = defineProps(importedProps)

function acceptsBoolean(value: boolean) {
  return value
}

function acceptsNumber(value: number) {
  return value
}

const ariaLabel: string | undefined = props.ariaLabel
const emptyCount = props.emptyValues?.length ?? 0
const valueOnClear: unknown = props.valueOnClear

acceptsBoolean(props.disabled)
acceptsBoolean(props.singlePanel)
acceptsNumber(props.pageCount)
acceptsNumber(props.pagerCount)
void valueOnClear
</script>

<template>
  <button :aria-label="ariaLabel" :disabled="disabled">{{ singlePanel ? title.toUpperCase() : title }} {{ pageCount + pagerCount + emptyCount }}</button>
</template>
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
    "baseUrl": ".",
    "paths": { "@element-plus/hooks": ["packages/hooks/index.ts"] },
    "noEmit": true
  },
  "vueCompilerOptions": { "strictTemplates": true },
  "include": ["src/**/*", "packages/**/*"]
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
        "imported runtime props should populate setup and template props, got: {snapshot:#?}"
    );
}

#[test]
fn type_based_with_defaults_omit_imported_defaults() {
    if resolve_test_tsgo_binary().is_none() {
        return;
    }
    let project_root = create_project_case(
        "type-with-defaults-imported-omit",
        &[
            (
                "src/defaults.ts",
                r#"export interface MessageProps {
  appendTo?: HTMLElement
  label?: string
  repeatNum?: number
}

const mutable = <T extends object>(value: T): T => value

export const messageDefaults = mutable({
  appendTo: document.body,
  label: 'Ready',
  repeatNum: 1,
} as const)
"#,
            ),
            (
                "src/omit.ts",
                r#"export function omit<T extends object, K extends keyof T>(value: T, ..._keys: K[]): Omit<T, K> {
  return value as Omit<T, K>
}
"#,
            ),
            (
                "src/App.vue",
                r#"<script setup lang="ts">
import { messageDefaults, type MessageProps } from './defaults'
import { omit } from './omit'

const props = withDefaults(
  defineProps<MessageProps>(),
  omit(messageDefaults, 'appendTo')
)

const repeat = props.repeatNum.toFixed()
const upper = props.label.toUpperCase()
</script>

<template>
  <span>{{ repeatNum.toFixed() }} {{ label.toUpperCase() }} {{ repeat }} {{ upper }}</span>
</template>
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
    "noEmit": true,
    "skipLibCheck": true
  },
  "vueCompilerOptions": { "strictTemplates": true },
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
        "imported withDefaults defaults should resolve through omit(), got: {snapshot:#?}"
    );
}

#[test]
fn imported_runtime_props_cache_rebinds_local_define_props_name() {
    if resolve_test_tsgo_binary().is_none() {
        return;
    }
    let project_root = create_project_case(
        "imported-runtime-props-cache-rebind",
        &[
            (
                "src/props.ts",
                r#"export const sharedProps = {
  label: { type: String, required: true },
} as const
"#,
            ),
            (
                "src/First.vue",
                r#"<script setup lang="ts">
import { sharedProps as firstProps } from './props'

const props = defineProps(firstProps)
const value: string = props.label
</script>

<template>{{ value }} {{ label.toUpperCase() }}</template>
"#,
            ),
            (
                "src/Second.vue",
                r#"<script setup lang="ts">
import { sharedProps as secondProps } from './props'

const props = defineProps(secondProps)
const value: string = props.label
</script>

<template>{{ value }} {{ label.toUpperCase() }}</template>
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
    "noEmit": true
  },
  "vueCompilerOptions": { "strictTemplates": true },
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
        "runtime props cache should rebind prop types to each SFC local runtime object, got: {snapshot:#?}"
    );
}
