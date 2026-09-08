use vize_carton::String;
use vize_croquis::EventHandlerScopeData;
use vize_relief::{ElementNode, PropNode, RootNode, TemplateChildNode};

use crate::virtual_ts::helpers::is_known_dom_event_name;

use super::super::handler_shape::{inline_callback_event_argument, is_callable_handler_reference};

pub(super) fn needs_typed_handler_assignment(data: &EventHandlerScopeData) -> bool {
    data.handler_expression.as_ref().is_some_and(|content| {
        (data.has_implicit_event && is_callable_handler_reference(content.as_str()))
            || inline_callback_event_argument(content.as_str()).is_some()
    })
}

pub(super) fn transition_hook_signature(
    template_source: Option<&str>,
    template_ast: Option<&RootNode<'_>>,
    directive_start: u32,
    event_name: &str,
) -> Option<(&'static str, &'static str)> {
    if !is_transition_hook(event_name)
        || !event_belongs_to_transition(template_source, template_ast, directive_start)
    {
        return None;
    }

    let args = match event_name {
        "enter" | "leave" | "appear" => "[el: Element, done: () => void]",
        _ => "[el: Element]",
    };
    Some(("Element", args))
}

pub(super) fn dynamic_component_custom_event(
    template_source: Option<&str>,
    template_ast: Option<&RootNode<'_>>,
    directive_start: u32,
    event_name: &str,
) -> bool {
    if is_known_dom_event_name(event_name) {
        return false;
    }
    matches!(
        event_host_tag(template_source, template_ast, directive_start),
        Some("component")
    )
}

pub(super) fn vnode_hook_signature(event_name: &str) -> Option<(&'static str, &'static str)> {
    if !is_vnode_hook(event_name) {
        return None;
    }
    Some(("import('vue').VNode", "[vnode: import('vue').VNode]"))
}

fn is_transition_hook(event_name: &str) -> bool {
    matches!(
        event_name,
        "before-enter"
            | "beforeEnter"
            | "enter"
            | "after-enter"
            | "afterEnter"
            | "enter-cancelled"
            | "enterCancelled"
            | "before-leave"
            | "beforeLeave"
            | "leave"
            | "after-leave"
            | "afterLeave"
            | "leave-cancelled"
            | "leaveCancelled"
            | "before-appear"
            | "beforeAppear"
            | "appear"
            | "after-appear"
            | "afterAppear"
            | "appear-cancelled"
            | "appearCancelled"
    )
}

fn is_vnode_hook(event_name: &str) -> bool {
    let rest = event_name
        .strip_prefix("vue:")
        .or_else(|| event_name.strip_prefix("vnode-"))
        .or_else(|| event_name.strip_prefix("vnode"));
    let Some(rest) = rest else {
        return false;
    };
    let normalized = rest
        .chars()
        .filter(|ch| *ch != '-')
        .flat_map(char::to_lowercase)
        .collect::<String>();
    matches!(
        normalized.as_str(),
        "beforemount" | "mounted" | "beforeupdate" | "updated" | "beforeunmount" | "unmounted"
    )
}

fn event_belongs_to_transition(
    template_source: Option<&str>,
    template_ast: Option<&RootNode<'_>>,
    directive_start: u32,
) -> bool {
    matches!(
        event_host_tag(template_source, template_ast, directive_start),
        Some("Transition" | "TransitionGroup" | "transition" | "transition-group")
    )
}

fn event_host_tag<'a>(
    template_source: Option<&'a str>,
    template_ast: Option<&'a RootNode<'_>>,
    directive_start: u32,
) -> Option<&'a str> {
    if let Some(tag) = template_ast.and_then(|root| event_host_tag_from_ast(root, directive_start))
    {
        return Some(tag);
    }
    let source = template_source?;
    event_host_tag_from_source(source, directive_start)
}

fn event_host_tag_from_ast<'root, 'arena>(
    root: &'root RootNode<'arena>,
    directive_start: u32,
) -> Option<&'root str> {
    root.children
        .iter()
        .find_map(|child| child_event_host_tag(child, directive_start))
}

fn child_event_host_tag<'root, 'arena>(
    child: &'root TemplateChildNode<'arena>,
    directive_start: u32,
) -> Option<&'root str> {
    match child {
        TemplateChildNode::Element(element) => element_event_host_tag(element, directive_start),
        TemplateChildNode::If(node) => node.branches.iter().find_map(|branch| {
            branch
                .children
                .iter()
                .find_map(|child| child_event_host_tag(child, directive_start))
        }),
        TemplateChildNode::IfBranch(branch) => branch
            .children
            .iter()
            .find_map(|child| child_event_host_tag(child, directive_start)),
        TemplateChildNode::For(node) => node
            .children
            .iter()
            .find_map(|child| child_event_host_tag(child, directive_start)),
        _ => None,
    }
}

fn element_event_host_tag<'root, 'arena>(
    element: &'root ElementNode<'arena>,
    directive_start: u32,
) -> Option<&'root str> {
    if element.props.iter().any(|prop| {
        matches!(
            prop,
            PropNode::Directive(directive) if directive.loc.span.start == directive_start
        )
    }) {
        return Some(element.tag);
    }
    element
        .children
        .iter()
        .find_map(|child| child_event_host_tag(child, directive_start))
}

fn event_host_tag_from_source(source: &str, directive_start: u32) -> Option<&str> {
    let prefix = source.get(..directive_start as usize)?;
    let open = host_tag_open_offset(prefix)?;
    if prefix[open..].starts_with("</") {
        return None;
    }
    prefix[open + 1..]
        .trim_start()
        .split(|ch: char| ch.is_ascii_whitespace() || ch == '/' || ch == '>')
        .next()
}

/// Byte offset of the `<` opening the tag the directive is written in, or
/// `None` when the offset is not inside a tag. Attribute values may contain
/// `<` (`:data="a < b"`), so the scan runs forward and tracks quoting instead
/// of taking the last `<`, which would read the comparison as a tag open.
fn host_tag_open_offset(prefix: &str) -> Option<usize> {
    let mut open = None;
    let mut quote = None;
    for (index, byte) in prefix.bytes().enumerate() {
        match (quote, byte) {
            (Some(open_quote), byte) if byte == open_quote => quote = None,
            (Some(_), _) => {}
            (None, b'"' | b'\'') if open.is_some() => quote = Some(byte),
            (None, b'<') => open = Some(index),
            (None, b'>') => open = None,
            (None, _) => {}
        }
    }
    open
}

#[cfg(test)]
mod tests {
    use super::{
        dynamic_component_custom_event, event_belongs_to_transition, event_host_tag,
        vnode_hook_signature,
    };
    use vize_carton::Allocator;

    fn directive_start(source: &str, directive: &str) -> u32 {
        source.find(directive).expect("directive in source") as u32
    }

    fn parse<'a>(allocator: &'a Allocator, source: &'a str) -> vize_relief::RootNode<'a> {
        let (root, errors) = vize_armature::parse(allocator, source);
        assert!(errors.is_empty(), "{errors:?}");
        root
    }

    #[test]
    fn host_tag_ignores_less_than_inside_an_attribute_value() {
        let source = "<Transition :data=\"a < b\" @enter=\"onEnter\" />";
        let allocator = Allocator::default();
        let root = parse(&allocator, source);
        let start = directive_start(source, "@enter");
        assert_eq!(
            event_host_tag(Some(source), Some(&root), start),
            Some("Transition")
        );
        assert!(event_belongs_to_transition(
            Some(source),
            Some(&root),
            start
        ));
    }

    #[test]
    fn host_tag_reports_a_plain_element_host() {
        let source = "<div @enter=\"onEnter\"></div>";
        let allocator = Allocator::default();
        let root = parse(&allocator, source);
        let start = directive_start(source, "@enter");
        assert_eq!(
            event_host_tag(Some(source), Some(&root), start),
            Some("div")
        );
        assert!(!event_belongs_to_transition(
            Some(source),
            Some(&root),
            start
        ));
    }

    #[test]
    fn host_tag_survives_a_preceding_dynamic_is_binding() {
        let source = "<component :is=\"Widget\" @picked=\"onPicked\"></component>";
        let allocator = Allocator::default();
        let root = parse(&allocator, source);
        let start = directive_start(source, "@picked");
        assert_eq!(
            event_host_tag(Some(source), Some(&root), start),
            Some("component")
        );
        assert!(dynamic_component_custom_event(
            Some(source),
            Some(&root),
            start,
            "picked"
        ));
    }

    #[test]
    fn vnode_hooks_use_vue_vnode_payloads() {
        assert_eq!(
            vnode_hook_signature("vue:mounted"),
            Some(("import('vue').VNode", "[vnode: import('vue').VNode]"))
        );
        assert_eq!(
            vnode_hook_signature("vue:before-unmount"),
            Some(("import('vue').VNode", "[vnode: import('vue').VNode]"))
        );
        assert_eq!(vnode_hook_signature("click"), None);
    }
}
