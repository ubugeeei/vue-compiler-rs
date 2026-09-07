//! Tests for template compilation utilities.

mod string_tracking;

use super::extraction::{extract_template_parts, extract_template_parts_full};
use super::vapor::{add_scope_id_to_template, transform_vapor_template_output};
use crate::types::{BlockLocation, SfcTemplateBlock};
use std::borrow::Cow;

#[test]
fn test_add_scope_id_to_template() {
    let input = r#"const t0 = _template("<div class='container'>Hello</div>")"#;
    let result = add_scope_id_to_template(input, "data-v-abc123");
    insta::assert_snapshot!(result.as_str());
}

#[test]
fn test_transform_vapor_template_output_current_render_format() {
    let template = SfcTemplateBlock {
        content: Cow::Borrowed("<div>{{ msg }}</div>"),
        loc: BlockLocation {
            start: 0,
            end: 0,
            tag_start: 0,
            tag_end: 0,
            start_line: 1,
            start_column: 1,
            end_line: 1,
            end_column: 1,
        },
        lang: None,
        src: None,
        attrs: Default::default(),
    };

    let vapor_code = r#"import { template as _template } from 'vue/vapor';
const t0 = _template("<div> </div>", true)

export function render(_ctx) {
  const n0 = t0()
  return n0
}"#;

    let result = transform_vapor_template_output(vapor_code, None, &template, None, "vue")
        .expect("current Vapor output should be transformed");
    insta::assert_snapshot!(result.as_str());
}

#[test]
fn test_extract_template_parts_full_brace_in_string() {
    let template_code = r#"import { toDisplayString as _toDisplayString } from 'vue'

export function render(_ctx, _cache) {
  return _toDisplayString(isArray.value ? ']' : '}')
}"#;

    let (imports, _hoisted, render_fn, render_fn_name) = extract_template_parts_full(template_code);

    assert_eq!(render_fn_name, "render");
    insta::assert_debug_snapshot!((&imports, &render_fn));
    let trimmed = render_fn.trim();
    assert!(
        trimmed.ends_with('}'),
        "Render function should end with closing brace. Got:\n{}",
        render_fn
    );
}

#[test]
fn test_extract_template_parts_basic() {
    let template_code = r#"import { createVNode as _createVNode } from 'vue'

const _hoisted_1 = { class: "test" }

export function render(_ctx, _cache) {
  return _createVNode("div", _hoisted_1, "Hello")
}"#;

    let (imports, hoisted, _preamble, render_body, render_fn_name) =
        extract_template_parts(template_code);

    assert_eq!(render_fn_name, "render");
    insta::assert_debug_snapshot!((&imports, &hoisted, &render_body));
}

#[test]
fn test_extract_template_parts_vapor_template_declarations() {
    let template_code = r#"import { template as _template, renderEffect as _renderEffect, setText as _setText } from 'vue'

const t0 = _template("<div> </div>", true)

function render(_ctx, $props, $emit, $attrs, $slots) {
  const n0 = t0()
  _renderEffect(() => _setText(n0, _ctx.msg))
  return n0
}"#;

    let (_imports, hoisted, _preamble, render_body, render_fn_name) =
        extract_template_parts(template_code);

    assert_eq!(render_fn_name, "render");
    insta::assert_debug_snapshot!((&hoisted, &render_body));
}

#[test]
fn test_extract_template_parts_full_preserves_vapor_top_level_side_effects() {
    let template_code = r#"import { delegateEvents as _delegateEvents, template as _template } from 'vue'

const t0 = _template("<button>ok</button>", true)
_delegateEvents("click")

function render(_ctx, $props, $emit, $attrs, $slots) {
  const n0 = t0()
  return n0
}"#;

    let (_imports, hoisted, render_fn, render_fn_name) = extract_template_parts_full(template_code);

    assert_eq!(render_fn_name, "render");
    insta::assert_debug_snapshot!((&hoisted, &render_fn));
}

#[test]
fn test_extract_template_parts_full_ssr_render_function() {
    let template_code = r#"import { ssrRenderComponent as _ssrRenderComponent } from "vue/server-renderer"

export function ssrRender(_ctx, _push, _parent, _attrs) {
  _push(_ssrRenderComponent(_ctx.Foo, null, null, _parent))
}"#;

    let (imports, _hoisted, render_fn, render_fn_name) = extract_template_parts_full(template_code);

    assert_eq!(render_fn_name, "ssrRender");
    insta::assert_debug_snapshot!((&imports, &render_fn));
}

#[test]
fn test_extract_template_parts_ssr_preserves_render_name_without_inline_body() {
    let template_code = r#"import { ssrRenderComponent as _ssrRenderComponent } from "vue/server-renderer"

export function ssrRender(_ctx, _push, _parent, _attrs) {
  _push(_ssrRenderComponent(_ctx.Foo, null, null, _parent))
}"#;

    let (_imports, _hoisted, _preamble, render_body, render_fn_name) =
        extract_template_parts(template_code);

    assert_eq!(render_fn_name, "ssrRender");
    assert!(
        render_body.is_empty(),
        "SSR render functions should stay separate instead of being inlined. Got:\n{}",
        render_body
    );
}

// --- Multiline template literal tests ---

#[test]
fn test_extract_template_parts_multiline_template_literal() {
    let template_code = r#"import { openBlock as _openBlock, createElementBlock as _createElementBlock, toDisplayString as _toDisplayString, createCommentVNode as _createCommentVNode } from "vue"

export function render(_ctx, _cache, $props, $setup, $data, $options) {
  return (show.value)
    ? (_openBlock(), _createElementBlock("div", {
      key: 0,
      class: "outer"
    }, [
      _createElementVNode("div", { class: "inner" }, [
        (ver.value)
          ? (_openBlock(), _createElementBlock("span", { key: 0 }, "\n        " + _toDisplayString(`${t("key")}: v${ver.value.major}.${
            ver.value.minor
          }`) + "\n      ", 1 /* TEXT */))
          : (_openBlock(), _createElementBlock("span", { key: 1 }, "no"))
      ])
    ]))
    : _createCommentVNode("v-if", true)
}"#;

    let (_imports, _hoisted, _preamble, render_body, _render_fn_name) =
        extract_template_parts(template_code);

    insta::assert_snapshot!(render_body.as_str());
}

#[test]
fn test_extract_template_parts_full_multiline_template_literal() {
    let template_code = r#"import { toDisplayString as _toDisplayString } from 'vue'

export function render(_ctx, _cache) {
  return _toDisplayString(`${t("key")}: v${ver.major}.${
    ver.minor
  }`)
}"#;

    let (_imports, _hoisted, render_fn, _render_fn_name) =
        extract_template_parts_full(template_code);

    insta::assert_snapshot!(render_fn.as_str());
    let trimmed = render_fn.trim();
    assert!(
        trimmed.ends_with('}'),
        "Render function should end with closing brace. Got:\n{}",
        render_fn
    );
}

// --- Deeply nested template literal extraction tests ---

#[test]
fn test_extract_template_parts_deeply_nested_multiline() {
    let template_code = r#"import { toDisplayString as _toDisplayString, createElementBlock as _createElementBlock, openBlock as _openBlock, createCommentVNode as _createCommentVNode, createElementVNode as _createElementVNode } from "vue"

export function render(_ctx, _cache, $props, $setup, $data, $options) {
  return (cond.value)
    ? (_openBlock(), _createElementBlock("div", { key: 0 }, [
        _createElementVNode("p", null, _toDisplayString(`${items.value.map(x => ({
          name: x.name,
          label: `${x.prefix}-${
            x.suffix
          }`
        })).length} items`)),
        _createElementVNode("span", null, "after")
      ]))
    : _createCommentVNode("v-if", true)
}"#;

    let (_imports, _hoisted, _preamble, render_body, _render_fn_name) =
        extract_template_parts(template_code);

    insta::assert_snapshot!(render_body.as_str());
}

#[test]
fn test_extract_template_parts_full_deeply_nested_multiline() {
    let template_code = r#"import { toDisplayString as _toDisplayString } from "vue"

export function render(_ctx, _cache) {
  return _toDisplayString(`${items.map(x => ({
    name: x.name,
    value: `nested-${
      x.value
    }`
  })).length} items`)
}"#;

    let (_imports, _hoisted, render_fn, _render_fn_name) =
        extract_template_parts_full(template_code);

    let trimmed = render_fn.trim();
    assert!(
        trimmed.ends_with('}'),
        "Render function should end with closing brace. Got:\n{}",
        render_fn
    );
    insta::assert_snapshot!(render_fn.as_str());
}

/// Regression for the `<script setup>` inline path truncating a multi-line hoisted
/// object literal at its first newline (producing invalid JS). The whole declaration
/// must be collected into `hoisted` intact, with balanced braces.
#[test]
fn test_extract_template_parts_multiline_hoisted_object() {
    let template_code = r#"import { createElementVNode as _createElementVNode } from "vue"

const _hoisted_1 = { style: {
  position: 'absolute',
  top: 0,
  left: 0,
  objectFit: 'cover',
} }

export function render(_ctx, _cache) {
  return _createElementVNode("img", _hoisted_1)
}"#;

    let (_imports, hoisted, _preamble, render_body, render_fn_name) =
        extract_template_parts(template_code);

    assert_eq!(render_fn_name, "render");
    assert!(
        hoisted.contains("objectFit: 'cover'"),
        "continuation lines of the hoisted const must be preserved, got:\n{hoisted}"
    );
    let opens = hoisted.matches('{').count();
    let closes = hoisted.matches('}').count();
    assert_eq!(opens, closes, "hoisted braces must be balanced:\n{hoisted}");
    assert!(
        render_body.contains("_hoisted_1"),
        "render body should still reference the hoist"
    );
}

/// Same regression for the vapor/ssr extraction path.
#[test]
fn test_extract_template_parts_full_multiline_hoisted_object() {
    let template_code = r#"import { createElementVNode as _createElementVNode } from "vue"

const _hoisted_1 = { style: {
  position: 'absolute',
  objectFit: 'cover',
} }

export function render(_ctx, _cache) {
  return _createElementVNode("img", _hoisted_1)
}"#;

    let (_imports, hoisted, render_fn, render_fn_name) = extract_template_parts_full(template_code);

    assert_eq!(render_fn_name, "render");
    assert!(
        hoisted.contains("objectFit: 'cover'"),
        "continuation lines of the hoisted const must be preserved, got:\n{hoisted}"
    );
    let opens = hoisted.matches('{').count();
    let closes = hoisted.matches('}').count();
    assert_eq!(opens, closes, "hoisted braces must be balanced:\n{hoisted}");
    assert!(
        render_fn.trim().ends_with('}'),
        "render function should remain intact:\n{render_fn}"
    );
}
