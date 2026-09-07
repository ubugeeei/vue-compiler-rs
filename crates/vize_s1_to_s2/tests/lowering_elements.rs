//! Exact-pinned element-level shapes: components by the native-tag rule,
//! namespaces by inheritance, `ui.model` contracts, `vue.directive`
//! ride-through, `ui.slot` outlets, and the Info deferral of bindings
//! the P2-8 op family cannot carry (whole-artifact equality throughout).

mod support;

use support::artifact;
use vize_davinci::diagnostic::{Diagnostic, Severity, Stage};
use vize_s0::{Span, cstr};

#[test]
fn a_non_native_tag_lowers_as_a_component() {
    let art = artifact("<MyComp>text</MyComp>");
    assert_eq!(
        art.folio,
        "[disegno]\n\
         ops=2\n\
         \n\
         [disegno.ops]\n\
         ui.component MyComp @0:21\n\
         \x20 ui.text \"text\" @8:12\n\
         \n"
    );
}

#[test]
fn the_svg_namespace_is_entered_by_tag_and_inherited() {
    let art = artifact("<svg><path/></svg>");
    assert_eq!(
        art.folio,
        "[disegno]\n\
         ops=2\n\
         \n\
         [disegno.ops]\n\
         ui.element svg ns=svg @0:18\n\
         \x20 ui.element path ns=svg @5:12\n\
         \n"
    );
}

#[test]
fn a_bare_svg_tag_enters_the_svg_namespace() {
    let art = artifact("<feImage />");
    assert_eq!(
        art.folio,
        "[disegno]\n\
         ops=1\n\
         \n\
         [disegno.ops]\n\
         ui.element feImage ns=svg @0:11\n\
         \n"
    );
}

#[test]
fn v_model_lowers_to_the_contract_with_synthesized_attributes() {
    // Read and write share one authored payload; element kind and the
    // dialect modifiers ride as attributes carrying the binding's span,
    // in declared order (element-kind, modifiers).
    let art = artifact("<input v-model.lazy.trim=\"msg\">");
    assert_eq!(
        art.folio,
        "[disegno]\n\
         ops=2\n\
         \n\
         [disegno.ops]\n\
         ui.element input @0:31\n\
         \x20 ui.model read=js(\"msg\" @26:29) write=js(\"msg\" @26:29) @7:30\n\
         \x20   attr element-kind=\"input\" @7:30\n\
         \x20   attr lazy @7:30\n\
         \x20   attr trim @7:30\n\
         \n"
    );
    assert_eq!(art.diagnostics, Vec::new());
}

#[test]
fn a_custom_directive_rides_through_as_the_dialect_op() {
    let art = artifact("<div v-pin:top.stop=\"v\"></div>");
    assert_eq!(
        art.folio,
        "[disegno]\n\
         ops=2\n\
         \n\
         [disegno.ops]\n\
         ui.element div @0:30\n\
         \x20 vue.directive \"pin\" arg=\"top\" mods=\"stop\" value=js(\"v\" @21:22) @5:23\n\
         \n"
    );
}

#[test]
fn a_slot_outlet_owns_its_fallback_and_normalizes_the_implicit_name() {
    let named = artifact("<slot name=\"s\"><span>f</span></slot>");
    assert_eq!(
        named.folio,
        "[disegno]\n\
         ops=3\n\
         \n\
         [disegno.ops]\n\
         ui.slot name=\"s\" @0:36\n\
         \x20 ui.element span @15:29\n\
         \x20   ui.text \"f\" @21:22\n\
         \n"
    );

    let implicit = artifact("<slot></slot>");
    assert_eq!(
        implicit.folio,
        "[disegno]\n\
         ops=1\n\
         \n\
         [disegno.ops]\n\
         ui.slot name=\"default\" @0:13\n\
         \n"
    );

    let dynamic = artifact("<slot :name=\"n\"></slot>");
    assert_eq!(
        dynamic.folio,
        "[disegno]\n\
         ops=1\n\
         \n\
         [disegno.ops]\n\
         ui.slot name=js(\"n\" @13:14) @0:23\n\
         \n"
    );
}

#[test]
fn one_way_bindings_lower_to_the_normalized_ops() {
    // The P2-9 series-5 surface: `:key` rides `ui.bind` on the iterated
    // element (element surface, exactly where the legacy lane keeps the
    // prop), and `@` spellings ride `ui.on` with modifiers verbatim.
    let art = artifact("<li v-for=\"(item, i) in items\" :key=\"item.id\">{{ item.name }}</li>");
    assert_eq!(
        art.folio,
        "[disegno]\n\
         ops=4\n\
         \n\
         [disegno.ops]\n\
         ui.for source=js(\"items\" @24:29) value=js(\"item\" @12:16) key=js(\"i\" @18:19) @0:66\n\
         \x20 ui.element li @0:66\n\
         \x20   ui.bind name=\"key\" value=js(\"item.id\" @37:44) @31:45\n\
         \x20   ui.interpolation js(\"item.name\" @49:58) @46:61\n\
         \n"
    );
    assert_eq!(art.diagnostics, vec![]);

    let on = artifact("<button @click.stop.prevent=\"go()\" v-on=\"handlers\">x</button>");
    assert_eq!(
        on.folio,
        "[disegno]\n\
         ops=4\n\
         \n\
         [disegno.ops]\n\
         ui.element button @0:61\n\
         \x20 ui.on name=\"click\" mods=\"stop,prevent\" handler=js(\"go()\" @29:33) @8:34\n\
         \x20 ui.on handler=js(\"handlers\" @41:49) @35:50\n\
         \x20 ui.text \"x\" @51:52\n\
         \n"
    );
    assert_eq!(on.diagnostics, vec![]);
}

#[test]
fn the_parser_shorthands_are_mirrored_at_lowering() {
    // The same-name shorthand (`:foo-bar` reads its camelized argument)
    // and the `.` dot shorthand (a synthesized leading `prop` modifier),
    // both exactly as the shipped parser applies them
    // (`vize_armature/src/parser/attribute.rs:267-340`).
    let art = artifact("<a :model-value .innerHTML=\"h\"></a>");
    assert_eq!(
        art.folio,
        "[disegno]\n\
         ops=3\n\
         \n\
         [disegno.ops]\n\
         ui.element a @0:35\n\
         \x20 ui.bind name=\"model-value\" value=js(\"modelValue\" @4:15) @3:15\n\
         \x20 ui.bind name=\"innerHTML\" mods=\"prop\" value=js(\"h\" @28:29) @16:30\n\
         \n"
    );
    assert_eq!(art.diagnostics, vec![]);
}

#[test]
fn v_once_lowers_to_the_dialect_flag() {
    let art = artifact("<div v-once>x</div>");
    assert_eq!(
        art.folio,
        "[disegno]\n\
         ops=3\n\
         \n\
         [disegno.ops]\n\
         ui.element div @0:19\n\
         \x20 vue.once @5:11\n\
         \x20 ui.text \"x\" @12:13\n\
         \n"
    );
    assert_eq!(art.diagnostics, vec![]);
}

#[test]
fn v_memo_lowers_to_the_dialect_op_with_the_expression() {
    let art = artifact(r#"<p v-memo="[id]">x</p>"#);
    assert_eq!(
        art.folio,
        "[disegno]\n\
         ops=3\n\
         \n\
         [disegno.ops]\n\
         ui.element p @0:22\n\
         \x20 vue.memo value=js(\"[id]\" @11:15) @3:16\n\
         \x20 ui.text \"x\" @17:18\n\
         \n"
    );
    assert_eq!(art.diagnostics, vec![]);
}

#[test]
fn v_memo_may_carry_an_opaque_expression() {
    let art = artifact(r#"<p v-memo="%">x</p>"#);
    assert_eq!(
        art.folio,
        "[disegno]\n\
         ops=3\n\
         \n\
         [disegno.ops]\n\
         ui.element p @0:19\n\
         \x20 vue.memo value=opaque(parse-rejected \"%\" @11:12) @3:13\n\
         \x20 ui.text \"x\" @14:15\n\
         \n"
    );
    assert_eq!(art.diagnostics, vec![]);
}

#[test]
fn v_show_lowers_to_the_dialect_op() {
    let art = artifact(r#"<p v-show="open">x</p>"#);
    assert_eq!(
        art.folio,
        "[disegno]\n\
         ops=3\n\
         \n\
         [disegno.ops]\n\
         ui.element p @0:22\n\
         \x20 vue.show value=js(\"open\" @11:15) @3:16\n\
         \x20 ui.text \"x\" @17:18\n\
         \n"
    );
    assert_eq!(art.diagnostics, vec![]);
}

#[test]
fn ill_formed_v_once_spellings_still_defer() {
    for (src, element_end, text_start, attr_end) in [
        (r#"<div v-once="x">y</div>"#, 23, 16, 15),
        (r#"<div v-once="">y</div>"#, 22, 15, 14),
        (r#"<div v-once=" ">y</div>"#, 23, 16, 15),
    ] {
        let art = artifact(src);
        assert_eq!(
            art.folio,
            cstr!(
                "[disegno]\n\
                 ops=2\n\
                 \n\
                 [disegno.ops]\n\
                 ui.element div @0:{element_end}\n\
                 \x20 ui.text \"y\" @{text_start}:{}\n\
                 \n",
                text_start + 1
            )
        );
        assert_eq!(
            art.diagnostics,
            vec![Diagnostic::new(
                Severity::Info,
                Stage::Semantic,
                Span::new(5, attr_end),
                "`v-once` is representable as `vue.once` only as the bare directive",
            )]
        );
    }
}

#[test]
fn v_pre_freezes_its_subtree_and_leaves_no_diagnostic() {
    // `v-pre` has an S2 story now: the spelling drops out (recorded as
    // `drop.v-pre`), the binding stays the attribute it was authored as
    // and the interpolation stays literal text, so nothing is deferred
    // and nothing is reported. The compiled form is pinned in
    // `vize_atelier_dom/tests/v_pre_freezes_its_subtree.rs`.
    let art = artifact("<p v-pre :x=\"1\">{{ y }}</p>");
    assert_eq!(
        art.folio,
        "[disegno]\n\
         ops=2\n\
         \n\
         [disegno.ops]\n\
         ui.element p @0:27\n\
         \x20 attr :x=\"1\" @9:15\n\
         \x20 ui.text \"{{ y }}\" @16:23\n\
         \n"
    );
    assert_eq!(art.diagnostics, vec![]);
    // The whole decision stream, exactly: the spelling is dropped under
    // its own rule and the deferral it used to carry is gone.
    let rules: Vec<&str> = art
        .provenance
        .iter()
        .map(|record| record.rule.as_str())
        .collect();
    assert_eq!(rules, ["lower.element", "drop.v-pre", "lower.v-pre-text"]);
}

#[test]
fn a_missing_end_tag_hole_becomes_a_surface_diagnostic() {
    // The tokenizer never reports a missing end tag (end-tag matching is
    // tree construction, not lexing), so the `ElementClose::Missing`
    // hole enters the unified channel at lowering — and the fragment
    // still lowers structurally, its span running to its last child.
    let art = artifact("<div><span>x</div>");
    assert_eq!(
        art.folio,
        "[disegno]\n\
         ops=3\n\
         \n\
         [disegno.ops]\n\
         ui.element div @0:18\n\
         \x20 ui.element span @5:12\n\
         \x20   ui.text \"x\" @11:12\n\
         \n"
    );
    assert_eq!(
        art.diagnostics,
        vec![Diagnostic::new(
            Severity::Error,
            Stage::Surface,
            Span::new(5, 10),
            "Element is missing end tag.",
        )]
    );
}
