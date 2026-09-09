use std::path::Path;

use vize_canon::{BatchTypeChecker, BatchTypeCheckerTrait, SfcTypeCheckOptions, type_check_sfc};

#[test]
fn enum_ref_template_comparisons_widen_initial_member() {
    let source = r#"<script setup lang="ts">
import { ref } from 'vue'

const enum ModalWindowState {
  None,
  ProfileEdit,
  PasswordChange,
}

const modalWindowState = ref(ModalWindowState.None)
</script>

<template>
  <ModalWindow :opened="modalWindowState === ModalWindowState.ProfileEdit" />
  <ModalWindow :opened="modalWindowState === ModalWindowState.PasswordChange" />
</template>"#;
    let options = SfcTypeCheckOptions::new("ModalActions.vue").with_virtual_ts();
    let result = type_check_sfc(source, &options);
    let virtual_ts = result.virtual_ts.expect("virtual ts should be generated");

    assert!(
        virtual_ts.contains("type __VizeWidenTemplateRef<T>"),
        "template ref unwrapping should use the widening helper:\n{virtual_ts}"
    );
    assert!(
        virtual_ts.contains(
            "type __U<T> = T extends import('vue').Ref ? __VizeWidenTemplateRef<T['value']> : T;"
        ),
        "Vue 3 template ref unwrapping must widen mutable literal values:\n{virtual_ts}"
    );
    assert!(
        virtual_ts.contains("var modalWindowState: __U<__R_modalWindowState> = undefined as any;"),
        "template should shadow the setup ref with an unwrapped alias:\n{virtual_ts}"
    );
    assert!(
        !virtual_ts.contains("void ModalWindowState;"),
        "const enum declarations must not be emitted as setup value anchors:\n{virtual_ts}"
    );
}

#[test]
fn accepts_enum_ref_template_comparisons_after_initial_member() {
    let project = create_project(&[("src/App.vue", ENUM_REF_SFC)]);
    let Some(snapshot) = snapshot_project_diagnostics(project.path()) else {
        return;
    };

    assert!(
        snapshot.iter().all(|(file, code, message)| {
            !(file == "src/App.vue"
                && matches!(code, Some(2367 | 2475))
                && message.contains("ModalWindowState"))
        }),
        "template enum comparisons should not be narrowed to the initial ref member or emit invalid const enum anchors: {snapshot:#?}"
    );
}

#[test]
fn branded_ref_template_arguments_keep_their_brand() {
    let result = type_check_sfc(
        BRANDED_REF_SFC,
        &SfcTypeCheckOptions::new("BrandedRef.vue").with_virtual_ts(),
    );
    let virtual_ts = result.virtual_ts.expect("virtual ts should be generated");
    assert!(
        virtual_ts.contains("T extends string ? string extends T ? string : T"),
        "primitive widening must preserve branded string intersections:\n{virtual_ts}"
    );

    let project = create_project(&[("src/BrandedRef.vue", BRANDED_REF_SFC)]);
    let Some(snapshot) = snapshot_project_diagnostics(project.path()) else {
        return;
    };
    assert!(
        snapshot.iter().all(|(file, code, message)| {
            !(file == "src/BrandedRef.vue"
                && *code == Some(2345)
                && message.contains("BrandedString"))
        }),
        "template ref unwrapping should retain the brand: {snapshot:#?}"
    );
}

#[test]
fn destructured_and_rest_props_do_not_emit_shadowing_template_aliases() {
    let options = SfcTypeCheckOptions::new("Button.vue").with_virtual_ts();
    let result = type_check_sfc(TYPED_ROUTE_BUTTON_SFC, &options);
    let virtual_ts = result.virtual_ts.expect("virtual ts should be generated");

    for name in ["leadingIcon", "appendIcon", "buttonProps"] {
        assert!(
            !virtual_ts.contains(&format!("const {name} = props[")),
            "destructured/rest prop local `{name}` must not be shadowed by a generated template prop alias:\n{virtual_ts}"
        );
        assert!(
            virtual_ts.contains(&format!("void {name};")),
            "destructured/rest prop local `{name}` should still be referenced for TS6133:\n{virtual_ts}"
        );
    }
}

#[test]
fn accepts_typed_route_destructured_and_rest_props() {
    let project = create_project(&[
        ("src/NuxtLink.vue", NUXT_LINK_SFC),
        ("src/Icon.vue", ICON_SFC),
        ("src/Button.vue", TYPED_ROUTE_BUTTON_SFC),
    ]);
    let Some(snapshot) = snapshot_project_diagnostics(project.path()) else {
        return;
    };

    assert!(
        snapshot.iter().all(|(file, code, message)| {
            if file != "src/Button.vue" {
                return true;
            }
            !matches!(code, Some(2322 | 2339 | 2558))
                && !message.contains("TypedRouteLocationRawFromName")
                && !message.contains("prependIcon")
                && !message.contains("appendIcon")
        }),
        "typed route destructured/rest props should not report false positives: {snapshot:#?}"
    );
}

#[test]
fn undefined_unknown_event_handler_stays_optional() {
    let result = type_check_sfc(
        UNDEFINED_EVENT_SFC,
        &SfcTypeCheckOptions::new("UndefinedEvent.vue").with_virtual_ts(),
    );
    let virtual_ts = result.virtual_ts.expect("virtual ts should be generated");
    assert!(virtual_ts.contains("undefined;  // handler expression"));
    assert!(!virtual_ts.contains("=> handler)((undefined))"));

    let project = create_project(&[("src/UndefinedEvent.vue", UNDEFINED_EVENT_SFC)]);
    let Some(snapshot) = snapshot_project_diagnostics(project.path()) else {
        return;
    };
    assert!(
        snapshot.iter().all(|(_, code, _)| *code != Some(2345)),
        "undefined listeners should not be required callbacks: {snapshot:#?}"
    );
}

fn create_project(files: &[(&str, &str)]) -> tempfile::TempDir {
    let project = tempfile::tempdir().expect("temp project should be created");
    write_tsconfig(project.path());
    write_vue_stub(project.path());
    for (path, source) in files {
        write_file(project.path(), path, source);
    }
    project
}

fn snapshot_project_diagnostics(project_root: &Path) -> Option<Vec<(String, Option<u32>, String)>> {
    let mut checker = BatchTypeChecker::new(project_root).ok()?;
    checker.scan_project().ok()?;
    let result = checker.check_project().ok()?;

    let mut snapshot: Vec<_> = result
        .diagnostics
        .into_iter()
        .map(|diagnostic| {
            (
                relative_path(project_root, &diagnostic.file),
                diagnostic.code,
                format!(
                    "{}:{}:{} {}",
                    diagnostic.line + 1,
                    diagnostic.column + 1,
                    match diagnostic.severity {
                        1 => "error",
                        2 => "warning",
                        3 => "info",
                        _ => "hint",
                    },
                    diagnostic.message
                ),
            )
        })
        .collect();
    snapshot.sort();
    Some(snapshot)
}

fn write_tsconfig(project_root: &Path) {
    write_file(
        project_root,
        "tsconfig.json",
        r#"{
  "compilerOptions": {
    "strict": true,
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "noEmit": true
  },
  "include": ["src/**/*"]
}"#,
    );
}

fn write_vue_stub(project_root: &Path) {
    write_file(
        project_root,
        "node_modules/vue/package.json",
        r#"{
  "name": "vue",
  "types": "index.d.ts"
}"#,
    );
    write_file(
        project_root,
        "node_modules/vue/index.d.ts",
        r#"export interface Ref<T = unknown> {
  value: T;
}

export function ref<T>(value: T): Ref<T>;
"#,
    );
}

fn write_file(project_root: &Path, path: &str, source: &str) {
    let path = project_root.join(path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, source).unwrap();
}

fn relative_path(root: &Path, file: &Path) -> String {
    file.strip_prefix(root)
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| file.display().to_string())
}

const ENUM_REF_SFC: &str = r#"<script setup lang="ts">
import { ref } from 'vue'

const enum ModalWindowState {
  None,
  ProfileEdit,
  PasswordChange,
}

const modalWindowState = ref(ModalWindowState.None)
</script>

<template>
  <ModalWindow :opened="modalWindowState === ModalWindowState.ProfileEdit" />
  <ModalWindow :opened="modalWindowState === ModalWindowState.PasswordChange" />
</template>
"#;

const BRANDED_REF_SFC: &str = r#"<script setup lang="ts">
import { ref } from "vue";
type BrandedString = string & { __brand: "BrandedString" };
function acceptBranded(value: BrandedString): string { return value; }
const brandedRef = ref<BrandedString>("" as BrandedString);
</script>
<template>{{ acceptBranded(brandedRef) }}</template>
"#;

const NUXT_LINK_SFC: &str = r#"<script setup lang="ts" generic="T extends string = string, P extends string = string">
type TypedRouteLocationRawFromName<Name extends string, Params extends string = string> = {
  name: Name
  params?: Record<Params, string>
}

defineProps<{
  to: string | TypedRouteLocationRawFromName<T, P>
}>()
</script>

<template>
  <a><slot /></a>
</template>
"#;

const ICON_SFC: &str = r#"<script setup lang="ts">
defineProps<{
  name: string
}>()
</script>

<template>
  <span>{{ name }}</span>
</template>
"#;

const TYPED_ROUTE_BUTTON_SFC: &str = r#"<script setup lang="ts" generic="T extends string = string, P extends string = string">
import NuxtLink from './NuxtLink.vue'
import Icon from './Icon.vue'

type TypedRouteLocationRawFromName<Name extends string, Params extends string = string> = {
  name: Name
  params?: Record<Params, string>
}

type BaseProps = {
  label: string
  prependIcon?: string
  appendIcon?: string
  leadingIcon?: number
  buttonProps?: boolean
}

type LinkProps<T extends string, P extends string> = BaseProps & {
  as: 'link'
  to: TypedRouteLocationRawFromName<T, P>
}

type NativeProps = BaseProps & {
  as?: 'button'
  to?: never
}

type Props<T extends string, P extends string> = LinkProps<T, P> | NativeProps

const {
  label,
  prependIcon: leadingIcon,
  appendIcon,
  ...buttonProps
} = defineProps<Props<T, P>>()
</script>

<template>
  <NuxtLink v-if="buttonProps.as === 'link'" :to="buttonProps.to">
    <Icon v-if="leadingIcon" :name="leadingIcon" />
    {{ label }}
    <Icon v-if="appendIcon" :name="appendIcon" />
  </NuxtLink>
</template>
"#;

const UNDEFINED_EVENT_SFC: &str = r#"<script setup lang="ts"></script>
<template><UnknownComponent @unknownEvent="undefined" /></template>
"#;
