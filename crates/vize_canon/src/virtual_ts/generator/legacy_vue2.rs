use vize_carton::config::VueVersion;
use vize_carton::cstr;
use vize_croquis::Croquis;

use super::super::helpers::VUE_TYPE_HELPERS;
use super::super::types::{VirtualTsGenerationOptions, VirtualTsOptions, VirtualTsOutput};
use super::generate_virtual_ts_with_offsets_and_checks;
use super::spans::DEFINE_COMPONENT_HELPER;

const LEGACY_VUE_TYPE_HELPERS: &str = r#"type __EmitShape<T> = T extends (...args: any[]) => any ? T : T extends Record<string, any> ? { [K in keyof T]: T[K] extends (...args: infer A) => any ? A : T[K] extends any[] ? T[K] : any[]; } : Record<string, any[]>;
type __EmitArgs<T, K extends keyof T> = T[K] extends any[] ? T[K] : any[];
type __EmitFn<T, __S = __EmitShape<T>, __K extends keyof __S & string = keyof __S & string, __U = { [K in __K]: (event: K, ...args: __EmitArgs<__S, K>) => void }[__K]> = __S extends (...args: any[]) => any ? __S : [__K] extends [never] ? (event: never, ...args: any[]) => void : (__U extends unknown ? (fn: __U) => void : never) extends (fn: infer __I) => void ? __I : never;
type __RuntimePropValue<T> = T extends abstract new (...args: any[]) => infer V ? V : T extends (...args: any[]) => infer V ? V : never;
type __RuntimePropCtorInner<T> = T extends null | undefined ? never : T extends readonly (infer U)[] ? __RuntimePropCtorInner<U> : T extends { type: infer U } ? __RuntimePropCtorInner<U> : T extends StringConstructor ? string : T extends NumberConstructor ? number : T extends BooleanConstructor ? boolean : T extends ArrayConstructor ? unknown[] : T extends ObjectConstructor ? Record<string, any> : T extends DateConstructor ? Date : T extends FunctionConstructor ? (...args: any[]) => any : __RuntimePropValue<T>;
type __RuntimePropCtor<T> = [__RuntimePropCtorInner<T>] extends [never] ? unknown : __RuntimePropCtorInner<T>;
type __RuntimePropHasBoolean<T> = T extends BooleanConstructor ? true : T extends readonly (infer U)[] ? __RuntimePropHasBoolean<U> : T extends { type: infer U } ? __RuntimePropHasBoolean<U> : false;
type __RuntimePropResolved<T> = T extends { required: true } ? true : T extends { default: any } ? true : __RuntimePropHasBoolean<T>;
type __RuntimePropShape<T extends Record<string, any>> = { [K in keyof T]: __RuntimePropResolved<T[K]> extends true ? __RuntimePropCtor<T[K]> : __RuntimePropCtor<T[K]> | undefined; };
type __DefaultFactory<T> = (props: any) => T;
type __WithDefaultValue<T> = T | __DefaultFactory<T>;
type __LooseRequired<T> = { [P in keyof (T & Required<T>)]: T[P] };
type __VizeBooleanKey<T, K extends keyof T = keyof T> = K extends any ? [Exclude<T[K], undefined>] extends [never] ? never : [Exclude<T[K], undefined>] extends [boolean] ? K : never : never;
type __DefineProps<T, __BKeys extends keyof T = never> = __LooseRequired<T>;
type __WithDefaultsArgs<T> = { [K in keyof T]?: __WithDefaultValue<T[K]> };
type __WithDefaultsResult<T, D, __BKeys extends keyof T = never, __Props = __LooseRequired<T>> = T extends unknown ? Omit<__Props, keyof D> & { [K in keyof D & keyof T]-?: [D[K]] extends [undefined] ? __LooseRequired<T>[K] : Exclude<__Props[K & keyof __Props], undefined> } : never;
type __Ref<T> = { value: T };
type __VizeModelModifiers<M extends PropertyKey> = Record<M, true | undefined>;
type __VizeWritableRef<G, S> = Omit<__Ref<G>, 'value'> & { get value(): G; set value(value: S); };
type __VizeModelRef<T, M extends PropertyKey = string, G = T, S = T> = __VizeWritableRef<G, S> & [__VizeModelRef<T, M, G, S>, __VizeModelModifiers<M>];
type __ShallowRef<T> = __Ref<T> & { readonly __v_isShallow?: true };
type __VizeKebabCase<S extends string> = S extends `${infer Head}${infer Tail}` ? Head extends Lowercase<Head> ? `${Head}${__VizeKebabCase<Tail>}` : `-${Lowercase<Head>}${__VizeKebabCase<Tail>}` : S;
type __VizeKebabProps<T> = { [K in keyof T & string as __VizeKebabCase<K>]: T[K] };
type __VizeComponentProps<T> = T extends unknown ? T & Partial<__VizeKebabProps<T>> : never;
type __VizeIsAny<T> = 0 extends (1 & T) ? true : false;
type __VizeVue2LooseEventArg<T> = __VizeIsAny<T> extends true ? any : [T] extends [Object] ? ([Object] extends [T] ? any : T) : T;
declare type __VizeVue2LooseEmitArgs<A extends readonly unknown[]> = { [K in keyof A]: __VizeVue2LooseEventArg<A[K]> };
type __VForEntry<T> = T extends number ? [item: number, key: number, index: number] : T extends string ? [item: string, key: number, index: number] : T extends readonly (infer U)[] ? [item: U, key: number, index: number] : T extends Iterable<infer U> ? [item: U, key: number, index: number] : [item: T[keyof T], key: keyof T extends string ? keyof T : `${keyof T & (string | number)}`, index: number];
declare function __vForList<T>(source: T | undefined | null): readonly __VForEntry<NonNullable<T>>[];"#;
const LEGACY_REF_UNWRAP_HELPER: &str =
    "    type __U<T> = T extends { value: infer __V } ? __V : T;\n";
const LEGACY_EXPOSED_UNWRAP_HELPER: &str = "type __VizeShallowUnwrapRef<T> = { [K in keyof T]: T[K] extends { value: infer __V } ? __V : T[K] };\n";
/// Template-scope ref unwrapping for the modern (Vue 3) dialect when the shared
/// preamble is *not* hoisted (check server, content mapper).
///
/// The widening conditional types stay here, inside `__template()`, instead of
/// joining the module-scope preamble: `__U` is their only reference, and
/// `TemplateRefUnwraps::emit_template_variables` emits no `__U` at all for a
/// component with no setup bindings in template scope. Module-scope
/// declarations are module-local, so an unused one surfaces to the user as a
/// `TS6196` hint on their own file.
const MODERN_REF_UNWRAP_HELPER: &str = r#"    type __VizeIsUnion<T, __U = T> = T extends unknown ? ([__U] extends [T] ? false : true) : false;
    type __VizeWidenTemplateRef<T> = __VizeIsAny<T> extends true ? T : __VizeIsUnion<T> extends true ? T : T extends string ? string extends T ? string : T : T extends number ? number extends T ? number : T : T extends boolean ? boolean extends T ? boolean : T : T;
    type __U<T> = T extends import('vue').Ref ? __VizeWidenTemplateRef<T['value']> : T;
"#;
/// [`MODERN_REF_UNWRAP_HELPER`] for the hoisted path: the ambient helpers file
/// (`SHARED_PREAMBLE_DTS`) is a `.d.ts` global script, so it declares the two
/// widening types once per program and never reports them unused.
///
/// `__U` itself stays per file — it is dialect-dependent. Hoisting the two
/// type-parameterized aliases saves 369 bytes in every generated `.vue.ts` and
/// stops TypeScript instantiating a distinct declaration per file instead of
/// caching one (#3443, #3460).
const MODERN_HOISTED_REF_UNWRAP_HELPER: &str =
    "    type __U<T> = T extends import('vue').Ref ? __VizeWidenTemplateRef<T['value']> : T;\n";
const MODERN_GENERIC_REF_UNWRAP_HELPER: &str =
    "    type __U<T> = T extends import('vue').Ref ? T['value'] : T;\n";
const MODERN_EXPOSED_UNWRAP_HELPER: &str = "type __VizeShallowUnwrapRef<T> = { [K in keyof T]: T[K] extends import('vue').Ref<infer __V> ? __V : T[K] };\n";
const LEGACY_DEFINE_COMPONENT_HELPER: &str = r#"type __VizeNuxt2Context = {
  app: any;
  route: any;
  params: Record<string, string>;
  query: Record<string, string | string[] | undefined>;
  store: any;
  error: (...args: any[]) => any;
  redirect: (...args: any[]) => any;
  req?: any;
  res?: any;
  env?: Record<string, unknown>;
  isDev?: boolean;
  isHMR?: boolean;
} & Record<string, any>;
type __VizeNuxt2PageOptions = ThisType<any> & {
  validate?: (context: __VizeNuxt2Context) => unknown;
  asyncData?: (context: __VizeNuxt2Context) => any;
  fetch?: (context: __VizeNuxt2Context) => any;
  head?: any;
  layout?: any;
  layoutTransition?: any;
  loading?: any;
  middleware?: any;
  scrollToTop?: any;
  transition?: any;
};
declare function __vizeDefineComponent<T>(options: T & __VizeNuxt2PageOptions): T;
"#;
pub(super) const LEGACY_COMPONENT_INSTANCE_HELPER: &str = r#"type __VizeVue2ComponentInstance = {
  $el: Element;
  $refs: Record<string, any>;
  $attrs: Record<string, unknown>;
  $listeners: Record<string, unknown>;
  $children: any[];
  $scopedSlots: Record<string, unknown>;
  $parent: any;
  $root: any;
  $options: Record<string, any>;
  $data: Record<string, any>;
  $on: (...args: any[]) => any;
  $off: (...args: any[]) => any;
  $once: (...args: any[]) => any;
  $set: (...args: any[]) => any;
  $delete: (...args: any[]) => any;
  $watch: (...args: any[]) => any;
  $nextTick: (...args: any[]) => any;
  $forceUpdate: () => void;
  $destroy: () => void;
  $createElement: (...args: any[]) => any;
  _c: (...args: any[]) => any;
};
"#;

pub(super) fn needs_legacy_vue2_helpers(legacy_vue2: bool, dialect: VueVersion) -> bool {
    legacy_vue2 || matches!(dialect, VueVersion::V2 | VueVersion::V2_7)
}

pub(super) fn vue_type_helpers(legacy_vue2: bool, dialect: VueVersion) -> &'static str {
    if needs_legacy_vue2_helpers(legacy_vue2, dialect) {
        LEGACY_VUE_TYPE_HELPERS
    } else {
        VUE_TYPE_HELPERS
    }
}

pub(super) fn ref_unwrap_helper(
    legacy_vue2: bool,
    dialect: VueVersion,
    hoist_shared_preamble: bool,
) -> &'static str {
    if needs_legacy_vue2_helpers(legacy_vue2, dialect) {
        LEGACY_REF_UNWRAP_HELPER
    } else if hoist_shared_preamble {
        MODERN_HOISTED_REF_UNWRAP_HELPER
    } else {
        MODERN_REF_UNWRAP_HELPER
    }
}

pub(super) fn ref_unwrap_helper_for_template(
    legacy_vue2: bool,
    dialect: VueVersion,
    has_generic_param: bool,
    hoist_shared_preamble: bool,
) -> &'static str {
    if has_generic_param && !needs_legacy_vue2_helpers(legacy_vue2, dialect) {
        MODERN_GENERIC_REF_UNWRAP_HELPER
    } else {
        ref_unwrap_helper(legacy_vue2, dialect, hoist_shared_preamble)
    }
}

pub(super) fn exposed_unwrap_helper(legacy_vue2: bool, dialect: VueVersion) -> &'static str {
    if needs_legacy_vue2_helpers(legacy_vue2, dialect) {
        LEGACY_EXPOSED_UNWRAP_HELPER
    } else {
        MODERN_EXPOSED_UNWRAP_HELPER
    }
}

pub(super) fn define_component_helper(legacy_vue2: bool, dialect: VueVersion) -> &'static str {
    if needs_legacy_vue2_helpers(legacy_vue2, dialect) {
        LEGACY_DEFINE_COMPONENT_HELPER
    } else {
        DEFINE_COMPONENT_HELPER
    }
}

pub(super) fn instance_helper(legacy_vue2: bool, dialect: VueVersion) -> &'static str {
    if needs_legacy_vue2_helpers(legacy_vue2, dialect) {
        LEGACY_COMPONENT_INSTANCE_HELPER
    } else {
        ""
    }
}

/// [`instance_suffix`] for the generic component constructor, where the SFC's
/// type parameters are in scope and a generic `Exposed` alias must therefore be
/// instantiated rather than left to its declared defaults (#3354).
///
/// `exposed_generic_args` is `None` when the alias takes no parameters, which
/// keeps the output identical to [`instance_suffix`].
pub(super) fn generic_instance_suffix(
    legacy_vue2: bool,
    dialect: VueVersion,
    has_exposed_type: bool,
    exposed_generic_args: Option<&str>,
) -> vize_carton::String {
    let Some(args) = exposed_generic_args.filter(|_| has_exposed_type) else {
        return vize_carton::String::from(instance_suffix(legacy_vue2, dialect, has_exposed_type));
    };

    if needs_legacy_vue2_helpers(legacy_vue2, dialect) {
        cstr!("}} & __VizeVue2ComponentInstance & __VizeShallowUnwrapRef<Exposed<{args}>>;\n")
    } else {
        cstr!("}} & __VizeComponentPublicBase & __VizeShallowUnwrapRef<Exposed<{args}>>;\n")
    }
}

pub(super) fn instance_suffix(
    legacy_vue2: bool,
    dialect: VueVersion,
    has_exposed_type: bool,
) -> &'static str {
    match (
        needs_legacy_vue2_helpers(legacy_vue2, dialect),
        has_exposed_type,
    ) {
        (true, true) => "} & __VizeVue2ComponentInstance & __VizeShallowUnwrapRef<Exposed>;\n",
        (true, false) => "} & __VizeVue2ComponentInstance;\n",
        (false, true) => "} & __VizeComponentPublicBase & __VizeShallowUnwrapRef<Exposed>;\n",
        (false, false) => "} & __VizeComponentPublicBase;\n",
    }
}

/// Generate virtual TypeScript with Vue 2.7 / Nuxt 2 compatibility enabled.
pub fn generate_virtual_ts_with_offsets_legacy_vue2(
    summary: &Croquis,
    script_content: Option<&str>,
    template_ast: Option<&vize_relief::RootNode<'_>>,
    script_offset: u32,
    template_offset: u32,
    options: &VirtualTsOptions,
) -> VirtualTsOutput {
    generate_virtual_ts_with_offsets_and_checks(
        summary,
        script_content,
        template_ast,
        script_offset,
        template_offset,
        options,
        VirtualTsGenerationOptions {
            legacy_vue2: true,
            ..Default::default()
        },
    )
}

#[cfg(test)]
mod tests {
    use std::{
        path::{Path, PathBuf},
        process::Command,
    };

    use vize_carton::{
        String, append,
        config::VueVersion,
        corsa_resolver::{CorsaResolveRequest, resolve_corsa_executable},
        cstr,
    };

    use super::vue_type_helpers;

    #[test]
    fn v_for_helper_preserves_member_array_aliases_without_losing_object_keys() {
        let Some(tsgo) = resolve_test_tsgo_binary() else {
            return;
        };
        let case_dir = std::env::temp_dir()
            .join(cstr!("vize-legacy-vfor-helper-{}", std::process::id()).as_str());
        let _ = std::fs::remove_dir_all(&case_dir);
        std::fs::create_dir_all(&case_dir).unwrap();
        let test_path = case_dir.join("vfor-helper.ts");
        let mut source = String::new("");
        append!(source, "{}\n\n", vue_type_helpers(true, VueVersion::V2));
        source.push_str(
            r#"interface QuestionField {
  questionFieldInputType: string;
  question: string;
  type: string;
}
interface QuestionFormat {
  id: string;
  questionField: QuestionField[];
}

declare const anySource: any;
for (const [answer, i] of __vForList(anySource.questionField)) {
  const keyText = `${i}question`;
  answer.questionFieldInputType;
  void keyText;
}

const obj: { foo: number; bar: number } = { foo: 1, bar: 2 };
for (const [value, key] of __vForList(obj)) {
  const valueCheck: number = value;
  const keyCheck: "foo" | "bar" = key;
  void valueCheck;
  void keyCheck;
}

declare const questionFormat: QuestionFormat;
for (const [answer, i] of __vForList(questionFormat.questionField)) {
  const inputType: string = answer.questionFieldInputType;
  const index: number = i;
  void inputType;
  void index;
}
"#,
        );
        std::fs::write(&test_path, source.as_bytes()).unwrap();

        let output = Command::new(tsgo)
            .args([
                "--ignoreConfig",
                "--noEmit",
                "--strict",
                "--target",
                "ES2022",
            ])
            .arg(&test_path)
            .output()
            .unwrap();
        let stdout = std::str::from_utf8(&output.stdout).unwrap();
        let stderr = std::str::from_utf8(&output.stderr).unwrap();
        assert!(
            output.status.success(),
            "legacy v-for helper should keep any/member-array aliases off the object overload while preserving object key types\nstdout:\n{stdout}\nstderr:\n{stderr}"
        );

        let _ = std::fs::remove_dir_all(&case_dir);
    }

    fn resolve_test_tsgo_binary() -> Option<PathBuf> {
        if let Ok(path) = std::env::var("CORSA_PATH")
            && Path::new(&path).exists()
        {
            return Some(PathBuf::from(path));
        }

        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)?;
        resolve_corsa_executable(CorsaResolveRequest {
            explicit_path: None,
            project_root: Some(root),
        })
        .ok()
    }
}
