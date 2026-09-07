//! `this.*` in a template expression — the Options-API idiom the corpus
//! does not have.
//!
//! Across the 13,050 hydrated corpus templates, `this` appears in a
//! template expression exactly **once**, and never in an interpolation:
//! primevue's `DesignColorPalette.vue`, as
//! `:style="{ backgroundColor: this.designerService.resolveColorPlain(color) }"`.
//! So every differential lane built on that corpus says essentially
//! nothing about a construct every Options-API component may use. It is
//! rare but trivially *enumerable*, which is the same shape of gap that
//! `text_shape_matrix.rs` and `v_pre_freezes_its_subtree.rs` close.
//!
//! The oracle is `compile_template_legacy_with_options`, the shipped
//! parse/transform/codegen lane, over the whole family in three option
//! shapes. Every shape here was also run against `@vue/compiler-dom`
//! 3.5.41 and 3.6.0-beta.10 (they agree with each other); where all of
//! vize differs from Vue, `the_places_vize_and_vue_disagree` pins the
//! difference rather than hiding it.

#![allow(
    clippy::disallowed_macros,
    clippy::disallowed_types,
    clippy::disallowed_methods
)]

mod support;

use vize_atelier_core::options::{BindingType, CodegenMode, CodegenOptions};
use vize_atelier_dom::DomCompilerOptions;
use vize_s0::Allocator;
use vize_s1_to_s2::{DomEmitMode, DomEmitOptions, LegacyCaps, emit_dom_source_with_options};

/// Every place a template expression can sit, carrying `this`.
const SHAPES: &[(&str, &str)] = &[
    ("bind_member", r#"<div :id="this.r">c</div>"#),
    ("bind_deep", r#"<div :id="this.a.b">c</div>"#),
    ("bind_call", r#"<div :id="this.f()">c</div>"#),
    ("bind_dollar", r#"<div :id="this.$refs.x">c</div>"#),
    ("bind_bare", r#"<div :id="this">c</div>"#),
    ("bind_computed", r#"<div :id="this['r']">c</div>"#),
    ("bind_dynamic_key", r#"<div :[this.k]="1">c</div>"#),
    ("interp_member", "<div>{{ this.r }}</div>"),
    ("interp_bare", "<div>{{ this }}</div>"),
    ("interp_mixed", "<div>a {{ this.r }} b</div>"),
    ("on_call", r#"<div @click="this.f()">c</div>"#),
    ("on_assign", r#"<div @click="this.r = 1">c</div>"#),
    (
        "on_statements",
        r#"<div @click="this.a(); this.b()">c</div>"#,
    ),
    ("if_member", r#"<p v-if="this.ok">c</p>"#),
    (
        "if_else_member",
        r#"<p v-if="this.ok">c</p><p v-else>d</p>"#,
    ),
    (
        "for_member",
        r#"<li v-for="i in this.list" :key="i">{{ i }}</li>"#,
    ),
    (
        "for_shadowed",
        r#"<li v-for="this_ in this.list" :key="this_">x</li>"#,
    ),
    ("model_member", r#"<input v-model="this.r">"#),
    ("show_member", r#"<div v-show="this.ok">c</div>"#),
    ("style_object", r#"<div :style="{ color: this.c }">c</div>"#),
    ("style_direct", r#"<div :style="this.s">c</div>"#),
    ("class_direct", r#"<div :class="this.c">c</div>"#),
    ("class_object", r#"<div :class="{ on: this.c }">c</div>"#),
    ("text_member", r#"<p v-text="this.r"></p>"#),
    ("html_member", r#"<div v-html="this.r"></div>"#),
    ("memo_member", r#"<div v-memo="[this.r]">c</div>"#),
    ("once_member", "<div v-once>{{ this.r }}</div>"),
    ("spread_member", r#"<div v-bind="this.attrs">c</div>"#),
    ("component_prop", r#"<MyComp :item="this.r" />"#),
    ("component_array", r#"<MyComp :range="[this.a, this.b]" />"#),
    ("slot_prop", r#"<slot :item="this.r">{{ this.r }}</slot>"#),
    (
        "nested",
        r#"<div :id="this.a"><span :id="this.b">{{ this.c }}</span></div>"#,
    ),
];

fn metadata() -> vize_atelier_core::options::BindingMetadata {
    support::bindings::script_setup_metadata(&[
        ("r", BindingType::SetupRef),
        ("MyComp", BindingType::SetupConst),
    ])
}

/// The three option shapes the DOM path actually ships in: the plain
/// function-mode default, the module + `prefix_identifiers` shape, and
/// the `<script setup>` production shape (`inline` + `cache_handlers`),
/// which is what `compile_template_block` turns on.
fn shape(name: &str) -> (DomCompilerOptions, DomEmitOptions<'static>) {
    let base_emit = DomEmitOptions::DEFAULT;
    match name {
        "default" => (DomCompilerOptions::default(), base_emit),
        "prefixed" => (
            DomCompilerOptions {
                mode: CodegenMode::Module,
                prefix_identifiers: true,
                ..Default::default()
            },
            DomEmitOptions {
                mode: DomEmitMode::Module,
                prefix_identifiers: true,
                ..base_emit
            },
        ),
        "production" => (
            DomCompilerOptions {
                mode: CodegenMode::Module,
                prefix_identifiers: true,
                inline: true,
                cache_handlers: true,
                ..Default::default()
            },
            DomEmitOptions {
                mode: DomEmitMode::Module,
                prefix_identifiers: true,
                inline: true,
                cache_handlers: true,
                ..base_emit
            },
        ),
        other => panic!("unknown option shape {other}"),
    }
}

#[test]
fn the_this_family_agrees_with_the_shipped_lane_in_every_option_shape() {
    let metadata = metadata();
    let table = support::bindings::binding_table(&metadata);
    let mut compared = 0usize;

    for shape_name in ["default", "prefixed", "production"] {
        let (mut options, emit) = shape(shape_name);
        options.binding_metadata = Some(metadata.clone());
        let emit = DomEmitOptions {
            bindings: Some(&table),
            ..emit
        };
        support::assert_s2_matches_shipped_with_options(
            SHAPES,
            &options,
            &CodegenOptions::default(),
            &emit,
        );
        compared += SHAPES.len();
    }

    // The scope proof: a matrix that quietly stopped generating shapes
    // would pass every assertion above.
    assert_eq!(compared, SHAPES.len() * 3);
    assert_eq!(compared, 96);
}

/// The render function's `return`, flattened — the import list's order is
/// its own subject and would only couple this test to it.
fn render_body(code: &str) -> String {
    code.lines()
        .map(str::trim)
        .skip_while(|line| !line.starts_with("return "))
        .collect::<Vec<_>>()
        .join(" ")
}

/// The S2 emit's body under the module + `prefix_identifiers` shape,
/// having first required the shipped lane to agree — so each expectation
/// below pins **both** lanes, and says so if they ever part.
fn prefixed_body(source: &str) -> String {
    let metadata = metadata();
    let table = support::bindings::binding_table(&metadata);
    let (mut options, emit) = shape("prefixed");
    options.binding_metadata = Some(metadata.clone());

    let allocator = Allocator::new();
    let s2 = render_body(
        &emit_dom_source_with_options(
            &allocator,
            source,
            LegacyCaps::for_version(options.dialect),
            &DomEmitOptions {
                bindings: Some(&table),
                ..emit
            },
        )
        .unwrap_or_else(|error| panic!("S2 emit refused {source:?}: {error:?}"))
        .assembled(),
    );
    let shipped = render_body(&support::shipped_with_options(
        source,
        &options,
        &CodegenOptions::default(),
    ));
    assert_eq!(s2, shipped, "the two lanes diverged on {source:?}");
    s2
}

/// Where vize and `@vue/compiler-dom` part company on this family.
///
/// All three are the same disagreement seen from three sides: Vue's
/// constant analysis calls a bare `this` **constant** and anything under
/// it **dynamic**, and vize's calls a bare `this` dynamic and a member
/// read off it constant. Neither is a miscompile — `normalizeStyle` is
/// the identity on an object literal, and a hoisted-versus-inlined props
/// object renders the same — but they are real output differences, and
/// they are pinned here so that closing one is a decision rather than a
/// surprise.
///
/// Every expectation below is required of the S2 emit **and** of the
/// shipped lane, so none of this arrived with the S2 lane: the shipped
/// transform has always read `this` this way.
#[test]
fn the_places_vize_and_vue_disagree() {
    // 1. `:style` over a `this` member. Vue emits
    //    `{ style: _normalizeStyle({ color: this.c }) }`; vize decides the
    //    object is constant and drops the helper. This is the one shape
    //    the corpus does have — primevue's `DesignColorPalette.vue`.
    assert_eq!(
        prefixed_body(r#"<div :style="{ color: this.c }">c</div>"#),
        r#"return (_openBlock(), _createElementBlock("div", { style: { color: this.c } }, "c", 4 /* STYLE */)) }"#,
    );

    // …and vize is not simply eliding the helper everywhere: a *binding*
    // read keeps it, which is what makes the `this` verdict a difference
    // in the constant analysis rather than in the style printer.
    assert_eq!(
        prefixed_body(r#"<div :style="{ color: r }">c</div>"#),
        r#"return (_openBlock(), _createElementBlock("div", { style: _normalizeStyle({ color: $setup.r }) }, "c", 4 /* STYLE */)) }"#,
    );

    // 2. A bare `this` as a bound value. Vue hoists the whole props
    //    object (`_hoisted_1 = { id: this }`) and emits **no** patch flag;
    //    vize keeps it a dynamic prop.
    assert_eq!(
        prefixed_body(r#"<div :id="this">c</div>"#),
        r#"return (_openBlock(), _createElementBlock("div", { id: this }, "c", 8 /* PROPS */, ["id"])) }"#,
    );

    // 3. A bare `this` interpolated. Vue emits no patch flag for the same
    //    reason; vize keeps `TEXT`.
    assert_eq!(
        prefixed_body("<div>{{ this }}</div>"),
        r#"return (_openBlock(), _createElementBlock("div", null, _toDisplayString(this), 1 /* TEXT */)) }"#,
    );

    // The direction that matters for safety is the other one: a `this`
    // read must never reach module scope, where `this` is `undefined`.
    // It does not — the props hoist declines it, while the same shape
    // over a literal is hoisted.
    assert_eq!(
        prefixed_body(r#"<div :style="{ color: 'red' }"></div>"#),
        r#"return (_openBlock(), _createElementBlock("div", _hoisted_1)) }"#,
    );
    assert_eq!(
        prefixed_body(r#"<MyComp :range="[this.a, this.b]" />"#),
        r#"return (_openBlock(), _createBlock($setup.MyComp, { range: [this.a, this.b] }, null, 8 /* PROPS */, ["range"])) }"#,
    );
}
