//! Typed per-rule lint options.
//!
//! A handful of lint rules accept project-local configuration so teams can
//! enforce their own architecture and design-system conventions through
//! `vize lint` instead of running a sidecar tool. Options live under
//! `linter.ruleOptions.<rule-name>` and are parsed into typed structs (no
//! untyped `serde_json::Value`) so the schema is discoverable and validation
//! stays strict.

use serde::{Deserialize, Serialize};

use crate::String;

mod casing;
mod html_self_closing;
mod hyphenation;
mod no_mutating_props;
mod sfc_element_order;

pub use casing::{
    ComponentNameInTemplateCasingOptions, CustomEventNameCasing, CustomEventNameCasingOptions,
    TemplateComponentNameCasing,
};
pub use html_self_closing::{
    HtmlSelfClosingHtmlOptions, HtmlSelfClosingOptions, HtmlSelfClosingStyle,
};
pub use hyphenation::HyphenationStyle;
pub use no_mutating_props::NoMutatingPropsOptions;
#[allow(unused_imports)]
pub use sfc_element_order::{SfcElementOrderGroup, SfcElementOrderOptions};

/// Per-rule configuration keyed by rule name.
///
/// Only the rules that actually accept options have typed fields; everything
/// else is ignored. The map is intentionally typed (rather than a free-form
/// `Value` bag) so unknown keys are rejected and the JSON schema is precise.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct LintRuleOptions {
    /// Options for `script/no-restricted-globals`.
    #[serde(rename = "script/no-restricted-globals")]
    pub no_restricted_globals: Option<NoRestrictedGlobalsOptions>,
    /// Options for `script/no-restricted-members`.
    #[serde(rename = "script/no-restricted-members")]
    pub no_restricted_members: Option<NoRestrictedMembersOptions>,
}

impl LintRuleOptions {
    /// Whether no rule options are configured.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.no_restricted_globals.is_none() && self.no_restricted_members.is_none()
    }

    /// Configured deny list for `script/no-restricted-globals` as
    /// `(name, optional message)` pairs. Empty when unconfigured.
    pub fn restricted_globals(&self) -> Vec<(String, Option<String>)> {
        self.no_restricted_globals
            .as_ref()
            .map(|options| {
                options
                    .globals
                    .iter()
                    .map(|global| (global.name.clone(), global.message.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Configured deny list for `script/no-restricted-members` as
    /// `(object, property, optional message)` tuples. Empty when unconfigured.
    pub fn restricted_members(&self) -> Vec<(String, String, Option<String>)> {
        self.no_restricted_members
            .as_ref()
            .map(|options| {
                options
                    .members
                    .iter()
                    .map(|member| {
                        (
                            member.object.clone(),
                            member.property.clone(),
                            member.message.clone(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Apply a later config layer to this option set.
    pub fn merge_from(&mut self, overlay: &Self) {
        if let Some(options) = &overlay.no_restricted_globals {
            self.no_restricted_globals = Some(options.clone());
        }
        if let Some(options) = &overlay.no_restricted_members {
            self.no_restricted_members = Some(options.clone());
        }
    }
}

/// Full typed lint rule options parsed from config.
///
/// This wraps the stable public [`LintRuleOptions`] shape with new config-only
/// options, so existing Rust consumers can still construct `LintRuleOptions`
/// using the previous fields while the CLI and LSP can read newer rule options.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct ConfigLintRuleOptions {
    #[serde(flatten)]
    stable: LintRuleOptions,
    /// Options for `vue/component-name-in-template-casing`.
    #[serde(rename = "vue/component-name-in-template-casing")]
    component_name_in_template_casing: Option<ComponentNameInTemplateCasingOptions>,
    /// Options for `script/custom-event-name-casing`.
    #[serde(rename = "script/custom-event-name-casing")]
    custom_event_name_casing: Option<CustomEventNameCasingOptions>,
    /// Options for `vue/no-mutating-props`.
    #[serde(rename = "vue/no-mutating-props")]
    no_mutating_props: Option<NoMutatingPropsOptions>,
    /// Options for `vue/sfc-element-order`.
    #[serde(rename = "vue/sfc-element-order")]
    sfc_element_order: Option<SfcElementOrderOptions>,
    /// Options for `vue/html-self-closing`.
    #[serde(rename = "vue/html-self-closing")]
    html_self_closing: Option<HtmlSelfClosingOptions>,
    /// Options for `vue/v-on-event-hyphenation`.
    #[serde(rename = "vue/v-on-event-hyphenation")]
    v_on_event_hyphenation: Option<HyphenationStyle>,
    /// Options for `vue/attribute-hyphenation`.
    #[serde(rename = "vue/attribute-hyphenation")]
    attribute_hyphenation: Option<HyphenationStyle>,
    /// Options for `musea/prefer-design-tokens`.
    #[serde(rename = "musea/prefer-design-tokens")]
    musea_prefer_design_tokens: Option<MuseaPreferDesignTokensOptions>,
}

impl ConfigLintRuleOptions {
    /// Build full config options from the stable public subset.
    #[inline]
    pub fn from_stable_options(stable: LintRuleOptions) -> Self {
        Self {
            stable,
            ..Self::default()
        }
    }

    /// Stable subset exposed by the original `load_linter_rule_options` API.
    #[inline]
    pub fn stable_options(&self) -> &LintRuleOptions {
        &self.stable
    }

    /// Whether no rule options are configured.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.stable.is_empty()
            && self.component_name_in_template_casing.is_none()
            && self.custom_event_name_casing.is_none()
            && self.no_mutating_props.is_none()
            && self.sfc_element_order.is_none()
            && self.html_self_closing.is_none()
            && self.v_on_event_hyphenation.is_none()
            && self.attribute_hyphenation.is_none()
            && self.musea_prefer_design_tokens.is_none()
    }

    /// Configured deny list for `script/no-restricted-globals`.
    #[inline]
    pub fn restricted_globals(&self) -> Vec<(String, Option<String>)> {
        self.stable.restricted_globals()
    }

    /// Configured deny list for `script/no-restricted-members`.
    #[inline]
    pub fn restricted_members(&self) -> Vec<(String, String, Option<String>)> {
        self.stable.restricted_members()
    }

    /// Configured casing for `vue/component-name-in-template-casing`.
    #[inline]
    pub fn component_name_in_template_casing(&self) -> Option<TemplateComponentNameCasing> {
        self.component_name_in_template_casing
            .as_ref()
            .map(|options| options.casing)
    }

    /// Configured casing for `script/custom-event-name-casing`.
    #[inline]
    pub fn custom_event_name_casing(&self) -> Option<CustomEventNameCasing> {
        self.custom_event_name_casing
            .as_ref()
            .map(|options| options.casing)
    }

    /// Configured options for `vue/no-mutating-props`.
    #[inline]
    pub fn no_mutating_props(&self) -> Option<NoMutatingPropsOptions> {
        self.no_mutating_props
    }

    /// Configured block order for `vue/sfc-element-order`.
    #[inline]
    pub fn sfc_element_order(&self) -> Option<SfcElementOrderOptions> {
        self.sfc_element_order.clone()
    }

    /// Configured self-closing style for `vue/html-self-closing`.
    #[inline]
    pub fn html_self_closing(&self) -> Option<HtmlSelfClosingOptions> {
        self.html_self_closing
    }

    /// Configured style for `vue/v-on-event-hyphenation`.
    #[inline]
    pub fn v_on_event_hyphenation(&self) -> Option<HyphenationStyle> {
        self.v_on_event_hyphenation
    }

    /// Configured style for `vue/attribute-hyphenation`.
    #[inline]
    pub fn attribute_hyphenation(&self) -> Option<HyphenationStyle> {
        self.attribute_hyphenation
    }

    /// Configured design-token primitive values for
    /// `musea/prefer-design-tokens` as `(value, path, tier)` tuples. Empty
    /// when unconfigured.
    pub fn musea_design_tokens(&self) -> Vec<(String, String, String)> {
        self.musea_prefer_design_tokens
            .as_ref()
            .map(|options| {
                options
                    .tokens
                    .iter()
                    .map(|token| (token.value.clone(), token.path.clone(), token.tier.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Apply a later config layer to this option set.
    pub fn merge_from(&mut self, overlay: &Self) {
        self.stable.merge_from(&overlay.stable);
        if let Some(options) = &overlay.component_name_in_template_casing {
            self.component_name_in_template_casing = Some(*options);
        }
        if let Some(options) = &overlay.custom_event_name_casing {
            self.custom_event_name_casing = Some(*options);
        }
        if let Some(options) = &overlay.no_mutating_props {
            self.no_mutating_props = Some(*options);
        }
        if let Some(options) = &overlay.sfc_element_order {
            self.sfc_element_order = Some(options.clone());
        }
        if let Some(options) = &overlay.html_self_closing {
            self.html_self_closing = Some(*options);
        }
        if let Some(style) = overlay.v_on_event_hyphenation {
            self.v_on_event_hyphenation = Some(style);
        }
        if let Some(style) = overlay.attribute_hyphenation {
            self.attribute_hyphenation = Some(style);
        }
        if let Some(options) = &overlay.musea_prefer_design_tokens {
            self.musea_prefer_design_tokens = Some(options.clone());
        }
    }
}

/// Options for `script/no-restricted-globals`.
///
/// When `globals` is non-empty it **replaces** the rule's built-in deny list;
/// otherwise the built-in defaults (`process`, `localStorage`, `sessionStorage`)
/// apply.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct NoRestrictedGlobalsOptions {
    /// Restricted global identifier references.
    pub globals: Vec<RestrictedGlobal>,
}

/// A single restricted global entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RestrictedGlobal {
    /// Identifier name to forbid (e.g. `process`).
    pub name: String,
    /// Optional advisory message shown in the diagnostic.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Options for `script/no-restricted-members`.
///
/// The rule is off unless `members` is configured; there is no built-in default
/// list. This is the project-local-rule mechanism: each entry flags an
/// `<object>.<property>` member access.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct NoRestrictedMembersOptions {
    /// Restricted `<object>.<property>` member accesses.
    pub members: Vec<RestrictedMember>,
}

/// A single restricted member-access entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RestrictedMember {
    /// Object identifier (e.g. `window`).
    pub object: String,
    /// Property name accessed on the object (e.g. `localStorage`).
    pub property: String,
    /// Optional advisory message shown in the diagnostic.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Options for `musea/prefer-design-tokens`.
///
/// The rule is off unless tokens are configured and the rule is enabled by
/// severity or implicitly by this non-empty token list. Each token maps one
/// hardcoded CSS value to a design-token path.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct MuseaPreferDesignTokensOptions {
    /// Design tokens that should replace matching hardcoded CSS values.
    pub tokens: Vec<MuseaDesignToken>,
}

/// A design token recognized by `musea/prefer-design-tokens`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MuseaDesignToken {
    /// Token path (e.g. `color.primary`).
    pub path: String,
    /// Primitive CSS value to match (e.g. `#3b82f6`).
    pub value: String,
    /// Token tier shown in diagnostics. Defaults to `primitive`.
    #[serde(default = "default_musea_design_token_tier")]
    pub tier: String,
}

fn default_musea_design_token_tier() -> String {
    "primitive".into()
}

#[cfg(test)]
mod tests;
