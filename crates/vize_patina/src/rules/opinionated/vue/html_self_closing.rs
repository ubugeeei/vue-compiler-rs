//! vue/html-self-closing
//!
//! Enforce self-closing style for HTML elements.
//!
//! ## Examples
//!
//! ### Invalid (default config)
//! ```vue
//! <div></div>  <!-- should be <div /> when empty -->
//! <img>        <!-- should be <img /> -->
//! <br>         <!-- should be <br /> -->
//! ```
//!
//! ### Valid
//! ```vue
//! <div />
//! <img />
//! <br />
//! <div>content</div>
//! ```

use crate::context::LintContext;
use crate::diagnostic::Severity;
use crate::rule::{Rule, RuleCategory, RuleMeta};
use vize_relief::{ElementNode, ElementType, Namespace};
use vize_s0::{is_html_tag, is_math_ml_tag, is_svg_tag, is_void_tag};

static META: RuleMeta = RuleMeta {
    name: "vue/html-self-closing",
    description: "Enforce self-closing style",
    category: RuleCategory::StronglyRecommended,
    fixable: true,
    default_severity: Severity::Warning,
};

/// Per-element self-closing policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HtmlSelfClosingStyle {
    /// Empty elements must use self-closing syntax.
    Always,
    /// Elements must use separate start and end tags.
    Never,
    /// Both forms are accepted.
    Any,
}

/// Self-closing policy for HTML element families.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HtmlSelfClosingHtmlOptions {
    pub void: HtmlSelfClosingStyle,
    pub normal: HtmlSelfClosingStyle,
    pub component: HtmlSelfClosingStyle,
}

impl Default for HtmlSelfClosingHtmlOptions {
    fn default() -> Self {
        Self {
            void: HtmlSelfClosingStyle::Always,
            normal: HtmlSelfClosingStyle::Any,
            component: HtmlSelfClosingStyle::Always,
        }
    }
}

/// Config for `vue/html-self-closing`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HtmlSelfClosingOptions {
    pub html: HtmlSelfClosingHtmlOptions,
    pub svg: HtmlSelfClosingStyle,
    pub math: HtmlSelfClosingStyle,
}

impl Default for HtmlSelfClosingOptions {
    fn default() -> Self {
        Self {
            html: HtmlSelfClosingHtmlOptions::default(),
            svg: HtmlSelfClosingStyle::Always,
            math: HtmlSelfClosingStyle::Always,
        }
    }
}

/// HTML self-closing style rule
#[derive(Default)]
pub struct HtmlSelfClosing {
    options: HtmlSelfClosingOptions,
}

impl HtmlSelfClosing {
    pub const fn new(options: HtmlSelfClosingOptions) -> Self {
        Self { options }
    }
}

impl Rule for HtmlSelfClosing {
    fn meta(&self) -> &'static RuleMeta {
        &META
    }

    fn enter_element<'a>(&self, ctx: &mut LintContext<'a>, element: &ElementNode<'a>) {
        check_element(ctx, element, self.options, false);
    }
}

/// Nuxt-preset variant of [`HtmlSelfClosing`] that also exempts
/// framework-registered Vuetify 2 components (`v-*` tags) from
/// self-closing diagnostics.
///
/// Vuetify components are globally registered so the linter cannot determine
/// their preferred closing style from source. Enabling the exemption in the
/// Nuxt preset keeps real Nuxt + Vuetify projects out of a self-closing
/// warning storm without loosening the rule for other presets.
pub(crate) struct HtmlSelfClosingNuxt {
    options: HtmlSelfClosingOptions,
}

impl HtmlSelfClosingNuxt {
    pub(crate) const fn new(options: HtmlSelfClosingOptions) -> Self {
        Self { options }
    }
}

impl Default for HtmlSelfClosingNuxt {
    fn default() -> Self {
        Self::new(HtmlSelfClosingOptions::default())
    }
}

impl Rule for HtmlSelfClosingNuxt {
    fn meta(&self) -> &'static RuleMeta {
        &META
    }

    fn enter_element<'a>(&self, ctx: &mut LintContext<'a>, element: &ElementNode<'a>) {
        check_element(ctx, element, self.options, true);
    }
}

fn check_element<'a>(
    ctx: &mut LintContext<'a>,
    element: &ElementNode<'a>,
    options: HtmlSelfClosingOptions,
    allow_vuetify_tags: bool,
) {
    let tag = element.tag;
    if allow_vuetify_tags && is_vuetify_tag(tag) {
        return;
    }

    let (style, message) = match classify_element(element) {
        SelfClosingElementKind::VoidHtml => {
            (options.html.void, ctx.t("vue/html-self-closing.void"))
        }
        SelfClosingElementKind::NormalHtml => {
            (options.html.normal, ctx.t("vue/html-self-closing.empty"))
        }
        SelfClosingElementKind::Component => (
            options.html.component,
            ctx.t("vue/html-self-closing.component"),
        ),
        SelfClosingElementKind::Svg => (options.svg, ctx.t("vue/html-self-closing.empty")),
        SelfClosingElementKind::Math => (options.math, ctx.t("vue/html-self-closing.empty")),
        SelfClosingElementKind::Other => return,
    };
    if matches!(style, HtmlSelfClosingStyle::Any) {
        return;
    }

    let has_children = !element.children.is_empty();
    let is_self_closing = element.is_self_closing
        || (matches!(style, HtmlSelfClosingStyle::Never) && authored_self_closing(ctx, element));

    match style {
        HtmlSelfClosingStyle::Always if !has_children && !is_self_closing => {
            ctx.warn_with_help(message, &element.loc, ctx.t("vue/html-self-closing.help"));
        }
        HtmlSelfClosingStyle::Never if is_self_closing => {
            ctx.warn_with_help(
                ctx.t("vue/html-self-closing.never"),
                &element.loc,
                ctx.t("vue/html-self-closing.never_help"),
            );
        }
        _ => {}
    }
}

#[derive(Clone, Copy)]
enum SelfClosingElementKind {
    VoidHtml,
    NormalHtml,
    Component,
    Svg,
    Math,
    Other,
}

fn classify_element(element: &ElementNode<'_>) -> SelfClosingElementKind {
    let tag = element.tag;
    match element.ns {
        Namespace::Svg => return SelfClosingElementKind::Svg,
        Namespace::MathMl => return SelfClosingElementKind::Math,
        Namespace::Html => {}
    }

    if is_void_tag(tag) {
        return SelfClosingElementKind::VoidHtml;
    }

    if is_svg_tag(tag) {
        return SelfClosingElementKind::Svg;
    }

    if is_math_ml_tag(tag) {
        return SelfClosingElementKind::Math;
    }

    if (element.tag_type == ElementType::Component || is_component_like_tag(tag))
        && !is_nuxt_builtin_component(tag)
    {
        return SelfClosingElementKind::Component;
    }

    if is_html_element_name(tag) {
        return SelfClosingElementKind::NormalHtml;
    }

    SelfClosingElementKind::Other
}

fn is_html_element_name(tag: &str) -> bool {
    if tag.bytes().all(|b| !b.is_ascii_uppercase()) {
        return is_html_tag(tag);
    }
    is_html_tag(&tag.to_lowercase())
}

fn is_component_like_tag(tag: &str) -> bool {
    tag.contains('-') || tag.chars().next().is_some_and(char::is_uppercase)
}

fn authored_self_closing(ctx: &LintContext<'_>, element: &ElementNode<'_>) -> bool {
    let start = element.loc.span.start as usize;
    let end = element.loc.span.end as usize;
    let Some(open_tag) = ctx.source.get(start..end) else {
        return false;
    };
    let mut saw_close = false;
    for byte in open_tag.bytes().rev() {
        match byte {
            b'>' if !saw_close => saw_close = true,
            b'/' if saw_close => return true,
            b' ' | b'\t' | b'\n' | b'\r' | 0x0c if saw_close => {}
            _ if saw_close => return false,
            _ => {}
        }
    }
    false
}

fn is_nuxt_builtin_component(tag: &str) -> bool {
    matches!(
        tag,
        "nuxt"
            | "nuxt-child"
            | "nuxt-page"
            | "nuxt-layout"
            | "nuxt-link"
            | "nuxt-loading-indicator"
            | "nuxt-error-boundary"
            | "client-only"
            | "no-ssr"
            | "Nuxt"
            | "NuxtChild"
            | "NuxtPage"
            | "NuxtLayout"
            | "NuxtLink"
            | "NuxtLoadingIndicator"
            | "NuxtErrorBoundary"
            | "ClientOnly"
            | "NoSsr"
    )
}

/// Matches the Vuetify `v-*` tag convention (e.g. `v-btn`, `v-dialog`).
fn is_vuetify_tag(tag: &str) -> bool {
    let bytes = tag.as_bytes();
    bytes.len() >= 3 && bytes[0] == b'v' && bytes[1] == b'-' && bytes[2].is_ascii_lowercase()
}

#[cfg(test)]
mod tests;
