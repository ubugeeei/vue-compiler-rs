//! Component prop value type-check generation.
//!
//! For each prop passed to a child component usage — dynamic bindings and
//! static attribute values alike — emits a typed assertion plus a single call
//! into the child's generic functional prop-checker so TypeScript can
//! validate the values and infer generics across the component boundary.

use super::super::helpers::to_safe_identifier_fragment;
use super::super::scope::append_ignored_vif_guard_open;
use super::super::types::{VizeMapping, VizeSubSpan};
use super::prop_sources::{
    append_prop_value, generated_prop_value, generated_prop_value_preserving_comments,
    prop_name_source_range, prop_value_source_range,
};
use vize_carton::FxHashMap;
use vize_carton::FxHashSet;
use vize_carton::String;
use vize_carton::append;
use vize_carton::cstr;
use vize_carton::profile;
use vize_croquis::{
    ScopeChain,
    croquis::{ComponentUsage, PassedProp},
};

pub(super) fn is_checkable_prop(prop: &PassedProp) -> bool {
    !prop.name_is_dynamic && prop.name.as_str() != "key" && prop.name.as_str() != "ref"
}

#[derive(Clone, Copy)]
pub(crate) struct ComponentPropSource<'a> {
    pub(crate) template: Option<&'a str>,
    pub(crate) offset: u32,
    pub(crate) scopes: &'a ScopeChain,
}

impl<'a> ComponentPropSource<'a> {
    pub(crate) const fn new(
        template: Option<&'a str>,
        offset: u32,
        scopes: &'a ScopeChain,
    ) -> Self {
        Self {
            template,
            offset,
            scopes,
        }
    }
}

pub(crate) struct ComponentPropCheckContext<'a, 'b> {
    pub(crate) ts: &'b mut String,
    pub(crate) mappings: &'b mut Vec<VizeMapping>,
    pub(crate) template_prop_names: &'a FxHashSet<String>,
    pub(crate) source_context: ComponentPropSource<'a>,
    pub(crate) indent: &'b str,
}

impl<'a, 'b> ComponentPropCheckContext<'a, 'b> {
    pub(crate) fn new(
        ts: &'b mut String,
        mappings: &'b mut Vec<VizeMapping>,
        template_prop_names: &'a FxHashSet<String>,
        source_context: ComponentPropSource<'a>,
        indent: &'b str,
    ) -> Self {
        Self {
            ts,
            mappings,
            template_prop_names,
            source_context,
            indent,
        }
    }
}

pub(super) fn collect_generated_class_bindings<'a>(
    usage: &'a ComponentUsage,
    template_prop_names: &FxHashSet<String>,
) -> Vec<(&'a PassedProp, String)> {
    usage
        .props
        .iter()
        .filter(|prop| is_checkable_prop(prop) && prop.name.as_str() == "class")
        .filter_map(|prop| {
            generated_prop_value(prop, template_prop_names).map(|value| (prop, value))
        })
        .collect()
}

pub(super) fn merged_class_binding_value(bindings: &[(&PassedProp, String)]) -> Option<String> {
    match bindings {
        [] => None,
        [(_, value)] => Some(value.clone()),
        _ => {
            let mut value = String::default();
            value.push('[');
            for (index, (_, binding_value)) in bindings.iter().enumerate() {
                if index > 0 {
                    value.push_str(", ");
                }
                append_prop_value(&mut value, binding_value.as_str());
            }
            value.push(']');
            Some(value)
        }
    }
}

/// Generate component prop value checks at the given indentation level.
pub(crate) fn generate_component_prop_checks(
    ctx: &mut ComponentPropCheckContext<'_, '_>,
    usage: &ComponentUsage,
    idx: usize,
    component_ref: &str,
) {
    let ts = &mut *ctx.ts;
    let mappings = &mut *ctx.mappings;
    let template_prop_names = ctx.template_prop_names;
    let source_context = ctx.source_context;
    let indent = ctx.indent;
    let component_type_name = to_safe_identifier_fragment(usage.name.as_str());
    let has_inline_callback = usage
        .props
        .iter()
        .any(crate::virtual_ts::scope::is_inline_callback_prop);
    let grouped_guard = has_inline_callback && usage.vif_guard.is_some();
    if grouped_guard {
        append_ignored_vif_guard_open(
            ts,
            indent,
            usage.vif_guard.as_deref().unwrap(),
            "Inference-only guard",
        );
    }
    let callback_indent = if grouped_guard {
        cstr!("{indent}  ")
    } else {
        String::from(indent)
    };
    let resolved_props = super::callback_prop_resolution::generate_callback_props_resolution(
        ts,
        usage,
        idx,
        component_ref,
        template_prop_names,
        source_context,
        callback_indent.as_str(),
    );
    let mut name_occurrences: FxHashMap<String, u32> = FxHashMap::default();
    for prop in &usage.props {
        if !is_checkable_prop(prop) && !crate::virtual_ts::scope::is_inline_ref_callback_prop(prop)
        {
            continue;
        }
        // Static attribute values are checked exactly like dynamic bindings:
        // a static `msg="text"` must still satisfy the child's prop type
        // (vue-tsc reports TS2322 here; skipping them was a false negative).
        if prop.value.is_some() {
            let prop_src_start = (source_context.offset + prop.start) as usize;
            let prop_src_end = (source_context.offset + prop.end) as usize;
            let value_src_range = prop_value_source_range(source_context, prop);
            let generated_value = profile!(
                "canon.virtual_ts.prop_check.value",
                if crate::virtual_ts::scope::is_inline_ref_callback_prop(prop) {
                    generated_prop_value_preserving_comments(prop, template_prop_names)
                } else {
                    generated_prop_value(prop, template_prop_names)
                }
                .unwrap_or_default()
            );
            append!(
                *ts,
                "{indent}// @vize-map: prop -> {prop_src_start}:{prop_src_end}\n",
            );

            let safe_prop_name = to_safe_identifier_fragment(prop.name.as_str());
            let expr_indent = if usage.vif_guard.is_some() {
                cstr!("{indent}  ")
            } else {
                indent.into()
            };

            if !grouped_guard && let Some(ref guard) = usage.vif_guard {
                append_ignored_vif_guard_open(ts, indent, guard, "Inference-only guard");
            }

            // A repeated attribute name (static class next to :class) still
            // checks every authored value, but each check constant needs a
            // unique name or the virtual TS redeclares it (TS2451).
            let occurrence = name_occurrences
                .entry(String::from(safe_prop_name.as_str()))
                .and_modify(|count| *count += 1)
                .or_insert(1);
            let check_name = if *occurrence == 1 {
                cstr!("__vize_prop_check_{idx}_{safe_prop_name}")
            } else {
                cstr!("__vize_prop_check_{idx}_{safe_prop_name}_{occurrence}")
            };
            let gen_stmt_start = ts.len();
            append!(*ts, "{expr_indent}const ");
            let check_name_start = ts.len();
            ts.push_str(check_name.as_str());
            let check_name_end = ts.len();
            if crate::virtual_ts::scope::is_inline_callback_prop(prop)
                && let Some(resolution) = resolved_props.as_ref()
            {
                let camel_prop_name = super::super::helpers::to_camel_case(prop.name.as_str());
                let resolved_props = resolution.resolved_props.as_str();
                let selected_props = resolution.selected_props.as_str();
                append!(
                    *ts,
                    ": __VizeResolvedProp<typeof {resolved_props}, typeof {selected_props}, '{camel_prop_name}', __{component_type_name}_{idx}_prop_{safe_prop_name}> = ",
                );
            } else {
                append!(
                    *ts,
                    ": __{component_type_name}_{idx}_prop_{safe_prop_name} = ",
                );
            }
            let value_gen_range = append_prop_value(ts, generated_value.as_str());
            ts.push_str(";\n");
            let gen_stmt_end = ts.len();
            append!(*ts, "{expr_indent}void {check_name};\n");

            // The synthetic identifier receives the child prop-type error
            // (TS2322-class), which vue-tsc anchors at the attribute name;
            // the initializer keeps the exact authored expression so errors
            // inside the value land on the authored bytes.
            let name_src_range = prop_name_source_range(source_context, prop);
            let mut sub_spans = Vec::new();
            if let Some(src_range) = name_src_range.or_else(|| value_src_range.clone()) {
                sub_spans.push(VizeSubSpan {
                    gen_range: check_name_start..check_name_end,
                    src_range,
                });
            }
            if let Some(src_range) = value_src_range {
                sub_spans.push(VizeSubSpan {
                    gen_range: value_gen_range,
                    src_range,
                });
            }
            mappings.push(VizeMapping {
                gen_range: gen_stmt_start..gen_stmt_end,
                src_range: prop_src_start..prop_src_end,
                sub_spans,
            });

            if !grouped_guard && usage.vif_guard.is_some() {
                append!(*ts, "{indent}}}\n");
            }
        }
    }
    if grouped_guard {
        append!(*ts, "{indent}}}\n");
    }

    super::generic_props_call::generate_generic_props_call(
        ts,
        mappings,
        usage,
        idx,
        template_prop_names,
        source_context,
        indent,
    );
}
