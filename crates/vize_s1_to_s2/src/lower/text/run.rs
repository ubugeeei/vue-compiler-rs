use alloc::vec::Vec as StdVec;

use vize_s0::{Box, Span, String, StringBuilder, cstr};
use vize_s1::SurfaceChild;
use vize_s2::expr::OpaqueReason;
use vize_s2::op::{InterpolationOp, Op, TextOp};

use super::{TextAction, TextPart, TextParts, collapse_fused, extends_run, rebuild_source};
use crate::lower::cx::Cx;
use crate::lower::expr::{desc, opaque_at, trimmed};

/// Fold a consumed dropped-member gap into the preceding part's
/// authored range, so the recorded parts tile the merged span exactly.
///
/// The gap is either a condensed whitespace run's dropped tail — which
/// the neighbour rules only ever place after its condensed head, a
/// static part — or a comment this compile is not preserving, which can
/// sit after either kind. Both fold the same way: the preceding part's
/// range grows to cover the consumed bytes.
fn fold_gap(parts: &mut StdVec<TextPart>, pending: &mut Option<u32>) {
    if let Some(gap_end) = pending.take()
        && let Some(last) = parts.last_mut()
    {
        last.span.end = gap_end;
    }
}

/// Lower the maximal text/interpolation run starting at `start`,
/// returning the index just past it. Dropped whitespace lowers to
/// nothing (recorded); a one-node run lowers as the plain leaf; a
/// longer run merges.
pub(crate) fn lower_text_run<'a>(
    cx: &mut Cx<'a>,
    children: &[SurfaceChild<'a>],
    plan: &[TextAction<'a>],
    start: usize,
    out: &mut vize_s0::Vec<'a, Op<'a>>,
) -> usize {
    if let SurfaceChild::Text(token) = &children[start]
        && plan[start] == TextAction::Drop
    {
        let span = cx.token_span(token);
        cx.record(
            "condense.drop-whitespace",
            None,
            token.text,
            String::default(),
            span,
        );
        return start + 1;
    }

    // Scan the run: span-contiguous text/interpolation children whose
    // plan keeps them. Adjacent static members **fuse** into one part —
    // two list-adjacent text nodes are one DOM text run (the shape only
    // split or recovered S1 trees present; a parse emits maximal runs)
    // — and the condense collapse re-runs across the fused content, so
    // a whitespace run straddling the seam condenses exactly as the
    // one-node spelling does. A `Drop`-planned member inside the run (a
    // condensed whitespace run's tail) is consumed, recorded, and its
    // bytes folded into the preceding part's authored range, so the
    // parts still tile the merged span; dropped bytes with no following
    // member stay outside the unit.
    let mut parts: StdVec<TextPart> = StdVec::new();
    let mut members = 0usize;
    let mut i = start;
    let mut end = 0u32;
    let mut pending_gap: Option<u32> = None;
    while i < children.len() {
        let probe = pending_gap.unwrap_or(end);
        // A comment the compile is not preserving is not a run boundary:
        // it is not a child at all. Vue's parser builds no node for it,
        // and `onText` appends the next chunk to the previous text child
        // without checking contiguity — so `a<!--c-->b` reaches
        // condensing as the one node `ab`. Consumed here exactly like a
        // dropped whitespace member: recorded, and its bytes folded into
        // the preceding part's range so the parts still tile the merged
        // span. Contiguity is still required, so the merged span is
        // still the authored bytes.
        if i > start
            && let SurfaceChild::Comment(token) = &children[i]
            && !cx.preserve_comments()
            && token.leading.is_empty()
            && cx.offset(token.text) == probe
        {
            let span = cx.token_span(token);
            cx.record("drop.comment", None, token.text, String::default(), span);
            pending_gap = Some(span.end);
            i += 1;
            continue;
        }
        if i > start && !extends_run(cx, &children[i], probe) {
            break;
        }
        match &children[i] {
            SurfaceChild::Text(token) => {
                let span = cx.token_span(token);
                if plan[i] == TextAction::Drop {
                    // A condensed whitespace run's tail member: consume
                    // and record it here (never a run start — the
                    // pre-scan arm returns those).
                    cx.record(
                        "condense.drop-whitespace",
                        None,
                        token.text,
                        String::default(),
                        span,
                    );
                    pending_gap = Some(span.end);
                    i += 1;
                    continue;
                }
                let content = match plan[i] {
                    TextAction::Content(content) => content,
                    _ => token.text,
                };
                fold_gap(&mut parts, &mut pending_gap);
                match parts.last_mut() {
                    Some(last) if !last.dynamic => {
                        last.text.push_str(content);
                        last.span.end = span.end;
                    }
                    _ => parts.push(TextPart {
                        text: String::from(content),
                        span,
                        dynamic: false,
                    }),
                }
                end = span.end;
            }
            SurfaceChild::Interpolation(node) => {
                let span = Span::new(cx.offset(node.open.text), cx.token_span(&node.close).end);
                let (slice, _) = trimmed(cx, node.content.text);
                fold_gap(&mut parts, &mut pending_gap);
                parts.push(TextPart {
                    text: String::from(slice),
                    span,
                    dynamic: true,
                });
                end = span.end;
            }
            _ => break,
        }
        members += 1;
        i += 1;
    }
    if !cx.condense_suppressed() {
        for part in parts.iter_mut().filter(|part| !part.dynamic) {
            collapse_fused(&mut part.text);
        }
    }

    if members == 1 {
        // A lone node never merges (the legacy run grouping's own rule);
        // it lowers as the plain leaf, with the condensed content.
        match &children[start] {
            SurfaceChild::Text(token) => {
                let content = match plan[start] {
                    TextAction::Content(content) => content,
                    _ => token.text,
                };
                if content != token.text {
                    let span = cx.token_span(token);
                    cx.record(
                        "condense.whitespace",
                        None,
                        token.text,
                        String::from(content),
                        span,
                    );
                }
                super::super::leaf::lower_text(cx, token, content, out);
            }
            child => super::super::leaf::lower_leaf(cx, child, out),
        }
        return start + 1;
    }

    let span = Span::new(parts[0].span.start, end);
    let raw = cx
        .source
        .get(span.start as usize..span.end as usize)
        .unwrap_or_default();
    let dynamic = parts.iter().filter(|part| part.dynamic).count();
    let node = cx.mint_op();
    if dynamic == 0 {
        // A text-only run merges into one `ui.text`.
        let mut content = StringBuilder::with_capacity_in(raw.len(), cx.allocator);
        for part in &parts {
            content.push_str(part.text.as_str());
        }
        cx.record(
            "lower.text-run",
            node,
            raw,
            cstr!("ui.text merged={members}"),
            span,
        );
        out.push(Op::Text(Box::new_in(
            TextOp {
                content: content.into_str(),
                span,
            },
            &cx.allocator,
        )));
    } else {
        // A mixed run is the compound representation: one
        // `ui.interpolation` under the pessimal opaque laws, parts
        // recorded beside the tree for the pass to validate and P2-11
        // to compile from.
        let rebuilt = rebuild_source(&parts);
        let mut source = StringBuilder::with_capacity_in(rebuilt.len(), cx.allocator);
        source.push_str(rebuilt.as_str());
        let expression = opaque_at(cx, OpaqueReason::Compound, source.into_str(), span);
        cx.record(
            "lower.compound",
            node,
            raw,
            cstr!(
                "ui.interpolation {} parts={} dynamic={}",
                desc(&expression),
                parts.len(),
                dynamic
            ),
            span,
        );
        cx.attach_texts(node, TextParts { parts });
        out.push(Op::Interpolation(Box::new_in(
            InterpolationOp { expression, span },
            &cx.allocator,
        )));
    }
    i
}

/// Lower a contiguous text/interpolation run under `v-pre`.
/// Interpolation delimiters render as authored text, while Vue's normal
/// whitespace condense still applies unless an enclosing `<pre>` disabled it.
pub(crate) fn lower_v_pre_text_run<'a>(
    cx: &mut Cx<'a>,
    children: &[SurfaceChild<'a>],
    start: usize,
    out: &mut vize_s0::Vec<'a, Op<'a>>,
) -> usize {
    let Some(mut end) = text_family_end(cx, &children[start]) else {
        return start + 1;
    };
    let start_offset = text_family_start(cx, &children[start]).unwrap_or(end);
    let mut i = start + 1;
    while i < children.len() {
        let Some(next_start) = text_family_start(cx, &children[i]) else {
            break;
        };
        if next_start != end {
            break;
        }
        let Some(next_end) = text_family_end(cx, &children[i]) else {
            break;
        };
        end = next_end;
        i += 1;
    }

    let span = Span::new(start_offset, end);
    let raw = cx
        .source
        .get(span.start as usize..span.end as usize)
        .unwrap_or_default();
    let mut content = String::from(raw);
    if !cx.condense_suppressed() {
        collapse_fused(&mut content);
    }
    let mut interned = StringBuilder::with_capacity_in(content.len(), cx.allocator);
    interned.push_str(content.as_str());
    let node = cx.mint_op();
    cx.record("lower.v-pre-text", node, raw, String::from("ui.text"), span);
    out.push(Op::Text(Box::new_in(
        TextOp {
            content: interned.into_str(),
            span,
        },
        &cx.allocator,
    )));
    i
}

fn text_family_start(cx: &Cx<'_>, child: &SurfaceChild<'_>) -> Option<u32> {
    match child {
        SurfaceChild::Text(token) => Some(cx.offset(token.text)),
        SurfaceChild::Interpolation(node) => Some(cx.offset(node.open.text)),
        _ => None,
    }
}

fn text_family_end(cx: &Cx<'_>, child: &SurfaceChild<'_>) -> Option<u32> {
    match child {
        SurfaceChild::Text(token) => Some(cx.token_span(token).end),
        SurfaceChild::Interpolation(node) => Some(cx.token_span(&node.close).end),
        _ => None,
    }
}
