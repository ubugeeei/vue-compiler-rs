use vize_relief::{DirectiveNode, ElementNode, ElementType, ExpressionNode, PropNode};

pub(super) fn native_model_kind<'a>(element: &ElementNode<'a>) -> Option<&'a str> {
    if !matches!(element.tag_type, ElementType::Element) {
        return None;
    }
    match element.tag {
        "input" if input_model_is_admitted(element) => Some("input"),
        "select" | "textarea" => Some(element.tag),
        _ => None,
    }
}

fn input_model_is_admitted(element: &ElementNode<'_>) -> bool {
    let mut static_type = None;
    for prop in &element.props {
        match prop {
            PropNode::Attribute(attribute) if attribute.name == "type" => {
                let Some(value) = attribute.value.as_ref().map(|value| value.content) else {
                    return false;
                };
                if !input_type_is_admitted(value) || static_type.is_some_and(|seen| seen != value) {
                    return false;
                }
                static_type = Some(value);
            }
            PropNode::Directive(directive)
                if directive.name == "bind" && !bind_preserves_input_type(directive) =>
            {
                return false;
            }
            _ => {}
        }
    }
    true
}

fn bind_preserves_input_type(directive: &DirectiveNode<'_>) -> bool {
    matches!(directive.arg.as_ref(), Some(ExpressionNode::Simple(simple))
        if simple.is_static && simple.content != "type")
}

fn input_type_is_admitted(input_type: &str) -> bool {
    matches!(input_type, "checkbox" | "radio" | "text")
}
