//! The first JSX-to-S2 lowering seam.
//!
//! This module deliberately admits only the lossless, local subset needed to
//! establish the S2 representation without inventing fallback semantics.
//! Callers receive a typed refusal for every Relief form whose S2 facts or DOM
//! realization have not landed yet. P2-16 expands the admitted family until
//! this is the authoritative JSX lowering.

use vize_relief::{
    ElementType, ExpressionNode, Namespace as ReliefNamespace, PropNode, RootNode,
    TemplateChildNode,
};
use vize_s0::{Allocator, Box, Vec};
use vize_s1_to_s2::lower::{LoweringFeatures, OpFamily};
use vize_s2::expr::ExprRef;
use vize_s2::op::{
    Attribute, BindOp, BindingOp, ComponentOp, DynamicName, ElementOp, InterpolationOp, Namespace,
    OnOp, Op, Region, TextOp,
};

use self::directives::lower_vue_directive;
use self::model::lower_model;
use self::slots::{has_slot_content, lower_slot_content, slot_template_span};

/// A JSX render root represented as S2 operations.
#[derive(Debug)]
pub struct JsxS2Root<'a> {
    /// The complete JSX/TSX module source backing every S2 span.
    pub source: &'a str,
    /// Render operations in authored order.
    pub root: Region<'a>,
    /// Number of operations, including attached bindings when they land.
    pub op_count: u32,
    /// S2 operation families observed while projecting this JSX root.
    pub features: LoweringFeatures,
}

/// A construct which needs a dedicated S2 lowering and must not be silently
/// projected through the static foundation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum S2Refusal {
    /// A Vue/JSX directive needs its matching S2 binding op and fact channel.
    Directive,
    /// A transformed Relief-only child needs an S2 structural lowering.
    TransformedChild,
    /// A JSX root contains a Relief child not represented by this foundation.
    UnsupportedChild,
    /// An element kind requires its dedicated S2 operation or realization.
    UnsupportedElement,
    /// A Relief expression is compound rather than one authored JS span.
    CompoundExpression,
}

/// Project the already-lowered JSX root into the initial, lossless S2 subset.
///
/// The projection keeps absolute source spans and parses interpolation text via
/// [`ExprRef`]. It intentionally refuses instead of degrading directive,
/// control-flow, slot, or compound-expression semantics.
pub fn try_lower_root<'a>(
    allocator: &'a Allocator,
    source: &'a str,
    root: &RootNode<'a>,
) -> Result<JsxS2Root<'a>, S2Refusal> {
    let mut op_count = 0;
    let mut features = LoweringFeatures::EMPTY;
    let ops = lower_children(allocator, &root.children, &mut op_count, &mut features)?;
    Ok(JsxS2Root {
        source,
        root: Region { ops },
        op_count,
        features,
    })
}

fn lower_children<'a>(
    allocator: &'a Allocator,
    children: &[TemplateChildNode<'a>],
    op_count: &mut u32,
    features: &mut LoweringFeatures,
) -> Result<Vec<'a, Op<'a>>, S2Refusal> {
    let mut ops = Vec::new_in(&allocator);
    for child in children {
        ops.push(lower_child(allocator, child, op_count, features)?);
    }
    Ok(ops)
}

fn lower_child<'a>(
    allocator: &'a Allocator,
    child: &TemplateChildNode<'a>,
    op_count: &mut u32,
    features: &mut LoweringFeatures,
) -> Result<Op<'a>, S2Refusal> {
    *op_count = op_count.saturating_add(1);
    match child {
        TemplateChildNode::Text(text) => Ok(Op::Text(Box::new_in(
            TextOp {
                content: text.content,
                span: text.loc.span,
            },
            &allocator,
        ))),
        TemplateChildNode::Interpolation(interpolation) => {
            let expression = lower_expression(allocator, &interpolation.content)?;
            Ok(Op::Interpolation(Box::new_in(
                InterpolationOp {
                    expression,
                    span: interpolation.loc.span,
                },
                &allocator,
            )))
        }
        TemplateChildNode::Element(element) => {
            lower_element(allocator, element, op_count, features)
        }
        TemplateChildNode::If(_)
        | TemplateChildNode::IfBranch(_)
        | TemplateChildNode::For(_)
        | TemplateChildNode::TextCall(_)
        | TemplateChildNode::CompoundExpression(_)
        | TemplateChildNode::Hoisted(_) => Err(S2Refusal::TransformedChild),
        TemplateChildNode::Comment(_) => Err(S2Refusal::UnsupportedChild),
    }
}

fn lower_element<'a>(
    allocator: &'a Allocator,
    element: &vize_relief::ElementNode<'a>,
    op_count: &mut u32,
    features: &mut LoweringFeatures,
) -> Result<Op<'a>, S2Refusal> {
    let props = lower_props(allocator, &element.props, element.tag_type, features)?;
    *op_count = op_count.saturating_add(props.binding_count);
    let children = Region {
        ops: lower_children(allocator, &element.children, op_count, features)?,
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
            *features = features.observing(OpFamily::SlotCarrier);
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
            *features = features.observing(OpFamily::SlotCarrier);
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

struct LoweredProps<'a> {
    attributes: Vec<'a, Attribute<'a>>,
    bindings: Vec<'a, BindingOp<'a>>,
    binding_count: u32,
}

fn lower_props<'a>(
    allocator: &'a Allocator,
    props: &[PropNode<'a>],
    element_type: ElementType,
    features: &mut LoweringFeatures,
) -> Result<LoweredProps<'a>, S2Refusal> {
    let mut attributes = Vec::new_in(&allocator);
    let mut bindings = Vec::new_in(&allocator);
    for prop in props {
        match prop {
            PropNode::Attribute(attribute) => attributes.push(Attribute {
                name: attribute.name,
                value: attribute.value.as_ref().map(|value| value.content),
                span: attribute.loc.span,
            }),
            PropNode::Directive(directive) => {
                bindings.push(lower_binding(allocator, directive, element_type, features)?);
            }
        }
    }
    let binding_count = bindings.len() as u32;
    Ok(LoweredProps {
        attributes,
        bindings,
        binding_count,
    })
}

fn lower_binding<'a>(
    allocator: &'a Allocator,
    directive: &vize_relief::DirectiveNode<'a>,
    element_type: ElementType,
    features: &mut LoweringFeatures,
) -> Result<BindingOp<'a>, S2Refusal> {
    match directive.name {
        "bind" | "on" => lower_bind_or_on(allocator, directive),
        "model" => lower_model(allocator, directive, element_type, features),
        "show" | "html" | "text" => lower_vue_directive(allocator, directive, element_type),
        "slot" => lower_slot_content(allocator, directive),
        _ => Err(S2Refusal::Directive),
    }
}

fn lower_bind_or_on<'a>(
    allocator: &'a Allocator,
    directive: &vize_relief::DirectiveNode<'a>,
) -> Result<BindingOp<'a>, S2Refusal> {
    let name = lower_dynamic_name(allocator, directive.arg.as_ref())?;
    let modifiers = lower_modifiers(allocator, directive);
    let expression = directive
        .exp
        .as_ref()
        .map(|expression| lower_expression(allocator, expression))
        .transpose()?;

    match directive.name {
        "bind" => Ok(BindingOp::Bind(Box::new_in(
            BindOp {
                name,
                modifiers,
                value: expression,
                span: directive.loc.span,
            },
            &allocator,
        ))),
        "on" => Ok(BindingOp::On(Box::new_in(
            OnOp {
                name,
                modifiers,
                handler: expression,
                span: directive.loc.span,
            },
            &allocator,
        ))),
        _ => unreachable!("directive name was admitted above"),
    }
}

fn lower_modifiers<'a>(
    allocator: &'a Allocator,
    directive: &vize_relief::DirectiveNode<'a>,
) -> Vec<'a, &'a str> {
    let mut modifiers = Vec::new_in(&allocator);
    for modifier in &directive.modifiers {
        modifiers.push(modifier.content);
    }
    modifiers
}

pub(super) fn lower_dynamic_name<'a>(
    allocator: &'a Allocator,
    name: Option<&ExpressionNode<'a>>,
) -> Result<Option<DynamicName<'a>>, S2Refusal> {
    let Some(name) = name else {
        return Ok(None);
    };
    match name {
        ExpressionNode::Simple(simple) if simple.is_static => {
            Ok(Some(DynamicName::Static(simple.content)))
        }
        ExpressionNode::Simple(simple) => Ok(Some(DynamicName::Dynamic(ExprRef::parse_js_in(
            allocator,
            simple.content,
            simple.loc.span,
        )))),
        ExpressionNode::Compound(_) => Err(S2Refusal::CompoundExpression),
    }
}

pub(super) fn lower_expression<'a>(
    allocator: &'a Allocator,
    expression: &ExpressionNode<'a>,
) -> Result<ExprRef<'a>, S2Refusal> {
    match expression {
        ExpressionNode::Simple(simple) => Ok(ExprRef::parse_js_in(
            allocator,
            simple.content,
            simple.loc.span,
        )),
        ExpressionNode::Compound(_) => Err(S2Refusal::CompoundExpression),
    }
}

const fn namespace(namespace: ReliefNamespace) -> Namespace {
    match namespace {
        ReliefNamespace::Html => Namespace::Html,
        ReliefNamespace::Svg => Namespace::Svg,
        ReliefNamespace::MathMl => Namespace::MathMl,
    }
}

#[cfg(test)]
mod tests;

mod directives;
mod model;
mod slots;
