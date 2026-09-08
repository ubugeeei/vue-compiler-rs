use vize_carton::{FxHashSet, String, append, cstr};
use vize_croquis::croquis::{ComponentUsage, PassedProp};

use super::component_ref_props::{append_ref_callback_helper, is_inline_ref_callback_prop};
use super::inline_callback_classifier::is_direct_inline_function_prop_value;
use crate::virtual_ts::helpers::{to_camel_case, to_safe_identifier_fragment};

/// Whether this prop's authored value is an inline function, which is the only
/// shape that needs the `__VizeCallableProp` fallback.
///
/// The name filters mirror [`append_per_prop_aliases`] exactly, because the two
/// must agree: emitting the helper for a prop the alias loop then skips leaves
/// it unreferenced, which is `TS6196` on a clean SFC.
pub(crate) fn is_inline_callback_prop(prop: &PassedProp) -> bool {
    if prop.name_is_dynamic || prop.name.as_str() == "key" || prop.name.as_str() == "ref" {
        return false;
    }
    prop.is_dynamic
        && prop
            .value
            .as_ref()
            .is_some_and(|value| is_direct_inline_function_prop_value(value.as_str()))
}

/// Whether the value contains an inline callback whose standalone generation
/// would lose contextual typing. Legacy Vue 2 globals use this broader shape
/// to avoid reporting `TS7006` for values such as `[(value) => !!value]`.
pub(super) fn contains_inline_function_prop_value(value: &str) -> bool {
    let value = value.trim();
    value.contains("=>") || value.starts_with("function") || value.starts_with("async function")
}

pub(super) fn has_inference_props(usage: &ComponentUsage) -> bool {
    usage.props.iter().any(|prop| {
        !prop.name_is_dynamic && prop.name.as_str() != "key" && prop.name.as_str() != "ref"
    })
}

/// The target the usage's whole props object literal is checked against. A
/// literal that omits a required prop is rejected as a whole (`TS2345` on the
/// element); a literal that passes a wrong value is elaborated to one `TS2322`
/// on that prop, matching vue-tsc's one-error behavior for #3569.
///
/// The elaborated `TS2322` lands on the literal's key, which maps back to the
/// authored attribute name — the same position, code and message the per-prop
/// check produces for that prop, so `dedup_diagnostics` collapses the pair. That
/// is why no widening is needed to keep a wrong prop from being reported twice.
///
/// Checking against the raw type is also the cheapest thing that can be
/// generated, which is a correctness property of its own here. An earlier
/// attempt at #3569 inferred the authored object as a generic `A` and picked
/// between a complete and a relaxed target by testing `A` against a projection
/// of the child's declared prop keys. On real projects that machinery both blew
/// past TypeScript's union-complexity limit (`TS2590`) and widened authored
/// string literals through the `A extends Record<string, unknown>` constraint,
/// turning `align="start"` into `string` and reporting correct code as wrong.
/// A checker that only works on toy inputs is worse than the bug it fixes, so
/// the whole-props target carries no conditional types, no mapped types and no
/// inference variable.
///
/// Consequences, covered by component-props and project tests: generated
/// children accept only public attrs unless a recorded fallthrough target opens
/// the attr tail, opaque children keep the permissive tail, Vue public/listener
/// props do not satisfy required own props, inline callback contextual typing
/// stays intact, and the `exactOptionalPropertyTypes` absent-vs-`undefined`
/// distinction survives.
///
/// Vize's public `$props` accepts camel- and kebab-case aliases, which requires
/// camel-case keys to be optional there. The generated props literal already
/// camelizes every authored static name, so the internal `__vizeRawProps`
/// marker preserves the declaration's requiredness for this whole-object
/// check without constructing every camel/kebab key combination. External
/// components have no marker and continue to use their public `$props`.
///
/// Only the **non-generic** branch of `__VizePropChecker` uses this type; a
/// generic child resolves through its own `__vizeCheck` signature and ignores
/// it, so the generic inference path is untouched.
///
/// Note the code divergence. TypeScript 6, which `vue-tsc` pins, reports the
/// exact-optional rejection as `TS2379`; the native TypeScript 7 runtime
/// vize runs reports the identical code against the identical target as `TS2345`
/// with the same explanation nested one level down. Confirmed by running both
/// compilers over the same file across five target shapes, including
/// `vue-tsc`'s own. It is a compiler-version difference, not something the
/// generated code can steer.
pub(super) fn append_prop_checker_alias(
    ts: &mut String,
    usage: &ComponentUsage,
    component_type_name: &str,
    component_ref: &str,
    idx: usize,
) {
    append!(
        *ts,
        "  type __{component_type_name}_Component_{idx} = typeof {component_ref};\n",
    );
    // The listener props synthesized from the child's `emits` join the check
    // target (#3890): they are part of Vue's public props contract, `vue-tsc`
    // lists them in the displayed parameter type, and their presence is what
    // types an authored `:on-save` binding instead of absorbing it as
    // `unknown`. A component without the marker contributes `{}`.
    append!(
        *ts,
        "  type __{component_type_name}_CheckProps_{idx} = __{component_type_name}_Props_{idx} & __VizeEmitListeners<__{component_type_name}_Component_{idx}>;\n",
    );
    append!(
        *ts,
        "  type __{component_type_name}_CheckTail_{idx} = __VizeComponentCheckTail<__{component_type_name}_Component_{idx}>;\n",
    );
    if usage_needs_per_prop_aliases(usage) {
        append!(
            *ts,
            "  type __{component_type_name}_ValueProps_{idx} = __{component_type_name}_Props_{idx} & __VizeEmitListeners<__{component_type_name}_Component_{idx}> & __VizePublicComponentAttrs;\n",
        );
        append!(
            *ts,
            "  type __{component_type_name}_FallthroughValue_{idx}<K extends PropertyKey> = __VizeFallthroughValue<__{component_type_name}_Component_{idx}, K>;\n",
        );
    }
    append!(
        *ts,
        "  type __{component_type_name}_Check_{idx} = __VizePropChecker<__{component_type_name}_Component_{idx}, __{component_type_name}_CheckProps_{idx}, __{component_type_name}_CheckTail_{idx}>;\n",
    );
}

fn usage_needs_per_prop_aliases(usage: &ComponentUsage) -> bool {
    usage.props.iter().any(|prop| {
        (!prop.name_is_dynamic && prop.name.as_str() != "key" && prop.value.is_some())
            && (prop.name.as_str() != "ref" || is_inline_ref_callback_prop(prop))
    })
}

/// The shared type helpers every per-usage prop check resolves through, emitted
/// once per template scope that has at least one checkable component usage.
/// Keep this set small; the child's own props type carries the contract.
pub(super) fn append_prop_check_helpers(
    ts: &mut String,
    usages: &[(usize, &ComponentUsage)],
    check_unknown_props: bool,
) {
    ts.push_str("  type __VizeIsAny<T> = 0 extends (1 & T) ? true : false;\n");
    // Inline parameter shapes keep `TS2345` messages close to `vue-tsc` while
    // preserving optionality and contextual typing. The strict tail is used
    // only when Vize knows the generated child's fallthrough surface.
    ts.push_str(
        "  type __VizeEmitListeners<C> = C extends { __vizeEmitProps?: infer __E } ? __VizeIsAny<__E> extends true ? {} : NonNullable<__E> : {};\n",
    );
    ts.push_str(
        "  type __VizeIsGeneratedComponent<C> = C extends { readonly __vizeComponentMarker: true } ? true : false;\n",
    );
    ts.push_str(
        "  type __VizeHasFallthroughProps<C> = __VizeIsGeneratedComponent<C> extends true ? C extends { readonly __vizeHasFallthroughProps: true } ? true : false : false;\n",
    );
    ts.push_str(
        "  type __VizeFallthroughProps<C> = __VizeHasFallthroughProps<C> extends true ? C extends { readonly __vizeFallthroughProps?: infer __F } ? __VizeIsAny<__F> extends true ? {} : NonNullable<__F> : {} : {};\n",
    );
    ts.push_str("  type __VizeVueVNodeProps = import('vue').VNodeProps;\n");
    ts.push_str("  type __VizeVueAllowedComponentProps = import('vue').AllowedComponentProps;\n");
    ts.push_str("  type __VizeVueComponentCustomProps = import('vue').ComponentCustomProps;\n");
    ts.push_str(
        "  interface __VizePublicComponentAttrs extends __VizeVueVNodeProps, __VizeVueAllowedComponentProps, __VizeVueComponentCustomProps {}\n",
    );
    if check_unknown_props {
        // Real HTML attributes are valid Vue fallthrough on any component, so
        // the strict surface accepts every attribute some native element
        // declares — `id`, `type`, `accept`, kebab `aria-*` plus camelized
        // spellings, custom `data-*` — while a name no element knows
        // (`depressed`) stays a strict finding (#4966). Derived from the
        // `__VizeNativeElements` program alias; degrades to `{}` on a `vue`
        // without `NativeElements`, like the native prop checks degrade.
        ts.push_str(concat!(
            "  type __VizeAllowedFallthroughAttrs<C> = __VizeHasFallthroughProps<C> extends true ? Record<string, unknown> : {};\n",
            "  type __VizeAttrCamel<S extends string> = S extends `${infer __H}-${infer __T}` ? `${__H}${Capitalize<__VizeAttrCamel<__T>>}` : S;\n",
            "  type __VizeNativeAttrNames = { [K in keyof __VizeNativeElements & string]: keyof __VizeNativeElements[K] }[keyof __VizeNativeElements & string] & string;\n",
            "  type __VizeGlobalHtmlAttrs = __VizeIsAny<__VizeNativeElements> extends true ? {} : { [K in __VizeNativeAttrNames as K | __VizeAttrCamel<K>]?: unknown } & { [K in `data${string}`]?: unknown };\n",
            "  type __VizeComponentCheckTail<C> = __VizeIsGeneratedComponent<C> extends true ? __VizePublicComponentAttrs & __VizeGlobalHtmlAttrs & __VizeAllowedFallthroughAttrs<C> : Record<string, unknown>;\n",
        ));
    } else {
        ts.push_str(
            "  type __VizeComponentCheckTail<C> = __VizeIsGeneratedComponent<C> extends true ? __VizePublicComponentAttrs & Record<string, unknown> : Record<string, unknown>;\n",
        );
    }
    ts.push_str(
        "  type __VizeComponentCheckProps<P, T> = { readonly [K in keyof P]: P[K] } & T;\n",
    );
    ts.push_str(
        "  type __VizePublicProps<C> = C extends { new (): { $props: infer __P } } ? __P : C extends (props: infer __P) => any ? __P : {};\n",
    );
    ts.push_str(
        "  type __VizeInstanceRawProps<C> = C extends { new (): { readonly __vizeRawProps?: infer __P } } ? __VizeIsAny<__P> extends true ? __VizePublicProps<C> : __P : __VizePublicProps<C>;\n",
    );
    ts.push_str(
        "  type __VizeStaticRawProps<C> = C extends { readonly __vizeRawProps?: infer __P } ? __VizeIsAny<__P> extends true ? __VizePublicProps<C> : __P : __VizePublicProps<C>;\n",
    );
    ts.push_str(
        "  type __VizeHasInstanceRawProps<C> = __VizeIsGeneratedComponent<C> extends true ? C extends { new (): { readonly __vizeRawProps?: infer __P } } ? __VizeIsAny<__P> extends true ? false : true : false : false;\n",
    );
    ts.push_str(
        "  type __VizeHasStaticRawProps<C> = __VizeIsGeneratedComponent<C> extends true ? C extends { readonly __vizeRawProps?: infer __P } ? __VizeIsAny<__P> extends true ? false : true : false : false;\n",
    );
    ts.push_str(
        "  type __VizeComponentRawProps<C> = __VizeHasInstanceRawProps<C> extends true ? __VizeInstanceRawProps<C> : __VizeStaticRawProps<C>;\n",
    );
    ts.push_str(
        "  type __VizeHasRawProps<C> = __VizeHasInstanceRawProps<C> extends true ? true : __VizeHasStaticRawProps<C>;\n",
    );
    ts.push_str(
        "  type __VizeFallthroughValue<C, K extends PropertyKey> = __VizeHasFallthroughProps<C> extends true ? C extends { readonly __vizeFallthroughProps?: infer __F } ? __VizeIsAny<__F> extends true ? unknown : __VizePropValue<NonNullable<__F>, K, unknown> : unknown : unknown;\n",
    );
    if usages.iter().any(|(_, usage)| {
        usage
            .events
            .iter()
            .any(|event| !event.name_is_dynamic && !event.name.is_empty())
    }) {
        ts.push_str(
            "  type __VizeEventName<K extends string> = K extends `on${infer E}` ? Uncapitalize<E> | __VizeKebabCase<Uncapitalize<E>> : never;\n",
        );
        ts.push_str(
            "  type __VizeKebabEventAliases<E> = { [K in keyof E & string as __VizeKebabCase<K> extends K ? never : __VizeKebabCase<K>]: E[K] };\n",
        );
        ts.push_str(
            "  type __VizeComponentEvents<C> = C extends { __vizeRawEmits?: infer __R; __vizeEventMap?: infer __E } ? [keyof NonNullable<__R>] extends [never] ? NonNullable<__E> : NonNullable<__R> & __VizeKebabEventAliases<NonNullable<__R>> : { [K in keyof (C extends { new (): { $props: infer __P } } ? __P : C extends (props: infer __P) => any ? __P : {}) & string as __VizeEventName<K>]: (C extends { new (): { $props: infer __P } } ? __P : C extends (props: infer __P) => any ? __P : {})[K] };\n",
        );
    }
    ts.push_str(
        "  type __VizePropChecker<C, P, T = __VizeComponentCheckTail<C>> = __VizeIsAny<C> extends true ? (props: { readonly [K in keyof P]: P[K] } & Record<string, unknown>) => void : C extends { __vizeCheck: infer __F } ? __VizeIsAny<__F> extends true ? (props: __VizeComponentCheckProps<P, T>) => void : __F extends (...args: any[]) => any ? __F : (props: __VizeComponentCheckProps<P, T>) => void : (props: __VizeComponentCheckProps<P, T>) => void;\n",
    );
    ts.push_str(
        "  type __VizePropValue<P, K extends PropertyKey, F = unknown, __V = P extends unknown ? (K extends keyof P ? P[K] : never) : never> = [__V] extends [never] ? F : __V;\n",
    );
    // Emitted only when a usage actually binds an inline callback, because
    // nothing else references these aliases and an unreferenced one is
    // `TS6196`. That reaches
    // check-server clients as an unmapped hint on an otherwise clean SFC, the
    // same way the native element aliases did before #3443. The ambient
    // `declare function` trick those use is not available here: these helpers
    // are emitted inside a template scope's function body, not at module level.
    //
    // A generic child's props come from its `__vizeCheck<T>(props)` call, so
    // `__X_Props_N` is `Record<string, unknown>` and every per-prop alias
    // resolves to `unknown`. An inline callback prop annotated `unknown` has
    // no contextual type, so `strict` reports TS7006 on parameters that are
    // in fact contextually typed by the checker call below — a new error on
    // correct code (#3446). `__VizeCallableProp` remains the safe fallback for
    // components without Vize's resolver. A generic Vize child is invoked once
    // through `__VizePropsResolver`; `__VizeResolvedProp` then selects the
    // instantiated callback type so return errors surface inside the authored
    // body, at the same leaf byte as vue-tsc. `any` is excluded from the
    // fallback so a genuinely `any` prop stays assignable from a non-function
    // value, and a resolved non-generic prop type is returned untouched.
    if usages
        .iter()
        .any(|(_, usage)| usage.props.iter().any(is_inline_callback_prop))
    {
        ts.push_str(
            "  type __VizeCallableProp<T> = __VizeIsAny<T> extends true ? T : unknown extends T ? (...args: any[]) => any : T;\n",
        );
        ts.push_str(
            "  type __VizePropsResolver<C> = C extends { __vizeResolveProps?: infer __F } ? (__F extends (...args: any[]) => any ? __F : (props: any) => {}) : (props: any) => {};\n",
        );
        ts.push_str(
            "  type __VizePropsSelector<R> = <A extends Partial<R> & Record<string, unknown>>(props: A) => A;\n",
        );
        ts.push_str("  type __VizeMissingProp = { readonly __vizeMissingProp: unique symbol };\n");
        ts.push_str(
            "  type __VizeResolvedPropEntry<R, K extends PropertyKey> = R extends unknown ? K extends keyof R ? { value: R[K] } : __VizeMissingProp : never;\n",
        );
        ts.push_str(
            "  type __VizeSelectedProps<R, A> = R extends unknown ? A extends Partial<R> ? R : never : never;\n",
        );
        ts.push_str(
            "  type __VizeResolvedProp<R, A, K extends PropertyKey, F, __S = __VizeSelectedProps<R, A>, __E = __VizeResolvedPropEntry<__S, K>, __A = __VizeResolvedPropEntry<R, K>, __P = Extract<__E, { value: unknown }>> = [__S] extends [never] ? F : [__P] extends [never] ? [Extract<__A, { value: unknown }>] extends [never] ? F : never : __P extends { value: infer V } ? V : never;\n",
        );
    }
    append_ref_callback_helper(ts, usages);
}
/// The type a per-prop check is annotated with.
///
/// An inline callback prop gets the `__VizeCallableProp` fallback for a child
/// without `__vizeResolveProps`. Vize generic children replace it at the value
/// check with their instantiated resolver result. Every other prop keeps the
/// statically extracted type.
pub(super) fn prop_alias_type(
    prop: &PassedProp,
    component_type_name: &str,
    idx: usize,
    camel_prop_name: &str,
    fallthrough_prop_name: &str,
) -> String {
    if prop.name.as_str() == "ref" && is_inline_ref_callback_prop(prop) {
        return String::from("__VizeComponentRefCallback");
    }
    let resolved = cstr!(
        "__VizePropValue<__{component_type_name}_ValueProps_{idx}, '{camel_prop_name}', __{component_type_name}_FallthroughValue_{idx}<'{fallthrough_prop_name}'>>"
    );
    if is_inline_callback_prop(prop) {
        cstr!("__VizeCallableProp<{resolved}>")
    } else {
        resolved
    }
}

/// One `__X_N_prop_<name>` alias per distinct prop name the usage binds.
///
/// A repeated attribute — a static `class` next to a bound `:class` — reuses the
/// same child prop type, and emitting the alias twice would be a `TS2300` in the
/// generated module, so the name set is deduplicated.
pub(super) fn append_per_prop_aliases(
    ts: &mut String,
    usage: &ComponentUsage,
    component_type_name: &str,
    idx: usize,
) {
    let mut declared_aliases = FxHashSet::default();
    for prop in &usage.props {
        if prop.name_is_dynamic
            || prop.name.as_str() == "key"
            || (prop.name.as_str() == "ref" && !is_inline_ref_callback_prop(prop))
        {
            continue;
        }
        if prop.value.is_none() {
            continue;
        }
        let camel_prop_name = to_camel_case(prop.name.as_str());
        let safe_prop_name = to_safe_identifier_fragment(prop.name.as_str());
        if !declared_aliases.insert(safe_prop_name.clone()) {
            continue;
        }
        append!(
            *ts,
            "  type __{component_type_name}_{idx}_prop_{safe_prop_name} = {};\n",
            prop_alias_type(
                prop,
                component_type_name,
                idx,
                &camel_prop_name,
                prop.name.as_str()
            ),
        );
    }
}
