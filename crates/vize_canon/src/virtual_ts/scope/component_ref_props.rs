use vize_carton::String;
use vize_croquis::croquis::{ComponentUsage, PassedProp};

use super::inline_callback_classifier::is_direct_inline_function_prop_value;

pub(crate) fn is_inline_ref_callback_prop(prop: &PassedProp) -> bool {
    !prop.name_is_dynamic
        && prop.name.as_str() == "ref"
        && prop.is_dynamic
        && prop
            .value
            .as_ref()
            .is_some_and(|value| is_direct_inline_function_prop_value(value.as_str()))
}

pub(super) fn append_ref_callback_helper(ts: &mut String, usages: &[(usize, &ComponentUsage)]) {
    if usages
        .iter()
        .any(|(_, usage)| usage.props.iter().any(is_inline_ref_callback_prop))
    {
        ts.push_str(
            "  type __VizeComponentRefCallback = (ref: any, refs: Record<string, any>) => void;\n",
        );
    }
}
