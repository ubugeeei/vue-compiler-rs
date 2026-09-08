use vize_croquis::macros::{
    DEFINE_EMITS, DEFINE_EXPOSE, DEFINE_MODEL, DEFINE_PROPS, DEFINE_SLOTS, WITH_DEFAULTS,
};

pub(crate) const USE_TEMPLATE_REF: &str = "useTemplateRef";

/// Names declared by the generated setup-scope helper block.
///
/// This includes Vue compiler macros plus runtime helper shims that are modeled
/// inside `__setup()`. It is intentionally broader than `COMPILER_MACRO_NAMES`.
pub(crate) const SETUP_SCOPE_HELPER_NAMES: &[&str] = &[
    DEFINE_PROPS,
    DEFINE_EMITS,
    DEFINE_EXPOSE,
    DEFINE_MODEL,
    DEFINE_SLOTS,
    WITH_DEFAULTS,
    USE_TEMPLATE_REF,
];

/// Shared type-helper text used both by the per-file embedded preamble and by
/// the hoisted ambient helpers file. Declared as a macro so the exact same
/// bytes can be spliced into both constants at compile time.
macro_rules! vue_type_aliases_text {
    () => {
        concat!(r#"type __EmitShape<T> = T extends (...args: any[]) => any ? T : T extends Record<string, any> ? { [K in keyof T]: T[K] extends (...args: infer A) => any ? A : T[K] extends any[] ? T[K] : any[]; } : Record<string, any[]>;
type __EmitArgs<T, K extends keyof T> = T[K] extends any[] ? T[K] : any[];
type __EmitFn<T, __S = __EmitShape<T>, __K extends keyof __S & string = keyof __S & string, __U = { [K in __K]: (event: K, ...args: __EmitArgs<__S, K>) => void }[__K]> = __S extends (...args: any[]) => any ? __S : [__K] extends [never] ? (event: never, ...args: any[]) => void : (__U extends unknown ? (fn: __U) => void : never) extends (fn: infer __I) => void ? __I : never;
type __RuntimePropValue<T> = T extends abstract new (...args: any[]) => infer V ? V : T extends (...args: any[]) => infer V ? V : never;
type __RuntimePropCtorInner<T> = T extends null | undefined ? never : T extends readonly (infer U)[] ? __RuntimePropCtorInner<U> : T extends { type: infer U } ? __RuntimePropCtorInner<U> : T extends StringConstructor ? string : T extends NumberConstructor ? number : T extends BooleanConstructor ? boolean : T extends ArrayConstructor ? unknown[] : T extends ObjectConstructor ? Record<string, any> : T extends DateConstructor ? Date : T extends FunctionConstructor ? (...args: any[]) => any : __RuntimePropValue<T>;
type __RuntimePropCtor<T> = [__RuntimePropCtorInner<T>] extends [never] ? unknown : __RuntimePropCtorInner<T>;
type __RuntimePropHasBoolean<T> = T extends BooleanConstructor ? true : T extends readonly (infer U)[] ? __RuntimePropHasBoolean<U> : T extends { type: infer U } ? __RuntimePropHasBoolean<U> : false;
type __RuntimePropResolved<T> = T extends { required: true } ? true : T extends { default: any } ? true : __RuntimePropHasBoolean<T>;
type __RuntimePropShape<T extends Record<string, any>> = { [K in keyof T]: __RuntimePropResolved<T[K]> extends true ? __RuntimePropCtor<T[K]> : __RuntimePropCtor<T[K]> | undefined; };
type __LooseRequired<T> = { [P in keyof (T & Required<T>)]: T[P] };
type __VizeBooleanKey<T, K extends keyof T = keyof T> = K extends any ? [Exclude<T[K], undefined>] extends [never] ? never : [Exclude<T[K], undefined>] extends [boolean] ? K : never : never; type __DefineProps<T, __BKeys extends keyof T = __VizeBooleanKey<T>> = Readonly<T> & { readonly [K in __BKeys]-?: boolean };
type __VizeIfAny<T, Y, N> = 0 extends (1 & T) ? Y : N;
type __VizeNotUndefined<T> = T extends undefined ? never : T;
type __VizeMappedOmit<T, K extends keyof any> = { [P in keyof T as P extends K ? never : P]: T[P] };
type __VizeDefaultNativeType = null | number | string | boolean | symbol | Function;
type __VizeInferDefault<P, T> = ((props: P) => T & {}) | (T extends __VizeDefaultNativeType ? T : never);
type __WithDefaultsArgs<T> = { [K in keyof T]?: __VizeInferDefault<T, T[K]> };
type __WithDefaultsResult<T, D, __BKeys extends keyof T = never> = T extends unknown ? Readonly<__VizeMappedOmit<T, keyof D>> & { readonly [K in keyof D as K extends keyof T ? K : never]-?: K extends keyof T ? D[K] extends undefined ? __VizeIfAny<D[K], __VizeNotUndefined<T[K]>, T[K]> : __VizeNotUndefined<T[K]> : never } & { readonly [K in __BKeys]-?: K extends keyof D ? D[K] extends undefined ? boolean | undefined : boolean : boolean } : never;
type __Ref<T> = import('vue').Ref<T>;
type __VizeModelModifiers<M extends PropertyKey> = Record<M, true | undefined>;
type __VizeWritableRef<G, S> = Omit<__Ref<G>, 'value'> & { get value(): G; set value(value: S); };
type __VizeModelRef<T, M extends PropertyKey = string, G = T, S = T> = __VizeWritableRef<G, S> & [__VizeModelRef<T, M, G, S>, __VizeModelModifiers<M>];
type __ShallowRef<T> = import('vue').ShallowRef<T>;
type __VizeIsAny<T> = 0 extends (1 & T) ? true : false;
type __VizeKebabCase<S extends string> = S extends `${infer Head}${infer Tail}` ? Head extends Lowercase<Head> ? `${Head}${__VizeKebabCase<Tail>}` : `-${Lowercase<Head>}${__VizeKebabCase<Tail>}` : S;
type __VizeKebabProps<T> = { [K in keyof T & string as __VizeKebabCase<K>]: T[K] };"#, "\n", include_str!("../component_prop_helpers.txt"))
    };
}

macro_rules! v_for_list_decls_text {
    () => {
        r#"type __VForEntry<T> = T extends number ? [item: number, key: number, index: number] : T extends string ? [item: string, key: number, index: number] : T extends readonly (infer U)[] ? [item: U, key: number, index: number] : T extends Iterable<infer U> ? [item: U, key: number, index: number] : [item: T[keyof T], key: keyof T extends string ? keyof T : `${keyof T & (string | number)}`, index: number];
declare function __vForList<T>(source: T | undefined | null): readonly __VForEntry<NonNullable<T>>[];"#
    };
}

macro_rules! vue_type_helpers_text {
    () => {
        concat!(vue_type_aliases_text!(), "// @ts-ignore TS2694/TS2307: a `vue` without `NativeElements` must degrade native prop checks to unchecked, never error. See virtual_ts/expressions/native_props.rs.\ntype __VizeNativeElements = import('vue').NativeElements;\ntype __VizeNativeElement<Tag extends PropertyKey> = Tag extends keyof __VizeNativeElements ? __VizeNativeElements[Tag] : unknown;\ntype __VizeNativeElementProp<Element, Prop extends PropertyKey> = Prop extends keyof Element ? Element[Prop] : unknown;\ndeclare function __vizeNativeElementProp<__Tag extends PropertyKey, __Prop extends PropertyKey>(value: __VizeNativeElementProp<__VizeNativeElement<__Tag>, __Prop>): void;\ntype __VizeComponentAttrCamel<S extends string> = S extends `${infer __H}-${infer __T}` ? `${__H}${Capitalize<__VizeComponentAttrCamel<__T>>}` : S;\ntype __VizeComponentNativeAttrNames = { [K in keyof __VizeNativeElements & string]: keyof __VizeNativeElements[K] }[keyof __VizeNativeElements & string] & string;\ntype __VizeComponentDataAttrs = { [K in `data-${string}` | `data${Capitalize<string>}`]?: unknown };\ntype __VizeComponentGlobalHtmlAttrs = __VizeIsAny<__VizeNativeElements> extends true ? {} : { [K in __VizeComponentNativeAttrNames as K | __VizeComponentAttrCamel<K>]?: unknown } & __VizeComponentDataAttrs;\ndeclare function __vizeComponentGlobalHtmlAttrs(value: __VizeComponentGlobalHtmlAttrs): void;\n// @ts-ignore TS2694/TS2307: a `vue` without `Directive` must degrade custom directive value checks to unchecked, never error. See virtual_ts/expressions/directive_values.rs.\ntype __VizeDirectiveValue<D> = D extends import('vue').Directive<any, infer V> ? V : unknown;\ndeclare function __vizeDirectiveValue<__D>(value: __VizeDirectiveValue<__D>): void;\n", v_for_list_decls_text!())
    };
}

/// Emit-overload helper text shared between the per-file embedded emission and
/// the hoisted ambient helpers file.
macro_rules! emit_overload_helpers_text {
    () => {
        concat!(
            "type __VizeOverloadProps<TOverload> = Pick<TOverload, keyof TOverload>;\n",
            "type __VizeOverloadUnionRecursive<TOverload, TPartialOverload = unknown, TDepth extends unknown[] = []> = TDepth['length'] extends 32 ? never : TOverload extends (...args: infer TArgs) => infer TReturn ? TPartialOverload extends TOverload ? never : __VizeOverloadUnionRecursive<TPartialOverload & TOverload, TPartialOverload & ((...args: TArgs) => TReturn) & __VizeOverloadProps<TOverload>, [...TDepth, unknown]> | ((...args: TArgs) => TReturn) : never;\n",
            "type __VizeOverloadUnion<TOverload extends (...args: any[]) => any> = Exclude<__VizeOverloadUnionRecursive<(() => never) & TOverload>, TOverload extends () => never ? never : () => never>;\n",
            "type __VizeOverloadParameters<T extends (...args: any[]) => any> = Parameters<__VizeOverloadUnion<T>>;\n",
            "type __VizeIsStringLiteral<T> = T extends string ? string extends T ? false : true : false;\n",
            "type __VizeParametersToFns<T extends any[]> = string extends T[0] ? { [K in string]: (...args: T extends [e: any, ...args: infer P] ? P : any[]) => any } : { [K in T[0]]: __VizeIsStringLiteral<K> extends true ? (...args: T extends [e: infer E, ...args: infer P] ? K extends E ? P : never : never) => any : never };\n",
            "type __EmitOptions<T> = { [K in keyof __EmitShape<T> & string]: (...args: __EmitArgs<__EmitShape<T>, K>) => any } & (__EmitShape<T> extends (...args: any[]) => any ? __VizeParametersToFns<__VizeOverloadParameters<__EmitShape<T>>> : {});\ntype __VizeCamelize<S extends string> = S extends `${infer Head}-${infer Tail}` ? `${Head}${Capitalize<__VizeCamelize<Tail>>}` : S;\ntype __VizeHandlerKey<K extends string> = `on${Capitalize<__VizeCamelize<K>>}`;\n",
        )
    };
}

pub const VUE_TYPE_HELPERS: &str = vue_type_helpers_text!();
pub(crate) const EMIT_OVERLOAD_HELPERS: &str = emit_overload_helpers_text!();
pub(crate) const EMIT_PROPS_HELPER: &str = "type __EmitProps<T> = { [K in keyof __EmitOptions<T> & string as __VizeHandlerKey<K>]?: __EmitOptions<T>[K] };\n";

pub const VUE_SETUP_HELPERS: &str = r#"  // Compiler macros (only valid in setup scope, not global)
  function defineProps<_T = unknown>(): __DefineProps<__LooseRequired<_T>, Extract<__VizeBooleanKey<_T>, keyof __LooseRequired<_T>>>;
  function defineProps<const _T extends readonly string[]>(_props: _T): { [K in _T[number]]?: any };
  function defineProps<const _T extends Record<string, any>>(_props: _T): __RuntimePropShape<_T>;
  function defineProps(_props?: any) { void _props; return undefined as any; }
  function defineEmits<_T = unknown>(): __EmitFn<_T>;
  function defineEmits<const _T extends readonly string[]>(_events: _T): (event: _T[number], ...args: any[]) => void;
  function defineEmits<const _T extends Record<string, any>>(_events: _T): __EmitFn<_T>;
  function defineEmits(_events?: any) { void _events; return (() => {}) as any; }
  function defineExpose<_T = unknown>(_exposed?: _T): void { void _exposed; }
  function defineModel<_T = unknown, _M extends PropertyKey = string, _G = _T, _S = _T>(): __VizeModelRef<_T | undefined, _M, _G | undefined, _S | undefined>;
  function defineModel<_T = unknown, _M extends PropertyKey = string, _G = _T, _S = _T>(_options: any): __VizeModelRef<_T, _M, _G, _S>;
  function defineModel<_T = unknown, _M extends PropertyKey = string, _G = _T, _S = _T>(_name: string, _options?: any): __VizeModelRef<_T, _M, _G, _S>;
  function defineModel(_name_or_options?: any, _options?: any) { void _name_or_options; void _options; return undefined as any; }
  function defineSlots<_T = unknown>(): _T { return undefined as unknown as _T; }
  function withDefaults<_T, _BKeys extends keyof _T, _D extends __WithDefaultsArgs<_T>>(_props: __DefineProps<_T, _BKeys>, _defaults: _D): __WithDefaultsResult<_T, _D, _BKeys>; function withDefaults(_props: any, _defaults: any) { void _props; void _defaults; return undefined as any; }
  function useTemplateRef<_T = any>(_key: string): __ShallowRef<_T | null> { void _key; return undefined as unknown as __ShallowRef<_T | null>; }
  // Mark compiler macros as used
  void defineProps; void defineEmits; void defineExpose; void defineModel; void defineSlots; void withDefaults; void useTemplateRef;"#;

pub(crate) const VUE_SETUP_HELPERS_HOISTED: &str = r#"  // Compiler macros (setup-scope only; signatures hoisted to the shared helpers file)
  const defineProps = __vize_defineProps;
  const defineEmits = __vize_defineEmits;
  const defineExpose = __vize_defineExpose;
  const defineModel = __vize_defineModel;
  const defineSlots = __vize_defineSlots;
  const withDefaults = __vize_withDefaults;
  const useTemplateRef = __vize_useTemplateRef;
  // Mark compiler macros as used
  void defineProps; void defineEmits; void defineExpose; void defineModel; void defineSlots; void withDefaults; void useTemplateRef;"#;

pub const SHARED_PREAMBLE_FILE_NAME: &str = "__vize_helpers.d.ts";

pub const SHARED_PREAMBLE_DTS: &str = concat!(
    "// ============================================\n",
    "// Shared ambient helpers for vize virtual TypeScript\n",
    "// Generated by vize\n",
    "// ============================================\n",
    "// Global script: one copy of these declarations per program replaces the\n",
    "// preamble previously embedded in every generated .vue.ts module.\n",
    "\n",
    "// ImportMeta augmentation (reference existing framework types)\n",
    "/// <reference types=\"vite/client\" />\n",
    "// Extend ImportMeta with Nuxt-specific properties not covered by vite/client\n",
    "interface ImportMeta {\n",
    "  client: boolean;\n",
    "  server: boolean;\n",
    "  dev: boolean;\n",
    "  prod: boolean;\n",
    "  ssr: boolean;\n",
    "}\n\ndeclare namespace JSX { interface IntrinsicAttributes { class?: unknown; style?: unknown; } }\n",
    "\n",
    "// Shared type helpers used by generated virtual modules\n",
    vue_type_helpers_text!(),
    "\n\n// Template-ref widening. Hoisted-only, deliberately absent from the per-file\n// preamble: a module-scope copy is dead code (TS6196) in every component whose\n// template scope emits no __U, and only generated template scopes use it.\ntype __VizeIsUnion<T, __U = T> = T extends unknown ? ([__U] extends [T] ? false : true) : false;\ntype __VizeWidenTemplateRef<T> = __VizeIsUnion<T> extends true ? T : T extends string ? keyof T extends keyof string ? string : T : T extends number ? keyof T extends keyof number ? number : T : T extends boolean ? keyof T extends keyof boolean ? boolean : T : T;\n\n",
    "// Emit-overload helpers (consumed by the per-file __EmitProps alias)\n",
    emit_overload_helpers_text!(),
    "\n",
    "// Compiler-macro signatures (aliased inside each module's __setup() scope)\n",
    "declare function __vize_defineProps<_T = unknown>(): __DefineProps<__LooseRequired<_T>, Extract<__VizeBooleanKey<_T>, keyof __LooseRequired<_T>>>;\n",
    "declare function __vize_defineProps<const _T extends readonly string[]>(_props: _T): { [K in _T[number]]?: any };\n",
    "declare function __vize_defineProps<const _T extends Record<string, any>>(_props: _T): __RuntimePropShape<_T>;\n",
    "declare function __vize_defineEmits<_T = unknown>(): __EmitFn<_T>;\n",
    "declare function __vize_defineEmits<const _T extends readonly string[]>(_events: _T): (event: _T[number], ...args: any[]) => void;\n",
    "declare function __vize_defineEmits<const _T extends Record<string, any>>(_events: _T): __EmitFn<_T>;\n",
    "declare function __vize_defineExpose<_T = unknown>(_exposed?: _T): void;\n",
    "declare function __vize_defineModel<_T = unknown, _M extends PropertyKey = string, _G = _T, _S = _T>(): __VizeModelRef<_T | undefined, _M, _G | undefined, _S | undefined>;\n",
    "declare function __vize_defineModel<_T = unknown, _M extends PropertyKey = string, _G = _T, _S = _T>(_options: any): __VizeModelRef<_T, _M, _G, _S>;\n",
    "declare function __vize_defineModel<_T = unknown, _M extends PropertyKey = string, _G = _T, _S = _T>(_name: string, _options?: any): __VizeModelRef<_T, _M, _G, _S>;\n",
    "declare function __vize_defineSlots<_T = unknown>(): _T;\n",
    "declare function __vize_withDefaults<_T, _BKeys extends keyof _T, _D extends __WithDefaultsArgs<_T>>(_props: __DefineProps<_T, _BKeys>, _defaults: _D): __WithDefaultsResult<_T, _D, _BKeys>;\n",
    "declare function __vize_useTemplateRef<_T = any>(_key: string): __ShallowRef<_T | null>;\n",
);

pub const DECLARATION_HELPERS_DTS: &str = concat!(
    "// Shared helper types for vize-generated declaration files.\n",
    "// Generated by vize\n",
    vue_type_aliases_text!(),
    emit_overload_helpers_text!(),
);
