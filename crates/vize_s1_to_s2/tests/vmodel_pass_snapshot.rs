//! TS-17 for the element/binding family (P2-9 series 5): committed
//! fixture in, pipeline out, **full normalized folio** snapshot — the
//! P2-4 harness shape. The snapshots show the surface this installment
//! landed: `ui.bind`/`ui.on` lines with the parser shorthands applied,
//! the outlet's props surface, and the model contract standing beside
//! them; the fault table, diagnostics, and walk accounting are the
//! structural supplements (assurance §4).

mod support;

use std::path::{Path, PathBuf};

use vize_davinci::assert_folio_snapshot;
use vize_davinci::folio::{Folio, FolioMode};
use vize_s1_to_s2::pass::{ModelFault, vmodel, vslot};

use support::{assert_transformed_sound, with_transformed};

fn fixture(name: &str) -> vize_s0::String {
    let path: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("vmodel")
        .join(name);
    let text = std::fs::read_to_string(path).expect("committed fixture reads");
    vize_s0::String::from(text.as_str())
}

#[test]
fn the_bindings_fixture_snapshots_the_post_pass_folio() {
    let source = fixture("bindings.vue");
    with_transformed(&source, |lowered, folio, facts, budget| {
        // The oracle: the full normalized folio after the pipeline ran.
        // The `.value` dot shorthand carries its synthesized `prop`
        // modifier, the valueless `:model-value` its camelized same-name
        // value, the outlet its props surface, and the two `ui.model`
        // contracts stand unexpanded.
        assert_folio_snapshot!(*folio);

        // Supplements: the model pass and the slot carriers' pass are
        // both bought by this fixture; `v-if` and `v-for` are not, so the
        // plan is three walks: two barriers plus the fusable analysis
        // singleton.
        assert_eq!(
            budget.print_to_string(FolioMode::Full).as_str(),
            "[budget-observer]\nwalks=3\npasses=3\nanalyses=0\npipelines=1\nfailures=0\n\n"
        );
        // Both models are valid: the fault table is empty, and the only
        // diagnostics are none at all.
        assert!(facts.model_faults.is_empty());
        assert_eq!(lowered.diagnostics, vec![]);
    });
    assert_transformed_sound(&source, "bindings.vue");
}

#[test]
fn the_invalid_fixture_snapshots_the_post_pass_folio() {
    let source = fixture("invalid.vue");
    with_transformed(&source, |lowered, folio, facts, _| {
        // The folio keeps every op — the S2 lane never removes an
        // invalid model (the fault table is the legacy removal's
        // preserving twin) — while the errors ride the channel.
        assert_folio_snapshot!(*folio);

        let messages: Vec<&str> = lowered
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect();
        assert_eq!(
            messages,
            vec![
                // The v-slot pass fires first (pipeline order): the
                // outlet-misplaced diagnostic this installment routed.
                vslot::MISPLACED_MESSAGE,
                // Then the model pass, in page order.
                vmodel::ON_SCOPE_MESSAGE,
                vmodel::ARG_ON_ELEMENT_MESSAGE,
                vmodel::ON_SCOPE_MESSAGE,
            ]
        );
        let faults: Vec<ModelFault> = facts
            .model_faults
            .sorted_entries()
            .into_iter()
            .map(|(_, fact)| fact.fault)
            .collect();
        assert_eq!(
            faults,
            vec![
                ModelFault::OnScope,
                ModelFault::ArgOnElement,
                ModelFault::OnScope,
            ]
        );
    });
    assert_transformed_sound(&source, "invalid.vue");
}
