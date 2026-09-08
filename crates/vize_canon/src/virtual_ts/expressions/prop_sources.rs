//! Authored source ranges and generated values for component prop checks.
//!
//! These helpers translate one passed prop into the pieces check emission
//! needs: the escaped or rewritten generated value, the authored
//! attribute-name range that anchors child prop-type errors (matching
//! vue-tsc), and the authored value range that anchors errors inside the
//! bound expression.

use super::component_props::ComponentPropSource;
use super::reserved_props::rewrite_reserved_template_prop;
use oxc_allocator::Allocator;
use oxc_ast::ast::Expression;
use oxc_parser::Parser;
use oxc_span::SourceType;
use std::ops::Range;
use vize_carton::FxHashSet;
use vize_carton::String;
use vize_croquis::croquis::PassedProp;
use vize_croquis::drawer::strip_js_comments;

fn push_ts_string_literal(out: &mut String, value: &str) {
    out.push('"');
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
    out.push('"');
}

pub(crate) fn generated_prop_value(
    prop: &PassedProp,
    template_prop_names: &FxHashSet<String>,
) -> Option<String> {
    generated_prop_value_with_comment_policy(prop, template_prop_names, false)
}

pub(crate) fn generated_prop_value_preserving_comments(
    prop: &PassedProp,
    template_prop_names: &FxHashSet<String>,
) -> Option<String> {
    generated_prop_value_with_comment_policy(prop, template_prop_names, true)
}

fn generated_prop_value_with_comment_policy(
    prop: &PassedProp,
    template_prop_names: &FxHashSet<String>,
    preserve_comments: bool,
) -> Option<String> {
    if !prop.is_dynamic {
        let mut value = String::default();
        if let Some(static_value) = prop.value.as_ref() {
            push_ts_string_literal(&mut value, static_value.as_str());
        } else {
            value.push_str("true");
        }
        return Some(value);
    }

    let raw_value = prop.value.as_ref()?.as_str();
    let value = if preserve_comments {
        std::borrow::Cow::Borrowed(raw_value)
    } else {
        strip_js_comments(raw_value)
    };
    let trimmed_value = value.as_ref().trim();
    let rewritten_value = rewrite_reserved_template_prop(trimmed_value, template_prop_names);
    Some(rewritten_value.as_ref().map_or_else(
        || String::from(value.as_ref()),
        |s| String::from(s.as_str()),
    ))
}

/// Append one generated prop value and return its range without synthetic
/// grouping bytes.
///
/// A top-level sequence needs grouping wherever a prop value sits beside
/// comma-delimited generated syntax (a declaration, object property, array
/// element, or spread). OXC distinguishes that shape from commas nested in a
/// call, array, object, or already-parenthesized sequence. The cheap byte check
/// only skips parsing expressions that cannot possibly be sequences.
pub(crate) fn append_prop_value(ts: &mut String, value: &str) -> Range<usize> {
    let needs_grouping = value.as_bytes().contains(&b',') && is_top_level_sequence(value);
    if needs_grouping {
        ts.push('(');
    }
    let value_start = ts.len();
    ts.push_str(value);
    let value_end = ts.len();
    if needs_grouping {
        ts.push(')');
    }
    value_start..value_end
}

fn is_top_level_sequence(value: &str) -> bool {
    let allocator = Allocator::default();
    matches!(
        Parser::new(&allocator, value, SourceType::ts()).parse_expression(),
        Ok(Expression::SequenceExpression(_))
    )
}

pub(crate) fn prop_value_source_range(
    source_context: ComponentPropSource<'_>,
    prop: &PassedProp,
) -> Option<std::ops::Range<usize>> {
    let source = source_context.template?;
    let value = prop.value.as_ref()?.as_str();
    let prop_start = prop.start as usize;
    let prop_end = prop.end as usize;
    let raw_prop = source.get(prop_start..prop_end)?;
    let relative_start = raw_prop.rfind(value)?;
    let source_start = source_context.offset as usize + prop_start + relative_start;
    Some(source_start..source_start + value.len())
}

/// The authored token a prop-type diagnostic anchors at — `msg` inside
/// `:msg="expr"`, `v-bind:msg="expr"` or `msg="text"`, `title` inside
/// `v-model:title="expr"`, and the directive itself for an argument-less
/// `v-model="expr"`.
///
/// vue-tsc anchors prop-type diagnostics at that token, so the synthetic check
/// identifier maps here for byte-identical positions.
pub(crate) fn prop_name_source_range(
    source_context: ComponentPropSource<'_>,
    prop: &PassedProp,
) -> Option<std::ops::Range<usize>> {
    let source = source_context.template?;
    let prop_start = prop.start as usize;
    let raw_prop = source.get(prop_start..prop.end as usize)?;
    let name_region = raw_prop.split('=').next().unwrap_or(raw_prop);
    let name = prop.name.as_str();
    let (prefix_len, token_len) = anchor_token(name_region, name)?;
    let source_start = source_context.offset as usize + prop_start + prefix_len;
    Some(source_start..source_start + token_len)
}

/// Byte offset of the anchor token inside `name_region` (the attribute text up
/// to `=`) and its length.
///
/// The authored name sits right after the binding prefix; matching there
/// (instead of searching) keeps names like `bind` from anchoring inside
/// `v-bind:` and modifiers like `.sync` from stealing the match.
fn anchor_token(name_region: &str, name: &str) -> Option<(usize, usize)> {
    // An argument-less `v-model` binds `modelValue`, a name that appears
    // nowhere in the source, so there is no argument token to anchor to.
    // vue-tsc anchors at the directive itself; before #3462 the missing match
    // fell all the way back to the bound expression. `v-model:title` keeps
    // anchoring at `title`, which already agreed with vue-tsc byte for byte.
    const V_MODEL: &str = "v-model";
    if name_region == V_MODEL || name_region.starts_with("v-model.") {
        return Some((0, V_MODEL.len()));
    }
    for prefix in ["v-bind:", "v-model:"] {
        if let Some(rest) = name_region.strip_prefix(prefix) {
            return rest.starts_with(name).then_some((prefix.len(), name.len()));
        }
    }
    if let Some(rest) = name_region.strip_prefix(':') {
        return rest.starts_with(name).then_some((1, name.len()));
    }
    if name_region.starts_with(name) {
        return Some((0, name.len()));
    }
    Some((name_region.find(name)?, name.len()))
}

#[cfg(test)]
mod tests {
    use super::{anchor_token, append_prop_value};

    #[test]
    fn groups_only_unparenthesized_top_level_sequences() {
        let cases = [
            ("void 0, callback", "(void 0, callback)", 1..17),
            ("(void 0, callback)", "(void 0, callback)", 0..18),
            ("invoke(value, other)", "invoke(value, other)", 0..20),
            ("[value, other]", "[value, other]", 0..14),
        ];

        for (source, expected, expected_range) in cases {
            let mut generated = vize_carton::String::default();
            let range = append_prop_value(&mut generated, source);
            assert_eq!(generated, expected, "unexpected grouping for `{source}`");
            assert_eq!(range, expected_range, "unexpected range for `{source}`");
            assert_eq!(&generated[range], source);
        }
    }

    #[test]
    fn prop_anchor_tokens_match_the_authored_binding() {
        assert_eq!(anchor_token("msg", "msg"), Some((0, 3)));
        assert_eq!(anchor_token(":msg", "msg"), Some((1, 3)));
        assert_eq!(anchor_token("v-bind:msg", "msg"), Some((7, 3)));
        // An argument-less `v-model` anchors at the directive, with or without
        // modifiers; a named one anchors at its argument.
        assert_eq!(anchor_token("v-model", "modelValue"), Some((0, 7)));
        assert_eq!(anchor_token("v-model.lazy", "modelValue"), Some((0, 7)));
        assert_eq!(anchor_token("v-model:title", "title"), Some((8, 5)));
        assert_eq!(anchor_token("v-model:title.trim", "title"), Some((8, 5)));
        // `bind` must not anchor inside the `v-bind:` prefix.
        assert_eq!(anchor_token("v-bind:bind", "bind"), Some((7, 4)));
        assert_eq!(anchor_token(":bind", "bind"), Some((1, 4)));
    }
}
