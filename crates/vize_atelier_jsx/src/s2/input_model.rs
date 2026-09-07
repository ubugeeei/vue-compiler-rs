use vize_relief::{ElementNode, ElementType, ExpressionNode, PropNode};

pub(super) fn allows_plain_input_model(element: &ElementNode<'_>) -> bool {
    matches!(element.tag_type, ElementType::Element)
        && element.tag == "input"
        && !element.props.iter().any(blocks_plain_input_model)
}

fn blocks_plain_input_model(prop: &PropNode<'_>) -> bool {
    matches!(prop, PropNode::Attribute(attribute) if attribute.name == "type")
        || matches!(prop, PropNode::Directive(directive)
            if directive.name == "bind"
                && !matches!(directive.arg.as_ref(), Some(ExpressionNode::Simple(simple))
                    if simple.is_static && simple.content != "type"))
}
