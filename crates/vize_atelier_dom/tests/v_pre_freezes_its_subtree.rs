//! `v-pre` freezes the element and everything under it.
//!
//! Vue's parser sets `inVPre` the moment it reads the spelling on an
//! opening tag and rewrites **every** prop on that element back to a
//! plain attribute under its raw authored name, for the element itself
//! and its whole subtree. So `:x="1"` stays the attribute `":x"` with the
//! string value `"1"`, `@click` stays `"@click"`, `v-if` never builds a
//! branch, `v-for` never builds a region, and `{{ x }}` is literal text.
//! The spelling itself is dropped from the output.
//!
//! The S2 lowering carried only the last of those: interpolations went
//! inert, and everything else compiled as usual. Since P2-11 made the S2
//! lane the shipped one, `v-pre` was effectively unimplemented in
//! production — `<div v-pre :x="1">` shipped `{ x: 1 }` where Vue emits
//! `{ ":x": "1" }`, and `<div v-pre><li v-for="i in l">` shipped a real
//! `renderList`.
//!
//! No corpus template caught it: `davinci_s2_transform_corpus`'s sweep
//! counts **zero** `v-pre` templates across the hydrated corpus, which is
//! why this needs its own witness. Expectations are `@vue/compiler-dom`
//! 3.6.0-beta.10's own output, cross-checked against this crate's legacy
//! parse/transform lane.

#![allow(clippy::disallowed_macros, clippy::disallowed_types)]

use vize_atelier_dom::{
    DomCompilerOptions, compile_template, compile_template_legacy_with_options,
};
use vize_s0::Allocator;

/// `(name, template)` — every directive kind, on the `v-pre` element
/// itself and one level under it.
const CASES: &[(&str, &str)] = &[
    ("self-bind", r#"<div><div v-pre :x="1">c</div></div>"#),
    (
        "self-bind-full",
        r#"<div><div v-pre v-bind:x="1">c</div></div>"#,
    ),
    ("self-on", r#"<div><div v-pre @click="go">c</div></div>"#),
    (
        "self-on-modified",
        r#"<div><div v-pre @click.stop="go">c</div></div>"#,
    ),
    ("self-show", r#"<div><div v-pre v-show="s">c</div></div>"#),
    ("self-html", r#"<div><div v-pre v-html="h">c</div></div>"#),
    ("self-model", r#"<div><input v-pre v-model="m"></div>"#),
    (
        "self-custom",
        r#"<div><div v-pre v-mine:arg.mod="v">c</div></div>"#,
    ),
    ("self-static", r#"<div><div v-pre id="a">c</div></div>"#),
    (
        "self-mixed",
        r#"<div><div v-pre id="a" :x="1" @y="z">c</div></div>"#,
    ),
    ("self-if", r#"<div><div v-pre v-if="a">c</div></div>"#),
    (
        "self-for",
        r#"<div><div v-pre v-for="i in l">c</div></div>"#,
    ),
    (
        "child-bind",
        r#"<div><div v-pre><span :x="1"></span></div></div>"#,
    ),
    (
        "child-interp",
        r#"<div><div v-pre><span>{{ x }}</span></div></div>"#,
    ),
    (
        "child-if",
        r#"<div><div v-pre><span v-if="a">y</span></div></div>"#,
    ),
    (
        "child-if-else",
        r#"<div><div v-pre><i v-if="a">y</i><i v-else>n</i></div></div>"#,
    ),
    (
        "child-for",
        r#"<div><div v-pre><li v-for="i in l">{{ i }}</li></div></div>"#,
    ),
    (
        "child-component",
        r#"<div><div v-pre><MyComp :x="1"></MyComp></div></div>"#,
    ),
    (
        "grandchild",
        r#"<div><div v-pre><span><b :x="1">{{ y }}</b></span></div></div>"#,
    ),
    // A nested spelling is already frozen, so it is an ordinary
    // attribute — only the element that opens the subtree drops its own.
    (
        "nested-v-pre",
        r#"<div><div v-pre><span v-pre></span></div></div>"#,
    ),
    (
        "nested-v-pre-with-bind",
        r#"<div><div v-pre><span v-pre :x="1">c</span></div></div>"#,
    ),
    (
        "sibling-outside",
        r#"<div><div v-pre :x="1"></div><span :y="2"></span></div>"#,
    ),
];

fn render_body(code: &str) -> String {
    code.lines()
        .map(str::trim)
        .skip_while(|line| !line.starts_with("return "))
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn a_v_pre_subtree_compiles_exactly_as_the_legacy_lane_does() {
    for (name, source) in CASES {
        let s2_allocator = Allocator::new();
        let (_, errors, s2) = compile_template(&s2_allocator, source);
        assert!(
            errors.is_empty(),
            "{name}: {source:?} should compile cleanly"
        );

        let legacy_allocator = Allocator::new();
        let (_, _, legacy) = compile_template_legacy_with_options(
            &legacy_allocator,
            source,
            DomCompilerOptions::default(),
        );

        assert_eq!(
            render_body(&s2.code),
            render_body(&legacy.code),
            "{name}: {source:?}",
        );
        assert_eq!(s2.preamble, legacy.preamble, "{name} preamble: {source:?}");
    }
}

/// Two shapes where **both** vize lanes differ from Vue, recorded so the
/// gap is a known one rather than a surprise. Neither is this fix's to
/// close — the fix is about the S2 lane matching the shipped lane — and
/// the two lanes do agree with each other on the first.
///
/// - `<MyComp v-pre :x="1">c</MyComp>`: Vue emits
///   `_createElementVNode("MyComp", { ":x": "1" }, "c")` — under `v-pre`
///   a component tag is not resolved, it stays a plain element. Both
///   vize lanes resolve it, which is what the pin below states.
/// - `<MyComp><template v-pre #head>c</template></MyComp>`: Vue keeps the
///   wrapper, `_createElementVNode("template", { "#head": "" }, […])`.
///   Both vize lanes unwrap it into the default slot, and there they also
///   disagree with each other on hoisting the content, so this one is not
///   in [`CASES`].
#[test]
fn the_component_tag_under_v_pre_is_a_known_gap() {
    let source = r#"<MyComp v-pre :x="1">c</MyComp>"#;
    let allocator = Allocator::new();
    let (_, _, s2) = compile_template(&allocator, source);
    let legacy_allocator = Allocator::new();
    let (_, _, legacy) = compile_template_legacy_with_options(
        &legacy_allocator,
        source,
        DomCompilerOptions::default(),
    );

    // The two lanes agree, which is this fix's contract.
    assert_eq!(render_body(&s2.code), render_body(&legacy.code));
    // And both resolve the component, where Vue would not. If that ever
    // changes, this pin is the place that says so out loud.
    assert_eq!(
        render_body(&s2.code),
        "return (_openBlock(), _createBlock(_component_MyComp, _hoisted_1, { \
default: _withCtx(() => [ _createTextVNode(\"c\") ]), _: 1 /* STABLE */ })) }",
    );
}

/// The frozen names, pinned exactly. Every expectation is
/// `@vue/compiler-dom` 3.6.0-beta.10's own, and the two shapes on the
/// element that *opens* the subtree show the reassembly: the tokenizer
/// had already split `v-bind:x` when `v-pre` took effect, so the name
/// comes back without its separator.
#[test]
fn the_frozen_attributes_keep_their_authored_spelling() {
    let expectations: &[(&str, &str)] = &[
        (
            r#"<div v-pre :x="1"><span></span></div>"#,
            r#"const _hoisted_1 = { ":x": "1" }"#,
        ),
        (
            r#"<div v-pre @click="go"><span></span></div>"#,
            r#"const _hoisted_1 = { "@click": "go" }"#,
        ),
        (
            r#"<div v-pre v-show="s"><span></span></div>"#,
            r#"const _hoisted_1 = { "v-show": "s" }"#,
        ),
        (
            r#"<div v-pre v-bind:x="1"><span></span></div>"#,
            r#"const _hoisted_1 = { "v-bindx": "1" }"#,
        ),
        (
            r#"<div v-pre v-my:a.m="v"><span></span></div>"#,
            r#"const _hoisted_1 = { "v-mya": "v" }"#,
        ),
    ];
    for (source, expected) in expectations {
        let allocator = Allocator::new();
        let (_, _, result) = compile_template(&allocator, source);
        let hoisted = result
            .preamble
            .lines()
            .find(|line| line.starts_with("const _hoisted_1"))
            .unwrap_or("<no hoisted props>");
        assert_eq!(hoisted, *expected, "{source:?}");
    }
}

/// Inside the subtree the authored name survives whole — the tokenizer
/// never split it there.
#[test]
fn a_descendant_keeps_its_separators() {
    let expectations: &[(&str, &str)] = &[
        (
            r#"<div v-pre><span :x="1"></span></div>"#,
            r#"return (_openBlock(), _createElementBlock("div", null, [ _createElementVNode("span", { ":x": "1" }) ])) }"#,
        ),
        (
            r#"<div v-pre><span v-bind:x="1"></span></div>"#,
            r#"return (_openBlock(), _createElementBlock("div", null, [ _createElementVNode("span", { "v-bind:x": "1" }) ])) }"#,
        ),
        (
            r#"<div v-pre><span v-if="a">y</span></div>"#,
            r#"return (_openBlock(), _createElementBlock("div", null, [ _createElementVNode("span", { "v-if": "a" }, "y") ])) }"#,
        ),
        (
            r#"<div v-pre><span v-for="i in l">{{ i }}</span></div>"#,
            r#"return (_openBlock(), _createElementBlock("div", null, [ _createElementVNode("span", { "v-for": "i in l" }, "{{ i }}") ])) }"#,
        ),
    ];
    for (source, expected) in expectations {
        let allocator = Allocator::new();
        let (_, _, result) = compile_template(&allocator, source);
        assert_eq!(render_body(&result.code), *expected, "{source:?}");
    }
}

/// The spelling itself never reaches the output — Vue emits the frozen
/// attributes without it.
#[test]
fn the_v_pre_attribute_itself_is_dropped() {
    let allocator = Allocator::new();
    let (_, _, result) = compile_template(&allocator, r#"<div v-pre :x="1">c</div>"#);
    let hoisted = result
        .preamble
        .lines()
        .find(|line| line.starts_with("const _hoisted_1"))
        .unwrap_or("<no hoisted props>");
    assert_eq!(hoisted, r#"const _hoisted_1 = { ":x": "1" }"#);
    assert_eq!(
        render_body(&result.code),
        r#"return (_openBlock(), _createElementBlock("div", _hoisted_1, "c")) }"#,
    );
}
