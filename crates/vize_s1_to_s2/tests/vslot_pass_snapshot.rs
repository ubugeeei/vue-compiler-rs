//! TS-17 for the `v-slot` pass (P2-9 series 3): committed fixture in,
//! pipeline out, **full normalized folio** snapshot — the P2-4 harness
//! shape, as the vif and vfor snapshots applied it.
//!
//! The snapshot is the oracle; the walk accounting, the grouping facts
//! and the diagnostics are the targeted structural supplements
//! (assurance §4). Since the v-slot pass preserves the tree, what the
//! snapshots pin is the *authored `ui.slot-content` surface surviving
//! the pipeline untouched* — name position, modifiers, params exactly
//! as lowered — while the canonical grouping (default-name synthesis
//! included) shows only in the facts and provenance beside it.

mod support;

use std::path::{Path, PathBuf};

use vize_davinci::assert_folio_snapshot;
use vize_davinci::folio::{Folio, FolioMode};
use vize_s1_to_s2::pass::vslot::{SlotCarrier, SlotName, SlotParams};
use vize_s2::scope::ScopeOrigin;

use support::{assert_transformed_sound, with_transformed};

fn fixture(name: &str) -> vize_s0::String {
    let path: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("vslot")
        .join(name);
    let text = std::fs::read_to_string(path).expect("committed fixture reads");
    vize_s0::String::from(text.as_str())
}

#[test]
fn the_groups_fixture_snapshots_the_post_pass_folio() {
    let source = fixture("groups.vue");
    with_transformed(&source, |lowered, folio, facts, budget| {
        // The oracle: the full normalized folio after the pipeline ran.
        assert_folio_snapshot!(*folio);

        // Supplements: the model-free plan's walk accounting through
        // the budget observer's own derived page.
        assert_eq!(
            budget.print_to_string(FolioMode::Full).as_str(),
            "[budget-observer]\nwalks=5\npasses=5\nanalyses=0\npipelines=1\nfailures=0\n\n"
        );
        // Three components grouped in document order: Card (pattern
        // params slot, modifier-folded slot, implicit default), Panel
        // (the bare v-slot's synthesized default name), List (the
        // dynamic group alone — filler never synthesizes a default).
        let entries = facts.slot_facts.sorted_entries();
        assert_eq!(entries.len(), 3);
        let card = &entries[0].1.groups;
        assert_eq!(
            card.iter()
                .map(|group| group.name.text())
                .collect::<Vec<_>>(),
            vec!["head", "body.raw", "default"]
        );
        assert!(matches!(card[2].carrier, SlotCarrier::Implicit));
        let panel = &entries[1].1.groups;
        assert_eq!(panel.len(), 1);
        assert!(matches!(
            &panel[0].name,
            SlotName::Static {
                origin: ScopeOrigin::Synthesized { .. },
                ..
            }
        ));
        assert!(matches!(&panel[0].params, SlotParams::Scoped { .. }));
        let list = &entries[2].1.groups;
        assert_eq!(list.len(), 1);
        assert!(matches!(&list[0].name, SlotName::Dynamic { .. }));
        // Every slot scope consumed with a fresh tag, in page order —
        // the pattern position enumerates no names (#4365), the rest
        // one each.
        let consumed: Vec<(&str, &str)> = lowered
            .provenance
            .iter()
            .filter(|record| record.rule.as_str() == "pass.v-slot.scope")
            .map(|record| (record.before.as_str(), record.after.as_str()))
            .collect();
        assert_eq!(
            consumed,
            vec![
                ("scope #0 bindings=0", "fact params=?"),
                ("scope #1 bindings=1", "fact params=row"),
                ("scope #2 bindings=1", "fact params=props"),
                ("scope #3 bindings=1", "fact params=value"),
            ]
        );
        // All four groups' spellings are valid Vue: no diagnostics.
        assert_eq!(lowered.diagnostics, vec![]);
    });
    assert_transformed_sound(&source, "groups.vue");
}

#[test]
fn the_invalid_fixture_snapshots_the_post_pass_folio() {
    let source = fixture("invalid.vue");
    with_transformed(&source, |lowered, folio, facts, _| {
        // The tree survives every error untouched (kept fragments, not
        // rollback), the four diagnostics ride the unified channel.
        assert_folio_snapshot!(*folio);
        let messages: Vec<&str> = lowered
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect();
        assert_eq!(
            messages,
            vec![
                "v-slot can only be used on components or <template> tags.",
                "Mixed v-slot usage with named slots detected.",
                "Duplicate slot names detected.",
                "Extraneous children found when component already has an explicit default slot.",
            ]
        );
        // Grouping still published: Modal keeps both sides of the mixed
        // usage (the same rule the differential projection applies to
        // both lanes), Grid drops the duplicate silently and keeps its
        // authored default.
        let entries = facts.slot_facts.sorted_entries();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].1.groups.len(), 2, "Modal: own + late");
        assert_eq!(
            entries[1]
                .1
                .groups
                .iter()
                .map(|group| group.name.text())
                .collect::<Vec<_>>(),
            vec!["cell", "default"],
            "Grid: duplicate dropped, authored default kept"
        );
    });
    assert_transformed_sound(&source, "invalid.vue");
}
