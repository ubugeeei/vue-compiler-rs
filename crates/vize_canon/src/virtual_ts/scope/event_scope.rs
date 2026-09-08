//! The `EventHandler` scope closure: one typed wrapper per `@event` binding.
//!
//! A component `@event` gets the child's full emit-argument tuple as the
//! closure's rest parameter, so multi-argument emits keep every parameter
//! (#1512); a native DOM event gets the single `$event` its type implies.
//!
//! Every wrapper is referenced with `void`, never invoked: TypeScript inlines
//! an immediately-invoked function expression's body into the enclosing
//! control-flow graph, so an inline handler assignment (`@click="x = 'a'"`)
//! would narrow `x` for every sibling binding checked after it — the false
//! TS2367 of #4962. A handler is a runtime callback; a plain function
//! expression keeps its body fully checked while its assignments stay out of
//! the render scope's flow, matching `vue-tsc`. Narrowing INTO a handler does
//! not rely on IIFE inlining either: each handler expression re-checks its own
//! `v-if` guard inside the closure (see `event_handler.rs`).

use vize_carton::{String, append, profile};
use vize_croquis::{EventHandlerScopeData, Scope};

use crate::virtual_ts::helpers::get_dom_event_type;
use crate::virtual_ts::types::VizeMapping;

mod event_targets;

use super::component_events::{ComponentEventTypeContext, generate_component_event_types};
use super::context::{EventHandlerExprContext, ScopeGenContext};
use super::event_handler::{event_name_source_range, generate_event_handler_expressions};
use event_targets::{
    dynamic_component_custom_event, needs_typed_handler_assignment, transition_hook_signature,
    vnode_hook_signature,
};

pub(super) fn generate_event_handler_scope(
    ts: &mut String,
    mappings: &mut Vec<VizeMapping>,
    ctx: &ScopeGenContext<'_, '_>,
    scope: &Scope,
    data: &EventHandlerScopeData,
    indent: &str,
    inner_indent: &str,
) {
    let scope_id = scope.id.as_u32();
    append!(*ts, "\n{indent}// @{} handler\n", data.event_name);

    if !ctx.check_options.check_emits {
        append!(*ts, "{indent}void (($event: any) => {{\n");
        profile!(
            "canon.virtual_ts.event_handler_expressions",
            generate_event_handler_expressions(
                ts,
                mappings,
                scope_id,
                &EventHandlerExprContext {
                    expressions_by_scope: ctx.expressions_by_scope,
                    data,
                    check_emits: false,
                    event_type: "any",
                    event_handler_type: None,
                    event_listener_type: None,
                    event_name_src_range: None,
                    template_prop_names: ctx.template_prop_names,
                    template_offset: ctx.template_offset,
                    indent: inner_indent,
                },
            )
        );
        append!(*ts, "{indent}}});\n");
        return;
    }

    if let Some((event_type, listener_args)) = vnode_hook_signature(data.event_name.as_str()) {
        let listener_type = vize_carton::cstr!("__vize_vnode_hook_{}_listener", scope_id);
        let handler_type = vize_carton::cstr!("__vize_vnode_hook_{}_handler", scope_id);
        let needs_typed_handler_assignment = needs_typed_handler_assignment(data);
        append!(
            *ts,
            "{indent}type {listener_type} = (...args: {listener_args}) => any;\n",
        );
        if needs_typed_handler_assignment {
            append!(
                *ts,
                "{indent}type {handler_type} = {{ bivarianceHack(...args: {listener_args}): any }}[\"bivarianceHack\"];\n",
            );
        }
        append!(
            *ts,
            "{indent}void ((...__vize_args: Parameters<{listener_type}>) => {{\n",
        );
        append!(
            *ts,
            "{inner_indent}const $event = __vize_args[0] as {event_type}; void $event;\n",
        );

        profile!(
            "canon.virtual_ts.event_handler_expressions",
            generate_event_handler_expressions(
                ts,
                mappings,
                scope_id,
                &EventHandlerExprContext {
                    expressions_by_scope: ctx.expressions_by_scope,
                    data,
                    check_emits: true,
                    event_type,
                    event_handler_type: needs_typed_handler_assignment
                        .then_some(handler_type.as_str()),
                    event_listener_type: Some(listener_type.as_str()),
                    event_name_src_range: event_name_source_range(
                        ctx.template_source,
                        ctx.template_offset,
                        scope.span.start..scope.span.end,
                        data.event_name.as_str(),
                    ),
                    template_prop_names: ctx.template_prop_names,
                    template_offset: ctx.template_offset,
                    indent: inner_indent,
                },
            )
        );

        append!(*ts, "{indent}}});\n");
    } else if data.target_component.is_some() {
        let needs_typed_handler_assignment = needs_typed_handler_assignment(data);
        let event_types = generate_component_event_types(
            ts,
            ComponentEventTypeContext {
                summary: ctx.summary,
                virtual_ts_options: ctx.virtual_ts_options,
                data,
                scope,
                syntactic_type_only_imported_names: ctx.syntactic_type_only_imported_names,
                template_prop_names: ctx.template_prop_names,
                legacy_vue2: ctx.legacy_vue2,
                needs_typed_handler_assignment,
                indent,
            },
        )
        .expect("component event handler should have a target component");
        let event_type = event_types.event_type;
        let handler_type = event_types.handler_type;
        let handler_type_expr = event_types.handler_type_expr;
        let listener_type = event_types.listener_type;
        let listener_type_expr = event_types.listener_type_expr;
        // Type the listener against the FULL emit tuple so multi-arg emits
        // keep every parameter (#1512); unresolved sigs stay variadic.
        append!(
            *ts,
            "{indent}type {listener_type} = {listener_type_expr};\n",
        );
        if let (Some(handler_type), Some(handler_type_expr)) = (&handler_type, &handler_type_expr) {
            append!(*ts, "{indent}type {handler_type} = {handler_type_expr};\n",);
        }
        // Keep the handler body in a function scope so assignments do not
        // narrow the surrounding render scope; `$event` is element 0.
        append!(
            *ts,
            "{indent}void ((...__vize_args: Parameters<{listener_type}>) => {{\n",
        );
        append!(
            *ts,
            "{inner_indent}const $event = __vize_args[0] as {event_type}; void $event;\n",
        );

        profile!(
            "canon.virtual_ts.event_handler_expressions",
            generate_event_handler_expressions(
                ts,
                mappings,
                scope_id,
                &EventHandlerExprContext {
                    expressions_by_scope: ctx.expressions_by_scope,
                    data,
                    check_emits: true,
                    event_type: event_type.as_str(),
                    event_handler_type: handler_type.as_deref(),
                    event_listener_type: Some(listener_type.as_str()),
                    event_name_src_range: event_name_source_range(
                        ctx.template_source,
                        ctx.template_offset,
                        scope.span.start..scope.span.end,
                        data.event_name.as_str(),
                    ),
                    template_prop_names: ctx.template_prop_names,
                    template_offset: ctx.template_offset,
                    indent: inner_indent,
                },
            )
        );

        append!(*ts, "{indent}}});\n");
    } else if let Some((event_type, listener_args)) = transition_hook_signature(
        ctx.template_source,
        ctx.template_ast,
        scope.span.start,
        data.event_name.as_str(),
    ) {
        let listener_type = vize_carton::cstr!("__vize_transition_{}_listener", scope_id);
        let handler_type = vize_carton::cstr!("__vize_transition_{}_handler", scope_id);
        let needs_typed_handler_assignment = needs_typed_handler_assignment(data);
        append!(
            *ts,
            "{indent}type {listener_type} = (...args: {listener_args}) => any;\n",
        );
        if needs_typed_handler_assignment {
            append!(
                *ts,
                "{indent}type {handler_type} = {{ bivarianceHack(...args: {listener_args}): any }}[\"bivarianceHack\"];\n",
            );
        }
        append!(
            *ts,
            "{indent}void ((...__vize_args: Parameters<{listener_type}>) => {{\n",
        );
        append!(
            *ts,
            "{inner_indent}const $event = __vize_args[0] as {event_type}; void $event;\n",
        );

        profile!(
            "canon.virtual_ts.event_handler_expressions",
            generate_event_handler_expressions(
                ts,
                mappings,
                scope_id,
                &EventHandlerExprContext {
                    expressions_by_scope: ctx.expressions_by_scope,
                    data,
                    check_emits: true,
                    event_type,
                    event_handler_type: needs_typed_handler_assignment
                        .then_some(handler_type.as_str()),
                    event_listener_type: Some(listener_type.as_str()),
                    event_name_src_range: event_name_source_range(
                        ctx.template_source,
                        ctx.template_offset,
                        scope.span.start..scope.span.end,
                        data.event_name.as_str(),
                    ),
                    template_prop_names: ctx.template_prop_names,
                    template_offset: ctx.template_offset,
                    indent: inner_indent,
                },
            )
        );

        append!(*ts, "{indent}}});\n");
    } else if dynamic_component_custom_event(
        ctx.template_source,
        ctx.template_ast,
        scope.span.start,
        data.event_name.as_str(),
    ) {
        let listener_type = vize_carton::cstr!("__vize_dynamic_component_{}_listener", scope_id);
        append!(
            *ts,
            "{indent}type {listener_type} = (...args: any[]) => any;\n",
        );
        append!(
            *ts,
            "{indent}void ((...__vize_args: Parameters<{listener_type}>) => {{\n",
        );
        append!(
            *ts,
            "{inner_indent}const $event = __vize_args[0] as any; void $event;\n",
        );

        profile!(
            "canon.virtual_ts.event_handler_expressions",
            generate_event_handler_expressions(
                ts,
                mappings,
                scope_id,
                &EventHandlerExprContext {
                    expressions_by_scope: ctx.expressions_by_scope,
                    data,
                    check_emits: true,
                    event_type: "any",
                    event_handler_type: None,
                    event_listener_type: Some(listener_type.as_str()),
                    event_name_src_range: event_name_source_range(
                        ctx.template_source,
                        ctx.template_offset,
                        scope.span.start..scope.span.end,
                        data.event_name.as_str(),
                    ),
                    template_prop_names: ctx.template_prop_names,
                    template_offset: ctx.template_offset,
                    indent: inner_indent,
                },
            )
        );

        append!(*ts, "{indent}}});\n");
    } else {
        let event_type = get_dom_event_type(data.event_name.as_str());
        append!(*ts, "{indent}void (($event: {event_type}) => {{\n");

        profile!(
            "canon.virtual_ts.event_handler_expressions",
            generate_event_handler_expressions(
                ts,
                mappings,
                scope_id,
                &EventHandlerExprContext {
                    expressions_by_scope: ctx.expressions_by_scope,
                    data,
                    check_emits: true,
                    event_type,
                    event_handler_type: None,
                    // Native DOM listeners keep the plain-closure shape,
                    // so there is no declared name to anchor at.
                    event_listener_type: None,
                    event_name_src_range: None,
                    template_prop_names: ctx.template_prop_names,
                    template_offset: ctx.template_offset,
                    indent: inner_indent,
                },
            )
        );

        append!(*ts, "{indent}}});\n");
    }
}
