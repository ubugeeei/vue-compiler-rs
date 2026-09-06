//! Vue 2 sugar legalization (P2-9 installment 7): the pass rewrites
//! dialect payloads into the Vue 3 surface. Vue 3 model-free artifacts
//! skip the `v-model` diagnostic pass (`walks=5`).

mod support;

use support::{
    assert_transformed_sound, assert_transformed_sound_caps, with_transformed,
    with_transformed_caps,
};
use vize_davinci::folio::{Folio, FolioMode};
use vize_s0::config::VueVersion;
use vize_s1_to_s2::LegacyCaps;

fn vue2() -> LegacyCaps {
    LegacyCaps::for_version(VueVersion::V2)
}

#[test]
fn vue3_model_free_legacy_spellings_skip_the_model_pass() {
    let source = r#"<Comp :title.sync="heading"/>"#;
    with_transformed(source, |lowered, folio, _, budget| {
        assert_eq!(
            folio.print_to_string(FolioMode::Full).as_str(),
            "[disegno]\n\
             ops=2\n\
             \n\
             [disegno.ops]\n\
             ui.component Comp @0:29\n\
             \x20 ui.bind name=\"title\" mods=\"sync\" value=js(\"heading\" @19:26) @6:27\n\
             \n"
        );
        assert_eq!(lowered.caps, LegacyCaps::VUE3);
        assert_eq!(
            Folio::print_to_string(budget, FolioMode::Full).as_str(),
            "[budget-observer]\nwalks=5\npasses=5\nanalyses=0\npipelines=1\nfailures=0\n\n"
        );
    });
    assert_transformed_sound(source, "vue3-sync-inert-pass");
}

#[test]
fn vue2_expands_sync_into_bind_plus_update_listener() {
    let source = r#"<Comp :title.sync="heading"/>"#;
    with_transformed_caps(source, vue2(), |lowered, folio, _, budget| {
        assert_eq!(
            folio.print_to_string(FolioMode::Full).as_str(),
            "[disegno]\n\
             ops=3\n\
             \n\
             [disegno.ops]\n\
             ui.component Comp @0:29\n\
             \x20 ui.bind name=\"title\" value=js(\"heading\" @19:26) @6:27\n\
             \x20 ui.on name=\"update:title\" handler=js(\"$event => ((heading) = $event)\" @6:27) @6:27\n\
             \n"
        );
        assert_eq!(
            Folio::print_to_string(budget, FolioMode::Full).as_str(),
            "[budget-observer]\nwalks=6\npasses=6\nanalyses=0\npipelines=1\nfailures=0\n\n"
        );
        assert_eq!(u64::from(lowered.op_count), folio.op_count());
    });
    assert_transformed_sound_caps(source, vue2(), "vue2-sync-expand");
}

#[test]
fn vue2_keeps_camel_on_the_bind() {
    let source = r#"<Comp :title.sync.camel="heading"/>"#;
    with_transformed_caps(source, vue2(), |_, folio, _, _| {
        assert_eq!(
            folio.print_to_string(FolioMode::Full).as_str(),
            "[disegno]\n\
             ops=3\n\
             \n\
             [disegno.ops]\n\
             ui.component Comp @0:35\n\
             \x20 ui.bind name=\"title\" mods=\"camel\" value=js(\"heading\" @25:32) @6:33\n\
             \x20 ui.on name=\"update:title\" handler=js(\"$event => ((heading) = $event)\" @6:33) @6:33\n\
             \n"
        );
    });
    assert_transformed_sound_caps(source, vue2(), "vue2-sync-camel-pass");
}

#[test]
fn vue2_rewrites_a_pipe_filter_to_the_asset_call() {
    let source = "{{msg | cap}}";
    with_transformed_caps(source, vue2(), |_, folio, facts, _| {
        assert_eq!(
            folio.print_to_string(FolioMode::Full).as_str(),
            "[disegno]\n\
             ops=1\n\
             \n\
             [disegno.ops]\n\
             ui.interpolation js(\"_filter_cap(msg)\" @2:11) @0:13\n\
             \n"
        );
        assert_eq!(
            facts
                .legacy
                .filters
                .iter()
                .map(|name| name.as_str())
                .collect::<std::vec::Vec<_>>(),
            ["cap"]
        );
    });
    assert_transformed_sound_caps(source, vue2(), "vue2-filter-wrap");
}

#[test]
fn vue2_rewrites_a_filter_with_args() {
    let source = "{{a | f(b)}}";
    with_transformed_caps(source, vue2(), |_, folio, facts, _| {
        assert_eq!(
            folio.print_to_string(FolioMode::Full).as_str(),
            "[disegno]\n\
             ops=1\n\
             \n\
             [disegno.ops]\n\
             ui.interpolation js(\"_filter_f(a,b)\" @2:10) @0:12\n\
             \n"
        );
        assert_eq!(
            facts
                .legacy
                .filters
                .iter()
                .map(|name| name.as_str())
                .collect::<std::vec::Vec<_>>(),
            ["f"]
        );
    });
    assert_transformed_sound_caps(source, vue2(), "vue2-filter-args");
}

#[test]
fn vue2_converts_slot_scope_into_slot_content() {
    let source = r#"<Comp><template slot-scope="props">x</template></Comp>"#;
    with_transformed_caps(source, vue2(), |_, folio, facts, _| {
        assert_eq!(
            folio.print_to_string(FolioMode::Full).as_str(),
            "[disegno]\n\
             ops=4\n\
             \n\
             [disegno.ops]\n\
             ui.component Comp @0:54\n\
             \x20 ui.element template @6:47\n\
             \x20   ui.slot-content params=js(\"props\" @28:33) @16:34\n\
             \x20   ui.text \"x\" @35:36\n\
             \n"
        );
        assert_eq!(facts.slot_facts.len(), 1);
    });
    assert_transformed_sound_caps(source, vue2(), "vue2-slot-scope-pass");
}

#[test]
fn vue2_strips_native_and_rewrites_keycodes() {
    let source = r#"<Comp @click.native @keyup.13="onKey"/>"#;
    with_transformed_caps(source, vue2(), |_, folio, _, _| {
        assert_eq!(
            folio.print_to_string(FolioMode::Full).as_str(),
            "[disegno]\n\
             ops=3\n\
             \n\
             [disegno.ops]\n\
             ui.component Comp @0:39\n\
             \x20 ui.on name=\"click\" @6:19\n\
             \x20 ui.on name=\"keyup\" mods=\"enter\" handler=js(\"onKey\" @31:36) @20:37\n\
             \n"
        );
    });
    assert_transformed_sound_caps(source, vue2(), "vue2-on-sugar");
}

#[test]
fn vue3_leaves_native_and_keycodes() {
    let source = r#"<Comp @click.native @keyup.13="onKey"/>"#;
    with_transformed(source, |_, folio, _, _| {
        assert_eq!(
            folio.print_to_string(FolioMode::Full).as_str(),
            "[disegno]\n\
             ops=3\n\
             \n\
             [disegno.ops]\n\
             ui.component Comp @0:39\n\
             \x20 ui.on name=\"click\" mods=\"native\" @6:19\n\
             \x20 ui.on name=\"keyup\" mods=\"13\" handler=js(\"onKey\" @31:36) @20:37\n\
             \n"
        );
    });
    assert_transformed_sound(source, "vue3-on-inert");
}
