//! Component event listener type generation.

mod generic_inference;
mod handler_context;

use vize_carton::{CompactString, FxHashSet, String, append, cstr};
use vize_croquis::{Croquis, EventHandlerScopeData, Scope, naming::to_pascal_case};

use crate::virtual_ts::component_reference::component_binding_reference;
use crate::virtual_ts::helpers::{to_safe_identifier, to_safe_identifier_fragment};
use crate::virtual_ts::types::VirtualTsOptions;
use generic_inference::{
    EmitInferenceContext, find_component_usage_for_event, generate_inferred_emit_args,
};
use handler_context::requires_unresolved_handler_implicit_any;

pub(super) struct ComponentEventTypes {
    pub(super) event_type: String,
    pub(super) handler_type: Option<String>,
    pub(super) handler_type_expr: Option<String>,
    pub(super) listener_type: String,
    pub(super) listener_type_expr: String,
}

pub(super) struct ComponentEventTypeContext<'a> {
    pub(super) summary: &'a Croquis,
    pub(super) virtual_ts_options: &'a VirtualTsOptions,
    pub(super) data: &'a EventHandlerScopeData,
    pub(super) scope: &'a Scope,
    pub(super) syntactic_type_only_imported_names: &'a FxHashSet<CompactString>,
    pub(super) template_prop_names: &'a FxHashSet<String>,
    pub(super) legacy_vue2: bool,
    pub(super) needs_typed_handler_assignment: bool,
    pub(super) indent: &'a str,
}

pub(super) fn generate_component_event_types(
    ts: &mut String,
    ctx: ComponentEventTypeContext<'_>,
) -> Option<ComponentEventTypes> {
    let ComponentEventTypeContext {
        summary,
        virtual_ts_options,
        data,
        scope,
        syntactic_type_only_imported_names,
        template_prop_names,
        legacy_vue2,
        needs_typed_handler_assignment,
        indent,
    } = ctx;
    let component_name = data.target_component.as_ref()?;
    let scope_id = scope.id.as_u32();
    let safe_event_name = to_safe_identifier(data.event_name.as_str());
    let component_ref = component_binding_reference(
        summary,
        virtual_ts_options,
        syntactic_type_only_imported_names,
        component_name.as_str(),
    );
    let component_type_name = to_safe_identifier_fragment(component_name.as_str());
    let pascal_event = to_pascal_case(data.event_name.as_str());
    let on_handler = cstr!("on{pascal_event}");
    let prop_key = if on_handler.contains(':') {
        cstr!("\"{}\"", on_handler.as_str())
    } else {
        on_handler
    };
    let prop_args = cstr!("__{component_type_name}_{scope_id}_{safe_event_name}_prop_args");
    let static_emit_args =
        cstr!("__{component_type_name}_{scope_id}_{safe_event_name}_static_emit_args");
    let emit_args = cstr!("__{component_type_name}_{scope_id}_{safe_event_name}_emit_args");
    let args_type = cstr!("__{component_type_name}_{scope_id}_{safe_event_name}_args");
    let event_type = cstr!("__{component_type_name}_{scope_id}_{safe_event_name}_event");
    let listener_type = cstr!("__{component_type_name}_{scope_id}_{safe_event_name}_listener");

    append!(
        *ts,
        "{indent}type {prop_args} = typeof {component_ref} extends {{ new (): {{ $props: infer __P }} }}\n",
    );
    append!(
        *ts,
        "{indent}  ? __P extends {{ {prop_key}?: (...args: infer __A) => any }} ? __A : unknown[]\n",
    );
    append!(
        *ts,
        "{indent}  : typeof {component_ref} extends (props: infer __P) => any\n",
    );
    append!(
        *ts,
        "{indent}    ? __P extends {{ {prop_key}?: (...args: infer __A) => any }} ? __A : unknown[]\n",
    );
    append!(*ts, "{indent}    : unknown[];\n");

    let inferred_emit_args = generate_inferred_emit_args(
        ts,
        &EmitInferenceContext {
            summary,
            component_name: component_name.as_str(),
            data,
            scope,
            component_ref: &component_ref,
            component_type_name: &component_type_name,
            safe_event_name: &safe_event_name,
            prop_key: &prop_key,
            template_prop_names,
            indent,
        },
    );

    if let Some(ref inferred) = inferred_emit_args {
        append!(
            *ts,
            "{indent}type {static_emit_args} = typeof {component_ref} extends {{ __vizeEmitProps?: infer __EP }}\n",
        );
        append!(
            *ts,
            "{indent}  ? __EP extends {{ {prop_key}?: (...args: infer __A) => any }} ? __A : unknown[]\n",
        );
        append!(*ts, "{indent}  : unknown[];\n");
        append!(
            *ts,
            "{indent}type {emit_args} = unknown[] extends {inferred} ? {static_emit_args} : {inferred};\n",
        );
        append!(
            *ts,
            "{indent}type {args_type} = unknown[] extends {inferred} ? (unknown[] extends {prop_args} ? {emit_args} : {prop_args}) : {inferred};\n",
        );
    } else {
        append!(
            *ts,
            "{indent}type {emit_args} = typeof {component_ref} extends {{ __vizeEmitProps?: infer __EP }}\n",
        );
        append!(
            *ts,
            "{indent}  ? __EP extends {{ {prop_key}?: (...args: infer __A) => any }} ? __A : unknown[]\n",
        );
        append!(
            *ts,
            "{indent}  : unknown[];\n{indent}type {args_type} = unknown[] extends {prop_args} ? {emit_args} : {prop_args};\n",
        );
    }

    // In legacy Vue 2 mode the listener rest args go through the loose emit
    // wrapper so object payload callbacks stay permissive; otherwise they use
    // the resolved emit argument tuple directly.
    let listener_args_type = if legacy_vue2 {
        let legacy_args_type =
            cstr!("__{component_type_name}_{scope_id}_{safe_event_name}_legacy_args");
        append!(
            *ts,
            "{indent}type {event_type} = {args_type} extends [] ? any : unknown[] extends {args_type} ? any : __VizeVue2LooseEventArg<{args_type}[0]>;\n",
        );
        append!(
            *ts,
            "{indent}type {legacy_args_type} = {args_type} extends [] ? any[] : unknown[] extends {args_type} ? any[] : __VizeVue2LooseEmitArgs<{args_type}>;\n",
        );
        legacy_args_type
    } else {
        append!(
            *ts,
            "{indent}type {event_type} = {args_type} extends [] ? any : unknown[] extends {args_type} ? any : {args_type}[0];\n",
        );
        args_type.clone()
    };
    // The modern listener returns `any`, matching the handler props Vue's own
    // `EmitFn`/Volar synthesis produce (`(user: User) => any`); an emit-payload
    // mismatch then elaborates with the same expected type `vue-tsc` prints.
    // `unknown` behaves identically in the expected-return position, so this is
    // display parity only (#3889).
    let listener_type_expr = if legacy_vue2 {
        cstr!("(...args: {listener_args_type}) => unknown")
    } else {
        cstr!(
            "unknown[] extends {args_type} ? ((...args: any[]) => any) : ((...args: {listener_args_type}) => any)"
        )
    };
    let has_script_component_binding = summary.binding_spans.contains_key(component_name.as_str());
    let requires_unresolved_handler =
        requires_unresolved_handler_implicit_any(summary, component_name, data, scope);
    let handler_type_expr = (!legacy_vue2
        && needs_typed_handler_assignment
        && (has_script_component_binding || requires_unresolved_handler))
        .then(|| {
            if has_script_component_binding {
                cstr!("unknown[] extends {args_type} ? ((...args: any[]) => any) : {listener_type}")
            } else {
                cstr!("unknown[] extends {args_type} ? unknown : {listener_type}")
            }
        });
    let handler_type = handler_type_expr
        .as_ref()
        .map(|_| cstr!("__{component_type_name}_{scope_id}_{safe_event_name}_handler"));
    Some(ComponentEventTypes {
        event_type,
        handler_type,
        handler_type_expr,
        listener_type,
        listener_type_expr,
    })
}
