use vize_carton::{FxHashSet, String, append};
use vize_croquis::{Croquis, macros::ModelDefinition};

use super::template_model_modifiers::{model_modifier_prop_name, model_modifier_type};

fn model_prop_type(model: &ModelDefinition) -> &str {
    model.model_type.as_deref().unwrap_or("unknown")
}

fn emit_model_prop_member(ts: &mut String, summary: &Croquis, model: &ModelDefinition) {
    let optional = if model.required { "" } else { "?" };
    let name = model.name.as_str();
    let prop_type = model_prop_type(model);
    append!(*ts, "  \"{name}\"{optional}: {prop_type};\n");
    let modifiers_name = model_modifier_prop_name(name);
    let modifier_type = model_modifier_type(summary, model);
    append!(
        *ts,
        "  \"{modifiers_name}\"?: Partial<Record<{modifier_type}, true>>;\n"
    );
}

pub(super) fn append_model_props_type_literal(
    ts: &mut String,
    summary: &Croquis,
    models: &[ModelDefinition],
) {
    ts.push_str("{\n");
    for model in models {
        emit_model_prop_member(ts, summary, model);
    }
    ts.push('}');
}

pub(super) fn append_macro_props_type_literal(
    ts: &mut String,
    summary: &Croquis,
    models: &[ModelDefinition],
) {
    ts.push_str("{\n");
    let mut emitted_names: FxHashSet<String> = FxHashSet::default();
    for prop in summary.macros.props() {
        let prop_type = prop.prop_type.as_deref().unwrap_or("unknown");
        let optional = if prop.required { "" } else { "?" };
        append!(*ts, "  {}{optional}: {prop_type};\n", prop.name);
        emitted_names.insert(prop.name.as_str().into());
    }
    for model in models {
        if emitted_names.contains(model.name.as_str()) {
            continue;
        }
        emit_model_prop_member(ts, summary, model);
    }
    ts.push('}');
}
