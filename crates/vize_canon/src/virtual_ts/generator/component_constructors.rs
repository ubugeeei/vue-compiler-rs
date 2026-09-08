//! Emission of the component instance type and its constructors.
//!
//! The generic constructor declares the SFC's type parameters, so module-scope
//! aliases that re-declared those parameters must be instantiated inside it: a
//! bare reference is legal TypeScript that silently resolves against the
//! alias's declared defaults instead of the caller's arguments (#3354).

use vize_carton::config::VueVersion;
use vize_carton::{String, append, cstr};

use super::legacy_vue2::{
    exposed_unwrap_helper, generic_instance_suffix, instance_helper, instance_suffix,
    needs_legacy_vue2_helpers,
};
use super::setup_props::SetupPropsPlan;

/// Which module-scope aliases took the SFC's type parameters, plus the flags
/// shaping the emitted instance type.
pub(super) struct ComponentInstanceAliases<'a> {
    pub(super) generic_params: Option<&'a (String, String)>,
    pub(super) slots_is_generic: bool,
    pub(super) emits_is_generic: bool,
    pub(super) exposed_is_generic: bool,
    pub(super) has_emits_for_props: bool,
    pub(super) has_exposed_type: bool,
    pub(super) has_authored_default: bool,
}

/// Reference to a module-scope alias inside the generic component constructor:
/// instantiated with the SFC's parameters when the alias re-declared them, bare
/// otherwise. Instantiating an alias that took no parameters is a hard
/// `TS2315`, so the decision is per alias.
fn alias_ref(name: &str, is_generic: bool, generic_names: &str) -> String {
    if is_generic {
        cstr!("{name}<{generic_names}>")
    } else {
        String::from(name)
    }
}

pub(super) fn emit_component_constructors(
    ts: &mut String,
    setup_props_plan: &SetupPropsPlan,
    aliases: &ComponentInstanceAliases<'_>,
    legacy_vue2: bool,
    dialect: VueVersion,
) {
    let legacy_component = needs_legacy_vue2_helpers(legacy_vue2, dialect);
    ts.push_str("// ========== Default Export ==========\n");
    ts.push_str(instance_helper(legacy_vue2, dialect));
    if aliases.has_exposed_type {
        ts.push_str(exposed_unwrap_helper(legacy_vue2, dialect));
    }
    if aliases.has_authored_default && legacy_component {
        ts.push_str("type __VizeComponentInstance = __VizeAuthoredInstance & {\n");
    } else if aliases.has_authored_default {
        ts.push_str(
            "type __VizeComponentInstance = Omit<__VizeAuthoredInstance, '$props' | '$emit' | '$slots'> & {\n",
        );
    } else {
        ts.push_str("type __VizeComponentInstance = {\n");
    }
    setup_props_plan.emit_component_props_field(
        ts,
        aliases.has_emits_for_props,
        aliases.generic_params.map(|(decl, _)| decl.as_str()),
        legacy_component,
    );
    if legacy_component {
        ts.push_str("  $emit: __EmitFn<Emits>;\n");
    } else {
        ts.push_str("  $emit: __VizeStrictPublicEmit<Emits>;\n");
    }
    ts.push_str("  $slots: Slots;\n");
    ts.push_str(instance_suffix(
        legacy_vue2,
        dialect,
        aliases.has_exposed_type,
    ));
    if legacy_component {
        ts.push_str(
            "type __VizeComponentConstructor = new (...args: any[]) => __VizeComponentInstance;\n",
        );
    } else {
        let props_ref = aliases
            .generic_params
            .map(|(decl, _)| {
                setup_props_plan.generic_fallback_component_props_type_ref(decl.as_str())
            })
            .unwrap_or_else(|| setup_props_plan.component_props_type_ref().into());
        let emit_props_ref = if aliases.has_emits_for_props {
            "__EmitProps<Emits>"
        } else {
            "{}"
        };
        // `__VizeComponentInput` is a conditional on the authored type
        // parameter, so the checker defers it instead of resolving the input
        // shape while it checks the alias declaration. That matters because
        // `__VizeComponentInputProps` reaches `__VizeFallthroughAttrs`, whose
        // `Omit<__VizeDomListenerProps, ...>` distributes over the whole DOM
        // event map: eagerly resolving it once per component made `vize check`
        // ~6% slower on a 1000-file corpus, and every file that never
        // instantiates the component paid it for nothing. Call sites still
        // resolve the exact same intersection once `__VizeAuthoredProps` is
        // inferred, so the authored contract is unchanged.
        append!(
            *ts,
            "type __VizeComponentConstructor = new <__VizeAuthoredProps = unknown>(props?: __VizeAuthoredProps & __VizeComponentInput<{props_ref}, {emit_props_ref}, __VizeAuthoredProps>, ...args: any[]) => __VizeUsePublicInstance<__VizeAuthoredProps> extends true ? __VizeComponentInstance : Omit<__VizeComponentInstance, '$props'> & {{\n\
  $props: __VizeAuthoredKeyWitness<__VizeAuthoredProps> & __VizeComponentInputProps<{props_ref}, {emit_props_ref}> & __VizeComponentInputGuard<{props_ref}, {emit_props_ref}, __VizeAuthoredProps>;\n\
}};\n"
        );
    }

    let Some((generic_decl, generic_names)) = aliases.generic_params else {
        return;
    };
    let slots_ref = alias_ref("Slots", aliases.slots_is_generic, generic_names);
    let emits_ref = alias_ref("Emits", aliases.emits_is_generic, generic_names);
    let emit_props_field = if aliases.has_emits_for_props {
        cstr!(" & __EmitProps<{emits_ref}>")
    } else {
        String::default()
    };
    if legacy_component {
        append!(
            *ts,
            "type __VizeGenericComponentConstructor = new <{generic_decl}>(...args: any[]) => "
        );
        if aliases.has_authored_default {
            ts.push_str("__VizeAuthoredInstance & ");
        }
        append!(
            *ts,
            "{{\n  $props: __VizeComponentProps<Props<{generic_names}>>{emit_props_field};\n  readonly __vizeRawProps?: Props<{generic_names}>;\n  $emit: __EmitFn<{emits_ref}>;\n  $slots: {slots_ref};\n"
        );
        ts.push_str(&generic_instance_suffix(
            legacy_vue2,
            dialect,
            aliases.has_exposed_type,
            aliases.exposed_is_generic.then_some(generic_names.as_str()),
        ));
        return;
    }

    let emit_props_ref = if aliases.has_emits_for_props {
        cstr!("__EmitProps<{emits_ref}>")
    } else {
        String::from("{}")
    };
    append!(
        *ts,
        "type __VizeGenericComponentInstance<{generic_decl}> = "
    );
    if aliases.has_authored_default {
        ts.push_str("Omit<__VizeAuthoredInstance, '$props' | '$emit' | '$slots'> & ");
    }
    append!(
        *ts,
        "{{\n  $props: Props<{generic_names}>{emit_props_field};\n  readonly __vizeRawProps?: Props<{generic_names}>;\n  $emit: __VizeStrictPublicEmit<{emits_ref}>;\n  $slots: {slots_ref};\n"
    );
    ts.push_str(&generic_instance_suffix(
        legacy_vue2,
        dialect,
        aliases.has_exposed_type,
        aliases.exposed_is_generic.then_some(generic_names.as_str()),
    ));
    // Deliberately not routed through `__VizeComponentInput`: a generic SFC
    // infers its own type parameters from the same argument, and hiding the
    // input shape behind a deferred conditional leaves listener parameters
    // without a contextual type (`TS7006` on `<Generic onPick={(value) => …} />`).
    // Generic components are rare, so they keep the eager spelling.
    append!(
        *ts,
        "type __VizeGenericComponentConstructor = new <{generic_decl}, __VizeAuthoredProps = unknown>(props?: __VizeAuthoredProps & __VizeComponentInputProps<Props<{generic_names}>, {emit_props_ref}> & __VizeComponentInputGuard<Props<{generic_names}>, {emit_props_ref}, __VizeAuthoredProps>, ...args: any[]) => __VizeUsePublicInstance<__VizeAuthoredProps> extends true ? __VizeGenericComponentInstance<{generic_names}> : Omit<__VizeGenericComponentInstance<{generic_names}>, '$props'> & {{\n  $props: __VizeAuthoredKeyWitness<__VizeAuthoredProps> & __VizeComponentInputProps<Props<{generic_names}>, {emit_props_ref}> & __VizeComponentInputGuard<Props<{generic_names}>, {emit_props_ref}, __VizeAuthoredProps>;\n}};\n"
    );
}
