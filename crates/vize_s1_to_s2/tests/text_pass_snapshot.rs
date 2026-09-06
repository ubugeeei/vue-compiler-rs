//! TS-17 for the text pass (P2-9 series 4): committed fixture in,
//! pipeline out, **full normalized folio** snapshot — the P2-4 harness
//! shape applied to the installment's surface.
//!
//! The snapshot is the oracle; the walk accounting, the recorded parts
//! and the provenance trail are the targeted structural supplements
//! (assurance §4).

mod support;

use std::path::{Path, PathBuf};

use vize_davinci::assert_folio_snapshot;
use vize_davinci::folio::{Folio, FolioMode};

use support::{assert_transformed_sound, with_transformed};

fn fixture(name: &str) -> vize_s0::String {
    let path: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("text")
        .join(name);
    let text = std::fs::read_to_string(path).expect("committed fixture reads");
    vize_s0::String::from(text.as_str())
}

#[test]
fn the_merge_fixture_snapshots_the_post_pass_folio() {
    let source = fixture("merge.vue");
    with_transformed(&source, |lowered, folio, facts, budget| {
        // The oracle: the whitespace-only indentation runs are gone,
        // the greeting merged into one `opaque(compound …)` op with the
        // interior run condensed, and the comment-punched `<p>` keeps
        // its two units — "a" alone, then the compound — because a
        // comment is a run boundary.
        assert_folio_snapshot!(*folio);

        // Supplements: the model-free plan's walk accounting.
        assert_eq!(
            budget.print_to_string(FolioMode::Full).as_str(),
            "[budget-observer]\nwalks=5\npasses=5\nanalyses=0\npipelines=1\nfailures=0\n\n"
        );
        // Two compounds, five and two parts, all validated (the pass
        // count-matches the recorded table).
        let entries = facts.text_facts.sorted_entries();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].1.parts.len(), 5);
        assert_eq!(entries[1].1.parts.len(), 2);
        assert_eq!(lowered.texts.len(), 2);
        // The consumption trail, in page order.
        let records: Vec<&str> = lowered
            .provenance
            .iter()
            .filter(|record| record.rule.as_str() == "pass.text.compound")
            .map(|record| record.after.as_str())
            .collect();
        assert_eq!(
            records,
            vec!["fact static=3 dynamic=2", "fact static=1 dynamic=1"]
        );
    });
    assert_transformed_sound(&source, "merge.vue");
}

#[test]
fn the_condense_fixture_snapshots_the_post_pass_folio() {
    let source = fixture("condense.vue");
    with_transformed(&source, |lowered, folio, facts, _| {
        // The oracle: the no-newline run between the spans survives as
        // one `ui.text " "`; the newline runs between elements are
        // gone; the div's mixed text collapsed and merged with its
        // interpolation; the `<pre>` content merged **uncondensed**
        // (the shipped `is_pre_tag` exemption).
        assert_folio_snapshot!(*folio);
        assert_eq!(facts.text_facts.len(), 2);
        // The pre compound keeps its bytes verbatim in every part.
        let entries = facts.text_facts.sorted_entries();
        let pre = entries[1].1;
        assert_eq!(pre.parts[0].text.as_str(), "  keep   ");
        assert_eq!(pre.parts[2].text.as_str(), "   bytes  ");
        // The condense decisions left their records.
        assert!(
            lowered
                .provenance
                .iter()
                .any(|record| record.rule.as_str() == "condense.drop-whitespace")
        );
        assert!(
            lowered
                .provenance
                .iter()
                .any(|record| record.rule.as_str() == "condense.whitespace")
        );
    });
    assert_transformed_sound(&source, "condense.vue");
}
