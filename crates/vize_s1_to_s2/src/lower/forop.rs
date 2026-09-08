//! Building `ui.for`: the split's sub-slices admitted position by
//! position, the whole value escape-classified when it cannot split, and
//! the hygiene scope minted for every iteration region.

use super::features::OpFamily;
use alloc::vec::Vec as StdVec;

use vize_s0::{Box, String, Vec, cstr};
use vize_s1::Element;

use vize_s2::expr::{ExprRef, OpaqueReason};
use vize_s2::op::{ForBinding, ForOp, Namespace, Op, Region};
use vize_s2::scope::{ScopeBinding, ScopeFacts, ScopeOrigin, ScopeTag};

use super::cx::{Cx, attr_slice, attr_span, element_span};
use super::element::{Analyzed, attr_value_text, element_core};
use super::expr::{desc, expr_at, opaque_at, simple_identifier, trimmed};
use super::structural::{
    ForWrapper, capture_wrapper_attrs, capture_wrapper_key, lower_children,
    record_template_drops_except,
};
use super::vfor::{split_aliases, split_for};

/// One `ui.for`'s consumed scope view, positions in grammar order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForParts {
    /// The introduction-site tag the lowering minted.
    pub tag: ScopeTag,
    /// The value position.
    pub value: ForName,
    /// The second position (object key).
    pub key: ForName,
    /// The third position (index).
    pub index: ForName,
}

/// One binding position's consumed name status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForName {
    /// The position's one enumerated simple-identifier name.
    Named(String),
    /// The position is authored but enumerates no name yet: a
    /// destructuring pattern, a foreign dialect, or the classified escape.
    Pending,
    /// The position was not authored.
    Absent,
}

impl ForName {
    /// The provenance spelling: the name, `?` for pending, `-` for absent.
    pub(crate) fn spell(&self) -> &str {
        match self {
            ForName::Named(name) => name.as_str(),
            ForName::Pending => "?",
            ForName::Absent => "-",
        }
    }
}

/// Build the `ui.for` for an element's `v-for` — or, when the directive
/// carries no expression at all, the kept fragment without it.
pub(crate) fn lower_for<'a>(
    cx: &mut Cx<'a>,
    element: &Element<'a>,
    analyzed: &Analyzed<'a>,
    ns: Namespace,
) -> Op<'a> {
    let attr_idx = analyzed.vfor.expect("caller checked v-for presence");
    let attr = &element.open.attrs[attr_idx];
    let raw = attr_value_text(element, attr_idx).filter(|raw| !raw.trim().is_empty());
    let Some(raw) = raw else {
        cx.error(
            attr_span(cx, attr),
            String::from("v-for is missing expression."),
        );
        cx.record(
            "error.v-for-no-expression",
            None,
            attr_slice(cx, attr),
            String::default(),
            attr_span(cx, attr),
        );
        return element_core(cx, element, analyzed, ns);
    };
    let (text, text_span) = trimmed(cx, raw);

    // The split runs over the **untrimmed** value, exactly as the shipped
    // splitter does: `v-for=" in xs"` has a viable separator (its alias
    // is empty) only because the leading whitespace counts.
    let split = split_for(raw)
        .and_then(|split| split_aliases(&raw[..split.alias_end]).map(|aliases| (split, aliases)));
    let node = cx.mint_op();
    let span = element_span(cx, element);
    let tag = cx.mint_scope();

    let binding = match split {
        None => {
            // The value cannot decompose under Vue's grammar: it rides
            // whole as the classified escape with pessimal semantics —
            // never a JS parse of the whole value (P2-5b, the
            // `a in b in c` disagreement).
            cx.error(text_span, String::from("v-for has invalid expression."));
            let source = opaque_at(cx, OpaqueReason::ForValue, text, text_span);
            let value = value_hole(cx, text);
            cx.record(
                "error.v-for-malformed",
                node,
                text,
                String::from("ui.for source=opaque(for-value) value=opaque(for-value)"),
                text_span,
            );
            let binding = ForBinding {
                source,
                value,
                key: None,
                index: None,
            };
            let scope = ScopeFacts {
                tag,
                bindings: StdVec::new(),
            };
            let parts = derive_for_parts(tag, &binding, &scope);
            cx.attach_scope(node, scope);
            cx.attach_for_parts(node, parts, 0, text_span);
            binding
        }
        Some((split, aliases)) => {
            let source = expr_at(cx, &raw[split.source_start..]);
            let value = match aliases.first() {
                Some(slice) if !slice.is_empty() => expr_at(cx, slice),
                // An absent alias is still a position: zero-width at the
                // value's start, escape-classified (P2-5b).
                _ => value_hole(cx, text),
            };
            let key = alias_position(cx, aliases.get(1));
            let index = alias_position(cx, aliases.get(2));

            let mut bindings = StdVec::new();
            for expr in [Some(&value), key.as_ref(), index.as_ref()]
                .into_iter()
                .flatten()
            {
                if let Some(name) = simple_identifier(expr) {
                    bindings.push(ScopeBinding {
                        name: String::from(name),
                        origin: ScopeOrigin::Authored { span: expr.span() },
                    });
                }
            }
            cx.record(
                "lower.for",
                node,
                text,
                cstr!("ui.for source={} value={}", desc(&source), desc(&value)),
                text_span,
            );
            let binding = ForBinding {
                source,
                value,
                key,
                index,
            };
            let scope = ScopeFacts { tag, bindings };
            let parts = derive_for_parts(tag, &binding, &scope);
            let binding_count = scope.bindings.len();
            cx.attach_scope(node, scope);
            cx.attach_for_parts(node, parts, binding_count, text_span);
            binding
        }
    };

    let region = Region {
        ops: if element.tag() == "template" && !analyzed.has_slot_spelling() {
            cx.report_missing_close(element);
            let captured = capture_wrapper_key(cx, element, analyzed);
            let (skip, key) = match captured {
                Some((index, key)) => (Some(index), Some(key)),
                None => (None, None),
            };
            let (attr_indexes, attributes, class) =
                capture_wrapper_attrs(cx, element, analyzed, skip);
            record_template_drops_except(cx, element, analyzed, skip, &attr_indexes);
            cx.attach_for_wrapper(
                node,
                ForWrapper {
                    key,
                    attributes,
                    class,
                },
            );
            lower_children(cx, &element.children, ns)
        } else {
            let op = element_core(cx, element, analyzed, ns);
            let mut ops: Vec<'a, Op<'a>> = Vec::new_in(&cx.allocator);
            ops.push(op);
            ops
        },
    };
    cx.observe(OpFamily::For);
    Op::For(Box::new_in(
        ForOp {
            binding,
            region,
            span,
        },
        &cx.allocator,
    ))
}

/// The zero-width escape at the alias's position (absent alias, or the
/// undecomposable whole).
fn value_hole<'a>(cx: &Cx<'a>, text: &'a str) -> ExprRef<'a> {
    let hole = &text[..0];
    let span = cx.span_of(hole);
    opaque_at(cx, OpaqueReason::ForValue, hole, span)
}

/// An optional key/index position: present iff authored non-empty.
fn alias_position<'a>(cx: &mut Cx<'a>, slice: Option<&&'a str>) -> Option<ExprRef<'a>> {
    match slice {
        Some(slice) if !slice.is_empty() => Some(expr_at(cx, slice)),
        _ => None,
    }
}

/// Re-derive the consumed scope view from the just-built `ForBinding`, then
/// assert it byte-equals the `ScopeFacts` the lowering recorded.
fn derive_for_parts(tag: ScopeTag, binding: &ForBinding<'_>, recorded: &ScopeFacts) -> ForParts {
    debug_assert!(
        recorded.tag == tag,
        "hygiene law broken: ui.for scope recorded tag {} but lowering minted {tag}",
        recorded.tag,
    );
    let undecomposable = matches!(
        &binding.source,
        ExprRef::Opaque(opaque) if matches!(opaque.reason, OpaqueReason::ForValue)
    );
    let value = if undecomposable {
        ForName::Pending
    } else {
        position(Some(&binding.value))
    };
    let key = position(binding.key.as_ref());
    let index = position(binding.index.as_ref());
    #[cfg(debug_assertions)]
    {
        let expected: StdVec<ScopeBinding> = [
            (&value, Some(&binding.value)),
            (&key, binding.key.as_ref()),
            (&index, binding.index.as_ref()),
        ]
        .into_iter()
        .filter_map(|(name, expr)| match (name, expr) {
            (ForName::Named(name), Some(expr)) => Some(ScopeBinding {
                name: name.clone(),
                origin: ScopeOrigin::Authored { span: expr.span() },
            }),
            _ => None,
        })
        .collect();
        debug_assert!(
            recorded.bindings == expected,
            "hygiene law broken: ui.for recorded bindings {:?} but its binding surface derives {:?}",
            recorded.bindings,
            expected,
        );
    }
    ForParts {
        tag,
        value,
        key,
        index,
    }
}

/// Classify one binding position.
fn position(expr: Option<&ExprRef<'_>>) -> ForName {
    let Some(expr) = expr else {
        return ForName::Absent;
    };
    if let Some(name) = simple_identifier(expr) {
        return ForName::Named(String::from(name));
    }
    if expr.source().is_empty() {
        return ForName::Absent;
    }
    ForName::Pending
}
