mod keyed_template_names;
mod mappings;
mod model_props;
mod options_api;
mod setup_scoped;
mod template_bindings;
mod template_model;
mod template_model_modifiers;
mod template_names;
mod with_defaults;
pub(crate) use mappings::{PropBindingMappings, PropsSource, prop_source};
use model_props::{append_macro_props_type_literal, append_model_props_type_literal};
pub(crate) use options_api::OptionsApiPropsSource;
pub(crate) use options_api::append_default_props;
use options_api::emit_options_api_props_type;
use setup_scoped::unused_generic_comment;
pub(crate) use setup_scoped::{PropsTypeEmission, generate_setup_scoped_props_artifact};
pub(crate) use template_model::{TemplatePropsModel, generate_props_variables};
pub(crate) use template_names::collect_template_prop_names;
use vize_carton::{CompactString, FxHashSet, String, append, cstr};
use vize_croquis::Croquis;

fn inner_macro_type(type_args: &str) -> &str {
    type_args
        .strip_prefix('<')
        .and_then(|s| s.strip_suffix('>'))
        .unwrap_or(type_args)
}

pub(crate) fn generate_props_type(
    ts: &mut String,
    summary: &Croquis,
    generic_param: Option<&str>,
    options_api_props: Option<&OptionsApiPropsSource>,
    type_only_imported_names: &FxHashSet<CompactString>,
    emission: PropsTypeEmission,
) {
    let props = summary.macros.props();
    let has_props = !props.is_empty();
    let models = summary.macros.models();
    let has_models = !models.is_empty();
    let define_props_type_args = summary
        .macros
        .define_props()
        .and_then(|m| m.type_args.as_ref());
    let define_props_inner_type =
        define_props_type_args.map(|type_args| inner_macro_type(type_args.trim()));
    let define_props_is_authored = summary.macros.define_props().is_some();
    let props_imported_type_only = type_only_imported_names.contains("Props");
    let imported_props_would_collide = props_imported_type_only
        && (define_props_is_authored || has_models || options_api_props.is_some());
    let props_already_defined = summary
        .type_exports
        .iter()
        .any(|te| te.name.as_str() == "Props")
        || imported_props_would_collide;

    let generic_decl = generic_param
        .map(|g| {
            let with_defaults = strip_const_modifiers(&add_generic_defaults(g));
            cstr!("<{with_defaults}>")
        })
        .unwrap_or_default();

    ts.push_str("// ========== Exported Types ==========\n");

    if emission == PropsTypeEmission::DeferredToSetup && define_props_type_args.is_some() {
    } else if imported_props_would_collide {
        if let Some(inner_type) = define_props_inner_type {
            ts.push_str("type __VizeResolvedProps = ");
            ts.push_str(inner_type);
            if has_models {
                ts.push_str(" & ");
                append_model_props_type_literal(ts, summary, models);
            }
            ts.push_str(";\n");
        } else if has_props || has_models {
            ts.push_str("type __VizeResolvedProps = ");
            append_macro_props_type_literal(ts, summary, models);
            ts.push_str(";\n");
        } else if let Some(options_api_props) = options_api_props {
            emit_options_api_props_type(
                ts,
                &generic_decl,
                options_api_props,
                "__VizeResolvedProps",
                false,
            );
        }
    } else if props_already_defined {
        if has_models {
            ts.push_str("type __VizeResolvedProps = Props & ");
            append_model_props_type_literal(ts, summary, models);
            ts.push_str(";\n");
        }
    } else if let Some(inner_type) = define_props_inner_type {
        ts.push_str(unused_generic_comment(generic_param, inner_type));
        if has_models {
            append!(*ts, "export type Props{generic_decl} = {inner_type} & ");
            append_model_props_type_literal(ts, summary, models);
            ts.push_str(";\n");
        } else {
            append!(*ts, "export type Props{generic_decl} = {inner_type};\n");
        }
    } else if has_props || has_models {
        append!(*ts, "export type Props{generic_decl} = ");
        append_macro_props_type_literal(ts, summary, models);
        ts.push_str(";\n");
    } else if let Some(options_api_props) = options_api_props {
        emit_options_api_props_type(ts, &generic_decl, options_api_props, "Props", true);
    } else {
        append!(*ts, "export type Props{generic_decl} = {{}};\n");
    }

    ts.push('\n');
}

/// Lookup key for a `defineProps<...>` type argument when resolving its fields
/// through the croquis `TypeResolver`.
///
/// Inline object types (`{ msg: string }`) are passed through verbatim — the
/// resolver parses them directly. A type *reference* may carry a generic
/// instantiation (`Foo<T>`); the resolver registers local types under their
/// bare declaration name, so strip the arguments to recover `Foo`.
pub(crate) fn type_reference_lookup_key(type_name: &str) -> &str {
    if type_name.trim_start().starts_with('{') {
        type_name
    } else {
        strip_generic_params(type_name).trim()
    }
}

/// Strip the outermost `<...>` pair from a type_args string, handling nested generics.
/// e.g., `"<Props>"` → `"Props"`, `"<Foo<T>>"` → `"Foo<T>"`, `"Props"` → `"Props"`
pub(crate) fn strip_outer_angle_brackets(s: &str) -> &str {
    let s = s.trim();
    if !s.starts_with('<') {
        return s;
    }
    // Find the matching '>' for the opening '<'
    let mut depth = 0i32;
    for (i, c) in s.char_indices() {
        match c {
            '<' => depth += 1,
            '>' => {
                depth -= 1;
                if depth == 0 && i == s.len() - 1 {
                    // The opening '<' matches the final '>' — strip them
                    return &s[1..i];
                }
            }
            _ => {}
        }
    }
    s
}

/// Strip generic parameters from a type name for interface lookup.
/// e.g., `"ContextMenuContentProps<T>"` → `"ContextMenuContentProps"`
pub(super) fn strip_generic_params(type_name: &str) -> &str {
    match type_name.find('<') {
        Some(pos) => &type_name[..pos],
        None => type_name,
    }
}

/// First identifier of a generic parameter declaration, skipping the TS 5.0
/// `const` modifier: `const T extends Tab` declares `T`, not `const`.
fn generic_param_name(param: &str) -> &str {
    let mut tokens = param.split_whitespace();
    match tokens.next() {
        Some("const") => tokens.next().unwrap_or(param),
        Some(token) => token,
        None => param,
    }
}

/// Extract just the generic parameter names from a full generic declaration.
/// e.g., `"T extends Foo, P extends Bar"` → `"T, P"`
/// e.g., `"T"` → `"T"`
/// e.g., `"T extends Record<string, any>, U"` → `"T, U"`
/// e.g., `"const T extends Tab"` → `"T"`
pub(crate) fn extract_generic_names(generic_param: &str) -> String {
    let mut names = String::default();
    let mut depth = 0i32; // track <> nesting
    let mut current_name = String::default();
    let mut in_extends = false;

    for ch in generic_param.chars() {
        match ch {
            '<' => depth += 1,
            '>' => depth -= 1,
            ',' if depth == 0 => {
                let trimmed = current_name.trim();
                if !trimmed.is_empty() {
                    // Extract just the name (before "extends")
                    let name = generic_param_name(trimmed);
                    if !names.is_empty() {
                        names.push_str(", ");
                    }
                    names.push_str(name);
                }
                current_name = String::default();
                in_extends = false;
                continue;
            }
            _ => {}
        }
        if depth == 0 {
            current_name.push(ch);
        }
    }

    // Handle the last parameter
    let trimmed = current_name.trim();
    if !trimmed.is_empty() {
        let name = generic_param_name(trimmed);
        if !names.is_empty() {
            names.push_str(", ");
        }
        names.push_str(name);
    }

    let _ = in_extends;
    names
}

/// Drop TS 5.0 `const` modifiers from a generic parameter list.
/// The modifier is only legal on function/method/class type parameters
/// (TS1277), so callers that splice parameters into `type`/`interface`
/// declarations must strip it first.
/// e.g., `"const T extends Tab = any"` → `"T extends Tab = any"`
pub(crate) fn strip_const_modifiers(generic_param: &str) -> String {
    let mut result = String::default();
    let mut depth = 0i32;
    let mut current_param = String::default();

    for ch in generic_param.chars() {
        match ch {
            '<' => depth += 1,
            '>' => depth -= 1,
            ',' if depth == 0 => {
                append_param_without_const(&mut result, current_param.trim());
                result.push_str(", ");
                current_param = String::default();
                continue;
            }
            _ => {}
        }
        current_param.push(ch);
    }

    let trimmed = current_param.trim();
    if !trimmed.is_empty() {
        append_param_without_const(&mut result, trimmed);
    }

    result
}

/// Append a single generic parameter with its leading `const` modifier removed.
fn append_param_without_const(result: &mut String, param: &str) {
    let stripped = param
        .strip_prefix("const")
        .filter(|rest| rest.starts_with(|ch: char| ch.is_ascii_whitespace()))
        .map(str::trim_start)
        .unwrap_or(param);
    result.push_str(stripped);
}

/// Add `= any` defaults to each generic parameter that doesn't already have a default.
/// e.g., `"T extends Foo, P"` → `"T extends Foo = any, P = any"`
/// e.g., `"T = string"` → `"T = string"` (unchanged, already has default)
pub(crate) fn add_generic_defaults(generic_param: &str) -> String {
    let mut result = String::default();
    let mut depth = 0i32;
    let mut current_param = String::default();

    for ch in generic_param.chars() {
        match ch {
            '<' => {
                depth += 1;
                current_param.push(ch);
            }
            '>' => {
                depth -= 1;
                current_param.push(ch);
            }
            ',' if depth == 0 => {
                append_param_with_default(&mut result, current_param.trim());
                result.push_str(", ");
                current_param = String::default();
            }
            _ => {
                current_param.push(ch);
            }
        }
    }

    // Handle the last parameter
    let trimmed = current_param.trim();
    if !trimmed.is_empty() {
        append_param_with_default(&mut result, trimmed);
    }

    result
}

/// Append a single generic parameter with `= any` default if it doesn't have one.
fn append_param_with_default(result: &mut String, param: &str) {
    result.push_str(param);
    // Check if this param already has a default (contains `=` at depth 0)
    let mut depth = 0i32;
    let has_default = param.chars().any(|ch| {
        match ch {
            '<' => depth += 1,
            '>' => depth -= 1,
            '=' if depth == 0 => return true,
            _ => {}
        }
        false
    });
    if !has_default {
        result.push_str(" = any");
    }
}

#[cfg(test)]
mod tests {
    use super::with_defaults::template_props_type_ref;
    use super::{
        add_generic_defaults, extract_generic_names, strip_const_modifiers,
        type_reference_lookup_key,
    };
    use vize_carton::{FxHashSet, String};

    #[test]
    fn extracts_generic_names_skipping_const_modifier() {
        assert_eq!(
            extract_generic_names("T extends Foo, P extends Bar"),
            "T, P"
        );
        assert_eq!(extract_generic_names("const T extends Tab"), "T");
        assert_eq!(
            extract_generic_names("const T extends Record<string, any>, U"),
            "T, U"
        );
        // A prop named `constant` must not lose its prefix.
        assert_eq!(extract_generic_names("constant extends Foo"), "constant");
    }

    #[test]
    fn strips_const_modifiers_for_type_declarations() {
        assert_eq!(
            strip_const_modifiers("const T extends Tab = any").as_str(),
            "T extends Tab = any"
        );
        assert_eq!(
            strip_const_modifiers("const T extends Record<string, any>, const U = any").as_str(),
            "T extends Record<string, any>, U = any"
        );
        assert_eq!(
            strip_const_modifiers("T extends Tab = any").as_str(),
            "T extends Tab = any"
        );
        assert_eq!(
            strip_const_modifiers("constant extends Foo").as_str(),
            "constant extends Foo"
        );
        assert_eq!(
            strip_const_modifiers(add_generic_defaults("const T extends Tab").as_str()).as_str(),
            "T extends Tab = any"
        );
    }

    #[test]
    fn builds_deterministic_with_defaults_props_type() {
        let mut names: FxHashSet<String> = FxHashSet::default();
        names.insert("label".into());
        names.insert("thickness".into());

        assert_eq!(
            template_props_type_ref("Props", &names),
            r#"__WithDefaultsResult<Props, Pick<Props, "label" | "thickness">>"#
        );
    }

    #[test]
    fn type_reference_lookup_key_strips_generics_but_preserves_inline_literals() {
        // Type references drop their generic instantiation so the resolver can
        // find the local declaration registered under its bare name.
        assert_eq!(type_reference_lookup_key("Props"), "Props");
        assert_eq!(type_reference_lookup_key("Foo<T>"), "Foo");
        assert_eq!(
            type_reference_lookup_key("ContextMenuContentProps<T, U>"),
            "ContextMenuContentProps"
        );
        // Inline object literals are passed through verbatim — the `<` inside a
        // property type must not be mistaken for a generic argument list.
        assert_eq!(
            type_reference_lookup_key("{ items: Array<{ id: string }> }"),
            "{ items: Array<{ id: string }> }"
        );
        assert_eq!(
            type_reference_lookup_key("  { msg: string }"),
            "  { msg: string }"
        );
    }
}
