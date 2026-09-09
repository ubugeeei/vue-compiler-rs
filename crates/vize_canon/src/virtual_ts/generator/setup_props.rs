use vize_carton::{CompactString, FxHashSet, String, append, cstr, profile};
use vize_croquis::Croquis;

use super::generics::generic_fallback_args;
use super::setup_scope::define_props_type_requires_setup_scope;
use crate::virtual_ts::macro_type_mappings::MacroTypeMappings;
use crate::virtual_ts::props::{
    OptionsApiPropsSource, PropBindingMappings, PropsSource, PropsTypeEmission,
    add_generic_defaults, append_default_props, extract_generic_names, generate_props_type,
    generate_props_variables, generate_setup_scoped_props_artifact,
};

pub(super) use crate::virtual_ts::props::prop_source;

/// Build the setup props plan and emit the module-level props type in one step.
/// Keeps `generator.rs` from re-threading `options_api_props` through a second
/// call site (and from growing past the source-length gate).
pub(super) fn generate_setup_props(
    ts: &mut String,
    source: PropsSource<'_>,
    generic_param: Option<&str>,
    options_api_props: Option<&OptionsApiPropsSource>,
    type_only_imported_names: &FxHashSet<CompactString>,
    props_is_public_export: bool,
) -> SetupPropsPlan {
    let summary = source.summary;
    let plan = SetupPropsPlan::new(
        summary,
        options_api_props,
        type_only_imported_names,
        props_is_public_export,
    );
    let generated_start = ts.len();
    profile!("canon.virtual_ts.generate_props_type", {
        plan.generate_props_type(
            ts,
            summary,
            generic_param,
            options_api_props,
            type_only_imported_names,
        );
    });
    let mut type_mappings = MacroTypeMappings::new(source.mappings, source.script, source.offset);
    type_mappings.map_exported_type(ts, generated_start, summary.macros.define_props(), "Props");
    type_mappings.map_model_props(
        ts,
        generated_start,
        summary.macros.models(),
        &summary.macros,
    );
    plan
}

pub(super) struct SetupPropsPlan {
    defer: bool,
    defer_options_api_props: bool,
    capture_options_api_default: bool,
    module_scope_declares_props: bool,
    uses_resolved_props: bool,
}

impl SetupPropsPlan {
    pub(super) fn new(
        summary: &Croquis,
        options_api_props: Option<&OptionsApiPropsSource>,
        type_only_imported_names: &FxHashSet<CompactString>,
        props_is_public_export: bool,
    ) -> Self {
        // A `Props` declaration only lands at module scope when it is hoisted
        // (emitted directly at module level) or when it is a value-dependent
        // export recaptured through the setup return (`props_is_public_export`).
        // A private, non-hoisted `type Props` stays inside `__setup`, so it must
        // NOT be treated as an existing public alias — otherwise the public
        // `export type Props` consumers need is suppressed.
        let has_models = !summary.macros.models().is_empty();
        let define_props_is_authored = summary.macros.define_props().is_some();
        let props_imported_type_only = type_only_imported_names
            .iter()
            .any(|name| name.as_str() == "Props");
        let options_api_props_present = options_api_props.is_some();
        let options_api_props_are_deferred =
            options_api_props.is_some_and(|source| source.deferred_object_source().is_some());
        let imported_props_would_collide = props_imported_type_only
            && (define_props_is_authored || has_models || options_api_props_present);
        let module_scope_declares_props = props_is_public_export
            || summary
                .type_exports
                .iter()
                .any(|te| te.hoisted && te.name.as_str() == "Props")
            || imported_props_would_collide;
        let defer = define_props_type_requires_setup_scope(summary);
        Self {
            defer,
            defer_options_api_props: options_api_props_are_deferred
                && (!module_scope_declares_props || imported_props_would_collide),
            capture_options_api_default: options_api_props
                .is_some_and(OptionsApiPropsSource::captures_default),
            module_scope_declares_props,
            uses_resolved_props: module_scope_declares_props
                && (defer
                    || has_models
                    || (imported_props_would_collide
                        && (define_props_is_authored || options_api_props_present))),
        }
    }

    pub(super) fn props_type_emission(&self) -> PropsTypeEmission {
        if self.defer {
            PropsTypeEmission::DeferredToSetup
        } else {
            PropsTypeEmission::Module
        }
    }

    pub(super) fn generate_props_type(
        &self,
        ts: &mut String,
        summary: &Croquis,
        generic_param: Option<&str>,
        options_api_props: Option<&OptionsApiPropsSource>,
        type_only_imported_names: &FxHashSet<CompactString>,
    ) {
        generate_props_type(
            ts,
            summary,
            generic_param,
            options_api_props,
            type_only_imported_names,
            self.props_type_emission(),
        );
    }

    pub(super) fn generate_props_variables(
        &self,
        ts: &mut String,
        source: PropsSource<'_>,
        check_props: bool,
    ) {
        let summary = source.summary;
        let mut binding_mappings =
            PropBindingMappings::new(source.mappings, summary, source.script, source.offset);
        generate_props_variables(ts, &mut binding_mappings, summary, check_props);
    }

    pub(super) fn component_props_type_ref(&self) -> &'static str {
        // Resolve through the synthesized intersection when an authored
        // module-level `Props` must absorb models or deferred setup props.
        if self.uses_resolved_props {
            "__VizeResolvedProps"
        } else {
            "Props"
        }
    }

    pub(super) fn generic_fallback_component_props_type_ref(&self, generic_decl: &str) -> String {
        let props_type_ref = self.component_props_type_ref();
        if props_type_ref != "Props" {
            return props_type_ref.into();
        }

        let args = generic_fallback_args(generic_decl);
        if args.is_empty() {
            props_type_ref.into()
        } else {
            cstr!("{props_type_ref}<{args}>")
        }
    }

    pub(super) fn component_value_props_type_ref(
        &self,
        generic_component_params: Option<&(String, String)>,
    ) -> String {
        generic_component_params
            .map(|(decl, _)| self.generic_fallback_component_props_type_ref(decl.as_str()))
            .unwrap_or_else(|| self.component_props_type_ref().into())
    }

    pub(super) fn emit_component_props_field(
        &self,
        mut ts: &mut String,
        has_emits_for_props: bool,
        generic_decl: Option<&str>,
        legacy_input_aliases: bool,
    ) {
        let props_type_ref = generic_decl
            .map(|decl| self.generic_fallback_component_props_type_ref(decl))
            .unwrap_or_else(|| self.component_props_type_ref().into());
        let public_props_type_ref = if legacy_input_aliases {
            cstr!("__VizeComponentProps<{props_type_ref}>")
        } else {
            props_type_ref.clone()
        };
        if has_emits_for_props {
            append!(
                ts,
                "  $props: {public_props_type_ref} & __EmitProps<Emits>;\n"
            );
        } else {
            append!(ts, "  $props: {public_props_type_ref};\n");
        }
        // Vize's generated whole-object template check extracts this marker
        // instead of the JSX input constructor. Both stay canonical: call-site
        // camel/kebab aliases are an input concern, not a public-instance type.
        append!(ts, "  readonly __vizeRawProps?: {props_type_ref};\n");
    }

    pub(super) fn emit_artifact(&self, ts: &mut String, source: PropsSource<'_>) {
        if self.defer {
            let generated_start = ts.len();
            generate_setup_scoped_props_artifact(ts, source.summary);
            MacroTypeMappings::new(source.mappings, source.script, source.offset).map_model_props(
                ts,
                generated_start,
                source.summary.macros.models(),
                &source.summary.macros,
            );
        }
    }

    pub(super) fn push_return_field(&self, fields: &mut Vec<&'static str>) {
        if self.capture_options_api_default {
            fields.push("__default__");
        }
        if self.defer {
            fields.push("__vize_setup_props");
        }
        if self.defer_options_api_props {
            fields.push("__vize_options_props");
        }
    }

    pub(super) fn emit_options_api_artifact(
        &self,
        mut ts: &mut String,
        options_api_props: Option<&OptionsApiPropsSource>,
    ) {
        let Some(source) =
            options_api_props.and_then(OptionsApiPropsSource::deferred_object_source)
        else {
            return;
        };
        if self.module_scope_declares_props && !self.defer_options_api_props {
            return;
        }
        let const_assertion = if source.trim_start().starts_with('{') {
            " as const"
        } else {
            ""
        };
        append!(
            ts,
            "\n  const __vize_options_props = ({source}{const_assertion});\n"
        );
    }

    pub(super) fn emit_module_export(
        &self,
        ts: &mut String,
        options_api_props: Option<&OptionsApiPropsSource>,
    ) {
        if self.defer {
            if self.module_scope_declares_props {
                // A `Props` already lives at module scope (hoisted, or restored
                // by the setup type-export plan); emit an internal alias for the
                // component instance to avoid a duplicate public declaration.
                ts.push_str(
                    "type __VizeResolvedProps = Awaited<ReturnType<typeof __setup>>[\"__vize_setup_props\"];\n\n",
                );
            } else {
                // No `Props` exists at module scope (inline type args, or a
                // private setup-scoped `Props`), so restore the public alias.
                ts.push_str(
                    "export type Props = Awaited<ReturnType<typeof __setup>>[\"__vize_setup_props\"];\n\n",
                );
            }
        } else if self.defer_options_api_props {
            let Some(source) =
                options_api_props.filter(|source| source.deferred_object_source().is_some())
            else {
                return;
            };
            if self.module_scope_declares_props {
                ts.push_str(
                    "type __VizeResolvedProps = __VizeOptionsPropShape<Awaited<ReturnType<typeof __setup>>[\"__vize_options_props\"]>",
                );
            } else {
                ts.push_str(
                    "export type Props = __VizeOptionsPropShape<Awaited<ReturnType<typeof __setup>>[\"__vize_options_props\"]>",
                );
            }
            append_default_props(ts, source);
            ts.push_str(";\n\n");
        }
    }

    pub(super) fn generic_component_params(
        &self,
        generic_param: Option<&str>,
    ) -> Option<(String, String)> {
        generic_param.filter(|_| !self.defer).map(|generic| {
            (
                add_generic_defaults(generic),
                extract_generic_names(generic),
            )
        })
    }
}
