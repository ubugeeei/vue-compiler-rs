//! Span recovery over the S1 surface — the extent questions the lowering
//! asks about an authored node, kept beside [`Cx`](super::Cx) rather than
//! inside it.
//!
//! S1 is a contiguous partition of the source, so every extent here is
//! recovered from the tokens the parser kept: a typed hole is zero-width
//! by policy and contributes nothing, which is what makes a `Missing`
//! close fall back to its children's extent instead of guessing.

use vize_s0::Span;
use vize_s1::{CloseTag, Element, ElementClose, SurfaceChild};

use super::Cx;

/// End offset of an element's extent: the last byte its subtree rendered.
/// S1 is a contiguous partition of the source, so this is the end of the
/// element's last present-or-hole token — a `Missing` close contributes
/// its children's extent (the typed hole is zero-width by policy).
fn element_end(cx: &Cx<'_>, element: &Element<'_>) -> u32 {
    match &element.close {
        ElementClose::Present(CloseTag { gt, .. }) => cx.token_span(gt).end,
        ElementClose::NotExpected => cx.token_span(&element.open.gt).end,
        ElementClose::Implicit | ElementClose::Missing => element
            .children
            .last()
            .map(|child| child_end(cx, child))
            .unwrap_or_else(|| cx.token_span(&element.open.gt).end),
    }
}

/// End offset of any child's extent.
fn child_end(cx: &Cx<'_>, child: &SurfaceChild<'_>) -> u32 {
    match child {
        SurfaceChild::Element(element) => element_end(cx, element),
        SurfaceChild::Interpolation(node) => cx.token_span(&node.close).end,
        SurfaceChild::Text(token)
        | SurfaceChild::Comment(token)
        | SurfaceChild::Cdata(token)
        | SurfaceChild::ProcessingInstruction(token)
        | SurfaceChild::Unexpected(token) => cx.token_span(token).end,
    }
}

/// The span of an element's whole extent (open tag through close).
pub(crate) fn element_span(cx: &Cx<'_>, element: &Element<'_>) -> Span {
    Span::new(
        cx.offset(element.open.lt_name.text),
        element_end(cx, element),
    )
}

/// The span of one authored attribute: name through the end of its value
/// (closing quote included when present).
pub(crate) fn attr_span(cx: &Cx<'_>, attr: &vize_s1::Attribute<'_>) -> Span {
    let start = cx.offset(attr.name.text);
    let end = match &attr.value {
        Some(value) => match &value.close_quote {
            Some(close) => cx.token_span(close).end,
            None => cx.token_span(&value.content).end,
        },
        None => match &attr.eq {
            Some(eq) => cx.token_span(eq).end,
            None => cx.token_span(&attr.name).end,
        },
    };
    Span::new(start, end)
}

/// The authored slice an attribute covers (for provenance `before`).
pub(crate) fn attr_slice<'a>(cx: &Cx<'a>, attr: &vize_s1::Attribute<'a>) -> &'a str {
    let span = attr_span(cx, attr);
    cx.source
        .get(span.start as usize..span.end as usize)
        .unwrap_or(attr.name.text)
}
