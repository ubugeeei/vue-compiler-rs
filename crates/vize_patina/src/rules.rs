//! Lint rules for Vue.js SFC files.

pub mod a11y;
pub mod css;
pub mod ecosystem;
pub mod html;
pub mod musea;
pub mod opinionated;
pub mod petite_vue;
pub mod script;
pub mod ssr;
pub mod type_aware;
pub(crate) mod url;
pub mod vapor;
pub mod vue;

pub use opinionated::vue::{
    ComponentCasing, HtmlSelfClosingHtmlOptions, HtmlSelfClosingOptions, HtmlSelfClosingStyle,
};

/// Register every rule that belongs to the Nuxt preset but not to the broader
/// presets it builds on.
pub(crate) fn register_nuxt(registry: &mut crate::rule::RuleRegistry) {
    opinionated::register_nuxt(registry);
    ecosystem::register_nuxt(registry);
}
