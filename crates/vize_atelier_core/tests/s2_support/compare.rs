//! Dual-run entry for the P2-9 comparator.
//!
//! Vue 3 is [`compare`]; other dialects use [`compare_with`]. Both
//! lanes receive the dialect: the shipped transform via
//! `TransformOptions.dialect`, and S2 via [`LegacyCaps`] at lower
//! time (the pass table follows `lowered.caps`).

use vize_atelier_core::parser::parse_with_options as old_parse_with_options;
use vize_atelier_core::{ParserOptions, TransformOptions, transform};
use vize_davinci::diagnostic::Severity;
use vize_davinci::pass::NoObserver;
use vize_s0::Allocator;
use vize_s0::config::VueVersion;
use vize_s1_to_s2::LegacyCaps;
use vize_s1_to_s2::pass::{TRANSFORM_LANE_FLAG, run_transform};
use vize_s2::folio::DisegnoFolio;

use super::{
    Counters, checks, hoist, hoist_old, old_lane, s2_lane, slots, slots_old, surface_check,
    surface_old, text, text_old,
};

/// Dual-run `source` through both lanes and compare the projections.
///
/// # Panics
///
/// Panics on any divergence inside the compared domain (TS-25), with
/// the template and both projections in the message.
pub fn compare(name: &str, source: &str, counters: &mut Counters) {
    compare_with(name, source, counters, VueVersion::V3);
}

/// Dual-run `source` under an explicit Vue dialect.
///
/// # Panics
///
/// Same contract as [`compare`].
pub fn compare_with(name: &str, source: &str, counters: &mut Counters, dialect: VueVersion) {
    counters.templates_seen += 1;
    if std::env::var(TRANSFORM_LANE_FLAG).is_ok_and(|value| value == "legacy") {
        counters.skipped_legacy_flag += 1;
        return;
    }

    // Legacy lane: the shipped parse + transform.
    //
    // `is_pre_tag` takes the shipped DOM configuration
    // (`crates/vize_atelier_dom/src/compile/stage_options.rs`) so both
    // lanes exempt `<pre>` from whitespace condensing the same way — the
    // default `|_| false` would condense inside `<pre>`, which no shipped
    // compile does. It feeds only the condense strategy, so every
    // pre-series-4 projection is unaffected.
    //
    // `comments` stays at its `ParserOptions` default of `true`, which
    // the slot, surface and hoist projections below are calibrated
    // against. The **text** projection cannot use this tree: comments are
    // an input to both halves of the text lane — the condense neighbour
    // rule and the run grouping — and the S2 side lowers with
    // `preserve_comments = false`, the shipped `DomCompilerOptions`
    // default. Comparing a comment-bearing legacy tree against a
    // comment-free S2 one made the lanes disagree by construction on
    // every comment-adjacent shape (11 of 12,017 corpus templates), so
    // the text projection takes its own parse below.
    //
    // Moving the other three projections onto the shipped configuration
    // is a task of its own, and not a free one: dropping comments there
    // retires `hoist_old::has_comment_child`, which is what keeps
    // comment-bearing owners out of the hoist replay, and the first owner
    // it then reaches already disagrees. `a-menu-item` in ant-design-vue's
    // `site/src/layouts/header/Navigation.vue` carries S2 facts
    // `NotStatic, props_hoistable, nested_static`, which `hoist::predict`
    // turns into `Props` where the legacy lane decides `None`. That is a
    // gap in the **replay**, not in what ships: `davinci_dom_corpus`
    // finds the two lanes' compiled output for that file byte-identical,
    // and a props hoist is visible in output (`_hoisted_1 = {…}`), so the
    // effective decision agrees. What differs is that `pass::hoist`
    // publishes `props_hoistable` as a *permission* — its module docs put
    // the position- and option-dependent decision in DOM realization —
    // and the replay reads it as the decision.
    let old_allocator = Allocator::new();
    let options = ParserOptions {
        is_pre_tag: |tag| tag == "pre",
        ..ParserOptions::default()
    };
    let (mut root, parse_errors) = old_parse_with_options(&old_allocator, source, options);
    if parse_errors.iter().any(|error| !error.code.is_recovery()) {
        counters.skipped_old_parse_errors += 1;
        return;
    }
    let _transform_errors = transform(
        &old_allocator,
        &mut root,
        transform_options(dialect, false),
        None,
    );
    let mut old_chains = Vec::new();
    let mut old_fors = Vec::new();
    old_lane::collect(&root.children, &mut old_chains, &mut old_fors);
    let mut old_units = Vec::new();
    let mut old_outlets = Vec::new();
    slots_old::collect_old(&root.children, source, &mut old_units, &mut old_outlets);
    // The text projection's own lane: the shipped comments-off parse, so
    // both sides answer the same question. Whitespace condensing runs at
    // parse time, so this has to be a second parse rather than a filter
    // over the tree above.
    let text_allocator = Allocator::new();
    let (mut text_root, _) = old_parse_with_options(
        &text_allocator,
        source,
        ParserOptions {
            is_pre_tag: |tag| tag == "pre",
            comments: false,
            ..ParserOptions::default()
        },
    );
    let _ = transform(
        &text_allocator,
        &mut text_root,
        transform_options(dialect, false),
        None,
    );
    let mut old_text_units = Vec::new();
    text_old::collect_units(&text_root.children, &mut old_text_units);
    let mut old_surfaces = Vec::new();
    surface_old::collect_surfaces(
        &root.children,
        false,
        &mut old_surfaces,
        &mut counters.surfaces,
    );

    // S2 lane: S1 parse -> S1-to-S2 lower -> the S2 passes through
    // the P2-2 pass manager (verifier between passes in debug).
    let s2_allocator = Allocator::new();
    let (tree, surface_errors) = vize_s1::parse(&s2_allocator, source);
    let mut lowered = vize_s1_to_s2::lower_with_caps(
        &s2_allocator,
        &tree,
        &surface_errors,
        LegacyCaps::for_version(dialect),
    );
    if lowered.diagnostics.iter().any(blocks_s2_comparison) {
        counters.skipped_s2_errors += 1;
        return;
    }
    let facts = run_transform(&mut lowered, &mut NoObserver);
    let folio = DisegnoFolio::of(&lowered.root.ops);
    let s2 = s2_lane::collect(
        &folio,
        &s2_lane::Tables {
            if_facts: &facts.if_facts,
            slot_facts: &facts.slot_facts,
            text_facts: &facts.text_facts,
            model_faults: &facts.model_faults,
        },
    );

    checks::check(name, source, &old_chains, &s2.chains, counters);
    checks::check_fors(name, source, &old_fors, &s2.fors, counters);
    slots::check(
        name,
        source,
        &old_units,
        &s2.units,
        &old_outlets,
        &s2.outlets,
        &mut counters.slots,
    );
    counters.text.rawtext_excluded += s2.text_rawtext_excluded;
    if s2.has_table {
        // The legacy in-table tree construction class ([`surface`]
        // module docs): owner order and count can genuinely differ
        // inside table subtrees, so the surface half skips whole.
        counters.surfaces.table_templates += 1;
    } else {
        counters.surfaces.models_invalid += s2.models_invalid;
        counters.surfaces.keys_excluded += s2.keys_excluded;
        surface_check::check(
            name,
            source,
            &old_surfaces,
            &s2.surfaces,
            &mut counters.surfaces,
        );
    }
    // The text projection's template-level v-pre class ([`text`] module
    // docs): the legacy parser honours `v-pre` and then erases it from
    // its tree, so the deterministic detector is the S2 lowering's own
    // deferral record.
    let has_vpre = lowered
        .provenance
        .iter()
        .any(|record| record.rule.as_str() == "defer.v-pre");
    if has_vpre {
        counters.text.vpre_templates += 1;
    } else {
        text::check(
            name,
            source,
            &old_text_units,
            &s2.text_units,
            &mut counters.text,
        );
    }

    // The hoist-decision half (series 6): the shipped hoisting run's
    // actual mutations against the S2 facts' predictions. Template-
    // level classes first (each detector's reasoning: [`hoist`] module
    // docs), then the shape pre-check (the pairing contract), then the
    // hoist-armed second legacy run and the three-tree walk.
    let models_excluded = s2.models_invalid > 0
        || s2
            .surfaces
            .iter()
            .any(|surface| surface.pattern_scoped && !surface.models.is_empty());
    if has_vpre {
        counters.hoist.vpre_templates += 1;
    } else if s2.has_table {
        counters.hoist.table_templates += 1;
    } else if models_excluded {
        counters.hoist.models_templates += 1;
    } else {
        let mut scan = hoist_old::TemplateScan::default();
        hoist_old::scan_template(&root.children, &mut scan);
        let mut old_shape = vize_s0::String::default();
        hoist_old::shape_of(&root.children, &mut old_shape);
        let mut s2_shape = vize_s0::String::default();
        hoist::shape_of_s2(&folio.ops, &mut s2_shape);
        if scan.classifier {
            counters.hoist.classifier_templates += 1;
        } else if scan.consts {
            counters.hoist.consts_templates += 1;
        } else if old_shape != s2_shape {
            counters.hoist.tree_templates += 1;
        } else {
            let hoist_allocator = Allocator::new();
            let options = ParserOptions {
                is_pre_tag: |tag| tag == "pre",
                ..ParserOptions::default()
            };
            let (mut hoisted_root, _) = old_parse_with_options(&hoist_allocator, source, options);
            let _ = transform(
                &hoist_allocator,
                &mut hoisted_root,
                transform_options(dialect, true),
                None,
            );
            hoist::check(
                name,
                source,
                &root.children,
                &hoisted_root.children,
                &folio.ops,
                &facts.static_facts,
                &mut counters.hoist,
            );
        }
    }
    counters.compared += 1;
}

fn transform_options(dialect: VueVersion, hoist_static: bool) -> TransformOptions {
    TransformOptions {
        dialect,
        hoist_static,
        ..TransformOptions::default()
    }
}

fn blocks_s2_comparison(diagnostic: &vize_davinci::diagnostic::Diagnostic) -> bool {
    diagnostic.severity == Severity::Error
        && diagnostic.message.as_str() != vize_s1_to_s2::pass::vif::SAME_KEY_MESSAGE
}
