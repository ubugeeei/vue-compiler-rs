//! P2-12b build-path walk witness for profiled source-map-free DOM compiles.

#![allow(clippy::disallowed_macros, clippy::disallowed_types)]

use std::path::Path;

use davinci_harness::fixtures::{LADDER, template_block};
use vize_atelier_dom::compile_template;
use vize_s0::Allocator;
use vize_s0::profiler::{CounterSummary, global_profiler};

static PROFILER_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[derive(Debug, Clone, Copy)]
struct TraversalBudget {
    walks: u64,
    visits: u64,
}

const CURRENT_S2_OBSERVER_WALKS: u64 = 7;
const CURRENT_BUILD_WALKS: u64 = 8;

#[test]
fn profile_build_walks_report_the_current_p2_12b_gap() {
    let _guard = lock_profiler();
    let fused_walk_target = phase_2_dom_walk_target();
    assert_eq!(
        fused_walk_target, 1,
        "P2-12a pins the phase-2 DOM fused-walk target"
    );

    for fixture in &LADDER {
        let template =
            template_block(fixture.source).expect("every ladder fixture has a template block");
        let baseline = traversal_budget(fixture.name);
        let profile = ProfileScope::enable();
        let allocator = Allocator::new();
        let (_, errors, result) = compile_template(&allocator, template);
        let counters = profile.finish();

        assert!(errors.is_empty(), "{} should compile cleanly", fixture.name);
        assert!(!result.code.is_empty(), "{} should emit code", fixture.name);

        let pre_s2_walks = counter(&counters, "davinci.s2_dom.pre_s2.walks");
        let pre_s2_visits = counter(&counters, "davinci.s2_dom.pre_s2.visits");
        let s2_walks = counter(&counters, "davinci.s2_dom.total.walks");
        let emit_walks = counter(&counters, "davinci.s2_dom.emit.walks");
        let emit_visits = counter(&counters, "davinci.s2_dom.emit.visits");
        let build_walks = counter(&counters, "davinci.s2_dom.build.walks");

        assert_eq!(
            pre_s2_walks, 1,
            "{} has one pre-S2 template walk",
            fixture.name
        );
        assert!(
            pre_s2_walks < baseline.walks && pre_s2_visits <= baseline.visits,
            "{} pre-S2 walk stayed under the P2-12a DOM ceiling",
            fixture.name
        );
        assert_eq!(
            emit_walks, fused_walk_target,
            "{} emit walk target",
            fixture.name
        );
        assert!(
            emit_visits <= baseline.visits,
            "{} emit visits stayed under the P2-12a DOM ceiling",
            fixture.name
        );
        assert_eq!(
            s2_walks, CURRENT_S2_OBSERVER_WALKS,
            "{} current S2 observer walk count",
            fixture.name
        );
        assert_eq!(
            build_walks,
            pre_s2_walks + s2_walks,
            "{} build walk counter must reconcile with its parts",
            fixture.name
        );
        assert_eq!(
            build_walks, CURRENT_BUILD_WALKS,
            "{} current profiled DOM build walk gap",
            fixture.name
        );
        assert!(
            build_walks > fused_walk_target,
            "{} still needs the parse-to-S2/fusion switch before P2-12b can close",
            fixture.name
        );
    }
}

fn traversal_budget(fixture: &str) -> TraversalBudget {
    let value = budgets();
    let id = format!("dom_{fixture}");
    let entry = value
        .get("traversal")
        .and_then(|traversal| traversal.get(&id))
        .unwrap_or_else(|| panic!("budgets.toml [traversal] is missing {id}"));
    TraversalBudget {
        walks: required_u64(entry, &id, "walks"),
        visits: required_u64(entry, &id, "visits"),
    }
}

fn phase_2_dom_walk_target() -> u64 {
    let value = budgets();
    let entry = value
        .get("target")
        .and_then(|target| target.get("phase-2"))
        .expect("budgets.toml has [target.phase-2]");
    required_u64(entry, "target.phase-2", "dom_walks_max")
}

fn budgets() -> toml::Value {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate directory")
        .parent()
        .expect("repo root");
    let text = std::fs::read_to_string(repo.join("davinci-road/plan/budgets.toml"))
        .expect("budgets.toml reads");
    toml::from_str(&text).expect("budgets.toml parses")
}

fn required_u64(entry: &toml::Value, id: &str, field: &str) -> u64 {
    entry
        .get(field)
        .and_then(toml::Value::as_integer)
        .and_then(|value| u64::try_from(value).ok())
        .unwrap_or_else(|| panic!("budgets.toml [traversal.{id}] has no u64 {field}"))
}

fn counter(counters: &CounterSummary, name: &str) -> u64 {
    counters
        .entries
        .iter()
        .find(|entry| entry.name == name)
        .unwrap_or_else(|| panic!("missing {name} profile counter"))
        .total
}

struct ProfileScope;

impl ProfileScope {
    fn enable() -> Self {
        let profiler = global_profiler();
        profiler.clear();
        profiler.enable();
        Self
    }

    fn finish(self) -> CounterSummary {
        let profiler = global_profiler();
        profiler.disable();
        profiler.counter_summary()
    }
}

impl Drop for ProfileScope {
    fn drop(&mut self) {
        let profiler = global_profiler();
        profiler.disable();
        profiler.clear();
    }
}

fn lock_profiler() -> std::sync::MutexGuard<'static, ()> {
    PROFILER_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
