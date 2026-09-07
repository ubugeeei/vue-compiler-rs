//! JSX VDOM bridge for the P2-16 S2 re-targeting slice.

use vize_s0::{Allocator, String};
use vize_s1_to_s2::lower::LoweringFeatures;
use vize_s1_to_s2::pass::S2Facts;
use vize_s1_to_s2::{
    DomEmitMode, DomEmitOptions, LegacyCaps, Lowered as S2Lowered, emit_dom_with_options,
};
use vize_s2::op::{BindingOp, ComponentOp, DynamicName, Op, Region};

use crate::s2::{JsxS2Root, S2Refusal};

use super::{VdomCompatOptions, VdomCompileOptions};

pub(super) struct S2VdomEmit {
    pub code: String,
    pub preamble: String,
}

pub(super) fn try_emit_s2_vdom<'a>(
    allocator: &'a Allocator,
    s2: Result<JsxS2Root<'a>, S2Refusal>,
    is_ts: bool,
    component_name: Option<&str>,
    scope_id: Option<&str>,
    options: &VdomCompileOptions,
    compat: &VdomCompatOptions<'_>,
) -> Option<S2VdomEmit> {
    if !compat.is_native_s2_surface()
        || options.source_map
        || options.hoist_static
        || options.cache_handlers
        || is_ts
    {
        return None;
    }

    let s2 = s2.ok()?;
    if !root_is_supported(&s2) {
        return None;
    }

    let lowered = S2Lowered {
        allocator,
        source: s2.source,
        root: s2.root,
        op_count: s2.op_count,
        diagnostics: Default::default(),
        provenance: Default::default(),
        scopes: Default::default(),
        texts: Default::default(),
        wrappers: Default::default(),
        for_wrappers: Default::default(),
        features: LoweringFeatures::EMPTY,
        caps: LegacyCaps::VUE3,
    };
    let facts = S2Facts::default();
    let emit = emit_dom_with_options(
        &lowered,
        &facts,
        &DomEmitOptions {
            mode: DomEmitMode::Module,
            runtime_module_name: "vue",
            runtime_global_name: "Vue",
            prefix_identifiers: false,
            hoist_static: false,
            inline: false,
            component_name,
            cache_handlers: false,
            hoisted_scope_id: None,
            scope_id,
            is_ts: false,
            comments: false,
            experimental_in_tag_comments: false,
            custom_element_patterns: &[],
            custom_element_predicate: None,
            bindings: None,
        },
    )
    .ok()?;

    Some(S2VdomEmit {
        code: emit.code,
        preamble: emit.preamble,
    })
}

impl VdomCompatOptions<'_> {
    fn is_native_s2_surface(&self) -> bool {
        self.transform_on_helper.is_none()
            && self.object_slots_helpers.is_none()
            && self.vnode_factory.is_none()
            && self.merge_props
            && !self.allow_static_v_model_arg_on_element
            && self.custom_element_spans.is_empty()
    }
}

fn root_is_supported(root: &JsxS2Root<'_>) -> bool {
    region_is_supported(&root.root)
}

fn region_is_supported(region: &Region<'_>) -> bool {
    region.ops.iter().all(op_is_supported)
}

fn op_is_supported(op: &Op<'_>) -> bool {
    match op {
        Op::Text(_) | Op::Interpolation(_) => true,
        Op::Element(element) => {
            bindings_are_supported(&element.bindings) && region_is_supported(&element.children)
        }
        Op::Component(component) => component_is_supported(component),
        Op::Comment(_) | Op::If(_) | Op::For(_) | Op::Slot(_) => false,
    }
}

fn bindings_are_supported(bindings: &[BindingOp<'_>]) -> bool {
    bindings.iter().all(|binding| {
        matches!(
            binding,
            BindingOp::Bind(_) | BindingOp::On(_) | BindingOp::VueShow(_)
        )
    })
}

fn component_is_supported(component: &ComponentOp<'_>) -> bool {
    let dynamic_component = component_is_dynamic(component);
    component.children.ops.is_empty()
        && (dynamic_component || !component_name_needs_component_semantics(component.name))
        && component
            .bindings
            .iter()
            .all(|binding| component_binding_is_supported(binding, dynamic_component))
}

fn component_is_dynamic(component: &ComponentOp<'_>) -> bool {
    matches!(component.name, "component" | "Component")
        && (component.attributes.iter().any(|attr| attr.name == "is")
            || component.bindings.iter().any(binding_is_is))
}

fn component_name_needs_component_semantics(name: &str) -> bool {
    matches!(
        name,
        "component"
            | "Component"
            | "Teleport"
            | "teleport"
            | "Suspense"
            | "suspense"
            | "KeepAlive"
            | "keep-alive"
            | "BaseTransition"
            | "base-transition"
            | "Transition"
            | "transition"
            | "TransitionGroup"
            | "transition-group"
    ) || name.contains('.')
}

fn binding_is_is(binding: &BindingOp<'_>) -> bool {
    matches!(
        binding,
        BindingOp::Bind(bind) if matches!(bind.name, Some(DynamicName::Static("is")))
    )
}

fn component_binding_is_supported(binding: &BindingOp<'_>, dynamic_component: bool) -> bool {
    match binding {
        BindingOp::Bind(bind) if bind.name.is_none() => {
            bind.value.is_some() && bind.modifiers.is_empty()
        }
        BindingOp::Bind(bind) if binding_is_is(binding) => {
            dynamic_component && bind.value.is_some() && bind.modifiers.is_empty()
        }
        BindingOp::Bind(bind) => {
            bind.value.is_some()
                && bind.modifiers.is_empty()
                && matches!(
                    bind.name,
                    Some(DynamicName::Static(name))
                        if !matches!(name, "is" | "key" | "ref")
                )
        }
        BindingOp::On(on) => {
            on.handler.is_some()
                && on.modifiers.is_empty()
                && matches!(on.name, Some(DynamicName::Static(_)))
        }
        BindingOp::VueShow(_) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests;
