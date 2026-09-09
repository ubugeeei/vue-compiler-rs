use oxc_ast::ast::{Argument, CallExpression, Expression};
use oxc_span::GetSpan;

use crate::macros::{ArtDefinition, DEFINE_ART, MacroKind, ModelDefinition};
use vize_carton::CompactString;

use super::super::ScriptParseResult;
use super::common::{
    argument_identifier, argument_object, argument_string_literal, component_name_from_source,
    fill_define_art_tags, object_bool_property, object_expression_source_property, object_property,
    object_string_property, object_u32_property,
};

pub fn process_call_expression(
    result: &mut ScriptParseResult,
    call: &CallExpression<'_>,
    source: &str,
) -> Option<MacroKind> {
    let callee_name = match &call.callee {
        Expression::Identifier(id) => id.name.as_str(),
        _ => return None,
    };

    let macro_kind = MacroKind::from_name(callee_name)?;

    let span = call.span;

    // Extract type arguments if present
    let type_args = call.type_arguments.as_ref().map(|tp| {
        let type_source = &source[tp.span.start as usize..tp.span.end as usize];
        CompactString::new(type_source)
    });

    // Extract runtime arguments
    let runtime_args = if !call.arguments.is_empty() {
        let args_start = call.arguments.first().map(|a| a.span().start);
        let args_end = call.arguments.last().map(|a| a.span().end);
        if let (Some(start), Some(end)) = (args_start, args_end) {
            Some(CompactString::new(&source[start as usize..end as usize]))
        } else {
            None
        }
    } else {
        None
    };

    // Add macro call
    result.macros.add_call(
        callee_name,
        macro_kind,
        span.start,
        span.end,
        runtime_args,
        type_args.clone(),
    );

    // Process macro-specific content
    match macro_kind {
        MacroKind::DefineProps => {
            // Extract props from type or runtime arguments
            if let Some(ref type_params) = call.type_arguments {
                super::props::extract_props_from_type(result, &type_params.params, source);
            } else if let Some(first_arg) = call.arguments.first() {
                super::props::extract_props_from_runtime(result, first_arg, source);
            }
        }

        MacroKind::DefineEmits => {
            // Extract emits from type or runtime arguments
            if let Some(ref type_params) = call.type_arguments {
                super::emits::extract_emits_from_type(result, &type_params.params, source);
            } else if let Some(first_arg) = call.arguments.first() {
                super::emits::extract_emits_from_runtime(result, first_arg, source);
            }
        }

        MacroKind::DefineSlots => {
            if let Some(ref type_params) = call.type_arguments {
                super::slots::extract_slots_from_type(result, &type_params.params, source);
            }
        }

        MacroKind::DefineModel => {
            // Extract model name (first string argument or 'modelValue' by default)
            let model_name = call
                .arguments
                .first()
                .and_then(|arg| {
                    if let Argument::StringLiteral(s) = arg {
                        Some(s.value.as_str())
                    } else {
                        None
                    }
                })
                .unwrap_or("modelValue");
            let declaration = call
                .arguments
                .first()
                .and_then(|arg| match arg {
                    Argument::StringLiteral(literal) => Some(literal.span),
                    _ => None,
                })
                .unwrap_or_else(|| call.callee.span());
            let explicit_model_type = call
                .type_arguments
                .as_ref()
                .and_then(|type_params| type_params.params.first())
                .and_then(|ty| {
                    source
                        .get(ty.span().start as usize..ty.span().end as usize)
                        .map(str::trim)
                        .filter(|text| !text.is_empty())
                        .map(CompactString::new)
                });
            let modifier_type = call
                .type_arguments
                .as_ref()
                .and_then(|type_params| type_params.params.get(1))
                .and_then(|ty| {
                    source
                        .get(ty.span().start as usize..ty.span().end as usize)
                        .map(str::trim)
                        .filter(|text| !text.is_empty())
                        .map(CompactString::new)
                });
            let options_arg = if matches!(call.arguments.first(), Some(Argument::StringLiteral(_)))
            {
                call.arguments.get(1)
            } else {
                call.arguments.first()
            };
            let (required, default_value) = options_arg
                .and_then(argument_object)
                .map(|options| {
                    (
                        object_bool_property(options, "required").unwrap_or(false),
                        object_expression_source_property(options, "default", source),
                    )
                })
                .unwrap_or((false, None));
            let model_type = explicit_model_type
                .or_else(|| options_arg.and_then(runtime_model_type_from_options_arg));

            result.macros.add_model_with_declaration(
                ModelDefinition {
                    name: CompactString::new(model_name),
                    local_name: CompactString::new(model_name),
                    model_type,
                    required,
                    default_value,
                },
                declaration.start,
                declaration.end,
            );
            if let Some(modifier_type) = modifier_type {
                result
                    .macros
                    .set_model_modifier_type(CompactString::new(model_name), modifier_type);
            }
        }

        MacroKind::WithDefaults => {
            // withDefaults wraps defineProps - find the inner call
            if let Some(Argument::CallExpression(inner_call)) = call.arguments.first() {
                process_call_expression(result, inner_call, source);
            }
        }

        MacroKind::DefineOptions
            if call
                .arguments
                .first()
                .and_then(argument_object)
                .and_then(|options| object_bool_property(options, "inheritAttrs"))
                == Some(false) =>
        {
            result.inherit_attrs_disabled = true;
        }

        MacroKind::Custom if callee_name == DEFINE_ART => {
            if let Some(art) = extract_define_art(result, call) {
                result.macros.set_define_art(art);
            }
        }

        _ => {}
    }

    Some(macro_kind)
}

fn runtime_model_type_from_options_arg(arg: &Argument<'_>) -> Option<CompactString> {
    let options = argument_object(arg)?;
    let type_expression = object_property(options, "type")?;
    runtime_model_type_from_expression(type_expression)
}

fn runtime_model_type_from_expression(expression: &Expression<'_>) -> Option<CompactString> {
    match expression {
        Expression::Identifier(identifier) => {
            runtime_constructor_model_type(identifier.name.as_str()).map(CompactString::new)
        }
        Expression::ArrayExpression(array) => {
            let mut types = Vec::new();
            for element in &array.elements {
                let Some(expression) = element.as_expression() else {
                    continue;
                };
                let Some(model_type) = runtime_model_type_from_expression(expression) else {
                    continue;
                };
                if !types.contains(&model_type) {
                    types.push(model_type);
                }
            }
            (!types.is_empty()).then(|| CompactString::new(types.join(" | ")))
        }
        Expression::ParenthesizedExpression(parenthesized) => {
            runtime_model_type_from_expression(&parenthesized.expression)
        }
        Expression::TSAsExpression(ts_as) => runtime_model_type_from_expression(&ts_as.expression),
        Expression::TSSatisfiesExpression(ts_satisfies) => {
            runtime_model_type_from_expression(&ts_satisfies.expression)
        }
        _ => None,
    }
}

fn runtime_constructor_model_type(name: &str) -> Option<&'static str> {
    match name {
        "String" => Some("string"),
        "Number" => Some("number"),
        "Boolean" => Some("boolean"),
        "Array" => Some("unknown[]"),
        "Object" => Some("Record<string, unknown>"),
        "Date" => Some("Date"),
        "Function" => Some("(...args: any[]) => any"),
        _ => None,
    }
}

fn extract_define_art(
    result: &ScriptParseResult,
    call: &CallExpression<'_>,
) -> Option<ArtDefinition> {
    let first_arg = call.arguments.first()?;
    let mut component_source_span = None;
    let mut component_source_value_span = None;
    let (component_name, component_source) =
        if let Some(source) = argument_string_literal(first_arg) {
            component_source_span = Some((source.literal_start, source.literal_end));
            component_source_value_span = Some((source.value_start, source.value_end));
            (
                component_name_from_source(source.value),
                Some(CompactString::new(source.value)),
            )
        } else {
            let component_name = argument_identifier(first_arg)?;
            (
                CompactString::new(component_name),
                result.import_sources.get(component_name).cloned(),
            )
        };
    let mut art = ArtDefinition {
        component_name,
        component_source,
        component_source_span,
        component_source_value_span,
        title: None,
        description: None,
        category: None,
        tags: Vec::new(),
        status: None,
        order: None,
    };

    if let Some(options) = call.arguments.get(1).and_then(argument_object) {
        art.title = object_string_property(options, "title");
        art.description = object_string_property(options, "description");
        art.category = object_string_property(options, "category");
        art.status = object_string_property(options, "status");
        art.order = object_u32_property(options, "order");
        fill_define_art_tags(options, &mut art.tags);
    }

    Some(art)
}
