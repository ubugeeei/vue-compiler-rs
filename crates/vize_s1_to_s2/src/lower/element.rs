//! Per-element lowering: attribute analysis, element/component/slot
//! dispatch, and the markup-namespace inheritance.
//!
//! # Namespace, v1 scope (recorded in the P2-8 record)
//!
//! The namespace rule is tag inheritance: SVG/MathML tags open their
//! namespace, and the SVG/MathML HTML integration points
//! (`foreignObject`/`desc`/`title`; `mi`/`mo`/`mn`/`ms`/`mtext`) return
//! their **children** to HTML. This approximates the HTML tree-construction
//! namespace algorithm the way the shipped compiler-dom logic does.

use super::features::OpFamily;
use alloc::vec::Vec as StdVec;

use vize_s0::{Box, Vec, cstr, is_math_ml_tag, is_native_tag, is_svg_tag};
use vize_s1::Element;

use vize_s2::op::{Attribute, BindingOp, ComponentOp, ElementOp, Namespace, Op, Region};

use super::binding::{Owner, lower_attr};
use super::cx::{Cx, attr_span, element_span};
use super::directive::{AttrForm, Head, classify};
use super::slot::lower_slot;
use super::structural::lower_children;

/// Which branch of a `v-if` chain an element opens or continues.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BranchKind {
    If,
    ElseIf,
    Else,
}

/// One element's attribute analysis, computed once per element.
#[derive(Debug)]
pub(crate) struct Analyzed<'a> {
    /// One form per authored attribute, by index.
    pub forms: StdVec<AttrForm<'a>>,
    /// The first branch directive (attr index, kind), when present.
    pub branch: Option<(usize, BranchKind)>,
    /// The first `v-for` (attr index), when present.
    pub vfor: Option<usize>,
}

/// Analyze an element's attributes: classify every name, note the
/// structural directives.
pub(crate) fn analyze<'a>(element: &Element<'a>) -> Analyzed<'a> {
    let mut forms = StdVec::with_capacity(element.open.attrs.len());
    let mut branch = None;
    let mut vfor = None;
    for (index, attr) in element.open.attrs.iter().enumerate() {
        let form = classify(attr.name.text);
        if let AttrForm::Directive(directive) = &form {
            let kind = match directive.head {
                Head::If => Some(BranchKind::If),
                Head::ElseIf => Some(BranchKind::ElseIf),
                Head::Else => Some(BranchKind::Else),
                _ => None,
            };
            if let Some(kind) = kind
                && branch.is_none()
            {
                branch = Some((index, kind));
            }
            if directive.head == Head::For && vfor.is_none() {
                vfor = Some(index);
            }
        }
        forms.push(form);
    }
    Analyzed {
        forms,
        branch,
        vfor,
    }
}

impl Analyzed<'_> {
    /// Whether the element carries a `v-slot` / `#` spelling. A
    /// `<template v-if #name>` / `<template v-for #name>` must keep the
    /// template op so the slot name survives unwrap (P2-11 createSlots).
    pub(crate) fn has_slot_spelling(&self) -> bool {
        self.forms.iter().any(
            |form| matches!(form, AttrForm::Directive(directive) if directive.head == Head::Slot),
        )
    }

    /// Whether the element carries a `v-pre` spelling.
    pub(crate) fn has_v_pre(&self) -> bool {
        self.forms.iter().any(
            |form| matches!(form, AttrForm::Directive(directive) if directive.head == Head::Pre),
        )
    }
}

/// The authored value text of an attribute, when it has a value node
/// (a `Missing` value hole is a zero-width slice, present but empty).
pub(crate) fn attr_value_text<'a>(element: &Element<'a>, index: usize) -> Option<&'a str> {
    element.open.attrs[index]
        .value
        .as_ref()
        .map(|value| value.content.text)
}

/// An element's own namespace, entered by tag.
fn enter_ns(parent: Namespace, tag: &str) -> Namespace {
    if is_svg_tag(tag) {
        Namespace::Svg
    } else if is_math_ml_tag(tag) {
        Namespace::MathMl
    } else {
        parent
    }
}

/// The namespace an element's children live in (the integration points
/// return to HTML).
fn children_ns(own: Namespace, tag: &str) -> Namespace {
    match own {
        Namespace::Svg if matches!(tag, "foreignObject" | "desc" | "title") => Namespace::Html,
        Namespace::MathMl
            if matches!(tag, "annotation-xml" | "mi" | "mo" | "mn" | "ms" | "mtext") =>
        {
            Namespace::Html
        }
        other => other,
    }
}

/// Lower one element (its structural directives already consumed by the
/// caller): `<slot>` outlet, component, or native element.
pub(crate) fn element_core<'a>(
    cx: &mut Cx<'a>,
    element: &Element<'a>,
    analyzed: &Analyzed<'a>,
    ns: Namespace,
) -> Op<'a> {
    cx.report_missing_close(element);
    let tag = element.tag();
    if tag == "slot" {
        return lower_slot(cx, element, analyzed, ns);
    }
    let own_ns = enter_ns(ns, tag);
    let child_ns = children_ns(own_ns, tag);
    let span = element_span(cx, element);
    let node = cx.mint_op();
    let component = !is_native_tag(tag) && !cx.is_custom_element(tag);

    let open_end = cx.token_span(&element.open.gt).end;
    let open_slice = cx
        .source
        .get(cx.offset(element.open.lt_name.text) as usize..open_end as usize)
        .unwrap_or(tag);
    if component {
        cx.observe(OpFamily::SlotCarrier);
        cx.record(
            "lower.component",
            node,
            open_slice,
            cstr!("ui.component {tag}"),
            span,
        );
    } else {
        let after = match own_ns {
            Namespace::Html => cstr!("ui.element {tag}"),
            Namespace::Svg => cstr!("ui.element {tag} ns=svg"),
            Namespace::MathMl => cstr!("ui.element {tag} ns=mathml"),
        };
        cx.record("lower.element", node, open_slice, after, span);
    }

    let take_scope = super::sugar::should_take(cx, element, analyzed);
    let scope_index = take_scope
        .then(|| super::sugar::scope_attr_index(element, analyzed))
        .flatten();
    let companion_slot = take_scope
        .then(|| super::sugar::companion_slot_index(element, analyzed))
        .flatten();

    let mut attributes: Vec<'a, Attribute<'a>> = Vec::new_in(&cx.allocator);
    let mut bindings: Vec<'a, BindingOp<'a>> = Vec::new_in(&cx.allocator);
    for (index, attr) in element.open.attrs.iter().enumerate() {
        if Some(index) == analyzed.branch.map(|(idx, _)| idx) || Some(index) == analyzed.vfor {
            continue;
        }
        if Some(index) == companion_slot {
            continue;
        }
        match &analyzed.forms[index] {
            AttrForm::Static if Some(index) == scope_index => {
                bindings.push(super::sugar::lower_slot_scope(
                    cx,
                    element,
                    index,
                    companion_slot,
                ));
            }
            AttrForm::Static => attributes.push(Attribute {
                name: attr.name.text,
                value: attr.value.as_ref().map(|value| value.content.text),
                span: attr_span(cx, attr),
            }),
            AttrForm::Directive(directive) => {
                let owner = Owner { tag, component };
                lower_attr(cx, element, index, directive, &owner, &mut bindings);
            }
        }
    }

    // `<pre>` keeps its bytes: condensing is suppressed for the whole
    // subtree (`lower::text`, the shipped `is_pre_tag` configuration).
    let suppress = super::text::suppresses_condense(tag);
    let suppress_v_pre = analyzed.has_v_pre();
    if suppress {
        cx.push_condense_suppression();
    }
    if suppress_v_pre {
        cx.push_v_pre_suppression();
    }
    let children = Region {
        ops: if component || own_ns != Namespace::Html {
            lower_children(cx, &element.children, child_ns)
        } else {
            super::table::lower_element_children(cx, tag, &element.children, child_ns)
        },
    };
    if suppress_v_pre {
        cx.pop_v_pre_suppression();
    }
    if suppress {
        cx.pop_condense_suppression();
    }
    if component {
        Op::Component(Box::new_in(
            ComponentOp {
                name: tag,
                attributes,
                bindings,
                children,
                span,
            },
            &cx.allocator,
        ))
    } else {
        Op::Element(Box::new_in(
            ElementOp {
                tag,
                namespace: own_ns,
                attributes,
                bindings,
                children,
                span,
            },
            &cx.allocator,
        ))
    }
}
