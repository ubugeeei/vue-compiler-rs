//! Props passed from a child component to its own `<slot>` outlets.
//!
//! `<slot>` is a Vue built-in rather than a normal component usage, but
//! `defineSlots` still provides a contextual prop type for outlet bindings.
//! Emitting those bindings as bare template expressions loses callback
//! parameter types and creates Vize-only `TS7006`.

mod collect;
mod emit;

use vize_carton::{CompactString, FxHashMap, FxHashSet, String, cstr};
use vize_croquis::{
    Croquis, ScopeData, ScopeKind,
    croquis::{PassedProp, SpreadProp},
};
use vize_relief::RootNode;

use crate::virtual_ts::generator::generics::references_any_identifier;
use crate::virtual_ts::props::extract_generic_names;
use std::ops::Range;

pub(super) use emit::generate_scope_slot_outlet_checks;

pub(super) struct SlotOutlet {
    scope_id: u32,
    name: CompactString,
    name_is_dynamic: bool,
    name_source_range: Option<std::ops::Range<u32>>,
    start: u32,
    vif_guard: Option<CompactString>,
    props: Vec<PassedProp>,
    spread_props: Vec<SpreadProp>,
    event_handler_ranges: Vec<Range<u32>>,
}

/// The `<slot>` outlets of one component, grouped by the scope that generates
/// them, together with the single slots type reference every outlet payload is
/// resolved through. Both are derived once per component so the AST walk and
/// the type-reference decision are not repeated per scope.
#[derive(Default)]
pub(super) struct SlotOutletChecks {
    by_scope: FxHashMap<u32, Vec<SlotOutlet>>,
    slots_type: String,
}

impl SlotOutletChecks {
    pub(super) fn collect(summary: &Croquis, root: Option<&RootNode<'_>>) -> Self {
        Self {
            by_scope: collect::collect_slot_outlets_by_scope(summary, root),
            slots_type: slots_type_ref(summary),
        }
    }

    pub(super) fn emit_helpers(&self, ts: &mut String) {
        emit::emit_slot_outlet_helpers(ts, &self.by_scope);
    }

    /// Authored ranges of the outlet bindings, which the generated outlet
    /// literals already type; emitting them again as bare statements would
    /// drop that contextual type.
    pub(super) fn expression_ranges(&self, summary: &Croquis) -> FxHashSet<(u32, u32)> {
        collect::slot_outlet_expression_ranges(summary, &self.by_scope)
    }

    pub(super) fn covers_event_handler_scope(&self, start: u32, end: u32) -> bool {
        self.by_scope.values().flatten().any(|outlet| {
            outlet
                .event_handler_ranges
                .iter()
                .any(|range| range.start == start && range.end == end)
        })
    }
}

/// The type `<slot>` outlet payloads are resolved through. A generic SFC whose
/// `defineSlots` type references its type parameters exports `Slots` with those
/// parameters re-declared, so the bare alias would collapse to their defaults
/// (#3065); instantiate it with the SFC's own parameters instead.
fn slots_type_ref(summary: &Croquis) -> String {
    let Some(define_slots) = summary.macros.define_slots() else {
        return String::from("Slots");
    };
    if summary.bindings.bindings.contains_key("slots") {
        return String::from("typeof slots");
    }
    let Some(generic_decl) = sfc_generic_param(summary) else {
        return String::from("Slots");
    };
    let generic_names = extract_generic_names(generic_decl);
    let names: Vec<String> = generic_names
        .split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(String::from)
        .collect();
    let slots_type = define_slots.type_args.as_deref().unwrap_or_default();
    if references_any_identifier(slots_type, &names) {
        cstr!("Slots<{generic_names}>")
    } else {
        String::from("Slots")
    }
}

/// The SFC's `<script setup generic="…">` parameter list.
fn sfc_generic_param(summary: &Croquis) -> Option<&str> {
    summary
        .scopes
        .iter()
        .find(|scope| matches!(scope.kind, ScopeKind::ScriptSetup))
        .and_then(|scope| match scope.data() {
            ScopeData::ScriptSetup(data) => data.generic.as_ref().map(|generic| generic.as_str()),
            _ => None,
        })
}
