//! The behavioural pin for `lower::features` and the plans it selects.
//!
//! Two claims are load-bearing and neither is checkable by reading the
//! planner, so both are asserted against real lowerings here:
//!
//! 1. **The bit is set whenever the op is built**, including by the paths
//!    that build it while *reporting an error*. An unset bit silently
//!    retires a mandatory pass — a lost fact or a lost diagnostic, not a
//!    slow compile. Every family gets a well-formed template and, where
//!    one exists, a malformed spelling that still mints the op: those are
//!    the cases a provenance-rule-name derivation got wrong.
//! 2. **Declining a pass costs no product.** For each family-free template
//!    the planned run is compared against the same artifact re-run with the
//!    features widened to every family, which forces the full table. The
//!    two must agree on the artifact, its diagnostics, its provenance and
//!    every fact table — while the pass counts differ, so the comparison
//!    cannot pass vacuously.

mod support;

use vize_davinci::diagnostic::Diagnostic;
use vize_davinci::folio::{Folio, FolioMode};
use vize_davinci::pass::BudgetObserver;
use vize_s0::Allocator;
use vize_s1::parse;
use vize_s1_to_s2::pass::{S2Facts, run_transform};
use vize_s1_to_s2::{LegacyCaps, LoweringFeatures, OpFamily, lower, lower_with_caps};
use vize_s2::folio::DisegnoFolio;

/// Controls whose absent families the planner is supposed to save walks on.
///
/// `planned_passes` includes the optional static analysis pass because DOM
/// emission consumes it when `hoist_static` is left enabled. Compound text
/// facts are lowering-published, so a compound run no longer buys a
/// transform pass.
const DECLINED_PASS_CASES: &[(&str, &str, u32)] = &[
    ("plain element", "<div class=\"a\">text</div>", 1),
    ("nested elements", "<section><p>a</p><p>b</p></section>", 1),
    ("lone interpolation", "<p>{{ msg }}</p>", 1),
    ("compound interpolation", "<p>{{ msg }} tail</p>", 1),
    (
        "bind and on",
        "<button :id=\"id\" @click=\"go\">x</button>",
        1,
    ),
    ("comment", "<div><!-- note --><span>x</span></div>", 1),
    ("show directive", "<div v-show=\"open\">x</div>", 1),
    ("html directive", "<div v-html=\"raw\"></div>", 1),
];

fn features_of(source: &str) -> LoweringFeatures {
    let allocator = Allocator::new();
    let (tree, errors) = parse(&allocator, source);
    lower(&allocator, &tree, &errors).features
}

#[test]
fn each_family_sets_exactly_its_own_bit() {
    let cases: &[(&str, &str, fn(LoweringFeatures) -> bool)] = &[
        (
            "v-if",
            "<div v-if=\"ok\">y</div>",
            LoweringFeatures::has_if_ops,
        ),
        (
            "v-else chain",
            "<div v-if=\"ok\">y</div><div v-else>n</div>",
            LoweringFeatures::has_if_ops,
        ),
        (
            "v-for",
            "<li v-for=\"item in items\">{{ item }}</li>",
            LoweringFeatures::has_for_ops,
        ),
        (
            "component",
            "<MyThing>child</MyThing>",
            LoweringFeatures::has_slot_carriers,
        ),
        (
            "slot outlet",
            "<slot name=\"head\"><b>fallback</b></slot>",
            LoweringFeatures::has_slot_carriers,
        ),
        (
            "v-slot on a template",
            "<MyThing><template #head>h</template></MyThing>",
            LoweringFeatures::has_slot_carriers,
        ),
        (
            "misplaced v-slot on a plain element",
            "<div v-slot:head>h</div>",
            LoweringFeatures::has_slot_carriers,
        ),
        (
            "v-model",
            "<input v-model=\"text\" />",
            LoweringFeatures::has_model_bindings,
        ),
        (
            "compound text",
            "<p>hello {{ name }}</p>",
            LoweringFeatures::has_text_compounds,
        ),
        // The malformed spellings are the reason the bit is set at the op
        // and not read off a provenance rule name: each of these lowers
        // under an `error.*` rule and still leaves its op in the tree, so
        // the pass that reads that op is still owed its walk.
        (
            "undecomposable v-for",
            "<p v-for=\"items\">kept</p>",
            LoweringFeatures::has_for_ops,
        ),
        (
            "v-if with no expression",
            "<p v-if>kept</p>",
            LoweringFeatures::has_if_ops,
        ),
    ];

    for (name, source, holds) in cases {
        let features = features_of(source);
        assert!(
            holds(features),
            "{name}: the lowering built the family but set no bit — \
             an op construction site is missing its `cx.observe`",
        );
    }
}

/// The boundary the positive cases cannot show: a spelling that errors
/// *without* minting the op leaves the bit clear, and should — declining
/// the pass there costs nothing, because there is no op to visit.
#[test]
fn an_erroring_spelling_that_mints_no_op_leaves_the_bit_clear() {
    // Both fall back to a plain element: the diagnostic is the
    // lowering's own and no region op survives to be visited.
    for source in ["<p v-for>kept</p>", "<p v-else>kept</p>"] {
        assert_eq!(
            features_of(source),
            LoweringFeatures::EMPTY,
            "{source} mints no region op",
        );
    }

    // The asymmetry worth knowing: `v-if` with no expression *does* mint
    // its `ui.if` (the branch is kept under the escape), so unlike the
    // two above it keeps the pass.
    assert!(features_of("<p v-if>kept</p>").has_if_ops());
}

#[test]
fn a_family_free_template_sets_no_bit() {
    for (name, source, expected_passes) in DECLINED_PASS_CASES {
        let features = features_of(source);
        if *name == "compound interpolation" {
            assert!(
                features.has_text_compounds(),
                "{name} should observe compound text facts"
            );
            assert!(!features.has_if_ops());
            assert!(!features.has_for_ops());
            assert!(!features.has_slot_carriers());
            assert!(!features.has_model_bindings());
        } else if *expected_passes == 1 {
            assert_eq!(
                features,
                LoweringFeatures::EMPTY,
                "{name} should reach the planner with no family claimed",
            );
        } else {
            unreachable!("{name} has no non-structural planned pass case");
        }
    }
}

/// The products a run is allowed to be judged by: the artifact itself,
/// every user-visible channel, and the size of each fact table.
#[derive(Debug, PartialEq, Eq)]
struct Products {
    folio: String,
    diagnostics: Vec<String>,
    provenance: usize,
    if_facts: usize,
    for_facts: usize,
    slot_facts: usize,
    text_facts: usize,
    model_faults: usize,
    static_facts: usize,
}

fn products(
    folio: &DisegnoFolio,
    diagnostics: &[Diagnostic],
    provenance: usize,
    facts: &S2Facts,
) -> Products {
    Products {
        folio: folio.print_to_string(FolioMode::Full).as_str().to_owned(),
        diagnostics: diagnostics
            .iter()
            .map(|diagnostic| format!("{diagnostic:?}"))
            .collect(),
        provenance,
        if_facts: facts.if_facts.len(),
        for_facts: facts.for_facts.len(),
        slot_facts: facts.slot_facts.len(),
        text_facts: facts.text_facts.len(),
        model_faults: facts.model_faults.len(),
        static_facts: facts.static_facts.len(),
    }
}

/// Run `source` through the pipeline, optionally widening the features
/// first so every mandatory pass is planned.
fn run(source: &str, force_every_pass: bool) -> (Products, u32) {
    let allocator = Allocator::new();
    let (tree, errors) = parse(&allocator, source);
    let mut lowered = lower_with_caps(&allocator, &tree, &errors, LegacyCaps::VUE3);
    if force_every_pass {
        lowered.features = [
            OpFamily::If,
            OpFamily::For,
            OpFamily::SlotCarrier,
            OpFamily::TextCompound,
            OpFamily::Model,
        ]
        .into_iter()
        .fold(LoweringFeatures::EMPTY, LoweringFeatures::observing);
    }
    let mut budget = BudgetObserver::new();
    let facts = run_transform(&mut lowered, &mut budget);
    let folio = DisegnoFolio::of(&lowered.root.ops);
    let provenance = lowered.provenance.len();
    (
        products(&folio, &lowered.diagnostics, provenance, &facts),
        budget.passes,
    )
}

#[test]
fn declining_a_pass_changes_no_product() {
    for (name, source, expected_planned_passes) in DECLINED_PASS_CASES {
        let (planned, planned_passes) = run(source, false);
        let (forced, forced_passes) = run(source, true);
        assert_eq!(
            planned, forced,
            "{name}: the planned run and the full table disagreed",
        );
        assert!(
            planned_passes < forced_passes,
            "{name}: the planner declined nothing ({planned_passes} of {forced_passes} passes), \
             so the comparison proves nothing",
        );
        assert_eq!(
            planned_passes, *expected_planned_passes,
            "{name}: planned pass count",
        );
        assert_eq!(forced_passes, 3, "{name}: the full table is three passes");
    }
}

/// The headline measurement, kept beside the claim it supports.
#[test]
fn a_family_free_template_plans_one_pass_not_three() {
    let (_, passes) = run("<div class=\"a\"><span>b</span></div>", false);
    assert_eq!(passes, 1);
}
