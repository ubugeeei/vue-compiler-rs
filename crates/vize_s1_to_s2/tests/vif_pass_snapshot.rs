//! TS-17 for the `v-if` pass (P2-9 series 1): committed fixture in,
//! pipeline out, **full normalized folio** snapshot — the P2-4 harness
//! shape (`crates/vize_davinci/tests/pipeline_snapshot.rs`), applied to
//! the first landed pass body.
//!
//! The snapshot is the oracle; the walk accounting, facts and
//! diagnostics are the targeted structural supplements (assurance §4).

mod support;

use std::path::{Path, PathBuf};

use vize_davinci::assert_folio_snapshot;
use vize_davinci::folio::{Folio, FolioMode};
use vize_s1_to_s2::pass::{BranchKeyKind, vif};

use support::{assert_transformed_sound, with_transformed};

fn fixture(name: &str) -> vize_s0::String {
    let path: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("vif")
        .join(name);
    let text = std::fs::read_to_string(path).expect("committed fixture reads");
    vize_s0::String::from(text.as_str())
}

#[test]
fn the_chain_fixture_snapshots_the_post_pass_folio() {
    let source = fixture("chain-keys.vue");
    with_transformed(&source, |lowered, folio, facts, budget| {
        // The oracle: the full normalized folio after the pipeline ran.
        // The `story` and `fallback` keys have left the attribute
        // surface; the iterated `img` keeps its `key` (the branch root
        // is the `ui.for`); the unwrapped template branch never had a
        // carrier op for the wrapper's attributes.
        assert_folio_snapshot!(*folio);

        // Supplements: the model-free plan's walk accounting through
        // the budget observer's own derived page.
        assert_eq!(
            budget.print_to_string(FolioMode::Full).as_str(),
            "[budget-observer]\nwalks=5\npasses=5\nanalyses=0\npipelines=1\nfailures=0\n\n"
        );
        // One fact entry (the chain's `ui.if`), keys on branches 0 and 2.
        let entries = facts.if_facts.sorted_entries();
        assert_eq!(entries.len(), 1);
        let branches = &entries[0].1.branches;
        assert_eq!(branches.len(), 3);
        assert_eq!(
            branches
                .iter()
                .map(|branch| branch.as_ref().map(|key| key.kind.clone()))
                .collect::<Vec<_>>(),
            vec![
                Some(BranchKeyKind::Static(Some("story".into()))),
                None,
                Some(BranchKeyKind::Static(Some("fallback".into()))),
            ]
        );
        // Distinct keys: no diagnostics beyond the lowering's own.
        assert_eq!(lowered.diagnostics, vec![]);
    });
    assert_transformed_sound(&source, "chain-keys.vue");
}

#[test]
fn the_collision_fixture_snapshots_the_post_pass_folio() {
    let source = fixture("collision.vue");
    with_transformed(&source, |lowered, folio, _, _| {
        // The folio is clean - all three keys extracted - while the
        // duplicate-key errors ride the diagnostics channel.
        assert_folio_snapshot!(*folio);
        assert_eq!(lowered.diagnostics.len(), 2);
        for diagnostic in &lowered.diagnostics {
            assert_eq!(diagnostic.message.as_str(), vif::SAME_KEY_MESSAGE);
        }
    });
    assert_transformed_sound(&source, "collision.vue");
}
