use vize_relief::{ElementNode, ElementType, Namespace as ReliefNamespace};
use vize_s0::{Allocator, Box};
use vize_s1_to_s2::lower::OpFamily;
use vize_s2::op::{ComponentOp, ElementOp, Namespace, Op, Region};

use super::native_model::native_model_kind;
use super::slots::{has_slot_content, slot_template_span};
use super::{ProjectCx, S2Refusal, lower_children, lower_props};

pub(super) fn lower_element<'a>(
    allocator: &'a Allocator,
    source: &'a str,
    element: &ElementNode<'a>,
    cx: &mut ProjectCx,
) -> Result<Op<'a>, S2Refusal> {
    let native_element_model_kind = native_model_kind(element);
    let props = lower_props(
        allocator,
        &element.props,
        element.tag_type,
        native_element_model_kind,
        cx,
    )?;
    cx.skip_ops(props.binding_count);
    let children = Region {
        ops: lower_children(allocator, source, &element.children, cx)?,
    };
    match element.tag_type {
        ElementType::Element => Ok(Op::Element(Box::new_in(
            ElementOp {
                tag: element.tag,
                namespace: namespace(element.ns),
                attributes: props.attributes,
                bindings: props.bindings,
                children,
                span: element.loc.span,
            },
            &allocator,
        ))),
        ElementType::Component => {
            cx.observe(OpFamily::SlotCarrier);
            Ok(Op::Component(Box::new_in(
                ComponentOp {
                    name: element.tag,
                    attributes: props.attributes,
                    bindings: props.bindings,
                    children,
                    span: element.loc.span,
                },
                &allocator,
            )))
        }
        ElementType::Template if has_slot_content(&props.bindings) => {
            cx.observe(OpFamily::SlotCarrier);
            let span = slot_template_span(element.loc.span, &props.bindings);
            Ok(Op::Element(Box::new_in(
                ElementOp {
                    tag: element.tag,
                    namespace: namespace(element.ns),
                    attributes: props.attributes,
                    bindings: props.bindings,
                    children,
                    span,
                },
                &allocator,
            )))
        }
        ElementType::Slot | ElementType::Template => Err(S2Refusal::UnsupportedElement),
    }
}

const fn namespace(namespace: ReliefNamespace) -> Namespace {
    match namespace {
        ReliefNamespace::Html => Namespace::Html,
        ReliefNamespace::Svg => Namespace::Svg,
        ReliefNamespace::MathMl => Namespace::MathMl,
    }
}
