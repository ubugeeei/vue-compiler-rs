//! Vue 2 filter rewrite for dynamic parts of merged text runs.

use alloc::vec::Vec as StdVec;

use vize_davinci::id::NodeId;
use vize_davinci::side_table::SideTable;
use vize_s0::{Allocator, Span, String};
use vize_s2::expr::{ExprRef, OpaqueExpr, OpaqueReason, VueFilterExpr};
use vize_s2::op::InterpolationOp;

use crate::lower::{TextPart, TextParts, rebuild_source};

pub(super) fn rewrite<'a>(
    allocator: &'a Allocator,
    source: &'a str,
    id: Option<NodeId>,
    interp: &mut InterpolationOp<'a>,
    texts: &mut SideTable<TextParts>,
    filters: &mut StdVec<String>,
) {
    let ExprRef::Opaque(opaque) = interp.expression else {
        return;
    };
    if opaque.reason != OpaqueReason::Compound {
        return;
    }
    let Some(id) = id else {
        return;
    };
    let Some(parts) = texts.get_mut(id) else {
        return;
    };
    let mut changed = false;
    for part in parts.parts.iter_mut().filter(|part| part.dynamic) {
        let Some((text, span)) = dynamic_part_expression(source, part) else {
            continue;
        };
        let Some(filter) = VueFilterExpr::parse_in(allocator, text, span) else {
            continue;
        };
        super::record_filters(filter, filters);
        part.text = super::wrap_source(filter);
        changed = true;
    }
    if changed {
        let rebuilt = rebuild_source(&parts.parts);
        let source = allocator.alloc_str(rebuilt.as_str());
        interp.expression = ExprRef::Opaque(allocator.alloc(OpaqueExpr {
            reason: OpaqueReason::Compound,
            source,
            span: opaque.span,
        }));
    }
    let ExprRef::Opaque(opaque) = interp.expression else {
        return;
    };
    parts.assert_compound_laws(id, interp.span, opaque.source);
}

fn dynamic_part_expression<'a>(source: &'a str, part: &TextPart) -> Option<(&'a str, Span)> {
    let raw = source.get(part.span.start as usize..part.span.end as usize)?;
    let inner_start = raw.find("{{")? + "{{".len();
    let inner_end = raw.rfind("}}")?;
    let inner = raw.get(inner_start..inner_end)?;
    let trimmed = inner.trim();
    if trimmed != part.text.as_str() {
        return None;
    }
    let leading = inner.len() - inner.trim_start().len();
    let start = part
        .span
        .start
        .saturating_add(u32::try_from(inner_start + leading).ok()?);
    let end = start.saturating_add(u32::try_from(trimmed.len()).ok()?);
    Some((trimmed, Span::new(start, end)))
}
