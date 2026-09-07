use vize_relief::{ElementNode, ElementType, ExpressionNode, PropNode};

pub(super) fn native_text_model_kind<'a>(element: &ElementNode<'a>) -> Option<&'a str> {
    if !matches!(element.tag_type, ElementType::Element) {
        return None;
    }
    match element.tag {
        "input" if element.props.iter().all(preserves_text_input_model) => Some("input"),
        "textarea" => Some("textarea"),
        _ => None,
    }
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
