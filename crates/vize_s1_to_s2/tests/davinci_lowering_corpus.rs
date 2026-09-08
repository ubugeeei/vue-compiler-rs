//! Davinci P2-8 corpus-runnable entry — the P1-6/P1-7 lane shape.
//!
//! Compiled only with the `davinci-differential` feature (`[[test]]
//! required-features` in Cargo.toml). Runs the committed battery with its
//! exact-pinned scope and census (the same numbers the plain suite pins,
//! so a feature-wiring regression fails loudly in both lanes), then,
//! with `VIZE_DAVINCI_DIFFERENTIAL_CORPUS=<dir>`, additionally sweeps
//! every `.vue` file under `<dir>`: the **whole file's bytes** go
//! through the S1 parser (SFC block markup as markup, script/style as
//! raw text — the TS-19 lane's framing) and then through [`lower`],
//! asserting the full soundness oracle per file: id accounting,
//! Canonical-rigor verification, side-table resolution, and the folio
//! round-trip. Every file lowers or diagnoses; none may panic. The
//! canonical fixture root fails closed unless its submodule inventory
//! reconciles; other roots sweep in smoke scope with
//! `closure_evidence=false` (see `davinci_test_support::corpus`).
//!
//! Run:
//!
//! ```text
//! VIZE_DAVINCI_DIFFERENTIAL_CORPUS=tests/_fixtures/_git \
//!     cargo test -p vize_s1_to_s2 --features davinci-differential \
//!     --test davinci_lowering_corpus -- --nocapture
//! ```

mod support;

use std::fs;

use davinci_test_support::surface_fixture as battery;
use support::{assert_sound, with_lowered};

#[test]
fn lowering_corpus_is_total() {
    // -- committed battery, exact-pinned scope and census --------------
    assert_eq!(battery::WELL_FORMED.len(), 16);
    assert_eq!(battery::MALFORMED.len(), 26);
    let mut ops = 0u64;
    let mut diagnostics = 0usize;
    let mut provenance = 0usize;
    let mut scopes = 0usize;
    for fixture in battery::WELL_FORMED.iter().chain(battery::MALFORMED) {
        assert_sound(fixture.source, fixture.name);
        with_lowered(fixture.source, |lowered, _folio| {
            ops += u64::from(lowered.op_count);
            diagnostics += lowered.diagnostics.len();
            provenance += lowered.provenance.len();
            scopes += lowered.scopes.len();
        });
    }
    // Re-pinned when compound text facts became lowering-published:
    // four committed battery fixtures now record `lower.text-fact`
    // provenance before the transform table.
    assert_eq!(
        (ops, diagnostics, provenance, scopes),
        (83, 28, 105, 1),
        "battery census moved: re-pin in both lanes deliberately"
    );

    // -- optional corpus sweep -----------------------------------------
    let Some(sweep) = davinci_test_support::corpus::resolve_env_sweep() else {
        eprintln!("VIZE_DAVINCI_DIFFERENTIAL_CORPUS unset: committed battery only");
        return;
    };
    let files = &sweep.files;
    assert!(
        !files.is_empty(),
        "corpus sweep found no .vue files under {}",
        sweep.root.display()
    );
    let mut checked = 0u64;
    let mut with_diagnostics = 0u64;
    for file in files {
        let Ok(source) = fs::read_to_string(file) else {
            continue;
        };
        let context = file.to_string_lossy();
        assert_sound(&source, context.as_ref());
        with_lowered(&source, |lowered, _folio| {
            if !lowered.diagnostics.is_empty() {
                with_diagnostics += 1;
            }
        });
        checked += 1;
    }
    eprintln!(
        "davinci lowering corpus sweep: files={} checked={} with_diagnostics={}",
        files.len(),
        checked,
        with_diagnostics
    );
}
