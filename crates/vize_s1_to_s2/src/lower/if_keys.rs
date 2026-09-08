//! Lowering-published `v-if` branch-key facts.
//!
//! The old `v-if` transform walk only lifted branch-carrier `key` props
//! into semantic facts and reported duplicate keys. The lowering already
//! owns the chain, branch order, wrapper captures, and page-order id, so
//! the same decision can be made while the branch body is still hot.

use alloc::vec::Vec as StdVec;

use vize_davinci::id::NodeId;
use vize_s0::{Span, String, Vec, cstr};
use vize_s2::expr::ExprRef;
use vize_s2::op::{Attribute, BindingOp, DynamicName, Op};

use super::cx::Cx;
use super::structural::WrapperKey;

/// The duplicate-key message, byte-identical to relief's
/// `ErrorCode::VIfSameKey` text so the two channels never drift on
/// wording.
pub const SAME_KEY_MESSAGE: &str = "v-if/v-else-if branches must use unique keys.";

/// One branch's extracted `key`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchKey {
    /// Which spelling carried the key.
    pub kind: BranchKeyKind,
    /// The authored attribute's range.
    pub span: Span,
}

/// The two key spellings a branch carrier can author.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BranchKeyKind {
    /// A static `key` attribute; `None` for a bare `key`, which never
    /// collides.
    Static(Option<String>),
    /// A `:key` binding: the trimmed authored value text, plus the
    /// index of the carrier binding that carries it (`None` for a
    /// wrapper-captured key, which has no op).
    Dynamic {
        /// The trimmed value text (the parser's same-name expansion
        /// applied: a valueless `:key` reads `key`).
        source: String,
        /// The carrier's binding index, for the surface exclusion.
        bind_index: Option<usize>,
    },
}

impl BranchKey {
    /// The text the collision check compares, kind-blind, exactly like
    /// the legacy `extract_key_value_str` under the default dialect.
    #[must_use]
    pub fn collision_text(&self) -> Option<&str> {
        match &self.kind {
            BranchKeyKind::Static(value) => value.as_deref(),
            BranchKeyKind::Dynamic { source, .. } if source.is_empty() => None,
            BranchKeyKind::Dynamic { source, .. } => Some(source.as_str()),
        }
    }
}

/// Per-`ui.if` branch-key facts, one slot per branch in authored order.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct IfFacts {
    /// `branches[i]` is branch `i`'s extracted key, when it had one.
    pub branches: StdVec<Option<BranchKey>>,
}

/// Facts cross compile boundaries with their artifact (P1-11).
const _: () = {
    const fn assert_owned<T: 'static>() {}
    assert_owned::<BranchKey>();
    assert_owned::<BranchKeyKind>();
    assert_owned::<IfFacts>();
};

/// 64-bit footprints, guarded like every node-size assert (the wasm32
/// lane is 32-bit).
#[cfg(target_pointer_width = "64")]
const _: () = {
    assert!(core::mem::size_of::<BranchKey>() == 48);
    assert!(core::mem::size_of::<IfFacts>() == 24);
};

/// Convert a captured `<template v-if>` wrapper key into the branch-key
/// fact that emit and parity consumers read.
#[must_use]
pub(crate) fn from_wrapper_key(key: &WrapperKey) -> BranchKey {
    match key {
        WrapperKey::Static { value, span } => BranchKey {
            kind: BranchKeyKind::Static(value.clone()),
            span: *span,
        },
        WrapperKey::Dynamic { source, span } => BranchKey {
            kind: BranchKeyKind::Dynamic {
                source: source.clone(),
                bind_index: None,
            },
            span: *span,
        },
    }
}

/// Attach the lowering-published branch-key facts to their `ui.if` op,
/// recording the extraction provenance and duplicate-key diagnostics
/// beside the artifact.
pub(crate) fn attach_if_facts(
    cx: &mut Cx<'_>,
    node: Option<NodeId>,
    keys: StdVec<Option<BranchKey>>,
) {
    for (index, key) in keys.iter().enumerate() {
        if let Some(key) = key {
            cx.record(
                "lower.if-branch-key",
                node,
                key_spelling(key).as_str(),
                cstr!("fact key branch={index}"),
                key.span,
            );
        }
    }

    for index in 1..keys.len() {
        let Some(later) = &keys[index] else {
            continue;
        };
        let Some(text) = later.collision_text() else {
            continue;
        };
        let collides = keys[..index].iter().any(|earlier| {
            earlier
                .as_ref()
                .and_then(BranchKey::collision_text)
                .is_some_and(|existing| existing == text)
        });
        if collides {
            cx.error(later.span, String::from(SAME_KEY_MESSAGE));
            cx.record(
                "error.v-if-same-key",
                node,
                key_spelling(later).as_str(),
                String::default(),
                later.span,
            );
        }
    }

    if keys.iter().any(Option::is_some)
        && let Some(id) = node
    {
        cx.if_facts.insert(id, IfFacts { branches: keys });
    }
}

/// The attribute/binding surface of a branch's carrier op: the branch
/// region's single root when it is an element, component, or slot outlet
/// and it is the branch's own carrier (`op.span == branch_span`).
fn carrier_surface<'w, 'a>(
    branch_span: Span,
    ops: &'w mut [Op<'a>],
) -> Option<(
    &'w mut Vec<'a, Attribute<'a>>,
    &'w mut Vec<'a, BindingOp<'a>>,
)> {
    let [op] = ops else {
        return None;
    };
    match op {
        Op::Element(element) if element.span == branch_span => {
            let element = &mut **element;
            Some((&mut element.attributes, &mut element.bindings))
        }
        Op::Component(component) if component.span == branch_span => {
            let component = &mut **component;
            Some((&mut component.attributes, &mut component.bindings))
        }
        Op::Slot(slot) if slot.span == branch_span => {
            let slot = &mut **slot;
            Some((&mut slot.attributes, &mut slot.bindings))
        }
        Op::Element(_)
        | Op::Component(_)
        | Op::Slot(_)
        | Op::Text(_)
        | Op::Comment(_)
        | Op::Interpolation(_)
        | Op::If(_)
        | Op::For(_) => None,
    }
}

/// A `ui.bind` whose argument spells `key` by static name or legacy
/// dynamic-argument content.
fn is_key_bind(binding: &BindingOp<'_>) -> bool {
    match binding {
        BindingOp::Bind(bind) => match bind.name {
            Some(DynamicName::Static("key")) => true,
            Some(DynamicName::Dynamic(ExprRef::Js(js))) => js.source == "key",
            _ => false,
        },
        BindingOp::On(_)
        | BindingOp::Model(_)
        | BindingOp::SlotContent(_)
        | BindingOp::VueDirective(_)
        | BindingOp::VueCssBind(_)
        | BindingOp::VueSync(_)
        | BindingOp::VueSlotScope(_)
        | BindingOp::VueOnce(_)
        | BindingOp::VueMemo(_)
        | BindingOp::VueShow(_)
        | BindingOp::VueHtml(_)
        | BindingOp::VueText(_)
        | BindingOp::VueCloak(_) => false,
    }
}

/// Extract the branch key off the carrier surface, when one is authored:
/// the first key spelling in authored (span) order.
pub(crate) fn take_carrier_key(branch_span: Span, ops: &mut [Op<'_>]) -> Option<BranchKey> {
    let (attributes, bindings) = carrier_surface(branch_span, ops)?;
    let static_at = attributes
        .iter()
        .position(|attribute| attribute.name == "key")
        .map(|index| (attributes[index].span.start, index));
    let dynamic_at = bindings
        .iter()
        .position(is_key_bind)
        .map(|index| (binding_span(&bindings[index]).start, index));
    match (static_at, dynamic_at) {
        (Some((attr_start, index)), Some((bind_start, _))) if attr_start < bind_start => {
            Some(take_static(attributes, index))
        }
        (Some((_, index)), None) => Some(take_static(attributes, index)),
        (_, Some((_, index))) => Some(read_dynamic(bindings, index)),
        (None, None) => None,
    }
}

/// The provenance `before` spelling of one extracted key.
pub(crate) fn key_spelling(key: &BranchKey) -> String {
    match &key.kind {
        BranchKeyKind::Static(Some(value)) => cstr!("key=\"{value}\""),
        BranchKeyKind::Static(None) => String::from("key"),
        BranchKeyKind::Dynamic { source, .. } => cstr!(":key=\"{source}\""),
    }
}

fn take_static<'a>(attributes: &mut Vec<'a, Attribute<'a>>, index: usize) -> BranchKey {
    let attribute = attributes.remove(index);
    BranchKey {
        kind: BranchKeyKind::Static(attribute.value.map(String::from)),
        span: attribute.span,
    }
}

fn read_dynamic<'a>(bindings: &[BindingOp<'a>], index: usize) -> BranchKey {
    let BindingOp::Bind(bind) = &bindings[index] else {
        unreachable!("is_key_bind admitted only ui.bind")
    };
    let source = bind
        .value
        .as_ref()
        .map(|value| String::from(value.source()))
        .unwrap_or_default();
    BranchKey {
        kind: BranchKeyKind::Dynamic {
            source,
            bind_index: Some(index),
        },
        span: bind.span,
    }
}

fn binding_span(binding: &BindingOp<'_>) -> Span {
    match binding {
        BindingOp::Bind(bind) => bind.span,
        BindingOp::On(on) => on.span,
        BindingOp::Model(model) => model.span,
        BindingOp::SlotContent(content) => content.span,
        BindingOp::VueDirective(directive) => directive.span,
        BindingOp::VueCssBind(bind) => bind.span,
        BindingOp::VueSync(sync) => sync.span,
        BindingOp::VueSlotScope(scope) => scope.span,
        BindingOp::VueOnce(once) => once.span,
        BindingOp::VueMemo(memo) => memo.span,
        BindingOp::VueShow(show) => show.span,
        BindingOp::VueHtml(html) => html.span,
        BindingOp::VueText(text) => text.span,
        BindingOp::VueCloak(cloak) => cloak.span,
    }
}
