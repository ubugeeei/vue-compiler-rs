//! Prop-name scope for the template walk of `vue/no-mutating-props`.

use vize_relief::{DirectiveNode, ExpressionNode};
use vize_s0::{FxHashSet, String};

use super::mutation_targets::{MutationTargetKind, prop_mutation_target_kind};

/// What the walk needs to decide whether a template mutation targets a prop.
pub(super) struct PropScope<'names> {
    pub(super) prop_names: &'names FxHashSet<&'names str>,
    pub(super) has_props_object_binding: bool,
    /// Names bound by an enclosing `v-for` alias or slot variable, which
    /// shadow a prop of the same name for the duration of that subtree.
    ///
    /// Collected as bare identifier tokens out of the alias / slot expression
    /// rather than by parsing the binding pattern, so a destructuring pattern
    /// contributes every name it mentions — including a renamed source key
    /// (`v-slot="{ msg: label }"` shadows both `msg` and `label`). That
    /// over-collects, which only ever *suppresses* a report, the safe
    /// direction for evidence that creates findings.
    pub(super) shadowed: Vec<String>,
}

impl PropScope<'_> {
    pub(super) fn mutation_kind(&self, target: &str) -> Option<MutationTargetKind> {
        let target = target.trim();
        if self.shadowed.iter().any(|name| {
            let name = name.as_str();
            target == name || target.strip_prefix(name).is_some_and(starts_member)
        }) {
            return None;
        }
        prop_mutation_target_kind(target, self.prop_names, self.has_props_object_binding)
    }
}

fn starts_member(rest: &str) -> bool {
    rest.starts_with('.') || rest.starts_with('[') || rest.starts_with("?.")
}

/// Push the value / key / index aliases of a `v-for` directive.
///
/// The parsed aliases are preferred; a `v-for` the parser could not split
/// falls back to the whole expression, whose identifier tokens include the
/// source as well as the aliases. Over-collecting only suppresses reports.
pub(super) fn push_for_aliases(directive: &DirectiveNode<'_>, out: &mut Vec<String>, source: &str) {
    if let Some(parsed) = directive.for_parse_result.as_ref() {
        for alias in [
            parsed.value.as_ref(),
            parsed.key.as_ref(),
            parsed.index.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            push_identifier_tokens(expression_source(alias, source), out);
        }
        return;
    }
    if let Some(exp) = directive.exp.as_ref() {
        push_identifier_tokens(expression_source(exp, source), out);
    }
}

pub(super) fn expression_source<'a>(exp: &'a ExpressionNode<'a>, source: &'a str) -> &'a str {
    match exp {
        ExpressionNode::Simple(simple) => simple.content,
        ExpressionNode::Compound(compound) => compound.loc.span.slice(source),
    }
}

/// Push every identifier-shaped token of `source` onto `out`.
///
/// Used for `v-for` aliases and slot variables, where the binding may be a
/// destructuring pattern. See [`PropScope::shadowed`] for why over-collecting
/// is the correct direction here.
pub(super) fn push_identifier_tokens(source: &str, out: &mut Vec<String>) {
    let bytes = source.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if !is_identifier_start(bytes[index]) {
            index += 1;
            continue;
        }
        let start = index;
        while index < bytes.len() && is_identifier_byte(bytes[index]) {
            index += 1;
        }
        out.push(String::new(&source[start..index]));
    }
}

#[inline]
fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_' || byte == b'$'
}

#[inline]
fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$'
}
