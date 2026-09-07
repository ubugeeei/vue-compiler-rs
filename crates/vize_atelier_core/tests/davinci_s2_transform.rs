//! P2-9, the plain-suite coverage witness: the committed battery (the
//! v-if half from series 1, the v-for half from series 2, the v-slot
//! half from series 3, the text half from series 4) dual-runs
//! legacy-vs-S2 with **exact-pinned** counters, so a cfg or flag
//! regression that disarms the differential lane fails loudly here (the
//! P1-6/P1-7 witness law). The corpus-widened entry is
//! `davinci_s2_transform_corpus.rs` (feature `davinci-differential`).

mod s2_support;

use s2_support::{
    BATTERY, Counters, HoistCounters, SlotCounters, SurfaceCounters, TextCounters, compare,
};

/// The battery's exact accounting. The corpus entry pins the same
/// numbers; both move together, deliberately, when the battery does.
fn expected() -> Counters {
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
    }
}

#[test]
fn the_battery_dual_runs_with_pinned_counts() {
    let mut counters = Counters::default();
    for (name, source) in BATTERY {
        compare(name, source, &mut counters);
    }
    assert_eq!(
        counters,
        expected(),
        "the v-if differential lane's battery accounting moved: re-pin in \
         BOTH this witness and the corpus entry deliberately"
    );
}

#[test]
fn the_same_key_wording_never_drifts_from_relief() {
    // The S2 pass duplicates relief's message text rather than the
    // dependency (ricalco must not depend on the legacy AST crate);
    // this pin is what keeps the two channels from drifting.
    assert_eq!(
        vize_s1_to_s2::pass::vif::SAME_KEY_MESSAGE,
        vize_atelier_core::ErrorCode::VIfSameKey.message()
    );
}

#[test]
fn the_v_slot_wording_never_drifts_from_relief() {
    // The series-3 messages, same discipline: the S2 pass duplicates
    // relief's text rather than the dependency; these pins own the
    // relief edge.
    use vize_atelier_core::ErrorCode;
    use vize_s1_to_s2::pass::vslot;
    assert_eq!(
        vslot::MISPLACED_MESSAGE,
        ErrorCode::VSlotMisplaced.message()
    );
    assert_eq!(
        vslot::MIXED_MESSAGE,
        ErrorCode::VSlotMixedSlotUsage.message()
    );
    assert_eq!(
        vslot::DUPLICATE_MESSAGE,
        ErrorCode::VSlotDuplicateSlotNames.message()
    );
    assert_eq!(
        vslot::EXTRANEOUS_MESSAGE,
        ErrorCode::VSlotExtraneousDefaultSlotChildren.message()
    );
}

#[test]
fn both_lanes_flag_the_duplicate_slot_name() {
    // End-to-end wording check on the battery's duplicate template: the
    // legacy transform reports `VSlotDuplicateSlotNames`, the S2 pass
    // appends the same text to the unified channel.
    let (_, source) = BATTERY
        .iter()
        .find(|(name, _)| *name == "duplicate-slot-names")
        .expect("the battery carries the duplicate template");

    let allocator = vize_s0::Allocator::new();
    let (mut root, parse_errors) = vize_atelier_core::parser::parse(&allocator, source);
    assert!(parse_errors.is_empty(), "{parse_errors:?}");
    let transform_errors = vize_atelier_core::transform(
        &allocator,
        &mut root,
        vize_atelier_core::TransformOptions::default(),
        None,
    );
    assert!(
        transform_errors
            .iter()
            .any(|error| error.code == vize_atelier_core::ErrorCode::VSlotDuplicateSlotNames),
        "the legacy lane must flag the duplicate: {transform_errors:?}"
    );

    let s2_allocator = vize_s0::Allocator::new();
    let (tree, surface_errors) = vize_s1::parse(&s2_allocator, source);
    let mut lowered = vize_s1_to_s2::lower(&s2_allocator, &tree, &surface_errors);
    let _facts =
        vize_s1_to_s2::pass::run_transform(&mut lowered, &mut vize_davinci::pass::NoObserver);
    assert!(
        lowered
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.as_str()
                == vize_s1_to_s2::pass::vslot::DUPLICATE_MESSAGE),
        "the S2 pass must flag the duplicate: {:?}",
        lowered.diagnostics
    );
}

#[test]
fn the_v_model_wording_never_drifts_from_relief() {
    // The series-5 messages, same discipline: the S2 pass duplicates
    // relief's text rather than the dependency; these pins own the
    // relief edge.
    use vize_atelier_core::ErrorCode;
    use vize_s1_to_s2::pass::vmodel;
    assert_eq!(vmodel::ON_SCOPE_MESSAGE, ErrorCode::VModelOnScope.message());
    assert_eq!(
        vmodel::ARG_ON_ELEMENT_MESSAGE,
        ErrorCode::VModelArgOnElement.message()
    );
}

#[test]
fn both_lanes_flag_the_scoped_model() {
    // End-to-end wording check on the battery's invalid-scope template:
    // the legacy transform reports `VModelOnScope`, the S2 pass appends
    // the same text to the unified channel.
    let (_, source) = BATTERY
        .iter()
        .find(|(name, _)| *name == "model-invalid-scope")
        .expect("the battery carries the invalid-scope template");

    let allocator = vize_s0::Allocator::new();
    let (mut root, parse_errors) = vize_atelier_core::parser::parse(&allocator, source);
    assert!(parse_errors.is_empty(), "{parse_errors:?}");
    let transform_errors = vize_atelier_core::transform(
        &allocator,
        &mut root,
        vize_atelier_core::TransformOptions::default(),
        None,
    );
    assert!(
        transform_errors
            .iter()
            .any(|error| error.code == vize_atelier_core::ErrorCode::VModelOnScope),
        "the legacy lane must flag the scoped model: {transform_errors:?}"
    );

    let s2_allocator = vize_s0::Allocator::new();
    let (tree, surface_errors) = vize_s1::parse(&s2_allocator, source);
    let mut lowered = vize_s1_to_s2::lower(&s2_allocator, &tree, &surface_errors);
    let _facts =
        vize_s1_to_s2::pass::run_transform(&mut lowered, &mut vize_davinci::pass::NoObserver);
    assert!(
        lowered
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.as_str()
                == vize_s1_to_s2::pass::vmodel::ON_SCOPE_MESSAGE),
        "the S2 pass must flag the scoped model: {:?}",
        lowered.diagnostics
    );
}

#[test]
fn both_lanes_flag_the_duplicate_key() {
    // End-to-end wording check on the battery's collision template: the
    // legacy transform reports `VIfSameKey`, the S2 pass appends the
    // same text to the unified channel.
    let (_, source) = BATTERY
        .iter()
        .find(|(name, _)| *name == "duplicate-keys")
        .expect("the battery carries the collision template");

    let allocator = vize_s0::Allocator::new();
    let (mut root, parse_errors) = vize_atelier_core::parser::parse(&allocator, source);
    assert!(parse_errors.is_empty(), "{parse_errors:?}");
    let transform_errors = vize_atelier_core::transform(
        &allocator,
        &mut root,
        vize_atelier_core::TransformOptions::default(),
        None,
    );
    assert!(
        transform_errors
            .iter()
            .any(|error| error.code == vize_atelier_core::ErrorCode::VIfSameKey),
        "the legacy lane must flag the collision: {transform_errors:?}"
    );

    let s2_allocator = vize_s0::Allocator::new();
    let (tree, surface_errors) = vize_s1::parse(&s2_allocator, source);
    let mut lowered = vize_s1_to_s2::lower(&s2_allocator, &tree, &surface_errors);
    let _facts =
        vize_s1_to_s2::pass::run_transform(&mut lowered, &mut vize_davinci::pass::NoObserver);
    assert!(
        lowered
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.as_str()
                == vize_s1_to_s2::pass::vif::SAME_KEY_MESSAGE),
        "the S2 pass must flag the collision: {:?}",
        lowered.diagnostics
    );
}

#[test]
fn the_legacy_flag_disarms_the_lane_visibly() {
    // Not an env-mutating test (the harness runs tests concurrently):
    // the flag's read is pinned by name here, and its disarm arm is the
    // `skipped_legacy_flag` counter the battery pin holds at zero.
    assert_eq!(
        vize_s1_to_s2::pass::TRANSFORM_LANE_FLAG,
        "VIZE_DAVINCI_TRANSFORM"
    );
}
