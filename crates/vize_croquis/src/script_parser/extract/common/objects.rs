use oxc_ast::ast::{
    ArrayExpressionElement, Expression, ObjectExpression, ObjectPropertyKind, PropertyKey,
};
use oxc_span::GetSpan;
use vize_carton::CompactString;

use super::arguments::expression_string_literal;

pub(in crate::script_parser::extract) fn object_string_property(
    object: &ObjectExpression<'_>,
    name: &str,
) -> Option<CompactString> {
    object_property(object, name)
        .and_then(expression_string_value)
        .map(CompactString::new)
}

pub(in crate::script_parser::extract) fn object_u32_property(
    object: &ObjectExpression<'_>,
    name: &str,
) -> Option<u32> {
    let value = object_property(object, name)?;
    match value {
        Expression::NumericLiteral(literal)
            if literal.value.is_finite()
                && literal.value >= 0.0
                && literal.value.fract() == 0.0 =>
        {
            Some(literal.value as u32)
        }
        _ => None,
    }
}

pub(in crate::script_parser::extract) fn object_bool_property(
    object: &ObjectExpression<'_>,
    name: &str,
) -> Option<bool> {
    let value = object_property(object, name)?;
    match value {
        Expression::BooleanLiteral(literal) => Some(literal.value),
        _ => None,
    }
}

pub(in crate::script_parser::extract) fn object_expression_source_property(
    object: &ObjectExpression<'_>,
    name: &str,
    source: &str,
) -> Option<CompactString> {
    let value = object_property(object, name)?;
    source
        .get(value.span().start as usize..value.span().end as usize)
        .map(CompactString::new)
}

pub(in crate::script_parser::extract) fn fill_define_art_tags(
    object: &ObjectExpression<'_>,
    tags: &mut Vec<CompactString>,
) {
    let Some(value) = object_property(object, "tags") else {
        return;
    };

    match value {
        Expression::ArrayExpression(array) => {
            for element in &array.elements {
                let ArrayExpressionElement::StringLiteral(literal) = element else {
                    continue;
                };
                let tag = literal.value.as_str();
                if !tag.is_empty() {
                    tags.push(CompactString::new(tag));
                }
            }
        }
        _ => {
            if let Some(csv) = expression_string_value(value) {
                for tag in csv.split(',') {
                    let tag = tag.trim();
                    if !tag.is_empty() {
                        tags.push(CompactString::new(tag));
                    }
                }
            }
        }
    }
}

pub(in crate::script_parser::extract) fn object_property<'a>(
    object: &'a ObjectExpression<'a>,
    name: &str,
) -> Option<&'a Expression<'a>> {
    object.properties.iter().find_map(|property| {
        let ObjectPropertyKind::ObjectProperty(property) = property else {
            return None;
        };
        (static_property_name(&property.key) == Some(name)).then_some(&property.value)
    })
}

pub(in crate::script_parser::extract) fn static_property_name<'a>(
    key: &'a PropertyKey<'a>,
) -> Option<&'a str> {
    match key {
        PropertyKey::StaticIdentifier(identifier) => Some(identifier.name.as_str()),
        PropertyKey::StringLiteral(literal) => Some(literal.value.as_str()),
        _ => None,
    }
}

pub(super) fn expression_string_value<'a>(expression: &'a Expression<'a>) -> Option<&'a str> {
    expression_string_literal(expression).map(|literal| literal.value)
}
