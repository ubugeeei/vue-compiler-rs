//! vue/attribute-hyphenation
//!
//! Enforce attribute naming style on custom components.
//!
//! ## Examples
//!
//! ### Invalid (default: always)
//! ```vue
//! <MyComponent myProp="value" />
//! <MyComponent :myProp="value" />
//! ```
//!
//! ### Valid
//! ```vue
//! <MyComponent my-prop="value" />
//! <MyComponent :my-prop="value" />
//! ```

use crate::context::LintContext;
use crate::diagnostic::Severity;
use crate::rule::{Rule, RuleCategory, RuleMeta};
use vize_relief::{DirectiveNode, ElementNode, ExpressionNode, PropNode};
use vize_s0::{String, ToCompactString, is_native_tag};

#[cfg(test)]
mod tests;

static META: RuleMeta = RuleMeta {
    name: "vue/attribute-hyphenation",
    description: "Enforce attribute naming style on custom components",
    category: RuleCategory::StronglyRecommended,
    fixable: true,
    default_severity: Severity::Warning,
};

const SVG_ATTRIBUTES_WEIRD_CASE: &[&str] = &[
    "attributeName",
    "attributeType",
    "baseFrequency",
    "baseProfile",
    "calcMode",
    "clipPathUnits",
    "contentScriptType",
    "contentStyleType",
    "diffuseConstant",
    "edgeMode",
    "externalResourcesRequired",
    "filterRes",
    "filterUnits",
    "glyphRef",
    "gradientTransform",
    "gradientUnits",
    "kernelMatrix",
    "kernelUnitLength",
    "keyPoints",
    "keySplines",
    "keyTimes",
    "lengthAdjust",
    "limitingConeAngle",
    "markerHeight",
    "markerUnits",
    "markerWidth",
    "maskContentUnits",
    "maskUnits",
    "numOctaves",
    "pathLength",
    "patternContentUnits",
    "patternTransform",
    "patternUnits",
    "pointsAtX",
    "pointsAtY",
    "pointsAtZ",
    "preserveAlpha",
    "preserveAspectRatio",
    "primitiveUnits",
    "referrerPolicy",
    "refX",
    "refY",
    "repeatCount",
    "repeatDur",
    "requiredExtensions",
    "requiredFeatures",
    "specularConstant",
    "specularExponent",
    "spreadMethod",
    "startOffset",
    "stdDeviation",
    "stitchTiles",
    "surfaceScale",
    "systemLanguage",
    "tableValues",
    "targetX",
    "targetY",
    "textLength",
    "viewBox",
    "viewTarget",
    "xChannelSelector",
    "yChannelSelector",
    "zoomAndPan",
];

/// Attribute hyphenation style
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HyphenationStyle {
    /// Require hyphenated attribute names: my-prop
    #[default]
    Always,
    /// Allow camelCase: myProp
    Never,
}

/// Attribute hyphenation rule
pub struct AttributeHyphenation {
    pub style: HyphenationStyle,
    /// Attributes to ignore
    pub ignore: Vec<String>,
}

impl Default for AttributeHyphenation {
    fn default() -> Self {
        Self {
            style: HyphenationStyle::Always,
            ignore: vec![
                // Common data attributes
                "data-".to_compact_string(),
                "aria-".to_compact_string(),
                // Vue specific
                "slot-scope".to_compact_string(),
            ],
        }
    }
}

impl AttributeHyphenation {
    pub fn new(style: HyphenationStyle) -> Self {
        Self {
            style,
            ..Self::default()
        }
    }

    fn is_custom_component(element: &ElementNode<'_>) -> bool {
        let tag = element.tag;
        // Custom components are either:
        // 1. PascalCase (starts with uppercase)
        // 2. Contains hyphen (kebab-case component)
        // 3. Not a known native HTML/SVG/MathML element
        // 4. A native element with an `is` customization
        if tag.chars().next().is_some_and(|c| c.is_uppercase()) {
            return true;
        }
        if tag.contains('-') {
            return true;
        }
        if !is_native_tag(tag) {
            return true;
        }
        element.props.iter().any(Self::is_customized_builtin)
    }

    fn should_ignore(&self, name: &str) -> bool {
        if SVG_ATTRIBUTES_WEIRD_CASE
            .iter()
            .any(|attr| name.contains(attr))
        {
            return true;
        }

        self.ignore
            .iter()
            .any(|pattern| name.contains(pattern.as_str()))
    }

    fn static_directive_arg<'a>(dir: &DirectiveNode<'a>) -> Option<&'a str> {
        if !matches!(dir.name, "bind" | "model") {
            return None;
        }

        match dir.arg.as_ref()? {
            ExpressionNode::Simple(s) if s.is_static => Some(s.content),
            _ => None,
        }
    }

    fn requires_hyphenation(name: &str) -> bool {
        name.chars().any(char::is_uppercase)
    }

    fn forbids_hyphenation(name: &str) -> bool {
        name.contains('-')
    }

    fn is_customized_builtin(prop: &PropNode<'_>) -> bool {
        match prop {
            PropNode::Attribute(attr) => attr.name == "is",
            PropNode::Directive(dir) if dir.name == "is" => true,
            PropNode::Directive(dir) if dir.name == "bind" => {
                Self::static_directive_arg(dir) == Some("is")
            }
            PropNode::Directive(_) => false,
        }
    }
}

impl Rule for AttributeHyphenation {
    fn meta(&self) -> &'static RuleMeta {
        &META
    }

    fn enter_element<'a>(&self, ctx: &mut LintContext<'a>, element: &ElementNode<'a>) {
        // Only check custom components
        if !Self::is_custom_component(element) {
            return;
        }

        for prop in &element.props {
            let (name, loc) = match prop {
                PropNode::Attribute(attr) => (attr.name, &attr.loc),
                PropNode::Directive(dir) => {
                    let Some(arg) = Self::static_directive_arg(dir) else {
                        continue;
                    };
                    (arg, &dir.loc)
                }
            };

            // Skip ignored attributes
            if self.should_ignore(name) {
                continue;
            }

            // Skip directive shorthand attributes when they were parsed as plain attributes.
            if name.starts_with("v-") || name.starts_with('@') || name.starts_with('#') {
                continue;
            }

            match self.style {
                HyphenationStyle::Always => {
                    if Self::requires_hyphenation(name) {
                        ctx.warn_with_help(
                            ctx.t("vue/attribute-hyphenation.message"),
                            loc,
                            ctx.t("vue/attribute-hyphenation.help"),
                        );
                    }
                }
                HyphenationStyle::Never => {
                    if Self::forbids_hyphenation(name) {
                        ctx.warn_with_help(
                            ctx.t("vue/attribute-hyphenation.message_never"),
                            loc,
                            ctx.t("vue/attribute-hyphenation.help_never"),
                        );
                    }
                }
            }
        }
    }
}
