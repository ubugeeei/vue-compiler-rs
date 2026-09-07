//! Attached bindings: `v-bind`/`v-on` into the normalized one-way ops
//! ([`super::bindop`], P2-9 series 5), `v-model` into the `ui.model`
//! contract, `v-slot` into `ui.slot-content`, custom directives into
//! `vue.directive`, well-formed `v-once`/`v-memo` into `vue.once` /
//! `vue.memo` ([`super::once_memo`]), and the deferral of what remains.
//!
//! # Unmappable means diagnostic, never a new op
//!
//! `v-show`, `v-html`, `v-text` and `v-cloak` are P2-11 dialect ops
//! (`vue.show`, `vue.html`, `vue.text`, `vue.cloak`) because the DOM
//! realization reads them directly. `v-pre` still has **no S2 op**, and
//! forcing it into
//! `vue.directive` would break its documented contract ("built-in
//! directives never appear here"). It is deferred: an `Info` diagnostic —
//! the input is not wrong, the stage is younger than the construct — plus
//! a provenance record naming the rule, with the owner op kept as the
//! fragment. `v-once`, `v-memo`, and the DOM directives now have dialect
//! ops; only ill-formed spellings still defer. The remaining set is still
//! DOM realization (P2-11).

use super::features::OpFamily;
use alloc::vec::Vec as StdVec;

use vize_s0::{Box, Span, String, Vec, cstr};
use vize_s1::Element;

use vize_s2::op::{
    Attribute, BindingContract, BindingOp, DynamicName, ModelOp, SlotContentOp, VueDirectiveOp,
};
use vize_s2::scope::{ScopeBinding, ScopeFacts, ScopeOrigin};

use super::cx::{Cx, attr_slice, attr_span};
use super::directive::{Arg, Directive, Head};
use super::element::attr_value_text;
use super::expr::{desc, expr_at, simple_identifier};

/// The op an attribute attaches to.
pub(crate) struct Owner<'a> {
    /// The owner's authored tag or component name.
    pub tag: &'a str,
    /// Whether the owner is a `ui.component`.
    pub component: bool,
}

/// Lower one non-structural directive attribute of an element or
/// component.
pub(crate) fn lower_attr<'a>(
    cx: &mut Cx<'a>,
    element: &Element<'a>,
    index: usize,
    directive: &Directive<'a>,
    owner: &Owner<'a>,
    bindings: &mut Vec<'a, BindingOp<'a>>,
) {
    let attr = &element.open.attrs[index];
    match directive.head {
        Head::Bind => bindings.push(super::bindop::lower_bind(cx, element, index, directive)),
        Head::On => bindings.push(super::bindop::lower_on(cx, element, index, directive)),
        Head::Model => {
            if let Some(op) = lower_model(cx, element, index, directive, owner) {
                bindings.push(op);
            }
        }
        Head::Custom => bindings.push(lower_custom(cx, element, index, directive)),
        Head::Slot => bindings.push(lower_slot_content(cx, element, index, directive)),
        Head::Once => {
            if let Some(op) = super::once_memo::lower_once(cx, element, index, directive) {
                bindings.push(op);
            }
        }
        Head::Memo => {
            if let Some(op) = super::once_memo::lower_memo(cx, element, index, directive) {
                bindings.push(op);
            }
        }
        Head::Pre => defer(
            cx,
            "defer.v-pre",
            attr_span(cx, attr),
            attr_slice(cx, attr),
            String::from(
                "`v-pre` has no S2 representation; its subtree lowers as ordinary content",
            ),
        ),
        Head::Show => {
            if let Some(op) = super::show::lower_show(cx, element, index, directive) {
                bindings.push(op);
            }
        }
        Head::Html => {
            if let Some(op) = super::html::lower_html(cx, element, index, directive) {
                bindings.push(op);
            }
        }
        Head::Text => {
            if let Some(op) = super::vtext::lower_text(cx, element, index, directive) {
                bindings.push(op);
            }
        }
        Head::Cloak => {
            bindings.push(super::cloak::lower_cloak(cx, element, index, directive));
        }
        Head::If | Head::ElseIf | Head::Else | Head::For => {
            // The structural pass consumed the first of each; a duplicate
            // is ignored under a record, never silently.
            cx.info(
                attr_span(cx, attr),
                cstr!(
                    "duplicate structural directive `{}` is ignored",
                    attr.name.text
                ),
            );
            cx.record(
                "drop.structural-duplicate",
                None,
                attr_slice(cx, attr),
                String::default(),
                attr_span(cx, attr),
            );
        }
    }
}

/// An `Info` deferral: rule + record + diagnostic, owner kept.
pub(crate) fn defer(
    cx: &mut Cx<'_>,
    rule: &'static str,
    span: Span,
    before: &str,
    message: String,
) {
    cx.info(span, message);
    cx.record(rule, None, before, String::default(), span);
}

/// `v-model` into the `ui.model` contract: read and write are the same
/// authored payload (custom accessors split them in later dialects);
/// the argument rides the model name position, and element kind plus
/// modifiers ride as synthesized attributes carrying the binding's span.
fn lower_model<'a>(
    cx: &mut Cx<'a>,
    element: &Element<'a>,
    index: usize,
    directive: &Directive<'a>,
    owner: &Owner<'a>,
) -> Option<BindingOp<'a>> {
    let attr = &element.open.attrs[index];
    let span = attr_span(cx, attr);
    let text = attr_value_text(element, index);
    if text.map(str::trim).is_none_or(str::is_empty) {
        cx.error(span, String::from("v-model is missing expression."));
        cx.record(
            "error.v-model-no-expression",
            None,
            attr_slice(cx, attr),
            String::default(),
            span,
        );
        return None;
    }
    let text = text?;
    let node = cx.mint_op();
    let expr = expr_at(cx, text);
    let argument = directive.arg.map(|arg| match arg {
        Arg::Static(argument) => DynamicName::Static(argument),
        Arg::Dynamic(inner) => DynamicName::Dynamic(expr_at(cx, inner)),
    });
    let mut attributes: Vec<'a, Attribute<'a>> = Vec::new_in(&cx.allocator);
    attributes.push(Attribute {
        name: "element-kind",
        value: Some(if owner.component {
            "component"
        } else {
            owner.tag
        }),
        span,
    });
    for modifier in &directive.modifiers {
        attributes.push(Attribute {
            name: modifier,
            value: None,
            span,
        });
    }
    cx.observe(OpFamily::Model);
    cx.record(
        "lower.model",
        node,
        attr_slice(cx, attr),
        String::from("ui.model"),
        span,
    );
    Some(BindingOp::Model(Box::new_in(
        ModelOp {
            contract: BindingContract {
                read: expr,
                write: expr,
            },
            argument,
            attributes,
            span,
        },
        &cx.allocator,
    )))
}

/// A user-defined directive rides through as the `vue.directive` dialect
/// op, exactly as authored.
fn lower_custom<'a>(
    cx: &mut Cx<'a>,
    element: &Element<'a>,
    index: usize,
    directive: &Directive<'a>,
) -> BindingOp<'a> {
    let attr = &element.open.attrs[index];
    let span = attr_span(cx, attr);
    let node = cx.mint_op();
    let argument = directive.arg.map(|arg| match arg {
        Arg::Static(name) => DynamicName::Static(name),
        Arg::Dynamic(inner) => DynamicName::Dynamic(expr_at(cx, inner)),
    });
    let mut modifiers: Vec<'a, &'a str> = Vec::new_in(&cx.allocator);
    for modifier in &directive.modifiers {
        modifiers.push(modifier);
    }
    let value = attr_value_text(element, index)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(|text| expr_at(cx, text));
    cx.record(
        "lower.vue-directive",
        node,
        attr_slice(cx, attr),
        cstr!("vue.directive \"{}\"", directive.name),
        span,
    );
    BindingOp::VueDirective(Box::new_in(
        VueDirectiveOp {
            name: directive.name,
            argument,
            modifiers,
            value,
            span,
        },
        &cx.allocator,
    ))
}

/// One `v-slot` spelling into the attached `ui.slot-content` op, exactly
/// as authored: name position (`None` when the spelling leaves it
/// implicit — the default-name synthesis is the slot pass's, where the
/// authored-vs-synthesized origin is recorded), modifiers verbatim, and
/// the params position when non-blank.
///
/// The slot-props scope facts (recorded since P2-8) key the binding op
/// itself: each spelling is its own binding-introduction site, so two
/// `v-slot`s on one carrier never share an entry. The props name is
/// recorded when it is a simple identifier; patterns wait for the one
/// identifier-enumeration seam (#4365).
pub(crate) fn lower_slot_content<'a>(
    cx: &mut Cx<'a>,
    element: &Element<'a>,
    index: usize,
    directive: &Directive<'a>,
) -> BindingOp<'a> {
    let attr = &element.open.attrs[index];
    let span = attr_span(cx, attr);
    let node = cx.mint_op();
    let name = directive.arg.map(|arg| match arg {
        Arg::Static(text) => DynamicName::Static(text),
        Arg::Dynamic(inner) => DynamicName::Dynamic(expr_at(cx, inner)),
    });
    let mut modifiers: Vec<'a, &'a str> = Vec::new_in(&cx.allocator);
    for modifier in &directive.modifiers {
        modifiers.push(modifier);
    }
    let params = attr_value_text(element, index)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(|text| expr_at(cx, text));
    if let Some(expr) = &params {
        let tag = cx.mint_scope();
        let mut scope_bindings = StdVec::new();
        if let Some(bound) = simple_identifier(expr) {
            scope_bindings.push(ScopeBinding {
                name: String::from(bound),
                origin: ScopeOrigin::Authored { span: expr.span() },
            });
        }
        cx.attach_scope(
            node,
            ScopeFacts {
                tag,
                bindings: scope_bindings,
            },
        );
    }
    let after = match &name {
        None => String::from("ui.slot-content"),
        Some(DynamicName::Static(text)) => cstr!("ui.slot-content \"{text}\""),
        Some(DynamicName::Dynamic(expr)) => cstr!("ui.slot-content {}", desc(expr)),
    };
    cx.record(
        "lower.slot-content",
        node,
        attr_slice(cx, attr),
        after,
        span,
    );
    cx.observe(OpFamily::SlotCarrier);
    BindingOp::SlotContent(Box::new_in(
        SlotContentOp {
            name,
            modifiers,
            params,
            span,
        },
        &cx.allocator,
    ))
}
