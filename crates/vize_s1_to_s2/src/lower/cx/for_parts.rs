//! `ui.for` fact attachment for the lowering context.

use vize_davinci::id::NodeId;
use vize_s0::{Span, cstr};

use super::Cx;

impl Cx<'_> {
    /// Attach a `ui.for` binding view to its op, when the op has an id.
    pub(crate) fn attach_for_parts(
        &mut self,
        node: Option<NodeId>,
        parts: super::super::forop::ForParts,
        binding_count: usize,
        span: Span,
    ) {
        if let Some(id) = node {
            assert!(
                !self
                    .for_facts
                    .iter()
                    .any(|(_, existing)| existing.tag == parts.tag),
                "hygiene law broken: ui.for {id} reuses scope tag {} - introduction sites mint fresh tags",
                parts.tag,
            );
            let before = cstr!("scope {} bindings={binding_count}", parts.tag);
            self.record(
                "lower.for-fact",
                node,
                before.as_str(),
                cstr!(
                    "fact value={} key={} index={}",
                    parts.value.spell(),
                    parts.key.spell(),
                    parts.index.spell()
                ),
                span,
            );
            self.for_facts.insert(id, parts);
        }
    }
}
