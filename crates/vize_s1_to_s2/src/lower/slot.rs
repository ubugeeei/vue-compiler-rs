//! `<slot>` outlets into `ui.slot`: name (static or computed), the props
//! surface, and the owned fallback region.
//!
//! An implicit name normalizes to `"default"` — the lowering decision
//! `SlotOp` documents as the dialect's, made here. Since the P2-9
//! element/binding-family installment the outlet carries its props
//! surface: every non-name static attribute is a slot prop
//! ([`Attribute`]), `v-bind`/`v-on` lower to their attached ops, and a
//! `v-slot` spelled **on** the outlet lowers to `ui.slot-content` so the
//! slot pass can fire the legacy `VSlotMisplaced` diagnostic where the
//! shipped lane fires it. `v-html` and `v-text` lower to dialect bindings
//! because the shipped lane emits them as slot props; `v-cloak` lowers as
//! an inert marker because the shipped lane removes it. A custom directive
//! on an outlet is Vue's own error and is dropped under it; the
//! still-deferred built-ins keep their counted `defer.slot-directive`
//! records with realization (P2-11) as the named owner.

use super::features::OpFamily;
use vize_s0::{Box, String, Vec, cstr};
use vize_s1::Element;

use vize_s2::expr::ExprRef;
use vize_s2::op::{Attribute, BindingOp, DynamicName, Namespace, Op, Region, SlotOp};

use super::binding::{defer, lower_slot_content};
use super::bindop::{RULE_SAME_NAME, lower_bind, lower_on};
use super::cx::{Cx, attr_slice, attr_span, element_span};
use super::directive::{Arg, AttrForm, Head};
use super::element::{Analyzed, attr_value_text};
use super::expr::{desc, expr_at};
use super::structural::lower_children;

/// Lower one `<slot>` element into a `ui.slot` outlet.
pub(crate) fn lower_slot<'a>(
    cx: &mut Cx<'a>,
    element: &Element<'a>,
    analyzed: &Analyzed<'a>,
    ns: Namespace,
) -> Op<'a> {
    let span = element_span(cx, element);
    let node = cx.mint_op();

    let mut name: Option<DynamicName<'a>> = None;
    let mut attributes: Vec<'a, Attribute<'a>> = Vec::new_in(&cx.allocator);
    let mut bindings: Vec<'a, BindingOp<'a>> = Vec::new_in(&cx.allocator);
    for (index, attr) in element.open.attrs.iter().enumerate() {
        if Some(index) == analyzed.branch.map(|(idx, _)| idx) || Some(index) == analyzed.vfor {
            continue;
        }
        match &analyzed.forms[index] {
            AttrForm::Static if attr.name.text == "name" && name.is_none() => {
                // A value-less `name` reads as the implicit name — the
                // shipped resolution (`codegen/slots/outlet.rs`), matched
                // exactly so the two lanes never differ on the spelling.
                name = Some(DynamicName::Static(
                    attr_value_text(element, index).unwrap_or("default"),
                ));
            }
            AttrForm::Static => attributes.push(Attribute {
                name: attr.name.text,
                value: attr_value_text(element, index),
                span: attr_span(cx, attr),
            }),
            AttrForm::Directive(directive) => match directive.head {
                Head::Bind
                    if matches!(directive.arg, Some(Arg::Static("name"))) && name.is_none() =>
                {
                    // The shipped parser applies Vue 3.4 same-name
                    // shorthand before slot-outlet name resolution, so
                    // a missing or blank `:name` / `v-bind:name`
                    // selects the runtime `name` variable.
                    match attr_value_text(element, index).map(str::trim) {
                        Some(text) if !text.is_empty() => {
                            name = Some(DynamicName::Dynamic(expr_at(cx, text)));
                        }
                        _ => {
                            let arg_text = match directive.arg {
                                Some(Arg::Static(text)) => text,
                                _ => "name",
                            };
                            let expr =
                                ExprRef::parse_js_in(cx.allocator, arg_text, cx.span_of(arg_text));
                            cx.record(
                                RULE_SAME_NAME,
                                node,
                                attr_slice(cx, attr),
                                cstr!("{arg_text}"),
                                attr_span(cx, attr),
                            );
                            name = Some(DynamicName::Dynamic(expr));
                        }
                    }
                }
                Head::Bind => bindings.push(lower_bind(cx, element, index, directive)),
                Head::On => bindings.push(lower_on(cx, element, index, directive)),
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
                Head::Slot => bindings.push(lower_slot_content(cx, element, index, directive)),
                Head::Custom => {
                    cx.error(
                        attr_span(cx, attr),
                        String::from("Unexpected custom directive on <slot> outlet."),
                    );
                    cx.record(
                        "error.slot-directive",
                        None,
                        attr_slice(cx, attr),
                        String::default(),
                        attr_span(cx, attr),
                    );
                }
                Head::Model => {
                    cx.error(
                        attr_span(cx, attr),
                        String::from("v-model is not supported on <slot> outlets."),
                    );
                    cx.record(
                        "error.v-model-on-slot",
                        None,
                        attr_slice(cx, attr),
                        String::default(),
                        attr_span(cx, attr),
                    );
                }
                _ => defer(
                    cx,
                    "defer.slot-directive",
                    attr_span(cx, attr),
                    attr_slice(cx, attr),
                    cstr!(
                        "`{}` on a <slot> outlet has no S2 op; it stays with DOM realization (P2-11)",
                        attr.name.text
                    ),
                ),
            },
        }
    }
    let name = name.unwrap_or(DynamicName::Static("default"));
    let after = match &name {
        DynamicName::Static(text) => cstr!("ui.slot \"{text}\""),
        DynamicName::Dynamic(expr) => cstr!("ui.slot {}", desc(expr)),
    };
    let open_end = cx.token_span(&element.open.gt).end;
    let open_slice = cx
        .source
        .get(cx.offset(element.open.lt_name.text) as usize..open_end as usize)
        .unwrap_or("slot");
    cx.record("lower.slot", node, open_slice, after, span);

    let fallback = Region {
        ops: lower_children(cx, &element.children, ns),
    };
    cx.observe(OpFamily::SlotCarrier);
    Op::Slot(Box::new_in(
        SlotOp {
            name,
            attributes,
            bindings,
            fallback,
            span,
        },
        &cx.allocator,
    ))
}
