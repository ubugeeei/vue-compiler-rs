use std::ops::Range;

use vize_carton::{FxHashMap, FxHashSet, String, append};
use vize_croquis::croquis::{PassedProp, SpreadProp};

use crate::virtual_ts::{
    expressions::{
        ComponentPropSource, append_prop_value, generated_prop_value, prop_name_source_range,
        prop_value_source_range,
    },
    helpers::to_camel_case,
    types::{VizeMapping, VizeSubSpan},
};

use super::super::context::ScopeGenContext;
use super::super::vif_guard::append_ignored_vif_guard_open;
use super::SlotOutlet;

struct SlotOutletCheckContext<'a> {
    slot_outlets_by_scope: &'a FxHashMap<u32, Vec<SlotOutlet>>,
    template_prop_names: &'a FxHashSet<String>,
    source_context: ComponentPropSource<'a>,
    slots_type_ref: &'a str,
    indent: &'a str,
}

struct PayloadType {
    text: String,
    name_gen_range: Option<Range<usize>>,
}

pub(super) fn emit_slot_outlet_helpers(
    ts: &mut String,
    slot_outlets_by_scope: &FxHashMap<u32, Vec<SlotOutlet>>,
) {
    let mut needs_static = false;
    let mut needs_dynamic = false;
    let mut needs_spread = false;
    for outlet in slot_outlets_by_scope
        .values()
        .flat_map(|outlets| outlets.iter())
    {
        if !outlet.spread_props.is_empty() {
            needs_spread = true;
        }
        if outlet.name_is_dynamic {
            needs_dynamic = true;
        } else {
            needs_static = true;
        }
    }
    if !needs_static && !needs_dynamic && !needs_spread {
        return;
    }

    // The payload is resolved through indexed access rather than a
    // `__K extends keyof __S` conditional: a slots type that instantiates the
    // SFC's own type parameters (`Slots<T>`, typically widened by a mapped type
    // over `T`) has a generic `keyof`, so such a conditional stays deferred and
    // nothing is assignable to it. `Parameters<…>` of the indexed slot resolves
    // even then, and both guards below keep the permissive fallbacks: a key the
    // slots type does not declare, or a slot that takes no payload, still types
    // the outlet literal as `unknown` instead of `never`/`undefined`.
    ts.push_str("  type __VizeSlotOutletFn = (...args: any[]) => any;\n");
    if needs_static {
        ts.push_str(
            "  type __VizeSlotOutletTarget<__S, __K extends PropertyKey> = Extract<NonNullable<__S[__K & keyof __S]>, __VizeSlotOutletFn>;\n",
        );
        ts.push_str(
            "  type __VizeSlotOutletArgs<__S, __K extends PropertyKey> = [__VizeSlotOutletTarget<__S, __K>] extends [never] ? [] : Parameters<__VizeSlotOutletTarget<__S, __K>>;\n",
        );
        ts.push_str(
            "  type __VizeSlotOutletPayload<__S, __K extends PropertyKey> = [__VizeSlotOutletArgs<__S, __K>] extends [[]] ? unknown : __VizeSlotOutletArgs<__S, __K>[0];\n",
        );
    }
    if needs_dynamic {
        ts.push_str(
            "  type __VizeAnySlotOutletTarget<__S> = Extract<NonNullable<__S[keyof __S]>, __VizeSlotOutletFn>;\n",
        );
        ts.push_str(
            "  type __VizeAnySlotOutletArgs<__S> = [__VizeAnySlotOutletTarget<__S>] extends [never] ? [] : Parameters<__VizeAnySlotOutletTarget<__S>>;\n",
        );
        ts.push_str(
            "  type __VizeAnySlotOutletPayload<__S> = [__VizeAnySlotOutletArgs<__S>] extends [[]] ? unknown : __VizeAnySlotOutletArgs<__S>[0];\n",
        );
    }
    if needs_spread {
        ts.push_str(
            "  type __VizeSlotOutletSpreadPayload<__T> = __T extends object ? __T : Record<string, unknown>;\n",
        );
        ts.push_str(
            "  function __vizeSlotOutletSpread<__T>(value: __T): __VizeSlotOutletSpreadPayload<__T> { return value as any; }\n",
        );
    }
}

pub(in crate::virtual_ts::scope) fn generate_scope_slot_outlet_checks(
    ts: &mut String,
    mappings: &mut Vec<VizeMapping>,
    scope_id: u32,
    ctx: &ScopeGenContext<'_, '_>,
    indent: &str,
) {
    if !ctx.check_options.check_props {
        return;
    }
    generate_slot_outlet_checks(
        ts,
        mappings,
        scope_id,
        SlotOutletCheckContext {
            slot_outlets_by_scope: &ctx.slot_outlets.by_scope,
            template_prop_names: ctx.template_prop_names,
            source_context: ComponentPropSource::new(
                ctx.template_source,
                ctx.template_offset,
                &ctx.summary.scopes,
            ),
            slots_type_ref: ctx.slot_outlets.slots_type.as_str(),
            indent,
        },
    );
}

fn generate_slot_outlet_checks(
    ts: &mut String,
    mappings: &mut Vec<VizeMapping>,
    scope_id: u32,
    ctx: SlotOutletCheckContext<'_>,
) {
    let SlotOutletCheckContext {
        slot_outlets_by_scope,
        template_prop_names,
        source_context,
        slots_type_ref,
        indent,
    } = ctx;
    let Some(outlets) = slot_outlets_by_scope.get(&scope_id) else {
        return;
    };

    for outlet in outlets {
        if let Some(ref guard) = outlet.vif_guard {
            append_ignored_vif_guard_open(ts, indent, guard, "Inference-only guard");
        }
        let expr_indent = if outlet.vif_guard.is_some() {
            let mut nested = String::from(indent);
            nested.push_str("  ");
            nested
        } else {
            String::from(indent)
        };
        append!(*ts, "{expr_indent}((__vize_slot_props: ",);
        let payload_type = outlet_payload_type(outlet, slots_type_ref);
        let payload_type_gen_start = ts.len();
        ts.push_str(payload_type.text.as_str());
        ts.push_str(") => { void __vize_slot_props; })(");
        if let (Some(gen_range), Some(src_range)) = (
            payload_type.name_gen_range,
            outlet.name_source_range.clone(),
        ) {
            mappings.push(VizeMapping {
                gen_range: payload_type_gen_start + gen_range.start
                    ..payload_type_gen_start + gen_range.end,
                src_range: (source_context.offset + src_range.start) as usize
                    ..(source_context.offset + src_range.end) as usize,
                sub_spans: Vec::new(),
            });
        }
        let literal_range = append_slot_outlet_literal(
            ts,
            mappings,
            outlet,
            template_prop_names,
            source_context,
            expr_indent.as_str(),
        );
        ts.push_str(");\n");

        let tag_src_start = (source_context.offset + outlet.start + 1) as usize;
        mappings.push(VizeMapping {
            gen_range: literal_range,
            src_range: tag_src_start..tag_src_start + "slot".len(),
            sub_spans: Vec::new(),
        });
        if outlet.vif_guard.is_some() {
            append!(*ts, "{indent}}}\n");
        }
    }
}

fn outlet_payload_type(outlet: &SlotOutlet, slots_type_ref: &str) -> PayloadType {
    if outlet.name_is_dynamic {
        return PayloadType {
            text: vize_carton::cstr!("__VizeAnySlotOutletPayload<{slots_type_ref}>"),
            name_gen_range: None,
        };
    }

    let mut text = String::from("__VizeSlotOutletPayload<");
    text.push_str(slots_type_ref);
    text.push_str(", ");
    let name_gen_range = append_ts_string_literal(&mut text, outlet.name.as_str());
    text.push('>');
    PayloadType {
        text,
        name_gen_range: Some(name_gen_range),
    }
}

fn append_slot_outlet_literal(
    ts: &mut String,
    mappings: &mut Vec<VizeMapping>,
    outlet: &SlotOutlet,
    template_prop_names: &FxHashSet<String>,
    source_context: ComponentPropSource<'_>,
    expr_indent: &str,
) -> Range<usize> {
    let literal_gen_start = ts.len();
    ts.push_str("{\n");

    for prop in &outlet.props {
        let Some(generated_value) = generated_prop_value(prop, template_prop_names) else {
            continue;
        };
        let prop_src_start = (source_context.offset + prop.start) as usize;
        let prop_src_end = (source_context.offset + prop.end) as usize;
        append!(*ts, "{expr_indent}  ");
        let entry_gen_start = ts.len();
        let camel_prop_name = to_camel_case(prop.name.as_str());
        append!(*ts, "\"{camel_prop_name}\"");
        let key_gen_end = ts.len();
        ts.push_str(": ");
        let value_gen_range = append_prop_value(ts, generated_value.as_str());
        let entry_gen_end = ts.len();
        ts.push_str(",\n");
        mappings.push(VizeMapping {
            gen_range: entry_gen_start..entry_gen_end,
            src_range: prop_src_start..prop_src_end,
            sub_spans: entry_sub_spans(
                source_context,
                prop,
                entry_gen_start..key_gen_end,
                value_gen_range,
            ),
        });
    }

    for spread in &outlet.spread_props {
        append!(*ts, "{expr_indent}  ...__vizeSlotOutletSpread((");
        let gen_range = append_prop_value(ts, spread.expression.as_str());
        ts.push_str(")),\n");
        let source_expression = spread_expression_source_range(source_context, spread);
        mappings.push(VizeMapping {
            gen_range: gen_range.clone(),
            src_range: (source_context.offset + spread.start) as usize
                ..(source_context.offset + spread.end) as usize,
            sub_spans: source_expression.map_or_else(Vec::new, |src_range| {
                vec![VizeSubSpan {
                    gen_range,
                    src_range,
                }]
            }),
        });
    }

    append!(*ts, "{expr_indent}}}");
    literal_gen_start..ts.len()
}

fn entry_sub_spans(
    source_context: ComponentPropSource<'_>,
    prop: &PassedProp,
    key_gen_range: Range<usize>,
    value_gen_range: Range<usize>,
) -> Vec<VizeSubSpan> {
    let mut sub_spans = Vec::new();
    // The key and the value are anchored independently: a value whose authored
    // text cannot be located (a shorthand bind, a rewritten reserved name) must
    // not discard the key span too.
    if let Some(name_src_range) = prop_name_source_range(source_context, prop) {
        sub_spans.push(VizeSubSpan {
            gen_range: key_gen_range,
            src_range: name_src_range,
        });
    }
    if let Some(value_src_range) = prop_value_source_range(source_context, prop) {
        sub_spans.push(VizeSubSpan {
            gen_range: value_gen_range,
            src_range: value_src_range,
        });
    }
    sub_spans
}

fn spread_expression_source_range(
    source_context: ComponentPropSource<'_>,
    spread: &SpreadProp,
) -> Option<Range<usize>> {
    let source = source_context.template?;
    let raw = source.get(spread.start as usize..spread.end as usize)?;
    let relative_start = raw.rfind(spread.expression.as_str())?;
    let source_start = source_context.offset as usize + spread.start as usize + relative_start;
    Some(source_start..source_start + spread.expression.len())
}

fn append_ts_string_literal(out: &mut String, value: &str) -> Range<usize> {
    out.push('"');
    let start = out.len();
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(ch),
        }
    }
    let end = out.len();
    out.push('"');
    start..end
}
