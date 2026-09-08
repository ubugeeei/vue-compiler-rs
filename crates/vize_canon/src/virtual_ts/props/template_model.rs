//! The one normalized prop model every template-scope prop surface is built from.
//!
//! Vue resolves a component's declarations into exactly one prop contract: the
//! authored types with its own `default:`/`withDefaults` substitution applied.
//! The template's `props` const, the bare per-prop bindings and `$props` are all
//! views of that single contract, so they are all derived from
//! [`TemplatePropsModel`] rather than from separate scans.
//!
//! Before #4145 `$props` was the odd one out: it was emitted as a generic
//! instance global, `__VizeInstanceGlobal<'$props'>`, which reads
//! `ComponentPublicInstance['$props']` with that helper's *default* type
//! arguments. `P` defaults to `{}` there, so `$props` collapsed to `{}` for
//! every component and each declared prop read reported `TS2339`.

use vize_carton::{FxHashSet, String, append, cstr};
use vize_croquis::macros::MacroKind;
use vize_croquis::{Croquis, ScopeData, ScopeKind};

use super::super::helpers::to_safe_identifier;
use super::keyed_template_names::collect_keyed_template_prop_names;
use super::mappings::PropBindingMappings;
use super::setup_scoped::props_type_ref;
use super::template_bindings::{
    emit_macro_template_prop_bindings, should_skip_template_prop_binding,
};
use super::with_defaults::{
    collect_with_defaults_default_names_from_source, template_props_type_ref,
};
use super::{strip_generic_params, strip_outer_angle_brackets, type_reference_lookup_key};
use crate::virtual_ts::generator::setup_scope::define_props_type_requires_setup_scope;

mod type_shape;

use type_shape::{has_top_level_type_operator, is_plain_inline_type_literal};

/// The component's resolved prop contract as the template sees it.
pub(crate) struct TemplatePropsModel {
    /// The authored contract: `Props`, `Props<T>`, or the setup-scoped
    /// `__VizeSetupProps` when the type argument only resolves inside `__setup`.
    authored_type_ref: String,
    /// The authored contract with Vue's default substitution applied. This is
    /// what a template read of a prop resolves against.
    resolved_type_ref: String,
    /// Props whose declaration carries a value Vue substitutes when the prop is
    /// absent, so the resolved contract may not leave them `undefined`.
    defaulted_prop_names: FxHashSet<String>,
    /// Whether the component declares any prop at all. A component that
    /// declares none has `$props: {}` in Vue too, so there is nothing to model.
    declares_props: bool,
}

impl TemplatePropsModel {
    pub(crate) fn new(summary: &Croquis) -> Self {
        let props = summary.macros.props();
        let models = summary.macros.models();
        let define_props_type_args = summary
            .macros
            .define_props()
            .and_then(|m| m.type_args.as_ref());
        let declares_props =
            !props.is_empty() || !models.is_empty() || define_props_type_args.is_some();

        let setup_scoped = define_props_type_requires_setup_scope(summary);
        let generic_param = sfc_generic_param(summary);
        let authored_type_ref =
            props_type_ref(generic_param, setup_scoped.then_some("__VizeSetupProps"));

        let mut defaulted_prop_names = collect_with_defaults_default_names(summary);
        // A runtime `default:` is Vue's own substitution, so the prop is never
        // `undefined` inside its own template. The virtual project pass also
        // marks type-based `withDefaults` entries when their defaults resolve
        // through imported values.
        for prop in props {
            if prop.default_value.is_some() {
                defaulted_prop_names.insert(prop.name.as_str().into());
            }
        }
        for model in models {
            if model.default_value.is_some() {
                defaulted_prop_names.insert(model.name.as_str().into());
            }
        }

        // `__DefineProps` is the setup macro's own return type, but it applies
        // `{ [K in __VizeBooleanKey<T>]-?: boolean }`, and that key filter can
        // only be decided when `keyof T` is concrete. A generic SFC leaves it
        // deferred, so TypeScript must also consider the branch where a
        // string-typed prop *is* a boolean key and resolves it to `T[K] &
        // boolean` — which made authored `as string` casts fail with `TS2352`
        // across nuxt-ui and reka-ui (#4242). It stays exactly where it already
        // was: a type-only declaration with no defaults, whose keys are known.
        // Everywhere else the authored alias is used unwrapped, which also keeps
        // the generated helper name out of diagnostic text.
        let base_type_ref = if define_props_type_args.is_some() && defaulted_prop_names.is_empty() {
            cstr!("__DefineProps<{authored_type_ref}>")
        } else {
            authored_type_ref.clone()
        };
        let resolved_type_ref =
            template_props_type_ref(base_type_ref.as_str(), &defaulted_prop_names);

        Self {
            authored_type_ref,
            resolved_type_ref,
            defaulted_prop_names,
            declares_props,
        }
    }

    /// The template's `$props`. Vue's public instance props are the same
    /// resolved contract, readonly at the top level only — `vue-tsc` reports
    /// `TS2540` for `$props.x = …` while accepting a nested `$props.x.y = …`,
    /// which is what a shallow `Readonly` gives.
    ///
    /// `None` for a component that declares no props: `$props` then keeps the
    /// generic instance-global form, whose `{}` is what `vue-tsc` reports too.
    pub(crate) fn instance_props_type(&self) -> Option<String> {
        self.declares_props
            .then(|| cstr!("Readonly<{}>", self.resolved_type_ref))
    }
}

/// The SFC's `<script setup generic="…">` parameter list, which the generated
/// `Props` alias is instantiated with.
fn sfc_generic_param(summary: &Croquis) -> Option<&str> {
    summary
        .scopes
        .iter()
        .find(|scope| matches!(scope.kind, ScopeKind::ScriptSetup))
        .and_then(|scope| match scope.data() {
            ScopeData::ScriptSetup(data) => data.generic.as_ref().map(|generic| generic.as_str()),
            _ => None,
        })
}

fn collect_with_defaults_default_names(summary: &Croquis) -> FxHashSet<String> {
    let mut names = FxHashSet::default();
    for call in summary.macros.all_calls() {
        if call.kind != MacroKind::WithDefaults {
            continue;
        }
        let Some(runtime_args) = &call.runtime_args else {
            continue;
        };
        collect_with_defaults_default_names_from_source(runtime_args.as_str(), &mut names);
    }
    names
}

pub(crate) fn generate_props_variables(
    ts: &mut String,
    binding_mappings: &mut PropBindingMappings<'_>,
    summary: &Croquis,
    check_props: bool,
) {
    let model = TemplatePropsModel::new(summary);
    if !model.declares_props {
        return;
    }

    let props = summary.macros.props();
    let has_props = !props.is_empty();
    let models = summary.macros.models();
    let define_props_type_args = summary
        .macros
        .define_props()
        .and_then(|m| m.type_args.as_ref());

    let props_type_ref = model.authored_type_ref.as_str();
    let template_props_type_ref = model.resolved_type_ref.as_str();
    let defaulted_prop_names = &model.defaulted_prop_names;

    ts.push_str("  // Props are available in template as variables\n");
    ts.push_str("  // Access via `propName` or `props.propName`\n");
    append!(
        *ts,
        "  const props: {template_props_type_ref} = {{}} as {template_props_type_ref};\n"
    );
    ts.push_str("  void props; // Mark as used to avoid TS6133\n");

    let mut emitted_names = FxHashSet::default();
    if let Some(type_args) = define_props_type_args {
        let type_name = strip_outer_angle_brackets(type_args.trim());

        let type_properties = summary
            .types
            .extract_properties(type_reference_lookup_key(type_name));
        for prop in &type_properties {
            if should_skip_template_prop_binding(summary, prop.name.as_str()) {
                continue;
            }
            binding_mappings.emit(
                ts,
                template_props_type_ref,
                prop.name.as_str(),
                defaulted_prop_names.contains(&prop.name),
            );
            emitted_names.insert(prop.name.as_str().into());
        }
        if has_props {
            emit_macro_template_prop_bindings(
                ts,
                binding_mappings,
                summary,
                template_props_type_ref,
                props,
                defaulted_prop_names,
                &mut emitted_names,
            );
        }

        if should_emit_keyed_template_prop_bindings(summary, type_name, &emitted_names) {
            for name in collect_keyed_template_prop_names(summary, &emitted_names) {
                if check_props {
                    emit_keyed_template_prop_binding(
                        ts,
                        template_props_type_ref,
                        props_type_ref,
                        name.as_str(),
                        defaulted_prop_names.contains(&name),
                    );
                } else {
                    emit_unchecked_template_prop_binding(ts, name.as_str());
                }
            }
        }
    } else if has_props {
        // Runtime-declared props: generate individual variables
        emit_macro_template_prop_bindings(
            ts,
            binding_mappings,
            summary,
            template_props_type_ref,
            props,
            defaulted_prop_names,
            &mut emitted_names,
        );
    }
    for model in models {
        if emitted_names.contains(model.name.as_str())
            || should_skip_template_prop_binding(summary, model.name.as_str())
        {
            continue;
        }
        binding_mappings.emit(
            ts,
            template_props_type_ref,
            model.name.as_str(),
            model.default_value.is_some(),
        );
        emitted_names.insert(model.name.as_str().into());
    }
    ts.push('\n');
}

fn emit_keyed_template_prop_binding(
    ts: &mut String,
    props_type_ref: &str,
    key_type_ref: &str,
    prop_name: &str,
    has_default: bool,
) {
    let binding_name = to_safe_identifier(prop_name);
    if has_default {
        append!(
            *ts,
            "  const {binding_name} = props[(\"{prop_name}\" satisfies keyof {key_type_ref})] as Exclude<{props_type_ref}[\"{prop_name}\"], undefined>;\n"
        );
    } else {
        append!(
            *ts,
            "  const {binding_name} = props[(\"{prop_name}\" satisfies keyof {key_type_ref})];\n"
        );
    }
    append!(*ts, "  void {binding_name};\n");
}

fn emit_unchecked_template_prop_binding(ts: &mut String, prop_name: &str) {
    let binding_name = to_safe_identifier(prop_name);
    append!(
        *ts,
        "  const {binding_name} = (props as Record<string, unknown>)[\"{prop_name}\"];\n"
    );
    append!(*ts, "  void {binding_name};\n");
}

fn should_emit_keyed_template_prop_bindings(
    summary: &Croquis,
    type_name: &str,
    emitted_names: &FxHashSet<String>,
) -> bool {
    if has_top_level_type_operator(type_name) {
        return true;
    }
    if is_plain_inline_type_literal(type_name) {
        return false;
    }
    let base_name = strip_generic_params(type_name).trim();
    if summary.types.definitions().has_interface_extends(base_name) {
        return true;
    }
    if let Some(body) = summary.types.definitions().resolve(base_name) {
        return has_top_level_type_operator(body.as_str())
            || !is_plain_inline_type_literal(body.as_str());
    }
    emitted_names.is_empty() && !summary.types.definitions().is_defined(base_name)
}
