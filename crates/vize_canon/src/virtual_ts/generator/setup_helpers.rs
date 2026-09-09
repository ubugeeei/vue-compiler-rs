//! Setup-scope compiler macro helper emission.
//!
//! Generic SFCs need a narrower `defineProps<T>()` boolean-prop model than the
//! shared helper can express safely. The shared conditional boolean-key helper
//! is intentionally left in place for non-generic SFCs, while this module uses
//! the parsed OXC type AST to pass only concrete local boolean keys for generic
//! setup scopes.

use vize_carton::{CompactString, FxHashSet, String, append};
use vize_croquis::Croquis;
use vize_relief::RootNode;

use crate::virtual_ts::helpers::{VUE_SETUP_HELPERS, VUE_SETUP_HELPERS_HOISTED};

mod boolean_keys;
mod template_ref_registry;

use boolean_keys::{DefinePropsBooleanKeys, collect_define_props_boolean_keys};
use template_ref_registry::template_ref_registry;

pub(super) struct SetupHelperComponentContext<'a> {
    pub(super) summary: &'a Croquis,
    pub(super) options: &'a crate::virtual_ts::types::VirtualTsOptions,
    pub(super) syntactic_type_only_imported_names: &'a FxHashSet<CompactString>,
}

pub(super) fn emit_setup_helpers(
    ts: &mut String,
    component_context: SetupHelperComponentContext<'_>,
    script_content: Option<&str>,
    generic_param: Option<&str>,
    hoist_shared_preamble: bool,
    template_ast: Option<&RootNode<'_>>,
) {
    // Static `ref="name"` attributes on plain elements, keyed for
    // `useTemplateRef` (#3896): the registry exists only to retype this
    // scope's shim, so it is collected here rather than by the caller.
    let registry = template_ref_registry(
        component_context.summary,
        component_context.options,
        script_content,
        template_ast,
        component_context.syntactic_type_only_imported_names,
    );
    let template_refs = registry.as_ref().map(|registry| registry.body.as_str());
    if let Some(registry) = registry.as_ref() {
        let dom_ref_helper = if registry.includes_dom_element {
            "  type __VizeDomElement<_Tag extends string, _Svg extends boolean = false> = _Svg extends true ? (_Tag extends keyof SVGElementTagNameMap ? SVGElementTagNameMap[_Tag] : Element) : (_Tag extends keyof HTMLElementTagNameMap ? HTMLElementTagNameMap[_Tag] : Element);\n"
        } else {
            ""
        };
        let component_ref_helper = if registry.includes_component {
            "  type __VizeTemplateComponentRef<_C> = _C extends abstract new (...args: any[]) => infer _I ? _I : any;\n"
        } else {
            ""
        };
        // `NativeElements` maps tags to their *props* for template checking;
        // a template ref holds the mounted DOM node, so the registry resolves
        // through the DOM tag-name maps instead (#3896).
        //
        // `_Svg` selects the map, and neither branch falls back to the other.
        // The two overlap on `a`, `script`, `style` and `title`, so an
        // HTML-first lookup would pin `<svg><a ref="link" /></svg>` to
        // `HTMLAnchorElement`, whose `href` is a `string` where `SVGAElement`
        // has an `SVGAnimatedString`; symmetrically, an SVG-map fallback in the
        // HTML branch would hand an element the parser placed in the HTML
        // namespace an SVG interface it cannot have (a custom renderer forces
        // HTML even for SVG tag names). A tag missing from its own map stops at
        // `Element`, which is what a custom element resolves to.
        let registry_body = registry.body.as_str();
        append!(
            *ts,
            "{dom_ref_helper}{component_ref_helper}  type __VizeTemplateRefs = {{{registry_body}}};\n  type __VizeUseTemplateRef = {{ <_K extends string>(_key: _K): Readonly<import('vue').ShallowRef<(_K extends keyof __VizeTemplateRefs ? __VizeTemplateRefs[_K] : any) | null>>; <_T>(_key: string): Readonly<import('vue').ShallowRef<_T | null>>; }};\n"
        );
    }
    let shims_start = ts.len();
    if generic_param.is_none() {
        ts.push_str(if hoist_shared_preamble {
            VUE_SETUP_HELPERS_HOISTED
        } else {
            VUE_SETUP_HELPERS
        });
        retype_use_template_ref(ts, shims_start, template_refs);
        return;
    }

    let Some(boolean_keys) = script_content.and_then(collect_define_props_boolean_keys) else {
        ts.push_str(if hoist_shared_preamble {
            VUE_SETUP_HELPERS_HOISTED
        } else {
            VUE_SETUP_HELPERS
        });
        retype_use_template_ref(ts, shims_start, template_refs);
        return;
    };
    emit_define_props_boolean_keys_type(ts, &boolean_keys);
    let shims_start = ts.len();
    if hoist_shared_preamble {
        emit_hoisted_setup_helpers(ts);
    } else {
        emit_embedded_setup_helpers(ts);
    }
    retype_use_template_ref(ts, shims_start, template_refs);
}

/// Swap the untyped `useTemplateRef` shim for one keyed by the template's
/// static ref registry (#3896). A keyed call resolves the registered element
/// (making `.value` `| null`-checked exactly like vue-tsc); an unregistered
/// key stays `any`, and an explicit type argument keeps the second call
/// signature. When no registry exists, the shims are left untouched.
fn retype_use_template_ref(ts: &mut String, shims_start: usize, template_refs: Option<&str>) {
    if template_refs.is_none() {
        return;
    }
    const ALIAS_SHIM: &str = "  const useTemplateRef = __vize_useTemplateRef;";
    const ALIAS_TYPED: &str =
        "  const useTemplateRef = __vize_useTemplateRef as unknown as __VizeUseTemplateRef;";
    const EMBEDDED_SHIM: &str = "  function useTemplateRef<_T = any>(_key: string): __ShallowRef<_T | null> { void _key; return undefined as unknown as __ShallowRef<_T | null>; }";
    const EMBEDDED_TYPED: &str = "  const useTemplateRef = (undefined as unknown as __VizeUseTemplateRef); void ((_key: string) => useTemplateRef(_key));";
    let tail = ts[shims_start..]
        .replace(ALIAS_SHIM, ALIAS_TYPED)
        .replace(EMBEDDED_SHIM, EMBEDDED_TYPED);
    ts.truncate(shims_start);
    ts.push_str(&tail);
}

fn emit_define_props_boolean_keys_type(ts: &mut String, collection: &DefinePropsBooleanKeys) {
    if collection.keys.is_empty() && !collection.has_unresolved_references {
        ts.push_str("  type __VizeDefinePropsBooleanKeys<_T> = never;\n");
        return;
    }

    ts.push_str("  type __VizeDefinePropsBooleanKeys<_T> =\n");
    if collection.has_unresolved_references {
        ts.push_str("    __VizeBooleanKey<_T>\n");
    }
    for (index, key) in collection.keys.iter().enumerate() {
        let separator = if index == 0 && !collection.has_unresolved_references {
            "    "
        } else {
            "  | "
        };
        let mut key_literal = String::default();
        push_ts_string_literal(&mut key_literal, key.as_str());
        append!(
            *ts,
            "{separator}(_T extends {{ {key_literal}?: boolean | undefined }} ? {key_literal} : never)\n"
        );
    }
    ts.push_str("  ;\n");
}

fn emit_hoisted_setup_helpers(ts: &mut String) {
    ts.push_str(
        r#"  // Compiler macros (setup-scope only; signatures hoisted to the shared helpers file)
  const defineProps = __vize_defineProps as {
    <_T = unknown>(): __DefineProps<__LooseRequired<_T>, Extract<__VizeDefinePropsBooleanKeys<_T>, keyof __LooseRequired<_T>>>;
    <const _T extends readonly string[]>(_props: _T): { [K in _T[number]]?: any };
    <const _T extends Record<string, any>>(_props: _T): __RuntimePropShape<_T>;
  };
  const defineEmits = __vize_defineEmits;
  const defineExpose = __vize_defineExpose;
  const defineModel = __vize_defineModel;
  const defineSlots = __vize_defineSlots;
  const withDefaults = __vize_withDefaults;
  const useTemplateRef = __vize_useTemplateRef;
  // Mark compiler macros as used
  void defineProps; void defineEmits; void defineExpose; void defineModel; void defineSlots; void withDefaults; void useTemplateRef;"#,
    );
}

fn emit_embedded_setup_helpers(ts: &mut String) {
    ts.push_str(
        r#"  // Compiler macros (only valid in setup scope, not global)
  function defineProps<_T = unknown>(): __DefineProps<__LooseRequired<_T>, Extract<__VizeDefinePropsBooleanKeys<_T>, keyof __LooseRequired<_T>>>;
  function defineProps<const _T extends readonly string[]>(_props: _T): { [K in _T[number]]?: any };
  function defineProps<const _T extends Record<string, any>>(_props: _T): __RuntimePropShape<_T>;
  function defineProps(_props?: any) { void _props; return undefined as any; }
  function defineEmits<_T = unknown>(): __EmitFn<_T>;
  function defineEmits<const _T extends readonly string[]>(_events: _T): (event: _T[number], ...args: any[]) => void;
  function defineEmits<const _T extends Record<string, any>>(_events: _T): __EmitFn<_T>;
  function defineEmits(_events?: any) { void _events; return (() => {}) as any; }
  function defineExpose<_T = unknown>(_exposed?: _T): void { void _exposed; }
  function defineModel<_T = unknown, _M extends PropertyKey = string, _G = _T, _S = _T>(): __VizeModelRef<_T | undefined, _M, _G | undefined, _S | undefined>;
  function defineModel<_T = unknown, _M extends PropertyKey = string, _O extends Record<string, any> = Record<string, any>, _V = __VizeModelOptionValue<_T, _O>>(_options: _O): __VizeModelRef<_V, _M, _V, _V>;
  function defineModel<_T = unknown, _M extends PropertyKey = string, _G = _T, _S = _T>(_options: any): __VizeModelRef<_T, _M, _G, _S>;
  function defineModel<_T = unknown, _M extends PropertyKey = string, _O extends Record<string, any> = Record<string, any>, _V = __VizeModelOptionValue<_T, _O>>(_name: string, _options: _O): __VizeModelRef<_V, _M, _V, _V>;
  function defineModel<_T = unknown, _M extends PropertyKey = string, _G = _T, _S = _T>(_name: string, _options?: any): __VizeModelRef<_T, _M, _G, _S>;
  function defineModel(_name_or_options?: any, _options?: any) { void _name_or_options; void _options; return undefined as any; }
  function defineSlots<_T = unknown>(): _T { return undefined as unknown as _T; }
  function withDefaults<_T, _BKeys extends keyof _T, _D extends __WithDefaultsArgs<_T>>(_props: __DefineProps<_T, _BKeys>, _defaults: _D): __WithDefaultsResult<_T, _D, _BKeys>; function withDefaults(_props: any, _defaults: any) { void _props; void _defaults; return undefined as any; }
  function useTemplateRef<_T = any>(_key: string): __ShallowRef<_T | null> { void _key; return undefined as unknown as __ShallowRef<_T | null>; }
  // Mark compiler macros as used
  void defineProps; void defineEmits; void defineExpose; void defineModel; void defineSlots; void withDefaults; void useTemplateRef;"#,
    );
}

fn push_ts_string_literal(out: &mut String, value: &str) {
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(ch),
        }
    }
    out.push('"');
}
