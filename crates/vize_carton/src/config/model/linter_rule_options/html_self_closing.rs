use serde::{Deserialize, Serialize};

/// Options for `vue/html-self-closing`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct HtmlSelfClosingOptions {
    pub html: HtmlSelfClosingHtmlOptions,
    #[serde(default = "default_always")]
    pub svg: HtmlSelfClosingStyle,
    #[serde(default = "default_always")]
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

/// HTML-family options for `vue/html-self-closing`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct HtmlSelfClosingHtmlOptions {
    #[serde(rename = "void")]
    #[serde(default = "default_always")]
    pub void_elements: HtmlSelfClosingStyle,
    #[serde(default = "default_any")]
    pub normal: HtmlSelfClosingStyle,
    #[serde(default = "default_always")]
    pub component: HtmlSelfClosingStyle,
}

impl Default for HtmlSelfClosingHtmlOptions {
    fn default() -> Self {
        Self {
            void_elements: HtmlSelfClosingStyle::Always,
            normal: HtmlSelfClosingStyle::Any,
            component: HtmlSelfClosingStyle::Always,
        }
    }
}

/// Self-closing policy value accepted by `vue/html-self-closing`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HtmlSelfClosingStyle {
    Always,
    Never,
    Any,
}

fn default_always() -> HtmlSelfClosingStyle {
    HtmlSelfClosingStyle::Always
}

fn default_any() -> HtmlSelfClosingStyle {
    HtmlSelfClosingStyle::Any
}
