//! Virtual TypeScript generation for Vue SFC type checking.
//!
//! This module generates TypeScript code that represents a Vue SFC's
//! runtime behavior, enabling type checking of template expressions
//! and script setup bindings.
//!
//! Key design: Uses closures from Croquis scope information instead of
//! `declare const` to properly model Vue's template scoping.

#[cfg(test)]
mod class_component_props_tests;
mod component_reference;
#[cfg(test)]
mod define_emits_usage_tests;
#[cfg(test)]
mod dynamic_component_names_tests;
#[cfg(test)]
mod event_handler_tests;
mod expressions;
mod generator;
mod helpers;
mod import_meta;
pub mod incremental;
#[cfg(test)]
mod interface_extends_tests;
#[cfg(test)]
mod legacy_vue2_vuetify_tests;
mod macro_type_mappings;
pub mod mapping;
mod props;
#[cfg(test)]
mod public_instance_guard_tests;
mod scope;
mod semantic_links;
#[cfg(test)]
mod strict_template_global_fallback_tests;
#[cfg(test)]
mod strict_template_globals_tests;
#[cfg(test)]
mod strict_template_scope_tests;
#[cfg(test)]
mod tests;
mod types;
#[cfg(test)]
mod unknown_props_tests;

#[cfg(any(test, feature = "native"))]
pub(crate) use generator::generate_virtual_ts_with_offsets_and_checks;
pub use generator::{
    generate_virtual_ts, generate_virtual_ts_with_offsets,
    generate_virtual_ts_with_offsets_legacy_vue2, generate_virtual_ts_with_offsets_options_api,
};
#[cfg(feature = "native")]
pub(crate) use helpers::to_safe_identifier;
pub use helpers::{
    DECLARATION_HELPERS_DTS, SHARED_PREAMBLE_DTS, SHARED_PREAMBLE_FILE_NAME, VUE_SETUP_HELPERS,
    VUE_TYPE_HELPERS,
};
pub use semantic_links::{VizeSemanticLink, VizeSemanticLinkKind};
#[cfg(feature = "native")]
pub(crate) use types::CSS_MODULE_GLOBAL_MARKER;
pub use types::{TemplateGlobal, VirtualTsOptions, VirtualTsOutput, VizeMapping, VizeSubSpan};

/// Shared type-only component contract for plain-TS JSX lowering in batch and
/// editor paths. Keep one declaration source so the two consumers cannot drift.
///
/// `__VizeJsxFallthroughAttrs` admits `class`/`style`, `data-`/`aria-`
/// attributes, and native DOM listeners not shadowed by a declared prop. The
/// listener set is a **deliberate divergence from `vue-tsc`**, which rejects
/// `<Comp onClick={…}/>` outright for a component that does not declare it even
/// though Vue forwards the listener to the fallthrough root at runtime; vize
/// accepts it *and* contextually types the event payload, so
/// `onClick={(event: string) => …}` is still an error. See
/// `crates/vize/tests/check_jsx_component_contract_cli.rs`, which pins both
/// halves of that contract.
///
/// `__VizeJsxSlotPayload` is the JSX analogue of the `.vue` template path's
/// `slot_props_type` (`virtual_ts::scope::emit`): a scoped slot's parameter is
/// typed from the host component's declared `$slots`, falling back to `any`
/// whenever the host or the slot is untyped so untyped slot hosts never produce
/// a false positive.
///
/// It resolves `$slots` on *constructable* hosts only, which covers the imported
/// `.vue` components #4042 is about. A local function component declares its
/// slots on the second `Ctx<Emits, Slots>` parameter instead, and those payloads
/// are intentionally left `any` here: that fallback is the conservative side of
/// the trade (it can miss an error, never fabricate one), and typing them is
/// tracked separately from this fix.
pub const JSX_COMPONENT_HELPER: &str = "type __VizeJsxKebabCase<S extends string> = S extends `${infer H}${infer T}` ? H extends Lowercase<H> ? `${H}${__VizeJsxKebabCase<T>}` : `-${Lowercase<H>}${__VizeJsxKebabCase<T>}` : S;\n\
type __VizeJsxCamelCase<S extends string> = S extends `data-${string}` | `aria-${string}` ? S : S extends `${infer H}-${infer T}` ? `${H}${Capitalize<__VizeJsxCamelCase<T>>}` : S;\n\
type __VizeJsxRawPropKeys<R> = R extends unknown ? { [K in keyof R]-?: K extends string ? K | __VizeJsxKebabCase<K> : K }[keyof R] : never;\n\
type __VizeJsxCanonicalRawProps<R> = R extends unknown ? { [K in keyof R as K extends string ? __VizeJsxCamelCase<K> : K]: R[K] } : never;\n\
type __VizeJsxIsUnion<T, U = T> = T extends unknown ? ([U] extends [T] ? false : true) : false;\n\
type __VizeJsxUnionKeys<T> = T extends unknown ? keyof T : never;\n\
type __VizeJsxUnionValue<T, K extends PropertyKey> = T extends unknown ? K extends keyof T ? T[K] : never : never;\n\
type __VizeJsxLooseUnionProps<T> = { [K in __VizeJsxUnionKeys<T>]?: __VizeJsxUnionValue<T, K> };\n\
type __VizeJsxNormalizeProps<T> = __VizeJsxIsUnion<T> extends true ? __VizeJsxLooseUnionProps<T> : T;\n\
type __VizeJsxDomListenerProps = { [K in keyof GlobalEventHandlersEventMap as K extends string ? `on${Capitalize<K>}` : never]?: (event: GlobalEventHandlersEventMap[K]) => void };\n\
type __VizeJsxFallthroughAttrs<Owned = {}> = { class?: unknown; style?: unknown } & { [K in `data-${string}`]?: unknown } & { [K in `aria-${string}`]?: unknown } & Omit<__VizeJsxDomListenerProps, keyof Owned>;\n\
type __VizeJsxSfcComponentProps<I> = I extends { $props: infer P } ? I extends { readonly __vizeRawProps?: infer R } ? Omit<P, __VizeJsxRawPropKeys<R>> & __VizeJsxCanonicalRawProps<R> & __VizeJsxFallthroughAttrs<P> : __VizeJsxCanonicalRawProps<P> & __VizeJsxFallthroughAttrs<P> : any;\n\
type __VizeJsxComponentProps<C> = C extends abstract new (...args: any[]) => infer I ? __VizeJsxNormalizeProps<__VizeJsxSfcComponentProps<I>> : C extends (props: infer P, ...args: any[]) => any ? __VizeJsxNormalizeProps<__VizeJsxCanonicalRawProps<P> & __VizeJsxFallthroughAttrs<P>> : any;\n\
type __VizeJsxSlotPayload<C, N extends string> = C extends abstract new (...args: any[]) => infer I ? I extends { $slots: infer S } ? N extends keyof S ? NonNullable<S[N]> extends (props: infer P, ...args: any[]) => any ? P : any : any : any : any;\n\
declare function __vize_jsx_component_spread__<O>(value: O): __VizeJsxCanonicalRawProps<Omit<O, 'key' | 'ref'>>;\n\
declare function __vize_jsx_component__<C>(component: C, props: __VizeJsxComponentProps<C>): any;\n\
declare function __vize_jsx_component_slot__<C, N extends string>(component: C, name: N, render: (payload: __VizeJsxSlotPayload<C, N>) => unknown): any;\n";
#[cfg(any(test, feature = "native"))]
pub(crate) use types::{VirtualTsCheckOptions, VirtualTsGenerationOptions};

pub fn generate_virtual_ts_with_offsets_and_lib_references(
    summary: &vize_croquis::Croquis,
    script_content: Option<&str>,
    template_ast: Option<&vize_relief::RootNode<'_>>,
    script_offset: u32,
    template_offset: u32,
    options: &VirtualTsOptions,
    lib_references: &[&str],
) -> VirtualTsOutput {
    generator::generate_virtual_ts_with_offsets_and_checks(
        summary,
        script_content,
        template_ast,
        script_offset,
        template_offset,
        options,
        types::VirtualTsGenerationOptions {
            lib_references: Some(lib_references),
            ..Default::default()
        },
    )
}
