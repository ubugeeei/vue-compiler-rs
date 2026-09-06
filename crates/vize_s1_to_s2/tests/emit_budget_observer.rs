//! P2-12b groundwork: S2 DOM emission exposes a walk budget without
//! changing the shipped-compatible render output.

#![allow(clippy::disallowed_macros, clippy::disallowed_types)]

use std::path::Path;

use davinci_harness::fixtures::{LADDER, template_block};
use vize_s0::Allocator;
use vize_s1_to_s2::{emit_dom_source, emit_dom_source_observed};

#[derive(Debug, Clone, Copy)]
struct TraversalBudget {
    walks: u32,
    visits: u32,
}

/// fixture name -> current S2 DOM emit-only walks and op visits.
const S2_DOM_EMIT_COUNTS: [(&str, u32, u32); 6] = [
    ("small", 1, 5),
    ("medium", 1, 33),
    ("large", 1, 54),
    ("stress-deep", 1, 72),
    ("stress-wide", 1, 2),
    ("stress-interp", 1, 201),
];

#[test]
fn observed_dom_emit_keeps_output_and_walk_budget() {
    let fused_walk_target = phase_2_dom_walk_target();
    for fixture in &LADDER {
        let template =
            template_block(fixture.source).expect("every ladder fixture has a template block");
        let observed_allocator = Allocator::new();
        let plain_allocator = Allocator::new();
        let observed = emit_dom_source_observed(&observed_allocator, template)
            .unwrap_or_else(|error| panic!("{} observed emit failed: {error:?}", fixture.name));
        let plain = emit_dom_source(&plain_allocator, template)
            .unwrap_or_else(|error| panic!("{} plain emit failed: {error:?}", fixture.name));
        let baseline = traversal_budget(fixture.name);
        let expected = s2_dom_emit_count(fixture.name);

        assert_eq!(
            observed.emit.assembled(),
            plain.assembled(),
            "{} observed emit must not change render output",
            fixture.name
        );
        assert_eq!(
            observed.budget.transform.walks, 5,
            "{} transform walks",
            fixture.name
        );
        assert_eq!(
            observed.budget.transform.passes, 5,
            "{} transform passes",
            fixture.name
        );
        assert_eq!(
            observed.budget.transform.pipelines, 1,
            "{} transform pipelines",
            fixture.name
        );
        assert_eq!(
            observed.budget.transform.failures, 0,
            "{} transform failures",
            fixture.name
        );
        assert_eq!(
            observed.budget.emit_walks, expected.walks,
            "{} S2 DOM emit walks",
            fixture.name
        );
        assert_eq!(
            observed.budget.emit_visits, expected.visits,
            "{} S2 DOM emit visits",
            fixture.name
        );
        assert!(
            observed.budget.emit_visits > 0,
            "{} emit must visit at least one op",
            fixture.name
        );
        assert!(
            observed.budget.emit_walks <= baseline.walks,
            "{} emit walks {} exceed P2-12a baseline {}",
            fixture.name,
            observed.budget.emit_walks,
            baseline.walks
        );
        assert!(
            observed.budget.emit_visits <= baseline.visits,
            "{} emit visits {} exceed P2-12a baseline {}",
            fixture.name,
            observed.budget.emit_visits,
            baseline.visits
        );
        assert!(
            observed.budget.total_walks() > fused_walk_target,
            "{} total walks {} already meet the phase-2 fused DOM target {} before the build path has switched",
            fixture.name,
            observed.budget.total_walks(),
            fused_walk_target
        );
        println!(
            "davinci.s2_dom.walk {} emit_walks={} emit_visits={} transform_walks={} transform_passes={} total_walks={} fused_walk_target={} baseline_walks={} baseline_visits={}",
            fixture.name,
            observed.budget.emit_walks,
            observed.budget.emit_visits,
            observed.budget.transform.walks,
            observed.budget.transform.passes,
            observed.budget.total_walks(),
            fused_walk_target,
            baseline.walks,
            baseline.visits
        );
    }
}

#[test]
fn model_bindings_keep_the_model_diagnostic_pass_in_the_emit_budget() {
    let observed_allocator = Allocator::new();
    let plain_allocator = Allocator::new();
    let source = r#"<input v-model="msg">"#;
    let observed = emit_dom_source_observed(&observed_allocator, source)
        .expect("observed model emit succeeds");
    let plain = emit_dom_source(&plain_allocator, source).expect("plain model emit succeeds");

    assert_eq!(
        observed.emit.assembled(),
        plain.assembled(),
        "the profiling observer must not change model output"
    );
    assert_eq!(observed.budget.transform.walks, 6);
    assert_eq!(observed.budget.transform.passes, 6);
    assert_eq!(observed.budget.emit_walks, 1);
    assert_eq!(observed.budget.total_walks(), 7);
}

fn s2_dom_emit_count(fixture: &str) -> TraversalBudget {
    S2_DOM_EMIT_COUNTS
        .iter()
        .find(|(name, _, _)| *name == fixture)
        .map(|(_, walks, visits)| TraversalBudget {
            walks: *walks,
            visits: *visits,
        })
        .unwrap_or_else(|| panic!("{fixture} has no pinned S2 DOM emit count"))
}

fn traversal_budget(fixture: &str) -> TraversalBudget {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate directory")
        .parent()
        .expect("repo root");
    let text = std::fs::read_to_string(repo.join("davinci-road/plan/budgets.toml"))
        .expect("budgets.toml reads");
    let value: toml::Value = toml::from_str(&text).expect("budgets.toml parses");
    let id = format!("dom_{fixture}");
    let entry = value
        .get("traversal")
        .and_then(|traversal| traversal.get(&id))
        .unwrap_or_else(|| panic!("budgets.toml [traversal] is missing {id}"));
    TraversalBudget {
        walks: required_u32(entry, &id, "walks"),
        visits: required_u32(entry, &id, "visits"),
    }
}

fn phase_2_dom_walk_target() -> u32 {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate directory")
        .parent()
        .expect("repo root");
    let text = std::fs::read_to_string(repo.join("davinci-road/plan/budgets.toml"))
        .expect("budgets.toml reads");
    let value: toml::Value = toml::from_str(&text).expect("budgets.toml parses");
    let entry = value
        .get("target")
        .and_then(|target| target.get("phase-2"))
        .expect("budgets.toml has [target.phase-2]");
    required_u32(entry, "target.phase-2", "dom_walks_max")
}

fn required_u32(entry: &toml::Value, id: &str, field: &str) -> u32 {
    entry
        .get(field)
        .and_then(toml::Value::as_integer)
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or_else(|| panic!("budgets.toml [traversal.{id}] has no u32 {field}"))
}
