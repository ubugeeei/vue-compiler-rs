use vize_relief::{ElementNode, ElementType, ExpressionNode, PropNode};

pub(super) fn allows_text_input_model(element: &ElementNode<'_>) -> bool {
    matches!(element.tag_type, ElementType::Element)
        && element.tag == "input"
        && element.props.iter().all(preserves_text_input_model)
}

fn preserves_text_input_model(prop: &PropNode<'_>) -> bool {
    match prop {
        PropNode::Attribute(attribute) if attribute.name == "type" => {
            matches!(attribute.value.as_ref(), Some(value) if value.content == "text")
        }
        PropNode::Directive(directive) if directive.name == "bind" => {
            matches!(directive.arg.as_ref(), Some(ExpressionNode::Simple(simple))
                if simple.is_static && simple.content != "type")
        }
        _ => true,
    }
}
