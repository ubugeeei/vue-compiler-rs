//! P2-9, the corpus-runnable differential entry — the
//! P1-6/P1-7 lane shape. Compiled only with the `davinci-differential`
//! feature (`[[test]] required-features`). Runs the committed battery
//! with the same exact-pinned counters as the plain witness, then, with
//! `VIZE_DAVINCI_DIFFERENTIAL_CORPUS=<dir>`, additionally dual-runs the
//! template block of every `.vue` file under `<dir>` through both
//! lanes. Divergence panics (TS-25); every skip is a counted class.
//! The canonical fixture root fails closed unless its submodule
//! inventory reconciles; other roots sweep in smoke scope with
//! `closure_evidence=false` (see `davinci_test_support::corpus`).
//!
//! Run:
//!
//! ```text
//! VIZE_DAVINCI_DIFFERENTIAL_CORPUS=tests/_fixtures/_git \
//!     cargo test -p vize_atelier_core --features davinci-differential \
//!     --test davinci_s2_transform_corpus -- --nocapture
//! ```

mod s2_support;

use std::fs;

use davinci_harness::fixtures::template_block;
use s2_support::{
    BATTERY, Counters, HoistCounters, SlotCounters, SurfaceCounters, TextCounters, compare,
};

#[test]
fn the_s2_transform_lane_holds_over_the_corpus() {
    // -- committed battery, the same exact pins as the plain witness --
    let mut counters = Counters::default();
    for (name, source) in BATTERY {
        compare(name, source, &mut counters);
    }
    assert_eq!(
        counters,
        Counters {
            templates_seen: 90,
            compared: 90,
            skipped_legacy_flag: 0,
            skipped_old_parse_errors: 0,
            skipped_s2_errors: 0,
            if_ops: 24,
            branches: 45,
            keys_static: 14,
            keys_dynamic: 2,
            keys_wrapper: 2,
            keys_dynamic_arg: 0,
            keys_compound: 0,
            conditions_compound: 0,
            for_ops: 19,
            for_values: 18,
            for_keys: 4,
            for_indexes: 1,
            for_values_absent: 1,
            for_compound: 0,
            slots: SlotCounters {
                units: 13,
                groups: 16,
                group_params: 8,
                groups_invented: 4,
                groups_dynamic: 1,
                units_conditional: 1,
                units_forwarded: 0,
                units_filler_default: 1,
                outlets: 7,
                outlets_dynamic: 1,
            },
            text: TextCounters {
                units: 109,
                parts_static: 95,
                parts_dynamic: 26,
                compound_units: 9,
                vpre_templates: 0,
                entity_templates: 1,
                rawtext_excluded: 1,
                parts_compound: 0,
                parts_filter: 0,
            },
            surfaces: SurfaceCounters {
                owners: 158,
                attrs: 14,
                binds: 8,
                binds_dynamic: 1,
                binds_spread: 1,
                ons: 3,
                ons_dynamic: 1,
                ons_spread: 1,
                directives: 2,
                models: 4,
                models_invalid: 2,
                models_dynamic_arg: 0,
                models_pattern_scope: 2,
                keys_excluded: 2,
                builtins_excluded: 3,
                wrapper_attrs: 2,
                entity_templates: 1,
                table_templates: 0,
                values_compound: 0,
            },
            hoist: HoistCounters {
                elements: 99,
                whole: 15,
                props: 7,
                wrapper_hoists: 0,
                comments_elements: 4,
                builtins_subtrees: 2,
                consts_templates: 1,
                classifier_templates: 2,
                models_templates: 3,
                tree_templates: 1,
                vpre_templates: 0,
                table_templates: 0,
            },
        },
        "battery accounting moved: re-pin in BOTH lanes deliberately"
    );

    // -- optional corpus sweep --------------------------------------
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
    let mut corpus = Counters::default();
    let mut unreadable = 0u64;
    let mut without_template = 0u64;
    for file in files {
        let Ok(source) = fs::read_to_string(file) else {
            unreadable += 1;
            continue;
        };
        let Some(template) = template_block(&source) else {
            without_template += 1;
            continue;
        };
        let context = file.to_string_lossy();
        compare(context.as_ref(), template, &mut corpus);
    }
    eprintln!(
        "davinci s2 transform corpus sweep: files={} unreadable={unreadable} \
         without_template={without_template} {corpus:?}",
        files.len(),
    );
}
